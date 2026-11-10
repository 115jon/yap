/**
 * Guestbook: short notes on someone's profile. Notes post immediately.
 *
 * Who may post: anyone who shares a server or a DM with the owner, unless
 * either has blocked the other, the owner blocked them from the guestbook,
 * or the owner turned it off. Each author gets `GUESTBOOK_DAILY_LIMIT` notes
 * per owner per day here (the route adds a per-minute limit on top). Bodies
 * are plain text, cleaned like every other profile text.
 *
 * The owner deletes any note, blocks a poster (which also removes their
 * notes) and turns the guestbook off; an author deletes their own notes.
 */

import type { D1Database } from "@cloudflare/workers-types";

import {
  cleanGuestbookBody,
  GUESTBOOK_DAILY_LIMIT,
  type BoardUserRef,
  type GuestbookNote,
} from "@/lib/profile-board";
import { ServiceError } from "@/lib/service-error";
import { getProfilePrefs } from "@/services/profile-prefs.service";

const DAY_MS = 86_400_000;
export const GUESTBOOK_PAGE_SIZE = 20;

/** Whether either user has blocked the other (relationships type 1). */
export async function eitherBlocked(
  db: D1Database,
  a: string,
  b: string,
): Promise<boolean> {
  const row = await db
    .prepare(
      `SELECT 1 AS hit FROM relationships
       WHERE type = 1 AND ((user_id = ? AND target_user_id = ?) OR (user_id = ? AND target_user_id = ?))
       LIMIT 1`,
    )
    .bind(a, b, b, a)
    .first<{ hit: number }>();
  return Boolean(row);
}

/** Whether `a` and `b` share a server or a DM (or group DM). */
export async function sharesServerOrDm(
  db: D1Database,
  a: string,
  b: string,
): Promise<boolean> {
  const row = await db
    .prepare(
      `SELECT
         EXISTS (SELECT 1 FROM server_members x JOIN server_members y ON y.server_id = x.server_id
                 WHERE x.user_id = ? AND y.user_id = ?)
         OR EXISTS (SELECT 1 FROM dm_recipients x JOIN dm_recipients y ON y.channel_id = x.channel_id
                 WHERE x.user_id = ? AND y.user_id = ?) AS shared`,
    )
    .bind(a, b, a, b)
    .first<{ shared: number }>();
  return row?.shared === 1;
}

export async function isGuestbookBlocked(
  db: D1Database,
  ownerId: string,
  userId: string,
): Promise<boolean> {
  const row = await db
    .prepare(
      `SELECT 1 AS hit FROM profile_guestbook_blocks WHERE owner_id = ? AND blocked_user_id = ?`,
    )
    .bind(ownerId, userId)
    .first<{ hit: number }>();
  return Boolean(row);
}

export type GuestbookDenial =
  | "GUESTBOOK_OWN"
  | "GUESTBOOK_OFF"
  | "GUESTBOOK_BLOCKED"
  | "GUESTBOOK_NO_SHARED_SPACE";

/** Why `authorId` may not post on `ownerId`'s guestbook, or null. */
export async function guestbookDenial(
  db: D1Database,
  ownerId: string,
  authorId: string,
): Promise<GuestbookDenial | null> {
  if (ownerId === authorId) return "GUESTBOOK_OWN";
  const [prefs, blocked, guestbookBlocked, shared] = await Promise.all([
    getProfilePrefs(db, ownerId),
    eitherBlocked(db, ownerId, authorId),
    isGuestbookBlocked(db, ownerId, authorId),
    sharesServerOrDm(db, ownerId, authorId),
  ]);
  if (!prefs.guestbook_enabled) return "GUESTBOOK_OFF";
  if (blocked || guestbookBlocked) return "GUESTBOOK_BLOCKED";
  if (!shared) return "GUESTBOOK_NO_SHARED_SPACE";
  return null;
}

const DENIAL_MESSAGES: Record<GuestbookDenial, string> = {
  GUESTBOOK_OWN: "You can't sign your own guestbook.",
  GUESTBOOK_OFF: "This guestbook is closed.",
  GUESTBOOK_BLOCKED: "You can't sign this guestbook.",
  GUESTBOOK_NO_SHARED_SPACE:
    "Only people who share a server or a DM with them can sign.",
};

