/**
 * Profile Board Service: reads and saves `profile_boards`, and derives what
 * the widgets show (play time, voice hours, badges, servers, guestbook).
 *
 * Saves are whole-document with an optimistic version: the client sends
 * the version it started from, and a save against a newer row gets a 409
 * with the server's copy, never a silent overwrite. Every save goes through
 * `parseBoard`, then the server fills or checks what only it can know: the
 * pinned message's snapshot (the owner's own server message), servers the
 * owner is really in, badges that exist.
 *
 * The Custom widget is the paid one (`customWidgets`). It is stored for
 * anyone, so a free user can draft it and a lapsed plan loses nothing, and
 * `boardForViewer` keeps it from every other viewer while the owner's plan
 * does not cover it.
 *
 * The Deadlock widget is free, but others only get it when there is
 * something to show: not linked, no matches and deleted-at-source are
 * the owner's alone (`getBoardView` drops the plate for everyone else).
 */

import type { D1Database } from "@cloudflare/workers-types";

import { BADGE_IDS } from "@/lib/badges";
import { isOwnerOnlyDeadlockState } from "@/lib/deadlock";
import type { Entitlement } from "@/lib/entitlements";
import {
  boardAssets,
  boardBytes,
  boardForViewer,
  EMPTY_BOARD,
  GUESTBOOK_PREVIEW,
  MAX_BOARD_BYTES,
  parseBoard,
  widgetAssetPrefix,
  type AnyProfileWidget,
  type BoardData,
  type BoardResponse,
  type BoardSaveResponse,
  type DroppedWidget,
  type ProfileBoard,
  type QuoteSnapshot,
  type TopGamesRange,
  type VoiceHoursPeriod,
  type WidgetType,
} from "@/lib/profile-board";
import { ServiceError } from "@/lib/service-error";
import { deadlockBoardData } from "@/services/deadlock/board";
import { getProfileExtras } from "@/services/entitlement.service";
import { retentionCutoff } from "@/services/profile-activity.service";
import {
  eitherBlocked,
  listGuestbook,
} from "@/services/profile-guestbook.service";
import { getProfilePrefs } from "@/services/profile-prefs.service";

const DAY_MS = 86_400_000;

export interface StoredBoard {
  board: ProfileBoard;
  version: number;
  updated_at: string | null;
}

export async function getStoredBoard(
  db: D1Database,
  userId: string,
): Promise<StoredBoard> {
  const row = await db
    .prepare(
      `SELECT board, version, updated_at FROM profile_boards WHERE user_id = ?`,
    )
    .bind(userId)
    .first<{ board: string; version: number; updated_at: string }>();
  if (!row) return { board: EMPTY_BOARD, version: 0, updated_at: null };
  return {
    board: parseBoard(row.board).board,
    version: row.version,
    updated_at: row.updated_at,
  };
}

// ── Saving ─────────────────────────────────────────────────────────────────

export type SaveBoardResult =
  | ({ ok: true } & BoardSaveResponse)
  | { ok: false; conflict: StoredBoard };

/**
 * The owner's pinned message: their own message in a server channel. DM
 * messages are refused (a DM is nobody else's to show).
 */
async function resolveQuote(
  db: D1Database,
  ownerId: string,
  messageId: string,
): Promise<QuoteSnapshot | null> {
  const row = await db
    .prepare(
      `SELECT m.content, m.created_at, m.channel_id, c.name AS channel_name,
              c.channel_type, c.server_id, s.name AS server_name
       FROM messages m
       JOIN channels c ON c.id = m.channel_id
       LEFT JOIN servers s ON s.id = c.server_id
       WHERE m.id = ? AND m.author_id = ?`,
    )
    .bind(messageId, ownerId)
    .first<{
      content: string;
      created_at: string;
      channel_id: string;
      channel_name: string | null;
      channel_type: string;
      server_id: string | null;
      server_name: string | null;
    }>();
  if (!row || !row.server_id || row.channel_type === "dm") return null;
  const parsed = parseBoard({
    widgets: [
      {
        id: "quote",
        type: "quote",
        size: "L",
        cfg: {
          message_id: messageId,
          snapshot: {
            text: row.content,
            at: row.created_at,
            channel_id: row.channel_id,
            channel_name: row.channel_name,
            server_id: row.server_id,
            server_name: row.server_name,
          },
        },
      },
    ],
  }).board.widgets[0];
  return parsed?.type === "quote" ? parsed.cfg.snapshot : null;
}

