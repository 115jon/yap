// The inbox against miniflare's D1 and the real notifications route: a
// mention or reply only reaches someone who can read the channel, and
// GET /api/notifications leaves out what the user can no longer see (a
// channel they lost, a server they left, a DM they are not in) and what a
// sender they blocked wrote, in the list and in the unread count. Kova is
// faked at its get-session call; nothing else is stubbed.

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
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import { generateMessageNotifications } from "../../src/services/message.service";
import { setTestRequestHeaders } from "../test-shims/react-start-server";

type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const NotificationsRoute = (
  await loadRoute("../../src/routes/api/notifications")
).Route;

type Handler = (args: {
  request: Request;
  params?: Record<string, string>;
}) => Promise<Response>;

const db = env.DB as unknown as D1Database;
const BASE = "https://yap.test";
const VIEW_CHANNELS = 1 << 8;

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
  id: string;
  everyoneId: string;
  channelId: string;
}

/** A server with one text channel; members get @everyone (VIEW_CHANNELS). */
async function newServer(...members: TestUser[]): Promise<TestServer> {
  const id = `server-${crypto.randomUUID()}`;
  const everyoneId = `role-${crypto.randomUUID()}`;
  const channelId = `text-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES (?, 'Inbox', ?)`,
    id,
    members[0]!.id,
  );
  await run(
    `INSERT INTO roles (id, server_id, name, permissions, is_default) VALUES (?, ?, '@everyone', ?, 1)`,
    everyoneId,
    id,
    VIEW_CHANNELS,
  );
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, ?, 'general', 'text')`,
    channelId,
    id,
  );
  const server = { id, everyoneId, channelId };
  for (const member of members) await join(server, member);
  return server;
}

async function join(server: TestServer, user: TestUser) {
  await run(
    `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
    server.id,
    user.id,
  );
  await run(
    `INSERT INTO member_roles (server_id, user_id, role_id) VALUES (?, ?, ?)`,
    server.id,
    user.id,
    server.everyoneId,
  );
}

async function leave(server: TestServer, user: TestUser) {
  await run(
    `DELETE FROM member_roles WHERE server_id = ? AND user_id = ?`,
    server.id,
    user.id,
  );
  await run(
    `DELETE FROM server_members WHERE server_id = ? AND user_id = ?`,
    server.id,
    user.id,
  );
}

/** A second channel of the server that @everyone may not see. */
async function privateChannel(server: TestServer) {
  const channelId = `text-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, ?, 'staff', 'text')`,
    channelId,
    server.id,
  );
  await run(
    `INSERT INTO channel_permission_overrides (id, channel_id, target_id, target_type, allow, deny)
     VALUES (?, ?, ?, 'role', 0, ?)`,
    crypto.randomUUID(),
    channelId,
    server.everyoneId,
    VIEW_CHANNELS,
  );
  return channelId;
}

/** Lets one user into a private channel (a user override). */
async function allowIn(channelId: string, user: TestUser) {
  await run(
    `INSERT INTO channel_permission_overrides (id, channel_id, target_id, target_type, allow, deny)
     VALUES (?, ?, ?, 'user', ?, 0)`,
    crypto.randomUUID(),
    channelId,
    user.id,
    VIEW_CHANNELS,
  );
}

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

