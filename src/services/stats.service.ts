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