/**
 * Fills and checks what only the server knows. A pinned message is
 * re-resolved only when its id changed (the snapshot is the point: an
 * edited or deleted message keeps showing as it was pinned).
 */
async function settleBoard(
  db: D1Database,
  ownerId: string,
  board: ProfileBoard,
  previous: ProfileBoard,
): Promise<ProfileBoard> {
  const widgets: AnyProfileWidget[] = [];
  let memberships: Set<string> | null = null;
  for (const widget of board.widgets) {
    if (widget.type === "quote") {
      const messageId = widget.cfg.message_id;
      const before = previous.widgets.find(
        (old) =>
          old.type === "quote" &&
          old.cfg.message_id === messageId &&
          old.cfg.snapshot,
      );
      const snapshot = !messageId
        ? null
        : before && before.type === "quote"
          ? before.cfg.snapshot
          : await resolveQuote(db, ownerId, messageId);
      widgets.push({
        ...widget,
        cfg: { message_id: snapshot ? messageId : null, snapshot },
      });
    } else if (widget.type === "servers") {
      if (!memberships) {
        const { results } = await db
          .prepare(`SELECT server_id FROM server_members WHERE user_id = ?`)
          .bind(ownerId)
          .all<{ server_id: string }>();
        memberships = new Set((results ?? []).map((row) => row.server_id));
      }
      const mine = memberships;
      widgets.push({
        ...widget,
        cfg: {
          server_ids: widget.cfg.server_ids.filter((id) => mine.has(id)),
        },
      });
    } else if (widget.type === "badges") {
      widgets.push({
        ...widget,
        cfg: {
          badge_ids: widget.cfg.badge_ids.filter((id) =>
            (BADGE_IDS as readonly string[]).includes(id),
          ),
        },
      });
    } else {
      widgets.push(widget);
    }
  }
  return { v: 1, widgets };
}

/**
 * Saves the owner's board if `baseVersion` is still current. The body is
 * cleaned (`parseBoard`), checked against the 8 KB ceiling and settled
 * (`settleBoard`). On a stale version, nothing is written and the result
 * carries the server's copy.
 */
export async function saveBoard(
  db: D1Database,
  input: {
    ownerId: string;
    board: unknown;
    baseVersion: number;
    entitlement: Entitlement;
    now?: number;
  },
): Promise<SaveBoardResult> {
  const now = input.now ?? Date.now();
  const current = await getStoredBoard(db, input.ownerId);
  if (current.version !== input.baseVersion) {
    return { ok: false, conflict: current };
  }
  const { board: parsed, dropped } = parseBoard(input.board);
  const board = await settleBoard(db, input.ownerId, parsed, current.board);
  if (boardBytes(board) > MAX_BOARD_BYTES) {
    throw new ServiceError(
      "That board is too big to save. Remove a widget or shorten some text.",
      413,
      "BOARD_TOO_LARGE",
    );
  }
  const updatedAt = new Date(now).toISOString();
  const json = JSON.stringify(board);
  const result =
    input.baseVersion === 0
      ? await db
          .prepare(
            `INSERT OR IGNORE INTO profile_boards (user_id, board, version, updated_at)
             SELECT id, ?, 1, ? FROM users WHERE id = ?`,
          )
          .bind(json, updatedAt, input.ownerId)
          .run()
      : await db
          .prepare(
            `UPDATE profile_boards SET board = ?, version = version + 1, updated_at = ?
             WHERE user_id = ? AND version = ?`,
          )
          .bind(json, updatedAt, input.ownerId, input.baseVersion)
          .run();
  if ((result.meta?.changes ?? 0) === 0) {
    // Someone saved between the read and the write.
    return { ok: false, conflict: await getStoredBoard(db, input.ownerId) };
  }
  const { drafts } = boardForViewer(board, {
    isOwner: true,
    ownerEntitlement: input.entitlement,
    now,
  });
  return {
    ok: true,
    board,
    version: input.baseVersion + 1,
    drafts,
    dropped: dropped satisfies DroppedWidget[],
    updated_at: updatedAt,
  };
}

