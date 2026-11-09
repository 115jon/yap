import { env } from "cloudflare:workers";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import schema from "../../../d1_schema.sql?raw";
import { clearGateCache, SESSION_COOKIE } from "../src/server/gate";
import {
  handleAdminRequest,
  resetAdminWorkerCache,
  routeApi,
  type AdminEnv,
} from "../src/server/worker";

// The admin Worker (yap-admin) end to end against miniflare's D1 and the
// main Worker's Durable Object classes (bound by wrangler.test.toml, as the
// cross-script bindings are in production). Kova is faked at its
// get-session call: a token is valid only with the "Yap Admin" app's
// publishable key, as app-scoped sessions are.

const db = env.DB as unknown as D1Database;
const BASE = "https://yap-admin.test";
const ADMIN_KEY = "pk_admin_test";

const ADMIN = "admin-user";
const SUSPENDED_ADMIN = "second-admin";
const PLAIN = "plain-user";
const TARGET = "target-user";

/** token -> Kova user id, for the admin app only. */
const KOVA_SESSIONS: Record<string, string> = {
  "admin-token": ADMIN,
  "suspended-admin-token": SUSPENDED_ADMIN,
  "plain-token": PLAIN,
  // A real admin, but a session of another Kova app (Yap itself).
  "yap-app-token": ADMIN,
};

const fakeKova: typeof fetch = async (input, init) => {
  const headers = new Headers(init?.headers);
  const token = headers.get("authorization")?.replace(/^Bearer /, "") ?? "";
  const key = headers.get("x-publishable-key");
  const userId = KOVA_SESSIONS[token];
  const valid =
    userId &&
    (token === "yap-app-token" ? key === "pk_yap" : key === ADMIN_KEY);
  return valid
    ? Response.json({ user: { id: userId }, session: { id: `s-${token}` } })
    : Response.json({ error: "unauthorized" }, { status: 401 });
};

const FILES: Record<string, { body: string; type: string }> = {
  "/index.html": {
    body: '<meta name="kova-key" content="__KOVA_ADMIN_PUBLISHABLE_KEY__"><title>Sign in</title><script src="/assets/boot.js"></script>',
    type: "text/html",
  },
  "/manifest.json": {
    body: JSON.stringify({
      "index.html": { file: "assets/boot.js", imports: ["_shared.js"] },
      "_shared.js": { file: "assets/shared.js" },
      "src/console/app.tsx": {
        file: "assets/console.js",
        isEntry: true,
        imports: ["_shared.js", "_core.js"],
      },
      "_core.js": { file: "assets/core.js", css: ["assets/console.css"] },
    }),
    type: "application/json",
  },
  "/assets/boot.js": { body: "boot()", type: "text/javascript" },
  "/assets/shared.js": { body: "shared()", type: "text/javascript" },
  "/assets/console.js": { body: "console()", type: "text/javascript" },
  "/assets/core.js": { body: "core()", type: "text/javascript" },
  "/assets/console.css": { body: "body{}", type: "text/css" },
};

const ASSETS = {
  fetch: async (input: RequestInfo | URL) => {
    const url = new URL(input instanceof Request ? input.url : String(input));
    const file = FILES[url.pathname];
    return file
      ? new Response(file.body, { headers: { "Content-Type": file.type } })
      : new Response("asset not found", { status: 404 });
  },
} as unknown as Fetcher;

const adminEnv = {
  ...(env as unknown as AdminEnv),
  ASSETS,
  SITE_ADMIN_USER_IDS: `${ADMIN}, ${SUSPENDED_ADMIN}`,
  KOVA_ADMIN_PUBLISHABLE_KEY: ADMIN_KEY,
} as AdminEnv;

