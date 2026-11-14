// Message pushes carry ids only, and the phone fetches the text from
// GET /api/notifications/:id/push. Against miniflare's D1 and the real
// notification code: no push format holds the message, sender, channel or
// server, and the fetch answers only the recipient, only while they can
// still see the message (not deleted, channel visible, no block).

import { env } from "cloudflare:workers";
import { beforeAll, describe, expect, it } from "vitest";

import schema from "../../d1_schema.sql?raw";
import { PERMISSIONS } from "../../src/lib/permissions";
import { notificationPushRoutes } from "../../src/server/routes/notification-push";
import { memoryAuth, testServices } from "../../src/server/testing";
import { generateMessageNotifications } from "../../src/services/message.service";
import {
  buildMessagePush,
  buildNativeMessagePush,
  type MessagePush,
} from "../../src/services/push.service";

const db = env.DB as unknown as D1Database;

/** Bearer header -> user id, for memoryAuth. */
const sessions: Record<string, string> = {};

async function run(sql: string, ...binds: unknown[]) {
  await db
    .prepare(sql)
    .bind(...binds)
    .run();
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

interface TestUser {
  id: string;
  username: string;
  name: string;
}

async function newUser(name: string): Promise<TestUser> {
  const tag = crypto.randomUUID().replaceAll("-", "").slice(0, 10);
  const user = { id: `${name}-${tag}`, username: `${name}${tag}`, name };
  await run(
    `INSERT INTO users (id, username, display_name, avatar_url, created_at)
     VALUES (?, ?, ?, ?, ?)`,
    user.id,
    user.username,
    `${name} Display`,
    `/api/avatars/${user.id}`,
    new Date().toISOString(),
  );
  sessions[`Bearer token-${user.id}`] = user.id;
  return user;
}

/** A server named "Secret Club" with #hidden-plans, everyone a viewer. */
async function serverChannel(...members: TestUser[]) {
  const serverId = `server-${crypto.randomUUID()}`;
  const channelId = `text-${crypto.randomUUID()}`;
  const everyone = `role-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES (?, 'Secret Club', ?)`,
    serverId,
    members[0]!.id,
  );
  await run(
    `INSERT INTO roles (id, server_id, name, permissions, is_default)
     VALUES (?, ?, '@everyone', ?, 1)`,
    everyone,
    serverId,
    PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES,
  );
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type)
     VALUES (?, ?, 'hidden-plans', 'text')`,
    channelId,
    serverId,
  );
  for (const member of members) {
    await run(
      `INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`,
      serverId,
      member.id,
    );
    await run(
      `INSERT INTO member_roles (server_id, user_id, role_id) VALUES (?, ?, ?)`,
      serverId,
      member.id,
      everyone,
    );
  }
  return { serverId, channelId, everyone };
}

async function dmBetween(a: TestUser, b: TestUser) {
  const channelId = `dm-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, NULL, ?, 'dm')`,
    channelId,
    `DM-${a.id}-${b.id}`,
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

/** Posts a real message and returns the pushes its notifications made. */
async function post(channelId: string, author: TestUser, content: string) {
  const messageId = crypto.randomUUID();
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, created_at, content_revision)
     VALUES (?, ?, ?, ?, ?, 1)`,
    messageId,
    channelId,
    author.id,
    content,
    new Date().toISOString(),
  );
  const broadcasts = await generateMessageNotifications(
    db,
    () => crypto.randomUUID(),
    {
      channelId,
      messageId,
      authorId: author.id,
      authorUsername: author.username,
      authorDisplayName: `${author.name} Display`,
      authorAvatarUrl: `/api/avatars/${author.id}`,
      content,
      contentRevision: 1,
    },
  );
  const pushes = broadcasts.flatMap((b) => (b.push ? [b.push] : []));
  return { messageId, pushes };
}

const routes = () =>
  notificationPushRoutes(testServices({ auth: memoryAuth(sessions), db }));

async function fetchText(user: TestUser | null, notificationId: string) {
  const headers = new Headers();
  if (user) headers.set("authorization", `Bearer token-${user.id}`);
  const response = await routes().GET({
    request: new Request(
      `https://yap.test/api/notifications/${encodeURIComponent(notificationId)}/push`,
      { headers },
    ),
    params: { id: notificationId },
  });
  return {
    status: response.status,
    cache: response.headers.get("cache-control"),
    body: (await response.json()) as Record<string, unknown>,
  };
}

/** Every key and string value in an FCM message, nested blocks included. */
function stringsIn(value: unknown): string[] {
  if (typeof value === "string") return [value];
  if (value && typeof value === "object") {
    return Object.entries(value).flatMap(([key, v]) => [key, ...stringsIn(v)]);
  }
  return [];
}

function expectNoText(push: MessagePush, words: readonly string[]) {
  for (const message of [
    buildNativeMessagePush(push),
    buildMessagePush(push),
  ]) {
    const strings = stringsIn(message).join("\n");
    for (const word of words) expect(strings).not.toContain(word);
  }
}

