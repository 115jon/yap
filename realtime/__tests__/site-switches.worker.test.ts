import { env } from "cloudflare:workers";
import { Effect, Exit } from "effect";
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
import { DEFAULT_KOVA_AUTH_URL } from "../../src/lib/site-defaults";
import { invalidateSuspensionCache } from "../../src/lib/suspension";
import { signedInUserId } from "../../src/server/kova-auth";
import { memoryAuth } from "../../src/server/testing";
import { extractAndProcessEmbedsWithStatus } from "../../src/services/embed-fetcher";
import { invalidateSiteSettingsCache } from "../../src/services/site-flags.service";

// Route modules load at runtime by path: their createFileRoute types need
// the app's generated route tree, which this tsconfig does not include.
type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const UploadsRoute = (
  await loadRoute("../../src/routes/api/channels/$id/uploads")
).Route;
const MessageUploadRoute = (
  await loadRoute("../../src/routes/api/channels/$id/messages/upload")
).Route;
const PrefetchRoute = (await loadRoute("../../src/routes/api/embeds/prefetch"))
  .Route;
const ResolveRoute = (
  await loadRoute("../../src/routes/api/listen-together/resolve")
).Route;
const SearchRoute = (
  await loadRoute("../../src/routes/api/listen-together/search")
).Route;
const StreamRoute = (
  await loadRoute("../../src/routes/api/listen-together/stream")
).Route;
const SiteConfigRoute = (await loadRoute("../../src/routes/api/site-config"))
  .Route;
const MeRoute = (await loadRoute("../../src/routes/api/users/me")).Route;
const NativeHandoffRoute = (
  await loadRoute("../../src/routes/api/desktop/native-handoff")
).Route;

// What the main Worker enforces from the admin console's writes (apps/admin):
// suspensions and the kill switches, against miniflare's D1 and the real
// route handlers. Kova is faked at the network edge (its get-session
// call): `Bearer <token>` maps to a user id in SESSIONS.

const db = env.DB as unknown as D1Database;
const BASE = "https://yap.test";

const OTHER = "other-user";
const PLAIN = "plain-user";
const TARGET = "target-user";
const SUSPENDED = "suspended-user";
const NEWCOMER = "brand-new-user";

