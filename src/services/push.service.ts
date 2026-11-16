/**
 * Push Service — device push tokens and message pushes over FCM, and the
 * notification text a device fetches for a push (pushes carry ids only).
 *
 * Token functions accept a D1Database. `sendMessagePushes` and
 * `sendReadPushes` never throw: a push is best effort and must not fail (or
 * retry) the work that made it.
 */

import type { D1Database } from "@cloudflare/workers-types";

import { resolveChannelAccessForUsers } from "@/lib/channel-access";
import { shouldNotify } from "@/lib/notification-levels";
import type { FcmMessage, FcmSender } from "@/server/fcm";
import { blockBetween } from "@/services/block.service";
import { effectiveSettingsForUsers } from "@/services/notification-settings.service";

// ─── Tokens ──────────────────────────────────────────────────────────────────

export interface PushTokenInput {
  token: string;
  platform: "android" | "ios";
  appVariant: string | null;
  /** The Kova session registering it; signing that session out deletes it. */
  sessionId: string | null;
}

/**
 * Registers `token` for `userId` and the session registering it, taking it
 * over from any previous owner or session.
 */
export async function upsertPushToken(
  db: D1Database,
  userId: string,
  id: string,
  input: PushTokenInput,
): Promise<void> {
  const now = new Date().toISOString();
  await db
    .prepare(
      `INSERT INTO push_tokens
         (id, user_id, token, platform, app_variant, push_format,
          session_id, created_at, last_seen_at)
       VALUES (?, ?, ?, ?, ?, 'native', ?, ?, ?)
       ON CONFLICT(token) DO UPDATE SET
         user_id = excluded.user_id,
         platform = excluded.platform,
         app_variant = excluded.app_variant,
         push_format = 'native',
         session_id = excluded.session_id,
         last_seen_at = excluded.last_seen_at`,
    )
    .bind(
      id,
      userId,
      input.token,
      input.platform,
      input.appVariant,
      input.sessionId,
      now,
      now,
    )
    .run();
}

/**
 * Unregisters `token`, only if `userId` owns it, and with it every token the
 * signing-out session (`sessionId`) registered, so a token that rotated
 * since goes too.
 */
export async function deletePushToken(
  db: D1Database,
  userId: string,
  token: string,
  sessionId: string | null = null,
): Promise<void> {
  if (!sessionId) {
    await db
      .prepare(`DELETE FROM push_tokens WHERE token = ? AND user_id = ?`)
      .bind(token, userId)
      .run();
    return;
  }
  await db
    .prepare(
      `DELETE FROM push_tokens
       WHERE user_id = ? AND (token = ? OR session_id = ?)`,
    )
    .bind(userId, token, sessionId)
    .run();
}

/**
 * Which of a user's tokens go when sessions are signed out: the tokens one
 * session registered, or those of every session but the caller's own.
 */
export type SignedOutSessions =
  | { readonly sessionId: string }
  | { readonly allExcept: string };

/**
 * Deletes the tokens the signed-out sessions registered, and the user's
 * tokens registered before tokens were bound to sessions (session_id NULL):
 * those may belong to the signed-out device and nothing says which. A live
 * device that loses one registers again on its next start.
 */
export async function deleteSessionPushTokens(
  db: D1Database,
  userId: string,
  sessions: SignedOutSessions,
): Promise<void> {
  if ("sessionId" in sessions) {
    await db
      .prepare(
        `DELETE FROM push_tokens
         WHERE user_id = ? AND (session_id = ? OR session_id IS NULL)`,
      )
      .bind(userId, sessions.sessionId)
      .run();
    return;
  }
  await db
    .prepare(
      `DELETE FROM push_tokens
       WHERE user_id = ? AND (session_id != ? OR session_id IS NULL)`,
    )
    .bind(userId, sessions.allExcept)
    .run();
}

/**
 * Each user's device tokens. Users on Do Not Disturb are left out: they get
 * no message pushes at all.
 */
async function tokensByUser(
  db: D1Database,
  userIds: readonly string[],
): Promise<Map<string, string[]>> {
  const byUser = new Map<string, string[]>();
  for (let offset = 0; offset < userIds.length; offset += 90) {
    const chunk = userIds.slice(offset, offset + 90);
    const { results } = await db
      .prepare(
        `SELECT t.user_id, t.token FROM push_tokens t
         JOIN users u ON u.id = t.user_id
         WHERE t.user_id IN (${chunk.map(() => "?").join(",")})
           AND COALESCE(u.status, 'online') != 'dnd'`,
      )
      .bind(...chunk)
      .all();
    for (const row of results ?? []) {
      const userId = row.user_id as string;
      byUser.set(userId, [...(byUser.get(userId) ?? []), row.token as string]);
    }
  }
  return byUser;
}

