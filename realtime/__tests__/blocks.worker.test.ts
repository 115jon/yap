// Blocks against miniflare's D1, the real route handlers and the real chat
// gateway: a block either way stops a new DM, closes an existing 1:1 DM to
// messages, reactions and rings, refuses friend requests, survives the
// blocked user "removing" the blocker, and silences the blocked user's
// mentions, replies and DMs for the blocker. Kova is faked at its
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
import { memoryAuth, testServices } from "../../src/server/testing";
import { socketTicketRoutes } from "../../src/server/routes/socket-ticket";
import { generateMessageNotifications } from "../../src/services/message.service";
import {
  acceptFriendRequest,
  blockUser,
  sendFriendRequest,
} from "../../src/services/social.service";
import { dmCallRoomSlug } from "../../src/services/user-sockets.service";
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

const removeRelationship = (user: TestUser, target: TestUser) =>
  call(FriendsRoute, "DELETE", user, "/api/friends", {
    target_user_id: target.id,
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

async function blockRow(blocker: TestUser, target: TestUser) {
  return db
    .prepare(
      `SELECT type FROM relationships WHERE user_id = ? AND target_user_id = ?`,
    )
    .bind(blocker.id, target.id)
    .first<{ type: number }>();
}

const YOU_CANT = {
  code: "BLOCKED_BY_USER",
  error: "You can't message this user.",
};
const UNBLOCK_FIRST = {
  code: "USER_BLOCKED",
  error: "You blocked this user. Unblock to message.",
};

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

describe("new DMs", () => {
  it("opens a DM when neither side blocked the other", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    expect((await openDm(alice, bob)).status).toBe(201);
  });

  it("refuses a new DM either way once one side blocked the other", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    expect((await block(bob, alice)).status).toBe(200);

    const blocked = await openDm(alice, bob);
    expect(blocked.status).toBe(403);
    expect(blocked.body).toMatchObject(YOU_CANT);

    const blocker = await openDm(bob, alice);
    expect(blocker.status).toBe(403);
    expect(blocker.body).toMatchObject(UNBLOCK_FIRST);

    const channels = await db
      .prepare(
        `SELECT COUNT(*) AS n FROM dm_recipients a
           JOIN dm_recipients b ON a.channel_id = b.channel_id
          WHERE a.user_id = ? AND b.user_id = ?`,
      )
      .bind(alice.id, bob.id)
      .first<{ n: number }>();
    expect(channels?.n).toBe(0);
  });
});

describe("existing DMs", () => {
  it("refuses messages from both sides with the copy for each", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await block(bob, alice);

    const fromBlocked = await send(alice, channelId);
    expect(fromBlocked.status).toBe(403);
    expect(fromBlocked.body).toMatchObject(YOU_CANT);

    const fromBlocker = await send(bob, channelId);
    expect(fromBlocker.status).toBe(403);
    expect(fromBlocker.body).toMatchObject(UNBLOCK_FIRST);

    const stored = await db
      .prepare(`SELECT COUNT(*) AS n FROM messages WHERE channel_id = ?`)
      .bind(channelId)
      .first<{ n: number }>();
    expect(stored?.n).toBe(0);
  });

  it("refuses new reactions", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, bob);
    await block(bob, alice);

    const reacted = await react(alice, channelId, messageId);
    expect(reacted.status).toBe(403);
    expect(reacted.body).toMatchObject(YOU_CANT);
    expect((await react(bob, channelId, messageId)).body).toMatchObject(
      UNBLOCK_FIRST,
    );

    const stored = await db
      .prepare(
        `SELECT COUNT(*) AS n FROM message_reactions WHERE message_id = ?`,
      )
      .bind(messageId)
      .first<{ n: number }>();
    expect(stored?.n).toBe(0);
  });

  it("opens again for sending after the blocker unblocks", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await block(bob, alice);
    expect((await send(alice, channelId)).status).toBe(403);

    expect((await removeRelationship(bob, alice)).status).toBe(200);
    // The DM is no longer refused (the send goes on past the block check).
    const after = await send(alice, channelId);
    expect(after.body?.code).not.toBe("BLOCKED_BY_USER");
    expect(after.body?.code).not.toBe("USER_BLOCKED");
  });
});

