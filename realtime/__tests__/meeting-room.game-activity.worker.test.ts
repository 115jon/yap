import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { beforeAll, describe, expect, it, onTestFinished, vi } from "vitest";
import {
  issueSocketTicket,
  verifySocketTicket,
} from "../../src/lib/voice/socket-ticket";
import {
  appendRealtimeAdmissionHeaders,
  createRealtimeAdmissionContext,
} from "../realtime-admission";
import { GatewayOp, HEARTBEAT_FRAME } from "../../src/lib/gateway-protocol";
import { GAME_ACTIVITY_STORAGE_PREFIX } from "../meeting-room/game-activity";

// Game activity on the global gateway (realtime/meeting-room/game-activity.ts):
// who hears it, when it is hidden, and how often it is sent.

type Frame = { op: number; d: any };

interface Client {
  socket: WebSocket;
  frames: Frame[];
  sessionId: string;
}

async function admissionHeaders(objectName: string, subject: string) {
  const claims = {
    accessMode: "authenticated" as const,
    audience: "global" as const,
    expiresAt: Date.now() + 60_000,
    nonce: crypto.randomUUID(),
    roomSlug: "global-gateway",
    subject,
  };
  const ticket = await issueSocketTicket(claims, env.CALLS_APP_SECRET);
  const verification = await verifySocketTicket(ticket, env.CALLS_APP_SECRET, {
    accessMode: "authenticated",
    audience: "global",
    now: Date.now(),
    roomSlug: "global-gateway",
  });
  if (!verification.ok) throw new Error(`ticket for ${objectName}`);
  return appendRealtimeAdmissionHeaders(
    new Headers({
      Upgrade: "websocket",
      "Sec-WebSocket-Protocol": "ralph.realtime.v1",
    }),
    await createRealtimeAdmissionContext(verification.claims),
  );
}

function gateway(objectName: string) {
  return env.MEETING_ROOM.get(env.MEETING_ROOM.idFromName(objectName));
}

/**
 * vi.waitFor with room-sized patience: its default 1 s is shorter than a room
 * round trip can take when the whole suite runs at once. 4 s stays under the
 * 5 s test timeout. Each poll also advances fake timers by `interval`, so a
 * whole wait advances them by at most 4 s.
 */
const SETTLE = { timeout: 4_000, interval: 5 };
const settle: typeof vi.waitFor = (callback, options = SETTLE) =>
  vi.waitFor(callback, options);

async function waitForFrame(
  client: Client,
  predicate: (frame: Frame) => boolean,
  from = 0,
): Promise<Frame> {
  let found: Frame | undefined;
  await settle(() => {
    found = client.frames.slice(from).find(predicate);
    expect(found).toBeDefined();
  });
  return found as Frame;
}

/** Opens a global socket, records every frame, and identifies. */
async function connect(objectName: string, userId: string): Promise<Client> {
  const response = await gateway(objectName).fetch(
    "https://internal/api/gateway?v=1",
    { headers: await admissionHeaders(objectName, userId) },
  );
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  if (!socket) throw new Error("no socket");
  const frames: Frame[] = [];
  socket.addEventListener("message", (event) => {
    frames.push(JSON.parse(event.data as string) as Frame);
  });
  socket.accept();
  const client: Client = { socket, frames, sessionId: "" };
  await waitForFrame(client, (frame) => frame.op === GatewayOp.Hello);
  socket.send(JSON.stringify({ op: GatewayOp.Identify, d: { name: userId } }));
  const ready = await waitForFrame(
    client,
    (frame) => frame.op === GatewayOp.Ready,
  );
  client.sessionId = ready.d.participant_id;
  return client;
}

function presenceUpdatesFor(client: Client, userId: string, from = 0) {
  return client.frames
    .slice(from)
    .filter(
      (frame) =>
        frame.op === GatewayOp.Dispatch &&
        frame.d?.event === "PRESENCE_UPDATE" &&
        frame.d.data?.user_id === userId &&
        "game_activity" in frame.d.data,
    )
    .map((frame) => frame.d.data);
}

function lastPresenceList(client: Client) {
  const lists = client.frames.filter(
    (frame) =>
      frame.op === GatewayOp.Dispatch && frame.d?.event === "PRESENCE_LIST",
  );
  return lists[lists.length - 1]?.d.data as {
    users: Array<{ user_id: string; game_activity?: unknown }>;
  };
}

