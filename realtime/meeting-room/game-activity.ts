// ── Game activity on the global gateway ─────────────────────────────────────
// Each session may report one game activity ("Playing X", op 43). Others see
// the newest one across a user's live sessions as `game_activity` on
// PRESENCE_UPDATE and PRESENCE_LIST, unless the user is offline or invisible
// or turned "Share my activity" off (users.share_activity = 0).
//
// Unlike status, which every global session hears, an activity only goes to
// its audience: users who share a server, a friendship or a DM with its
// owner. Activity updates are sent once, not replayed: a resumed session gets
// a fresh PRESENCE_LIST instead (handleResume), and sockets close with their
// activity (the client sends it again after Ready or Resumed).
//
// Updates are coalesced per user: at most one every BROADCAST_INTERVAL_MS,
// the last value wins, and an update that changes nothing visible is skipped.

import {
  pickGameActivity,
  type GameActivity,
} from "../../src/lib/gateway-protocol";

export const GAME_ACTIVITY_STORAGE_PREFIX = "act:";
export const GAME_ACTIVITY_BROADCAST_INTERVAL_MS = 5_000;
/** How long a user's audience (who may see their activity) is reused. */
export const GAME_ACTIVITY_AUDIENCE_TTL_MS = 60_000;
/** A started_at older than this is treated as "now" (a stale clock). */
const MAX_ACTIVITY_AGE_MS = 7 * 24 * 60 * 60 * 1000;
const MAX_AUDIENCE_CACHE_ENTRIES = 2_000;

interface StoredGameActivity {
  user_id: string;
  activity: GameActivity;
  updated_at: number;
  /** The owner's share_activity flag when this was written. */
  share: boolean;
}

interface SessionActivity {
  userId: string;
  activity: GameActivity;
  updatedAt: number;
}

interface BroadcastState {
  lastSentAt: number;
  /** JSON of the activity last sent to the audience ("null" for none). */
  lastSentKey: string;
  pending: boolean;
}

interface AudienceEntry {
  ids: Set<string>;
  expiresAt: number;
}

/** `game_activity` per PRESENCE_LIST entry, and whether a catch-up is due. */
export interface PresenceListActivities {
  activities: Map<string, GameActivity | null>;
  catchUp: boolean;
}

interface ActivitySession {
  id: string;
  user_id?: string;
}

interface PresenceSnapshot {
  user_id: string;
  status: string;
  platforms: unknown[];
}

interface AudienceDatabase {
  prepare(query: string): {
    bind(...values: unknown[]): {
      all<T>(): Promise<{ results?: T[] }>;
      first<T>(): Promise<T | null>;
    };
  };
}

interface ActivityStorage {
  list<T>(options: { prefix: string }): Promise<Map<string, T>>;
  put(key: string, value: unknown): Promise<void>;
  delete(keys: string | string[]): Promise<boolean | number>;
}

export interface GameActivityHost {
  storage: ActivityStorage;
  db: AudienceDatabase;
  waitUntil(promise: Promise<unknown>): void;
  /** Identified sessions on this object, by socket. */
  sessions(): Iterable<[WebSocket, ActivitySession]>;
  /** False for a socket that is closing or still replaying a resume. */
  canDeliver(ws: WebSocket): boolean;
  presenceSnapshot(userId: string): PresenceSnapshot;
  send(ws: WebSocket, msg: { op: number; d: unknown }): void;
  dispatchOp: number;
  log: { error(...args: unknown[]): void };
  /**
   * A session's game ended (cleared, replaced by another game, or its
   * socket closed) while its owner shared activity: the profile Activity
   * timeline and play-time rollup record it. Called once per session's
   * game; the writer is idempotent across sessions reporting the same one.
   */
  onGameEnded?(userId: string, activity: GameActivity, endedAt: number): void;
}

const AUDIENCE_QUERIES = [
  // Users who share a server.
  `SELECT DISTINCT other.user_id AS id
     FROM server_members mine
     JOIN server_members other ON other.server_id = mine.server_id
    WHERE mine.user_id = ?`,
  // Friends (accepted; both directions are stored).
  "SELECT target_user_id AS id FROM relationships WHERE user_id = ? AND type = 0",
  // Users in a DM or group DM with them.
  `SELECT DISTINCT other.user_id AS id
     FROM dm_recipients mine
     JOIN dm_recipients other ON other.channel_id = mine.channel_id
    WHERE mine.user_id = ?`,
] as const;

function activityKey(activity: GameActivity | null): string {
  return activity ? JSON.stringify(activity) : "null";
}

