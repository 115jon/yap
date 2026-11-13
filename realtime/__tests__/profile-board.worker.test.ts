// Profile boards against a real D1 and R2 (workerd): saves with a version
// conflict, the board limits, the Custom widget's perk (drafted on free,
// hidden again when a plan lapses), guestbook permissions, rate limits,
// blocks and the off switch, Activity writes from each source (idempotent),
// retention, and what other viewers may see. Auth is a stub keyed by an
// `x-test-user` header; the SQL, the R2 calls and every check are real.

import { env } from "cloudflare:workers";
import { beforeAll, describe, expect, it } from "vitest";

import schemaSql from "../../d1_schema.sql?raw";
import migrationSql from "../../migrations/043_profile_boards.sql?raw";
import type {
  ActivityPage,
  BoardConflictBody,
  BoardResponse,
  BoardSaveResponse,
  GuestbookNote,
} from "../../src/lib/profile-board";
import { checkRateLimitDO } from "../../src/lib/rate-limit";
import { awardBadge, sweepAutoBadges } from "../../src/services/badge.service";
import {
  gameSessionStatements,
  pruneProfileActivity,
  recordCallSession,
  recordGameSession,
  recordServerJoin,
} from "../../src/services/profile-activity.service";
import {
  createProfileBoardHandlers,
  PROFILE_BOARD_RATE_LIMITS,
  type ProfileBoardDeps,
} from "../../src/services/profile-board-handlers";
import { recordVoiceSession } from "../../src/services/stats.service";
import { UNCHECKED_IMAGE_MESSAGE } from "../../src/lib/upload-metadata/strip-upload";
import {
  pngChunk,
  pngWith,
} from "../../src/lib/__tests__/fixtures/metadata-files";

const db = env.DB as unknown as D1Database;
const bucket = env.BUCKET as unknown as R2Bucket;
const BASE = "https://yap.test";
const NOW = Date.parse("2026-11-09T12:00:00.000Z");
const DAY = 86_400_000;

function statements(sql: string): string[] {
  return sql
    .split("\n")
    .filter((line) => !line.trim().startsWith("--"))
    .join("\n")
    .split(/;\s*\n/)
    .map((statement) => statement.trim().replace(/;$/, ""))
    .filter(Boolean);
}

async function ensureColumn(table: string, column: string, type: string) {
  const { results } = await db
    .prepare(`PRAGMA table_info(${table})`)
    .all<{ name: string }>();
  if (!(results ?? []).some((row) => row.name === column)) {
    await db.prepare(`ALTER TABLE ${table} ADD COLUMN ${column} ${type}`).run();
  }
}

beforeAll(async () => {
  for (const statement of statements(schemaSql)) {
    await db.prepare(statement).run();
  }
  // The migration itself, twice: it must be idempotent.
  for (const pass of [1, 2]) {
    void pass;
    for (const statement of statements(migrationSql)) {
      await db.prepare(statement).run();
    }
  }
  // Other worker test files may have made trimmed copies of these tables
  // in the same database first.
  await ensureColumn("servers", "icon_url", "TEXT");
  await ensureColumn("users", "display_name", "TEXT");
  await ensureColumn("users", "avatar_url", "TEXT");
  await ensureColumn("users", "share_activity", "INTEGER NOT NULL DEFAULT 1");
});

// ── Fixtures ───────────────────────────────────────────────────────────────

function tag() {
  return crypto.randomUUID().slice(0, 8);
}

async function addUser(id: string) {
  await db
    .prepare(
      `INSERT INTO users (id, username, created_at) VALUES (?, ?, '2026-01-01T00:00:00.000Z')`,
    )
    .bind(id, id)
    .run();
  return id;
}

async function addServer(id: string, ownerId: string, members: string[]) {
  await db
    .prepare(`INSERT INTO servers (id, name, owner_id) VALUES (?, ?, ?)`)
    .bind(id, `Server ${id}`, ownerId)
    .run();
  for (const member of members) {
    await db
      .prepare(`INSERT INTO server_members (server_id, user_id) VALUES (?, ?)`)
      .bind(id, member)
      .run();
  }
}

async function addChannel(id: string, serverId: string, name: string) {
  await db
    .prepare(
      `INSERT INTO channels (id, server_id, name, channel_type) VALUES (?, ?, ?, 'voice')`,
    )
    .bind(id, serverId, name)
    .run();
}

