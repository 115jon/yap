/**
 * Stats Service: the `user_stats` writes. One batch per event (a message
 * written, a voice session ended), run beside the write that caused it and
 * never inside its transaction: callers hand `record*` to `waitUntil`, so a
 * stat that fails cannot fail the message or the call.
 */

import type {
  D1Database,
  D1PreparedStatement,
} from "@cloudflare/workers-types";

import { isNightOwlMoment } from "@/lib/badges";
import { utcOffsetMinutesIn } from "@/lib/request-time-zone";
import { nightOwlAward, voiceRegularCheck } from "@/services/badge.service";

/**
 * One message written by `userId` at `now`: messages_sent + 1, and Night
 * owl when it is 3 a.m. in `timeZone` (Cloudflare's, from the request;
 * request-time-zone.ts), else in UTC.
 */
export function messageSentStatements(
  db: D1Database,
  userId: string,
  now: Date,
  timeZone?: string | null,
): D1PreparedStatement[] {
  const at = now.toISOString();
  const statements = [
    db
      .prepare(
        `INSERT INTO user_stats (user_id, messages_sent, first_seen_at, last_active_at, updated_at)
         VALUES (?, 1, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
           messages_sent = user_stats.messages_sent + 1,
           first_seen_at = COALESCE(user_stats.first_seen_at, excluded.first_seen_at),
           last_active_at = excluded.last_active_at,
           updated_at = excluded.updated_at`,
      )
      .bind(userId, at, at, at),
  ];
  if (isNightOwlMoment(now, utcOffsetMinutesIn(timeZone, now))) {
    statements.push(nightOwlAward(db, userId, at));
  }
  return statements;
}

/**
 * One voice session of `userId` ended: add its voice and stream seconds
 * (already capped by the caller), then check Voice regular.
 */
export function voiceSessionStatements(
  db: D1Database,
  userId: string,
  {
    voiceSeconds,
    streamSeconds,
    endedAt,
  }: { voiceSeconds: number; streamSeconds: number; endedAt: Date },
): D1PreparedStatement[] {
  const at = endedAt.toISOString();
  return [
    db
      .prepare(
        `INSERT INTO user_stats (user_id, voice_seconds, stream_seconds, first_seen_at, last_active_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(user_id) DO UPDATE SET
           voice_seconds = user_stats.voice_seconds + excluded.voice_seconds,
           stream_seconds = user_stats.stream_seconds + excluded.stream_seconds,
           first_seen_at = COALESCE(user_stats.first_seen_at, excluded.first_seen_at),
           last_active_at = excluded.last_active_at,
           updated_at = excluded.updated_at`,
      )
      .bind(userId, voiceSeconds, streamSeconds, at, at, at),
    voiceRegularCheck(db, userId, at),
  ];
}

/**
 * Writes `statements` off the request's critical path: a stat that fails to
 * land is logged and dropped, never an error for the message or the call
 * that caused it.
 */
async function runStatWrite(
  db: D1Database,
  statements: () => D1PreparedStatement[],
  what: string,
): Promise<void> {
  try {
    await db.batch(statements());
  } catch (error) {
    console.warn(
      `user_stats ${what} write failed: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

export function recordMessageSent(
  db: D1Database,
  userId: string,
  now: Date,
  timeZone?: string | null,
): Promise<void> {
  return runStatWrite(
    db,
    () => messageSentStatements(db, userId, now, timeZone),
    "message",
  );
}

export function recordVoiceSession(
  db: D1Database,
  userId: string,
  session: { voiceSeconds: number; streamSeconds: number; endedAt: Date },
): Promise<void> {
  if (session.voiceSeconds <= 0 && session.streamSeconds <= 0) {
    return Promise.resolve();
  }
  return runStatWrite(
    db,
    () => voiceSessionStatements(db, userId, session),
    "voice session",
  );
}

/** `recordMessageSent`, handed to the request's `waitUntil`. */
export function scheduleMessageSent(
  waitUntil: (promise: Promise<unknown>) => void,
  db: D1Database,
  userId: string,
  now: Date,
  timeZone?: string | null,
): void {
  waitUntil(recordMessageSent(db, userId, now, timeZone));
}

/**
 * A legacy account claimed by a new id: its plan, badges and stats move onto
 * `toId`, merged with whatever `toId` already has, then the legacy rows go.
 * - plan: the target keeps its own row, except that a founder grant wins
 * - badges: the union, each with its earliest date
 * - stats: counters summed, the earliest first_seen, the latest last_active
 * Batch these after `toId`'s users row exists and before the legacy row is
 * deleted.
 */
export function mergeLegacyAccountStatements(
  db: D1Database,
  fromId: string,
  toId: string,
  now: string = new Date().toISOString(),
): D1PreparedStatement[] {
  return [
    db
      .prepare(
        `INSERT INTO user_entitlements
           (user_id, tier, source, started_at, expires_at, stripe_customer_id, stripe_subscription_id, updated_at)
         SELECT ?, tier, source, started_at, expires_at, stripe_customer_id, stripe_subscription_id, ?
         FROM user_entitlements WHERE user_id = ?
         ON CONFLICT(user_id) DO UPDATE SET
           tier = excluded.tier,
           source = excluded.source,
           started_at = excluded.started_at,
           expires_at = excluded.expires_at,
           updated_at = excluded.updated_at
         WHERE excluded.tier = 'founder' AND user_entitlements.tier <> 'founder'`,
      )
      .bind(toId, now, fromId),
    db
      .prepare(
        `INSERT INTO user_badges (user_id, badge_id, awarded_at, source)
         SELECT ?, badge_id, awarded_at, source FROM user_badges WHERE user_id = ?
         ON CONFLICT(user_id, badge_id) DO UPDATE SET
           awarded_at = min(user_badges.awarded_at, excluded.awarded_at)`,
      )
      .bind(toId, fromId),
    db
      .prepare(
        `INSERT INTO user_stats
           (user_id, voice_seconds, stream_seconds, messages_sent, first_seen_at, last_active_at, updated_at)
         SELECT ?, voice_seconds, stream_seconds, messages_sent, first_seen_at, last_active_at, ?
         FROM user_stats WHERE user_id = ?
         ON CONFLICT(user_id) DO UPDATE SET
           voice_seconds = user_stats.voice_seconds + excluded.voice_seconds,
           stream_seconds = user_stats.stream_seconds + excluded.stream_seconds,
           messages_sent = user_stats.messages_sent + excluded.messages_sent,
           first_seen_at = COALESCE(min(user_stats.first_seen_at, excluded.first_seen_at), user_stats.first_seen_at, excluded.first_seen_at),
           last_active_at = COALESCE(max(user_stats.last_active_at, excluded.last_active_at), user_stats.last_active_at, excluded.last_active_at),
           updated_at = excluded.updated_at`,
      )
      .bind(toId, now, fromId),
    db.prepare(`DELETE FROM user_entitlements WHERE user_id = ?`).bind(fromId),
    db.prepare(`DELETE FROM user_badges WHERE user_id = ?`).bind(fromId),
    db.prepare(`DELETE FROM user_stats WHERE user_id = ?`).bind(fromId),
  ];
}