function normalizeStored(value: unknown): StoredGameActivity | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<StoredGameActivity>;
  if (
    typeof record.user_id !== "string" ||
    typeof record.updated_at !== "number" ||
    typeof record.share !== "boolean" ||
    !record.activity ||
    typeof record.activity !== "object" ||
    typeof record.activity.name !== "string"
  ) {
    return null;
  }
  return record as StoredGameActivity;
}

/** started_at in the future or implausibly old becomes `now`. */
export function clampActivityStart(startedAt: number, now: number): number {
  if (startedAt > now || now - startedAt > MAX_ACTIVITY_AGE_MS) return now;
  return startedAt;
}

export class GameActivityRegistry {
  /** Coalescing window; tests shorten it. */
  broadcastIntervalMs = GAME_ACTIVITY_BROADCAST_INTERVAL_MS;
  private readonly bySession = new Map<string, SessionActivity>();
  /** share_activity per user with a live session that set an activity. */
  private readonly shareByUser = new Map<string, boolean>();
  private readonly audiences = new Map<string, AudienceEntry>();
  private readonly broadcasts = new Map<string, BroadcastState>();

  constructor(private readonly host: GameActivityHost) {}

  get size(): number {
    return this.bySession.size;
  }

  /** Loads stored activities of sessions that survived hibernation. */
  async restore(liveSessions: Map<string, string | undefined>) {
    const stored = await this.host.storage.list<unknown>({
      prefix: GAME_ACTIVITY_STORAGE_PREFIX,
    });
    const stale: string[] = [];
    for (const [key, value] of stored) {
      const sessionId = key.slice(GAME_ACTIVITY_STORAGE_PREFIX.length);
      const record = normalizeStored(value);
      if (!record || liveSessions.get(sessionId) !== record.user_id) {
        stale.push(key);
        continue;
      }
      this.bySession.set(sessionId, {
        userId: record.user_id,
        activity: record.activity,
        updatedAt: record.updated_at,
      });
      this.shareByUser.set(record.user_id, record.share);
    }
    for (const userId of new Set(
      [...this.bySession.values()].map((entry) => entry.userId),
    )) {
      // What the audience last heard, so a later clear is not skipped.
      this.broadcasts.set(userId, {
        lastSentAt: 0,
        lastSentKey: activityKey(this.visible(userId)),
        pending: false,
      });
    }
    for (let i = 0; i < stale.length; i += 128) {
      await this.host.storage.delete(stale.slice(i, i + 128));
    }
  }

  /**
   * Sets (or with `null` clears) one session's activity. Returns false when
   * nothing changed. The caller checks the session is still live after the
   * await (the first activity of a user reads share_activity from D1).
   */
  async set(
    sessionId: string,
    userId: string,
    activity: GameActivity | null,
    now = Date.now(),
  ): Promise<boolean> {
    const previous = this.bySession.get(sessionId);
    if (activity === null) return this.removeSession(sessionId, now);
    const picked = pickGameActivity(activity);
    const next: GameActivity = {
      ...picked,
      started_at: clampActivityStart(picked.started_at, now),
    };
    if (
      previous &&
      previous.userId === userId &&
      activityKey(previous.activity) === activityKey(next)
    ) {
      return false;
    }
    if (!this.shareByUser.has(userId)) {
      const share = await this.loadShare(userId);
      // A profile update (setShare) may have answered while D1 was read.
      if (!this.shareByUser.has(userId)) this.shareByUser.set(userId, share);
    }
    if (
      previous &&
      previous.userId === userId &&
      (previous.activity.name !== next.name ||
        previous.activity.started_at !== next.started_at)
    ) {
      this.reportEnded(previous, now);
    }
    this.bySession.set(sessionId, { userId, activity: next, updatedAt: now });
    this.persist(sessionId);
    return true;
  }

  /** The socket closed: its activity goes with it. */
  removeSession(sessionId: string, now = Date.now()): boolean {
    const entry = this.bySession.get(sessionId);
    if (!entry) return false;
    this.reportEnded(entry, now);
    this.bySession.delete(sessionId);
    this.host.waitUntil(
      this.host.storage
        .delete(GAME_ACTIVITY_STORAGE_PREFIX + sessionId)
        .catch((error) =>
          this.host.log.error("Failed to delete game activity", error),
        ),
    );
    return true;
  }

  /** Hands an ended game to the host, if its owner shares activity. */
  private reportEnded(entry: SessionActivity, endedAt: number) {
    if (!this.host.onGameEnded) return;
    if (this.shareByUser.get(entry.userId) !== true) return;
    try {
      this.host.onGameEnded(entry.userId, entry.activity, endedAt);
    } catch (error) {
      this.host.log.error("Game end report failed", error);
    }
  }

