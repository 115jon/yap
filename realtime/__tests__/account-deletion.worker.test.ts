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
import { setTestRequestHeaders } from "../test-shims/react-start-server";
import { invalidatePendingDeletionCache } from "../../src/lib/account-deletion";
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import {
  purgeAccount,
  purgeDueAccountDeletions,
} from "../../src/services/account-deletion.service";

// Self-service account deletion end to end, against miniflare's D1 and R2
// and the real route handlers, with throwaway users. Kova is faked at its
// get-session call: `Bearer <token>` maps to a user id in SESSIONS.

type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const DeletionRoute = (await loadRoute("../../src/routes/api/account/deletion"))
  .Route;
const MeRoute = (await loadRoute("../../src/routes/api/users/me")).Route;
const ProfileRoute = (await loadRoute("../../src/routes/api/users/$id/profile"))
  .Route;
const BoardRoute = (await loadRoute("../../src/routes/api/users/$id/board"))
  .Route;
const CheckUsernameRoute = (
  await loadRoute("../../src/routes/api/check-username")
).Route;
const MessagesRoute = (
  await loadRoute("../../src/routes/api/channels/$id/messages")
).Route;

const db = env.DB as unknown as D1Database;
const bucket = env.BUCKET as unknown as R2Bucket;
const BASE = "https://yap.test";

// A fresh id per test: the deletion route's rate limit is per account.
let ALICE = "alice-throwaway-0";
let aliceRun = 0;
const BOB = "bob-throwaway";
const CAROL = "carol-throwaway";

const SESSIONS: Record<string, string> = {
  "alice-token": ALICE,
  "bob-token": BOB,
  "carol-token": CAROL,
};

type Handler = (args: {
  request: Request;
  params?: Record<string, string>;
}) => Promise<Response>;

function handler(route: { options: unknown }, method: string): Handler {
  const handlers = (
    route.options as { server: { handlers: Record<string, Handler> } }
  ).server.handlers;
  const fn = handlers[method];
  if (!fn) throw new Error(`no ${method} handler`);
  return fn;
}

