/**
 * The Deadlock fetcher: keeps one digest per tracked Steam account fresh
 * without spending more of deadlock-api.com than it must.
 *
 * One run takes the accounts that are due (or the ones named), and then:
 *   1. one batch rank call and one batch hero-stats call for all of them;
 *      together they say, per account, whether a match was played since
 *      the last run (`signal`);
 *   2. for an account with a new match (or a post-game check, or a history
 *      older than a day): its match history, reduced to the digest, and the
 *      newest match's metadata once (matches never change);
 *   3. the digest, the next due time and the failure count are written.
 *
 * A failure keeps the last good digest: the API answering 429/503/504 sets
 * `retry_at` from Retry-After, and every failure backs the account off
 * exponentially up to six hours. A 403 means the account asked the API to
 * delete its data: the digest is dropped and the state is `protected`.
 *
 * Cadence (`nextDueAt`): every 10 minutes while the owner plays Deadlock
 * and for two hours after (presence), checks at +3 and +15 minutes after a
 * game ends (queued), every 6 hours otherwise, daily after 30 idle days.
 * A viewed plate older than 15 minutes queues one fetch.
 */

import {
  buildDigest,
  type ApiHistoryRow,
  type ApiRank,
  type DeadlockDigest,
  type MatchExtras,
  itemAssetKey,
  reduceMatchMetadata,
} from "@/lib/deadlock";

import type { ApiFailureKind, DeadlockApi } from "./api";
import {
  loadManifest,
  mirrorDeadlockAssets,
  type StoredManifest,
} from "./assets";

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

export const DEADLOCK_CADENCE = {
  playing: 10 * MINUTE,
  /** After the game closes, the fast cadence lasts this long. */
  playingTail: 2 * HOUR,
  /** A presence session older than this without an end counts as over. */
  playingMax: 12 * HOUR,
  idle: 6 * HOUR,
  lapsed: DAY,
  lapsedAfter: 30 * DAY,
  /** A viewed plate older than this queues a fetch. */
  viewStale: 15 * MINUTE,
  /** One on-demand fetch per account per this long. */
  queueDedupe: 5 * MINUTE,
  /** The history is re-read at least this often, new match or not. */
  historyMaxAge: DAY,
  /** After a game: check again at these delays (the API's ingest lag). */
  postGame: [3 * MINUTE, 15 * MINUTE],
  backoffBase: 5 * MINUTE,
  backoffMax: 6 * HOUR,
} as const;

/** Accounts one run fetches at most; the rest stay due for the next. */
export const DEADLOCK_BATCH_MAX = 50;
/** The fetcher shows "6h old" once the copy is older than this and failing. */
export const DEADLOCK_STALE_AFTER = 30 * MINUTE;

export type RefreshReason = "cron" | "view" | "post_game" | "manual";

export interface FetchStateRow {
  account_id: number;
  user_id: string;
  next_due_at: number;
  retry_at: number | null;
  fail_count: number;
  last_error: string | null;
  attempted_at: number | null;
  signal: string | null;
  history_at: number | null;
  live_since: number | null;
  live_until: number | null;
  queued_at: number | null;
}

interface SnapshotRow {
  account_id: number;
  status: string;
  digest: string | null;
  fetched_at: number | null;
  newest_match_id: number | null;
}

export interface FetcherDeps {
  db: D1Database;
  api: DeadlockApi;
  kv: KVNamespace;
  bucket: R2Bucket;
  /** For the art (mirrorDeadlockAssets); the API client has its own. */
  fetch: typeof fetch;
  /** Tells open profiles of `userId` that the plate's data changed. */
  notify?(userId: string): Promise<void>;
  now?: () => number;
}

export interface RefreshOutcome {
  account_id: number;
  result:
    | "updated"
    | "unchanged"
    | "empty"
    | "protected"
    | "failed"
    | "skipped";
  error?: ApiFailureKind | "no_manifest";
}