async function deleteStaleToken(db: D1Database, token: string): Promise<void> {
  await db.prepare(`DELETE FROM push_tokens WHERE token = ?`).bind(token).run();
}

// ─── Payload ─────────────────────────────────────────────────────────────────
//
// A push names no message text, attachment, person, channel or server: FCM
// (Google) carries it. It holds ids, and the app fetches what it shows
// (`GET /api/notifications/:id/push`, `notificationPushText` below) with
// its own session.

/** Marks a data-only push the app's native FCM service handles. */
const NATIVE_FORMAT = "2";

/** One message push: ids only, so no text can reach FCM. */
export interface MessagePush {
  recipientId: string;
  notificationId: string;
  /** "message": a plain message in a channel set to "all messages". */
  type: "mention" | "reply" | "dm" | "message";
  channelId: string;
  serverId: string | null;
  messageId: string;
  senderId: string;
  /** ISO time the message was sent. */
  sentAt: string;
}

/** The text an older native build draws as-is: the kind, no content. */
const LEGACY_TEXT: Record<MessagePush["type"], string> = {
  dm: "New direct message",
  mention: "New mention",
  reply: "New reply",
  message: "New message",
};

/** The string-valued data of a native push; optional keys are added later. */
interface NativePushData {
  [key: string]: string;
}

/**
 * The data-only FCM message for a device: no `notification` block, so the
 * app renders it (and can group, reply and clear it) after fetching its
 * text by `notificationId`. Null values are left out.
 */
export function buildNativeMessagePush(push: MessagePush): FcmMessage {
  const data: NativePushData = {
    ralph: NATIVE_FORMAT,
    kind: "message",
    type: push.type,
    channelId: push.channelId,
    messageId: push.messageId,
    notificationId: push.notificationId,
    tag: `channel:${push.channelId}`,
    senderId: push.senderId,
    sentAt: push.sentAt,
    // For app builds from before ids-only pushes, which draw these as-is:
    // the kind of message, never its text or a name. Newer builds fetch
    // the real text and ignore both.
    senderName: "Yap",
    preview: LEGACY_TEXT[push.type],
  };
  if (push.serverId) data.serverId = push.serverId;
  return { data, android: { priority: "HIGH", ttl: "86400s" } };
}

// ─── Notification text (fetched by the device) ───────────────────────────────

/** The longest preview: what the push itself carried before. */
export const PREVIEW_MAX = 180;

/** A message's notification line: whitespace folded, at most PREVIEW_MAX. */
export function previewOf(type: MessagePush["type"], content: string): string {
  const text = content.replace(/\s+/g, " ").trim();
  if (!text) {
    if (type === "mention") return "Mentioned you";
    if (type === "reply") return "Replied to you";
    return "Sent an attachment";
  }
  const chars = Array.from(text);
  return chars.length > PREVIEW_MAX
    ? `${chars
        .slice(0, PREVIEW_MAX - 1)
        .join("")
        .trimEnd()}…`
    : text;
}

/** What `GET /api/notifications/:id/push` answers: one notification's text. */
export interface NotificationPushText {
  type: MessagePush["type"];
  channel_id: string;
  server_id: string | null;
  message_id: string;
  sender: {
    id: string;
    name: string;
    /** The stored avatar URL as-is; may be a relative `/api/...` path. */
    avatar_url: string | null;
  };
  /** "#general" for a server channel; null for a DM. */
  conversation: string | null;
  server_name: string | null;
  preview: string;
}

/** The columns both kinds of notification read, from `m` (the message) on. */
const PUSH_TEXT_COLUMNS = `m.id AS message_id, m.channel_id, m.author_id, m.content,
              julianday(m.created_at) <= COALESCE(
                (SELECT julianday(rs.last_read_at) FROM read_states rs
                 WHERE rs.user_id = r.id AND rs.channel_id = m.channel_id), 0)
                AS read_since,
              u.username, u.display_name, u.avatar_url,
              c.name AS channel_name, c.channel_type, c.server_id,
              s.name AS server_name`;
const PUSH_TEXT_JOINS = `JOIN channels c ON c.id = m.channel_id
       LEFT JOIN users u ON u.id = m.author_id
       LEFT JOIN servers s ON s.id = c.server_id`;

/** An "all messages" push's id: `message:<message id>:message:<user id>`. */
const ALL_MESSAGES_ID = /^message:(.+):message:(.+)$/;