function req(
  method: string,
  path: string,
  token: string | null,
  body?: unknown,
): Request {
  return new Request(`${BASE}${path}`, {
    method,
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body !== undefined ? { "content-type": "application/json" } : {}),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

const realFetch = globalThis.fetch;
let revokeCalls: unknown[] = [];

function fakeKova() {
  return vi
    .spyOn(globalThis, "fetch")
    .mockImplementation(async (input, init) => {
      const url =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (url.startsWith(DEFAULT_KOVA_AUTH_URL)) {
        const headers = new Headers(init?.headers);
        const token =
          headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
        const userId = SESSIONS[token];
        if (!userId) return Response.json({}, { status: 401 });
        if (url.includes("/api/auth/get-session")) {
          return Response.json({
            user: { id: userId },
            session: { id: `session-${userId}` },
          });
        }
        if (url.endsWith("/sessions/revoke")) {
          revokeCalls.push(JSON.parse(String(init?.body)));
          return Response.json({ revoked: 2 });
        }
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

async function count(sql: string, ...binds: unknown[]) {
  const row = await db
    .prepare(sql)
    .bind(...binds)
    .first<{ n: number }>();
  return Number(row?.n ?? 0);
}

async function seed() {
  aliceRun += 1;
  ALICE = `alice-throwaway-${aliceRun}`;
  SESSIONS["alice-token"] = ALICE;
  const now = new Date().toISOString();
  for (const table of [
    "account_deletions",
    "message_reactions",
    "messages",
    "dm_recipients",
    "member_roles",
    "roles",
    "server_members",
    "channels",
    "servers",
    "relationships",
    "push_tokens",
    "profile_boards",
    "user_avatar_uploads",
    "r2_cleanup_jobs",
    "users",
  ]) {
    await run(`DELETE FROM ${table}`);
  }
  for (const [id, username] of [
    [ALICE, "alice"],
    [BOB, "bob"],
    [CAROL, "carol"],
  ]) {
    await run(
      `INSERT INTO users (id, username, display_name, bio, avatar_url, created_at) VALUES (?, ?, ?, ?, ?, ?)`,
      id,
      username,
      `${username} display`,
      `${username} bio`,
      `/api/avatars/${id}/a.png`,
      now,
    );
  }
  // Bob's server, with Alice as a member holding a role.
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES ('srv-bob', 'Bob Place', ?)`,
    BOB,
  );
  await run(
    `INSERT INTO roles (id, server_id, name, permissions, is_default) VALUES ('role-everyone', 'srv-bob', '@everyone', 3840, 1)`,
  );
  await run(
    `INSERT INTO roles (id, server_id, name, permissions) VALUES ('role-mod', 'srv-bob', 'mod', 0)`,
  );
  for (const user of [BOB, ALICE]) {
    await run(
      `INSERT INTO server_members (server_id, user_id) VALUES ('srv-bob', ?)`,
      user,
    );
    await run(
      `INSERT INTO member_roles (server_id, user_id, role_id) VALUES ('srv-bob', ?, 'role-everyone')`,
      user,
    );
  }
  await run(
    `INSERT INTO member_roles (server_id, user_id, role_id) VALUES ('srv-bob', ?, 'role-mod')`,
    ALICE,
  );
  // Carol owns a server: her deletion is blocked.
  await run(
    `INSERT INTO servers (id, name, owner_id) VALUES ('srv-carol', 'Carol Hall', ?)`,
    CAROL,
  );
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES ('ch-general', 'srv-bob', 'general', 'text')`,
  );
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES ('m-1', 'ch-general', ?, 'hello from alice', ?)`,
    ALICE,
    now,
  );
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES ('m-2', 'ch-general', ?, 'hi alice', ?)`,
    BOB,
    now,
  );
  await run(
    `INSERT INTO message_reactions (message_id, user_id, emoji) VALUES ('m-2', ?, '👍')`,
    ALICE,
  );
  // A DM between Alice and Bob.
  await run(
    `INSERT INTO channels (id, server_id, name, channel_type) VALUES ('dm-ab', NULL, 'dm', 'dm')`,
  );
  for (const user of [ALICE, BOB]) {
    await run(
      `INSERT INTO dm_recipients (channel_id, user_id) VALUES ('dm-ab', ?)`,
      user,
    );
  }
  await run(
    `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES ('m-dm', 'dm-ab', ?, 'a private hello', ?)`,
    ALICE,
    now,
  );
  // Friends, a push token, a board and an avatar file.
  await run(
    `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 0), (?, ?, 0)`,
    ALICE,
    BOB,
    BOB,
    ALICE,
  );
  await run(
    `INSERT INTO push_tokens (id, user_id, token, platform) VALUES ('pt-1', ?, 'fcm-alice', 'android')`,
    ALICE,
  );
  await run(
    `INSERT INTO profile_boards (user_id, board, version, updated_at) VALUES (?, '{"widgets":[]}', 1, ?)`,
    ALICE,
    now,
  );
  await run(
    `INSERT INTO user_avatar_uploads (id, user_id, file_key, avatar_url, content_type) VALUES ('av-1', ?, ?, '/api/avatars/x', 'image/png')`,
    ALICE,
    `avatars/${ALICE}/av-1.png`,
  );
  await bucket.put(`avatars/${ALICE}/av-1.png`, "png");
  await bucket.put(`profile-assets/banner/${ALICE}.png`, "png");
  await bucket.put(`avatars/${BOB}/keep.png`, "png");
}

const deletion = (method: string, token: string, body?: unknown) =>
  handler(
    DeletionRoute,
    method,
  )({
    request: req(method, "/api/account/deletion", token, body),
  });

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

beforeEach(async () => {
  revokeCalls = [];
  invalidatePendingDeletionCache();
  fakeKova();
  await seed();
});

afterEach(() => {
  setTestRequestHeaders(null);
  vi.restoreAllMocks();
});

describe("account deletion request", () => {
  it("needs the username typed to confirm", async () => {
    const res = await deletion("POST", "alice-token", {
      confirm_username: "not-alice",
    });
    expect(res.status).toBe(400);
    expect(await res.json()).toMatchObject({ code: "CONFIRMATION_MISMATCH" });
    expect(await count(`SELECT COUNT(*) AS n FROM account_deletions`)).toBe(0);
  });

  it("is blocked while the account owns a server", async () => {
    const res = await deletion("POST", "carol-token", {
      confirm_username: "carol",
    });
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({
      code: "OWNS_SERVERS",
      servers: [{ id: "srv-carol", name: "Carol Hall" }],
    });
    expect(await count(`SELECT COUNT(*) AS n FROM account_deletions`)).toBe(0);
  });

  it("schedules deletion in 30 days, hides the profile and leaves servers", async () => {
    const before = Date.now();
    const res = await deletion("POST", "alice-token", {
      confirm_username: "@Alice",
    });
    expect(res.status).toBe(200);
    const body = (await res.json()) as {
      status: string;
      delete_after: string;
    };
    expect(body.status).toBe("pending");
    const days = (Date.parse(body.delete_after) - before) / 86_400_000;
    expect(days).toBeGreaterThan(29.99);
    expect(days).toBeLessThan(30.01);

    // Other sessions signed out at Kova.
    expect(revokeCalls).toEqual([{ allOthers: true }]);

    const row = await db
      .prepare(
        `SELECT username, display_name, avatar_url, bio, status FROM users WHERE id = ?`,
      )
      .bind(ALICE)
      .first<Record<string, string | null>>();
    expect(row).toMatchObject({
      display_name: "Deleted user",
      avatar_url: null,
      bio: null,
      status: "offline",
    });
    expect(row?.username).toMatch(/^deleted_user_[0-9a-f]{8}$/);
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM server_members WHERE user_id = ?`,
        ALICE,
      ),
    ).toBe(0);
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM push_tokens WHERE user_id = ?`,
        ALICE,
      ),
    ).toBe(0);
    // Messages and DMs are untouched during the grace period.
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM messages WHERE author_id = ?`,
        ALICE,
      ),
    ).toBe(2);

    // Asking again changes nothing.
    const again = await deletion("POST", "alice-token", {
      confirm_username: "whatever",
    });
    expect(again.status).toBe(200);
    expect(
      ((await again.json()) as { delete_after: string }).delete_after,
    ).toBe(body.delete_after);
  });

  it("refuses everything but status and restore while pending", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });

    const me = await handler(
      MeRoute,
      "GET",
    )({
      request: req("GET", "/api/users/me", "alice-token"),
    });
    expect(me.status).toBe(403);
    expect(await me.json()).toMatchObject({ code: "ACCOUNT_PENDING_DELETION" });

    const status = await deletion("GET", "alice-token");
    expect(status.status).toBe(200);
    expect(await status.json()).toMatchObject({
      status: "pending",
      purging: false,
    });
  });

  it("hides the profile, board and username from others while pending", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });

    const profile = await handler(
      ProfileRoute,
      "GET",
    )({
      request: req("GET", `/api/users/${ALICE}/profile`, "bob-token"),
      params: { id: ALICE },
    });
    const shown = (await profile.json()) as {
      user: { display_name: string; bio: string | null; username: string };
    };
    expect(shown.user.display_name).toBe("Deleted user");
    expect(shown.user.bio).toBeNull();
    expect(shown.user.username).not.toBe("alice");

    const board = await handler(
      BoardRoute,
      "GET",
    )({
      request: req("GET", `/api/users/${ALICE}/board`, "bob-token"),
      params: { id: ALICE },
    });
    expect(board.status).toBe(404);
    expect(await board.json()).toMatchObject({ code: "PROFILE_UNAVAILABLE" });

    setTestRequestHeaders({ authorization: "Bearer bob-token" });
    const taken = await handler(
      CheckUsernameRoute,
      "GET",
    )({
      request: req("GET", "/api/check-username?username=alice", "bob-token"),
    });
    expect(await taken.json()).toMatchObject({ available: false });
  });
});

describe("restoring within the grace period", () => {
  it("puts the profile, servers and roles back", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    const res = await deletion("DELETE", "alice-token");
    expect(res.status).toBe(200);
    expect(await res.json()).toMatchObject({ status: "active" });

    const row = await db
      .prepare(`SELECT username, display_name, bio FROM users WHERE id = ?`)
      .bind(ALICE)
      .first();
    expect(row).toMatchObject({
      username: "alice",
      display_name: "alice display",
      bio: "alice bio",
    });
    const roles = await db
      .prepare(
        `SELECT role_id FROM member_roles WHERE user_id = ? ORDER BY role_id`,
      )
      .bind(ALICE)
      .all<{ role_id: string }>();
    expect(roles.results.map((r) => r.role_id)).toEqual([
      "role-everyone",
      "role-mod",
    ]);
    expect(await count(`SELECT COUNT(*) AS n FROM account_deletions`)).toBe(0);

    // Signed in as usual again.
    const me = await handler(
      MeRoute,
      "GET",
    )({
      request: req("GET", "/api/users/me", "alice-token"),
    });
    expect(me.status).toBe(200);
  });

  it("does not rejoin a server that banned the account meanwhile", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    await run(
      `INSERT INTO server_bans (server_id, user_id, banned_by) VALUES ('srv-bob', ?, ?)`,
      ALICE,
      BOB,
    );
    await deletion("DELETE", "alice-token");
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM server_members WHERE user_id = ?`,
        ALICE,
      ),
    ).toBe(0);
    await run(`DELETE FROM server_bans`);
  });
});

