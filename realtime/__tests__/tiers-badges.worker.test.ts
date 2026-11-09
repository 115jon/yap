// Tiers, badges and stats against a real D1 (workerd): the founder grant in
// migration 039, the profile extras, the gateway's voice and stream seconds
// (one write per session end, capped at 24 h), message counts and Night owl,
// idempotent automatic awards, and the admin gate on manual badges.

import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { describe, expect, it } from "vitest";

import migrationSql from "../../migrations/039_tiers_badges_stats.sql?raw";
import {
  issueSocketTicket,
  verifySocketTicket,
} from "../../src/lib/voice/socket-ticket";
import {
  setManualBadge,
  sweepAutoBadges,
} from "../../src/services/badge.service";
import { getProfileExtras } from "../../src/services/entitlement.service";
import {
  mergeLegacyAccountStatements,
  recordMessageSent,
} from "../../src/services/stats.service";
import { TIME_ZONE_HEADER } from "../../src/lib/request-time-zone";
import {
  appendRealtimeAdmissionHeaders,
  createRealtimeAdmissionContext,
} from "../realtime-admission";

type Frame = { op: number; d: unknown };

const db = env.DB;
const HOUR_MS = 3_600_000;

/** The pre-039 tables the migration and the rules read, trimmed down. */
async function createBaseTables() {
  await db.batch([
    db.prepare(
      `CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT NOT NULL, display_name TEXT, avatar_url TEXT, avatar_display TEXT, created_at TEXT NOT NULL DEFAULT (datetime('now')))`,
    ),
    db.prepare(
      `CREATE TABLE IF NOT EXISTS messages (id TEXT PRIMARY KEY, channel_id TEXT NOT NULL, author_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE, content TEXT NOT NULL, created_at TEXT NOT NULL, type TEXT)`,
    ),
    db.prepare(
      `CREATE TABLE IF NOT EXISTS servers (id TEXT PRIMARY KEY, name TEXT NOT NULL, owner_id TEXT NOT NULL)`,
    ),
    db.prepare(
      `CREATE TABLE IF NOT EXISTS server_members (server_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY (server_id, user_id))`,
    ),
  ]);
}

/** Migration 039, statement by statement (comments stripped). */
async function applyMigration() {
  const statements = migrationSql
    .split("\n")
    .filter((line) => !line.trim().startsWith("--"))
    .join("\n")
    .split(/;\s*\n/)
    .map((statement) => statement.trim().replace(/;$/, ""))
    .filter(Boolean);
  for (const statement of statements) {
    await db.prepare(statement).run();
  }
}

async function setup() {
  await createBaseTables();
  await applyMigration();
}

async function addUser(id: string, createdAt = new Date().toISOString()) {
  await db
    .prepare(`INSERT INTO users (id, username, created_at) VALUES (?, ?, ?)`)
    .bind(id, id, createdAt)
    .run();
}

async function stats(userId: string) {
  return db
    .prepare(
      `SELECT voice_seconds, stream_seconds, messages_sent, first_seen_at FROM user_stats WHERE user_id = ?`,
    )
    .bind(userId)
    .first<{
      voice_seconds: number;
      stream_seconds: number;
      messages_sent: number;
      first_seen_at: string | null;
    }>();
}

async function badges(userId: string) {
  const { results } = await db
    .prepare(
      `SELECT badge_id, awarded_at, source FROM user_badges WHERE user_id = ? ORDER BY badge_id`,
    )
    .bind(userId)
    .all<{ badge_id: string; awarded_at: string; source: string }>();
  return results;
}

/** Polls `read` until `done` (the gateway writes in waitUntil). */
async function eventually<T>(
  read: () => Promise<T>,
  done: (value: T) => boolean,
  timeoutMs = 3000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let value = await read();
  while (!done(value) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 25));
    value = await read();
  }
  return value;
}

