/**
 * Deadlock and presence, in the gateway Durable Object:
 *
 *   resolveLiveState     checks a client's structured live state against
 *                        the user's linked Steam account and maps the party
 *                        to the Yap users it may name, so no account id is
 *                        stored or sent
 *   mutualParty          the party a viewer may see: only members whose own
 *                        verified live state names this user back
 *   noteDeadlockActivity moves the fetch cadence: every 10 minutes while
 *                        the game runs, checks at +3 and +15 minutes after a
 *                        match or the game ends
 *
 * Imports nothing that needs a request context: the Durable Object passes
 * its own bindings.
 */

import type { GameActivity } from "@ralph/protocol";

import { isDeadlockActivity } from "@/lib/deadlock";

import { DEADLOCK_CADENCE } from "./fetcher";
import { createDeadlockRefreshTask, queueDelaySeconds } from "./tasks";

const STEAM_ID64_BASE = 76561197960265728n;

export interface LiveEnv {
  DB: D1Database;
  BACKGROUND_TASKS?: {
    send(body: never, options?: QueueSendOptions): Promise<unknown>;
  };
}

function steamId64(accountId: number): string {
  return (BigInt(accountId) + STEAM_ID64_BASE).toString();
}

/**
 * The activity as the gateway keeps it. A Deadlock live state survives
 * only when its `account_id` is the user's linked Steam account (a smurf
 * or someone else's PC reports plain "Playing Deadlock"); `party` becomes
 * the Yap users who linked those Steam accounts and show them on their
 * profile, the user excluded. That list is only a claim: it is stored with
 * the activity but never sent as is (the gateway sends `mutualParty`).
 * `account_id` and `party` never leave this function.
 */
export async function resolveLiveState(
  db: D1Database,
  userId: string,
  activity: GameActivity | null,
): Promise<GameActivity | null> {
  if (!activity?.game) return activity;
  const { game, ...rest } = activity;
  if (game.kind !== "deadlock" || game.account_id === undefined) return rest;
  let linked: { external_id: string } | null = null;
  try {
    linked = await db
      .prepare(
        `SELECT external_id FROM user_connections WHERE user_id = ? AND provider = 'steam'`,
      )
      .bind(userId)
      .first<{ external_id: string }>();
  } catch {
    linked = null;
  }
  if (!linked || linked.external_id !== steamId64(game.account_id)) return rest;

  const partyIds = [...new Set(game.party ?? [])].filter(
    (id) => id !== game.account_id,
  );
  let partyUserIds: string[] = [];
  if (partyIds.length > 0) {
    try {
      const { results } = await db
        .prepare(
          `SELECT user_id, external_id FROM user_connections
           WHERE provider = 'steam' AND show_on_profile = 1
             AND external_id IN (${partyIds.map(() => "?").join(",")})`,
        )
        .bind(...partyIds.map(steamId64))
        .all<{ user_id: string; external_id: string }>();
      const byId = new Map(
        (results ?? []).map((row) => [row.external_id, row.user_id]),
      );
      partyUserIds = partyIds
        .map((id) => byId.get(steamId64(id)))
        .filter((id): id is string => !!id && id !== userId);
    } catch {
      partyUserIds = [];
    }
  }
  const live: {
    -readonly [K in keyof NonNullable<GameActivity["game"]>]: NonNullable<
      GameActivity["game"]
    >[K];
  } = {
    kind: "deadlock",
    phase: game.phase,
  };
  if (game.hero !== undefined) live.hero = game.hero;
  if (game.mode !== undefined) live.mode = game.mode;
  if (game.phase_started_at !== undefined)
    live.phase_started_at = game.phase_started_at;
  if (partyUserIds.length > 0) live.party_user_ids = partyUserIds;
  return { ...rest, game: live };
}

/**
 * The party others may see in `activity`: only the claimed members whose
 * own current, verified live state (`activityOf`) claims this user back.
 * A one-sided claim names nobody, so a client cannot make up a party or use
 * the gateway to learn which Yap user owns a Steam account.
 */
export function mutualParty(
  userId: string,
  activity: GameActivity | null,
  activityOf: (userId: string) => GameActivity | null,
): GameActivity | null {
  const claimed = activity?.game?.party_user_ids;
  if (!activity?.game || !claimed) return activity;
  const mutual = claimed.filter((memberId) => {
    const theirs = activityOf(memberId)?.game;
    return (
      theirs?.kind === "deadlock" &&
      (theirs.party_user_ids ?? []).includes(userId)
    );
  });
  const { party_user_ids: _claimed, ...game } = activity.game;
  void _claimed;
  return {
    ...activity,
    game: mutual.length > 0 ? { ...game, party_user_ids: mutual } : game,
  };
}

/** Post-game checks for one user's accounts: at most one pair per window. */
export const POST_GAME_DEDUPE_MS = 10 * 60_000;

/**
 * Claims the post-game checks for the user's tracked accounts: only rows
 * that had none queued in the last ten minutes, so a reconnect (the game
 * "ends" and starts again) or a flapping phase queues nothing more.
 */
async function claimPostGame(db: D1Database, userId: string, now: number) {
  const { results } = await db
    .prepare(
      `UPDATE deadlock_fetch_state SET post_game_at = ?
       WHERE user_id = ? AND (post_game_at IS NULL OR post_game_at <= ?)
       RETURNING account_id`,
    )
    .bind(now, userId, now - POST_GAME_DEDUPE_MS)
    .all<{ account_id: number }>();
  return (results ?? []).map((row) => row.account_id);
}

async function queuePostGame(env: LiveEnv, accountIds: number[]) {
  if (!env.BACKGROUND_TASKS || accountIds.length === 0) return;
  const envelope = createDeadlockRefreshTask(accountIds, "post_game");
  for (const delay of DEADLOCK_CADENCE.postGame) {
    await env.BACKGROUND_TASKS.send(envelope as never, {
      contentType: "json",
      delaySeconds: queueDelaySeconds(delay),
    });
  }
}

/**
 * A user's activity changed (`previous` → `next`; null is none). Only
 * Deadlock moves anything, and only for an account the fetcher tracks.
 */
export async function noteDeadlockActivity(
  env: LiveEnv,
  userId: string,
  previous: GameActivity | null,
  next: GameActivity | null,
  now: number = Date.now(),
): Promise<"started" | "ended" | "match_ended" | null> {
  const was = isDeadlockActivity(previous);
  const is = isDeadlockActivity(next);
  if (!was && !is) return null;
  const c = DEADLOCK_CADENCE;
  if (!was && is) {
    await env.DB.prepare(
      `UPDATE deadlock_fetch_state SET live_since = ?, live_until = NULL,
         next_due_at = MIN(next_due_at, ?) WHERE user_id = ?`,
    )
      .bind(now, now + c.playing, userId)
      .run();
    return "started";
  }
  if (was && !is) {
    await env.DB.prepare(
      `UPDATE deadlock_fetch_state SET live_until = ?,
         next_due_at = MIN(next_due_at, ?) WHERE user_id = ?`,
    )
      .bind(now, now + c.postGame[0]!, userId)
      .run();
    await queuePostGame(env, await claimPostGame(env.DB, userId, now));
    return "ended";
  }
  const left = previous?.game?.phase;
  const entered = next?.game?.phase;
  if (entered === "post_match" && left !== "post_match") {
    await queuePostGame(env, await claimPostGame(env.DB, userId, now));
    return "match_ended";
  }
  return null;
}