  /** The user has an activity, or their audience last heard one. */
  hasActivityFor(userId: string): boolean {
    const state = this.broadcasts.get(userId);
    if (state && (state.pending || state.lastSentKey !== "null")) return true;
    for (const entry of this.bySession.values()) {
      if (entry.userId === userId) return true;
    }
    return false;
  }

  /**
   * The activity others may see: the newest across the user's live
   * sessions, or null when they share none, are offline or invisible, or
   * have sharing off (or its flag could not be read).
   */
  visible(userId: string): GameActivity | null {
    if (this.shareByUser.get(userId) !== true) return null;
    if (this.host.presenceSnapshot(userId).status === "offline") return null;
    let newest: SessionActivity | null = null;
    for (const [, session] of this.host.sessions()) {
      if (session.user_id !== userId) continue;
      const entry = this.bySession.get(session.id);
      if (entry && (!newest || entry.updatedAt > newest.updatedAt)) {
        newest = entry;
      }
    }
    return newest?.activity ?? null;
  }

  /** "Share my activity" changed (USER_PROFILE_UPDATE from the profile route). */
  setShare(userId: string, share: boolean) {
    let live = false;
    for (const [, session] of this.host.sessions()) {
      if (session.user_id === userId) live = true;
    }
    if (!live) {
      this.shareByUser.delete(userId);
      return;
    }
    if (this.shareByUser.get(userId) === share) return;
    this.shareByUser.set(userId, share);
    for (const [sessionId, entry] of this.bySession) {
      if (entry.userId === userId) this.persist(sessionId);
    }
    this.schedule(userId);
  }

  /**
   * Sends the user's visible activity to their audience, at most once per
   * broadcastIntervalMs (last value wins), skipping an unchanged value.
   */
  schedule(userId: string, minDelayMs = 0) {
    let state = this.broadcasts.get(userId);
    if (!state) {
      state = { lastSentAt: 0, lastSentKey: "null", pending: false };
      this.broadcasts.set(userId, state);
    }
    if (state.pending) return;
    state.pending = true;
    const wait = Math.max(
      minDelayMs,
      state.lastSentAt + this.broadcastIntervalMs - Date.now(),
    );
    const flush = async () => {
      if (wait > 0) await new Promise((resolve) => setTimeout(resolve, wait));
      await this.flush(userId);
    };
    this.host.waitUntil(
      flush().catch((error) =>
        this.host.log.error("Game activity broadcast failed", error),
      ),
    );
  }

  private async flush(userId: string) {
    const state = this.broadcasts.get(userId);
    if (!state) return;
    state.pending = false;
    const pendingKey = activityKey(this.visible(userId));
    if (pendingKey === state.lastSentKey) {
      this.forgetIfGone(userId);
      return;
    }
    state.lastSentAt = Date.now();
    const audience = await this.audienceOf(userId);
    // Build after the await: the value sent is the latest one.
    const activity = this.visible(userId);
    state.lastSentKey = activityKey(activity);
    const msg = {
      op: this.host.dispatchOp,
      d: {
        event: "PRESENCE_UPDATE",
        data: {
          ...this.host.presenceSnapshot(userId),
          game_activity: activity,
        },
      },
    };
    for (const [ws, session] of this.host.sessions()) {
      if (!session.user_id) continue;
      if (session.user_id !== userId && !audience.has(session.user_id)) {
        continue;
      }
      if (!this.host.canDeliver(ws)) continue;
      this.host.send(ws, msg);
    }
    this.forgetIfGone(userId);
  }

  /**
   * `game_activity` for each PRESENCE_LIST entry as `viewerId` may see it.
   * Uses the viewer's cached audience; when it is not cached and someone
   * has an activity, those entries are null and `catchUp` is true: send
   * the list, then call catchUp(ws, viewerId).
   */
  forPresenceList(
    viewerId: string | undefined,
    userIds: Iterable<string>,
  ): PresenceListActivities {
    const activities = new Map<string, GameActivity | null>();
    const audience = viewerId ? this.cachedAudience(viewerId) : null;
    let catchUp = false;
    for (const userId of userIds) {
      const activity = this.visible(userId);
      if (!activity) {
        activities.set(userId, null);
      } else if (userId === viewerId || audience?.has(userId)) {
        activities.set(userId, activity);
      } else {
        activities.set(userId, null);
        if (viewerId && !audience) catchUp = true;
      }
    }
    return { activities, catchUp };
  }

