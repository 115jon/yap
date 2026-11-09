// Profile connections against miniflare's D1 and the real route handlers:
// the Steam OpenID 2.0 link (start, callback, check_authentication), the
// one-time state and its browser cookie, uniqueness, unlink, what profile
// viewers see, and the rate limits. Kova and Steam are faked at the network
// edge; nothing here talks to a real Steam account.

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
import {
  consumeLinkState,
  createLinkState,
  getVerifiedConnection,
  LINK_STATE_TTL_MS,
  linkConnection,
} from "../../src/services/connection.service";
import {
  parseSteamClaimedId,
  STEAM_OPENID_ENDPOINT,
  steamAccountId,
} from "../../src/services/connections/steam";

type RouteModule = { Route: { options: unknown } };
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;

const ListRoute = (await loadRoute("../../src/routes/api/connections")).Route;
const ProviderRoute = (
  await loadRoute("../../src/routes/api/connections/$provider")
).Route;
const StartRoute = (
  await loadRoute("../../src/routes/api/connections/$provider/start")
).Route;
const CallbackRoute = (
  await loadRoute("../../src/routes/api/connections/$provider/callback")
).Route;
const ProfileRoute = (await loadRoute("../../src/routes/api/users/$id/profile"))
  .Route;

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
const OPENID_NS = "http://specs.openid.net/auth/2.0";
const STEAM_BASE = 76561197960265728n;

/* ------------------------------------------------------------------ */
/* Fakes                                                                */
/* ------------------------------------------------------------------ */

/** Bearer token -> user id. Each test makes its own users. */
const sessions = new Map<string, string>();

type SteamFake = {
  check: "valid" | "invalid" | "down" | "throw";
  /** id64 -> profile XML; missing -> 404. */
  profiles: Map<string, string>;
  checkCalls: Array<{ url: string; body: URLSearchParams }>;
};

let steam: SteamFake;

const realFetch = globalThis.fetch;

function profileXml(id64: string, name: string, avatar: string) {
  return `<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<profile>
  <steamID64>${id64}</steamID64>
  <steamID><![CDATA[${name}]]></steamID>
  <avatarIcon><![CDATA[${avatar.replace("_full", "")}]]></avatarIcon>
  <avatarFull><![CDATA[${avatar}]]></avatarFull>
  <privacyState>public</privacyState>
</profile>`;
}

function installFakes() {
  vi.spyOn(globalThis, "fetch").mockImplementation(async (input, init) => {
    const request = input instanceof Request ? input : null;
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    if (url.startsWith(`${DEFAULT_KOVA_AUTH_URL}/api/auth/get-session`)) {
      const headers = new Headers(init?.headers ?? request?.headers);
      const token = headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
      const id = sessions.get(token);
      return id
        ? Response.json({
            user: { id, username: id, name: id },
            session: { id: `session-${id}` },
          })
        : Response.json({ error: "unauthorized" }, { status: 401 });
    }
    if (url === STEAM_OPENID_ENDPOINT && (init?.method ?? "GET") === "POST") {
      steam.checkCalls.push({
        url,
        body: new URLSearchParams(String(init?.body ?? "")),
      });
      if (steam.check === "throw") throw new TypeError("network down");
      if (steam.check === "down") {
        return new Response("Service Unavailable", { status: 503 });
      }
      return new Response(
        `ns:${OPENID_NS}\nis_valid:${steam.check === "valid"}\n`,
        { status: 200, headers: { "Content-Type": "text/plain" } },
      );
    }
    const profile =
      /^https:\/\/steamcommunity\.com\/profiles\/(\d{17})\?xml=1$/.exec(url);
    if (profile) {
      const xml = steam.profiles.get(profile[1]!);
      return xml
        ? new Response(xml, { headers: { "Content-Type": "text/xml" } })
        : new Response("Not found", { status: 404 });
    }
    if (url.includes("steamcommunity.com") || url.includes("steampowered")) {
      throw new Error(`unexpected Steam call: ${url}`);
    }
    return realFetch(input as RequestInfo, init);
  });
}