/**
 * Deletes the owner's widget images that no saved board references and
 * that are older than a day (a younger one may belong to a draft that is
 * not saved yet). Best effort; returns the keys deleted.
 */
export async function cleanupWidgetAssets(
  bucket: R2Bucket,
  ownerId: string,
  board: ProfileBoard,
  now: number = Date.now(),
): Promise<string[]> {
  const keep = boardAssets(board);
  const prefix = widgetAssetPrefix(ownerId);
  const listed = await bucket.list({ prefix, limit: 200 });
  const stale = listed.objects
    .filter((object) => {
      const name = object.key.slice(prefix.length);
      return (
        !keep.has(name) && now - new Date(object.uploaded).getTime() > DAY_MS
      );
    })
    .map((object) => object.key);
  if (stale.length > 0) await bucket.delete(stale);
  return stale;
}

// ── Reading, with the widgets' data ────────────────────────────────────────

function rangeStartDay(range: TopGamesRange, now: number): string | null {
  if (range === "all") return null;
  const days = range === "7d" ? 7 : 30;
  return new Date(now - (days - 1) * DAY_MS).toISOString().slice(0, 10);
}

async function topGames(
  db: D1Database,
  ownerId: string,
  range: TopGamesRange,
  now: number,
): Promise<NonNullable<BoardData["top_games"]>> {
  const start = rangeStartDay(range, now);
  const { results } = await db
    .prepare(
      `SELECT game_key, MAX(name) AS name, MAX(icon_url) AS icon_url, SUM(seconds) AS seconds
       FROM game_sessions WHERE user_id = ?${start ? " AND day >= ?" : ""}
       GROUP BY game_key ORDER BY seconds DESC`,
    )
    .bind(...(start ? [ownerId, start] : [ownerId]))
    .all<{ name: string; icon_url: string | null; seconds: number }>();
  const rows = results ?? [];
  return {
    range,
    games: rows.slice(0, 5).map((row) => ({
      name: row.name,
      icon_url: row.icon_url,
      seconds: row.seconds,
    })),
    total_seconds: rows.reduce((sum, row) => sum + row.seconds, 0),
    game_count: rows.length,
  };
}

async function gameTotals(
  db: D1Database,
  ownerId: string,
  now: number,
): Promise<Record<string, { week: number; all: number }>> {
  const weekStart = new Date(now - 6 * DAY_MS).toISOString().slice(0, 10);
  const { results } = await db
    .prepare(
      `SELECT game_key, SUM(seconds) AS total,
              SUM(CASE WHEN day >= ? THEN seconds ELSE 0 END) AS week,
              MAX(day) AS last_day
       FROM game_sessions WHERE user_id = ?
       GROUP BY game_key ORDER BY last_day DESC LIMIT 20`,
    )
    .bind(weekStart, ownerId)
    .all<{ game_key: string; total: number; week: number }>();
  const totals: Record<string, { week: number; all: number }> = {};
  for (const row of results ?? []) {
    totals[row.game_key] = { week: row.week, all: row.total };
  }
  return totals;
}

function utcDay(at: number): string {
  return new Date(at).toISOString().slice(0, 10);
}