function call(
  method: string,
  path: string,
  {
    cookie,
    bearer,
    body,
    origin = BASE,
  }: {
    cookie?: string;
    bearer?: string;
    body?: unknown;
    origin?: string | null;
  } = {},
): Promise<Response> {
  const headers = new Headers({ "cf-ray": "test-ray-1" });
  if (cookie) headers.set("cookie", `${SESSION_COOKIE}=${cookie}`);
  if (bearer) headers.set("authorization", `Bearer ${bearer}`);
  if (origin) headers.set("origin", origin);
  if (body !== undefined) headers.set("content-type", "application/json");
  return handleAdminRequest(
    new Request(`${BASE}${path}`, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    }),
    adminEnv,
    { fetch: fakeKova },
  );
}

/** Everything a refusal shows a client: status, headers and body. */
async function fingerprint(response: Response): Promise<string> {
  return JSON.stringify({
    status: response.status,
    headers: [...response.headers].sort(),
    body: await response.text(),
  });
}

async function run(sql: string, ...binds: unknown[]) {
  await db
    .prepare(sql)
    .bind(...binds)
    .run();
}

async function auditRows(action?: string) {
  const { results } = await db
    .prepare(
      `SELECT * FROM admin_audit_log ${action ? "WHERE action = ?" : ""} ORDER BY created_at DESC`,
    )
    .bind(...(action ? [action] : []))
    .all<Record<string, string | null>>();
  return results ?? [];
}

beforeAll(async () => {
  const statements = schema
    .split("\n")
    .map((line) => line.replace(/--.*$/, ""))
    .join("\n")
    .split(";")
    .map((statement) => statement.trim())
    .filter(Boolean);
  for (const statement of statements) await db.prepare(statement).run();
  const now = new Date().toISOString();
  for (const id of [ADMIN, SUSPENDED_ADMIN, PLAIN, TARGET]) {
    await run(
      `INSERT OR IGNORE INTO users (id, username, display_name, created_at) VALUES (?, ?, ?, ?)`,
      id,
      id,
      id,
      now,
    );
  }
  await run(
    `UPDATE users SET suspended_at = ? WHERE id = ?`,
    now,
    SUSPENDED_ADMIN,
  );
});

beforeEach(() => {
  clearGateCache();
  resetAdminWorkerCache();
});

// ── The gate ────────────────────────────────────────────────────────────────

const API: Array<[string, string, unknown?]> = [
  ["GET", "/api/session"],
  ["POST", "/api/sign-out"],
  ["GET", "/api/users"],
  ["GET", `/api/users/${TARGET}`],
  ["POST", `/api/users/${TARGET}/plan`, { tier: "founder" }],
  [
    "POST",
    `/api/users/${TARGET}/badges`,
    { badge_id: "bug-hunter", action: "award" },
  ],
  ["POST", `/api/users/${TARGET}/suspension`, { suspended: true }],
  ["POST", `/api/users/${TARGET}/disconnect`],
  ["GET", "/api/live"],
  ["GET", "/api/insights?days=7"],
  ["GET", "/api/settings"],
  ["PUT", "/api/settings", { flags: { signups: false } }],
  ["GET", "/api/audit"],
  ["GET", "/api/nope"],
];

/** The console's client routes (console/router.tsx): admins only. */
const ROUTES = [
  "/people",
  "/people/",
  `/people/${TARGET}`,
  "/live",
  "/insights",
  "/switches",
  "/audit",
];

const PAGES = [
  ...ROUTES,
  "/console.html",
  "/index.html",
  "/assets/console.js",
  "/assets/core.js",
  "/assets/console.css",
  "/manifest.json",
  "/nope",
  "/.env",
];