/** When an account is next due after a good fetch. */
export function nextDueAt(
  now: number,
  state: Pick<FetchStateRow, "live_since" | "live_until">,
  lastPlayedAt: number | null,
): number {
  const c = DEADLOCK_CADENCE;
  const live =
    state.live_since !== null &&
    state.live_since > now - c.playingMax &&
    (state.live_until === null || state.live_until > now - c.playingTail);
  if (live) return now + c.playing;
  if (lastPlayedAt !== null && lastPlayedAt * 1000 < now - c.lapsedAfter) {
    return now + c.lapsed;
  }
  return now + c.idle;
}

/** The wait after the `failCount`th failure in a row. */
export function backoffMs(failCount: number, retryAfterMs: number | null) {
  const c = DEADLOCK_CADENCE;
  const exponential = Math.min(
    c.backoffMax,
    c.backoffBase * 2 ** Math.max(0, failCount - 1),
  );
  return Math.min(c.backoffMax, Math.max(exponential, retryAfterMs ?? 0));
}

function chunk<T>(items: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += size)
    out.push(items.slice(i, i + size));
  return out;
}

async function selectRows(
  db: D1Database,
  now: number,
  accountIds: readonly number[] | null,
): Promise<FetchStateRow[]> {
  if (accountIds) {
    if (accountIds.length === 0) return [];
    const rows: FetchStateRow[] = [];
    for (const part of chunk([...new Set(accountIds)], 90)) {
      const { results } = await db
        .prepare(
          `SELECT * FROM deadlock_fetch_state
           WHERE account_id IN (${part.map(() => "?").join(",")})
             AND (retry_at IS NULL OR retry_at <= ?)`,
        )
        .bind(...part, now)
        .all<FetchStateRow>();
      rows.push(...(results ?? []));
    }
    return rows.slice(0, DEADLOCK_BATCH_MAX);
  }
  const { results } = await db
    .prepare(
      `SELECT * FROM deadlock_fetch_state
       WHERE next_due_at <= ? AND (retry_at IS NULL OR retry_at <= ?)
       ORDER BY next_due_at LIMIT ?`,
    )
    .bind(now, now, DEADLOCK_BATCH_MAX)
    .all<FetchStateRow>();
  return results ?? [];
}