describe("migration 039: the founder grant", () => {
  it("makes every existing user a founder with the Founder badge and stats", async () => {
    await createBaseTables();
    const a = `founder-a-${crypto.randomUUID()}`;
    const b = `founder-b-${crypto.randomUUID()}`;
    await addUser(a, "2026-01-02 03:04:05");
    await addUser(b, "2026-05-06T07:08:09.000Z");
    await db.batch([
      db
        .prepare(
          `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES (?, 'c', ?, 'hi', '2026-01-03')`,
        )
        .bind(crypto.randomUUID(), a),
      db
        .prepare(
          `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES (?, 'c', ?, 'yo', '2026-01-04')`,
        )
        .bind(crypto.randomUUID(), a),
      // A call record is the gateway's, not the user's.
      db
        .prepare(
          `INSERT INTO messages (id, channel_id, author_id, content, created_at, type) VALUES (?, 'c', ?, 'Missed call', '2026-01-05', 'call')`,
        )
        .bind(crypto.randomUUID(), a),
    ]);

    await applyMigration();
    // Running it again changes nothing (IF NOT EXISTS, OR IGNORE).
    await applyMigration();

    for (const id of [a, b]) {
      const row = await db
        .prepare(
          `SELECT tier, source, expires_at FROM user_entitlements WHERE user_id = ?`,
        )
        .bind(id)
        .first();
      expect(row).toEqual({
        tier: "founder",
        source: "grant",
        expires_at: null,
      });
      expect(await badges(id)).toEqual([
        expect.objectContaining({ badge_id: "founder", source: "grant" }),
      ]);
    }
    expect(await stats(a)).toMatchObject({
      voice_seconds: 0,
      stream_seconds: 0,
      messages_sent: 2,
      first_seen_at: "2026-01-02 03:04:05",
    });
    expect(await stats(b)).toMatchObject({
      messages_sent: 0,
      first_seen_at: "2026-05-06T07:08:09.000Z",
    });

    const extras = await getProfileExtras(db, a);
    expect(extras.entitlement).toMatchObject({
      tier: "founder",
      source: "grant",
    });
    expect(extras.badges.map((badge) => badge.id)).toEqual(["founder"]);
  });

  it("leaves users who sign up afterwards on the free plan", async () => {
    await setup();
    const late = `late-${crypto.randomUUID()}`;
    await addUser(late);
    const extras = await getProfileExtras(db, late);
    expect(extras.entitlement.tier).toBe("free");
    expect(extras.badges).toEqual([]);
  });
});

// ── The gateway's voice and stream seconds ─────────────────────────────────

function meetingRoom(roomName: string) {
  return env.MEETING_ROOM.get(env.MEETING_ROOM.idFromName(roomName));
}

async function admissionHeaders(
  roomSlug: string,
  subject: string,
  audience: "room" | "global" = "room",
) {
  const ticket = await issueSocketTicket(
    {
      accessMode: "authenticated",
      audience,
      expiresAt: Date.now() + 60_000,
      nonce: crypto.randomUUID(),
      roomSlug,
      subject,
    },
    env.CALLS_APP_SECRET,
  );
  const verification = await verifySocketTicket(ticket, env.CALLS_APP_SECRET, {
    accessMode: "authenticated",
    audience,
    now: Date.now(),
    roomSlug,
  });
  if (!verification.ok) throw new Error("Expected test ticket to verify");
  return appendRealtimeAdmissionHeaders(
    new Headers({
      Upgrade: "websocket",
      "Sec-WebSocket-Protocol": "ralph.realtime.v1",
    }),
    await createRealtimeAdmissionContext(verification.claims),
  );
}

function nextOp(socket: WebSocket, op: number): Promise<Frame> {
  return new Promise((resolve) => {
    const onMessage = (event: MessageEvent) => {
      const frame = JSON.parse(event.data as string) as Frame;
      if (frame.op !== op) return;
      socket.removeEventListener("message", onMessage);
      resolve(frame);
    };
    socket.addEventListener("message", onMessage);
  });
}

