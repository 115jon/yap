/**
 * Listen Together bookkeeping for the profile Activity timeline.
 *
 * A listening session is one user's stretch in a room while tracks play. Every
 * time a track starts, each user in the room gets a tally of one more track
 * (and of that title). The tally is flushed, and dropped, when the user
 * leaves the room or the queue stops, and the writer (recordListenSession)
 * decides whether it was long enough to show. Tallies live in the room's
 * SQLite storage so a hibernation does not lose them; they are written once
 * per track start, never per tick.
 */

export interface ListenActivitySql {
  exec<T extends Record<string, unknown>>(
    query: string,
    ...params: unknown[]
  ): Iterable<T>;
}

/** What the writer (recordListenSession) takes. */
export interface ListenActivityRecord {
  userId: string;
  sessionKey: string;
  channelId: string | null;
  startedAt: number;
  endedAt: number;
  tracks: number;
  topTrack: string | null;
}

export interface ListenActivityDeps {
  sql: ListenActivitySql;
  /** The room's key: part of each session's idempotency key. */
  getRoomKey: () => string;
  /** The voice channel the room serves, when known. */
  getChannelId: () => string | null;
  /** Hands a finished session to the writer (it never throws). */
  record: (input: ListenActivityRecord) => void;
}

const MAX_TITLE_LENGTH = 120;
/** Bounds one tally's stored size; tracks past it still count toward the total. */
const MAX_DISTINCT_TITLES = 100;

interface TallyRow extends Record<string, unknown> {
  started_at: number;
  tracks: number;
  counts_json: string;
}

export function initListenActivitySchema(sql: ListenActivitySql) {
  sql.exec(`
    CREATE TABLE IF NOT EXISTS listen_activity (
      user_id TEXT PRIMARY KEY,
      started_at INTEGER NOT NULL,
      tracks INTEGER NOT NULL DEFAULT 0,
      counts_json TEXT NOT NULL DEFAULT '{}'
    );
  `);
}

function parseCounts(value: unknown): Record<string, number> {
  if (typeof value !== "string") return {};
  try {
    const parsed: unknown = JSON.parse(value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return {};
    }
    const counts: Record<string, number> = {};
    for (const [title, count] of Object.entries(parsed)) {
      if (typeof count === "number" && Number.isFinite(count) && count > 0) {
        counts[title] = count;
      }
    }
    return counts;
  } catch {
    return {};
  }
}

/** The most counted title; the earliest to appear wins a tie. */
export function topTrackOf(counts: Record<string, number>): string | null {
  let top: string | null = null;
  let best = 0;
  for (const [title, count] of Object.entries(counts)) {
    if (count > best) {
      top = title;
      best = count;
    }
  }
  return top;
}

export class ListenActivityTracker {
  constructor(private readonly deps: ListenActivityDeps) {}

  /**
   * A track started: every user in the room (`userIds`) heard one more.
   * Blank titles still count as a track.
   */
  trackStarted(
    title: string,
    userIds: Iterable<string>,
    now: number = Date.now(),
  ) {
    const key = title.trim().slice(0, MAX_TITLE_LENGTH);
    for (const userId of new Set(userIds)) {
      if (!userId) continue;
      const row = this.load(userId);
      const counts = row ? parseCounts(row.counts_json) : {};
      if (
        key &&
        (key in counts || Object.keys(counts).length < MAX_DISTINCT_TITLES)
      ) {
        counts[key] = (counts[key] ?? 0) + 1;
      }
      this.deps.sql.exec(
        `INSERT INTO listen_activity (user_id, started_at, tracks, counts_json)
         VALUES (?, ?, 1, ?)
         ON CONFLICT(user_id) DO UPDATE SET
           tracks = listen_activity.tracks + 1,
           counts_json = excluded.counts_json`,
        userId,
        now,
        JSON.stringify(counts),
      );
    }
  }

  /**
   * A user left the room: their session ends. Does nothing when they have no
   * tally (a second call included).
   */
  flushUser(userId: string, now: number = Date.now()) {
    const row = this.load(userId);
    if (!row) return;
    this.deps.sql.exec("DELETE FROM listen_activity WHERE user_id = ?", userId);
    this.emit(userId, row, now);
  }

  /** The queue stopped: every session in the room ends. */
  flushAll(now: number = Date.now()) {
    const rows = [
      ...this.deps.sql.exec<TallyRow & { user_id: string }>(
        "SELECT user_id, started_at, tracks, counts_json FROM listen_activity",
      ),
    ];
    if (rows.length === 0) return;
    this.deps.sql.exec("DELETE FROM listen_activity");
    for (const row of rows) this.emit(row.user_id, row, now);
  }

  private load(userId: string): TallyRow | null {
    const row = [
      ...this.deps.sql.exec<TallyRow>(
        "SELECT started_at, tracks, counts_json FROM listen_activity WHERE user_id = ?",
        userId,
      ),
    ][0];
    return row ?? null;
  }

  private emit(userId: string, row: TallyRow, now: number) {
    const startedAt = Number(row.started_at);
    this.deps.record({
      userId,
      sessionKey: `${this.deps.getRoomKey()}:${startedAt}`,
      channelId: this.deps.getChannelId(),
      startedAt,
      endedAt: now,
      tracks: Number(row.tracks),
      topTrack: topTrackOf(parseCounts(row.counts_json)),
    });
  }
}
