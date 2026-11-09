// ── Account suspension ──────────────────────────────────────────────────────
// A site admin can suspend an account (users.suspended_at). The auth path
// (requireAuth and the edge's signedInUserId) refuses a suspended caller
// with 403 ACCOUNT_SUSPENDED, so no API call, socket ticket or upload goes
// through for them while it lasts.
//
// The check runs on every authenticated request, so it reads one small set
// (the suspended ids, via a partial index) and keeps it for SUSPENSION_TTL_MS
// per isolate. A suspension made here clears this isolate's copy at once;
// other isolates pick it up within the TTL. If the set cannot be read (D1
// down, or the migration not applied yet) the check fails open: an outage
// must not sign everyone out.

import { env } from "cloudflare:workers";

export const SUSPENSION_TTL_MS = 15_000;
export const ACCOUNT_SUSPENDED = "ACCOUNT_SUSPENDED";

type SuspensionDb = Pick<D1Database, "prepare">;

let cached: { at: number; ids: ReadonlySet<string> } | null = null;

function defaultDb(): SuspensionDb | null {
  const db = (env as { DB?: SuspensionDb } | undefined)?.DB;
  return db && typeof db.prepare === "function" ? db : null;
}

/** The suspended account ids, from this isolate's copy while it is fresh. */
export async function suspendedUserIds(
  db: SuspensionDb | null = defaultDb(),
  now: number = Date.now(),
): Promise<ReadonlySet<string>> {
  if (cached && now - cached.at < SUSPENSION_TTL_MS) return cached.ids;
  if (!db) return new Set();
  try {
    const { results } = await db
      .prepare(`SELECT id FROM users WHERE suspended_at IS NOT NULL`)
      .all<{ id: string }>();
    const ids = new Set((results ?? []).map((row) => row.id));
    cached = { at: now, ids };
    return ids;
  } catch (error) {
    console.warn(
      `suspension check unavailable: ${error instanceof Error ? error.message : String(error)}`,
    );
    // Keep the last answer (or none) for the TTL too, so an outage costs
    // one failed query per isolate per TTL, not one per request.
    const ids = cached?.ids ?? new Set<string>();
    cached = { at: now, ids };
    return ids;
  }
}

export async function isSuspended(
  userId: string,
  db?: SuspensionDb | null,
): Promise<boolean> {
  const ids = await suspendedUserIds(db === undefined ? defaultDb() : db);
  return ids.has(userId);
}

/** Forget this isolate's copy (after a suspend or unsuspend here). */
export function invalidateSuspensionCache(): void {
  cached = null;
}

export const SUSPENDED_MESSAGE =
  "This account is suspended. Contact the Yap team if you think this is a mistake.";