/** A signed-in socket in `roomName`'s voice, identified. */
async function joinVoice(roomName: string, userId: string) {
  const response = await meetingRoom(roomName).fetch(
    `https://internal/api/channels/${roomName}/ws?v=1`,
    { headers: await admissionHeaders(roomName, userId) },
  );
  expect(response.status).toBe(101);
  const socket = response.webSocket!;
  const hello = nextOp(socket, 8);
  socket.accept();
  await hello;
  const ready = nextOp(socket, 2);
  socket.send(JSON.stringify({ op: 0, d: { name: userId } }));
  const { d } = await ready;
  return {
    socket,
    participantId: (d as { participant_id: string }).participant_id,
  };
}

type StatSession = {
  id: string;
  voice_joined_at?: number;
  stream_started_at?: number;
  self_stream: boolean;
};

/** Moves this session's clocks back, as if it had been in voice a while. */
function backdate(
  roomName: string,
  participantId: string,
  joinedAgoMs: number,
  streamAgoMs?: number,
) {
  return runInDurableObject(meetingRoom(roomName), (instance) => {
    const sessions = (
      instance as unknown as { sessions: Map<unknown, StatSession> }
    ).sessions;
    for (const session of sessions.values()) {
      if (session.id !== participantId) continue;
      session.voice_joined_at = Date.now() - joinedAgoMs;
      if (streamAgoMs !== undefined) {
        session.stream_started_at = Date.now() - streamAgoMs;
      }
    }
  });
}

async function leave(socket: WebSocket) {
  const closed = new Promise((resolve) =>
    socket.addEventListener("close", resolve, { once: true }),
  );
  socket.send(JSON.stringify({ op: 11, d: {} }));
  await closed;
}

describe("MeetingRoom voice stats", () => {
  it("adds a session's voice and stream time once, capped at 24 h", async () => {
    await setup();
    const userId = `voice-${crypto.randomUUID()}`;
    await addUser(userId);
    const roomName = crypto.randomUUID();

    const { socket, participantId } = await joinVoice(roomName, userId);
    // Start a share, then pretend the session began 30 h ago and the share
    // two hours ago.
    socket.send(JSON.stringify({ op: 15, d: { self_stream: true } }));
    await eventually(
      () =>
        runInDurableObject(meetingRoom(roomName), (instance) =>
          [
            ...(
              instance as unknown as { sessions: Map<unknown, StatSession> }
            ).sessions.values(),
          ].some((session) => session.stream_started_at !== undefined),
        ),
      Boolean,
    );
    await backdate(roomName, participantId, 30 * HOUR_MS, 2 * HOUR_MS);
    await leave(socket);

    const after = await eventually(
      () => stats(userId),
      (row) => (row?.voice_seconds ?? 0) > 0,
    );
    expect(after?.voice_seconds).toBe(86_400);
    expect(after?.stream_seconds).toBeGreaterThanOrEqual(7_200);
    expect(after?.stream_seconds).toBeLessThan(7_210);

    // A second, short session adds to the first.
    const second = await joinVoice(roomName, userId);
    await backdate(roomName, second.participantId, 10 * 60_000);
    await leave(second.socket);
    const total = await eventually(
      () => stats(userId),
      (row) => (row?.voice_seconds ?? 0) > 86_400,
    );
    expect(total?.voice_seconds).toBeGreaterThanOrEqual(86_400 + 600);
    expect(total?.voice_seconds).toBeLessThan(86_400 + 610);
    expect(total?.stream_seconds).toBe(after?.stream_seconds);
  });

  it("awards Voice regular when a session crosses 100 hours", async () => {
    await setup();
    const userId = `regular-${crypto.randomUUID()}`;
    await addUser(userId);
    await db
      .prepare(
        `INSERT INTO user_stats (user_id, voice_seconds, updated_at) VALUES (?, ?, ?)`,
      )
      .bind(userId, 360_000 - 900, new Date().toISOString())
      .run();
    const roomName = crypto.randomUUID();
    const { socket, participantId } = await joinVoice(roomName, userId);
    await backdate(roomName, participantId, 30 * 60_000);
    await leave(socket);

    const awarded = await eventually(
      () => badges(userId),
      (rows) => rows.length > 0,
    );
    expect(awarded).toEqual([
      expect.objectContaining({ badge_id: "voice-regular", source: "auto" }),
    ]);
  });
});

