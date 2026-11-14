/**
 * Notification Settings Service — per-server, per-channel and per-DM
 * notification levels and mutes (rules in src/lib/notification-levels.ts).
 *
 * All functions accept a D1Database and throw ServiceError for error cases.
 */

import type { D1Database } from "@cloudflare/workers-types";

import { resolveChannelAccess } from "@/lib/channel-access";
import {
  type EffectiveNotificationSettings,
  isNotificationLevel,
  MUTE_DURATION_MS,
  type MuteDuration,
  type NotificationLevel,
  type NotificationSetting,
  type NotificationTargetType,
  resolveNotificationSettings,
} from "@/lib/notification-levels";
import { ServiceError } from "@/lib/service-error";

const USER_IDS_PER_CHUNK = 90;

function rowToSetting(row: Record<string, unknown>): NotificationSetting {
  return {
    target_type: row.target_type as NotificationTargetType,
    target_id: row.target_id as string,
    level: isNotificationLevel(row.level) ? row.level : null,
    muted: Number(row.muted) === 1,
    muted_until: (row.muted_until as string | null) ?? null,
  };
}

// ─── Reads ───────────────────────────────────────────────────────────────────

/** Every setting the user stored, timed mutes that ran out included. */
export async function listNotificationSettings(
  db: D1Database,
  userId: string,
): Promise<NotificationSetting[]> {
  const { results } = await db
    .prepare(
      `SELECT target_type, target_id, level, muted, muted_until
       FROM notification_settings WHERE user_id = ?`,
    )
    .bind(userId)
    .all();
  return (results ?? []).map(rowToSetting);
}

/**
 * The effective settings in one channel for each of `userIds`: each user's
 * channel row, then their server row (none for a DM).
 */
export async function effectiveSettingsForUsers(
  db: D1Database,
  channel: { id: string; serverId: string | null; isDm: boolean },
  userIds: readonly string[],
  nowMs: number = Date.now(),
): Promise<Map<string, EffectiveNotificationSettings>> {
  const unique = [...new Set(userIds)];
  const channelRows = new Map<string, NotificationSetting>();
  const serverRows = new Map<string, NotificationSetting>();
  for (let offset = 0; offset < unique.length; offset += USER_IDS_PER_CHUNK) {
    const chunk = unique.slice(offset, offset + USER_IDS_PER_CHUNK);
    const placeholders = chunk.map(() => "?").join(",");
    const serverClause =
      channel.serverId && !channel.isDm
        ? " OR (target_type = 'server' AND target_id = ?)"
        : "";
    const binds: unknown[] = [channel.id];
    if (serverClause) binds.push(channel.serverId);
    const { results } = await db
      .prepare(
        `SELECT user_id, target_type, target_id, level, muted, muted_until
         FROM notification_settings
         WHERE ((target_type = 'channel' AND target_id = ?)${serverClause})
           AND user_id IN (${placeholders})`,
      )
      .bind(...binds, ...chunk)
      .all();
    for (const row of results ?? []) {
      const setting = rowToSetting(row);
      const target =
        setting.target_type === "channel" ? channelRows : serverRows;
      target.set(row.user_id as string, setting);
    }
  }
  const effective = new Map<string, EffectiveNotificationSettings>();
  for (const userId of unique) {
    effective.set(
      userId,
      resolveNotificationSettings({
        isDm: channel.isDm,
        channel: channelRows.get(userId),
        server: serverRows.get(userId),
        nowMs,
      }),
    );
  }
  return effective;
}

/**
 * Users who chose "all messages" for a server channel (on the channel, or on
 * its server with no channel level of their own) and are not muted there.
 * Server membership and channel visibility are the caller's to check.
 */
