/**
 * Profile Activity: the timeline the server writes (never the client) and
 * the reads behind the Activity tab.
 *
 * Writes come from where the tracking already is: game activity on the
 * global gateway, voice and stream time at a voice session's end, badge
 * awards and server joins. Each event has a deterministic id and is written
 * with INSERT OR IGNORE, so a retry, a resumed socket or two gateways racing
 * never write it twice. Like the stats writes, they run beside the change
 * that caused them (waitUntil) and a failure is logged, never thrown.
 *
 * Nothing is written while the user is invisible (status 'offline' while
 * connected) or has "Share my activity" off (users.share_activity = 0), as
 * of when the session ends: the writes check the users row (RECORDS_ACTIVITY).
 *
 * Reads filter for the viewer: the owner sees everything. Others must be in
 * the owner's presence audience (a shared server, a friendship or a DM, as
 * game activity on presence; inPresenceAudience), never see hidden events or
 * kinds or calls outside a server (DM calls), and voice and stream events
 * lose the channel and server names when the viewer is not in that server.
 */

import type {
  D1Database,
  D1PreparedStatement,
} from "@cloudflare/workers-types";

import {
  ACTIVITY_MIN_LISTEN_TRACKS,
  ACTIVITY_MIN_SECONDS,
  ACTIVITY_RETENTION_DAYS,
  cleanText,
  DEFAULT_ACTIVITY_RETENTION,
  isActivityKind,
  type ActivityEvent,
  type ActivityEventData,
  type ActivityKind,
  type ActivityPage,
  type ActivitySettings,
} from "@/lib/profile-board";
import { getProfilePrefs } from "@/services/profile-prefs.service";

const DAY_MS = 86_400_000;

/**
 * The users row `u` lets the timeline record: "Share my activity" on and not
 * invisible (a chosen status of 'offline').
 */
const RECORDS_ACTIVITY =
  "u.share_activity != 0 AND COALESCE(u.status, 'online') != 'offline'";

function iso(at: number | Date): string {
  return (typeof at === "number" ? new Date(at) : at).toISOString();
}

/** The lower-cased, trimmed game name: one row per game in the rollup. */
export function gameKey(name: string): string {
  return name.trim().toLowerCase().slice(0, 128);
}