interface NoteRow {
  id: string;
  body: string;
  created_at: string;
  author_id: string;
  username: string | null;
  display_name: string | null;
  avatar_url: string | null;
}

function noteFromRow(row: NoteRow): GuestbookNote {
  const author: BoardUserRef = {
    id: row.author_id,
    username: row.username ?? "unknown",
    display_name: row.display_name,
    avatar_url: row.avatar_url,
  };
  return { id: row.id, author, body: row.body, created_at: row.created_at };
}

const SELECT_NOTES = `SELECT g.id, g.body, g.created_at, g.author_id,
    u.username, u.display_name, u.avatar_url
  FROM profile_guestbook g LEFT JOIN users u ON u.id = g.author_id`;

export interface PostNoteInput {
  ownerId: string;
  authorId: string;
  body: unknown;
  id: string;
  now?: number;
}

/** Posts a note. Throws a ServiceError (400/403/404/429) when it can't. */
export async function postGuestbookNote(
  db: D1Database,
  input: PostNoteInput,
): Promise<GuestbookNote> {
  const body = cleanGuestbookBody(input.body);
  if (!body) {
    throw new ServiceError("Write something first.", 400, "GUESTBOOK_EMPTY");
  }
  const owner = await db
    .prepare(`SELECT id FROM users WHERE id = ?`)
    .bind(input.ownerId)
    .first<{ id: string }>();
  if (!owner) throw ServiceError.notFound("User not found");

  const denial = await guestbookDenial(db, input.ownerId, input.authorId);
  if (denial) throw new ServiceError(DENIAL_MESSAGES[denial], 403, denial);

  const now = input.now ?? Date.now();
  const since = new Date(now - DAY_MS).toISOString();
  const today = await db
    .prepare(
      `SELECT COUNT(*) AS count FROM profile_guestbook
       WHERE owner_id = ? AND author_id = ? AND created_at >= ?`,
    )
    .bind(input.ownerId, input.authorId, since)
    .first<{ count: number }>();
  if ((today?.count ?? 0) >= GUESTBOOK_DAILY_LIMIT) {
    throw new ServiceError(
      `That's ${GUESTBOOK_DAILY_LIMIT} notes today. Try again tomorrow.`,
      429,
      "GUESTBOOK_DAILY_LIMIT",
    );
  }

  const createdAt = new Date(now).toISOString();
  await db
    .prepare(
      `INSERT INTO profile_guestbook (id, owner_id, author_id, body, created_at)
       VALUES (?, ?, ?, ?, ?)`,
    )
    .bind(input.id, input.ownerId, input.authorId, body, createdAt)
    .run();
  const row = await db
    .prepare(`${SELECT_NOTES} WHERE g.id = ?`)
    .bind(input.id)
    .first<NoteRow>();
  return row
    ? noteFromRow(row)
    : {
        id: input.id,
        author: {
          id: input.authorId,
          username: "unknown",
          display_name: null,
          avatar_url: null,
        },
        body,
        created_at: createdAt,
      };
}

export interface GuestbookPage {
  notes: GuestbookNote[];
  total: number;
  next_cursor: string | null;
  enabled: boolean;
  can_post: boolean;
  /** Owner only: who is blocked from the guestbook. */
  blocked?: BoardUserRef[];
}

/**
 * Notes on `ownerId`'s guestbook, newest first, as `viewerId` sees them.
 * Someone either side blocked gets a 403, as for the board. A closed
 * guestbook still lists its notes to the owner, and to nobody else.
 */