async function voiceHours(
  db: D1Database,
  ownerId: string,
  period: VoiceHoursPeriod,
  now: number,
  showLongest: boolean,
): Promise<NonNullable<BoardData["voice_hours"]>> {
  const today = new Date(now);
  let start: Date;
  let previousStart: Date;
  if (period === "month") {
    start = new Date(Date.UTC(today.getUTCFullYear(), today.getUTCMonth(), 1));
    previousStart = new Date(
      Date.UTC(today.getUTCFullYear(), today.getUTCMonth() - 1, 1),
    );
  } else {
    start = new Date(now - 29 * DAY_MS);
    previousStart = new Date(start.getTime() - 30 * DAY_MS);
  }
  const startDay = utcDay(start.getTime());
  const previousDay = utcDay(previousStart.getTime());
  const [rows, previous, longest] = await Promise.all([
    db
      .prepare(
        `SELECT day, voice_seconds, stream_seconds FROM daily_activity
         WHERE user_id = ? AND day >= ? ORDER BY day`,
      )
      .bind(ownerId, startDay)
      .all<{ day: string; voice_seconds: number; stream_seconds: number }>(),
    db
      .prepare(
        `SELECT COALESCE(SUM(voice_seconds), 0) AS total FROM daily_activity
         WHERE user_id = ? AND day >= ? AND day < ?`,
      )
      .bind(ownerId, previousDay, startDay)
      .first<{ total: number }>(),
    showLongest
      ? db
          .prepare(
            // Calls outside a server (DMs) never count: the board is public.
            `SELECT MAX(seconds) AS longest FROM profile_activity
             WHERE user_id = ? AND kind = 'voice' AND hidden = 0 AND started_at >= ?
               AND json_extract(data, '$.server_id') IS NOT NULL`,
          )
          .bind(ownerId, start.toISOString())
          .first<{ longest: number | null }>()
      : Promise.resolve(null),
  ]);
  const dayCount =
    Math.floor((Date.parse(utcDay(now)) - start.getTime()) / DAY_MS) + 1;
  const days = Array.from({ length: Math.max(1, dayCount) }, () => 0);
  let total = 0;
  let stream = 0;
  for (const row of rows.results ?? []) {
    const index = Math.floor((Date.parse(row.day) - start.getTime()) / DAY_MS);
    if (index >= 0 && index < days.length) days[index] = row.voice_seconds;
    total += row.voice_seconds;
    stream += row.stream_seconds;
  }
  return {
    period,
    start_day: startDay,
    days,
    total_seconds: total,
    previous_seconds: previous?.total ?? 0,
    stream_seconds: stream,
    longest_seconds: longest?.longest ?? 0,
  };
}

async function boardServers(
  db: D1Database,
  serverIds: string[],
  ownerId: string,
  viewerId: string,
): Promise<NonNullable<BoardData["servers"]>> {
  if (serverIds.length === 0) return [];
  const placeholders = serverIds.map(() => "?").join(",");
  const { results } = await db
    .prepare(
      `SELECT s.id, s.name, s.icon_url,
              (SELECT COUNT(*) FROM server_members m WHERE m.server_id = s.id) AS member_count,
              EXISTS (SELECT 1 FROM server_members v WHERE v.server_id = s.id AND v.user_id = ?) AS viewer_is_member
       FROM servers s
       JOIN server_members o ON o.server_id = s.id AND o.user_id = ?
       WHERE s.id IN (${placeholders})`,
    )
    .bind(viewerId, ownerId, ...serverIds)
    .all<{
      id: string;
      name: string;
      icon_url: string | null;
      member_count: number;
      viewer_is_member: number;
    }>();
  const byId = new Map((results ?? []).map((row) => [row.id, row]));
  return serverIds.flatMap((id) => {
    const row = byId.get(id);
    return row
      ? [
          {
            id: row.id,
            name: row.name,
            icon_url: row.icon_url,
            member_count: row.member_count,
            viewer_is_member: row.viewer_is_member === 1,
          },
        ]
      : [];
  });
}

/**
 * The data every widget type on `board` needs, as `viewerId` may see it.
 * Activity-backed numbers (last game, last listen, longest call) follow the
 * owner's Activity privacy for other viewers.
 */