async function snapshotsFor(
  db: D1Database,
  accountIds: number[],
): Promise<Map<number, SnapshotRow>> {
  const map = new Map<number, SnapshotRow>();
  for (const part of chunk(accountIds, 90)) {
    const { results } = await db
      .prepare(
        `SELECT account_id, status, digest, fetched_at, newest_match_id
         FROM deadlock_snapshots WHERE account_id IN (${part.map(() => "?").join(",")})`,
      )
      .bind(...part)
      .all<SnapshotRow>();
    for (const row of results ?? []) map.set(row.account_id, row);
  }
  return map;
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

interface HeroStatRow {
  account_id?: number;
  matches_played?: number;
  last_played?: number;
}

function signalOf(rank: ApiRank | undefined, stats: HeroStatRow[]): string {
  let matches = 0;
  let last = 0;
  for (const row of stats) {
    matches += Number(row.matches_played) || 0;
    last = Math.max(last, Number(row.last_played) || 0);
  }
  return `${rank?.last_match?.match_id ?? 0}:${matches}:${last}`;
}

/** What the old digest knew about its newest match, reused when unchanged. */
function extrasFrom(digest: DeadlockDigest | null): MatchExtras | null {
  const last = digest?.last_match;
  if (!last) return null;
  return {
    match_id: last.id,
    level: last.level,
    hero_damage: last.hero_damage,
    objective_damage: last.objective_damage,
    items: last.items,
  };
}

/**
 * Fetches the accounts that are due (or `accountIds`, still respecting
 * their Retry-After). Never throws for an API failure; each account's
 * outcome is returned and written.
 */
export async function refreshDeadlockAccounts(
  deps: FetcherDeps,
  input: { accountIds?: readonly number[]; reason?: RefreshReason } = {},
): Promise<RefreshOutcome[]> {
  const now = (deps.now ?? Date.now)();
  const reason = input.reason ?? "cron";
  const rows = await selectRows(deps.db, now, input.accountIds ?? null);
  if (rows.length === 0) return [];
  const ids = rows.map((row) => row.account_id);

  // Claim the rows before any call, so an overlapping run skips them.
  await deps.db.batch(
    ids.map((id) =>
      deps.db
        .prepare(
          `UPDATE deadlock_fetch_state SET attempted_at = ?, queued_at = NULL,
             next_due_at = MAX(next_due_at, ?) WHERE account_id = ?`,
        )
        .bind(now, now + DEADLOCK_CADENCE.playing, id),
    ),
  );

  const manifest = await loadManifest(deps);
  if (!manifest) {
    await recordFailures(deps.db, rows, now, "unavailable", null);
    return rows.map((row) => ({
      account_id: row.account_id,
      result: "failed",
      error: "no_manifest",
    }));
  }

  const joined = ids.join(",");
  const [ranks, heroStats] = await Promise.all([
    deps.api.get<ApiRank[]>("/v1/players/rank", { account_ids: joined }),
    deps.api.get<HeroStatRow[]>("/v1/players/hero-stats", {
      account_ids: joined,
    }),
  ]);
  if (!ranks.ok || !heroStats.ok) {
    const failed = !ranks.ok ? ranks : heroStats;
    if (!failed.ok) {
      await recordFailures(
        deps.db,
        rows,
        now,
        failed.kind,
        failed.retryAfterMs,
      );
      return rows.map((row) => ({
        account_id: row.account_id,
        result: "failed",
        error: failed.kind,
      }));
    }
  }
  const rankById = new Map<number, ApiRank>();
  for (const rank of ranks.ok && Array.isArray(ranks.body) ? ranks.body : []) {
    if (rank && typeof rank.account_id === "number") {
      rankById.set(rank.account_id, rank);
    }
  }
  const statsById = new Map<number, HeroStatRow[]>();
  for (const row of heroStats.ok && Array.isArray(heroStats.body)
    ? heroStats.body
    : []) {
    if (!row || typeof row.account_id !== "number") continue;
    const list = statsById.get(row.account_id) ?? [];
    list.push(row);
    statsById.set(row.account_id, list);
  }

  const snapshots = await snapshotsFor(deps.db, ids);
  const outcomes: RefreshOutcome[] = [];
  // One account at a time: the API client already keeps two calls in
  // flight, and a history is up to a few MB to parse.
  for (const row of rows) {
    outcomes.push(
      await refreshOne(deps, {
        row,
        snapshot: snapshots.get(row.account_id) ?? null,
        rank: rankById.get(row.account_id),
        stats: statsById.get(row.account_id) ?? [],
        manifest,
        reason,
        now,
      }),
    );
  }
  return outcomes;
}

async function refreshOne(
  deps: FetcherDeps,
  input: {
    row: FetchStateRow;
    snapshot: SnapshotRow | null;
    rank: ApiRank | undefined;
    stats: HeroStatRow[];
    manifest: StoredManifest;
    reason: RefreshReason;
    now: number;
  },
): Promise<RefreshOutcome> {
  const { row, snapshot, manifest, now } = input;
  const accountId = row.account_id;
  const previous = parseDigest(snapshot?.digest ?? null);
  let rank = input.rank ?? null;

  // The rank batch leaves protected accounts out; ask once, singly.
  if (!rank && input.stats.length === 0) {
    const single = await deps.api.get<ApiRank>(`/v1/players/${accountId}/rank`);
    if (!single.ok && single.kind === "protected") {
      await markProtected(deps, row, now);
      return { account_id: accountId, result: "protected" };
    }
    if (single.ok && single.body && typeof single.body === "object") {
      rank = { ...single.body, account_id: accountId };
    }
  }

  const signal = signalOf(rank ?? undefined, input.stats);
  const needHistory =
    !previous ||
    snapshot?.status !== "ok" ||
    input.reason === "post_game" ||
    input.reason === "manual" ||
    signal !== row.signal ||
    row.history_at === null ||
    row.history_at < now - DEADLOCK_CADENCE.historyMaxAge;

  let digest: DeadlockDigest | null = previous;
  let newestMatchId = snapshot?.newest_match_id ?? null;
  let historyAt = row.history_at;

  if (needHistory) {
    const history = await deps.api.get<ApiHistoryRow[]>(
      `/v1/players/${accountId}/match-history`,
      {},
      // A 429 here still carries the stored history.
      { acceptStatuses: [429] },
    );
    if (!history.ok) {
      if (history.kind === "protected") {
        await markProtected(deps, row, now);
        return { account_id: accountId, result: "protected" };
      }
      await recordFailures(
        deps.db,
        [row],
        now,
        history.kind,
        history.retryAfterMs,
      );
      return { account_id: accountId, result: "failed", error: history.kind };
    }
    const rows = Array.isArray(history.body) ? history.body : [];
    historyAt = now;
    if (rows.length === 0) {
      await writeResult(deps, row, {
        status: "empty",
        digest: null,
        newestMatchId: null,
        signal,
        historyAt,
        lastPlayedAt: null,
        now,
      });
      return { account_id: accountId, result: "empty" };
    }
    let newest = 0;
    for (const entry of rows)
      newest = Math.max(newest, Number(entry.match_id) || 0);
    let extras = newest === newestMatchId ? extrasFrom(previous) : null;
    if (!extras && newest > 0) {
      const metadata = await deps.api.get<unknown>(
        `/v1/matches/${newest}/metadata`,
      );
      // Optional: a failed metadata read still updates everything else.
      if (metadata.ok) extras = reduceMatchMetadata(metadata.body, accountId);
    }
    digest = buildDigest({
      history: rows,
      rank,
      extras,
      manifest,
      now: Math.floor(now / 1000),
    });
    newestMatchId = newest || null;
    const itemKeys = (digest.last_match?.items ?? []).map(itemAssetKey);
    if (itemKeys.length > 0) {
      try {
        await mirrorDeadlockAssets(deps, manifest, itemKeys);
      } catch {
        // Art is best effort; the plate shows empty slots.
      }
    }
  } else if (digest && rank) {
    // No new match: only the rank can have moved (a decay, a daily
    // Eternus refresh). Ranges stay as of the digest's computed_at.
    const badge = Math.max(0, Number(rank.badge) || 0);
    if (digest.rank?.badge !== badge) {
      const days = [...digest.rank_days];
      if (days.length > 0 && badge > 0) days[days.length - 1] = badge;
      digest = {
        ...digest,
        rank: {
          badge,
          calibration_left: digest.rank?.calibration_left ?? null,
        },
        rank_days: days,
      };
    }
  }

  await writeResult(deps, row, {
    status: "ok",
    digest,
    newestMatchId,
    signal,
    historyAt,
    lastPlayedAt: digest?.last_played_at ?? null,
    now,
  });
  const changed =
    JSON.stringify(digest) !== JSON.stringify(previous) ||
    snapshot?.status !== "ok";
  if (changed && deps.notify) {
    try {
      await deps.notify(row.user_id);
    } catch {
      // The next open of the profile reads it anyway.
    }
  }
  return { account_id: accountId, result: changed ? "updated" : "unchanged" };
}

async function writeResult(
  deps: FetcherDeps,
  row: FetchStateRow,
  result: {
    status: "ok" | "empty";
    digest: DeadlockDigest | null;
    newestMatchId: number | null;
    signal: string;
    historyAt: number | null;
    lastPlayedAt: number | null;
    now: number;
  },
) {
  const { db } = deps;
  const due = nextDueAt(result.now, row, result.lastPlayedAt);
  await db.batch([
    db
      .prepare(
        `INSERT INTO deadlock_snapshots (account_id, user_id, status, digest, fetched_at, newest_match_id)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(account_id) DO UPDATE SET
           user_id = excluded.user_id, status = excluded.status,
           digest = excluded.digest, fetched_at = excluded.fetched_at,
           newest_match_id = excluded.newest_match_id`,
      )
      .bind(
        row.account_id,
        row.user_id,
        result.status,
        result.digest ? JSON.stringify(result.digest) : null,
        result.now,
        result.newestMatchId,
      ),
    db
      .prepare(
        `UPDATE deadlock_fetch_state SET fail_count = 0, retry_at = NULL,
           last_error = NULL, signal = ?, history_at = ?, next_due_at = ?
         WHERE account_id = ?`,
      )
      .bind(result.signal, result.historyAt, due, row.account_id),
  ]);
}

async function markProtected(
  deps: FetcherDeps,
  row: FetchStateRow,
  now: number,
) {
  const { db } = deps;
  await db.batch([
    db
      .prepare(
        `INSERT INTO deadlock_snapshots (account_id, user_id, status, digest, fetched_at, newest_match_id)
         VALUES (?, ?, 'protected', NULL, ?, NULL)
         ON CONFLICT(account_id) DO UPDATE SET
           status = 'protected', digest = NULL, fetched_at = excluded.fetched_at,
           newest_match_id = NULL`,
      )
      .bind(row.account_id, row.user_id, now),
    // Checked again daily: the owner may have undone it.
    db
      .prepare(
        `UPDATE deadlock_fetch_state SET fail_count = 0, retry_at = NULL,
           last_error = 'protected', signal = NULL, history_at = NULL,
           next_due_at = ? WHERE account_id = ?`,
      )
      .bind(now + DEADLOCK_CADENCE.lapsed, row.account_id),
  ]);
  if (deps.notify) {
    try {
      await deps.notify(row.user_id);
    } catch {
      // Best effort.
    }
  }
}

async function recordFailures(
  db: D1Database,
  rows: FetchStateRow[],
  now: number,
  kind: ApiFailureKind | "unavailable",
  retryAfterMs: number | null,
) {
  await db.batch(
    rows.map((row) => {
      const fails = row.fail_count + 1;
      const until = now + backoffMs(fails, retryAfterMs);
      return db
        .prepare(
          `UPDATE deadlock_fetch_state SET fail_count = ?, retry_at = ?,
             next_due_at = ?, last_error = ? WHERE account_id = ?`,
        )
        .bind(fails, until, until, kind, row.account_id);
    }),
  );
}

// ── Tracking ───────────────────────────────────────────────────────────────

const STEAM_ACCOUNT_SQL = `CAST(json_extract(uc.metadata, '$.account_id') AS INTEGER)`;
const HAS_WIDGET_SQL = `pb.board LIKE '%"type":"deadlock"%'`;

/**
 * Tracks exactly the accounts whose owner has a linked Steam account and
 * the widget on their board: adds the missing (due now) and drops the
 * rest, with their snapshots. Returns how many were added and dropped.
 */
export async function reconcileDeadlockTracking(
  db: D1Database,
  now: number = Date.now(),
): Promise<{ added: number; dropped: number }> {
  const added = await db
    .prepare(
      `INSERT INTO deadlock_fetch_state (account_id, user_id, next_due_at)
       SELECT ${STEAM_ACCOUNT_SQL}, uc.user_id, ?
       FROM user_connections uc JOIN profile_boards pb ON pb.user_id = uc.user_id
       WHERE uc.provider = 'steam' AND ${HAS_WIDGET_SQL}
         AND ${STEAM_ACCOUNT_SQL} > 0
       ON CONFLICT(account_id) DO NOTHING`,
    )
    .bind(now)
    .run();
  const keep = `SELECT ${STEAM_ACCOUNT_SQL} FROM user_connections uc
       JOIN profile_boards pb ON pb.user_id = uc.user_id
       WHERE uc.provider = 'steam' AND ${HAS_WIDGET_SQL}
         AND ${STEAM_ACCOUNT_SQL} IS NOT NULL`;
  const [dropped] = await db.batch([
    db.prepare(
      `DELETE FROM deadlock_fetch_state WHERE account_id NOT IN (${keep})`,
    ),
    db.prepare(
      `DELETE FROM deadlock_snapshots WHERE account_id NOT IN (${keep})`,
    ),
  ]);
  return {
    added: added.meta?.changes ?? 0,
    dropped: dropped?.meta?.changes ?? 0,
  };
}

/** The user's linked Steam account id (32-bit), or null. */
export async function linkedAccountId(
  db: D1Database,
  userId: string,
): Promise<{ accountId: number; persona: string } | null> {
  const row = await db
    .prepare(
      `SELECT ${STEAM_ACCOUNT_SQL} AS account_id, uc.display_name AS persona
       FROM user_connections uc WHERE uc.user_id = ? AND uc.provider = 'steam'`,
    )
    .bind(userId)
    .first<{ account_id: number | null; persona: string }>();
  return row && row.account_id && row.account_id > 0
    ? { accountId: row.account_id, persona: row.persona }
    : null;
}
