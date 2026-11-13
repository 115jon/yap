// /api/sessions on the Effect edge: the real route against an in-memory
// Kova, and against Kova over HTTP for the app-session endpoints.

import { Effect, Option } from "effect";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { createMockD1 } from "@/lib/__tests__/mock-d1";
import { AuthUnavailable } from "@/server/errors";
import {
  KovaAuth,
  type AppSessionRevocation,
  type KovaAppSession,
} from "@/server/kova-auth";
import { sessionRoutes, type SessionRow } from "@/server/routes/sessions";
import { memoryAuth, testServices } from "@/server/testing";

const TOKEN = "Bearer good-token";
const SESSIONS = { [TOKEN]: "user-1" };
// memoryAuth names the caller's session after the user.
const CURRENT = "session-user-1";
const PHONE_UA = "YapMobile/0.6.0";
const SECRET = "raw-session-token-value";

interface Answer {
  sessions: SessionRow[];
  othersListed: boolean;
  revoked?: number | null;
  code?: string;
}

function read(response: Response): Promise<Answer> {
  // SAFETY: the route's own answers, asserted field by field below.
  return response.json() as Promise<Answer>;
}

function call(
  method: "GET" | "DELETE",
  body?: unknown,
  headers: Record<string, string> = {},
) {
  return new Request("https://meet.test/api/sessions", {
    method,
    headers: { authorization: TOKEN, ...headers },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

const LISTED: KovaAppSession[] = [
  {
    id: CURRENT,
    createdAt: "2026-11-01T10:00:00.000Z",
    updatedAt: "2026-11-10T10:00:00.000Z",
    expiresAt: "2099-01-01T00:00:00.000Z",
    userAgent: "YapNative/0.6.0",
  },
  {
    id: "older-laptop",
    createdAt: "2026-11-02T10:00:00.000Z",
    updatedAt: "2026-11-03T10:00:00.000Z",
    expiresAt: "2099-01-01T00:00:00.000Z",
    userAgent:
      "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36",
  },
  {
    id: "phone",
    createdAt: 1_793_000_000_000,
    updatedAt: 1_794_000_000_000,
    expiresAt: "2099-01-01T00:00:00.000Z",
    userAgent: PHONE_UA,
    devicePlatform: "android",
    deviceName: "Pixel 8",
  },
  {
    id: "expired",
    updatedAt: "2026-11-09T10:00:00.000Z",
    expiresAt: "2020-01-01T00:00:00.000Z",
    userAgent: PHONE_UA,
  },
];

beforeEach(() => {
  vi.spyOn(console, "warn").mockImplementation(() => {});
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("GET /api/sessions", () => {
  it("lists the caller's sessions, its own first, with activity", async () => {
    const routes = sessionRoutes(
      testServices({
        auth: memoryAuth(SESSIONS, {
          listAppSessions: () => Effect.succeed(Option.some(LISTED)),
        }),
      }),
    );

    const response = await routes.GET({
      request: call("GET", undefined, { "user-agent": PHONE_UA }),
    });
    const body = await read(response);

    expect(response.status).toBe(200);
    expect(body.othersListed).toBe(true);
    expect(body.sessions.map((s) => s.id)).toEqual([
      CURRENT,
      "phone",
      "older-laptop",
    ]);
    expect(body.sessions[0]).toMatchObject({
      isCurrent: true,
      status: "active",
      activity: { deviceType: "Mobile", browserName: "Yap mobile app" },
    });
    expect(body.sessions[1]).toMatchObject({
      isCurrent: false,
      lastActiveAt: 1_794_000_000_000,
      createdAt: 1_793_000_000_000,
      activity: {
        deviceType: "Mobile",
        isMobile: true,
        browserName: "Yap mobile app on Android",
        deviceName: "Pixel 8",
      },
    });
    expect(body.sessions[2]).toMatchObject({
      isCurrent: false,
      lastActiveAt: Date.parse("2026-11-03T10:00:00.000Z"),
      activity: { deviceType: "Desktop", browserName: "Chrome on Windows" },
    });
  });

  it("answers the caller's own session alone while Kova cannot list", async () => {
    const routes = sessionRoutes(testServices({ auth: memoryAuth(SESSIONS) }));

    const response = await routes.GET({
      request: call("GET", undefined, { "user-agent": PHONE_UA }),
    });
    const body = await read(response);

    expect(response.status).toBe(200);
    expect(body.othersListed).toBe(false);
    expect(body.sessions).toHaveLength(1);
    expect(body.sessions[0]).toMatchObject({
      id: CURRENT,
      isCurrent: true,
      activity: { deviceType: "Mobile", browserName: "Yap mobile app" },
    });
  });

  it("answers 401 without a session", async () => {
    const signedOut = await sessionRoutes(
      testServices({ auth: memoryAuth(SESSIONS) }),
    ).GET({ request: call("GET", undefined, { authorization: "Bearer no" }) });

    expect(signedOut.status).toBe(401);
  });

  it("still answers the caller's own session when the list call fails", async () => {
    const down = await sessionRoutes(
      testServices({
        auth: memoryAuth(SESSIONS, {
          listAppSessions: () =>
            Effect.fail(new AuthUnavailable({ reason: "timeout" })),
        }),
      }),
    ).GET({ request: call("GET") });
    const body = await read(down);

    expect(down.status).toBe(200);
    expect(body.othersListed).toBe(false);
    expect(body.sessions.map((s) => s.id)).toEqual([CURRENT]);
  });

  it("reports no last-active time when Kova's row never moved", async () => {
    const routes = sessionRoutes(
      testServices({
        auth: memoryAuth(SESSIONS, {
          listAppSessions: () =>
            Effect.succeed(
              Option.some([
                {
                  id: "unmoved",
                  createdAt: "2026-11-05T10:00:00.000Z",
                  updatedAt: "2026-11-05T10:00:00.000Z",
                },
                {
                  id: "reported",
                  createdAt: "2026-11-01T10:00:00.000Z",
                  updatedAt: "2026-11-01T10:00:00.000Z",
                  lastActiveAt: "2026-11-10T10:00:00.000Z",
                },
              ]),
            ),
        }),
      }),
    );

    const body = await read(await routes.GET({ request: call("GET") }));

    expect(body.sessions.slice(1)).toMatchObject([
      { id: "reported", lastActiveAt: Date.parse("2026-11-10T10:00:00.000Z") },
      { id: "unmoved", lastActiveAt: null },
    ]);
  });
});

describe("DELETE /api/sessions", () => {
  function revoking(answer: Option.Option<number | null>) {
    const asked: AppSessionRevocation[] = [];
    const db = createMockD1();
    const routes = sessionRoutes(
      testServices({
        auth: memoryAuth(SESSIONS, {
          revokeAppSessions: (_headers, revocation) => {
            asked.push(revocation);
            return Effect.succeed(answer);
          },
        }),
        db,
      }),
    );
    return { asked, db, routes };
  }

  it("signs out one other session and deletes its push tokens", async () => {
    const { asked, db, routes } = revoking(Option.some(1));

    const response = await routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });

    expect(response.status).toBe(200);
    expect(await read(response)).toEqual({ revoked: 1 });
    expect(asked).toEqual([{ sessionId: "phone" }]);
    // Tokens from before they were bound to a session (NULL) go too.
    db.assertCalledWith(
      /DELETE FROM push_tokens\s+WHERE user_id = \? AND \(session_id = \? OR session_id IS NULL\)/,
      ["user-1", "phone"],
    );
  });

  it("signs out all other sessions and keeps only the caller's push tokens", async () => {
    const { asked, db, routes } = revoking(Option.some(3));

    const response = await routes.DELETE({
      request: call("DELETE", { allOthers: true }),
    });

    expect(await read(response)).toEqual({ revoked: 3 });
    expect(asked).toEqual([{ allOthers: true }]);
    db.assertCalledWith(
      /DELETE FROM push_tokens\s+WHERE user_id = \? AND \(session_id != \? OR session_id IS NULL\)/,
      ["user-1", CURRENT],
    );
  });

  it("refuses the caller's own session and an ambiguous or bad body", async () => {
    const { asked, db, routes } = revoking(Option.some(1));

    const own = await routes.DELETE({
      request: call("DELETE", { sessionId: CURRENT }),
    });
    const both = await routes.DELETE({
      request: call("DELETE", { sessionId: "phone", allOthers: true }),
    });
    const neither = await routes.DELETE({ request: call("DELETE", {}) });
    const notString = await routes.DELETE({
      request: call("DELETE", { sessionId: 1 }),
    });

    expect(own.status).toBe(400);
    expect((await read(own)).code).toBe("CURRENT_SESSION");
    expect(both.status).toBe(400);
    expect(neither.status).toBe(400);
    expect(notString.status).toBe(400);
    expect((await read(notString)).code).toBe("INVALID_BODY");
    expect(asked).toEqual([]);
    expect(db.calls).toHaveLength(0);
  });

  it("answers 501 while Kova cannot revoke another session", async () => {
    const { db, routes } = revoking(Option.none());

    const response = await routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });

    expect(response.status).toBe(501);
    expect((await read(response)).code).toBe("SESSION_REVOKE_UNSUPPORTED");
    expect(db.calls).toHaveLength(0);
  });
});

describe("Kova's app-session endpoints over HTTP", () => {
  function kovaAnswering(...answers: Response[]) {
    const calls: Array<{ url: string; init: RequestInit }> = [];
    const layer = KovaAuth.layerWith({
      url: "https://kova.test",
      publishableKey: "pk_test",
      timeout: "200 millis",
      fetch: async (input, init) => {
        calls.push({ url: String(input), init: init ?? {} });
        const url = String(input);
        if (url.endsWith("/api/auth/get-session")) {
          return Response.json({
            user: { id: "user-1" },
            session: { id: CURRENT, token: SECRET },
          });
        }
        return (answers.shift() ?? new Response(null, { status: 404 })).clone();
      },
    });
    return {
      calls,
      routes: sessionRoutes(testServices({ auth: layer, db: createMockD1() })),
    };
  }

  it("lists with the caller's bearer alone and no token in the answer", async () => {
    // A Kova that leaked tokens into its rows must not leak them further.
    const kova = kovaAnswering(
      Response.json({
        sessions: LISTED.map((session) => ({ ...session, token: SECRET })),
      }),
    );

    const response = await kova.routes.GET({
      request: call("GET", undefined, { cookie: "kova.session=abc" }),
    });
    const text = await response.clone().text();
    const body = await read(response);
    const list = kova.calls.find((c) => c.url.endsWith("/sessions"));
    const lookups = kova.calls.filter((c) =>
      c.url.endsWith("/api/auth/get-session"),
    );

    expect(response.status).toBe(200);
    expect(text).not.toContain(SECRET);
    // The caller is identified by the same credential the list goes out with.
    expect(
      lookups.map((c) => new Headers(c.init.headers).get("cookie")),
    ).toEqual([null]);
    expect(body.othersListed).toBe(true);
    expect(body.sessions).toHaveLength(3);
    expect(list?.url).toBe("https://kova.test/api/pub/apps/pk_test/sessions");
    expect(Object.fromEntries(new Headers(list?.init.headers))).toEqual({
      authorization: TOKEN,
      "x-publishable-key": "pk_test",
    });
  });

  it("reads a 404 as not supported, and a 401 or 403 as a 502, never a 401", async () => {
    const missing = kovaAnswering(
      new Response("404 Not Found", { status: 404 }),
    );
    const listed = await missing.routes.GET({ request: call("GET") });
    expect((await read(listed)).othersListed).toBe(false);

    for (const status of [401, 403]) {
      const refused = kovaAnswering(
        Response.json({ error: "invalid_session" }, { status }),
      );
      const revoked = await refused.routes.DELETE({
        request: call("DELETE", { allOthers: true }),
      });

      expect(revoked.status).toBe(502);
      expect((await read(revoked)).code).toBe("KOVA_REFUSED");
    }
  });

  it("answers 503 for a Kova outage or a non-JSON 2xx on revoke", async () => {
    const outage = kovaAnswering(new Response(null, { status: 503 }));
    const page = kovaAnswering(new Response("<html>", { status: 200 }));

    const down = await outage.routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });
    const proxied = await page.routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });

    expect(down.status).toBe(503);
    expect(proxied.status).toBe(503);
  });

  it("lists nothing more from a malformed list body", async () => {
    const kova = kovaAnswering(Response.json({ rows: [] }));

    const response = await kova.routes.GET({ request: call("GET") });
    const body = await read(response);

    expect(response.status).toBe(200);
    expect(body.othersListed).toBe(false);
    expect(body.sessions).toHaveLength(1);
  });

  it("takes any 2xx revoke as done, with or without a count", async () => {
    const kova = kovaAnswering(new Response(null, { status: 204 }));

    const response = await kova.routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });

    expect(response.status).toBe(200);
    expect(await read(response)).toEqual({ revoked: null });
  });

  it("posts the revocation to Kova", async () => {
    const kova = kovaAnswering(Response.json({ revoked: 1 }));

    const response = await kova.routes.DELETE({
      request: call("DELETE", { sessionId: "phone" }),
    });
    const revoke = kova.calls.find((c) => c.url.endsWith("/sessions/revoke"));

    expect(await read(response)).toEqual({ revoked: 1 });
    expect(revoke?.init.method).toBe("POST");
    expect(JSON.parse(String(revoke?.init.body))).toEqual({
      sessionId: "phone",
    });
  });
});
