import { env } from "cloudflare:workers";
import { runInDurableObject } from "cloudflare:test";
import { beforeAll, describe, expect, it, vi } from "vitest";

import { GatewayOp } from "../../src/lib/gateway-protocol";
import {
  issueSocketTicket,
  verifySocketTicket,
} from "../../src/lib/voice/socket-ticket";
import {
  appendRealtimeAdmissionHeaders,
  createRealtimeAdmissionContext,
} from "../realtime-admission";

// Deadlock live state through the real gateway (op 43 → handleActivityUpdate
// → PRESENCE_UPDATE): account ids and one-sided party claims never reach
// anyone, a party shows only when both members claim each other, and an
// overtaken frame never lands after a newer one.

type Frame = { op: number; d: any };

interface Client {
  socket: WebSocket;
  frames: Frame[];
}

const DEADLOCK_APP_ID = "1276737795012165766";
const STEAM_BASE = 76561197960265728n;

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

async function waitForFrame(
  client: Client,
  predicate: (frame: Frame) => boolean,
): Promise<Frame> {
  let found: Frame | undefined;
  await vi.waitFor(
    () => {
      found = [...client.frames].reverse().find(predicate);
      expect(found).toBeDefined();
    },
    { timeout: 3_000, interval: 5 },
  );
  return found as Frame;
}

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
  const client: Client = { socket, frames };
  await waitForFrame(client, (frame) => frame.op === GatewayOp.Hello);
  socket.send(JSON.stringify({ op: GatewayOp.Identify, d: { name: userId } }));
  await waitForFrame(client, (frame) => frame.op === GatewayOp.Ready);
  return client;
}

function send(client: Client, activity: unknown) {
  client.socket.send(
    JSON.stringify({
      op: GatewayOp.ActivityUpdate,
      d: { game_activity: activity },
    }),
  );
}

function deadlock(game: Record<string, unknown>) {
  return {
    kind: "playing",
    name: "Deadlock",
    app_id: DEADLOCK_APP_ID,
    started_at: Date.now() - 60_000,
    source: "detected",
    game: { kind: "deadlock", ...game },
  };
}

/** The newest game_activity `client` heard for `userId`. */
function latestActivity(client: Client, userId: string) {
  const updates = client.frames.filter(
    (frame) =>
      frame.op === GatewayOp.Dispatch &&
      frame.d?.event === "PRESENCE_UPDATE" &&
      frame.d.data?.user_id === userId &&
      "game_activity" in frame.d.data,
  );
  return updates.at(-1)?.d.data.game_activity;
}