describe("gateway time zone", () => {
  it("keeps the time zone the Worker stamped at upgrade on the session", async () => {
    const userId = `tz-${crypto.randomUUID()}`;
    const headers = await admissionHeaders("global-gateway", userId, "global");
    headers.set(TIME_ZONE_HEADER, "Asia/Kamchatka");
    const response = await meetingRoom("global-gateway").fetch(
      "https://internal/api/gateway?v=1",
      { headers },
    );
    expect(response.status).toBe(101);
    const socket = response.webSocket!;
    const hello = nextOp(socket, 8);
    socket.accept();
    await hello;
    const ready = nextOp(socket, 2);
    socket.send(JSON.stringify({ op: 0, d: { name: userId } }));
    await ready;

    const zones = await runInDurableObject(
      meetingRoom("global-gateway"),
      (instance) =>
        [
          ...(
            instance as unknown as {
              sessions: Map<
                unknown,
                { user_id?: string; admission?: { timeZone?: string } }
              >;
            }
          ).sessions.values(),
        ].flatMap((session) =>
          session.user_id === userId ? [session.admission?.timeZone] : [],
        ),
    );
    expect(zones).toEqual(["Asia/Kamchatka"]);
    socket.close();
  });
});

describe("message stats and Night owl", () => {
  it("counts messages and awards Night owl at 3 a.m. local, once", async () => {
    await setup();
    const userId = `owl-${crypto.randomUUID()}`;
    await addUser(userId);

    // 15:00 UTC is 03:00 in Kamchatka (UTC+12): Night owl. Without a time
    // zone (no cf) it reads UTC, where it is not.
    const zone = "Asia/Kamchatka";
    await recordMessageSent(db, userId, new Date("2026-11-09T15:00:00Z"));
    expect(await badges(userId)).toEqual([]);
    await recordMessageSent(db, userId, new Date("2026-11-09T15:00:00Z"), zone);
    await recordMessageSent(db, userId, new Date("2026-11-09T15:30:00Z"), zone);

    expect((await stats(userId))?.messages_sent).toBe(3);
    const owl = await badges(userId);
    expect(owl).toEqual([
      expect.objectContaining({
        badge_id: "night-owl",
        source: "auto",
        awarded_at: "2026-11-09T15:00:00.000Z",
      }),
    ]);
  });
});