export async function boardData(
  db: D1Database,
  input: {
    ownerId: string;
    viewerId: string;
    board: ProfileBoard;
    badges: Array<{ id: string; awarded_at: string }>;
    now: number;
    /** A Deadlock account whose plate is old enough to fetch again. */
    onDeadlockRefresh?(accountId: number): void;
  },
): Promise<BoardData> {
  const { ownerId, viewerId, board, now } = input;
  const isOwner = ownerId === viewerId;
  const types = new Set<WidgetType>(board.widgets.map((widget) => widget.type));
  const prefs = await getProfilePrefs(db, ownerId);
  const showKind = (kind: "game" | "voice" | "listen") =>
    isOwner || !prefs.activity.hidden_kinds.includes(kind);
  const cutoff = retentionCutoff(prefs.activity.retention_days, now);
  const data: BoardData = {};
  const jobs: Array<Promise<void>> = [];

  for (const widget of board.widgets) {
    if (widget.type === "top_games") {
      jobs.push(
        topGames(db, ownerId, widget.cfg.range, now).then((value) => {
          data.top_games = value;
        }),
      );
    } else if (widget.type === "voice_hours") {
      jobs.push(
        voiceHours(db, ownerId, widget.cfg.period, now, showKind("voice")).then(
          (value) => {
            data.voice_hours = value;
          },
        ),
      );
    } else if (widget.type === "servers") {
      jobs.push(
        boardServers(db, widget.cfg.server_ids, ownerId, viewerId).then(
          (value) => {
            data.servers = value;
          },
        ),
      );
    } else if (widget.type === "quote" && widget.cfg.snapshot) {
      const serverId = widget.cfg.snapshot.server_id;
      jobs.push(
        (isOwner || !serverId
          ? Promise.resolve(isOwner)
          : db
              .prepare(
                `SELECT 1 AS hit FROM server_members WHERE server_id = ? AND user_id = ?`,
              )
              .bind(serverId, viewerId)
              .first<{ hit: number }>()
              .then(Boolean)
        ).then((visible) => {
          data.quote_visible = visible;
        }),
      );
    }
  }

  if (types.has("now_playing")) {
    jobs.push(
      (async () => {
        const [last, totals] = await Promise.all([
          showKind("game")
            ? db
                .prepare(
                  `SELECT data, ended_at, seconds FROM profile_activity
                   WHERE user_id = ? AND kind = 'game' AND started_at >= ?${isOwner ? "" : " AND hidden = 0"}
                   ORDER BY started_at DESC LIMIT 1`,
                )
                .bind(ownerId, cutoff)
                .first<{
                  data: string;
                  ended_at: string | null;
                  seconds: number | null;
                }>()
            : Promise.resolve(null),
          gameTotals(db, ownerId, now),
        ]);
        if (last) {
          let parsed: { name?: unknown; icon_url?: unknown } = {};
          try {
            parsed = JSON.parse(last.data) as typeof parsed;
          } catch {
            parsed = {};
          }
          data.last_game =
            typeof parsed.name === "string"
              ? {
                  name: parsed.name,
                  icon_url:
                    typeof parsed.icon_url === "string"
                      ? parsed.icon_url
                      : null,
                  ended_at: last.ended_at ?? "",
                  seconds: last.seconds ?? 0,
                }
              : null;
        } else {
          data.last_game = null;
        }
        data.game_totals = totals;
      })(),
    );
  }

  // The owner always gets the Deadlock state (cheap: D1 only), so a plate
  // just added in the editor knows whether Steam is linked.
  const deadlockWidget = board.widgets.find(
    (widget) => widget.type === "deadlock",
  );
  if (deadlockWidget || isOwner) {
    jobs.push(
      deadlockBoardData(db, {
        ownerId,
        isOwner,
        range:
          deadlockWidget?.type === "deadlock"
            ? deadlockWidget.cfg.range
            : "30d",
        track: !!deadlockWidget,
        now,
      })
        .then((view) => {
          data.deadlock = view.data;
          if (view.refresh !== null) input.onDeadlockRefresh?.(view.refresh);
        })
        // D1 trouble here costs the plate, never the board.
        .catch(() => {
          delete data.deadlock;
        }),
    );
  }

  if (types.has("listening")) {
    jobs.push(
      (async () => {
        if (!showKind("listen")) {
          data.last_listen = null;
          return;
        }
        const row = await db
          .prepare(
            `SELECT data, ended_at FROM profile_activity
             WHERE user_id = ? AND kind = 'listen' AND started_at >= ?${isOwner ? "" : " AND hidden = 0 AND json_extract(data, '$.server_id') IS NOT NULL"}
             ORDER BY started_at DESC LIMIT 1`,
          )
          .bind(ownerId, cutoff)
          .first<{ data: string; ended_at: string | null }>();
        if (!row) {
          data.last_listen = null;
          return;
        }
        let parsed: { tracks?: unknown; top_track?: unknown } = {};
        try {
          parsed = JSON.parse(row.data) as typeof parsed;
        } catch {
          parsed = {};
        }
        data.last_listen = {
          tracks: typeof parsed.tracks === "number" ? parsed.tracks : 0,
          top_track:
            typeof parsed.top_track === "string" ? parsed.top_track : null,
          ended_at: row.ended_at ?? "",
        };
      })(),
    );
  }

  if (types.has("badges")) {
    data.badges = [...input.badges].sort((a, b) =>
      b.awarded_at.localeCompare(a.awarded_at),
    );
  }

  if (types.has("guestbook")) {
    jobs.push(
      listGuestbook(db, { ownerId, viewerId, limit: GUESTBOOK_PREVIEW }).then(
        (page) => {
          data.guestbook = {
            enabled: page.enabled,
            notes: page.notes,
            total: page.total,
            can_post: page.can_post,
            denial: page.denial,
          };
        },
      ),
    );
  }

  await Promise.all(jobs);
  return data;
}