describe("the purge after 30 days", () => {
  it("leaves accounts inside the grace period alone", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    const result = await purgeDueAccountDeletions(db, bucket, Date.now());
    expect(result).toEqual({ purged: 0, failed: 0 });
    expect(
      await count(`SELECT COUNT(*) AS n FROM users WHERE id = ?`, ALICE),
    ).toBe(1);
  });

  it("anonymises kept messages and deletes the rest", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    const later = Date.now() + 31 * 86_400_000;
    const result = await purgeDueAccountDeletions(db, bucket, later);
    expect(result).toEqual({ purged: 1, failed: 0 });

    // Nothing in Yap's database carries the Kova id any more.
    expect(
      await count(`SELECT COUNT(*) AS n FROM users WHERE id = ?`, ALICE),
    ).toBe(0);
    expect(await count(`SELECT COUNT(*) AS n FROM account_deletions`)).toBe(0);
    for (const [table, column] of [
      ["messages", "author_id"],
      ["dm_recipients", "user_id"],
      ["relationships", "user_id"],
      ["relationships", "target_user_id"],
      ["message_reactions", "user_id"],
      ["profile_boards", "user_id"],
      ["user_avatar_uploads", "user_id"],
      ["member_roles", "user_id"],
    ] as const) {
      expect(
        await count(
          `SELECT COUNT(*) AS n FROM ${table} WHERE ${column} = ?`,
          ALICE,
        ),
      ).toBe(0);
    }

    // Messages stay, server and DM alike, under a "Deleted user".
    const kept = await db
      .prepare(
        `SELECT m.id, m.content, u.display_name, u.deleted_at
         FROM messages m JOIN users u ON u.id = m.author_id
         WHERE m.id IN ('m-1', 'm-dm') ORDER BY m.id`,
      )
      .all<{
        id: string;
        content: string;
        display_name: string;
        deleted_at: string | null;
      }>();
    expect(kept.results).toHaveLength(2);
    for (const message of kept.results) {
      expect(message.display_name).toBe("Deleted user");
      expect(message.deleted_at).not.toBeNull();
    }
    // Bob still has the DM listed with the deleted account in it.
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM dm_recipients WHERE channel_id = 'dm-ab'`,
      ),
    ).toBe(2);

    // The account's files are gone; others' are not.
    expect(await bucket.head(`avatars/${ALICE}/av-1.png`)).toBeNull();
    expect(await bucket.head(`profile-assets/banner/${ALICE}.png`)).toBeNull();
    expect(await bucket.head(`avatars/${BOB}/keep.png`)).not.toBeNull();

    // Bob reads the channel: the author is anonymised, the content kept.
    setTestRequestHeaders({ authorization: "Bearer bob-token" });
    const listed = await handler(
      MessagesRoute,
      "GET",
    )({
      request: req("GET", "/api/channels/ch-general/messages", "bob-token"),
      params: { id: "ch-general" },
    });
    expect(listed.status, await listed.clone().text()).toBe(200);
    const text = await listed.text();
    expect(text).toContain("hello from alice");
    expect(text).toContain("Deleted user");
    expect(text).not.toContain("alice display");
    expect(text).not.toContain(ALICE);
  });

  it("never purges an account restored after the sweep listed it", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    await deletion("DELETE", "alice-token");
    const later = Date.now() + 31 * 86_400_000;
    expect(await purgeAccount(db, bucket, ALICE, later)).toBeNull();
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM messages WHERE author_id = ?`,
        ALICE,
      ),
    ).toBe(2);
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM users WHERE deleted_at IS NOT NULL`,
      ),
    ).toBe(0);
  });

  it("refuses a restore once the purge has claimed the account", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    await run(
      `UPDATE account_deletions SET tombstone_id = 'deleted-claimed', delete_after = ? WHERE user_id = ?`,
      new Date(Date.now() - 1000).toISOString(),
      ALICE,
    );
    const res = await deletion("DELETE", "alice-token");
    expect(res.status).toBe(409);
    expect(await res.json()).toMatchObject({ code: "DELETION_IN_PROGRESS" });
    const row = await db
      .prepare(`SELECT display_name FROM users WHERE id = ?`)
      .bind(ALICE)
      .first<{ display_name: string }>();
    expect(row?.display_name).toBe("Deleted user");
    expect(
      await count(
        `SELECT COUNT(*) AS n FROM server_members WHERE user_id = ?`,
        ALICE,
      ),
    ).toBe(0);
  });

  it("leaves a suspended account for the site admin", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    await run(
      `UPDATE users SET suspended_at = ? WHERE id = ?`,
      new Date().toISOString(),
      ALICE,
    );
    const result = await purgeDueAccountDeletions(
      db,
      bucket,
      Date.now() + 31 * 86_400_000,
    );
    expect(result).toEqual({ purged: 0, failed: 1 });
    expect(
      await count(`SELECT COUNT(*) AS n FROM users WHERE id = ?`, ALICE),
    ).toBe(1);
  });

  it("rewrites mentions of the account and clears its queued files", async () => {
    await run(
      `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES ('m-3', 'ch-general', ?, ?, ?)`,
      BOB,
      `thanks <@${ALICE}>`,
      new Date().toISOString(),
    );
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    await purgeDueAccountDeletions(db, bucket, Date.now() + 31 * 86_400_000);
    const mention = await db
      .prepare(`SELECT content FROM messages WHERE id = 'm-3'`)
      .first<{ content: string }>();
    expect(mention?.content).not.toContain(ALICE);
    expect(mention?.content).toMatch(/^thanks <@deleted-/);
    // Deleted files leave nothing in the retry queue.
    expect(await count(`SELECT COUNT(*) AS n FROM r2_cleanup_jobs`)).toBe(0);
  });

  it("does not purge an account that owns a server", async () => {
    await deletion("POST", "alice-token", { confirm_username: "alice" });
    // Ownership given to the account after it asked (not possible through
    // the API, which refuses it every request).
    await run(`UPDATE servers SET owner_id = ? WHERE id = 'srv-bob'`, ALICE);
    const result = await purgeDueAccountDeletions(
      db,
      bucket,
      Date.now() + 31 * 86_400_000,
    );
    expect(result).toEqual({ purged: 0, failed: 1 });
    expect(
      await count(`SELECT COUNT(*) AS n FROM servers WHERE id = 'srv-bob'`),
    ).toBe(1);
    expect(
      await count(
        `SELECT attempts AS n FROM account_deletions WHERE user_id = ?`,
        ALICE,
      ),
    ).toBe(1);
  });
});