/* ------------------------------------------------------------------ */
/* Helpers                                                              */
/* ------------------------------------------------------------------ */

async function run(sql: string, ...binds: unknown[]) {
  await db
    .prepare(sql)
    .bind(...binds)
    .run();
}

/** A fresh user with a session token. */
async function newUser(name = "user") {
  const id = `${name}-${crypto.randomUUID()}`;
  await run(
    `INSERT INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)`,
    id,
    id,
    name,
    new Date().toISOString(),
  );
  const token = `token-${id}`;
  sessions.set(token, id);
  return { id, token };
}

function steamId64(): string {
  const accountId = 1 + Math.floor(Math.random() * 2_000_000_000);
  return String(STEAM_BASE + BigInt(accountId));
}

function randomIp() {
  return `10.${Math.floor(Math.random() * 255)}.${Math.floor(
    Math.random() * 255,
  )}.${Math.floor(Math.random() * 255)}`;
}

function req(
  method: string,
  path: string,
  token: string | null,
  body?: unknown,
  headers: Record<string, string> = {},
) {
  return new Request(path.startsWith("http") ? path : `${BASE}${path}`, {
    method,
    headers: {
      ...(token ? { authorization: `Bearer ${token}` } : {}),
      ...(body !== undefined ? { "content-type": "application/json" } : {}),
      ...headers,
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

/** POST start: the Steam URL, its return_to and the binding cookie. */
async function start(token: string) {
  const response = await handler(
    StartRoute,
    "POST",
  )({
    request: req("POST", "/api/connections/steam/start", token, {}),
    params: { provider: "steam" },
  });
  expect(response.status).toBe(200);
  const { url } = (await response.json()) as { url: string };
  const steamUrl = new URL(url);
  const setCookie = response.headers.get("set-cookie") ?? "";
  const cookie = /__Host-yap_link_steam=([^;]+)/.exec(setCookie)?.[1] ?? "";
  return {
    steamUrl,
    returnTo: steamUrl.searchParams.get("openid.return_to")!,
    cookie,
    setCookie,
  };
}

/** What Steam would append to return_to after a sign-in. */
function assertion(
  returnTo: string,
  id64: string,
  overrides: Record<string, string | null> = {},
): URL {
  const claimed = `https://steamcommunity.com/openid/id/${id64}`;
  const fields: Record<string, string> = {
    "openid.ns": OPENID_NS,
    "openid.mode": "id_res",
    "openid.op_endpoint": STEAM_OPENID_ENDPOINT,
    "openid.claimed_id": claimed,
    "openid.identity": claimed,
    "openid.return_to": returnTo,
    "openid.response_nonce": `${new Date().toISOString().slice(0, 19)}Zab12cd34`,
    "openid.assoc_handle": "1234567890",
    "openid.signed":
      "signed,op_endpoint,claimed_id,identity,return_to,response_nonce,assoc_handle",
    "openid.sig": "c2lnbmF0dXJlLW5vdC1jaGVja2VkLWxvY2FsbHk=",
  };
  const url = new URL(returnTo);
  for (const [key, value] of Object.entries({ ...fields, ...overrides })) {
    if (value === null) url.searchParams.delete(key);
    else url.searchParams.set(key, value);
  }
  return url;
}

async function callback(
  url: URL | string,
  cookie: string | null,
  ip = randomIp(),
) {
  const response = await handler(
    CallbackRoute,
    "GET",
  )({
    request: new Request(url, {
      headers: {
        ...(cookie
          ? { cookie: `other=1; __Host-yap_link_steam=${cookie}` }
          : {}),
        "cf-connecting-ip": ip,
      },
    }),
    params: { provider: "steam" },
  });
  expect(response.status).toBe(303);
  const location = new URL(response.headers.get("location")!, BASE);
  expect(location.pathname).toBe("/chat");
  expect(location.searchParams.get("connection")).toBe("steam");
  return {
    status: location.searchParams.get("connection_status"),
    setCookie: response.headers.get("set-cookie") ?? "",
  };
}

/** Start and finish a link for `token` as Steam account `id64`. */
async function link(token: string, id64: string) {
  const { returnTo, cookie } = await start(token);
  return callback(assertion(returnTo, id64), cookie);
}

async function rows(userId: string) {
  const { results } = await db
    .prepare(`SELECT * FROM user_connections WHERE user_id = ?`)
    .bind(userId)
    .all<Record<string, unknown>>();
  return results;
}

async function listMine(token: string) {
  const response = await handler(
    ListRoute,
    "GET",
  )({
    request: req("GET", "/api/connections", token),
  });
  expect(response.status).toBe(200);
  return (await response.json()) as {
    connections: Array<Record<string, unknown>>;
  };
}

async function viewProfile(viewerToken: string, targetId: string) {
  const response = await handler(
    ProfileRoute,
    "GET",
  )({
    request: req("GET", `/api/users/${targetId}/profile`, viewerToken),
    params: { id: targetId },
  });
  expect(response.status).toBe(200);
  const text = await response.text();
  return {
    text,
    user: (JSON.parse(text) as { user: { connections?: unknown[] } }).user,
  };
}

/* ------------------------------------------------------------------ */
/* Setup                                                                */
/* ------------------------------------------------------------------ */

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
  steam = { check: "valid", profiles: new Map(), checkCalls: [] };
  installFakes();
});

afterEach(() => {
  vi.restoreAllMocks();
});

/* ------------------------------------------------------------------ */
/* Tests                                                                */
/* ------------------------------------------------------------------ */

describe("steam ids", () => {
  it("parses only Steam's own claimed_id into a SteamID64 and account id", () => {
    expect(
      parseSteamClaimedId(
        "https://steamcommunity.com/openid/id/76561197960287930",
      ),
    ).toBe("76561197960287930");
    expect(steamAccountId("76561197960287930")).toBe(22202);
    for (const forged of [
      "http://steamcommunity.com/openid/id/76561197960287930",
      "https://steamcommunity.com.evil.example/openid/id/76561197960287930",
      "https://evil.example/openid/id/76561197960287930",
      "https://steamcommunity.com/openid/id/76561197960287930/",
      "https://steamcommunity.com/openid/id/76561197960287930?x=1",
      "https://steamcommunity.com/openid/id/7656119796028793",
      // Account id 0 and ids outside the individual-account range.
      "https://steamcommunity.com/openid/id/76561197960265728",
      "https://steamcommunity.com/openid/id/12345678901234567",
    ]) {
      expect(parseSteamClaimedId(forged)).toBeNull();
    }
  });
});

describe("POST /api/connections/steam/start", () => {
  it("needs a session", async () => {
    const response = await handler(
      StartRoute,
      "POST",
    )({
      request: req("POST", "/api/connections/steam/start", null, {}),
      params: { provider: "steam" },
    });
    expect(response.status).toBe(401);
  });

  it("refuses an unknown provider", async () => {
    const me = await newUser();
    const response = await handler(
      StartRoute,
      "POST",
    )({
      request: req("POST", "/api/connections/myspace/start", me.token, {}),
      params: { provider: "myspace" },
    });
    expect(response.status).toBe(404);
  });

  it("sends the browser to Steam with a one-time return_to on the Yap host", async () => {
    const me = await newUser();
    const { steamUrl, returnTo, setCookie, cookie } = await start(me.token);
    expect(`${steamUrl.origin}${steamUrl.pathname}`).toBe(
      STEAM_OPENID_ENDPOINT,
    );
    expect(steamUrl.searchParams.get("openid.mode")).toBe("checkid_setup");
    expect(steamUrl.searchParams.get("openid.ns")).toBe(OPENID_NS);
    expect(steamUrl.searchParams.get("openid.realm")).toBe(
      "https://yap.115jon.com",
    );
    expect(returnTo).toMatch(
      /^https:\/\/yap\.115jon\.com\/api\/connections\/steam\/callback\?state=[A-Za-z0-9_-]{43}$/,
    );
    expect(steamUrl.href).not.toContain(".site");
    expect(setCookie).toMatch(/HttpOnly/);
    expect(setCookie).toMatch(/Secure/);
    expect(setCookie).toMatch(/SameSite=Lax/);
    expect(setCookie).toMatch(/^__Host-yap_link_steam=/);
    expect(setCookie).toMatch(/Path=\/;/);
    expect(setCookie).not.toMatch(/Domain=/i);
    expect(cookie).toMatch(/^[A-Za-z0-9_-]{43}$/);
    // Only hashes are stored.
    const state = new URL(returnTo).searchParams.get("state")!;
    const stored = await db
      .prepare(`SELECT * FROM connection_link_states WHERE user_id = ?`)
      .bind(me.id)
      .all<Record<string, unknown>>();
    expect(stored.results).toHaveLength(1);
    expect(JSON.stringify(stored.results[0])).not.toContain(cookie);
    expect(stored.results[0]!.state_hash).not.toBe(state);
  });

  it("keeps one link in flight per user: a new start replaces the last", async () => {
    const me = await newUser();
    const first = await start(me.token);
    const second = await start(me.token);
    const id64 = steamId64();
    expect(
      (await callback(assertion(first.returnTo, id64), first.cookie)).status,
    ).toBe("expired");
    expect(
      (await callback(assertion(second.returnTo, id64), second.cookie)).status,
    ).toBe("linked");
  });
});

describe("GET /api/connections/steam/callback", () => {
  it("links a verified Steam account and stores the 32-bit account id", async () => {
    const me = await newUser();
    const id64 = steamId64();
    steam.profiles.set(
      id64,
      profileXml(
        id64,
        "Gabe & Co <3",
        "https://avatars.steamstatic.com/abc123_full.jpg",
      ),
    );
    const { returnTo, cookie } = await start(me.token);
    const result = await callback(assertion(returnTo, id64), cookie);
    expect(result.status).toBe("linked");
    // The cookie is cleared on the way out.
    expect(result.setCookie).toMatch(/__Host-yap_link_steam=;.*Max-Age=0/);

    // Steam was asked, at the fixed endpoint, with every field echoed.
    expect(steam.checkCalls).toHaveLength(1);
    const body = steam.checkCalls[0]!.body;
    expect(body.get("openid.mode")).toBe("check_authentication");
    expect(body.get("openid.claimed_id")).toBe(
      `https://steamcommunity.com/openid/id/${id64}`,
    );
    expect(body.get("openid.sig")).toBeTruthy();

    const [row] = await rows(me.id);
    expect(row).toMatchObject({
      provider: "steam",
      external_id: id64,
      display_name: "Gabe & Co <3",
      profile_url: `https://steamcommunity.com/profiles/${id64}`,
      avatar_url: "https://avatars.steamstatic.com/abc123_full.jpg",
      show_on_profile: 1,
    });
    const accountId = Number(BigInt(id64) - STEAM_BASE);
    expect(JSON.parse(String(row!.metadata))).toEqual({
      steam_id64: id64,
      account_id: accountId,
    });

    const verified = await getVerifiedConnection(db, me.id, "steam");
    expect(verified).toMatchObject({
      externalId: id64,
      showOnProfile: true,
      metadata: { account_id: accountId },
    });

    // The owner's list carries no external id.
    const { connections } = await listMine(me.token);
    expect(connections).toEqual([
      {
        provider: "steam",
        display_name: "Gabe & Co <3",
        profile_url: `https://steamcommunity.com/profiles/${id64}`,
        avatar_url: "https://avatars.steamstatic.com/abc123_full.jpg",
        show_on_profile: true,
        verified_at: expect.any(String),
      },
    ]);
  });

  it("falls back to the id when the profile XML can't be read, and drops foreign avatars", async () => {
    const me = await newUser();
    const id64 = steamId64();
    await link(me.token, id64);
    expect((await rows(me.id))[0]).toMatchObject({
      display_name: id64,
      avatar_url: null,
    });

    const other = await newUser();
    const id = steamId64();
    steam.profiles.set(
      id,
      profileXml(id, "\u0007Evil‮ name", "https://evil.example/a.jpg"),
    );
    await link(other.token, id);
    expect((await rows(other.id))[0]).toMatchObject({
      display_name: "Evil name",
      avatar_url: null,
    });
  });

  it("ignores a profile XML that is about another account", async () => {
    const me = await newUser();
    const id64 = steamId64();
    steam.profiles.set(
      id64,
      profileXml(
        steamId64(),
        "Someone else",
        "https://avatars.steamstatic.com/x_full.jpg",
      ),
    );
    await link(me.token, id64);
    expect((await rows(me.id))[0]).toMatchObject({ display_name: id64 });
  });

  it("rejects a forged claimed_id without asking Steam", async () => {
    const me = await newUser();
    const id64 = steamId64();
    for (const claimed of [
      `https://evil.example/openid/id/${id64}`,
      `http://steamcommunity.com/openid/id/${id64}`,
      `https://steamcommunity.com/openid/id/${id64}/../../evil`,
    ]) {
      const { returnTo, cookie } = await start(me.token);
      const result = await callback(
        assertion(returnTo, id64, {
          "openid.claimed_id": claimed,
          "openid.identity": claimed,
        }),
        cookie,
      );
      expect(result.status).toBe("invalid");
    }
    // identity and claimed_id must agree.
    const { returnTo, cookie } = await start(me.token);
    expect(
      (
        await callback(
          assertion(returnTo, id64, {
            "openid.identity": `https://steamcommunity.com/openid/id/${steamId64()}`,
          }),
          cookie,
        )
      ).status,
    ).toBe("invalid");
    expect(steam.checkCalls).toHaveLength(0);
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("rejects an assertion from another OpenID endpoint", async () => {
    const me = await newUser();
    const id64 = steamId64();
    const { returnTo, cookie } = await start(me.token);
    const result = await callback(
      assertion(returnTo, id64, {
        "openid.op_endpoint": "https://evil.example/openid/login",
      }),
      cookie,
    );
    expect(result.status).toBe("invalid");
    expect(steam.checkCalls).toHaveLength(0);
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("rejects a wrong namespace, mode, return_to, unsigned fields, an old nonce and repeated fields", async () => {
    const me = await newUser();
    const id64 = steamId64();
    const cases: Array<(returnTo: string) => URL> = [
      (r) =>
        assertion(r, id64, { "openid.ns": "http://openid.net/signon/1.1" }),
      (r) => assertion(r, id64, { "openid.mode": "setup_needed" }),
      (r) =>
        assertion(r, id64, {
          "openid.return_to": `${r}&extra=1`,
        }),
      (r) =>
        assertion(r, id64, {
          "openid.signed": "signed,op_endpoint,claimed_id,identity",
        }),
      (r) => assertion(r, id64, { "openid.sig": null }),
      (r) =>
        assertion(r, id64, {
          "openid.response_nonce": "2020-01-01T00:00:00Zold",
        }),
      (r) => {
        const url = assertion(r, id64);
        url.searchParams.append(
          "openid.claimed_id",
          `https://steamcommunity.com/openid/id/${steamId64()}`,
        );
        return url;
      },
    ];
    for (const make of cases) {
      const { returnTo, cookie } = await start(me.token);
      expect((await callback(make(returnTo), cookie)).status).toBe("invalid");
    }
    expect(steam.checkCalls).toHaveLength(0);
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("links nothing when Steam says the assertion is not valid", async () => {
    const me = await newUser();
    steam.check = "invalid";
    expect((await link(me.token, steamId64())).status).toBe("invalid");
    expect(steam.checkCalls).toHaveLength(1);
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("reports a Steam outage as unavailable", async () => {
    const me = await newUser();
    steam.check = "down";
    expect((await link(me.token, steamId64())).status).toBe("unavailable");
    steam.check = "throw";
    expect((await link(me.token, steamId64())).status).toBe("unavailable");
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("reports a cancelled sign-in", async () => {
    const me = await newUser();
    const { returnTo, cookie } = await start(me.token);
    const url = new URL(returnTo);
    url.searchParams.set("openid.ns", OPENID_NS);
    url.searchParams.set("openid.mode", "cancel");
    expect((await callback(url, cookie)).status).toBe("cancelled");
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("works once: a replayed callback finds no state", async () => {
    const me = await newUser();
    const id64 = steamId64();
    const { returnTo, cookie } = await start(me.token);
    const url = assertion(returnTo, id64);
    expect((await callback(url, cookie)).status).toBe("linked");
    await run(`DELETE FROM user_connections WHERE user_id = ?`, me.id);
    expect((await callback(url, cookie)).status).toBe("expired");
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("refuses an expired state", async () => {
    const me = await newUser();
    const started = await createLinkState(db, {
      userId: me.id,
      provider: "steam",
      callbackUrl: "https://yap.115jon.com/api/connections/steam/callback",
      now: Date.now() - LINK_STATE_TTL_MS - 1000,
    });
    expect(
      (
        await callback(
          assertion(started.returnTo, steamId64()),
          started.binding,
        )
      ).status,
    ).toBe("expired");
    expect(steam.checkCalls).toHaveLength(0);
  });

  it("refuses a callback from a browser that didn't start the link, and burns the state", async () => {
    const victim = await newUser("victim");
    const attacker = await newUser("attacker");
    const id64 = steamId64();
    // The attacker starts a link and sends the Steam URL to the victim; the
    // victim's browser has no cookie for it.
    const { returnTo, cookie } = await start(attacker.token);
    const url = assertion(returnTo, id64);
    expect((await callback(url, null)).status).toBe("expired");
    // A wrong cookie fails the same way, and the state is gone after either.
    expect((await callback(url, "x".repeat(43))).status).toBe("expired");
    expect((await callback(url, cookie)).status).toBe("expired");
    expect(await rows(attacker.id)).toHaveLength(0);
    expect(await rows(victim.id)).toHaveLength(0);
    expect(steam.checkCalls).toHaveLength(0);
  });

  it("refuses a missing or malformed state", async () => {
    const me = await newUser();
    const { cookie } = await start(me.token);
    const base = `${BASE}/api/connections/steam/callback`;
    for (const href of [
      base,
      `${base}?state=short`,
      `${base}?state=${"a".repeat(43)}`,
    ]) {
      const status = (await callback(assertion(href, steamId64()), cookie))
        .status;
      expect(["invalid", "expired"]).toContain(status);
    }
    expect(steam.checkCalls).toHaveLength(0);
  });

  it("consumes a state for its own provider only", async () => {
    const me = await newUser();
    const started = await createLinkState(db, {
      userId: me.id,
      provider: "steam",
      callbackUrl: "https://yap.115jon.com/api/connections/steam/callback",
    });
    const nonce = new URL(started.returnTo).searchParams.get("state");
    // A provider the row doesn't name is refused (and the row consumed).
    const other = await consumeLinkState(db, {
      provider: "twitch" as never,
      nonce,
      binding: started.binding,
    });
    expect(other).toEqual({ ok: false, reason: "invalid" });
  });
});

describe("uniqueness", () => {
  it("links one Steam account to one Yap account", async () => {
    const first = await newUser("first");
    const second = await newUser("second");
    const id64 = steamId64();
    expect((await link(first.token, id64)).status).toBe("linked");
    expect((await link(second.token, id64)).status).toBe("taken");
    expect(await rows(second.id)).toHaveLength(0);
    expect((await rows(first.id))[0]).toMatchObject({ external_id: id64 });

    // The constraint holds below the service too.
    await expect(
      run(
        `INSERT INTO user_connections (user_id, provider, external_id, display_name, verified_at, created_at, updated_at) VALUES (?, 'steam', ?, 'x', 'now', 'now', 'now')`,
        second.id,
        id64,
      ),
    ).rejects.toThrow(/UNIQUE/);
  });

  it("keeps one account per provider: linking another replaces it and keeps the profile choice", async () => {
    const me = await newUser();
    const firstId = steamId64();
    await link(me.token, firstId);
    await handler(
      ProviderRoute,
      "PATCH",
    )({
      request: req("PATCH", "/api/connections/steam", me.token, {
        showOnProfile: false,
      }),
      params: { provider: "steam" },
    });
    const secondId = steamId64();
    expect((await link(me.token, secondId)).status).toBe("linked");
    const all = await rows(me.id);
    expect(all).toHaveLength(1);
    expect(all[0]).toMatchObject({ external_id: secondId, show_on_profile: 0 });

    // The freed account can go to someone else now.
    const other = await newUser();
    expect((await link(other.token, firstId)).status).toBe("linked");

    await expect(
      run(
        `INSERT INTO user_connections (user_id, provider, external_id, display_name, verified_at, created_at, updated_at) VALUES (?, 'steam', ?, 'x', 'now', 'now', 'now')`,
        me.id,
        steamId64(),
      ),
    ).rejects.toThrow(/UNIQUE|PRIMARY/);
  });

  it("reports a race on the unique index as taken", async () => {
    const a = await newUser();
    const b = await newUser();
    const id64 = steamId64();
    const account = {
      externalId: id64,
      displayName: "x",
      profileUrl: null,
      avatarUrl: null,
      metadata: {},
    };
    expect(await linkConnection(db, a.id, "steam", account)).toEqual({
      ok: true,
    });
    expect(await linkConnection(db, b.id, "steam", account)).toEqual({
      ok: false,
      reason: "taken",
    });
  });
});

describe("show on profile and unlink", () => {
  it("toggles show_on_profile, and 404s with nothing linked", async () => {
    const me = await newUser();
    const patch = (token: string, body: unknown) =>
      handler(
        ProviderRoute,
        "PATCH",
      )({
        request: req("PATCH", "/api/connections/steam", token, body),
        params: { provider: "steam" },
      });
    expect((await patch(me.token, { showOnProfile: false })).status).toBe(404);
    await link(me.token, steamId64());
    expect((await patch(me.token, { showOnProfile: "no" })).status).toBe(400);
    expect((await patch(me.token, { showOnProfile: false })).status).toBe(200);
    expect((await listMine(me.token)).connections[0]).toMatchObject({
      show_on_profile: false,
    });
    expect((await patch(me.token, { showOnProfile: true })).status).toBe(200);
    expect((await rows(me.id))[0]).toMatchObject({ show_on_profile: 1 });
  });

  it("unlinks, idempotently", async () => {
    const me = await newUser();
    const id64 = steamId64();
    await link(me.token, id64);
    const unlink = () =>
      handler(
        ProviderRoute,
        "DELETE",
      )({
        request: req("DELETE", "/api/connections/steam", me.token),
        params: { provider: "steam" },
      });
    const first = await unlink();
    expect(await first.json()).toEqual({ provider: "steam", removed: true });
    expect(await rows(me.id)).toHaveLength(0);
    expect(await getVerifiedConnection(db, me.id, "steam")).toBeNull();
    expect((await listMine(me.token)).connections).toEqual([]);
    const again = await unlink();
    expect(await again.json()).toEqual({ provider: "steam", removed: false });

    // Another Yap account can link it now.
    const other = await newUser();
    expect((await link(other.token, id64)).status).toBe("linked");
  });

  it("needs a session and a known provider", async () => {
    const me = await newUser();
    const unauthorized = await handler(
      ProviderRoute,
      "DELETE",
    )({
      request: req("DELETE", "/api/connections/steam", null),
      params: { provider: "steam" },
    });
    expect(unauthorized.status).toBe(401);
    const unknown = await handler(
      ProviderRoute,
      "DELETE",
    )({
      request: req("DELETE", "/api/connections/myspace", me.token),
      params: { provider: "myspace" },
    });
    expect(unknown.status).toBe(404);
  });
});

describe("what profile viewers see", () => {
  it("shows provider, name, profile URL and avatar, and no external id", async () => {
    const owner = await newUser("owner");
    const viewer = await newUser("viewer");
    const id64 = steamId64();
    steam.profiles.set(
      id64,
      profileXml(id64, "Owner", "https://avatars.steamstatic.com/o_full.jpg"),
    );
    await link(owner.token, id64);

    const { user, text } = await viewProfile(viewer.token, owner.id);
    expect(user.connections).toEqual([
      {
        provider: "steam",
        display_name: "Owner",
        profile_url: `https://steamcommunity.com/profiles/${id64}`,
        avatar_url: "https://avatars.steamstatic.com/o_full.jpg",
      },
    ]);
    // The id appears only inside the profile URL; no account id, metadata,
    // verification time or the switch.
    const accountId = String(BigInt(id64) - STEAM_BASE);
    expect(text.split(id64)).toHaveLength(2);
    expect(text).not.toContain(`"${accountId}"`);
    expect(text).not.toContain(`:${accountId}`);
    for (const key of [
      "external_id",
      "account_id",
      "metadata",
      "verified_at",
      "show_on_profile",
      "steam_id64",
    ]) {
      expect(text).not.toContain(key);
    }
  });

  it("hides a connection the owner turned off, from everyone", async () => {
    const owner = await newUser("owner");
    const viewer = await newUser("viewer");
    await link(owner.token, steamId64());
    await handler(
      ProviderRoute,
      "PATCH",
    )({
      request: req("PATCH", "/api/connections/steam", owner.token, {
        showOnProfile: false,
      }),
      params: { provider: "steam" },
    });
    expect(
      (await viewProfile(viewer.token, owner.id)).user.connections,
    ).toEqual([]);
    expect((await viewProfile(owner.token, owner.id)).user.connections).toEqual(
      [],
    );
    // Still linked, for the owner and for server features.
    expect((await listMine(owner.token)).connections).toHaveLength(1);
    expect(await getVerifiedConnection(db, owner.id, "steam")).not.toBeNull();
  });

  it("shows nothing to someone the owner blocked", async () => {
    const owner = await newUser("owner");
    const blocked = await newUser("blocked");
    const friend = await newUser("friend");
    await link(owner.token, steamId64());
    const now = new Date().toISOString();
    await run(
      `INSERT INTO relationships (user_id, target_user_id, type, created_at, updated_at) VALUES (?, ?, 1, ?, ?)`,
      owner.id,
      blocked.id,
      now,
      now,
    );
    expect(
      (await viewProfile(blocked.token, owner.id)).user.connections,
    ).toEqual([]);
    expect(
      (await viewProfile(friend.token, owner.id)).user.connections,
    ).toHaveLength(1);
  });
});

describe("rate limits", () => {
  it("limits starts per user", async () => {
    const me = await newUser();
    const statuses: number[] = [];
    for (let index = 0; index < 11; index += 1) {
      const response = await handler(
        StartRoute,
        "POST",
      )({
        request: req("POST", "/api/connections/steam/start", me.token, {}),
        params: { provider: "steam" },
      });
      statuses.push(response.status);
    }
    expect(statuses.slice(0, 10).every((status) => status === 200)).toBe(true);
    expect(statuses[10]).toBe(429);
  });

  it("limits callbacks per IP, before touching any state", async () => {
    const me = await newUser();
    const ip = randomIp();
    const href = `${BASE}/api/connections/steam/callback?state=${"a".repeat(43)}`;
    const outcomes: Array<string | null> = [];
    for (let index = 0; index < 20; index += 1) {
      outcomes.push((await callback(href, null, ip)).status);
    }
    expect(outcomes.every((status) => status === "expired")).toBe(true);
    // Over the limit, a real link from this IP is refused too.
    const { returnTo, cookie } = await start(me.token);
    expect(
      (await callback(assertion(returnTo, steamId64()), cookie, ip)).status,
    ).toBe("rate_limited");
    expect(await rows(me.id)).toHaveLength(0);
  });

  it("limits toggles and unlinks per user", async () => {
    const me = await newUser();
    const statuses: number[] = [];
    for (let index = 0; index < 31; index += 1) {
      const response = await handler(
        ProviderRoute,
        "DELETE",
      )({
        request: req("DELETE", "/api/connections/steam", me.token),
        params: { provider: "steam" },
      });
      statuses.push(response.status);
    }
    expect(statuses.slice(0, 30).every((status) => status === 200)).toBe(true);
    expect(statuses[30]).toBe(429);
  });
});