async function runActivityWrite(
  db: D1Database,
  statements: () => D1PreparedStatement[],
  what: string,
): Promise<void> {
  try {
    const list = statements();
    if (list.length > 0) await db.batch(list);
  } catch (error) {
    console.warn(
      `profile_activity ${what} write failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

// ── Games ──────────────────────────────────────────────────────────────────

export interface GameSessionInput {
  userId: string;
  name: string;
  iconUrl?: string | null;
  /** Epoch ms the game started (the activity's started_at). */
  startedAt: number;
  endedAt: number;
}

/**
 * A finished game session: an Activity event and the per-day rollup that
 * Top games reads. The rollup is added only when the event is new (it is
 * written first, guarded by the event's absence), so a second report of
 * the same session adds nothing. Sessions under five minutes are skipped.
 */
export function gameSessionStatements(
  db: D1Database,
  input: GameSessionInput,
): D1PreparedStatement[] {
  const name = cleanText(input.name, 128);
  if (!name) return [];
  const seconds = Math.floor((input.endedAt - input.startedAt) / 1000);
  if (!Number.isFinite(seconds) || seconds < ACTIVITY_MIN_SECONDS.game) {
    return [];
  }
  const capped = Math.min(seconds, DAY_MS / 1000);
  const key = gameKey(name);
  const id = `game:${input.userId}:${Math.floor(input.startedAt)}:${key}`;
  const icon =
    typeof input.iconUrl === "string" && input.iconUrl.length <= 512
      ? input.iconUrl
      : null;
  const endedIso = iso(input.endedAt);
  const data = JSON.stringify({ name, icon_url: icon });
  return [
    db
      .prepare(
        `INSERT INTO game_sessions (user_id, day, game_key, name, icon_url, seconds, sessions)
         SELECT u.id, ?, ?, ?, ?, ?, 1 FROM users u
         WHERE u.id = ? AND ${RECORDS_ACTIVITY}
           AND NOT EXISTS (SELECT 1 FROM profile_activity WHERE id = ?)
         ON CONFLICT(user_id, day, game_key) DO UPDATE SET
           seconds = game_sessions.seconds + excluded.seconds,
           sessions = game_sessions.sessions + 1,
           name = excluded.name,
           icon_url = COALESCE(excluded.icon_url, game_sessions.icon_url)`,
      )
      .bind(endedIso.slice(0, 10), key, name, icon, capped, input.userId, id),
    db
      .prepare(
        `INSERT OR IGNORE INTO profile_activity
           (id, user_id, kind, ref, data, started_at, ended_at, seconds)
         SELECT ?, u.id, 'game', ?, ?, ?, ?, ? FROM users u
         WHERE u.id = ? AND ${RECORDS_ACTIVITY}`,
      )
      .bind(
        id,
        key,
        data,
        iso(input.startedAt),
        endedIso,
        capped,
        input.userId,
      ),
  ];
}

export function recordGameSession(
  db: D1Database,
  input: GameSessionInput,
): Promise<void> {
  return runActivityWrite(db, () => gameSessionStatements(db, input), "game");
}

// ── Voice and streams ──────────────────────────────────────────────────────

export interface CallSessionInput {
  userId: string;
  /** Unique per session (the gateway's `session id:joined at`). */
  sessionKey: string;
  channelId: string | null;
  startedAt: number;
  endedAt: number;
}

/**
 * A voice session or a screen share that ended. Names come from D1 at
 * write time (a snapshot: a renamed channel keeps the old name here).
 */
function callStatement(
  db: D1Database,
  kind: "voice" | "stream",
  input: CallSessionInput,
): D1PreparedStatement | null {
  const seconds = Math.floor((input.endedAt - input.startedAt) / 1000);
  if (!Number.isFinite(seconds) || seconds < ACTIVITY_MIN_SECONDS[kind]) {
    return null;
  }
  const capped = Math.min(seconds, DAY_MS / 1000);
  const id = `${kind}:${input.userId}:${input.sessionKey}`.slice(0, 256);
  return db
    .prepare(
      `INSERT OR IGNORE INTO profile_activity
         (id, user_id, kind, ref, data, started_at, ended_at, seconds)
       SELECT ?, u.id, ?, ?,
         CASE WHEN c.id IS NULL THEN '{}' ELSE json_object(
           'channel_id', c.id,
           'channel_name', CASE WHEN c.channel_type = 'dm' THEN NULL ELSE c.name END,
           'server_id', c.server_id,
           'server_name', s.name
         ) END,
         ?, ?, ?
       FROM users u
       LEFT JOIN channels c ON c.id = ?
       LEFT JOIN servers s ON s.id = c.server_id
       WHERE u.id = ? AND ${RECORDS_ACTIVITY}`,
    )
    .bind(
      id,
      kind,
      input.channelId,
      iso(input.startedAt),
      iso(input.endedAt),
      capped,
      input.channelId,
      input.userId,
    );
}

export function callSessionStatements(
  db: D1Database,
  kind: "voice" | "stream",
  input: CallSessionInput,
): D1PreparedStatement[] {
  const statement = callStatement(db, kind, input);
  return statement ? [statement] : [];
}

export function recordCallSession(
  db: D1Database,
  kind: "voice" | "stream",
  input: CallSessionInput,
): Promise<void> {
  return runActivityWrite(
    db,
    () => callSessionStatements(db, kind, input),
    kind,
  );
}

// ── Listen Together ────────────────────────────────────────────────────────

export interface ListenSessionInput {
  userId: string;
  sessionKey: string;
  channelId: string | null;
  startedAt: number;
  endedAt: number;
  tracks: number;
  topTrack?: string | null;
}

export function listenSessionStatements(
  db: D1Database,
  input: ListenSessionInput,
): D1PreparedStatement[] {
  if (input.tracks < ACTIVITY_MIN_LISTEN_TRACKS) return [];
  const id = `listen:${input.userId}:${input.sessionKey}`.slice(0, 256);
  const topTrack = cleanText(input.topTrack ?? "", 120) || null;
  return [
    db
      .prepare(
        `INSERT OR IGNORE INTO profile_activity
           (id, user_id, kind, ref, data, started_at, ended_at, seconds)
         SELECT ?, u.id, 'listen', ?,
           json_object('tracks', ?, 'top_track', ?,
             'channel_id', c.id,
             'channel_name', CASE WHEN c.channel_type = 'dm' THEN NULL ELSE c.name END,
             'server_id', c.server_id, 'server_name', s.name),
           ?, ?, ?
         FROM users u
         LEFT JOIN channels c ON c.id = ?
         LEFT JOIN servers s ON s.id = c.server_id
         WHERE u.id = ? AND ${RECORDS_ACTIVITY}`,
      )
      .bind(
        id,
        input.channelId,
        Math.max(0, Math.floor(input.tracks)),
        topTrack,
        iso(input.startedAt),
        iso(input.endedAt),
        Math.max(0, Math.floor((input.endedAt - input.startedAt) / 1000)),
        input.channelId,
        input.userId,
      ),
  ];
}

export function recordListenSession(
  db: D1Database,
  input: ListenSessionInput,
): Promise<void> {
  return runActivityWrite(
    db,
    () => listenSessionStatements(db, input),
    "listen",
  );
}

// ── Badges and server joins ────────────────────────────────────────────────

/**
 * Badge events for awards made at `awardedAt` (batch it after the award
 * statements). Restricted to `userId` when given; the daily sweep passes
 * none and gets one event per user it awarded. Ids are `badge:<user>:<id>`,
 * so an award is one event however often it is checked.
 */
export function badgeActivityStatement(
  db: D1Database,
  awardedAt: string,
  userId?: string,
): D1PreparedStatement {
  const sql = `INSERT OR IGNORE INTO profile_activity
       (id, user_id, kind, ref, data, started_at)
     SELECT 'badge:' || user_id || ':' || badge_id, user_id, 'badge', badge_id,
       '{}', awarded_at
     FROM user_badges WHERE awarded_at = ?${userId ? " AND user_id = ?" : ""}`;
  return userId
    ? db.prepare(sql).bind(awardedAt, userId)
    : db.prepare(sql).bind(awardedAt);
}

/** "Joined <server>": one event per membership start. */
export function serverJoinStatement(
  db: D1Database,
  userId: string,
  serverId: string,
  joinedAt: string,
): D1PreparedStatement {
  return db
    .prepare(
      `INSERT OR IGNORE INTO profile_activity
         (id, user_id, kind, ref, data, started_at)
       SELECT ?, sm.user_id, 'server_join', s.id,
         json_object('server_id', s.id, 'server_name', s.name), ?
       FROM server_members sm JOIN servers s ON s.id = sm.server_id
       WHERE sm.user_id = ? AND sm.server_id = ?`,
    )
    .bind(
      `server_join:${userId}:${serverId}:${joinedAt}`,
      joinedAt,
      userId,
      serverId,
    );
}

export function recordServerJoin(
  db: D1Database,
  userId: string,
  serverId: string,
  joinedAt: string,
): Promise<void> {
  return runActivityWrite(
    db,
    () => [serverJoinStatement(db, userId, serverId, joinedAt)],
    "server join",
  );
}

// ── Retention ──────────────────────────────────────────────────────────────

/** The oldest `started_at` kept under `days` of retention at `now`. */
export function retentionCutoff(days: number, now: number): string {
  return iso(now - days * DAY_MS);
}

/**
 * The daily prune: deletes every event older than its owner's retention
 * (no prefs row is the default, 30 days). Returns the rows deleted.
 */
export async function pruneProfileActivity(
  db: D1Database,
  now: number = Date.now(),
): Promise<number> {
  const statements = ACTIVITY_RETENTION_DAYS.map((days) =>
    db
      .prepare(
        days === DEFAULT_ACTIVITY_RETENTION
          ? `DELETE FROM profile_activity WHERE started_at < ? AND user_id NOT IN (
               SELECT user_id FROM profile_prefs WHERE activity_retention_days != ?)`
          : `DELETE FROM profile_activity WHERE started_at < ? AND user_id IN (
               SELECT user_id FROM profile_prefs WHERE activity_retention_days = ?)`,
      )
      .bind(retentionCutoff(days, now), days),
  );
  const results = await db.batch(statements);
  return results.reduce((sum, result) => sum + (result.meta?.changes ?? 0), 0);
}

// ── Reads ──────────────────────────────────────────────────────────────────

export const ACTIVITY_PAGE_SIZE = 20;

interface ActivityRow {
  id: string;
  kind: string;
  ref: string | null;
  data: string | null;
  started_at: string;
  ended_at: string | null;
  seconds: number | null;
  hidden: number;
}

function parseData(value: string | null): ActivityEventData {
  if (!value) return {};
  try {
    const parsed = JSON.parse(value) as unknown;
    if (!parsed || typeof parsed !== "object") return {};
    const raw = parsed as Record<string, unknown>;
    const out: ActivityEventData = {};
    const text = (key: keyof ActivityEventData) => {
      const field = raw[key];
      if (typeof field === "string" && field) {
        (out as Record<string, unknown>)[key] = field;
      }
    };
    text("name");
    text("channel_id");
    text("channel_name");
    text("server_id");
    text("server_name");
    text("top_track");
    if (typeof raw.icon_url === "string") out.icon_url = raw.icon_url;
    if (typeof raw.tracks === "number") out.tracks = raw.tracks;
    return out;
  } catch {
    return {};
  }
}

export function encodeActivityCursor(row: {
  started_at: string;
  id: string;
}): string {
  return btoa(`${row.started_at}|${row.id}`)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

export function decodeActivityCursor(
  cursor: string | null | undefined,
): { started_at: string; id: string } | null {
  if (!cursor || cursor.length > 600) return null;
  try {
    const text = atob(cursor.replace(/-/g, "+").replace(/_/g, "/"));
    const bar = text.indexOf("|");
    if (bar <= 0) return null;
    const startedAt = text.slice(0, bar);
    if (!Number.isFinite(Date.parse(startedAt))) return null;
    return { started_at: startedAt, id: text.slice(bar + 1) };
  } catch {
    return null;
  }
}

/**
 * Whether `viewerId` is in `ownerId`'s presence audience: they share a
 * server, a friendship or a DM, as for game activity on presence
 * (realtime/meeting-room/game-activity.ts AUDIENCE_QUERIES). A DM counts
 * only once the owner has written in it: anyone can open a DM with anyone,
 * so an empty one would let a stranger in.
 */
export async function inPresenceAudience(
  db: D1Database,
  ownerId: string,
  viewerId: string,
): Promise<boolean> {
  if (ownerId === viewerId) return true;
  const row = await db
    .prepare(
      `SELECT 1 AS shared WHERE
         EXISTS (SELECT 1 FROM server_members mine
                   JOIN server_members other ON other.server_id = mine.server_id
                  WHERE mine.user_id = ? AND other.user_id = ?)
         OR EXISTS (SELECT 1 FROM relationships
                     WHERE user_id = ? AND target_user_id = ? AND type = 0)
         OR EXISTS (SELECT 1 FROM dm_recipients mine
                      JOIN dm_recipients other ON other.channel_id = mine.channel_id
                     WHERE mine.user_id = ? AND other.user_id = ?
                       AND EXISTS (SELECT 1 FROM messages m
                                    WHERE m.channel_id = mine.channel_id
                                      AND m.author_id = mine.user_id))`,
    )
    .bind(ownerId, viewerId, ownerId, viewerId, ownerId, viewerId)
    .first<{ shared: number }>();
  return row?.shared === 1;
}

/** Servers among `serverIds` that `viewerId` is a member of. */
async function viewerServers(
  db: D1Database,
  viewerId: string,
  serverIds: string[],
): Promise<Set<string>> {
  const unique = [...new Set(serverIds)].slice(0, 100);
  if (unique.length === 0) return new Set();
  const { results } = await db
    .prepare(
      `SELECT server_id FROM server_members WHERE user_id = ? AND server_id IN (${unique.map(() => "?").join(",")})`,
    )
    .bind(viewerId, ...unique)
    .all<{ server_id: string }>();
  return new Set((results ?? []).map((row) => row.server_id));
}

/** Drops where a call was when the viewer is not in that server. */
export function redactForViewer(
  data: ActivityEventData,
  viewerServerIds: Set<string>,
): ActivityEventData {
  if (!data.channel_id) return data;
  if (data.server_id && viewerServerIds.has(data.server_id)) return data;
  // A DM call names nobody's channel anyway; a server the viewer is not in
  // is "a private server".
  const { channel_id, channel_name, server_id, server_name, ...rest } = data;
  void channel_id;
  void channel_name;
  void server_id;
  void server_name;
  return rest;
}

export interface ListActivityInput {
  ownerId: string;
  viewerId: string;
  cursor?: string | null;
  kind?: ActivityKind | null;
  limit?: number;
  now?: number;
}

/**
 * One page of `ownerId`'s timeline as `viewerId` may see it, newest first.
 * Events past the owner's retention are left out even before the cron
 * deletes them. Callers check the presence audience (inPresenceAudience).
 */
export async function listProfileActivity(
  db: D1Database,
  input: ListActivityInput,
): Promise<ActivityPage> {
  const now = input.now ?? Date.now();
  const isOwner = input.ownerId === input.viewerId;
  const prefs = await getProfilePrefs(db, input.ownerId);
  const settings: ActivitySettings = prefs.activity;
  const limit = Math.max(1, Math.min(50, input.limit ?? ACTIVITY_PAGE_SIZE));
  const cursor = decodeActivityCursor(input.cursor);

  const where = ["user_id = ?", "started_at >= ?"];
  const params: unknown[] = [
    input.ownerId,
    retentionCutoff(settings.retention_days, now),
  ];
  if (!isOwner) {
    where.push("hidden = 0");
    // Calls outside a server (DM and group DM calls) are the owner's alone.
    where.push(
      "NOT (kind IN ('voice', 'stream', 'listen') AND json_extract(data, '$.server_id') IS NULL)",
    );
    if (settings.hidden_kinds.length > 0) {
      where.push(
        `kind NOT IN (${settings.hidden_kinds.map(() => "?").join(",")})`,
      );
      params.push(...settings.hidden_kinds);
    }
  }
  if (input.kind && isActivityKind(input.kind)) {
    where.push("kind = ?");
    params.push(input.kind);
  }
  if (cursor) {
    where.push("(started_at < ? OR (started_at = ? AND id < ?))");
    params.push(cursor.started_at, cursor.started_at, cursor.id);
  }
  const { results } = await db
    .prepare(
      `SELECT id, kind, ref, data, started_at, ended_at, seconds, hidden
       FROM profile_activity WHERE ${where.join(" AND ")}
       ORDER BY started_at DESC, id DESC LIMIT ?`,
    )
    .bind(...params, limit + 1)
    .all<ActivityRow>();
  const rows = (results ?? []).filter((row) => isActivityKind(row.kind));
  const page = rows.slice(0, limit);
  const parsed = page.map((row) => ({ row, data: parseData(row.data) }));
  const memberOf = isOwner
    ? null
    : await viewerServers(
        db,
        input.viewerId,
        parsed.flatMap(({ data }) => (data.server_id ? [data.server_id] : [])),
      );

  const events: ActivityEvent[] = parsed.map(({ row, data }) => ({
    id: row.id,
    kind: row.kind as ActivityKind,
    ref: row.ref,
    data: memberOf ? redactForViewer(data, memberOf) : data,
    started_at: row.started_at,
    ended_at: row.ended_at,
    seconds: row.seconds,
    hidden: row.hidden === 1,
  }));
  const last = page.at(-1);
  return {
    events,
    next_cursor:
      rows.length > limit && last ? encodeActivityCursor(last) : null,
    is_owner: isOwner,
    ...(isOwner ? { settings } : {}),
  };
}

/** The owner hides or shows one event. False when it is not theirs. */
export async function setActivityEventHidden(
  db: D1Database,
  ownerId: string,
  eventId: string,
  hidden: boolean,
): Promise<boolean> {
  const result = await db
    .prepare(
      `UPDATE profile_activity SET hidden = ? WHERE id = ? AND user_id = ?`,
    )
    .bind(hidden ? 1 : 0, eventId, ownerId)
    .run();
  return (result.meta?.changes ?? 0) > 0;
}