describe("the Kova gate", () => {
  it("answers every refusal with the same bare 404", async () => {
    const reference = await fingerprint(await call("GET", "/nope"));
    expect(JSON.parse(reference)).toEqual({
      status: 404,
      headers: [],
      body: "",
    });

    const callers: Array<{ name: string; cookie?: string }> = [
      { name: "no token" },
      { name: "invalid token", cookie: "not-a-real-token" },
      { name: "valid non-admin", cookie: "plain-token" },
      { name: "suspended admin", cookie: "suspended-admin-token" },
      { name: "admin of another Kova app", cookie: "yap-app-token" },
    ];
    for (const caller of callers) {
      for (const [method, path, body] of API) {
        const response = await call(method, path, {
          cookie: caller.cookie,
          body,
        });
        expect(
          await fingerprint(response),
          `${caller.name} ${method} ${path}`,
        ).toBe(reference);
      }
      for (const path of PAGES) {
        const response = await call("GET", path, { cookie: caller.cookie });
        expect(await fingerprint(response), `${caller.name} GET ${path}`).toBe(
          reference,
        );
      }
    }
    // Nothing a refused call asked for happened.
    expect(await auditRows()).toHaveLength(0);
    const target = await db
      .prepare(`SELECT suspended_at FROM users WHERE id = ?`)
      .bind(TARGET)
      .first<{ suspended_at: string | null }>();
    expect(target?.suspended_at).toBeNull();
  });

  it("serves only the sign-in page and its own files to everyone else", async () => {
    const page = await call("GET", "/?kova_auth_code=abc&state=x");
    expect(page.status).toBe(200);
    const html = await page.text();
    expect(html).toContain(`content="${ADMIN_KEY}"`);
    expect(html).not.toContain("console");
    expect((await call("GET", "/assets/boot.js")).status).toBe(200);
    expect((await call("GET", "/assets/shared.js")).status).toBe(200);
    // The page itself only at "/", and none of the console's routes.
    expect((await call("GET", "/index.html")).status).toBe(404);
    for (const path of ROUTES) {
      expect((await call("GET", path)).status, path).toBe(404);
    }
  });

  it("trades a token for the session cookie only for a site admin", async () => {
    const reference = await fingerprint(await call("GET", "/nope"));
    for (const bearer of [
      undefined,
      "not-a-real-token",
      "plain-token",
      "suspended-admin-token",
      "yap-app-token",
    ]) {
      const refused = await call("POST", "/api/session", { bearer });
      expect(await fingerprint(refused), `bearer ${bearer}`).toBe(reference);
    }
    // Another site's page cannot do it for the admin either.
    expect(
      await fingerprint(
        await call("POST", "/api/session", {
          bearer: "admin-token",
          origin: "https://evil.test",
        }),
      ),
    ).toBe(reference);

    const granted = await call("POST", "/api/session", {
      bearer: "admin-token",
    });
    expect(granted.status).toBe(204);
    const cookie = granted.headers.get("set-cookie") ?? "";
    expect(cookie).toContain(`${SESSION_COOKIE}=admin-token`);
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("Secure");
    expect(cookie).toContain("SameSite=Strict");
  });

  it("lets the admin's session in to the console, its files and the API", async () => {
    // The same page at "/" and at every console route (deep links, refresh).
    for (const path of ["/", ...ROUTES]) {
      const page = await call("GET", path, { cookie: "admin-token" });
      expect(page.status, path).toBe(200);
      const html = await page.text();
      expect(html, path).toContain(`content="${ADMIN_KEY}"`);
      expect(page.headers.get("cache-control"), path).toBe("no-store");
    }
    for (const file of [
      "/assets/console.js",
      "/assets/core.js",
      "/assets/console.css",
    ]) {
      expect(
        (await call("GET", file, { cookie: "admin-token" })).status,
        file,
      ).toBe(200);
    }
    expect(
      (await call("GET", "/manifest.json", { cookie: "admin-token" })).status,
    ).toBe(404);
    // The session names the console bundle the boot page loads in place.
    const session = await call("GET", "/api/session", {
      cookie: "admin-token",
    });
    expect(session.headers.get("cache-control")).toBe("no-store");
    expect(await session.json()).toMatchObject({
      user_id: ADMIN,
      console: {
        script: "/assets/console.js",
        styles: ["/assets/console.css"],
      },
    });
    for (const [method, path] of API.filter(([method]) => method === "GET")) {
      if (path === "/api/nope") continue;
      expect(
        (await call(method, path, { cookie: "admin-token" })).status,
        path,
      ).toBe(200);
    }
    // Writes need the same origin, even with the session.
    expect(
      (
        await call("PUT", "/api/settings", {
          cookie: "admin-token",
          origin: "https://evil.test",
          body: { flags: { signups: false } },
        })
      ).status,
    ).toBe(404);
    const signOut = await call("POST", "/api/sign-out", {
      cookie: "admin-token",
    });
    expect(signOut.status).toBe(204);
    expect(signOut.headers.get("set-cookie")).toContain("Max-Age=0");
  });

  it("does not cache a Kova outage as a refusal", async () => {
    const reference = await fingerprint(await call("GET", "/nope"));
    let failures = 0;
    for (const outage of [
      () => new Response(null, { status: 429 }),
      () => new Response(null, { status: 503 }),
      () => {
        throw new TypeError("fetch failed");
      },
    ]) {
      clearGateCache();
      const flaky: typeof fetch = async (input, init) => {
        failures += 1;
        if (failures % 2 === 1) return outage();
        return fakeKova(input, init);
      };
      const ask = () =>
        handleAdminRequest(
          new Request(`${BASE}/api/session`, {
            headers: { cookie: `${SESSION_COOKIE}=admin-token` },
          }),
          adminEnv,
          { fetch: flaky },
        );
      // The outage is still a bare 404, byte for byte...
      expect(await fingerprint(await ask())).toBe(reference);
      // ...but the next request asks Kova again and gets in.
      const next = await ask();
      expect(next.status).toBe(200);
      expect(await next.json()).toMatchObject({ user_id: ADMIN });
    }

    // A definite "no" is cached: Kova is asked once per TTL.
    clearGateCache();
    let asked = 0;
    const counting: typeof fetch = async (input, init) => {
      asked += 1;
      return fakeKova(input, init);
    };
    for (let i = 0; i < 3; i += 1) {
      await handleAdminRequest(
        new Request(`${BASE}/api/users`, {
          headers: { cookie: `${SESSION_COOKIE}=plain-token` },
        }),
        adminEnv,
        { fetch: counting },
      );
    }
    expect(asked).toBe(1);
  });

  it("routes only the known API paths and methods", () => {
    expect(routeApi("GET", "/api/users")).toEqual({ action: "people" });
    expect(routeApi("POST", "/api/users")).toBeNull();
    expect(routeApi("POST", "/api/users/u%201/plan")).toEqual({
      action: "plan",
      userId: "u 1",
    });
    expect(routeApi("GET", "/api/users/u1/plan")).toBeNull();
    expect(routeApi("DELETE", "/api/settings")).toBeNull();
  });
});