const SESSIONS: Record<string, { id: string; username?: string }> = {
  "other-token": { id: OTHER, username: "other" },
  "plain-token": { id: PLAIN, username: "plain" },
  "target-token": { id: TARGET, username: "target" },
  "suspended-token": { id: SUSPENDED, username: "suspended" },
  "newcomer-token": { id: NEWCOMER, username: "newcomer" },
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
  headers: Record<string, string> = {},
): Request {
  return new Request(`${BASE}${path}`, {
    method,
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body !== undefined ? { "content-type": "application/json" } : {}),
      "cf-ray": "test-ray-1",
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

const realFetch = globalThis.fetch;

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
      if (url.startsWith(`${DEFAULT_KOVA_AUTH_URL}/api/auth/get-session`)) {
        const headers = new Headers(
          init?.headers ??
            (input instanceof Request ? input.headers : undefined),
        );
        const token =
          headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
        const user = SESSIONS[token];
        return user
          ? Response.json({
              user: {
                id: user.id,
                username: user.username,
                name: user.username,
              },
              session: { id: `session-${user.id}` },
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

async function setFlag(flag: string, on: boolean) {
  const current = await db
    .prepare(`SELECT value FROM site_settings WHERE key = 'flags'`)
    .first<{ value: string }>();
  const flags = { ...(current ? JSON.parse(current.value) : {}), [flag]: on };
  await run(
    `INSERT INTO site_settings (key, value, updated_at, updated_by) VALUES ('flags', ?, ?, 'test')
     ON CONFLICT(key) DO UPDATE SET value = excluded.value`,
    JSON.stringify(flags),
    new Date().toISOString(),
  );
  invalidateSiteSettingsCache();
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
  const now = new Date().toISOString();
  for (const [id, username] of [
    [OTHER, "other"],
    [PLAIN, "plain"],
    [TARGET, "target"],
    [SUSPENDED, "suspended"],
  ]) {
    await run(
      `INSERT OR IGNORE INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)`,
      id,
      username,
      username,
      now,
    );
  }
  // A DM between the target and the admin, for the upload paths.
  await run(
    `INSERT OR IGNORE INTO channels (id, server_id, name, channel_type) VALUES ('dm-1', NULL, 'dm', 'dm')`,
  );
  await run(
    `INSERT OR IGNORE INTO dm_recipients (channel_id, user_id) VALUES ('dm-1', ?)`,
    TARGET,
  );
  await run(
    `INSERT OR IGNORE INTO dm_recipients (channel_id, user_id) VALUES ('dm-1', ?)`,
    OTHER,
  );
});

beforeEach(() => {
  fakeKova();
  invalidateSiteSettingsCache();
  invalidateSuspensionCache();
});

afterEach(() => {
  vi.restoreAllMocks();
  setTestRequestHeaders(null);
});

describe("suspension (written by the admin Worker)", () => {
  async function setSuspended(userId: string, suspended: boolean) {
    await run(
      `UPDATE users SET suspended_at = ?, suspended_reason = ? WHERE id = ?`,
      suspended ? new Date().toISOString() : null,
      suspended ? "spam" : null,
      userId,
    );
    invalidateSuspensionCache();
  }

  it("refuses a suspended account at the auth path, then lets it back in", async () => {
    const search = handler(SearchRoute, "GET");
    const asSuspended = () =>
      search({
        request: req(
          "GET",
          "/api/listen-together/search?q=a",
          "suspended-token",
        ),
      });

    // Before: auth passes (the route then wants a room).
    expect((await asSuspended()).status).toBe(400);

    await setSuspended(SUSPENDED, true);
    const refused = await asSuspended();
    expect(refused.status).toBe(403);
    expect(await refused.json()).toMatchObject({ code: "ACCOUNT_SUSPENDED" });
    // Someone else is untouched.
    expect(
      (
        await search({
          request: req("GET", "/api/listen-together/search?q=a", "plain-token"),
        })
      ).status,
    ).toBe(400);

    // The edge (socket tickets, DMs, calls) refuses them too.
    const edge = await Effect.runPromiseExit(
      signedInUserId(req("GET", "/api/gateway", "x")).pipe(
        Effect.provide(memoryAuth({ "Bearer x": SUSPENDED })),
      ),
    );
    expect(Exit.isFailure(edge)).toBe(true);
    expect(JSON.stringify(edge)).toContain("ACCOUNT_SUSPENDED");

    await setSuspended(SUSPENDED, false);
    expect((await asSuspended()).status).toBe(400);
  });

  it("fails open when the suspension list cannot be read", async () => {
    const { suspendedUserIds } = await import("../../src/lib/suspension");
    invalidateSuspensionCache();
    const broken = {
      prepare: () => {
        throw new Error("D1 down");
      },
    } as unknown as D1Database;
    expect((await suspendedUserIds(broken)).size).toBe(0);
  });
});

// ── Kill switches ───────────────────────────────────────────────────────────

async function expectPaused(response: Response) {
  expect(response.status).toBe(503);
  expect(await response.json()).toMatchObject({ code: "FEATURE_PAUSED" });
}

describe("the public site config", () => {
  it("serves the switches and the live banner, and drops an ended one", async () => {
    const at = new Date().toISOString();
    await setFlag("listenTogether", false);
    await run(
      `INSERT OR REPLACE INTO site_settings (key, value, updated_at, updated_by) VALUES ('announcement', ?, ?, 'test')`,
      JSON.stringify({
        id: "a1",
        text: "Maintenance at 22:00 UTC",
        tone: "warning",
        updated_at: at,
      }),
      at,
    );
    invalidateSiteSettingsCache();
    const config = await handler(
      SiteConfigRoute,
      "GET",
    )({
      request: req("GET", "/api/site-config", null),
    });
    expect(await config.json()).toMatchObject({
      flags: { listenTogether: false },
      announcement: { text: "Maintenance at 22:00 UTC", tone: "warning" },
    });
    await run(
      `UPDATE site_settings SET value = ? WHERE key = 'announcement'`,
      JSON.stringify({
        id: "a1",
        text: "Old",
        tone: "info",
        expires_at: "2020-01-01T00:00:00Z",
      }),
    );
    invalidateSiteSettingsCache();
    const ended = await handler(
      SiteConfigRoute,
      "GET",
    )({
      request: req("GET", "/api/site-config", null),
    });
    expect(await ended.json()).toMatchObject({ announcement: null });
    await run(`DELETE FROM site_settings WHERE key = 'announcement'`);
    await setFlag("listenTogether", true);
  });
});

describe("the desktop native handoff switch", () => {
  const read = async () => {
    invalidateSiteSettingsCache();
    const response = await handler(
      NativeHandoffRoute,
      "GET",
    )({ request: req("GET", "/api/desktop/native-handoff", null) });
    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=60");
    return response.json();
  };
  const store = (value: string) =>
    run(
      `INSERT OR REPLACE INTO site_settings (key, value, updated_at, updated_by) VALUES ('nativeHandoff', ?, ?, 'test')`,
      value,
      new Date().toISOString(),
    );

  afterEach(async () => {
    await run(`DELETE FROM site_settings WHERE key = 'nativeHandoff'`);
    invalidateSiteSettingsCache();
  });

  it("is off at 0 % until it is set", async () => {
    await run(`DELETE FROM site_settings WHERE key = 'nativeHandoff'`);
    expect(await read()).toEqual({ enabled: false, rolloutPercent: 0 });
  });

  it("serves what the admin console stored, and nothing else", async () => {
    await store(JSON.stringify({ enabled: true, rolloutPercent: 25 }));
    expect(await read()).toEqual({ enabled: true, rolloutPercent: 25 });
  });

  it("serves off for a stored value it cannot read", async () => {
    for (const value of [
      "not json",
      JSON.stringify({ enabled: "true", rolloutPercent: 50 }),
      JSON.stringify({ enabled: true, rolloutPercent: 500 }),
    ]) {
      await store(value);
      expect(await read()).toEqual({ enabled: false, rolloutPercent: 0 });
    }
  });
});

describe("kill switches block their paths", () => {
  afterEach(async () => {
    for (const flag of [
      "chunkedUploads",
      "listenTogether",
      "linkEmbeds",
      "soundboardUploads",
      "signups",
    ]) {
      await setFlag(flag, true);
    }
  });

  it("chunked uploads: create is refused, before auth", async () => {
    const create = handler(UploadsRoute, "POST");
    const call = () =>
      create({
        request: req("POST", "/api/channels/dm-1/uploads", "target-token", {
          filename: "clip.mp4",
          size: 20 * 1024 * 1024,
          content_type: "video/mp4",
        }),
        params: { id: "dm-1" },
      });
    await setFlag("chunkedUploads", false);
    await expectPaused(await call());
    await setFlag("chunkedUploads", true);
    expect((await call()).status).not.toBe(503);
  });

  it("Listen Together: resolve, search and stream are refused", async () => {
    await setFlag("listenTogether", false);
    await expectPaused(
      await handler(
        ResolveRoute,
        "POST",
      )({
        request: req("POST", "/api/listen-together/resolve", "target-token", {
          roomSlug: "r",
          url: "https://youtu.be/x",
        }),
      }),
    );
    await expectPaused(
      await handler(
        SearchRoute,
        "GET",
      )({
        request: req("GET", "/api/listen-together/search?q=a", "target-token"),
      }),
    );
    await expectPaused(
      await handler(
        StreamRoute,
        "GET",
      )({
        request: req(
          "GET",
          "/api/listen-together/stream?videoId=x",
          "target-token",
        ),
      }),
    );
    await setFlag("listenTogether", true);
    expect(
      (
        await handler(
          SearchRoute,
          "GET",
        )({
          request: req(
            "GET",
            "/api/listen-together/search?q=a",
            "target-token",
          ),
        })
      ).status,
    ).toBe(400);
  });

  it("link embeds: the prefetch route is refused and nothing is fetched", async () => {
    await setFlag("linkEmbeds", false);
    await expectPaused(
      await handler(
        PrefetchRoute,
        "POST",
      )({
        request: req("POST", "/api/embeds/prefetch", "target-token", {
          urls: ["https://example.com/a"],
        }),
      }),
    );
    const fetchSpy = vi.mocked(globalThis.fetch);
    fetchSpy.mockClear();
    const result = await extractAndProcessEmbedsWithStatus(
      "look https://example.com/page",
    );
    expect(result).toEqual({ embeds: [], hadFailures: false, outcomes: [] });
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("soundboard uploads are refused; message attachments are not", async () => {
    setTestRequestHeaders({ authorization: "Bearer target-token" });
    const upload = handler(MessageUploadRoute, "POST");
    const form = (purpose?: string) => {
      const data = new FormData();
      data.set(
        "file",
        new File([new Uint8Array(64)], "honk.mp3", { type: "audio/mpeg" }),
      );
      if (purpose) data.set("purpose", purpose);
      return new Request(`${BASE}/api/channels/dm-1/messages/upload`, {
        method: "POST",
        body: data,
      });
    };
    await setFlag("soundboardUploads", false);
    await expectPaused(
      await upload({ request: form("soundboard"), params: { id: "dm-1" } }),
    );
    await setFlag("soundboardUploads", true);
    // Past the switch: a DM is not a server, which is the next check.
    const passed = await upload({
      request: form("soundboard"),
      params: { id: "dm-1" },
    });
    expect(passed.status).toBe(400);
  });

  it("sign-ups: a first sign-in makes no profile while paused", async () => {
    const me = handler(MeRoute, "GET");
    await setFlag("signups", false);
    await expectPaused(
      await me({ request: req("GET", "/api/users/me", "newcomer-token") }),
    );
    expect(
      await db
        .prepare(`SELECT 1 FROM users WHERE id = ?`)
        .bind(NEWCOMER)
        .first(),
    ).toBeNull();
    // Existing accounts are unaffected.
    expect(
      (await me({ request: req("GET", "/api/users/me", "target-token") }))
        .status,
    ).toBe(200);
    await setFlag("signups", true);
    expect(
      (await me({ request: req("GET", "/api/users/me", "newcomer-token") }))
        .status,
    ).toBe(200);
    expect(
      await db
        .prepare(`SELECT 1 FROM users WHERE id = ?`)
        .bind(NEWCOMER)
        .first(),
    ).not.toBeNull();
  });
});