describe("automatic awards", () => {
  it("sweeps idempotently: a second run awards nothing and keeps dates", async () => {
    await setup();
    const owner = `owner-${crypto.randomUUID()}`;
    const server = `server-${crypto.randomUUID()}`;
    await addUser(owner, "2000-01-01T00:00:00.000Z");
    await db
      .prepare(`INSERT INTO servers (id, name, owner_id) VALUES (?, 'S', ?)`)
      .bind(server, owner)
      .run();
    await db.batch(
      Array.from({ length: 10 }, (_, index) =>
        db
          .prepare(
            `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
          )
          .bind(server, index === 0 ? owner : `${server}-m${index}`),
      ),
    );
    await db
      .prepare(
        `INSERT INTO user_stats (user_id, voice_seconds, updated_at) VALUES (?, 360000, ?)`,
      )
      .bind(owner, new Date().toISOString())
      .run();

    const first = await sweepAutoBadges(db, "2026-11-09T00:00:00.000Z");
    expect(first["server-owner"]).toBeGreaterThanOrEqual(1);
    expect(first["voice-regular"]).toBeGreaterThanOrEqual(1);
    const awarded = await badges(owner);
    expect(awarded.map((row) => row.badge_id)).toEqual([
      "early-yapper",
      "server-owner",
      "voice-regular",
    ]);

    const second = await sweepAutoBadges(db, "2026-11-10T00:00:00.000Z");
    expect(second).toEqual({
      "early-yapper": 0,
      "server-owner": 0,
      "voice-regular": 0,
    });
    expect(await badges(owner)).toEqual(awarded);
  });
});

describe("manual badges: the admin gate", () => {
  it("refuses non-admins and badges with their own rules", async () => {
    await setup();
    const target = `target-${crypto.randomUUID()}`;
    await addUser(target);
    const admins = new Set(["admin-1"]);

    const refused = await setManualBadge(db, {
      actorId: "someone",
      adminIds: admins,
      targetUserId: target,
      badgeId: "bug-hunter",
      action: "award",
    });
    expect(refused).toMatchObject({
      ok: false,
      status: 403,
      code: "BADGES_FORBIDDEN",
    });
    expect(await badges(target)).toEqual([]);

    // Nobody is an admin while the list is empty.
    expect(
      await setManualBadge(db, {
        actorId: "admin-1",
        adminIds: new Set(),
        targetUserId: target,
        badgeId: "bug-hunter",
        action: "award",
      }),
    ).toMatchObject({ ok: false, status: 403 });

    for (const badgeId of ["founder", "voice-regular", "loud", "night-owl"]) {
      expect(
        await setManualBadge(db, {
          actorId: "admin-1",
          adminIds: admins,
          targetUserId: target,
          badgeId,
          action: "award",
        }),
      ).toMatchObject({ ok: false, code: "BADGE_NOT_MANUAL" });
    }
    expect(
      await setManualBadge(db, {
        actorId: "admin-1",
        adminIds: admins,
        targetUserId: target,
        badgeId: "nope",
        action: "award",
      }),
    ).toMatchObject({ ok: false, code: "BADGE_UNKNOWN" });
    expect(
      await setManualBadge(db, {
        actorId: "admin-1",
        adminIds: admins,
        targetUserId: "no-such-user",
        badgeId: "bug-hunter",
        action: "award",
      }),
    ).toMatchObject({ ok: false, status: 404 });
    expect(await badges(target)).toEqual([]);
  });

  it("lets an admin award and revoke Bug hunter, idempotently", async () => {
    await setup();
    const target = `hunter-${crypto.randomUUID()}`;
    await addUser(target);
    const input = {
      actorId: "admin-1",
      adminIds: new Set(["admin-1"]),
      targetUserId: target,
      badgeId: "bug-hunter",
    };

    expect(
      await setManualBadge(db, {
        ...input,
        action: "award",
        now: "2026-11-09T00:00:00.000Z",
      }),
    ).toEqual({ ok: true, changed: true });
    expect(
      await setManualBadge(db, {
        ...input,
        action: "award",
        now: "2026-12-01T00:00:00.000Z",
      }),
    ).toEqual({ ok: true, changed: false });
    expect(await badges(target)).toEqual([
      {
        badge_id: "bug-hunter",
        awarded_at: "2026-11-09T00:00:00.000Z",
        source: "admin",
      },
    ]);

    expect(await setManualBadge(db, { ...input, action: "revoke" })).toEqual({
      ok: true,
      changed: true,
    });
    expect(await setManualBadge(db, { ...input, action: "revoke" })).toEqual({
      ok: true,
      changed: false,
    });
    expect(await badges(target)).toEqual([]);
  });
});

describe("legacy account claims", () => {
  async function seedStats(
    userId: string,
    row: [number, number, number, string | null, string | null],
  ) {
    await db
      .prepare(
        `INSERT INTO user_stats (user_id, voice_seconds, stream_seconds, messages_sent, first_seen_at, last_active_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, 'x')`,
      )
      .bind(userId, ...row)
      .run();
  }

  it("merges plan, badges and stats into the new id, then drops the legacy rows", async () => {
    await setup();
    const legacy = `legacy-${crypto.randomUUID()}`;
    const fresh = `fresh-${crypto.randomUUID()}`;
    await addUser(legacy);
    await addUser(fresh);
    await seedStats(legacy, [
      100,
      10,
      5,
      "2026-01-01T00:00:00Z",
      "2026-03-01T00:00:00Z",
    ]);
    await seedStats(fresh, [
      20,
      2,
      1,
      "2026-02-01T00:00:00Z",
      "2026-11-01T00:00:00Z",
    ]);
    await db.batch([
      db
        .prepare(
          `INSERT INTO user_entitlements (user_id, tier, source, started_at, updated_at) VALUES (?, 'founder', 'grant', '2026-11-09', 'x')`,
        )
        .bind(legacy),
      db
        .prepare(
          `INSERT INTO user_badges (user_id, badge_id, awarded_at, source) VALUES (?, 'founder', '2026-11-09T00:00:00Z', 'grant'), (?, 'night-owl', '2026-11-10T00:00:00Z', 'auto')`,
        )
        .bind(legacy, legacy),
      db
        .prepare(
          `INSERT INTO user_badges (user_id, badge_id, awarded_at, source) VALUES (?, 'night-owl', '2026-11-12T00:00:00Z', 'auto')`,
        )
        .bind(fresh),
    ]);

    await db.batch(
      mergeLegacyAccountStatements(
        db,
        legacy,
        fresh,
        "2026-11-20T00:00:00.000Z",
      ),
    );

    expect(await stats(fresh)).toMatchObject({
      voice_seconds: 120,
      stream_seconds: 12,
      messages_sent: 6,
      first_seen_at: "2026-01-01T00:00:00Z",
    });
    const lastActive = await db
      .prepare(`SELECT last_active_at FROM user_stats WHERE user_id = ?`)
      .bind(fresh)
      .first<{ last_active_at: string }>();
    expect(lastActive?.last_active_at).toBe("2026-11-01T00:00:00Z");
    expect(await badges(fresh)).toEqual([
      expect.objectContaining({ badge_id: "founder", source: "grant" }),
      // The earlier of the two dates.
      expect.objectContaining({
        badge_id: "night-owl",
        awarded_at: "2026-11-10T00:00:00Z",
      }),
    ]);
    expect((await getProfileExtras(db, fresh)).entitlement.tier).toBe(
      "founder",
    );

    expect(await stats(legacy)).toBeNull();
    expect(await badges(legacy)).toEqual([]);
    expect(
      await db
        .prepare(`SELECT 1 FROM user_entitlements WHERE user_id = ?`)
        .bind(legacy)
        .first(),
    ).toBeNull();
  });

  it("moves stats whole when the new id has none, and keeps a null side", async () => {
    await setup();
    const legacy = `legacy-${crypto.randomUUID()}`;
    const fresh = `fresh-${crypto.randomUUID()}`;
    await addUser(legacy);
    await addUser(fresh);
    await seedStats(legacy, [7, 0, 3, null, "2026-03-01T00:00:00Z"]);

    await db.batch(mergeLegacyAccountStatements(db, legacy, fresh));
    expect(await stats(fresh)).toMatchObject({
      voice_seconds: 7,
      messages_sent: 3,
      first_seen_at: null,
    });

    // A later merge adds to it; a known first_seen fills a null one.
    await seedStats(legacy, [1, 0, 0, "2025-12-01T00:00:00Z", null]);
    await db.batch(mergeLegacyAccountStatements(db, legacy, fresh));
    expect(await stats(fresh)).toMatchObject({
      voice_seconds: 8,
      messages_sent: 3,
      first_seen_at: "2025-12-01T00:00:00Z",
    });
  });
});
