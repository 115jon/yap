// "Who can DM you" against miniflare's D1, the real route handlers and the
// real chat gateway: each setting gates new DMs, first messages, reactions
// and rings; a DM the recipient wrote in stays open; a block wins over
// "everyone"; a server owner gets no bypass; the setting is saved through
// /api/update-profile and never shown to anyone else. Kova is faked at its
// get-session call; nothing else is stubbed.
import { env } from "cloudflare:workers";
import {
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";

import schema from "../../d1_schema.sql?raw";
import {
  issueSocketTicket,
  verifySocketTicket,
} from "../../src/lib/voice/socket-ticket";
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import {
  appendRealtimeAdmissionHeaders,
  createRealtimeAdmissionContext,
} from "../realtime-admission";
import { setTestRequestHeaders } from "../test-shims/react-start-server";

type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const DmRoute = (await loadRoute("../../src/routes/api/dms")).Route;
const FriendsRoute = (await loadRoute("../../src/routes/api/friends")).Route;
const MessagesRoute = (
  await loadRoute("../../src/routes/api/channels/$id/messages")
).Route;
const ReactionsRoute = (
  await loadRoute("../../src/routes/api/channels/$id/reactions")
).Route;
const PinsRoute = (await loadRoute("../../src/routes/api/channels/$id/pins"))
  .Route;
const ProfileRoute = (await loadRoute("../../src/routes/api/update-profile"))
  .Route;
const MeRoute = (await loadRoute("../../src/routes/api/users/me")).Route;
const UserProfileRoute = (
  await loadRoute("../../src/routes/api/users/$id/profile")
).Route;

type Handler = (args: {
  request: Request;
  params?: Record<string, string>;
}) => Promise<Response>;

function handler(route: { options: unknown }, method: string): Handler {
  const fn = (
    route.options as { server: { handlers: Record<string, Handler> } }
  ).server.handlers[method];
  if (!fn) throw new Error(`no ${method} handler`);
  return fn;
}

const db = env.DB as unknown as D1Database;
const BASE = "https://yap.test";

/** Bearer token -> user id. */
const sessions = new Map<string, string>();
const realFetch = globalThis.fetch;

function fakeKova() {
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    if (url.startsWith(`${DEFAULT_KOVA_AUTH_URL}/api/auth/get-session`)) {
      const headers = new Headers(
        init?.headers ?? (input instanceof Request ? input.headers : undefined),
      );
      const token = headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
      const id = sessions.get(token);
      return id
        ? Response.json({
            user: { id, username: id, name: id },
            session: { id: `session-${id}` },
          })
        : Response.json({ error: "unauthorized" }, { status: 401 });
    }
    return realFetch(input as RequestInfo, init);
  });
}

async function run(sql: string, ...binds: unknown[]) {
  await db
    .prepare(sql)
    .bind(...binds)
    .run();
}

interface TestUser {
  id: string;
  username: string;
  token: string;
}

/** A fresh user (a word-only username, so `@username` mentions parse). */
async function newUser(name: string): Promise<TestUser> {
  const tag = crypto.randomUUID().replaceAll("-", "").slice(0, 10);
  const id = `${name}-${tag}`;
  const username = `${name}${tag}`;
  await run(
    `INSERT INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)`,
    id,
    username,
    name,
    new Date().toISOString(),
  );
  const token = `token-${id}`;
  sessions.set(token, id);
  return { id, username, token };
}

/** An existing 1:1 DM between two users (made before any block). */
async function dmBetween(a: TestUser, b: TestUser): Promise<string> {
  const channelId = `dm-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type, position, created_at)
     VALUES (?, NULL, ?, 'dm', 0, ?)`,
    channelId,
    `DM-${a.id}-${b.id}`,
    new Date().toISOString(),
  );
  await run(
    `INSERT INTO dm_recipients (channel_id, user_id) VALUES (?, ?), (?, ?)`,
    channelId,
    a.id,
    channelId,
    b.id,
  );
  return channelId;
}

async function insertMessage(channelId: string, author: TestUser) {
  const id = crypto.randomUUID();
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, created_at, content_revision)
     VALUES (?, ?, ?, 'hello', ?, 1)`,
    id,
    channelId,
    author.id,
    new Date().toISOString(),
  );
  return id;
}

/** Calls a route handler as `user`. */
async function call(
  route: { options: unknown },
  method: string,
  user: TestUser,
  path: string,
  body?: unknown,
  params?: Record<string, string>,
) {
  const headers = new Headers({ authorization: `Bearer ${user.token}` });
  // Routes that read the current request's headers (requireAuth()).
  setTestRequestHeaders(headers);
  if (body !== undefined) headers.set("content-type", "application/json");
  const response = await handler(
    route,
    method,
  )({
    request: new Request(`${BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
    params,
  });
  return {
    status: response.status,
    body: (await response.json().catch(() => null)) as Record<
      string,
      unknown
    > | null,
  };
}