/** The row for `notificationId` if it is `userId`'s, with its message. */
async function pushTextRow(
  db: D1Database,
  userId: string,
  notificationId: string,
): Promise<Record<string, unknown> | null> {
  const plain = ALL_MESSAGES_ID.exec(notificationId);
  if (plain) {
    // No inbox row: the delivery marker says the push went to this user.
    if (plain[2] !== userId) return null;
    return db
      .prepare(
        `SELECT 'message' AS type, 0 AS is_read, r.status AS user_status,
                ${PUSH_TEXT_COLUMNS}
         FROM notification_delivery_markers k
         JOIN messages m ON m.id = k.message_id
         JOIN users r ON r.id = ?
         ${PUSH_TEXT_JOINS}
         WHERE k.dedupe_key = ? AND k.notification_id = ?`,
      )
      .bind(userId, `${plain[1]}:message:${userId}`, notificationId)
      .first<Record<string, unknown>>();
  }
  return db
    .prepare(
      `SELECT n.type, n.is_read, r.status AS user_status, ${PUSH_TEXT_COLUMNS}
       FROM notifications n
       JOIN messages m ON m.id = n.message_id AND m.channel_id = n.channel_id
       JOIN users r ON r.id = n.user_id
       ${PUSH_TEXT_JOINS}
       WHERE n.id = ? AND n.user_id = ?`,
    )
    .bind(notificationId, userId)
    .first<Record<string, unknown>>();
}

const PUSH_TYPES: ReadonlySet<string> = new Set<MessagePush["type"]>([
  "mention",
  "reply",
  "dm",
  "message",
]);

/**
 * The text of `userId`'s notification `notificationId`, for the device that
 * got its push, or null when there is nothing the user may see or wants
 * shown: not their notification, the message deleted, the channel no
 * longer visible to them (left the server or the DM, lost View Channels),
 * a block between them and the author either way, the notification or the
 * channel read since, the user on Do Not Disturb, or the channel or server
 * muted (or its level lowered) so it would not notify now. The device asks
 * after the push arrives, so this is the state at that moment, not at send
 * time. A database failure throws (the route answers 500), never null.
 */
export async function notificationPushText(
  db: D1Database,
  userId: string,
  notificationId: string,
): Promise<NotificationPushText | null> {
  const row = await pushTextRow(db, userId, notificationId);
  if (!row) return null;

  const type = row.type as string;
  if (!PUSH_TYPES.has(type)) return null;
  const pushType = type as MessagePush["type"];
  if (Number(row.is_read) === 1 || Number(row.read_since) === 1) return null;
  if (row.user_status === "dnd") return null;
  const channelId = row.channel_id as string;
  const authorId = row.author_id as string;
  const isDm = row.channel_type === "dm" || row.server_id === null;
  const serverId = isDm ? null : (row.server_id as string);
  const [decisions, block, settings] = await Promise.all([
    // The batch form, as it tells a D1 failure apart from "no access":
    // a failure must reach the route as a 500, not read as not found.
    resolveChannelAccessForUsers(db, channelId, [userId]),
    blockBetween(db, userId, authorId),
    effectiveSettingsForUsers(db, { id: channelId, serverId, isDm }, [userId]),
  ]);
  const access = decisions.get(userId);
  if (access?.kind === "error") {
    throw new Error("notificationPushText: channel access lookup failed");
  }
  if (access?.kind !== "allowed" || block) return null;
  // The rule the push was sent by (message.service.ts), on today's settings.
  const effective = settings.get(userId);
  if (
    effective &&
    !shouldNotify(effective, pushType === "mention" || pushType === "reply")
  ) {
    return null;
  }

  const displayName = (row.display_name as string | null)?.trim();
  const channelName = row.channel_name as string | null;
  return {
    type: pushType,
    channel_id: channelId,
    server_id: serverId,
    message_id: row.message_id as string,
    sender: {
      id: authorId,
      name: displayName || (row.username as string | null) || "Someone",
      avatar_url: (row.avatar_url as string | null) ?? null,
    },
    conversation: !isDm && channelName ? `#${channelName}` : null,
    server_name: isDm ? null : ((row.server_name as string | null) ?? null),
    preview: previewOf(pushType, (row.content as string | null) ?? ""),
  };
}

// ─── Fan-out ─────────────────────────────────────────────────────────────────

/**
 * Sends each push to every device its recipient registered and deletes the
 * tokens FCM calls dead. A null sender
 * (push not configured) does nothing.
 */
export async function sendMessagePushes(
  db: D1Database,
  sender: FcmSender | null,
  pushes: readonly MessagePush[],
): Promise<void> {
  if (!sender || pushes.length === 0) return;
  try {
    const tokens = await tokensByUser(db, [
      ...new Set(pushes.map((push) => push.recipientId)),
    ]);
    const sends = pushes.flatMap((push) =>
      (tokens.get(push.recipientId) ?? []).map(async (token) => {
        const result = await sender.send(token, buildNativeMessagePush(push));
        if (result.ok) return;
        if (!result.staleToken) {
          console.warn("push.failed", { status: result.status });
          return;
        }
        await deleteStaleToken(db, token);
      }),
    );
    const outcomes = await Promise.allSettled(sends);
    const rejected = outcomes.filter((o) => o.status === "rejected").length;
    if (rejected > 0) console.warn("push.errored", { count: rejected });
  } catch (error: unknown) {
    console.error("push.fanout_failed", error);
  }
}