async function setPlan(
  userId: string,
  tier: "loud" | "founder",
  expiresAt: string | null,
) {
  await db
    .prepare(
      `INSERT INTO user_entitlements (user_id, tier, source, started_at, expires_at, updated_at)
       VALUES (?, ?, 'stripe', '2026-10-01T00:00:00.000Z', ?, '2026-10-01T00:00:00.000Z')
       ON CONFLICT(user_id) DO UPDATE SET tier = excluded.tier, expires_at = excluded.expires_at`,
    )
    .bind(userId, tier, expiresAt)
    .run();
}

async function block(userId: string, targetId: string) {
  await db
    .prepare(
      `INSERT INTO relationships (user_id, target_user_id, type) VALUES (?, ?, 1)`,
    )
    .bind(userId, targetId)
    .run();
}

function handlers(overrides: Partial<ProfileBoardDeps> = {}) {
  return createProfileBoardHandlers({
    db,
    bucket,
    authenticate: async (request) => {
      const user = request.headers.get("x-test-user");
      return user
        ? { userId: user }
        : Response.json({ error: "Unauthorized" }, { status: 401 });
    },
    rateLimit: async () => null,
    now: () => NOW,
    ...overrides,
  });
}

function req(
  method: string,
  path: string,
  user: string,
  body?: unknown,
): Request {
  return new Request(`${BASE}${path}`, {
    method,
    headers: {
      "x-test-user": user,
      ...(body === undefined ? {} : { "Content-Type": "application/json" }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

async function save(user: string, board: unknown, version: number) {
  const response = await handlers().putBoard(
    req("PUT", "/api/users/me/board", user, { board, version }),
    "me",
  );
  return { status: response.status, body: (await response.json()) as never };
}

async function view(owner: string, viewer: string) {
  const response = await handlers().getBoard(
    req("GET", `/api/users/${owner}/board`, viewer),
    owner,
  );
  return {
    status: response.status,
    body: (await response.json()) as BoardResponse,
  };
}

// ── Saving ─────────────────────────────────────────────────────────────────

describe("board saves", () => {
  it("bumps the version and refuses a save from an older one with the server copy", async () => {
    const owner = await addUser(`owner-${tag()}`);
    const first = await save(
      owner,
      { widgets: [{ id: "mood-a", type: "mood", cfg: { text: "hi" } }] },
      0,
    );
    expect(first.status).toBe(200);
    expect((first.body as BoardSaveResponse).version).toBe(1);

    const second = await save(owner, { widgets: [] }, 1);
    expect((second.body as BoardSaveResponse).version).toBe(2);

    // A second device still on version 1.
    const stale = await save(
      owner,
      { widgets: [{ id: "x1234", type: "links" }] },
      1,
    );
    expect(stale.status).toBe(409);
    const conflict = stale.body as BoardConflictBody;
    expect(conflict.code).toBe("BOARD_VERSION_CONFLICT");
    expect(conflict.version).toBe(2);
    expect(conflict.board.widgets).toEqual([]);

    // A first save from a device that thinks there is no board yet.
    const fresh = await save(owner, { widgets: [] }, 0);
    expect(fresh.status).toBe(409);
  });

  it("caps widgets, drops what it can't keep and reports it", async () => {
    const owner = await addUser(`owner-${tag()}`);
    const widgets = [
      ...Array.from({ length: 3 }, (_, i) => ({
        id: `links${i}aa`,
        type: "links",
      })),
      ...Array.from({ length: 12 }, (_, i) => ({
        id: `cd${i}aaaa`,
        type: "countdown",
      })),
      { id: "zzzzzz", type: "teleporter" },
    ];
    const result = await save(owner, { widgets }, 0);
    const saved = result.body as BoardSaveResponse;
    expect(saved.board.widgets.filter((w) => w.type === "links")).toHaveLength(
      2,
    );
    expect(
      saved.board.widgets.filter((w) => w.type === "countdown"),
    ).toHaveLength(1);
    expect(saved.dropped.map((d) => d.reason).sort()).toEqual(
      [
        ...Array(1).fill("type_limit"),
        ...Array(11).fill("type_limit"),
        "unknown_type",
      ].sort(),
    );
  });

  it("refuses a body that is too big, a missing version and someone else's board", async () => {
    const owner = await addUser(`owner-${tag()}`);
    const huge = await handlers().putBoard(
      req("PUT", "/api/users/me/board", owner, {
        board: { widgets: [], pad: "x".repeat(70 * 1024) },
        version: 0,
      }),
      "me",
    );
    expect(huge.status).toBe(413);

    const noVersion = await handlers().putBoard(
      req("PUT", "/api/users/me/board", owner, { board: { widgets: [] } }),
      "me",
    );
    expect(noVersion.status).toBe(400);

    const other = await handlers().putBoard(
      req("PUT", "/api/users/someone/board", owner, {
        board: { widgets: [] },
        version: 0,
      }),
      "someone",
    );
    expect(other.status).toBe(403);
  });

  it("applies the save rate limit before anything else", async () => {
    const owner = await addUser(`owner-${tag()}`);
    const limited = handlers({
      rateLimit: async (_user, action) =>
        action === "save"
          ? Response.json({ error: "slow" }, { status: 429 })
          : null,
    });
    const response = await limited.putBoard(
      req("PUT", "/api/users/me/board", owner, {
        board: { widgets: [] },
        version: 0,
      }),
      "me",
    );
    expect(response.status).toBe(429);
  });

  it("only pins the owner's own server messages, as a snapshot", async () => {
    const owner = await addUser(`owner-${tag()}`);
    const other = await addUser(`other-${tag()}`);
    const server = `srv-${tag()}`;
    await addServer(server, owner, [owner, other]);
    const channel = `ch-${tag()}`;
    await db
      .prepare(
        `INSERT INTO channels (id, server_id, name) VALUES (?, ?, 'general')`,
      )
      .bind(channel, server)
      .run();
    const mine = `m-${tag()}`;
    const theirs = `m-${tag()}`;
    for (const [id, author, text] of [
      [mine, owner, "we lost. I did not uninstall"],
      [theirs, other, "not yours"],
    ]) {
      await db
        .prepare(
          `INSERT INTO messages (id, channel_id, author_id, content, created_at) VALUES (?, ?, ?, ?, '2026-10-12T02:14:00.000Z')`,
        )
        .bind(id, channel, author, text)
        .run();
    }
    const pinned = await save(
      owner,
      { widgets: [{ id: "quote1", type: "quote", cfg: { message_id: mine } }] },
      0,
    );
    const quote = (pinned.body as BoardSaveResponse).board.widgets[0];
    expect(quote.cfg).toMatchObject({
      message_id: mine,
      snapshot: {
        text: "we lost. I did not uninstall",
        channel_name: "general",
      },
    });

    const stolen = await save(
      owner,
      {
        widgets: [{ id: "quote1", type: "quote", cfg: { message_id: theirs } }],
      },
      1,
    );
    expect((stolen.body as BoardSaveResponse).board.widgets[0].cfg).toEqual({
      message_id: null,
      snapshot: null,
    });

    // Someone outside the server sees that a message is pinned, not its text's source.
    const stranger = await addUser(`stranger-${tag()}`);
    await save(
      owner,
      { widgets: [{ id: "quote1", type: "quote", cfg: { message_id: mine } }] },
      2,
    );
    expect((await view(owner, stranger)).body.data.quote_visible).toBe(false);
    expect((await view(owner, other)).body.data.quote_visible).toBe(true);
  });
});

// ── The Custom widget's perk ───────────────────────────────────────────────

describe("the Custom widget", () => {
  const board = {
    widgets: [
      { id: "custom-1", type: "custom", cfg: { title: "Commissions open" } },
      { id: "mood-1", type: "mood", cfg: { text: "ranked grind" } },
    ],
  };

  it("is a draft on a free plan: stored, shown to the owner only", async () => {
    const owner = await addUser(`free-${tag()}`);
    const viewer = await addUser(`viewer-${tag()}`);
    const saved = (await save(owner, board, 0)).body as BoardSaveResponse;
    expect(saved.board.widgets.map((w) => w.id)).toEqual([
      "custom-1",
      "mood-1",
    ]);
    expect(saved.drafts).toEqual(["custom-1"]);

    const mine = (await view(owner, owner)).body;
    expect(mine.board.widgets.map((w) => w.id)).toEqual(["custom-1", "mood-1"]);
    expect(mine.drafts).toEqual(["custom-1"]);

    const theirs = (await view(owner, viewer)).body;
    expect(theirs.board.widgets.map((w) => w.id)).toEqual(["mood-1"]);
    expect(theirs.drafts).toEqual([]);
  });

  it("shows while Loud, hides when the plan lapses, and comes back on renewal", async () => {
    const owner = await addUser(`loud-${tag()}`);
    const viewer = await addUser(`viewer-${tag()}`);
    await setPlan(owner, "loud", "2099-01-01T00:00:00.000Z");
    const saved = (await save(owner, board, 0)).body as BoardSaveResponse;
    expect(saved.drafts).toEqual([]);
    expect(
      (await view(owner, viewer)).body.board.widgets.map((w) => w.id),
    ).toContain("custom-1");

    await setPlan(owner, "loud", "2026-11-01T00:00:00.000Z");
    expect(
      (await view(owner, viewer)).body.board.widgets.map((w) => w.id),
    ).not.toContain("custom-1");
    const stored = await db
      .prepare(`SELECT board FROM profile_boards WHERE user_id = ?`)
      .bind(owner)
      .first<{ board: string }>();
    expect(stored?.board).toContain("custom-1");
    expect((await view(owner, owner)).body.drafts).toEqual(["custom-1"]);

    await setPlan(owner, "loud", "2099-01-01T00:00:00.000Z");
    expect(
      (await view(owner, viewer)).body.board.widgets.map((w) => w.id),
    ).toContain("custom-1");
  });
});

// ── Widget images ──────────────────────────────────────────────────────────

/** A real PNG of at least `size` bytes (padded with a private chunk). */
function png(size = 64): Uint8Array<ArrayBuffer> {
  const base = pngWith();
  const padding = size - base.length - 12;
  const file =
    padding > 0 ? pngWith(pngChunk("prVt", new Uint8Array(padding))) : base;
  return new Uint8Array(file);
}

/** A PNG signature and nothing a decoder can walk. */
function brokenPng(): Uint8Array<ArrayBuffer> {
  const bytes = new Uint8Array(64);
  bytes.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  return bytes;
}

function animatedGif(): Uint8Array<ArrayBuffer> {
  const header = [...new TextEncoder().encode("GIF89a"), 1, 0, 1, 0, 0, 0, 0];
  const frame = [0x2c, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 1, 0x44, 0];
  return new Uint8Array([...header, ...frame, ...frame, 0x3b]);
}

async function upload(
  user: string,
  bytes: Uint8Array<ArrayBuffer>,
  name: string,
) {
  const form = new FormData();
  form.append("file", new File([bytes], name));
  return handlers().uploadAsset(
    new Request(`${BASE}/api/users/me/board/assets`, {
      method: "POST",
      headers: { "x-test-user": user },
      body: form,
    }),
    "me",
  );
}

describe("widget images", () => {
  it("stores a still image under the owner's prefix", async () => {
    const owner = await addUser(`img-${tag()}`);
    const response = await upload(owner, png(), "a.png");
    expect(response.status).toBe(201);
    const body = (await response.json()) as { asset: string; url: string };
    expect(body.asset).toMatch(/^[a-z0-9]{20}\.png$/);
    expect(body.url).toBe(`/api/profile-assets/widget/${owner}/${body.asset}`);
    expect(
      await bucket.head(`profile-assets/widget/${owner}/${body.asset}`),
    ).not.toBeNull();
  });

  it("refuses animation without Yap Loud, non-images and files over the plan's limit", async () => {
    const owner = await addUser(`img-${tag()}`);
    expect((await upload(owner, animatedGif(), "a.gif")).status).toBe(403);
    expect(
      (
        await upload(
          owner,
          new TextEncoder().encode("<svg onload=alert(1)></svg> padding"),
          "x.svg",
        )
      ).status,
    ).toBe(415);
    expect(
      (await upload(owner, png(8 * 1024 * 1024 + 16), "big.png")).status,
    ).toBe(413);

    await setPlan(owner, "loud", "2099-01-01T00:00:00.000Z");
    expect((await upload(owner, animatedGif(), "a.gif")).status).toBe(201);
    expect(
      (await upload(owner, png(8 * 1024 * 1024 + 16), "big.png")).status,
    ).toBe(201);
  });

  it("refuses an image its metadata scan couldn't finish", async () => {
    const owner = await addUser(`img-${tag()}`);
    const response = await upload(owner, brokenPng(), "broken.png");
    expect(response.status).toBe(415);
    expect(JSON.stringify(await response.json())).toContain(
      UNCHECKED_IMAGE_MESSAGE,
    );
    const stored = await bucket.list({
      prefix: `profile-assets/widget/${owner}/`,
    });
    expect(stored.objects).toEqual([]);
  });
});

// ── Guestbook ──────────────────────────────────────────────────────────────

async function sign(
  owner: string,
  author: string,
  body: string,
  deps?: Partial<ProfileBoardDeps>,
) {
  const response = await handlers(deps).postGuestbook(
    req("POST", `/api/users/${owner}/guestbook`, author, { body }),
    owner,
  );
  return {
    status: response.status,
    body: (await response.json()) as GuestbookNote & { code?: string },
  };
}

describe("guestbook", () => {
  async function pair() {
    const owner = await addUser(`gb-owner-${tag()}`);
    const friend = await addUser(`gb-friend-${tag()}`);
    await addServer(`gb-srv-${tag()}`, owner, [owner, friend]);
    return { owner, friend };
  }

  it("lets people who share a server sign, and nobody else", async () => {
    const { owner, friend } = await pair();
    const stranger = await addUser(`gb-stranger-${tag()}`);
    const posted = await sign(
      owner,
      friend,
      "  best duo partner‮, terrible taste ",
    );
    expect(posted.status).toBe(201);
    expect(posted.body.body).toBe("best duo partner, terrible taste");

    const refused = await sign(owner, stranger, "hi");
    expect(refused.status).toBe(403);
    expect(refused.body.code).toBe("GUESTBOOK_NO_SHARED_SPACE");
    expect((await sign(owner, owner, "me")).body.code).toBe("GUESTBOOK_OWN");
    expect((await sign(owner, friend, "   ")).status).toBe(400);
  });

  it("lets people in a DM sign", async () => {
    const owner = await addUser(`gb-owner-${tag()}`);
    const dmFriend = await addUser(`gb-dm-${tag()}`);
    const dm = `dm-${tag()}`;
    await db
      .prepare(
        `INSERT INTO channels (id, name, channel_type) VALUES (?, 'dm', 'dm')`,
      )
      .bind(dm)
      .run();
    for (const user of [owner, dmFriend]) {
      await db
        .prepare(
          `INSERT INTO dm_recipients (channel_id, user_id) VALUES (?, ?)`,
        )
        .bind(dm, user)
        .run();
    }
    expect((await sign(owner, dmFriend, "hey")).status).toBe(201);
  });

  it("caps notes per author per day, and applies the per-minute limit", async () => {
    const { owner, friend } = await pair();
    for (let i = 0; i < 5; i += 1) {
      expect((await sign(owner, friend, `note ${i}`)).status).toBe(201);
    }
    const sixth = await sign(owner, friend, "one more");
    expect(sixth.status).toBe(429);
    expect(sixth.body.code).toBe("GUESTBOOK_DAILY_LIMIT");

    // The real rate limiter Durable Object, 3 a minute per author.
    const other = await pair();
    const real: Partial<ProfileBoardDeps> = {
      rateLimit: (userId, action) =>
        checkRateLimitDO(
          userId,
          `profile-board-${action}`,
          PROFILE_BOARD_RATE_LIMITS[action],
        ),
    };
    const statuses: number[] = [];
    for (let i = 0; i < 4; i += 1) {
      statuses.push(
        (await sign(other.owner, other.friend, `quick ${i}`, real)).status,
      );
    }
    expect(statuses).toEqual([201, 201, 201, 429]);
  });

  it("refuses anyone either side blocked, and hides the guestbook from them", async () => {
    const { owner, friend } = await pair();
    await block(owner, friend);
    const refused = await sign(owner, friend, "hi");
    expect(refused.body.code).toBe("GUESTBOOK_BLOCKED");
    const list = await handlers().getGuestbook(
      req("GET", `/api/users/${owner}/guestbook`, friend),
      owner,
    );
    expect(list.status).toBe(403);
    expect((await view(owner, friend)).status).toBe(403);
  });

  it("blocks a poster from the guestbook only, removing their notes", async () => {
    const { owner, friend } = await pair();
    await sign(owner, friend, "first");
    const blocked = await handlers().blockGuestbook(
      req("POST", "/api/users/me/guestbook/blocks", owner, { user_id: friend }),
      "me",
    );
    expect(blocked.status).toBe(200);
    expect((await sign(owner, friend, "again")).body.code).toBe(
      "GUESTBOOK_BLOCKED",
    );
    const page = (await (
      await handlers().getGuestbook(
        req("GET", "/api/users/me/guestbook", owner),
        "me",
      )
    ).json()) as {
      notes: unknown[];
      blocked: Array<{ id: string }>;
    };
    expect(page.notes).toEqual([]);
    expect(page.blocked.map((user) => user.id)).toEqual([friend]);
    // They can still see the board.
    expect((await view(owner, friend)).status).toBe(200);

    await handlers().unblockGuestbook(
      req("DELETE", `/api/users/me/guestbook/blocks/${friend}`, owner),
      "me",
      friend,
    );
    expect((await sign(owner, friend, "back")).status).toBe(201);
  });

  it("turns off: nobody can sign and others see it closed", async () => {
    const { owner, friend } = await pair();
    await sign(owner, friend, "before");
    await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", owner, {
        guestbook_enabled: false,
      }),
      "me",
    );
    expect((await sign(owner, friend, "after")).body.code).toBe(
      "GUESTBOOK_OFF",
    );
    const theirs = (await (
      await handlers().getGuestbook(
        req("GET", `/api/users/${owner}/guestbook`, friend),
        owner,
      )
    ).json()) as {
      enabled: boolean;
      notes: unknown[];
    };
    expect(theirs).toMatchObject({ enabled: false, notes: [] });
    const mine = (await (
      await handlers().getGuestbook(
        req("GET", "/api/users/me/guestbook", owner),
        "me",
      )
    ).json()) as {
      notes: unknown[];
    };
    expect(mine.notes).toHaveLength(1);
  });

  it("lets the owner delete any note and an author only their own", async () => {
    const { owner, friend } = await pair();
    const third = await addUser(`gb-third-${tag()}`);
    await addServer(`gb-srv-${tag()}`, owner, [owner, third]);
    const a = (await sign(owner, friend, "from friend")).body;
    const b = (await sign(owner, third, "from third")).body;
    const del = (actor: string, noteId: string) =>
      handlers().deleteGuestbookNote(
        req("DELETE", `/api/users/${owner}/guestbook/${noteId}`, actor),
        owner,
        noteId,
      );
    expect((await del(third, a.id)).status).toBe(404);
    expect((await del(friend, a.id)).status).toBe(200);
    expect((await del(owner, b.id)).status).toBe(200);
  });

  it("shows the latest notes on the board with whether the viewer can post", async () => {
    const { owner, friend } = await pair();
    const stranger = await addUser(`gb-stranger-${tag()}`);
    await save(owner, { widgets: [{ id: "gb-widget", type: "guestbook" }] }, 0);
    for (const text of ["one", "two", "three", "four"])
      await sign(owner, friend, text);
    const theirs = (await view(owner, friend)).body.data.guestbook;
    expect(theirs?.notes).toHaveLength(3);
    expect(theirs).toMatchObject({
      total: 4,
      can_post: true,
      enabled: true,
      denial: null,
    });
    // Why someone can't sign travels with the board, so the plate can say.
    expect((await view(owner, stranger)).body.data.guestbook).toMatchObject({
      can_post: false,
      denial: "GUESTBOOK_NO_SHARED_SPACE",
    });
    // The owner sees their own notes (and isn't told they're barred).
    expect((await view(owner, owner)).body.data.guestbook).toMatchObject({
      total: 4,
      enabled: true,
      can_post: false,
      denial: "GUESTBOOK_OWN",
    });
  });
});

// ── Activity: writes ───────────────────────────────────────────────────────

async function events(userId: string) {
  const { results } = await db
    .prepare(
      `SELECT id, kind, ref, data, seconds FROM profile_activity WHERE user_id = ? ORDER BY started_at`,
    )
    .bind(userId)
    .all<{
      id: string;
      kind: string;
      ref: string | null;
      data: string;
      seconds: number | null;
    }>();
  return results ?? [];
}

describe("activity writes", () => {
  it("records a game once, however often it is reported, and skips short ones", async () => {
    const user = await addUser(`act-${tag()}`);
    const game = {
      userId: user,
      name: "Deadlock",
      iconUrl: null,
      startedAt: NOW - 3 * 3_600_000,
      endedAt: NOW - 3_600_000,
    };
    // Two sessions (desktop and web) reporting the same game end.
    await recordGameSession(db, game);
    await recordGameSession(db, game);
    await db.batch(gameSessionStatements(db, game));
    await recordGameSession(db, {
      ...game,
      name: "Balatro",
      startedAt: NOW - 60_000,
      endedAt: NOW,
    });

    const rows = await events(user);
    expect(rows).toHaveLength(1);
    expect(rows[0]).toMatchObject({
      kind: "game",
      ref: "deadlock",
      seconds: 7200,
    });
    const rollup = await db
      .prepare(`SELECT seconds, sessions FROM game_sessions WHERE user_id = ?`)
      .bind(user)
      .first<{ seconds: number; sessions: number }>();
    expect(rollup).toEqual({ seconds: 7200, sessions: 1 });
  });

  it("records calls of 15 minutes or more with where they were", async () => {
    const user = await addUser(`act-${tag()}`);
    const server = `srv-${tag()}`;
    await addServer(server, user, [user]);
    const channel = `vc-${tag()}`;
    await addChannel(channel, server, "Lounge");
    const call = {
      userId: user,
      sessionKey: "s1:1",
      channelId: channel,
      startedAt: NOW - 3_600_000,
      endedAt: NOW,
    };
    await recordCallSession(db, "voice", call);
    await recordCallSession(db, "voice", call);
    await recordCallSession(db, "voice", {
      ...call,
      sessionKey: "s2:1",
      startedAt: NOW - 60_000,
    });
    await recordCallSession(db, "stream", {
      ...call,
      sessionKey: "s1:2",
      startedAt: NOW - 10 * 60_000,
    });
    const rows = await events(user);
    expect(rows.map((row) => row.kind)).toEqual(["voice", "stream"]);
    expect(JSON.parse(rows[0].data)).toMatchObject({
      channel_name: "Lounge",
      server_id: server,
    });
  });

  it("records badges from every award path once", async () => {
    const user = await addUser(`act-${tag()}`);
    await awardBadge(
      db,
      user,
      "bug-hunter",
      "admin",
      "2026-11-09T10:00:00.000Z",
    );
    await awardBadge(
      db,
      user,
      "bug-hunter",
      "admin",
      "2026-11-09T11:00:00.000Z",
    );
    // Voice regular through the stats write that earns it.
    await recordVoiceSession(db, user, {
      voiceSeconds: 101 * 3600,
      streamSeconds: 0,
      endedAt: new Date("2026-11-09T11:30:00.000Z"),
    });
    await sweepAutoBadges(db, "2026-11-09T12:00:00.000Z");
    const kinds = (await events(user))
      .filter((row) => row.kind === "badge")
      .map((row) => row.ref)
      .sort();
    expect(kinds).toContain("bug-hunter");
    expect(kinds).toContain("voice-regular");
    expect(new Set(kinds).size).toBe(kinds.length);
  });

  it("records a server join", async () => {
    const user = await addUser(`act-${tag()}`);
    const server = `srv-${tag()}`;
    await addServer(server, user, [user]);
    await recordServerJoin(db, user, server, "2026-11-08T09:00:00.000Z");
    await recordServerJoin(db, user, server, "2026-11-08T09:00:00.000Z");
    const rows = (await events(user)).filter(
      (row) => row.kind === "server_join",
    );
    expect(rows).toHaveLength(1);
    expect(JSON.parse(rows[0].data)).toMatchObject({
      server_name: `Server ${server}`,
    });
  });
});

// ── Activity: retention and privacy ────────────────────────────────────────

async function insertEvent(
  userId: string,
  kind: string,
  ageDays: number,
  data: object = {},
  hidden = 0,
) {
  const id = `${kind}:${userId}:${ageDays}:${tag()}`;
  await db
    .prepare(
      `INSERT INTO profile_activity (id, user_id, kind, ref, data, started_at, hidden) VALUES (?, ?, ?, NULL, ?, ?, ?)`,
    )
    .bind(
      id,
      userId,
      kind,
      JSON.stringify(data),
      new Date(NOW - ageDays * DAY).toISOString(),
      hidden,
    )
    .run();
  return id;
}

async function activity(owner: string, viewer: string, query = "") {
  const response = await handlers().getActivity(
    req("GET", `/api/users/${owner}/activity${query}`, viewer),
    owner,
  );
  return {
    status: response.status,
    body: (await response.json()) as ActivityPage,
  };
}

describe("activity retention", () => {
  it("deletes each owner's events past their retention", async () => {
    const week = await addUser(`ret7-${tag()}`);
    const month = await addUser(`ret30-${tag()}`);
    const quarter = await addUser(`ret90-${tag()}`);
    await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", week, {
        activity: { retention_days: 7 },
      }),
      "me",
    );
    await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", quarter, {
        activity: { retention_days: 90 },
      }),
      "me",
    );
    for (const user of [week, month, quarter]) {
      for (const age of [1, 10, 40, 100]) await insertEvent(user, "game", age);
    }
    await pruneProfileActivity(db, NOW);
    expect(await events(week)).toHaveLength(1);
    expect(await events(month)).toHaveLength(2);
    expect(await events(quarter)).toHaveLength(3);
  });

  it("leaves events past retention out of reads before the cron runs", async () => {
    const user = await addUser(`ret-${tag()}`);
    await insertEvent(user, "badge", 2);
    await insertEvent(user, "badge", 45);
    expect((await activity(user, user)).body.events).toHaveLength(1);
  });

  it("refuses a retention that isn't 7, 30 or 90 days", async () => {
    const user = await addUser(`ret-${tag()}`);
    const response = await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", user, {
        activity: { retention_days: 365 },
      }),
      "me",
    );
    expect(response.status).toBe(400);
  });
});