const block = (blocker: TestUser, target: TestUser) =>
  call(FriendsRoute, "PUT", blocker, "/api/friends", {
    target_user_id: target.id,
    action: "block",
  });

const openDm = (user: TestUser, target: TestUser) =>
  call(DmRoute, "POST", user, "/api/dms", { target_user_id: target.id });

const send = (user: TestUser, channelId: string, content = "hi") =>
  call(
    MessagesRoute,
    "POST",
    user,
    `/api/channels/${channelId}/messages`,
    { content },
    { id: channelId },
  );

const react = (user: TestUser, channelId: string, messageId: string) =>
  call(
    ReactionsRoute,
    "PUT",
    user,
    `/api/channels/${channelId}/reactions`,
    { message_id: messageId, emoji: "👍" },
    { id: channelId },
  );

beforeAll(async () => {
  const statements = schema
    .split("\n")
    .map((line) => line.replace(/--.*$/, ""))
    .join("\n")
    .split(";")
    .map((statement) => statement.trim())
    .filter(Boolean);
  for (const statement of statements) {
    await db.prepare(statement).run();
  }
});

beforeEach(() => {
  fakeKova();
});

afterEach(() => {
  vi.restoreAllMocks();
  setTestRequestHeaders(null);
});

// ── The chat gateway ───────────────────────────────────────────────────────

async function openGateway(
  user: TestUser,
  roomName = "dm-permission-gateway",
): Promise<WebSocket> {
  const ticket = await issueSocketTicket(
    {
      accessMode: "authenticated",
      audience: "global",
      expiresAt: Date.now() + 60_000,
      nonce: crypto.randomUUID(),
      roomSlug: "global-gateway",
      subject: user.id,
    },
    env.CALLS_APP_SECRET,
  );
  const verification = await verifySocketTicket(ticket, env.CALLS_APP_SECRET, {
    accessMode: "authenticated",
    audience: "global",
    now: Date.now(),
    roomSlug: "global-gateway",
  });
  if (!verification.ok) throw new Error("Expected test ticket to verify");
  const headers = appendRealtimeAdmissionHeaders(
    new Headers({
      Upgrade: "websocket",
      "Sec-WebSocket-Protocol": "ralph.realtime.v1",
    }),
    await createRealtimeAdmissionContext(verification.claims),
  );
  const room = env.MEETING_ROOM.get(env.MEETING_ROOM.idFromName(roomName));
  const response = await room.fetch("https://internal/api/gateway?v=1", {
    headers,
  });
  expect(response.status).toBe(101);
  const socket = response.webSocket;
  if (!socket) throw new Error("Expected WebSocket upgrade response");
  socket.accept();
  await nextOp(socket, 8);
  const ready = nextOp(socket, 2);
  socket.send(JSON.stringify({ op: 0, d: { name: user.username } }));
  await ready;
  return socket;
}

async function nextOp(
  socket: WebSocket,
  opcode: number,
): Promise<{ op: number; d: Record<string, unknown> }> {
  while (true) {
    const message = await new Promise<MessageEvent<string>>((resolve) => {
      socket.addEventListener("message", resolve, { once: true });
    });
    const parsed = JSON.parse(message.data) as {
      op: number;
      d: Record<string, unknown>;
    };
    if (parsed.op === opcode) return parsed;
  }
}

async function nextRingStop(socket: WebSocket) {
  while (true) {
    const message = await nextOp(socket, 19);
    if (message.d.event === "CALL_RING_STOP") return message.d;
  }
}

const NOT_ACCEPTING = "NOT_ACCEPTING_DMS";

const setDmPermission = (user: TestUser, value: unknown) =>
  call(ProfileRoute, "PATCH", user, "/api/update-profile", {
    dmPermission: value,
  });

async function friends(a: TestUser, b: TestUser) {
  const now = new Date().toISOString();
  await run(
    `INSERT INTO relationships (user_id, target_user_id, type, created_at, updated_at)
     VALUES (?, ?, 0, ?, ?), (?, ?, 0, ?, ?)`,
    a.id,
    b.id,
    now,
    now,
    b.id,
    a.id,
    now,
    now,
  );
}