async function insertMessage(
  channelId: string,
  author: TestUser,
  content = "hello",
  replyToId: string | null = null,
) {
  const id = crypto.randomUUID();
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, reply_to_id, created_at, content_revision)
     VALUES (?, ?, ?, ?, ?, ?, 1)`,
    id,
    channelId,
    author.id,
    content,
    replyToId,
    new Date().toISOString(),
  );
  return id;
}

/** Posts a real message and runs the notification step for it. */
async function notify(
  channelId: string,
  author: TestUser,
  content: string,
  replyToId?: string,
) {
  const messageId = await insertMessage(
    channelId,
    author,
    content,
    replyToId ?? null,
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
}

async function storedFor(user: TestUser) {
  const { results } = await db
    .prepare(`SELECT channel_id, type FROM notifications WHERE user_id = ?`)
    .bind(user.id)
    .all<{ channel_id: string; type: string }>();
  return results ?? [];
}

/** A stored notification row, as an older server version left it. */
async function storeNotification(
  user: TestUser,
  from: TestUser,
  channelId: string,
  serverId: string | null,
  content: string,
) {
  const messageId = await insertMessage(channelId, from, content);
  const id = crypto.randomUUID();
  await run(
    `INSERT INTO notifications (id, user_id, type, channel_id, server_id, message_id, from_user_id, content, is_read, created_at)
     VALUES (?, ?, 'mention', ?, ?, ?, ?, ?, 0, ?)`,
    id,
    user.id,
    channelId,
    serverId,
    messageId,
    from.id,
    content,
    new Date().toISOString(),
  );
  return id;
}

async function inbox(user: TestUser, query = "") {
  const headers = new Headers({ authorization: `Bearer ${user.token}` });
  setTestRequestHeaders(headers);
  const fn = (
    NotificationsRoute.options as {
      server: { handlers: Record<string, Handler> };
    }
  ).server.handlers.GET!;
  const response = await fn({
    request: new Request(`${BASE}/api/notifications${query}`, { headers }),
  });
  expect(response.status).toBe(200);
  const body = (await response.json()) as {
    data?: {
      notifications: { id: string; content: string }[];
      unread_count: number;
    };
    notifications?: { id: string; content: string }[];
    unread_count?: number;
  };
  const data = body.data ?? body;
  return {
    ids: (data.notifications ?? []).map((n) => n.id),
    contents: (data.notifications ?? []).map((n) => n.content),
    unread: data.unread_count ?? -1,
  };
}

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

describe("mentions and replies", () => {
  it("notifies only mentioned users who can read the channel", async () => {
    const alice = await newUser("alice");
    const member = await newUser("member");
    const outsider = await newUser("outsider");
    const server = await newServer(alice, member);

    const sent = await notify(
      server.channelId,
      alice,
      `hi @${member.username} and @${outsider.username}`,
    );

    expect(sent.map((n) => n.userId)).toEqual([member.id]);
    expect(await storedFor(member)).toEqual([
      { channel_id: server.channelId, type: "mention" },
    ]);
    // No row, no gateway event, no push for someone outside the server.
    expect(await storedFor(outsider)).toEqual([]);
  });

  it("leaves out a member kept out of the channel, unless an override lets them in", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const carol = await newUser("carol");
    const server = await newServer(alice, bob, carol);
    const staff = await privateChannel(server);
    await allowIn(staff, alice);
    await allowIn(staff, carol);

    const sent = await notify(
      staff,
      alice,
      `secret for @${bob.username} and @${carol.username}`,
    );

    expect(sent.map((n) => n.userId)).toEqual([carol.id]);
    expect(await storedFor(bob)).toEqual([]);
  });

  it("does not notify a reply's parent author who left the server", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const server = await newServer(alice, bob);
    const parent = await insertMessage(server.channelId, bob);
    await leave(server, bob);

    expect(await notify(server.channelId, alice, "replying", parent)).toEqual(
      [],
    );
    expect(await storedFor(bob)).toEqual([]);

    // Back in the server, a reply reaches them again.
    await join(server, bob);
    const again = await notify(server.channelId, alice, "again", parent);
    expect(again.map((n) => n.userId)).toEqual([bob.id]);
  });

  it("does not notify someone the author blocked, or who blocked the author", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const carol = await newUser("carol");
    const server = await newServer(alice, bob, carol);
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
      alice.id,
      bob.id,
    );
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
      carol.id,
      alice.id,
    );

    const sent = await notify(
      server.channelId,
      alice,
      `hi @${bob.username} and @${carol.username}`,
    );
    expect(sent).toEqual([]);
  });

  it("still notifies the other person in a DM", async () => {
    const alice = await newUser("alice");
    const bob = await newUser("bob");
    const dm = await dmBetween(alice, bob);

    const sent = await notify(dm, alice, `hey @${bob.username}`);
    expect(sent.map((n) => n.userId)).toEqual([bob.id]);
  });
});

describe("GET /api/notifications", () => {
  it("leaves out channels the user can no longer read, in the list and the count", async () => {
    const me = await newUser("me");
    const friend = await newUser("friend");
    const stranger = await newUser("stranger");
    const kept = await newServer(friend, me);
    const left = await newServer(friend, me);
    const staff = await privateChannel(kept);
    const dm = await dmBetween(friend, me);
    const otherDm = await dmBetween(friend, stranger);

    const visible = await storeNotification(
      me,
      friend,
      kept.channelId,
      kept.id,
      "visible",
    );
    const inDm = await storeNotification(me, friend, dm, null, "in our dm");
    await storeNotification(me, friend, staff, kept.id, "staff secret");
    await storeNotification(me, friend, left.channelId, left.id, "left server");
    await storeNotification(me, friend, otherDm, null, "someone else's dm");
    await leave(left, me);

    const all = await inbox(me);
    expect(new Set(all.ids)).toEqual(new Set([visible, inDm]));
    expect(all.contents).not.toContain("staff secret");
    expect(all.contents).not.toContain("left server");
    expect(all.contents).not.toContain("someone else's dm");
    expect(all.unread).toBe(2);

    const unread = await inbox(me, "?unread=true");
    expect(new Set(unread.ids)).toEqual(new Set([visible, inDm]));
    expect(unread.unread).toBe(2);

    // Let in, the staff mention shows again.
    await allowIn(staff, me);
    expect((await inbox(me)).unread).toBe(3);
  });

  it("leaves out what a blocked sender wrote, and brings it back on unblock", async () => {
    const me = await newUser("me");
    const friend = await newUser("friend");
    const pest = await newUser("pest");
    const server = await newServer(me, friend, pest);

    const fromFriend = await storeNotification(
      me,
      friend,
      server.channelId,
      server.id,
      "from a friend",
    );
    await storeNotification(
      me,
      pest,
      server.channelId,
      server.id,
      "from a pest",
    );
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
      me.id,
      pest.id,
    );

    const blocked = await inbox(me);
    expect(blocked.ids).toEqual([fromFriend]);
    expect(blocked.unread).toBe(1);

    await run(
      `DELETE FROM relationships WHERE user_id = ? AND target_user_id = ?`,
      me.id,
      pest.id,
    );
    expect((await inbox(me)).unread).toBe(2);

    // A block the other way round hides it as well: the pest blocked me.
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
      pest.id,
      me.id,
    );
    const blockedBy = await inbox(me);
    expect(blockedBy.ids).toEqual([fromFriend]);
    expect(blockedBy.unread).toBe(1);

    await run(
      `DELETE FROM relationships WHERE user_id = ? AND target_user_id = ?`,
      pest.id,
      me.id,
    );
    expect((await inbox(me)).unread).toBe(2);
  });

  it("fills a page past rows the user can no longer see", async () => {
    const me = await newUser("me");
    const friend = await newUser("friend");
    const server = await newServer(friend, me);
    const staff = await privateChannel(server);

    const shown: string[] = [];
    for (let i = 0; i < 5; i++) {
      shown.push(
        await storeNotification(
          me,
          friend,
          server.channelId,
          server.id,
          `ok ${i}`,
        ),
      );
    }
    // Newer than every visible row, so a LIMIT before the filter would
    // return an empty page.
    await new Promise((resolve) => setTimeout(resolve, 5));
    for (let i = 0; i < 30; i++) {
      await storeNotification(me, friend, staff, server.id, `staff ${i}`);
    }

    const page = await inbox(me, "?limit=5");
    expect(new Set(page.ids)).toEqual(new Set(shown));
    expect(page.unread).toBe(5);
  });
});