/**
 * `ownerId`'s board as `viewerId` sees it: filtered (`boardForViewer`) and
 * with its widgets' data. Someone either side blocked gets a 403.
 */
export async function getBoardView(
  db: D1Database,
  input: {
    ownerId: string;
    viewerId: string;
    now?: number;
    onDeadlockRefresh?(accountId: number): void;
  },
): Promise<BoardResponse> {
  const now = input.now ?? Date.now();
  const isOwner = input.ownerId === input.viewerId;
  const user = await db
    .prepare(`SELECT id FROM users WHERE id = ?`)
    .bind(input.ownerId)
    .first<{ id: string }>();
  if (!user) throw ServiceError.notFound("User not found");
  if (!isOwner && (await eitherBlocked(db, input.ownerId, input.viewerId))) {
    throw new ServiceError("You can't see this profile.", 403, "BLOCKED");
  }
  const [stored, extras] = await Promise.all([
    getStoredBoard(db, input.ownerId),
    getProfileExtras(db, input.ownerId),
  ]);
  const viewed = boardForViewer(stored.board, {
    isOwner,
    ownerEntitlement: extras.entitlement,
    now,
  });
  const { drafts } = viewed;
  let { board } = viewed;
  const data = await boardData(db, {
    ownerId: input.ownerId,
    viewerId: input.viewerId,
    board,
    badges: extras.badges,
    now,
    onDeadlockRefresh: input.onDeadlockRefresh,
  });
  // Nothing to show (not linked, no matches, deleted at source): the
  // plate is the owner's alone.
  if (
    !isOwner &&
    board.widgets.some((widget) => widget.type === "deadlock") &&
    (!data.deadlock || isOwnerOnlyDeadlockState(data.deadlock.state))
  ) {
    board = {
      v: 1,
      widgets: board.widgets.filter((widget) => widget.type !== "deadlock"),
    };
    delete data.deadlock;
  }
  return {
    user_id: input.ownerId,
    board,
    version: stored.version,
    drafts,
    is_owner: isOwner,
    data,
    updated_at: stored.updated_at,
  };
}