  /** Loads the viewer's audience ahead of a PRESENCE_LIST, if anyone plays. */
  async prefetchAudience(viewerId: string | undefined) {
    if (!viewerId || this.bySession.size === 0) return;
    await this.audienceOf(viewerId);
  }

  /** Sends the activities a PRESENCE_LIST had to leave out (see above). */
  async catchUp(ws: WebSocket, viewerId: string) {
    const audience = await this.audienceOf(viewerId);
    const users = new Set<string>();
    for (const entry of this.bySession.values()) users.add(entry.userId);
    for (const userId of users) {
      if (userId === viewerId || !audience.has(userId)) continue;
      const activity = this.visible(userId);
      if (!activity || !this.host.canDeliver(ws)) continue;
      this.host.send(ws, {
        op: this.host.dispatchOp,
        d: {
          event: "PRESENCE_UPDATE",
          data: {
            ...this.host.presenceSnapshot(userId),
            game_activity: activity,
          },
        },
      });
    }
  }

  /** Drops activities of sessions that are gone (alarm safety net). */
  prune(liveSessionIds: Set<string>) {
    for (const sessionId of [...this.bySession.keys()]) {
      if (!liveSessionIds.has(sessionId)) {
        const userId = this.bySession.get(sessionId)?.userId;
        this.removeSession(sessionId);
        if (userId) this.schedule(userId);
      }
    }
    const now = Date.now();
    for (const [userId, entry] of this.audiences) {
      if (entry.expiresAt <= now) this.audiences.delete(userId);
    }
  }

  private cachedAudience(userId: string): Set<string> | null {
    const entry = this.audiences.get(userId);
    return entry && entry.expiresAt > Date.now() ? entry.ids : null;
  }

  /** Who may see `userId`'s activity. A failed query means nobody else. */
  private async audienceOf(userId: string): Promise<Set<string>> {
    const cached = this.cachedAudience(userId);
    if (cached) return cached;
    let complete = true;
    const rows = await Promise.all(
      AUDIENCE_QUERIES.map((query) =>
        this.host.db
          .prepare(query)
          .bind(userId)
          .all<{ id: unknown }>()
          .then((result) => result.results ?? [])
          .catch((error) => {
            complete = false;
            this.host.log.error("Game activity audience query failed", error);
            return [];
          }),
      ),
    );
    const ids = new Set<string>();
    for (const row of rows.flat()) {
      if (typeof row.id === "string" && row.id !== userId) ids.add(row.id);
    }
    if (complete) {
      if (this.audiences.size >= MAX_AUDIENCE_CACHE_ENTRIES) {
        const now = Date.now();
        for (const [key, entry] of this.audiences) {
          if (entry.expiresAt <= now) this.audiences.delete(key);
        }
        if (this.audiences.size >= MAX_AUDIENCE_CACHE_ENTRIES) {
          this.audiences.clear();
        }
      }
      this.audiences.set(userId, {
        ids,
        expiresAt: Date.now() + GAME_ACTIVITY_AUDIENCE_TTL_MS,
      });
    }
    return ids;
  }

  /** users.share_activity; a missing row is the default (on), an error off. */
  private async loadShare(userId: string): Promise<boolean> {
    try {
      const row = await this.host.db
        .prepare("SELECT share_activity FROM users WHERE id = ?")
        .bind(userId)
        .first<{ share_activity: unknown }>();
      if (!row) return true;
      return row.share_activity !== 0 && row.share_activity !== false;
    } catch (error) {
      this.host.log.error("Failed to read share_activity", error);
      return false;
    }
  }

  private persist(sessionId: string) {
    const entry = this.bySession.get(sessionId);
    if (!entry) return;
    const record: StoredGameActivity = {
      user_id: entry.userId,
      activity: entry.activity,
      updated_at: entry.updatedAt,
      share: this.shareByUser.get(entry.userId) === true,
    };
    this.host.waitUntil(
      this.host.storage
        .put(GAME_ACTIVITY_STORAGE_PREFIX + sessionId, record)
        .catch((error) =>
          this.host.log.error("Failed to store game activity", error),
        ),
    );
  }

  /** A user with no session left: forget their flag and broadcast state. */
  private forgetIfGone(userId: string) {
    for (const [, session] of this.host.sessions()) {
      if (session.user_id === userId) return;
    }
    const state = this.broadcasts.get(userId);
    if (state?.pending) return;
    this.shareByUser.delete(userId);
    this.broadcasts.delete(userId);
  }
}