describe("message pushes carry ids only", () => {
  it("a mention's push names nothing; the recipient fetches the text", async () => {
    const ada = await newUser("ada");
    const bob = await newUser("bob");
    const { serverId, channelId } = await serverChannel(ada, bob);
    const { messageId, pushes } = await post(
      channelId,
      ada,
      `@${bob.username}   the launch code is   swordfish`,
    );

    expect(pushes).toHaveLength(1);
    const [push] = pushes;
    expectNoText(push!, [
      "swordfish",
      "launch code",
      "ada Display",
      ada.username,
      "hidden-plans",
      "Secret Club",
      "avatars",
    ]);
    expect(Object.keys(buildNativeMessagePush(push!).data!).sort()).toEqual([
      "channelId",
      "kind",
      "messageId",
      "notificationId",
      "ralph",
      "senderId",
      "sentAt",
      "serverId",
      "tag",
      "type",
    ]);

    const text = await fetchText(bob, push!.notificationId);
    expect(text.status).toBe(200);
    expect(text.cache).toBe("no-store");
    expect(text.body).toEqual({
      type: "mention",
      channel_id: channelId,
      server_id: serverId,
      message_id: messageId,
      sender: {
        id: ada.id,
        name: "ada Display",
        avatar_url: `/api/avatars/${ada.id}`,
      },
      conversation: "#hidden-plans",
      server_name: "Secret Club",
      preview: `@${bob.username} the launch code is swordfish`,
    });
  });

  it("a DM's push names nothing; its text has no conversation", async () => {
    const ada = await newUser("ada");
    const bob = await newUser("bob");
    const channelId = await dmBetween(ada, bob);
    const { pushes } = await post(channelId, ada, "meet at the old mill");

    expect(pushes).toHaveLength(1);
    expectNoText(pushes[0]!, ["old mill", "ada Display", ada.username]);

    const text = await fetchText(bob, pushes[0]!.notificationId);
    expect(text.status).toBe(200);
    expect(text.body).toMatchObject({
      type: "dm",
      server_id: null,
      conversation: null,
      server_name: null,
      sender: { id: ada.id, name: "ada Display" },
      preview: "meet at the old mill",
    });
  });

  it("caps the preview at the 180 characters the push carried", async () => {
    const ada = await newUser("ada");
    const bob = await newUser("bob");
    const channelId = await dmBetween(ada, bob);
    const { pushes } = await post(channelId, ada, "x".repeat(1500));

    const text = await fetchText(bob, pushes[0]!.notificationId);
    expect(text.body.preview).toBe(`${"x".repeat(179)}…`);
  });
});

describe("GET /api/notifications/:id/push answers only what the user may see", () => {
  async function mentioned() {
    const ada = await newUser("ada");
    const bob = await newUser("bob");
    const eve = await newUser("eve");
    const server = await serverChannel(ada, bob, eve);
    const { messageId, pushes } = await post(
      server.channelId,
      ada,
      `@${bob.username} psst`,
    );
    const id = pushes[0]!.notificationId;
    expect((await fetchText(bob, id)).status).toBe(200);
    return { ada, bob, eve, ...server, messageId, id };
  }

  it("needs a session", async () => {
    const { id } = await mentioned();

    expect((await fetchText(null, id)).status).toBe(401);
  });

  it("is 404 for anyone but the recipient, even in the same channel", async () => {
    const { ada, eve, id } = await mentioned();

    for (const other of [ada, eve]) {
      const text = await fetchText(other, id);
      expect(text.status).toBe(404);
      expect(JSON.stringify(text.body)).not.toContain("psst");
    }
    expect((await fetchText(eve, "no-such-notification")).status).toBe(404);
    expect((await fetchText(eve, "bad id/../x")).status).toBe(404);
  });

  it("is 404 once the message is deleted", async () => {
    const { bob, messageId, id } = await mentioned();
    await run(`DELETE FROM messages WHERE id = ?`, messageId);

    expect((await fetchText(bob, id)).status).toBe(404);
  });

  it("is 404 when the recipient can no longer view the channel", async () => {
    const { bob, channelId, everyone, id } = await mentioned();
    await run(
      `INSERT INTO channel_permission_overrides (id, channel_id, target_id, target_type, allow, deny)
       VALUES (?, ?, ?, 'role', 0, ?)`,
      crypto.randomUUID(),
      channelId,
      everyone,
      PERMISSIONS.VIEW_CHANNELS,
    );

    expect((await fetchText(bob, id)).status).toBe(404);
  });

  it("is 404 when the recipient left the server", async () => {
    const { bob, serverId, id } = await mentioned();
    await run(
      `DELETE FROM server_members WHERE server_id = ? AND user_id = ?`,
      serverId,
      bob.id,
    );

    expect((await fetchText(bob, id)).status).toBe(404);
  });

  it("is 404 for a mention of someone outside the server", async () => {
    const ada = await newUser("ada");
    const outsider = await newUser("out");
    const { channelId } = await serverChannel(ada);
    const { messageId } = await post(channelId, ada, "members only");
    // A notification row for them, however it came to be.
    const id = crypto.randomUUID();
    await run(
      `INSERT INTO notifications (id, user_id, type, channel_id, message_id, from_user_id, content)
       VALUES (?, ?, 'mention', ?, ?, ?, 'members only')`,
      id,
      outsider.id,
      channelId,
      messageId,
      ada.id,
    );

    expect((await fetchText(outsider, id)).status).toBe(404);
  });

  it("is 404 after a block either way", async () => {
    for (const blockerIsRecipient of [true, false]) {
      const { ada, bob, id } = await mentioned();
      const [blocker, target] = blockerIsRecipient ? [bob, ada] : [ada, bob];
      await run(
        `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
        blocker.id,
        target.id,
      );

      expect((await fetchText(bob, id)).status).toBe(404);
    }
  });

  it("is 404 when the recipient left the DM", async () => {
    const ada = await newUser("ada");
    const bob = await newUser("bob");
    const channelId = await dmBetween(ada, bob);
    const { pushes } = await post(channelId, ada, "still there?");
    await run(
      `DELETE FROM dm_recipients WHERE channel_id = ? AND user_id = ?`,
      channelId,
      bob.id,
    );

    expect((await fetchText(bob, pushes[0]!.notificationId)).status).toBe(404);
  });
});