describe("friend requests and the block itself", () => {
  it("refuses a friend request from the blocked user, and to them", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await block(bob, alice);

    const fromBlocked = await call(
      FriendsRoute,
      "POST",
      alice,
      "/api/friends",
      {
        username: bob.username,
      },
    );
    expect(fromBlocked.status).toBe(403);
    expect(fromBlocked.body?.code).toBe("BLOCKED_BY_USER");

    const fromBlocker = await call(FriendsRoute, "POST", bob, "/api/friends", {
      username: alice.username,
    });
    expect(fromBlocker.status).toBe(409);
    expect(fromBlocker.body?.code).toBe("USER_BLOCKED");

    expect(await blockRow(alice, bob)).toBeNull();
    expect(await blockRow(bob, alice)).toEqual({ type: 1 });
  });

  it("is not lifted by the blocked user removing the blocker", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await block(bob, alice);

    expect((await removeRelationship(alice, bob)).status).toBe(200);
    expect(await blockRow(bob, alice)).toEqual({ type: 1 });
    expect((await send(alice, channelId)).body).toMatchObject(YOU_CANT);
  });

  it("keeps both blocks when both sides block, and each unblocks alone", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await block(bob, alice);
    await block(alice, bob);
    expect(await blockRow(bob, alice)).toEqual({ type: 1 });
    expect(await blockRow(alice, bob)).toEqual({ type: 1 });

    // Each is asked to unblock first while their own block stands.
    expect((await send(alice, channelId)).body).toMatchObject(UNBLOCK_FIRST);

    await removeRelationship(alice, bob);
    expect(await blockRow(bob, alice)).toEqual({ type: 1 });
    expect((await send(alice, channelId)).body).toMatchObject(YOU_CANT);
  });
});

