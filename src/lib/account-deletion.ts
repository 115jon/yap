// ── Account deletion: the grace period gate ─────────────────────────────────
// An account that asked to be deleted (account_deletions, migration 045)
// keeps its Kova identity but can do nothing on Yap until it is restored or
// purged: the auth path (requireAuth and the edge's signedInIdentity) answers
// every request with 403 ACCOUNT_PENDING_DELETION and the date, except the
// routes that pass `allowPendingDeletion` (the deletion status and restore).
// Clients read that answer as "show the restore screen".
//
// Like suspensions (lib/suspension.ts), the check reads one small set per
// isolate and keeps it for PENDING_DELETION_TTL_MS. A hit is confirmed with
// a direct read, so a restore made in another isolate takes effect at once;
// a request made in another isolate may let the account through for up to
// the TTL, which the deletion route covers by closing its sockets and
// signing out its other sessions. If the set cannot be read the check fails
// open, as suspensions do: an outage must not lock everyone out.

import { env } from "cloudflare:workers";

import { ACCOUNT_PENDING_DELETION_CODE } from "@/lib/account-deletion-events";

export {
  ACCOUNT_DELETION_GRACE_DAYS,
  DELETED_USER_DISPLAY_NAME,
  deletionDateFrom,
} from "@/lib/account-deletion-events";

export const ACCOUNT_PENDING_DELETION = ACCOUNT_PENDING_DELETION_CODE;
export const PENDING_DELETION_TTL_MS = 15_000;

type GateDb = Pick<D1Database, "prepare">;

let cached: { at: number; ids: ReadonlySet<string> } | null = null;

function defaultDb(): GateDb | null {
  const db = (env as { DB?: GateDb } | undefined)?.DB;
  return db && typeof db.prepare === "function" ? db : null;
}

async function pendingIds(db: GateDb, now: number) {
  if (cached && now - cached.at < PENDING_DELETION_TTL_MS) return cached.ids;
  try {
    const { results } = await db
      .prepare(`SELECT user_id FROM account_deletions`)
      .all<{ user_id: string }>();
    const ids = new Set((results ?? []).map((row) => row.user_id));
    cached = { at: now, ids };
    return ids;
  } catch (error) {
    console.warn(
      `account deletion check unavailable: ${error instanceof Error ? error.message : String(error)}`,
    );
    const ids = cached?.ids ?? new Set<string>();
    cached = { at: now, ids };
    return ids;
  }
}

/**
 * When the account will be deleted (ISO), or null when it is not waiting for
 * deletion (or the check could not run).
 */
export async function pendingDeletionDate(
  userId: string,
  db: GateDb | null = defaultDb(),
  now: number = Date.now(),
): Promise<string | null> {
  if (!db) return null;
  const ids = await pendingIds(db, now);
  if (!ids.has(userId)) return null;
  try {
    const row = await db
      .prepare(`SELECT delete_after FROM account_deletions WHERE user_id = ?`)
      .bind(userId)
      .first<{ delete_after: string }>();
    return row?.delete_after ?? null;
  } catch {
    return null;
  }
}

/** Forget this isolate's copy (after a request or restore here). */
export function invalidatePendingDeletionCache(): void {
  cached = null;
}

/** The sentence the 403 carries; clients show their own restore screen. */
export function pendingDeletionMessage(deleteAfter: string): string {
  const day = new Date(deleteAfter);
  const when = Number.isNaN(day.getTime())
    ? "soon"
    : `on ${day.toISOString().slice(0, 10)}`;
  return `This account is scheduled for deletion ${when}. Restore it to use Yap again.`;
}