export async function listGuestbook(
  db: D1Database,
  input: {
    ownerId: string;
    viewerId: string;
    cursor?: string | null;
    limit?: number;
  },
): Promise<GuestbookPage> {
  const isOwner = input.ownerId === input.viewerId;
  if (!isOwner && (await eitherBlocked(db, input.ownerId, input.viewerId))) {
    throw new ServiceError("You can't see this guestbook.", 403, "BLOCKED");
  }
  const prefs = await getProfilePrefs(db, input.ownerId);
  const limit = Math.max(1, Math.min(50, input.limit ?? GUESTBOOK_PAGE_SIZE));
  const denial = isOwner
    ? "GUESTBOOK_OWN"
    : await guestbookDenial(db, input.ownerId, input.viewerId);
  if (!isOwner && !prefs.guestbook_enabled) {
    return {
      notes: [],
      total: 0,
      next_cursor: null,
      enabled: false,
      can_post: false,
    };
  }

  const cursor = parseNoteCursor(input.cursor);
  const where = ["g.owner_id = ?"];
  const params: unknown[] = [input.ownerId];
  if (cursor) {
    where.push("(g.created_at < ? OR (g.created_at = ? AND g.id < ?))");
    params.push(cursor.created_at, cursor.created_at, cursor.id);
  }
  const [page, count] = await Promise.all([
    db
      .prepare(
        `${SELECT_NOTES} WHERE ${where.join(" AND ")}
         ORDER BY g.created_at DESC, g.id DESC LIMIT ?`,
      )
      .bind(...params, limit + 1)
      .all<NoteRow>(),
    db
      .prepare(
        `SELECT COUNT(*) AS count FROM profile_guestbook WHERE owner_id = ?`,
      )
      .bind(input.ownerId)
      .first<{ count: number }>(),
  ]);
  const rows = page.results ?? [];
  const shown = rows.slice(0, limit);
  const last = shown.at(-1);
  const result: GuestbookPage = {
    notes: shown.map(noteFromRow),
    total: count?.count ?? 0,
    next_cursor:
      rows.length > limit && last ? `${last.created_at}|${last.id}` : null,
    enabled: prefs.guestbook_enabled,
    can_post: denial === null,
  };
  if (isOwner) result.blocked = await listGuestbookBlocks(db, input.ownerId);
  return result;
}

function parseNoteCursor(
  cursor: string | null | undefined,
): { created_at: string; id: string } | null {
  if (!cursor || cursor.length > 300) return null;
  const bar = cursor.indexOf("|");
  if (bar <= 0) return null;
  const createdAt = cursor.slice(0, bar);
  if (!Number.isFinite(Date.parse(createdAt))) return null;
  return { created_at: createdAt, id: cursor.slice(bar + 1) };
}

/**
 * Deletes a note: the guestbook's owner may delete any, an author their
 * own. False when there is no such note for this actor.
 */
export async function deleteGuestbookNote(
  db: D1Database,
  input: { ownerId: string; noteId: string; actorId: string },
): Promise<boolean> {
  const result = await db
    .prepare(
      `DELETE FROM profile_guestbook
       WHERE id = ? AND owner_id = ? AND (owner_id = ? OR author_id = ?)`,
    )
    .bind(input.noteId, input.ownerId, input.actorId, input.actorId)
    .run();
  return (result.meta?.changes ?? 0) > 0;
}

/** Blocks `userId` from the owner's guestbook and removes their notes. */
export async function blockFromGuestbook(
  db: D1Database,
  ownerId: string,
  userId: string,
  now: string = new Date().toISOString(),
): Promise<void> {
  if (ownerId === userId) {
    throw new ServiceError("You can't block yourself.", 400, "GUESTBOOK_OWN");
  }
  const user = await db
    .prepare(`SELECT id FROM users WHERE id = ?`)
    .bind(userId)
    .first<{ id: string }>();
  if (!user) throw ServiceError.notFound("User not found");
  await db.batch([
    db
      .prepare(
        `INSERT OR IGNORE INTO profile_guestbook_blocks (owner_id, blocked_user_id, created_at)
         VALUES (?, ?, ?)`,
      )
      .bind(ownerId, userId, now),
    db
      .prepare(
        `DELETE FROM profile_guestbook WHERE owner_id = ? AND author_id = ?`,
      )
      .bind(ownerId, userId),
  ]);
}

export async function unblockFromGuestbook(
  db: D1Database,
  ownerId: string,
  userId: string,
): Promise<boolean> {
  const result = await db
    .prepare(
      `DELETE FROM profile_guestbook_blocks WHERE owner_id = ? AND blocked_user_id = ?`,
    )
    .bind(ownerId, userId)
    .run();
  return (result.meta?.changes ?? 0) > 0;
}

export async function listGuestbookBlocks(
  db: D1Database,
  ownerId: string,
): Promise<BoardUserRef[]> {
  const { results } = await db
    .prepare(
      `SELECT u.id, u.username, u.display_name, u.avatar_url
       FROM profile_guestbook_blocks b JOIN users u ON u.id = b.blocked_user_id
       WHERE b.owner_id = ? ORDER BY b.created_at DESC LIMIT 200`,
    )
    .bind(ownerId)
    .all<BoardUserRef>();
  return results ?? [];
}
