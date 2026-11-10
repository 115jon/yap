/**
 * What the board route sends for the Deadlock widget: the owner's state
 * and last good digest, cut to the owner's range for everyone else. Reads
 * only D1 (never the API): a plate that is older than 15 minutes asks for
 * one queued fetch (`refresh`) and still shows the copy it has.
 */

import {
  digestForRange,
  type DeadlockBoardData,
  type DeadlockDigest,
  type DeadlockRange,
  type DeadlockState,
} from "@/lib/deadlock";

import {
  DEADLOCK_CADENCE,
  DEADLOCK_STALE_AFTER,
  linkedAccountId,
  type FetchStateRow,
} from "./fetcher";

export interface DeadlockView {
  data: DeadlockBoardData;
  /** An account to fetch now (queued once per five minutes), or null. */
  refresh: number | null;
}

function parseDigest(raw: string | null): DeadlockDigest | null {
  if (!raw) return null;
  try {
    const digest = JSON.parse(raw) as DeadlockDigest;
    return digest && digest.v === 1 ? digest : null;
  } catch {
    return null;
  }
}

/**
 * The plate's data for one viewer. `track` (the owner has the widget on
 * the board) starts tracking an account the fetcher does not know yet.
 */
export async function deadlockBoardData(
  db: D1Database,
  input: {
    ownerId: string;
    isOwner: boolean;
    range: DeadlockRange;
    track: boolean;
    now: number;
  },
): Promise<DeadlockView> {
  const { now } = input;
  let link: Awaited<ReturnType<typeof linkedAccountId>> = null;
  try {
    link = await linkedAccountId(db, input.ownerId);
  } catch {
    link = null;
  }
  if (!link) {
    return {
      data: {
        state: "not_linked",
        digest: null,
        fetched_at: null,
        stale: false,
      },
      refresh: null,
    };
  }

  const [snapshot, fetchState] = await Promise.all([
    db
      .prepare(
        `SELECT status, digest, fetched_at FROM deadlock_snapshots WHERE account_id = ?`,
      )
      .bind(link.accountId)
      .first<{
        status: string;
        digest: string | null;
        fetched_at: number | null;
      }>(),
    db
      .prepare(`SELECT * FROM deadlock_fetch_state WHERE account_id = ?`)
      .bind(link.accountId)
      .first<FetchStateRow>(),
  ]);

  const digest = parseDigest(snapshot?.digest ?? null);
  let state: DeadlockState = "pending";
  if (snapshot?.status === "protected") state = "protected";
  else if (snapshot?.status === "empty") state = "empty";
  else if (snapshot?.status === "ok" && digest) state = "ok";

  const fetchedAt = snapshot?.fetched_at ?? null;
  const stale =
    state === "ok" &&
    !!fetchState &&
    fetchState.fail_count > 0 &&
    fetchedAt !== null &&
    now - fetchedAt > DEADLOCK_STALE_AFTER;

  let refresh: number | null = null;
  if (input.track) {
    if (!fetchState) {
      await db
        .prepare(
          `INSERT INTO deadlock_fetch_state (account_id, user_id, next_due_at, queued_at)
           VALUES (?, ?, ?, ?) ON CONFLICT(account_id) DO NOTHING`,
        )
        .bind(link.accountId, input.ownerId, now, now)
        .run();
      refresh = link.accountId;
    } else {
      const last = Math.max(fetchedAt ?? 0, fetchState.attempted_at ?? 0);
      const due =
        now - last > DEADLOCK_CADENCE.viewStale &&
        (fetchState.retry_at === null || fetchState.retry_at <= now) &&
        (fetchState.queued_at === null ||
          now - fetchState.queued_at > DEADLOCK_CADENCE.queueDedupe);
      if (due) {
        const claimed = await db
          .prepare(
            `UPDATE deadlock_fetch_state SET queued_at = ?
             WHERE account_id = ? AND (queued_at IS NULL OR queued_at <= ?)`,
          )
          .bind(now, link.accountId, now - DEADLOCK_CADENCE.queueDedupe)
          .run();
        if ((claimed.meta?.changes ?? 0) > 0) refresh = link.accountId;
      }
    }
  }

  const shown =
    digest && state === "ok"
      ? input.isOwner
        ? digest
        : digestForRange(digest, input.range)
      : null;
  const data: DeadlockBoardData = {
    state,
    digest: shown,
    fetched_at: fetchedAt !== null ? new Date(fetchedAt).toISOString() : null,
    stale,
  };
  if (input.isOwner) data.persona = link.persona;
  return { data, refresh };
}