// ─── Read elsewhere ──────────────────────────────────────────────────────────

/** Only notifications this recent can still be sitting in a tray. */
const READ_PUSH_LOOKBACK_DAYS = 7;
/** At most one read push per user and channel per this long, per isolate. */
const READ_PUSH_MIN_INTERVAL_MS = 10_000;

/**
 * Whether marking `channelId` read now clears anything: the user has a
 * notification there, or was pushed a message as an "all messages" member
 * (a delivery marker, no notification row), newer than their previous
 * `last_read_at` (or any, with no read state yet). Call it before
 * `markChannelAsRead` moves the mark.
 *
 * `created_at` is ISO text from the app, but defaults and older rows are
 * SQLite's `YYYY-MM-DD HH:MM:SS`; `julianday()` reads both. The lookback and
 * `LIMIT 1` keep it cheap, and `idx_notifications_user` narrows it to the
 * user's rows. The messages side also compares `created_at` as text against
 * a day a little before the lookback (a prefix both formats sort after), so
 * the channel's history index reads only that range, not every message.
 */
export async function hasNotificationsSinceLastRead(
  db: D1Database,
  userId: string,
  channelId: string,
): Promise<boolean> {
  const since = new Date(
    Date.now() - (READ_PUSH_LOOKBACK_DAYS + 1) * 86_400_000,
  )
    .toISOString()
    .slice(0, 10);
  const row = await db
    .prepare(
      `SELECT 1 AS found FROM notifications
       WHERE user_id = ? AND channel_id = ?
         AND julianday(created_at) > julianday('now', '-${READ_PUSH_LOOKBACK_DAYS} days')
         AND julianday(created_at) > COALESCE(
           (SELECT julianday(last_read_at) FROM read_states
            WHERE user_id = ? AND channel_id = ?), 0)
       UNION ALL
       SELECT 1 AS found FROM messages m
       JOIN notification_delivery_markers k
         ON k.dedupe_key = m.id || ':message:' || ?
       WHERE m.channel_id = ? AND m.created_at >= ?
         AND julianday(m.created_at) > julianday('now', '-${READ_PUSH_LOOKBACK_DAYS} days')
         AND julianday(m.created_at) > COALESCE(
           (SELECT julianday(last_read_at) FROM read_states
            WHERE user_id = ? AND channel_id = ?), 0)
       LIMIT 1`,
    )
    .bind(
      userId,
      channelId,
      userId,
      channelId,
      userId,
      channelId,
      since,
      userId,
      channelId,
    )
    .first();
  return row !== null && row !== undefined;
}

const lastReadPushAt = new Map<string, number>();

/** Whether a read push for `key` may go out now; records it if so. */
function takeReadPushSlot(key: string, nowMs: number): boolean {
  const last = lastReadPushAt.get(key);
  if (last !== undefined && nowMs - last < READ_PUSH_MIN_INTERVAL_MS) {
    return false;
  }
  for (const [other, at] of lastReadPushAt) {
    if (nowMs - at >= READ_PUSH_MIN_INTERVAL_MS) lastReadPushAt.delete(other);
  }
  lastReadPushAt.set(key, nowMs);
  return true;
}

/**
 * Tells the user's devices that `channelId` was read (at
 * `readAt`, ISO) so they can clear its tray notifications. Never throws.
 */
export async function sendReadPushes(
  db: D1Database,
  sender: FcmSender | null,
  userId: string,
  channelId: string,
  readAt: string,
  nowMs: number = Date.now(),
): Promise<void> {
  if (!sender) return;
  try {
    if (!takeReadPushSlot(`${userId}:${channelId}`, nowMs)) return;
    const { results } = await db
      .prepare(`SELECT token FROM push_tokens WHERE user_id = ?`)
      .bind(userId)
      .all();
    const message: FcmMessage = {
      data: { ralph: NATIVE_FORMAT, kind: "read", channelId, readAt },
      android: { priority: "NORMAL", ttl: "3600s" },
    };
    const sends = (results ?? []).map(async (row) => {
      const token = row.token as string;
      const result = await sender.send(token, message);
      if (result.ok) return;
      if (!result.staleToken) {
        console.warn("push.failed", { status: result.status });
        return;
      }
      await deleteStaleToken(db, token);
    });
    const outcomes = await Promise.allSettled(sends);
    const rejected = outcomes.filter((o) => o.status === "rejected").length;
    if (rejected > 0) console.warn("push.errored", { count: rejected });
  } catch (error: unknown) {
    console.error("push.read_failed", error);
  }
}
