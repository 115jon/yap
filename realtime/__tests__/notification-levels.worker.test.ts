// Notification levels and mutes against miniflare's D1 and the real route
// handler: the API stores and validates a user's level and mute per server,
// channel and DM; message postprocessing then leaves out the inbox entries
// and pushes those settings silence, and pushes every message to members who
// chose "all messages". Kova is faked at its get-session call.

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
import { PERMISSIONS } from "../../src/lib/permissions";
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import {
  generateAllMessagePushes,
  processMessagePostprocessingTask,
} from "../../src/services/message.service";
import type { MessagePush } from "../../src/services/push.service";
import { setTestRequestHeaders } from "../test-shims/react-start-server";

type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;
const SettingsRoute = (
  await loadRoute("../../src/routes/api/notification-settings")
).Route;

type Handler = (args: { request: Request }) => Promise<Response>;

function handler(method: string): Handler {
  const fn = (
    SettingsRoute.options as { server: { handlers: Record<string, Handler> } }
  ).server.handlers[method];
  if (!fn) throw new Error(`no ${method} handler`);
  return fn;
}

const db = env.DB as unknown as D1Database;
const BASE = "https://yap.test";
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

interface TestServer {
  serverId: string;
  channelId: string;
  otherChannelId: string;
}

/** A server whose @everyone can see and post in its two channels. */
async function newServer(
  owner: TestUser,
  members: TestUser[],
): Promise<TestServer> {
  const tag = crypto.randomUUID();
  const serverId = `srv-${tag}`;
  const roleId = `role-${tag}`;
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES (?, 'Lab', ?)`,
    serverId,
    owner.id,
  );
  await run(
    `INSERT INTO roles (id, server_id, name, permissions, is_default)
     VALUES (?, ?, '@everyone', ?, 1)`,
    roleId,
    serverId,
    PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES,
  );
  for (const member of [owner, ...members]) {
    await run(
      `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
      serverId,
      member.id,
    );
    await run(
      `INSERT INTO member_roles (server_id, user_id, role_id) VALUES (?, ?, ?)`,
      serverId,
      member.id,
      roleId,
    );
  }
  const channelId = `ch-${tag}`;
  const otherChannelId = `ch2-${tag}`;
  for (const [id, name] of [
    [channelId, "general"],
    [otherChannelId, "random"],
  ]) {
    await run(
      `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, ?, ?, 'text')`,
      id,
      serverId,
      name,
    );
  }
  return { serverId, channelId, otherChannelId };
}

async function dmBetween(a: TestUser, b: TestUser): Promise<string> {
  const channelId = `dm-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, NULL, 'dm', 'dm')`,
    channelId,
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

async function call(method: string, user: TestUser, body?: unknown) {
  const headers = new Headers({ authorization: `Bearer ${user.token}` });
  setTestRequestHeaders(headers);
  if (body !== undefined) headers.set("content-type", "application/json");
  const response = await handler(method)({
    request: new Request(`${BASE}/api/notification-settings`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
  });
  return {
    status: response.status,
    body: (await response.json().catch(() => null)) as Record<
      string,
      unknown
    > | null,
  };
}

const setting = (
  user: TestUser,
  target_type: "server" | "channel",
  target_id: string,
  change: { level?: string | null; mute?: string },
) => call("PUT", user, { target_type, target_id, ...change });

/** Posts a message and runs its postprocessing, returning the pushes. */
async function post(
  author: TestUser,
  channelId: string,
  content: string,
  replyToId?: string,
): Promise<{ messageId: string; pushes: MessagePush[]; inbox: string[] }> {
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
  const pushes: MessagePush[] = [];
  await processMessagePostprocessingTask(
    db,
    { messageId, channelId, revision: 1, notify: true },
    async () => {},
    {
      push: async (batch) => {
        pushes.push(...batch);
      },
    },
  );
  const { results } = await db
    .prepare(`SELECT user_id FROM notifications WHERE message_id = ?`)
    .bind(messageId)
    .all<{ user_id: string }>();
  return {
    messageId,
    pushes,
    inbox: (results ?? []).map((row) => row.user_id),
  };
}

const pushedTo = (pushes: MessagePush[]) =>
  pushes.map((push) => [push.recipientId, push.type]);

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
});

