/**
 * Notification Service — pure business logic for notification operations.
 *
 * All functions accept a D1Database, return plain objects, and throw ServiceError
 * for error cases.
 */

import { ServiceError } from "@/lib/service-error";
import type { AvatarDisplay } from "@/lib/avatar-display";
import { visibleChannelIds } from "@/lib/channel-access";
import type { D1Database } from "@cloudflare/workers-types";

// ─── listNotifications ───────────────────────────────────────────────────────

export interface ListNotificationsOptions {
  limit: number;
  unreadOnly: boolean;
}

export interface Notification {
  id: unknown;
  type: unknown;
  channel_id: unknown;
  server_id: unknown;
  message_id: unknown;
  from_user: {
    id: unknown;
    username: string;
    display_name: string | null;
    avatar_url: unknown;
    avatar_display: AvatarDisplay | string | null;
  };
  content: unknown;
  is_read: boolean;
  created_at: unknown;
  channel_name: unknown;
  server_name: unknown;
}

// A stored notification keeps its snippet after a block between the two
// users (either way) or after the recipient loses the channel (left the
// server, a role or override took it away). The list leaves those out, and
// so does the unread count. The other side of every block the user is in
// is read once per query (two binds of the user id), not once per row.
const NOT_BLOCKED = `(n.from_user_id IS NULL OR n.from_user_id NOT IN (
  SELECT target_user_id FROM relationships WHERE type = 1 AND user_id = ?
  UNION
  SELECT user_id FROM relationships WHERE type = 1 AND target_user_id = ?
))`;

// No channel (or an empty one) has nothing to check; otherwise the channel
// must be in the JSON array of channels the user can read.
const IN_VISIBLE_CHANNEL = `(n.channel_id IS NULL OR n.channel_id = ''
  OR n.channel_id IN (SELECT value FROM json_each(?)))`;

export async function listNotifications(
  db: D1Database,
  userId: string,
  opts: ListNotificationsOptions,
): Promise<{ notifications: Notification[]; unread_count: number }> {
  const cappedLimit = Math.max(1, Math.min(opts.limit || 50, 100));

  const unreadClause = opts.unreadOnly ? "AND n.is_read = 0" : "";

  // Every channel the user has a notification in (an unread one, for the
  // unread list, which is all the count needs too), with its unread count;
  // the access check runs once over these, so the page below can filter in
  // SQL and its LIMIT counts only rows the user may see.
  const { results: byChannel } = await db
    .prepare(
      `SELECT n.channel_id AS channel_id,
              SUM(CASE WHEN n.is_read = 0 THEN 1 ELSE 0 END) AS unread
         FROM notifications n
        WHERE n.user_id = ? ${unreadClause} AND ${NOT_BLOCKED}
        GROUP BY n.channel_id`,
    )
    .bind(userId, userId, userId)
    .all<{ channel_id: string | null; unread: number | null }>();

  const channelOf = (value: unknown) =>
    typeof value === "string" && value.length > 0 ? value : null;
  const visible = await visibleChannelIds(
    db,
    userId,
    (byChannel ?? [])
      .map((r) => channelOf(r.channel_id))
      .filter((id): id is string => id !== null),
  );
  const shown = (channelId: unknown) => {
    const id = channelOf(channelId);
    return id === null || visible.has(id);
  };
  const unreadCount = (byChannel ?? []).reduce(
    (sum, r) => (shown(r.channel_id) ? sum + Number(r.unread ?? 0) : sum),
    0,
  );

  const { results } = await db
    .prepare(
      `SELECT n.*,
              u.username as from_username, u.display_name as from_display_name, u.avatar_url as from_avatar_url, u.avatar_display as from_avatar_display,
              CASE
                WHEN c.channel_type = 'dm' THEN COALESCE(NULLIF(TRIM(u.display_name), ''), u.username)
                ELSE c.name
              END as channel_name,
              s.name as server_name
       FROM notifications n
       LEFT JOIN users u ON u.id = n.from_user_id
       LEFT JOIN channels c ON c.id = n.channel_id
       LEFT JOIN servers s ON s.id = n.server_id
       WHERE n.user_id = ? ${unreadClause}
         AND ${NOT_BLOCKED}
         AND ${IN_VISIBLE_CHANNEL}
       ORDER BY n.created_at DESC
       LIMIT ?`,
    )
    .bind(userId, userId, userId, JSON.stringify([...visible]), cappedLimit)
    .all();

  const notifications: Notification[] = (results ?? []).map(
    (r: Record<string, unknown>) => ({
      id: r.id,
      type: r.type,
      channel_id: r.channel_id,
      server_id: r.server_id,
      message_id: r.message_id,
      from_user: {
        id: r.from_user_id,
        username: (r.from_username as string) ?? "Unknown",
        display_name: (r.from_display_name as string) ?? null,
        avatar_url: r.from_avatar_url,
        avatar_display:
          (r.from_avatar_display as AvatarDisplay | string | null) ?? null,
      },
      content: r.content,
      is_read: !!r.is_read,
      created_at: r.created_at,
      channel_name: r.channel_name,
      server_name: r.server_name,
    }),
  );

  return {
    notifications,
    unread_count: unreadCount,
  };
}

// ─── markNotificationsRead ───────────────────────────────────────────────────

export interface MarkReadInput {
  ids?: string[];
  all?: boolean;
}

export async function markNotificationsRead(
  db: D1Database,
  userId: string,
  input: MarkReadInput,
): Promise<void> {
  if (input.all) {
    await db
      .prepare(
        `UPDATE notifications SET is_read = 1 WHERE user_id = ? AND is_read = 0`,
      )
      .bind(userId)
      .run();
    return;
  }

  if (input.ids && input.ids.length > 0) {
    const placeholders = input.ids.map(() => "?").join(",");
    await db
      .prepare(
        `UPDATE notifications SET is_read = 1 WHERE user_id = ? AND id IN (${placeholders})`,
      )
      .bind(userId, ...input.ids)
      .run();
    return;
  }

  throw ServiceError.badRequest("Provide ids array or all: true");
}

// ─── clearNotifications ──────────────────────────────────────────────────────

export async function clearNotifications(
  db: D1Database,
  userId: string,
): Promise<string> {
  const clearedAt = new Date().toISOString();
  await db.batch([
    db.prepare(`DELETE FROM notifications WHERE user_id = ?`).bind(userId),
    db
      .prepare(
        `INSERT INTO notification_clear_watermarks (user_id, cleared_at)
         VALUES (?, ?)
         ON CONFLICT(user_id) DO UPDATE SET cleared_at = excluded.cleared_at`,
      )
      .bind(userId, clearedAt),
  ]);
  return clearedAt;
}