async function setBroadcastInterval(objectName: string, ms: number) {
  await runInDurableObject(gateway(objectName), (instance) => {
    (
      instance as unknown as { gameActivity: { broadcastIntervalMs: number } }
    ).gameActivity.broadcastIntervalMs = ms;
  });
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

let accountSeq = 70_000_000 + Math.floor(Math.random() * 1_000_000) * 10;

/** A user in `serverId` with a linked Steam account (shown on profile). */
async function player(serverId: string, show = true) {
  const userId = `dl-${crypto.randomUUID()}`;
  accountSeq += 1;
  const accountId = accountSeq;
  const id64 = String(STEAM_BASE + BigInt(accountId));
  await env.DB.prepare(
    "INSERT INTO users (id, username, share_activity) VALUES (?, ?, 1)",
  )
    .bind(userId, userId)
    .run();
  await env.DB.prepare(
    "INSERT OR IGNORE INTO server_members (server_id, user_id) VALUES (?, ?)",
  )
    .bind(serverId, userId)
    .run();
  await env.DB.prepare(
    `INSERT INTO user_connections (user_id, provider, external_id, display_name, show_on_profile, metadata, verified_at, created_at, updated_at)
     VALUES (?, 'steam', ?, ?, ?, ?, '2026-11-09', '2026-11-09', '2026-11-09')`,
  )
    .bind(
      userId,
      id64,
      userId,
      show ? 1 : 0,
      JSON.stringify({ steam_id64: id64, account_id: accountId }),
    )
    .run();
  return { userId, accountId };
}

beforeAll(async () => {
  const statements = [
    "CREATE TABLE IF NOT EXISTS users (id TEXT PRIMARY KEY, username TEXT NOT NULL, status TEXT, share_activity INTEGER NOT NULL DEFAULT 1)",
    "CREATE TABLE IF NOT EXISTS server_members (server_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY (server_id, user_id))",
    "CREATE TABLE IF NOT EXISTS relationships (user_id TEXT NOT NULL, target_user_id TEXT NOT NULL, type INTEGER NOT NULL, PRIMARY KEY (user_id, target_user_id))",
    "CREATE TABLE IF NOT EXISTS dm_recipients (channel_id TEXT NOT NULL, user_id TEXT NOT NULL, PRIMARY KEY (channel_id, user_id))",
    `CREATE TABLE IF NOT EXISTS user_connections (user_id TEXT NOT NULL, provider TEXT NOT NULL, external_id TEXT NOT NULL, display_name TEXT NOT NULL, profile_url TEXT, avatar_url TEXT, show_on_profile INTEGER NOT NULL DEFAULT 1, metadata TEXT NOT NULL DEFAULT '{}', verified_at TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY (user_id, provider), UNIQUE (provider, external_id))`,
    "CREATE TABLE IF NOT EXISTS deadlock_fetch_state (account_id INTEGER PRIMARY KEY, user_id TEXT NOT NULL, next_due_at INTEGER NOT NULL, retry_at INTEGER, fail_count INTEGER NOT NULL DEFAULT 0, last_error TEXT, attempted_at INTEGER, signal TEXT, history_at INTEGER, live_since INTEGER, live_until INTEGER, queued_at INTEGER, post_game_at INTEGER)",
  ];
  for (const sql of statements) await env.DB.prepare(sql).run();
  await env.DB.prepare(
    "ALTER TABLE users ADD COLUMN share_activity INTEGER NOT NULL DEFAULT 1",
  )
    .run()
    .catch(() => {});
});

describe("Deadlock live state through the gateway", () => {
  it("strips account ids and shows a party only when both members claim it", async () => {
    const objectName = crypto.randomUUID();
    const serverId = `server-${crypto.randomUUID()}`;
    const a = await player(serverId);
    const b = await player(serverId);
    const viewerId = `viewer-${crypto.randomUUID()}`;
    await env.DB.prepare(
      "INSERT INTO server_members (server_id, user_id) VALUES (?, ?)",
    )
      .bind(serverId, viewerId)
      .run();
    await setBroadcastInterval(objectName, 10);
    const aClient = await connect(objectName, a.userId);
    const bClient = await connect(objectName, b.userId);
    const viewer = await connect(objectName, viewerId);

    // A claims B; B has not said anything yet.
    send(
      aClient,
      deadlock({
        phase: "in_match",
        hero: "haze",
        account_id: a.accountId,
        party: [b.accountId, 999_999],
        // A client cannot name Yap users itself.
        party_user_ids: [viewerId],
      }),
    );
    await waitForFrame(
      viewer,
      (frame) =>
        frame.d?.data?.user_id === a.userId &&
        frame.d.data.game_activity?.game?.hero === "haze",
    );
    for (const client of [viewer, aClient]) {
      const seen = latestActivity(client, a.userId);
      expect(seen.game).toEqual({
        kind: "deadlock",
        phase: "in_match",
        hero: "haze",
      });
    }
    const raw = JSON.stringify(viewer.frames);
    expect(raw).not.toContain(String(a.accountId));
    expect(raw).not.toContain(String(b.accountId));
    expect(raw).not.toContain('"party"');

    // B claims A back: both now name each other, to everyone.
    send(
      bClient,
      deadlock({
        phase: "in_match",
        hero: "wraith",
        account_id: b.accountId,
        party: [a.accountId],
      }),
    );
    await waitForFrame(
      viewer,
      (frame) =>
        frame.d?.data?.user_id === a.userId &&
        frame.d.data.game_activity?.game?.party_user_ids?.[0] === b.userId,
    );
    expect(latestActivity(viewer, b.userId)?.game?.party_user_ids).toEqual([
      a.userId,
    ]);
    expect(JSON.stringify(viewer.frames)).not.toContain(String(b.accountId));

    // B leaves the party: A's party goes too.
    send(
      bClient,
      deadlock({ phase: "hideout", hero: "wraith", account_id: b.accountId }),
    );
    await waitForFrame(
      viewer,
      (frame) =>
        frame.d?.data?.user_id === a.userId &&
        frame.d.data.game_activity?.game !== undefined &&
        !frame.d.data.game_activity.game.party_user_ids,
    );

    for (const client of [aClient, bClient, viewer]) {
      client.socket.close(1000, "done");
    }
  });

  it("never names a member who hides Steam from their profile", async () => {
    const objectName = crypto.randomUUID();
    const serverId = `server-${crypto.randomUUID()}`;
    const a = await player(serverId);
    const hidden = await player(serverId, false);
    await setBroadcastInterval(objectName, 10);
    const aClient = await connect(objectName, a.userId);
    const hiddenClient = await connect(objectName, hidden.userId);
    send(
      hiddenClient,
      deadlock({
        phase: "in_match",
        account_id: hidden.accountId,
        party: [a.accountId],
      }),
    );
    send(
      aClient,
      deadlock({
        phase: "in_match",
        account_id: a.accountId,
        party: [hidden.accountId],
      }),
    );
    await sleep(150);
    await waitForFrame(
      hiddenClient,
      (frame) =>
        frame.d?.data?.user_id === a.userId &&
        frame.d.data.game_activity?.game?.phase === "in_match",
    );
    expect(
      latestActivity(hiddenClient, a.userId)?.game?.party_user_ids,
    ).toBeUndefined();
    aClient.socket.close(1000, "done");
    hiddenClient.socket.close(1000, "done");
  });

  it("applies op-43 frames in the order they were sent", async () => {
    const objectName = crypto.randomUUID();
    const serverId = `server-${crypto.randomUUID()}`;
    const a = await player(serverId);
    await setBroadcastInterval(objectName, 10);
    const aClient = await connect(objectName, a.userId);

    // The first frame's account check is slow (its D1 read is held back);
    // the second frame has nothing to check. The second must win.
    const final = await runInDurableObject(
      gateway(objectName),
      async (instance) => {
        const room = instance as unknown as {
          env: { DB: D1Database };
          ctx: DurableObjectState;
          handleActivityUpdate(ws: WebSocket, d: unknown): Promise<void>;
          gameActivity: { visible(id: string): { name: string } | null };
        };
        const realDb = room.env.DB;
        let release!: () => void;
        const held = new Promise<void>((resolve) => {
          release = resolve;
        });
        room.env.DB = new Proxy(realDb, {
          get(target, key, receiver) {
            if (key !== "prepare") return Reflect.get(target, key, receiver);
            return (sql: string) => {
              const statement = target.prepare(sql);
              if (!sql.includes("FROM user_connections")) return statement;
              return {
                bind: (...args: unknown[]) => {
                  const bound = statement.bind(...args);
                  return {
                    first: async () => {
                      await held;
                      return bound.first();
                    },
                    all: async () => {
                      await held;
                      return bound.all();
                    },
                  };
                },
              };
            };
          },
        });
        try {
          const ws = room.ctx.getWebSockets()[0]!;
          const slow = room.handleActivityUpdate(ws, {
            game_activity: deadlock({
              phase: "in_match",
              account_id: a.accountId,
            }),
          });
          await room.handleActivityUpdate(ws, {
            game_activity: {
              kind: "playing",
              name: "Chess",
              started_at: Date.now() - 1000,
              source: "manual",
            },
          });
          release();
          await slow;
        } finally {
          room.env.DB = realDb;
        }
        return room.gameActivity.visible(a.userId);
      },
    );
    expect(final?.name).toBe("Chess");
    await sleep(80);
    expect(latestActivity(aClient, a.userId)?.name).toBe("Chess");
    aClient.socket.close(1000, "done");
  });
});
