/**
 * Blocks between two users (relationships type 1), Discord-style.
 *
 * A block is one-way in the table (`user_id` blocked `target_user_id`) and
 * two-way in effect for direct messages:
 *
 * - neither side can open a new DM with the other;
 * - in an existing 1:1 DM neither side can send messages, add reactions,
 *   ring or show typing. The blocked user is told "You can't message this
 *   user"; the blocker is told to unblock first (Discord also stops the
 *   blocker from messaging someone they blocked);
 * - neither side can join the DM's call room, and a block ends a ring or a
 *   call already running between them;
 * - a blocked user's friend requests are refused;
 * - a blocked user's mentions, replies and DMs never notify the blocker.
 *
 * Server channels are not refused: there the blocker's client collapses the
 * blocked user's messages instead.
 */

import { ServiceError } from "@/lib/service-error";
import type { D1Database } from "@cloudflare/workers-types";

/** `you_blocked`: the actor blocked the other user; `blocked_you`: the reverse. */
export type BlockState = "you_blocked" | "blocked_you";

/** The error codes the API returns for a refusal caused by a block. */
export const BLOCK_ERROR = {
  /** The actor blocked the other user. */
  youBlocked: "USER_BLOCKED",
  /** The other user blocked the actor. */
  blockedYou: "BLOCKED_BY_USER",
} as const;

export const BLOCK_MESSAGE_COPY = {
  you_blocked: "You blocked this user. Unblock to message.",
  blocked_you: "You can't message this user.",
} as const satisfies Record<BlockState, string>;

/** The refusal copy for joining a 1:1 DM's call room across a block. */
export const BLOCK_CALL_COPY = {
  you_blocked: "You blocked this user. Unblock to call.",
  blocked_you: "You can't call this user.",
} as const satisfies Record<BlockState, string>;

/**
 * Whether either user blocked the other. When both did, the actor's own
 * block wins (`you_blocked`), so they are asked to unblock first.
 */
export async function blockBetween(
  db: D1Database,
  actorId: string,
  otherId: string,
): Promise<BlockState | null> {
  if (!actorId || !otherId || actorId === otherId) return null;
  const { results } = await db
    .prepare(
      `SELECT user_id FROM relationships
        WHERE type = 1
          AND ((user_id = ? AND target_user_id = ?)
            OR (user_id = ? AND target_user_id = ?))`,
    )
    .bind(actorId, otherId, otherId, actorId)
    .all<{ user_id: string }>();
  const blockers = new Set((results ?? []).map((row) => row.user_id));
  if (blockers.has(actorId)) return "you_blocked";
  if (blockers.has(otherId)) return "blocked_you";
  return null;
}

/**
 * The block between `actorId` and the other recipient of a 1:1 DM channel,
 * or null when there is none (or the channel is not a 1:1 DM).
 */
export async function dmBlockState(
  db: D1Database,
  actorId: string,
  channelId: string,
): Promise<BlockState | null> {
  const { results } = await db
    .prepare(
      `SELECT r.user_id
         FROM dm_recipients other
         JOIN channels c ON c.id = other.channel_id AND c.channel_type = 'dm'
         JOIN relationships r
           ON r.type = 1
          AND ((r.user_id = ?1 AND r.target_user_id = other.user_id)
            OR (r.user_id = other.user_id AND r.target_user_id = ?1))
        WHERE other.channel_id = ?2 AND other.user_id != ?1`,
    )
    .bind(actorId, channelId)
    .all<{ user_id: string }>();
  const rows = results ?? [];
  if (rows.length === 0) return null;
  return rows.some((row) => row.user_id === actorId)
    ? "you_blocked"
    : "blocked_you";
}

/** The refusal for a block, as the API reports it (403 with a code). */
export function blockRefusal(
  state: BlockState,
  copy: Record<BlockState, string> = BLOCK_MESSAGE_COPY,
): ServiceError {
  return ServiceError.forbidden(
    copy[state],
    state === "you_blocked" ? BLOCK_ERROR.youBlocked : BLOCK_ERROR.blockedYou,
  );
}

/** Throws the block refusal when the actor may not message in this DM. */
export async function assertCanMessageInDm(
  db: D1Database,
  actorId: string,
  channelId: string,
): Promise<void> {
  const state = await dmBlockState(db, actorId, channelId);
  if (state) throw blockRefusal(state);
}

/**
 * The users who blocked `authorId`; the author's mentions, replies and DM
 * messages never notify them (inbox or push).
 */
export async function usersWhoBlocked(
  db: D1Database,
  authorId: string,
): Promise<Set<string>> {
  const { results } = await db
    .prepare(
      `SELECT user_id FROM relationships WHERE target_user_id = ? AND type = 1`,
    )
    .bind(authorId)
    .all<{ user_id: string }>();
  return new Set((results ?? []).map((row) => row.user_id));
}