describe("notifications", () => {
  async function serverChannel(...members: TestUser[]) {
    const serverId = `server-${crypto.randomUUID()}`;
    const channelId = `text-${crypto.randomUUID()}`;
    await run(
      `INSERT INTO servers (id, name, owner_id) VALUES (?, 'Blocks', ?)`,
      serverId,
      members[0]!.id,
    );
    await run(
      `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, ?, 'general', 'text')`,
      channelId,
      serverId,
    );
    for (const member of members) {
      await run(
        `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
        serverId,
        member.id,
      );
    }
    return channelId;
  }

  /** Posts a real message (notifications need it) and notifies for it. */
  const notify = async (
    channelId: string,
    author: TestUser,
    content: string,
    replyToId?: string,
  ) => {
    const messageId = crypto.randomUUID();
    await run(
      `INSERT INTO messages (id, channel_id, author_id, content, reply_to_id, created_at, content_revision)
       VALUES (?, ?, ?, ?, ?, ?, 1)`,
      messageId,
      channelId,
      author.id,
      content,
      replyToId ?? null,
      new Date().toISOString(),
    );
    return generateMessageNotifications(db, () => crypto.randomUUID(), {
      channelId,
      messageId,
      authorId: author.id,
      authorUsername: author.username,
      authorDisplayName: null,
      authorAvatarUrl: null,
      content,
      replyToId,
      contentRevision: 1,
    });
  };

  async function inbox(user: TestUser) {
    const row = await db
      .prepare(`SELECT COUNT(*) AS n FROM notifications WHERE user_id = ?`)
      .bind(user.id)
      .first<{ n: number }>();
    return row?.n ?? 0;
  }

  it("drops a blocked user's mentions and replies for the blocker only", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const carol = await newUser("carol");
    const channelId = await serverChannel(alice, bob, carol);
    await block(bob, alice);

    const mentions = await notify(
      channelId,
      alice,
      `hey @${bob.username} and @${carol.username}`,
    );
    expect(mentions.map((n) => n.userId)).toEqual([carol.id]);
    // The broadcast carries the push; nothing for bob means no push either.
    expect(mentions.every((n) => n.userId !== bob.id)).toBe(true);

    const bobsMessage = await insertMessage(channelId, bob);
    const reply = await notify(channelId, alice, "replying", bobsMessage);
    expect(reply).toEqual([]);

    expect(await inbox(bob)).toBe(0);
    expect(await inbox(carol)).toBe(1);
  });

  it("drops a DM notification from someone the recipient blocked", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    // Before the block the DM notifies (and would push).
    const before = await notify(channelId, alice, "before");
    expect(before.map((n) => [n.userId, n.push?.type])).toEqual([
      [bob.id, "dm"],
    ]);
    await block(bob, alice);

    expect(await notify(channelId, alice, "hello")).toEqual([]);
    expect(await inbox(bob)).toBe(1);
  });

  it("still notifies the blocked user about the blocker's mentions", async () => {
    // A block silences the blocked user for the blocker, not the reverse.
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await serverChannel(alice, bob);
    await block(bob, alice);

    const sent = await notify(channelId, bob, `@${alice.username} hi`);
    expect(sent.map((n) => n.userId)).toEqual([alice.id]);
  });
});

// ── The chat gateway ───────────────────────────────────────────────────────

async function openGateway(
  user: TestUser,
  roomName = "blocks-gateway",
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

describe("chat gateway", () => {
  it("refuses messages and reactions sent over the socket", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, bob);
    await block(bob, alice);
    const socket = await openGateway(alice);

    const refused = nextOp(socket, 18);
    socket.send(
      JSON.stringify({ op: 20, d: { channel_id: channelId, content: "hi" } }),
    );
    await expect(refused).resolves.toMatchObject({
      d: { code: 4003, message: YOU_CANT.error },
    });

    const reaction = nextOp(socket, 18);
    socket.send(
      JSON.stringify({
        op: 24,
        d: { channel_id: channelId, message_id: messageId, emoji: "👍" },
      }),
    );
    await expect(reaction).resolves.toMatchObject({
      d: { code: 4003, message: YOU_CANT.error },
    });

    const stored = await db
      .prepare(
        `SELECT
           (SELECT COUNT(*) FROM messages WHERE channel_id = ?) AS messages,
           (SELECT COUNT(*) FROM message_reactions WHERE message_id = ?) AS reactions`,
      )
      .bind(channelId, messageId)
      .first<{ messages: number; reactions: number }>();
    expect(stored).toEqual({ messages: 1, reactions: 0 });
    socket.close();
  });

  it("refuses a ring either way with the blocked reason", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    await block(bob, alice);
    const aliceSocket = await openGateway(alice);
    const bobSocket = await openGateway(bob);

    for (const [socket, target] of [
      [aliceSocket, bob],
      [bobSocket, alice],
    ] as const) {
      const stop = nextRingStop(socket);
      socket.send(
        JSON.stringify({
          op: 36,
          d: { channel_id: channelId, target_user_id: target.id },
        }),
      );
      await expect(stop).resolves.toMatchObject({
        data: { call_id: null, reason: "blocked" },
      });
    }

    aliceSocket.close();
    bobSocket.close();
  });
});

// ── Edits and pins ─────────────────────────────────────────────────────────

const PinsRoute = (await loadRoute("../../src/routes/api/channels/$id/pins"))
  .Route;

const edit = (
  user: TestUser,
  channelId: string,
  body: Record<string, unknown>,
) =>
  call(
    MessagesRoute,
    "PATCH",
    user,
    `/api/channels/${channelId}/messages`,
    body,
    { id: channelId },
  );

const pin = (
  user: TestUser,
  channelId: string,
  messageId: string,
  pinned = true,
) =>
  call(
    PinsRoute,
    "PUT",
    user,
    `/api/channels/${channelId}/pins`,
    { message_id: messageId, pinned },
    { id: channelId },
  );

async function storedMessage(messageId: string) {
  return db
    .prepare(
      `SELECT content, embeds, is_pinned, content_revision FROM messages WHERE id = ?`,
    )
    .bind(messageId)
    .first<{
      content: string;
      embeds: string | null;
      is_pinned: number;
      content_revision: number;
    }>();
}

describe("edits and pins in a blocked DM", () => {
  it("refuses content edits and embed removal over REST", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const alicesMessage = await insertMessage(channelId, alice);
    const bobsMessage = await insertMessage(channelId, bob);
    await run(
      `UPDATE messages SET embeds = ? WHERE id = ?`,
      JSON.stringify([{ url: "https://example.com", title: "Example" }]),
      alicesMessage,
    );
    await block(bob, alice);

    const content = await edit(alice, channelId, {
      message_id: alicesMessage,
      content: "edited",
    });
    expect(content.status).toBe(403);
    expect(content.body).toMatchObject(YOU_CANT);

    const embeds = await edit(alice, channelId, {
      message_id: alicesMessage,
      embeds: [],
    });
    expect(embeds.status).toBe(403);
    expect(embeds.body).toMatchObject(YOU_CANT);

    const fromBlocker = await edit(bob, channelId, {
      message_id: bobsMessage,
      content: "edited",
    });
    expect(fromBlocker.status).toBe(403);
    expect(fromBlocker.body).toMatchObject(UNBLOCK_FIRST);

    expect(await storedMessage(alicesMessage)).toMatchObject({
      content: "hello",
      content_revision: 1,
    });
    expect((await storedMessage(alicesMessage))?.embeds).toContain("Example");
    expect(await storedMessage(bobsMessage)).toMatchObject({
      content: "hello",
      content_revision: 1,
    });
  });

  it("still edits in a DM with no block", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, alice);

    const edited = await edit(alice, channelId, {
      message_id: messageId,
      content: "edited",
    });
    expect(edited.status).toBe(200);
    expect((await storedMessage(messageId))?.content).toBe("edited");
  });

  it("refuses an edit sent over the socket (op 21)", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, alice);
    await block(bob, alice);
    const socket = await openGateway(alice);

    const refused = nextOp(socket, 18);
    socket.send(
      JSON.stringify({
        op: 21,
        d: { message_id: messageId, content: "edited" },
      }),
    );
    await expect(refused).resolves.toMatchObject({
      d: { code: 4003, message: YOU_CANT.error },
    });
    expect(await storedMessage(messageId)).toMatchObject({
      content: "hello",
      content_revision: 1,
    });
    socket.close();
  });

  it("refuses pinning and unpinning from both sides", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const pinnedId = await insertMessage(channelId, bob);
    const loose = await insertMessage(channelId, bob);
    expect((await pin(alice, channelId, pinnedId)).status).toBe(200);
    await block(bob, alice);

    const pinned = await pin(alice, channelId, loose);
    expect(pinned.status).toBe(403);
    expect(pinned.body).toMatchObject(YOU_CANT);
    const unpinned = await pin(bob, channelId, pinnedId, false);
    expect(unpinned.status).toBe(403);
    expect(unpinned.body).toMatchObject(UNBLOCK_FIRST);

    expect((await storedMessage(loose))?.is_pinned).toBe(0);
    expect((await storedMessage(pinnedId))?.is_pinned).toBe(1);
  });

  it("refuses a pin from someone outside the DM", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const mallory = await newUser("mallory");
    const channelId = await dmBetween(alice, bob);
    const messageId = await insertMessage(channelId, bob);

    const outsider = await pin(mallory, channelId, messageId);
    expect(outsider.status).toBe(403);
    expect((await storedMessage(messageId))?.is_pinned).toBe(0);
  });
});

// ── The DM call room's admission ───────────────────────────────────────────

describe("DM call room admission", () => {
  const ticketRoutes = (user: TestUser) =>
    socketTicketRoutes(
      testServices({
        auth: memoryAuth({ [`Bearer ${user.token}`]: user.id }),
        db,
        env: {
          CALLS_APP_SECRET: env.CALLS_APP_SECRET,
          REALTIME_TICKET_SECRET: env.REALTIME_TICKET_SECRET,
        },
      }),
    );

  async function admit(user: TestUser, channelId: string, roomSlug: string) {
    const response = await ticketRoutes(user).POST({
      request: new Request(`${BASE}/api/voice/socket-ticket`, {
        method: "POST",
        headers: {
          authorization: `Bearer ${user.token}`,
          "content-type": "application/json",
        },
        body: JSON.stringify({ audience: "voice", channelId, roomSlug }),
      }),
    });
    return {
      status: response.status,
      body: (await response.json()) as Record<string, unknown>,
    };
  }

  it("admits both members until one blocks the other, then neither", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const slug = dmCallRoomSlug(alice.id, bob.id);

    expect(await admit(alice, channelId, slug)).toMatchObject({
      status: 200,
      body: { room_slug: slug },
    });
    expect((await admit(bob, channelId, slug)).status).toBe(200);

    await block(bob, alice);

    expect(await admit(alice, channelId, slug)).toMatchObject({
      status: 403,
      body: { code: "BLOCKED_BY_USER", error: "You can't call this user." },
    });
    expect(await admit(bob, channelId, slug)).toMatchObject({
      status: 403,
      body: {
        code: "USER_BLOCKED",
        error: "You blocked this user. Unblock to call.",
      },
    });

    // Unblocked: the room opens again.
    await removeRelationship(bob, alice);
    expect((await admit(alice, channelId, slug)).status).toBe(200);
  });
});

// ── Rings and calls ended by a block ───────────────────────────────────────

/** A gateway socket on the global gateway, where the block route reaches. */
async function openGlobalGateway(user: TestUser) {
  return openGateway(user, "global-gateway");
}

async function ring(caller: WebSocket, channelId: string, callee: TestUser) {
  const ringing = nextDispatch(caller, "CALL_RINGING");
  caller.send(
    JSON.stringify({
      op: 36,
      d: { channel_id: channelId, target_user_id: callee.id },
    }),
  );
  return (await ringing).data as { call_id: string };
}

async function nextDispatch(socket: WebSocket, event: string) {
  while (true) {
    const message = await nextOp(socket, 19);
    if (message.d.event === event) return message.d;
  }
}

async function callRecords(channelId: string) {
  const row = await db
    .prepare(
      `SELECT COUNT(*) AS n FROM messages WHERE channel_id = ? AND type = 'call'`,
    )
    .bind(channelId)
    .first<{ n: number }>();
  return row?.n ?? 0;
}

describe("rings and calls across a new block", () => {
  it("stops a ringing call for both when either side blocks", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const aliceSocket = await openGlobalGateway(alice);
    const bobSocket = await openGlobalGateway(bob);

    const incoming = nextDispatch(bobSocket, "CALL_RING");
    const { call_id } = await ring(aliceSocket, channelId, bob);
    await incoming;

    const callerStop = nextRingStop(aliceSocket);
    const calleeStop = nextRingStop(bobSocket);
    expect((await block(bob, alice)).status).toBe(200);
    await expect(callerStop).resolves.toMatchObject({
      data: { call_id, reason: "blocked" },
    });
    await expect(calleeStop).resolves.toMatchObject({
      data: { call_id, reason: "blocked" },
    });

    // The ring is gone: answering it now finds nothing to accept.
    const late = nextRingStop(bobSocket);
    bobSocket.send(JSON.stringify({ op: 37, d: { call_id } }));
    await expect(late).resolves.toMatchObject({
      data: { call_id, reason: "expired" },
    });
    // No missed-call record from someone just blocked.
    expect(await callRecords(channelId)).toBe(0);

    aliceSocket.close();
    bobSocket.close();
  });

  it("settles an answer as blocked when a block landed while it rang", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const aliceSocket = await openGlobalGateway(alice);
    const bobSocket = await openGlobalGateway(bob);

    const incoming = nextDispatch(bobSocket, "CALL_RING");
    const { call_id } = await ring(aliceSocket, channelId, bob);
    await incoming;

    // The block row lands without the route (another device, a race).
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type, created_at, updated_at)
       VALUES (?, ?, 1, ?, ?)`,
      bob.id,
      alice.id,
      new Date().toISOString(),
      new Date().toISOString(),
    );

    const callerStop = nextRingStop(aliceSocket);
    const calleeStop = nextRingStop(bobSocket);
    bobSocket.send(JSON.stringify({ op: 37, d: { call_id } }));
    await expect(callerStop).resolves.toMatchObject({
      data: { call_id, reason: "blocked" },
    });
    await expect(calleeStop).resolves.toMatchObject({
      data: { call_id, reason: "blocked" },
    });
    expect(await callRecords(channelId)).toBe(0);

    aliceSocket.close();
    bobSocket.close();
  });

  it("ends a running DM call: both are put out of its room", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const channelId = await dmBetween(alice, bob);
    const slug = dmCallRoomSlug(alice.id, bob.id);
    const aliceRoom = await openCallRoom(slug, channelId, alice);
    const bobRoom = await openCallRoom(slug, channelId, bob);

    const aliceClosed = closeOf(aliceRoom);
    const bobClosed = closeOf(bobRoom);
    expect((await block(alice, bob)).status).toBe(200);

    // AdmissionRejected: the clients must not reconnect.
    await expect(aliceClosed).resolves.toBe(4008);
    await expect(bobClosed).resolves.toBe(4008);
  });
});