// ── Mutations and the audit log ─────────────────────────────────────────────

describe("admin mutations write the audit log", () => {
  const as = (method: string, path: string, body?: unknown) =>
    call(method, path, { cookie: "admin-token", body });

  beforeEach(async () => {
    await run(`DELETE FROM admin_audit_log`);
  });

  it("plan: founder, then Loud with an end, then free", async () => {
    const until = new Date(Date.now() + 30 * 86_400_000).toISOString();
    for (const body of [
      { tier: "founder" },
      { tier: "loud", expires_at: until },
      { tier: "free" },
    ]) {
      expect((await as("POST", `/api/users/${TARGET}/plan`, body)).status).toBe(
        200,
      );
    }
    const rows = await auditRows("plan.set");
    expect(rows).toHaveLength(3);
    const [last, middle, first] = rows;
    expect(first).toMatchObject({
      actor_id: ADMIN,
      target_type: "user",
      target_id: TARGET,
      cf_ray: "test-ray-1",
    });
    expect(JSON.parse(first!.before_json!)).toEqual({ tier: "free" });
    expect(JSON.parse(first!.after_json!)).toMatchObject({
      tier: "founder",
      source: "admin",
    });
    expect(JSON.parse(middle!.after_json!)).toMatchObject({
      tier: "loud",
      expires_at: until,
    });
    expect(JSON.parse(last!.after_json!)).toEqual({ tier: "free" });
    expect(
      await db
        .prepare(`SELECT 1 FROM user_entitlements WHERE user_id = ?`)
        .bind(TARGET)
        .first(),
    ).toBeNull();
    expect(
      await db
        .prepare(
          `SELECT 1 FROM user_badges WHERE user_id = ? AND badge_id = 'founder'`,
        )
        .bind(TARGET)
        .first(),
    ).toBeNull();
  });

  it("refuses a Loud end in the past and logs nothing", async () => {
    const response = await as("POST", `/api/users/${TARGET}/plan`, {
      tier: "loud",
      expires_at: "2020-01-01T00:00:00Z",
    });
    expect(response.status).toBe(400);
    expect(await auditRows()).toHaveLength(0);
  });

  it("badges: award and revoke a manual badge; automatic ones are refused", async () => {
    const badge = (body: unknown) =>
      as("POST", `/api/users/${TARGET}/badges`, body);
    expect(
      (await badge({ badge_id: "bug-hunter", action: "award" })).status,
    ).toBe(200);
    expect(
      (await badge({ badge_id: "bug-hunter", action: "revoke" })).status,
    ).toBe(200);
    expect(
      (await badge({ badge_id: "voice-regular", action: "award" })).status,
    ).toBe(400);
    expect(await auditRows("badge.award")).toHaveLength(1);
    expect(await auditRows("badge.revoke")).toHaveLength(1);
  });

  it("settings: a switch and the announcement", async () => {
    const flags = await as("PUT", "/api/settings", {
      flags: { listenTogether: false },
    });
    expect(await flags.json()).toMatchObject({
      flags: { listenTogether: false },
    });
    expect(
      (
        await as("PUT", "/api/settings", {
          announcement: {
            text: "Maintenance",
            tone: "warning",
            link_url: "javascript:alert(1)",
          },
        })
      ).status,
    ).toBe(400);
    expect(
      (
        await as("PUT", "/api/settings", {
          announcement: { text: "Maintenance", tone: "warning" },
        })
      ).status,
    ).toBe(200);
    const [row] = await auditRows("flags.update");
    expect(JSON.parse(row!.before_json!)).toEqual({ listenTogether: true });
    expect(JSON.parse(row!.after_json!)).toEqual({ listenTogether: false });
    expect(await auditRows("announcement.update")).toHaveLength(1);
    await as("PUT", "/api/settings", {
      flags: { listenTogether: true },
      announcement: null,
    });
  });

  it("suspension is logged, and an admin cannot suspend themselves", async () => {
    expect(
      (
        await as("POST", `/api/users/${TARGET}/suspension`, {
          suspended: true,
          reason: "spam",
        })
      ).status,
    ).toBe(200);
    const [row] = await auditRows("user.suspend");
    expect(JSON.parse(row!.after_json!)).toMatchObject({
      suspended_reason: "spam",
    });
    expect(
      (
        await as("POST", `/api/users/${TARGET}/suspension`, {
          suspended: false,
        })
      ).status,
    ).toBe(200);
    expect(await auditRows("user.unsuspend")).toHaveLength(1);
    expect(
      (await as("POST", `/api/users/${ADMIN}/suspension`, { suspended: true }))
        .status,
    ).toBe(400);
  });

  it("disconnect writes its audit row first, then the outcome", async () => {
    const response = await as("POST", `/api/users/${TARGET}/disconnect`);
    expect(response.status).toBe(200);
    const [row] = await auditRows("user.disconnect");
    expect(JSON.parse(row!.after_json!)).toEqual({ closed_sockets: 0 });
  });
});