export async function usersWantingAllMessages(
  db: D1Database,
  channelId: string,
  serverId: string,
  nowMs: number = Date.now(),
): Promise<string[]> {
  const { results } = await db
    .prepare(
      `SELECT DISTINCT ns.user_id FROM notification_settings ns
       WHERE ns.level = 'all'
         AND ((ns.target_type = 'channel' AND ns.target_id = ?)
           OR (ns.target_type = 'server' AND ns.target_id = ?
               AND NOT EXISTS (
                 SELECT 1 FROM notification_settings c
                 WHERE c.user_id = ns.user_id AND c.target_type = 'channel'
                   AND c.target_id = ? AND c.level IS NOT NULL)))`,
    )
    .bind(channelId, serverId, channelId)
    .all();
  const candidates = (results ?? []).map((row) => row.user_id as string);
  if (candidates.length === 0) return [];
  const effective = await effectiveSettingsForUsers(
    db,
    { id: channelId, serverId, isDm: false },
    candidates,
    nowMs,
  );
  return candidates.filter((userId) => {
    const settings = effective.get(userId);
    return settings?.level === "all" && !settings.muted;
  });
}

// ─── Writes ──────────────────────────────────────────────────────────────────

export interface NotificationSettingUpdate {
  /** Absent: keep; null: inherit. */
  level?: NotificationLevel | null;
  /** Absent: keep; "off": unmute. */
  mute?: MuteDuration | "off";
}

/**
 * Checks the user may hold a setting for the target: a member of the server,
 * or able to see the channel (a DM's recipient).
 */
async function assertTargetAccess(
  db: D1Database,
  userId: string,
  targetType: NotificationTargetType,
  targetId: string,
): Promise<void> {
  if (targetType === "server") {
    const member = await db
      .prepare(
        `SELECT 1 FROM server_members WHERE server_id = ? AND user_id = ? LIMIT 1`,
      )
      .bind(targetId, userId)
      .first();
    if (!member) throw ServiceError.notFound("Server not found");
    return;
  }
  const access = await resolveChannelAccess(db, userId, targetId);
  if (!access) throw ServiceError.notFound("Channel not found");
}

/**
 * Applies `update` to the user's setting for the target and returns the
 * result. A setting left with no level and no mute is deleted (and returned
 * as the inherited default: level null, not muted).
 */
export async function updateNotificationSetting(
  db: D1Database,
  userId: string,
  targetType: NotificationTargetType,
  targetId: string,
  update: NotificationSettingUpdate,
  nowMs: number = Date.now(),
): Promise<NotificationSetting> {
  await assertTargetAccess(db, userId, targetType, targetId);

  const current = (await db
    .prepare(
      `SELECT target_type, target_id, level, muted, muted_until
       FROM notification_settings
       WHERE user_id = ? AND target_type = ? AND target_id = ?`,
    )
    .bind(userId, targetType, targetId)
    .first()) as Record<string, unknown> | null;
  const before: NotificationSetting = current
    ? rowToSetting(current)
    : {
        target_type: targetType,
        target_id: targetId,
        level: null,
        muted: false,
        muted_until: null,
      };

  const next: NotificationSetting = { ...before };
  if (update.level !== undefined) next.level = update.level;
  if (update.mute === "off") {
    next.muted = false;
    next.muted_until = null;
  } else if (update.mute === "forever") {
    next.muted = true;
    next.muted_until = null;
  } else if (update.mute !== undefined) {
    next.muted = true;
    next.muted_until = new Date(
      nowMs + MUTE_DURATION_MS[update.mute],
    ).toISOString();
  }

  if (next.level === null && !next.muted) {
    await db
      .prepare(
        `DELETE FROM notification_settings
         WHERE user_id = ? AND target_type = ? AND target_id = ?`,
      )
      .bind(userId, targetType, targetId)
      .run();
    return next;
  }

  await db
    .prepare(
      `INSERT INTO notification_settings
         (user_id, target_type, target_id, level, muted, muted_until, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(user_id, target_type, target_id) DO UPDATE SET
         level = excluded.level,
         muted = excluded.muted,
         muted_until = excluded.muted_until,
         updated_at = excluded.updated_at`,
    )
    .bind(
      userId,
      targetType,
      targetId,
      next.level,
      next.muted ? 1 : 0,
      next.muted_until,
      new Date(nowMs).toISOString(),
    )
    .run();
  return next;
}