async function openCallRoom(slug: string, channelId: string, user: TestUser) {
  const ticket = await issueSocketTicket(
    {
      accessMode: "authenticated",
      audience: "room",
      channelId,
      expiresAt: Date.now() + 60_000,
      nonce: crypto.randomUUID(),
      roomSlug: slug,
      subject: user.id,
    },
    env.CALLS_APP_SECRET,
  );
  const verification = await verifySocketTicket(ticket, env.CALLS_APP_SECRET, {
    accessMode: "authenticated",
    audience: "room",
    now: Date.now(),
    roomSlug: slug,
  });
  if (!verification.ok) throw new Error("Expected test ticket to verify");
  const headers = appendRealtimeAdmissionHeaders(
    new Headers({
      Upgrade: "websocket",
      "Sec-WebSocket-Protocol": "ralph.realtime.v1",
    }),
    await createRealtimeAdmissionContext(verification.claims),
  );
  const room = env.MEETING_ROOM.get(env.MEETING_ROOM.idFromName(slug));
  const response = await room.fetch(
    `https://internal/api/channels/${slug}/ws?v=1`,
    { headers },
  );
  const socket = response.webSocket;
  if (!socket) {
    throw new Error(
      `Expected room WebSocket, got ${response.status} ${await response.text()}`,
    );
  }
  socket.accept();
  await nextOp(socket, 8);
  const ready = nextOp(socket, 2);
  socket.send(JSON.stringify({ op: 0, d: { name: user.username } }));
  await ready;
  return socket;
}