describe("the notification settings API", () => {
  it("stores a level and a mute and lists them", async () => {
    const alice = await newUser("alice");
    const lab = await newServer(alice, []);

    const put = await setting(alice, "channel", lab.channelId, {
      level: "all",
      mute: "1h",
    });
    expect(put.status).toBe(200);
    const stored = put.body?.setting as Record<string, unknown>;
    expect(stored).toMatchObject({
      target_type: "channel",
      target_id: lab.channelId,
      level: "all",
      muted: true,
    });
    const until = Date.parse(stored.muted_until as string);
    expect(until - Date.now()).toBeGreaterThan(59 * 60 * 1000);
    expect(until - Date.now()).toBeLessThanOrEqual(60 * 60 * 1000);

    await setting(alice, "server", lab.serverId, { mute: "forever" });
    const list = await call("GET", alice);
    expect(list.status).toBe(200);
    const settings = list.body?.settings as Record<string, unknown>[];
    expect(settings).toHaveLength(2);
    expect(settings).toContainEqual({
      target_type: "server",
      target_id: lab.serverId,
      level: null,
      muted: true,
      muted_until: null,
    });
  });

  it("keeps the other field when only one changes, and drops a row back to default", async () => {
    const alice = await newUser("alice");
    const lab = await newServer(alice, []);

    await setting(alice, "channel", lab.channelId, { level: "none" });
    const muted = await setting(alice, "channel", lab.channelId, {
      mute: "8h",
    });
    expect(muted.body?.setting).toMatchObject({ level: "none", muted: true });

    const unmuted = await setting(alice, "channel", lab.channelId, {
      mute: "off",
    });
    expect(unmuted.body?.setting).toMatchObject({
      level: "none",
      muted: false,
      muted_until: null,
    });

    await setting(alice, "channel", lab.channelId, { level: null });
    const row = await db
      .prepare(`SELECT 1 FROM notification_settings WHERE user_id = ?`)
      .bind(alice.id)
      .first();
    expect(row).toBeNull();
  });

  it("refuses servers and channels the user cannot see, and bad values", async () => {
    const alice = await newUser("alice");
    const mallory = await newUser("mallory");
    const bob = await newUser("bob");
    const lab = await newServer(alice, []);
    const dm = await dmBetween(alice, bob);

    expect(
      (await setting(mallory, "server", lab.serverId, { mute: "forever" }))
        .status,
    ).toBe(404);
    expect(
      (await setting(mallory, "channel", lab.channelId, { level: "all" }))
        .status,
    ).toBe(404);
    expect(
      (await setting(mallory, "channel", dm, { mute: "forever" })).status,
    ).toBe(404);
    expect(
      (await setting(bob, "channel", dm, { mute: "forever" })).status,
    ).toBe(200);
    expect(
      (await setting(alice, "channel", lab.channelId, { level: "loud" }))
        .status,
    ).toBe(400);
    expect(
      (await setting(alice, "channel", lab.channelId, { mute: "2h" })).status,
    ).toBe(400);
  });

  it("needs a signed-in user", async () => {
    const response = await call("GET", {
      id: "nobody",
      username: "nobody",
      token: "not-a-session",
    });
    expect(response.status).toBe(401);
  });
});

describe("mentions and replies in a server", () => {
  it("still notify in a muted channel or server, unless the level is none", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const lab = await newServer(alice, [bob]);

    await setting(bob, "channel", lab.channelId, { mute: "forever" });
    const mutedChannel = await post(
      alice,
      lab.channelId,
      `hey @${bob.username}`,
    );
    expect(mutedChannel.inbox).toEqual([bob.id]);
    expect(pushedTo(mutedChannel.pushes)).toEqual([[bob.id, "mention"]]);

    await setting(bob, "server", lab.serverId, { mute: "24h" });
    const mutedServer = await post(
      alice,
      lab.otherChannelId,
      `hey @${bob.username}`,
    );
    expect(pushedTo(mutedServer.pushes)).toEqual([[bob.id, "mention"]]);

    await setting(bob, "channel", lab.channelId, { level: "none" });
    const silenced = await post(alice, lab.channelId, `hey @${bob.username}`);
    expect(silenced.inbox).toEqual([]);
    expect(silenced.pushes).toEqual([]);
    const reply = await post(
      alice,
      lab.channelId,
      "a reply",
      (await post(bob, lab.channelId, "first")).messageId,
    );
    expect(reply.inbox).toEqual([]);
    expect(reply.pushes).toEqual([]);
  });

  it("take the server's level when the channel has none, and the channel's over it", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const lab = await newServer(alice, [bob]);

    await setting(bob, "server", lab.serverId, { level: "none" });
    expect(
      (await post(alice, lab.channelId, `@${bob.username}`)).pushes,
    ).toEqual([]);

    await setting(bob, "channel", lab.channelId, { level: "mentions" });
    expect(
      pushedTo((await post(alice, lab.channelId, `@${bob.username}`)).pushes),
    ).toEqual([[bob.id, "mention"]]);
    expect(
      (await post(alice, lab.otherChannelId, `@${bob.username}`)).pushes,
    ).toEqual([]);
  });

  it("notify after a timed mute and a none level are gone", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const lab = await newServer(alice, [bob]);

    // A mute that already ran out is not a mute.
    await run(
      `INSERT INTO notification_settings
         (user_id, target_type, target_id, level, muted, muted_until, updated_at)
       VALUES (?, 'channel', ?, 'all', 1, ?, ?)`,
      bob.id,
      lab.channelId,
      new Date(Date.now() - 1000).toISOString(),
      new Date().toISOString(),
    );
    expect(
      pushedTo((await post(alice, lab.channelId, "plain")).pushes),
    ).toEqual([[bob.id, "message"]]);
  });
});