describe("activity privacy", () => {
  it("hides hidden kinds and events from others, not from the owner", async () => {
    const owner = await addUser(`priv-${tag()}`);
    const viewer = await addUser(`viewer-${tag()}`);
    await insertEvent(owner, "game", 1, { name: "Deadlock" });
    await insertEvent(owner, "server_join", 2, { server_name: "Cozy" });
    const hiddenId = await insertEvent(owner, "badge", 3);
    await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", owner, {
        activity: { hidden_kinds: ["server_join"] },
      }),
      "me",
    );
    const hide = await handlers().patchActivityEvent(
      req("PATCH", `/api/users/me/activity/${hiddenId}`, owner, {
        hidden: true,
      }),
      "me",
      hiddenId,
    );
    expect(hide.status).toBe(200);

    const theirs = (await activity(owner, viewer)).body;
    expect(theirs.events.map((event) => event.kind)).toEqual(["game"]);
    expect(theirs.settings).toBeUndefined();

    const mine = (await activity(owner, owner)).body;
    expect(mine.events.map((event) => event.kind)).toEqual([
      "game",
      "server_join",
      "badge",
    ]);
    expect(mine.events[2].hidden).toBe(true);
    expect(mine.settings?.hidden_kinds).toEqual(["server_join"]);

    // Hiding someone else's event does nothing.
    const notMine = await handlers().patchActivityEvent(
      req("PATCH", `/api/users/me/activity/${hiddenId}`, viewer, {
        hidden: false,
      }),
      "me",
      hiddenId,
    );
    expect(notMine.status).toBe(404);
  });

  it("names a call's channel only to viewers in that server", async () => {
    const owner = await addUser(`priv-${tag()}`);
    const member = await addUser(`member-${tag()}`);
    const outsider = await addUser(`outsider-${tag()}`);
    const server = `srv-${tag()}`;
    await addServer(server, owner, [owner, member]);
    await insertEvent(owner, "voice", 1, {
      channel_id: "vc",
      channel_name: "Lounge",
      server_id: server,
      server_name: "Raid HQ",
    });
    expect((await activity(owner, member)).body.events[0].data).toMatchObject({
      channel_name: "Lounge",
    });
    const redacted = (await activity(owner, outsider)).body.events[0].data;
    expect(redacted.channel_name).toBeUndefined();
    expect(redacted.server_name).toBeUndefined();
  });

  it("pages newest first and filters by kind", async () => {
    const owner = await addUser(`page-${tag()}`);
    for (let i = 0; i < 25; i += 1)
      await insertEvent(owner, i % 2 ? "game" : "badge", i * 0.1);
    const first = (await activity(owner, owner)).body;
    expect(first.events).toHaveLength(20);
    const second = (
      await activity(owner, owner, `?cursor=${first.next_cursor}`)
    ).body;
    expect(second.events).toHaveLength(5);
    expect(second.next_cursor).toBeNull();
    const games = (await activity(owner, owner, "?kind=game")).body;
    expect(games.events.every((event) => event.kind === "game")).toBe(true);
  });

  it("refuses the timeline to anyone either side blocked", async () => {
    const owner = await addUser(`priv-${tag()}`);
    const blocked = await addUser(`blocked-${tag()}`);
    await block(blocked, owner);
    expect((await activity(owner, blocked)).status).toBe(403);
  });
});