function closeOf(socket: WebSocket) {
  return new Promise<number>((resolve) => {
    socket.addEventListener("close", (event) => resolve(event.code), {
      once: true,
    });
  });
}

// ── The friend-accept race ─────────────────────────────────────────────────

describe("accepting a friend request a block raced", () => {
  /**
   * The database, with `beforeBatch` run just before the accept's batch:
   * what lands between the request's read and its UPDATEs.
   */
  function racing(beforeBatch: () => Promise<void>): D1Database {
    let raced = false;
    const wrapped: Pick<D1Database, "prepare" | "batch"> = {
      prepare: (query) => db.prepare(query),
      batch: async (statements) => {
        if (!raced) {
          raced = true;
          await beforeBatch();
        }
        return db.batch(statements);
      },
    };
    // The social service reads and writes through prepare and batch only.
    return wrapped as D1Database;
  }

  async function rows(a: TestUser, b: TestUser) {
    return {
      ab: (await blockRow(a, b))?.type ?? null,
      ba: (await blockRow(b, a))?.type ?? null,
    };
  }

  it("keeps the requester's block when they block before the accept lands", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    // alice asks bob: alice→bob 3, bob→alice 2.
    await call(FriendsRoute, "POST", alice, "/api/friends", {
      username: bob.username,
    });

    const accepting = acceptFriendRequest(
      racing(async () => {
        await blockUser(db, alice.id, bob.id);
      }),
      bob.id,
      alice.id,
    );
    await expect(accepting).rejects.toMatchObject({
      status: 403,
      code: "BLOCKED_BY_USER",
    });
    expect(await rows(alice, bob)).toEqual({ ab: 1, ba: null });
  });

  it("keeps the accepter's own block from another device", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await call(FriendsRoute, "POST", alice, "/api/friends", {
      username: bob.username,
    });

    await expect(
      acceptFriendRequest(
        racing(async () => {
          await blockUser(db, bob.id, alice.id);
        }),
        bob.id,
        alice.id,
      ),
    ).rejects.toMatchObject({ status: 409, code: "USER_BLOCKED" });
    expect(await rows(alice, bob)).toEqual({ ab: null, ba: 1 });
  });

  it("keeps the block when sending a request back auto-accepts", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await call(FriendsRoute, "POST", alice, "/api/friends", {
      username: bob.username,
    });

    // bob sends one back (an auto-accept) while alice blocks him.
    await expect(
      sendFriendRequest(
        racing(async () => {
          await blockUser(db, alice.id, bob.id);
        }),
        bob.id,
        alice.username,
      ),
    ).rejects.toMatchObject({ status: 403, code: "BLOCKED_BY_USER" });
    expect(await rows(alice, bob)).toEqual({ ab: 1, ba: null });
  });

  it("still accepts when nothing raced it", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    await call(FriendsRoute, "POST", alice, "/api/friends", {
      username: bob.username,
    });
    const accepted = await call(FriendsRoute, "PUT", bob, "/api/friends", {
      target_user_id: alice.id,
      action: "accept",
    });
    expect(accepted.status).toBe(200);
    expect(await rows(alice, bob)).toEqual({ ab: 0, ba: 0 });
  });
});