describe("all messages", () => {
  it("pushes every message to members who chose it, once, with no inbox entry", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const carol = await newUser("carol");
    const lab = await newServer(alice, [bob, carol]);

    // Default (mentions): a plain message pushes nobody.
    expect((await post(alice, lab.channelId, "hello")).pushes).toEqual([]);

    await setting(bob, "server", lab.serverId, { level: "all" });
    const plain = await post(alice, lab.channelId, "hello again");
    expect(pushedTo(plain.pushes)).toEqual([[bob.id, "message"]]);
    expect(plain.inbox).toEqual([]);
    expect(plain.pushes[0]).toMatchObject({
      serverId: lab.serverId,
      channelName: "general",
      content: "hello again",
    });

    // A redelivered task sends nothing new.
    const again = await generateAllMessagePushes(db, {
      channelId: lab.channelId,
      messageId: plain.messageId,
      authorId: alice.id,
      authorUsername: alice.username,
      authorDisplayName: null,
      authorAvatarUrl: null,
      content: "hello again",
      contentRevision: 1,
      skip: new Set(),
    });
    expect(again).toEqual([]);

    // A mention is pushed once, as a mention.
    const mention = await post(alice, lab.channelId, `@${bob.username} look`);
    expect(pushedTo(mention.pushes)).toEqual([[bob.id, "mention"]]);

    // The author's own messages never push them.
    expect((await post(bob, lab.channelId, "mine")).pushes).toEqual([]);
  });

  it("stays silent while muted, under a channel level, or without access", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const lab = await newServer(alice, [bob]);

    await setting(bob, "server", lab.serverId, { level: "all" });
    await setting(bob, "channel", lab.otherChannelId, { level: "mentions" });
    expect((await post(alice, lab.otherChannelId, "hi")).pushes).toEqual([]);

    await setting(bob, "channel", lab.channelId, { mute: "1h" });
    expect((await post(alice, lab.channelId, "hi")).pushes).toEqual([]);
    await setting(bob, "channel", lab.channelId, { mute: "off" });
    await setting(bob, "server", lab.serverId, { mute: "forever" });
    expect((await post(alice, lab.channelId, "hi")).pushes).toEqual([]);
    await setting(bob, "server", lab.serverId, { mute: "off" });
    expect(pushedTo((await post(alice, lab.channelId, "hi")).pushes)).toEqual([
      [bob.id, "message"],
    ]);

    // Hidden from bob by a channel override: no push.
    const everyone = await db
      .prepare(`SELECT id FROM roles WHERE server_id = ? AND is_default = 1`)
      .bind(lab.serverId)
      .first<{ id: string }>();
    await run(
      `INSERT INTO channel_permission_overrides
         (id, channel_id, target_id, target_type, allow, deny)
       VALUES (?, ?, ?, 'role', 0, ?)`,
      crypto.randomUUID(),
      lab.channelId,
      everyone!.id,
      PERMISSIONS.VIEW_CHANNELS,
    );
    expect((await post(alice, lab.channelId, "secret")).pushes).toEqual([]);

    // Left the server: no push.
    await run(
      `DELETE FROM server_members WHERE server_id = ? AND user_id = ?`,
      lab.serverId,
      bob.id,
    );
    expect(
      (await post(alice, lab.otherChannelId, `@${bob.username}`)).pushes.filter(
        (push) => push.type === "message",
      ),
    ).toEqual([]);
  });

  it("skips someone who blocked the author", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const lab = await newServer(alice, [bob]);
    await setting(bob, "channel", lab.channelId, { level: "all" });
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type, created_at)
       VALUES (?, ?, 1, ?)`,
      bob.id,
      alice.id,
      new Date().toISOString(),
    );
    expect((await post(alice, lab.channelId, "hi")).pushes).toEqual([]);
  });
});

describe("DMs", () => {
  it("notify by default, and not when muted or below all", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const dm = await dmBetween(alice, bob);

    const first = await post(alice, dm, "hi");
    expect(first.inbox).toEqual([bob.id]);
    expect(pushedTo(first.pushes)).toEqual([[bob.id, "dm"]]);

    await setting(bob, "channel", dm, { mute: "8h" });
    const muted = await post(alice, dm, `hi @${bob.username}`);
    expect(muted.inbox).toEqual([]);
    expect(muted.pushes).toEqual([]);

    await setting(bob, "channel", dm, { mute: "off", level: "mentions" });
    expect((await post(alice, dm, "plain")).pushes).toEqual([]);
    expect(
      pushedTo((await post(alice, dm, `@${bob.username} you there`)).pushes),
    ).toEqual([[bob.id, "mention"]]);

    await setting(bob, "channel", dm, { level: "none" });
    expect((await post(alice, dm, `@${bob.username}`)).pushes).toEqual([]);
  });
});