/** A server owned by the first user with everyone given as members. */
async function shareServer(...members: TestUser[]) {
  const serverId = `server-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES (?, 'DMs', ?)`,
    serverId,
    members[0]!.id,
  );
  for (const member of members) {
    await run(
      `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
      serverId,
      member.id,
    );
  }
  return serverId;
}

async function storedPermission(user: TestUser) {
  const row = await db
    .prepare(`SELECT dm_permission FROM users WHERE id = ?`)
    .bind(user.id)
    .first<{ dm_permission: string }>();
  return row?.dm_permission;
}

describe("the setting", () => {
  it("defaults to everyone, so new DMs open as before", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    expect(await storedPermission(bob)).toBe("everyone");
    expect((await openDm(alice, bob)).status).toBe(201);
  });

  it("is saved through update-profile and read back from /api/users/me", async () => {
    const bob = await newUser("bob");
    const saved = await setDmPermission(bob, "friends_and_servers");
    expect(saved.status).toBe(200);
    expect(saved.body).toMatchObject({
      user: { dm_permission: "friends_and_servers" },
    });
    expect(await storedPermission(bob)).toBe("friends_and_servers");

    const me = await call(MeRoute, "GET", bob, "/api/users/me");
    expect(me.status).toBe(200);
    expect(JSON.stringify(me.body)).toContain(
      '"dm_permission":"friends_and_servers"',
    );
  });

  it("refuses an unknown value and keeps the old one", async () => {
    const bob = await newUser("bob");
    await setDmPermission(bob, "nobody");
    const refused = await setDmPermission(bob, "admins");
    expect(refused.status).toBe(400);
    expect(await storedPermission(bob)).toBe("nobody");
  });

  it("is never shown on the user's profile to someone else", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await setDmPermission(bob, "nobody");
    const profile = await call(
      UserProfileRoute,
      "GET",
      alice,
      `/api/users/${bob.id}/profile`,
      undefined,
      { id: bob.id },
    );
    expect(profile.status).toBe(200);
    expect(JSON.stringify(profile.body)).not.toContain("dm_permission");

    const own = await call(
      UserProfileRoute,
      "GET",
      bob,
      `/api/users/${bob.id}/profile`,
      undefined,
      { id: bob.id },
    );
    expect(JSON.stringify(own.body)).toContain('"dm_permission":"nobody"');
  });

  it("is never sent with the recipient of a new DM", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const opened = await openDm(alice, bob);
    expect(JSON.stringify(opened.body)).not.toContain("dm_permission");
  });
});

describe("new DMs", () => {
  it("friends_and_servers lets friends and server members in, and no one else", async () => {
    const bob = await newUser("bob");
    const friend = await newUser("friend");
    const member = await newUser("member");
    const stranger = await newUser("stranger");
    await setDmPermission(bob, "friends_and_servers");
    await friends(bob, friend);
    await shareServer(member, bob);

    expect((await openDm(friend, bob)).status).toBe(201);
    expect((await openDm(member, bob)).status).toBe(201);

    const refused = await openDm(stranger, bob);
    expect(refused.status).toBe(403);
    expect(refused.body).toEqual({
      error: "bob isn't accepting messages from you.",
      code: NOT_ACCEPTING,
    });
  });

  it("nobody refuses everyone, friends and the server's owner included", async () => {
    const bob = await newUser("bob");
    const friend = await newUser("friend");
    const owner = await newUser("owner");
    await setDmPermission(bob, "nobody");
    await friends(bob, friend);
    // The owner of a server bob is in: no admin bypass.
    await shareServer(owner, bob);

    for (const sender of [friend, owner]) {
      const refused = await openDm(sender, bob);
      expect(refused.status).toBe(403);
      expect(refused.body?.code).toBe(NOT_ACCEPTING);
    }
    const channels = await db
      .prepare(`SELECT COUNT(*) AS n FROM dm_recipients WHERE user_id = ?`)
      .bind(bob.id)
      .first<{ n: number }>();
    expect(channels?.n).toBe(0);
  });

  it("only limits who reaches the owner, not who the owner reaches", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await setDmPermission(bob, "nobody");
    expect((await openDm(bob, alice)).status).toBe(201);
  });

  it("lets a block win over everyone", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await block(bob, alice);
    const refused = await openDm(alice, bob);
    expect(refused.status).toBe(403);
    expect(refused.body?.code).toBe("BLOCKED_BY_USER");
  });

  it("still opens an existing DM to read it", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await setDmPermission(bob, "nobody");
    const opened = await openDm(alice, bob);
    expect(opened.status).toBe(200);
    expect(opened.body).toMatchObject({ id: channelId });
  });
});

describe("existing DMs", () => {
  it("refuses the first message into a DM the recipient never wrote in", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await insertMessage(channelId, alice);
    await setDmPermission(bob, "nobody");

    const refused = await send(alice, channelId);
    expect(refused.status).toBe(403);
    expect(refused.body).toMatchObject({ code: NOT_ACCEPTING });
    const stored = await db
      .prepare(`SELECT COUNT(*) AS n FROM messages WHERE channel_id = ?`)
      .bind(channelId)
      .first<{ n: number }>();
    expect(stored?.n).toBe(1);
  });

  it("keeps a DM the recipient wrote in open under every setting", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await insertMessage(channelId, bob);
    await setDmPermission(bob, "nobody");

    const sent = await send(alice, channelId);
    expect(sent.body?.code).not.toBe(NOT_ACCEPTING);
    expect(sent.status).toBeLessThan(300);
  });

  it("lets the owner write first, which opens the DM to the reply", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await setDmPermission(bob, "nobody");
    const opened = await openDm(bob, alice);
    const channelId = (opened.body as { id: string }).id;
    expect((await send(alice, channelId)).body?.code).toBe(NOT_ACCEPTING);
    expect((await send(bob, channelId)).status).toBeLessThan(300);
    expect((await send(alice, channelId)).status).toBeLessThan(300);
  });

  it("refuses reactions too", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, alice);
    await setDmPermission(bob, "friends_and_servers");
    const refused = await react(alice, channelId, messageId);
    expect(refused.status).toBe(403);
    expect(refused.body?.code).toBe(NOT_ACCEPTING);
  });

  it("opens again once the two share a server", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await setDmPermission(bob, "friends_and_servers");
    expect((await send(alice, channelId)).body?.code).toBe(NOT_ACCEPTING);
    await shareServer(bob, alice);
    expect((await send(alice, channelId)).status).toBeLessThan(300);
  });

  it("counts a call the recipient placed as writing in the DM", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await setDmPermission(bob, "nobody");
    // A call record is stored as the caller's message (call.service.ts).
    await run(
      `INSERT INTO messages (id, channel_id, author_id, content, created_at, content_revision, type, metadata)
       VALUES (?, ?, ?, 'Missed call', ?, 1, 'call', '{}')`,
      crypto.randomUUID(),
      channelId,
      bob.id,
      new Date().toISOString(),
    );
    expect((await send(alice, channelId)).status).toBeLessThan(300);
  });

  it("refuses pins too (a pin notifies the recipient)", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, alice);
    await setDmPermission(bob, "nobody");
    const pinned = await call(
      PinsRoute,
      "PUT",
      alice,
      `/api/channels/${channelId}/pins`,
      { message_id: messageId, pinned: true },
      { id: channelId },
    );
    expect(pinned.status).toBe(403);
    expect(pinned.body?.code).toBe(NOT_ACCEPTING);
  });

  it("reports a block before the setting", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await setDmPermission(bob, "nobody");
    await block(bob, alice);
    expect((await send(alice, channelId)).body?.code).toBe("BLOCKED_BY_USER");
  });
});

describe("chat gateway", () => {
  it("refuses a message and a reaction sent over the socket", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, alice);
    await setDmPermission(bob, "nobody");
    const socket = await openGateway(alice);

    const refused = nextOp(socket, 18);
    socket.send(
      JSON.stringify({ op: 20, d: { channel_id: channelId, content: "hi" } }),
    );
    await expect(refused).resolves.toMatchObject({
      d: { code: 4003, message: "bob isn't accepting messages from you." },
    });

    const reaction = nextOp(socket, 18);
    socket.send(
      JSON.stringify({
        op: 24,
        d: { channel_id: channelId, message_id: messageId, emoji: "👍" },
      }),
    );
    await expect(reaction).resolves.toMatchObject({ d: { code: 4003 } });
    socket.close();
  });

  it("refuses a ring with the not_accepting reason and its sentence", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await setDmPermission(bob, "nobody");
    const aliceSocket = await openGateway(alice);
    const bobSocket = await openGateway(bob);

    const stop = nextRingStop(aliceSocket);
    aliceSocket.send(
      JSON.stringify({
        op: 36,
        d: { channel_id: channelId, target_user_id: bob.id },
      }),
    );
    await expect(stop).resolves.toMatchObject({
      data: {
        call_id: null,
        reason: "not_accepting",
        message: "bob isn't accepting calls from you.",
      },
    });

    // Bob ringing alice is his own choice: his setting does not stop it.
    const ringing = nextOp(aliceSocket, 19);
    bobSocket.send(
      JSON.stringify({
        op: 36,
        d: { channel_id: channelId, target_user_id: alice.id },
      }),
    );
    await expect(ringing).resolves.toMatchObject({
      d: { event: "CALL_RING" },
    });

    aliceSocket.close();
    bobSocket.close();
  });
});