// ── Board data from activity ───────────────────────────────────────────────

describe("board data", () => {
  it("fills top games and the last game, and respects a hidden game kind", async () => {
    const owner = await addUser(`data-${tag()}`);
    const viewer = await addUser(`viewer-${tag()}`);
    await recordGameSession(db, {
      userId: owner,
      name: "Deadlock",
      startedAt: NOW - 5 * 3_600_000,
      endedAt: NOW - 3_600_000,
    });
    await recordGameSession(db, {
      userId: owner,
      name: "Balatro",
      startedAt: NOW - 40 * 60_000,
      endedAt: NOW - 10 * 60_000,
    });
    await save(
      owner,
      {
        widgets: [
          { id: "np-1", type: "now_playing" },
          { id: "tg-1", type: "top_games" },
        ],
      },
      0,
    );

    const data = (await view(owner, viewer)).body.data;
    expect(data.top_games?.games.map((game) => game.name)).toEqual([
      "Deadlock",
      "Balatro",
    ]);
    expect(data.last_game?.name).toBe("Balatro");
    expect(data.game_totals?.deadlock).toEqual({ week: 14_400, all: 14_400 });

    await handlers().patchPrefs(
      req("PATCH", "/api/users/me/profile-prefs", owner, {
        activity: { hidden_kinds: ["game"] },
      }),
      "me",
    );
    expect((await view(owner, viewer)).body.data.last_game).toBeNull();
    expect((await view(owner, owner)).body.data.last_game?.name).toBe(
      "Balatro",
    );
  });
});
