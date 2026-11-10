/**
 * Badge Service: awards in `user_badges`. Every write is idempotent (the
 * primary key is user + badge, inserts are INSERT OR IGNORE), so a sweep, a
 * retry or two gateways racing never award a badge twice or move its date.
 *
 * Automatic badges are checked two ways: cheap per-user checks batched with
 * the stat change that can earn them (`voiceRegularCheck`, `nightOwlAward`),
 * and the daily sweep (`sweepAutoBadges`) that catches the rest (rank and
 * server size change without any stat write).
 */

import type {
  D1Database,
  D1PreparedStatement,
} from "@cloudflare/workers-types";

import {
  EARLY_YAPPER_RANK,
  isBadgeId,
  isManualBadge,
  SERVER_OWNER_MEMBERS,
  VOICE_REGULAR_SECONDS,
  type BadgeAwardSource,
  type BadgeId,
} from "@/lib/badges";
import { badgeActivityStatement } from "@/services/profile-activity.service";

/** Awards `badgeId` unless the user already has it. True when it was new. */
export async function awardBadge(
  db: D1Database,
  userId: string,
  badgeId: BadgeId,
  source: BadgeAwardSource,
  now: string = new Date().toISOString(),
): Promise<boolean> {
  const result = await db
    .prepare(
      `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
       SELECT id, ?, ?, ? FROM users WHERE id = ?`,
    )
    .bind(badgeId, now, source, userId)
    .run();
  const awarded = (result.meta?.changes ?? 0) > 0;
  if (awarded) await recordBadgeActivity(db, now, userId);
  return awarded;
}

/**
 * The profile Activity events for awards made at `awardedAt` (all users,
 * or one). A separate, best-effort write after the award: a timeline that
 * fails to update never costs the badge.
 */
export async function recordBadgeActivity(
  db: D1Database,
  awardedAt: string,
  userId?: string,
): Promise<void> {
  try {
    await badgeActivityStatement(db, awardedAt, userId).run();
  } catch (error) {
    console.warn(
      `profile_activity badge write failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

export async function revokeBadge(
  db: D1Database,
  userId: string,
  badgeId: BadgeId,
): Promise<boolean> {
  const result = await db
    .prepare(`DELETE FROM user_badges WHERE user_id = ? AND badge_id = ?`)
    .bind(userId, badgeId)
    .run();
  return (result.meta?.changes ?? 0) > 0;
}

// ── Admin awards ────────────────────────────────────────────────────────────

export type AdminBadgeAction = "award" | "revoke";

export type AdminBadgeResult =
  | { ok: true; changed: boolean }
  | {
      ok: false;
      status: 400 | 403 | 404;
      code:
        | "BADGES_FORBIDDEN"
        | "BADGE_UNKNOWN"
        | "BADGE_NOT_MANUAL"
        | "USER_NOT_FOUND";
      message: string;
    };

/**
 * An admin awarding or revoking a manual badge (Bug hunter, Yap team).
 * `adminIds` is the admin gate (COLLECTIBLES_ADMIN_USER_IDS); an actor not
 * in it is refused before anything is read.
 */
export async function setManualBadge(
  db: D1Database,
  input: {
    actorId: string;
    adminIds: ReadonlySet<string>;
    targetUserId: string;
    badgeId: string;
    action: AdminBadgeAction;
    now?: string;
  },
): Promise<AdminBadgeResult> {
  if (!input.adminIds.has(input.actorId)) {
    return {
      ok: false,
      status: 403,
      code: "BADGES_FORBIDDEN",
      message: "Only admins can award badges.",
    };
  }
  if (!isBadgeId(input.badgeId)) {
    return {
      ok: false,
      status: 400,
      code: "BADGE_UNKNOWN",
      message: "There is no badge with that id.",
    };
  }
  if (!isManualBadge(input.badgeId)) {
    return {
      ok: false,
      status: 400,
      code: "BADGE_NOT_MANUAL",
      message: "That badge is awarded automatically, not by hand.",
    };
  }
  const user = await db
    .prepare(`SELECT id FROM users WHERE id = ?`)
    .bind(input.targetUserId)
    .first<{ id: string }>();
  if (!user) {
    return {
      ok: false,
      status: 404,
      code: "USER_NOT_FOUND",
      message: "User not found.",
    };
  }
  const changed =
    input.action === "award"
      ? await awardBadge(
          db,
          input.targetUserId,
          input.badgeId,
          "admin",
          input.now,
        )
      : await revokeBadge(db, input.targetUserId, input.badgeId);
  return { ok: true, changed };
}

// ── Automatic awards ───────────────────────────────────────────────────────

/** Voice regular for `userId`, if their stats reached it (batch after the
 * stats write). */
export function voiceRegularCheck(
  db: D1Database,
  userId: string,
  now: string,
): D1PreparedStatement {
  return db
    .prepare(
      `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
       SELECT user_id, 'voice-regular', ?, 'auto' FROM user_stats
       WHERE user_id = ? AND voice_seconds >= ?`,
    )
    .bind(now, userId, VOICE_REGULAR_SECONDS);
}

/** Night owl for `userId` (the caller decided the moment qualifies). */
export function nightOwlAward(
  db: D1Database,
  userId: string,
  now: string,
): D1PreparedStatement {
  return db
    .prepare(
      `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
       SELECT id, 'night-owl', ?, 'auto' FROM users WHERE id = ?`,
    )
    .bind(now, userId);
}

export interface SweepResult {
  "early-yapper": number;
  "server-owner": number;
  "voice-regular": number;
}

/**
 * The daily sweep: every stat rule over every user, as set-based inserts.
 * Idempotent; returns how many awards each rule added this run.
 */
export async function sweepAutoBadges(
  db: D1Database,
  now: string = new Date().toISOString(),
): Promise<SweepResult> {
  const [early, owner, voice] = await db.batch([
    // The first 100 accounts by creation (ties broken by id, so the set is
    // stable from run to run).
    db
      .prepare(
        `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
         SELECT id, 'early-yapper', ?, 'auto' FROM (
           SELECT id FROM users ORDER BY created_at ASC, id ASC LIMIT ?
         )`,
      )
      .bind(now, EARLY_YAPPER_RANK),
    db
      .prepare(
        `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
         SELECT DISTINCT s.owner_id, 'server-owner', ?, 'auto'
         FROM servers s
         JOIN users u ON u.id = s.owner_id
         WHERE (SELECT COUNT(*) FROM server_members sm WHERE sm.server_id = s.id) >= ?`,
      )
      .bind(now, SERVER_OWNER_MEMBERS),
    db
      .prepare(
        `INSERT OR IGNORE INTO user_badges (user_id, badge_id, awarded_at, source)
         SELECT user_id, 'voice-regular', ?, 'auto' FROM user_stats
         WHERE voice_seconds >= ?`,
      )
      .bind(now, VOICE_REGULAR_SECONDS),
  ]);
  await recordBadgeActivity(db, now);
  return {
    "early-yapper": early?.meta?.changes ?? 0,
    "server-owner": owner?.meta?.changes ?? 0,
    "voice-regular": voice?.meta?.changes ?? 0,
  };
}