function activityOf(list: ReturnType<typeof lastPresenceList>, userId: string) {
  return list.users.find((user) => user.user_id === userId)?.game_activity;
}

function playing(name: string, extra: Record<string, unknown> = {}) {
  return {
    kind: "playing",
    name,
    started_at: Date.now() - 60_000,
    source: "manual",
    ...extra,
  };
}

function setActivity(client: Client, activity: unknown) {
  client.socket.send(
    JSON.stringify({
      op: GatewayOp.ActivityUpdate,
      d: { game_activity: activity },
    }),
  );
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

async function shareServer(serverId: string, userIds: string[]) {
  for (const userId of userIds) {
    await env.DB.prepare(
      "INSERT OR IGNORE INTO server_members (server_id, user_id) VALUES (?, ?)",
    )
      .bind(serverId, userId)
      .run();
  }
}

/** Shortens the coalescing window so tests do not wait 5 s per update. */
async function setBroadcastInterval(objectName: string, ms: number) {
  await runInDurableObject(gateway(objectName), (instance) => {
    (
      instance as unknown as {
        gameActivity: { broadcastIntervalMs: number };
      }
    ).gameActivity.broadcastIntervalMs = ms;
  });
}

/**
 * Runs the room's due fake timers. It has to happen inside the room: a timer
 * callback resolves a promise the room created, and workerd refuses that
 * from another I/O context ("Cannot perform I/O on behalf of a different
 * Durable Object").
 */
async function advanceRoomTimers(objectName: string, ms: number) {
  await runInDurableObject(gateway(objectName), () => {
    vi.advanceTimersByTime(ms);
  });
}

/** The coalescer's view of one user: is a flush waiting, what it would send. */
async function broadcastState(objectName: string, userId: string) {
  return runInDurableObject(gateway(objectName), (instance) => {
    const gameActivity = (
      instance as unknown as {
        gameActivity: {
          broadcasts: Map<string, { lastSentAt: number; pending: boolean }>;
          visible(userId: string): { name?: string } | null;
        };
      }
    ).gameActivity;
    const state = gameActivity.broadcasts.get(userId);
    return {
      lastSentAt: state?.lastSentAt ?? 0,
      pending: state?.pending ?? false,
      visible: gameActivity.visible(userId)?.name ?? null,
    };
  });
}

beforeAll(async () => {
  await env.DB.prepare(
    "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT NOT NULL, status TEXT, share_activity INTEGER NOT NULL DEFAULT 1)",
  ).run();
  await env.DB.prepare(
    "ALTER TABLE users ADD COLUMN share_activity INTEGER NOT NULL DEFAULT 1",
  )
    .run()
    .catch(() => {});
  await env.DB.prepare(
    "CREATE TABLE IF NOT EXISTS server_members (server_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY (server_id, user_id))",
  ).run();
  await env.DB.prepare(
    "CREATE TABLE IF NOT EXISTS relationships (user_id TEXT NOT NULL, target_user_id TEXT NOT NULL, type INTEGER NOT NULL, PRIMARY KEY (user_id, target_user_id))",
  ).run();
  await env.DB.prepare(
    "CREATE TABLE IF NOT EXISTS dm_recipients (channel_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY (channel_id, user_id))",
  ).run();
});

describe("MeetingRoom game activity", () => {
  it("shows an activity to its audience only, live and in PRESENCE_LIST", async () => {
    const objectName = crypto.randomUUID();
    const [player, mate, friend, stranger, late] = [
      "player",
      "mate",
      "friend",
      "stranger",
      "late",
    ].map((name) => `${name}-${crypto.randomUUID()}`);
    const serverId = `server-${crypto.randomUUID()}`;
    await shareServer(serverId, [player, mate, late]);
    await env.DB.prepare(
      "INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 0), (?, ?, 0)",
    )
      .bind(player, friend, friend, player)
      .run();

    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);
    const friendClient = await connect(objectName, friend);
    const strangerClient = await connect(objectName, stranger);

    setActivity(playerClient, playing("Minecraft", { details: "Survival" }));
    const seen = await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.event === "PRESENCE_UPDATE" &&
        frame.d.data?.user_id === player &&
        frame.d.data.game_activity !== undefined,
    );
    expect(seen.d.data).toMatchObject({
      status: "online",
      game_activity: {
        kind: "playing",
        name: "Minecraft",
        details: "Survival",
        source: "manual",
      },
    });
    // Not replayed: no seq on the frame.
    expect(seen.d.seq).toBeUndefined();
    await waitForFrame(
      friendClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity?.name === "Minecraft",
    );
    // The owner's own sessions hear it too.
    await waitForFrame(
      playerClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity?.name === "Minecraft",
    );
    await sleep(100);
    expect(presenceUpdatesFor(strangerClient, player)).toEqual([]);

    const lateClient = await connect(objectName, late);
    expect(activityOf(lastPresenceList(lateClient), player)).toMatchObject({
      name: "Minecraft",
    });
    const stranger2 = await connect(objectName, `${stranger}-2`);
    expect(activityOf(lastPresenceList(stranger2), player)).toBeNull();

    for (const client of [
      playerClient,
      mateClient,
      friendClient,
      strangerClient,
      lateClient,
      stranger2,
    ]) {
      client.socket.close(1000, "done");
    }
  });

  it("drops the activity and its stored copy when the socket closes", async () => {
    const objectName = crypto.randomUUID();
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);

    setActivity(playerClient, playing("Celeste"));
    await waitForFrame(
      mateClient,
      (frame) => frame.d?.data?.game_activity?.name === "Celeste",
    );
    const key = GAME_ACTIVITY_STORAGE_PREFIX + playerClient.sessionId;
    await settle(async () => {
      const stored = await runInDurableObject(gateway(objectName), (_, state) =>
        state.storage.get(key),
      );
      expect(stored).toMatchObject({ user_id: player, share: true });
    });

    const from = mateClient.frames.length;
    playerClient.socket.close(1000, "done");
    await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.event === "PRESENCE_UPDATE" &&
        frame.d.data?.user_id === player &&
        frame.d.data.status === "offline",
      from,
    );
    await settle(async () => {
      const stored = await runInDurableObject(gateway(objectName), (_, state) =>
        state.storage.get(key),
      );
      expect(stored).toBeUndefined();
    });
    const again = await connect(objectName, `${mate}-later`);
    expect(activityOf(lastPresenceList(again), player)).toBeUndefined();
    mateClient.socket.close(1000, "done");
    again.socket.close(1000, "done");
  });

  it("hides the activity while invisible and shows it again after", async () => {
    const objectName = crypto.randomUUID();
    await setBroadcastInterval(objectName, 20);
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);

    setActivity(playerClient, playing("Hades"));
    await waitForFrame(
      mateClient,
      (frame) => frame.d?.data?.game_activity?.name === "Hades",
    );

    let from = mateClient.frames.length;
    playerClient.socket.send(
      JSON.stringify({
        op: GatewayOp.PresenceUpdate,
        d: { status: "offline" },
      }),
    );
    const hidden = await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity === null,
      from,
    );
    expect(hidden.d.data.status).toBe("offline");
    const viewer = await connect(objectName, `${mate}-viewer`);
    await shareServer(`server-${crypto.randomUUID()}`, [
      player,
      `${mate}-viewer`,
    ]);
    expect(activityOf(lastPresenceList(viewer), player)).toBeNull();

    from = mateClient.frames.length;
    playerClient.socket.send(
      JSON.stringify({ op: GatewayOp.PresenceUpdate, d: { status: "online" } }),
    );
    await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity?.name === "Hades",
      from,
    );
    for (const client of [playerClient, mateClient, viewer]) {
      client.socket.close(1000, "done");
    }
  });

  it("shares nothing while share_activity is 0, until it is turned on", async () => {
    const objectName = crypto.randomUUID();
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await env.DB.prepare(
      "INSERT INTO users (id, username, share_activity) VALUES (?, ?, 0)",
    )
      .bind(player, player)
      .run();
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);

    setActivity(playerClient, playing("Tetris"));
    await sleep(150);
    expect(
      presenceUpdatesFor(mateClient, player).filter(
        (data) => data.game_activity !== null,
      ),
    ).toEqual([]);
    const viewer = await connect(objectName, mate);
    expect(activityOf(lastPresenceList(viewer), player)).toBeNull();

    // The profile route tells the gateway (broadcastToUser).
    const response = await gateway(objectName).fetch(
      "https://internal/broadcast",
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          target_user_id: player,
          event: "USER_PROFILE_UPDATE",
          data: { user_id: player, share_activity: true },
        }),
      },
    );
    expect(response.status).toBe(200);
    await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity?.name === "Tetris",
    );
    for (const client of [playerClient, mateClient, viewer]) {
      client.socket.close(1000, "done");
    }
  });

  it("coalesces updates per user: the first goes now, then only the last", async () => {
    const objectName = crypto.randomUUID();
    // The room's setTimeout is faked and the window is 60 s, so whether "Two"
    // and "Three" land inside it does not depend on how fast "One" arrived.
    // Only advanceRoomTimers ends the window: vi.waitFor also advances fake
    // timers, by its interval per poll, but each wait advances them by at
    // most SETTLE.timeout (4 s) and the test does two before it. Date stays
    // real: the room computes the wait from Date.now(), and tickets need it.
    await setBroadcastInterval(objectName, 60_000);
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    onTestFinished(() => {
      vi.useRealTimers();
      playerClient.socket.close(1000, "done");
      mateClient.socket.close(1000, "done");
    });
    const startedAt = Date.now() - 1_000;
    // Nothing was sent for this user yet, so the first update goes now.
    setActivity(playerClient, playing("One", { started_at: startedAt }));
    await waitForFrame(
      mateClient,
      (frame) => frame.d?.data?.game_activity?.name === "One",
    );
    setActivity(playerClient, playing("Two", { started_at: startedAt }));
    setActivity(playerClient, playing("Three", { started_at: startedAt }));
    // Both updates are handled, and one flush is waiting on the window.
    await settle(async () => {
      expect(await broadcastState(objectName, player)).toMatchObject({
        pending: true,
        visible: "Three",
      });
    });
    expect(presenceUpdatesFor(mateClient, player)).toHaveLength(1);

    await advanceRoomTimers(objectName, 60_000);
    await waitForFrame(
      mateClient,
      (frame) => frame.d?.data?.game_activity?.name === "Three",
    );
    expect(
      presenceUpdatesFor(mateClient, player).map(
        (data) => data.game_activity?.name,
      ),
    ).toEqual(["One", "Three"]);

    // The same activity again changes nothing and sends nothing. The
    // player's heartbeat is answered after the update frame is handled, and
    // by then no flush is waiting and none has stamped a new send.
    const sent = await broadcastState(objectName, player);
    expect(sent.pending).toBe(false);
    setActivity(playerClient, playing("Three", { started_at: startedAt }));
    const acks = playerClient.frames.length;
    playerClient.socket.send(HEARTBEAT_FRAME);
    await waitForFrame(
      playerClient,
      (frame) => frame.op === GatewayOp.HeartbeatACK,
      acks,
    );
    expect(await broadcastState(objectName, player)).toEqual(sent);
    expect(presenceUpdatesFor(mateClient, player)).toHaveLength(2);
  });

  it("rate limits ActivityUpdate at 12 a minute", async () => {
    const objectName = crypto.randomUUID();
    const player = `player-${crypto.randomUUID()}`;
    const client = await connect(objectName, player);
    for (let i = 0; i < 13; i += 1) {
      setActivity(client, playing(`Game ${i}`));
    }
    const limited = await waitForFrame(
      client,
      (frame) => frame.op === GatewayOp.Error && frame.d?.code === 4290,
    );
    expect(limited).toBeDefined();
    expect(
      client.frames.filter(
        (frame) => frame.op === GatewayOp.Error && frame.d?.code === 4290,
      ),
    ).toHaveLength(1);
    client.socket.close(1000, "done");
  });

  it("refuses an icon that is not our own path", async () => {
    const objectName = crypto.randomUUID();
    const client = await connect(objectName, `player-${crypto.randomUUID()}`);
    setActivity(
      client,
      playing("Phish", { icon_url: "https://example.com/icon.png" }),
    );
    const error = await waitForFrame(
      client,
      (frame) => frame.op === GatewayOp.Error,
    );
    expect(error.d).toEqual({
      code: 4000,
      message: "Invalid activity payload",
    });
    const stored = await runInDurableObject(
      gateway(objectName),
      (instance) =>
        (instance as unknown as { gameActivity: { size: number } }).gameActivity
          .size,
    );
    expect(stored).toBe(0);
    client.socket.close(1000, "done");
  });

  it("sends a resumed session a fresh PRESENCE_LIST with activities", async () => {
    const objectName = crypto.randomUUID();
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);

    // The mate drops; the player starts a game meanwhile.
    mateClient.socket.close();
    await settle(async () => {
      const held = await runInDurableObject(gateway(objectName), (instance) =>
        (
          instance as unknown as { resumableSessions: Map<string, unknown> }
        ).resumableSessions.has(mateClient.sessionId),
      );
      expect(held).toBe(true);
    });
    setActivity(playerClient, playing("Outer Wilds"));
    await waitForFrame(
      playerClient,
      (frame) => frame.d?.data?.game_activity?.name === "Outer Wilds",
    );

    const response = await gateway(objectName).fetch(
      "https://internal/api/gateway?v=1",
      { headers: await admissionHeaders(objectName, mate) },
    );
    const socket = response.webSocket;
    if (!socket) throw new Error("no socket");
    const resumed: Client = { socket, frames: [], sessionId: "" };
    socket.addEventListener("message", (event) => {
      resumed.frames.push(JSON.parse(event.data as string) as Frame);
    });
    socket.accept();
    await waitForFrame(resumed, (frame) => frame.op === GatewayOp.Hello);
    socket.send(
      JSON.stringify({
        op: GatewayOp.Resume,
        d: { session_id: mateClient.sessionId, seq_ack: 0 },
      }),
    );
    await waitForFrame(resumed, (frame) => frame.op === GatewayOp.Resumed);
    await waitForFrame(
      resumed,
      (frame) =>
        frame.d?.event === "PRESENCE_LIST" &&
        activityOf(frame.d.data, player) !== undefined,
    );
    expect(activityOf(lastPresenceList(resumed), player)).toMatchObject({
      name: "Outer Wilds",
    });
    playerClient.socket.close(1000, "done");
    socket.close(1000, "done");
  });

  it("drops a session's activity when a Resume takes it over from a lost socket", async () => {
    const objectName = crypto.randomUUID();
    await setBroadcastInterval(objectName, 50);
    const player = `player-${crypto.randomUUID()}`;
    const mate = `mate-${crypto.randomUUID()}`;
    await shareServer(`server-${crypto.randomUUID()}`, [player, mate]);
    const playerClient = await connect(objectName, player);
    const mateClient = await connect(objectName, mate);

    setActivity(playerClient, playing("Balatro"));
    await waitForFrame(
      mateClient,
      (frame) => frame.d?.data?.game_activity?.name === "Balatro",
    );

    // The old socket is still live here (its close never arrived).
    const seqAck = Math.max(
      0,
      ...playerClient.frames.map((frame) =>
        typeof frame.d?.seq === "number" ? frame.d.seq : 0,
      ),
    );
    const response = await gateway(objectName).fetch(
      "https://internal/api/gateway?v=1",
      { headers: await admissionHeaders(objectName, player) },
    );
    const socket = response.webSocket;
    if (!socket) throw new Error("no socket");
    const resumed: Client = { socket, frames: [], sessionId: "" };
    socket.addEventListener("message", (event) => {
      resumed.frames.push(JSON.parse(event.data as string) as Frame);
    });
    socket.accept();
    await waitForFrame(resumed, (frame) => frame.op === GatewayOp.Hello);
    const from = mateClient.frames.length;
    socket.send(
      JSON.stringify({
        op: GatewayOp.Resume,
        d: { session_id: playerClient.sessionId, seq_ack: seqAck },
      }),
    );
    await waitForFrame(resumed, (frame) => frame.op === GatewayOp.Resumed);

    const key = GAME_ACTIVITY_STORAGE_PREFIX + playerClient.sessionId;
    await settle(async () => {
      const stored = await runInDurableObject(gateway(objectName), (_, state) =>
        state.storage.get(key),
      );
      expect(stored).toBeUndefined();
    });
    // Not re-sent: the audience hears it is gone.
    await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity === null,
      from,
    );

    // Sent again on the resumed socket: back for the audience.
    const again = mateClient.frames.length;
    setActivity(resumed, playing("Balatro"));
    await waitForFrame(
      mateClient,
      (frame) =>
        frame.d?.data?.user_id === player &&
        frame.d.data.game_activity?.name === "Balatro",
      again,
    );
    socket.close(1000, "done");
    mateClient.socket.close(1000, "done");
  });
});
