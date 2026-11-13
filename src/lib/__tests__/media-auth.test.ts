import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  env: {} as Record<string, string | undefined>,
  requireCredentials: vi.fn(),
  isSuspended: vi.fn(),
  logs: [] as unknown[][],
}));

vi.mock("@/lib/api-helpers", () => ({
  getEnv: () => mocks.env,
  requireCredentials: mocks.requireCredentials,
  suspendedResponse: () =>
    Response.json({ error: "suspended" }, { status: 403 }),
  unauthorizedResponse: () =>
    Response.json({ error: "Unauthorized" }, { status: 401 }),
}));

vi.mock("@/lib/suspension", () => ({ isSuspended: mocks.isSuspended }));

vi.mock("@/lib/console-logger", () => ({
  clog: () => {
    const record =
      (level: string) =>
      (...args: unknown[]) =>
        mocks.logs.push([level, ...args]);
    return {
      info: record("info"),
      warn: record("warn"),
      error: record("error"),
      debug: record("debug"),
    };
  },
}));

import {
  LEGACY_QUERY_TOKEN_SUNSET,
  legacyQueryTokenClient,
  requireMediaAuth,
  STALE_WEB_TAB_SUNSET,
} from "../media-auth";
import { issueMediaGrant, withMediaCap } from "../media-capability";

const ATTACHMENT = "https://yap.test/api/attachments/c1/a1/f.png";
const SECRET = "ticket-secret";

beforeEach(() => {
  mocks.env = { REALTIME_TICKET_SECRET: SECRET };
  mocks.requireCredentials.mockReset();
  mocks.requireCredentials.mockResolvedValue({ userId: "from-session" });
  mocks.isSuspended.mockReset();
  mocks.isSuspended.mockResolvedValue(false);
  mocks.logs = [];
});

function credentialsPassed() {
  return mocks.requireCredentials.mock.calls[0]?.[1];
}

describe("requireMediaAuth", () => {
  it("authenticates with the request's own headers", async () => {
    const result = await requireMediaAuth(
      new Request(ATTACHMENT, { headers: { authorization: "Bearer abc" } }),
    );
    expect(result).toEqual({ userId: "from-session" });
    expect(credentialsPassed()).toEqual({
      authorization: "Bearer abc",
      cookie: null,
    });
  });

  it("never treats a browser's ?token= as a session", async () => {
    mocks.requireCredentials.mockResolvedValue(
      Response.json({ error: "Unauthorized" }, { status: 401 }),
    );
    const result = await requireMediaAuth(
      new Request(`${ATTACHMENT}?token=session-token`),
    );
    expect(result).toBeInstanceOf(Response);
    expect((result as Response).status).toBe(401);
    expect(credentialsPassed()).toEqual({ authorization: null, cookie: null });
  });

  it("ignores ?token= when the request has its own header", async () => {
    await requireMediaAuth(
      new Request(`${ATTACHMENT}?token=other`, {
        headers: {
          authorization: "Bearer mine",
          origin: "http://tauri.localhost",
        },
      }),
    );
    expect(credentialsPassed()).toEqual({
      authorization: "Bearer mine",
      cookie: null,
    });
  });

  it("accepts ?token= from the Tauri app's webview until the sunset, logging no token", async () => {
    const result = await requireMediaAuth(
      new Request(`${ATTACHMENT}?token=legacy-token`, {
        headers: { referer: "http://tauri.localhost/chat" },
      }),
    );
    expect(result).toEqual({ userId: "from-session" });
    expect(credentialsPassed()).toEqual({
      authorization: "Bearer legacy-token",
      cookie: null,
    });
    expect(JSON.stringify(mocks.logs)).toContain("Deprecated ?token=");
    expect(JSON.stringify(mocks.logs)).not.toContain("legacy-token");
    expect(JSON.stringify(mocks.logs)).not.toContain("f.png");
  });

  it("keeps the legacy rule narrow: Tauri and stale web tabs, reads only, before their sunsets", () => {
    const tauri = { referer: "http://tauri.localhost/" };
    const staleTab = {
      "sec-fetch-site": "same-origin",
      "sec-fetch-dest": "image",
    };
    const before = STALE_WEB_TAB_SUNSET - 1;
    const client = (init: RequestInit, now = before) =>
      legacyQueryTokenClient(new Request(ATTACHMENT, init), now);

    expect(client({ headers: tauri })).toBe("tauri");
    expect(client({ headers: { origin: "https://tauri.localhost" } })).toBe(
      "tauri",
    );
    expect(client({ headers: tauri }, LEGACY_QUERY_TOKEN_SUNSET)).toBeNull();
    expect(client({ headers: staleTab })).toBe("web-stale-tab");
    expect(client({ headers: staleTab }, STALE_WEB_TAB_SUNSET)).toBeNull();
    // A pasted link: opened from elsewhere, or a top-level navigation.
    expect(
      client({
        headers: { "sec-fetch-site": "none", "sec-fetch-dest": "document" },
      }),
    ).toBeNull();
    expect(
      client({
        headers: {
          "sec-fetch-site": "same-origin",
          "sec-fetch-dest": "document",
        },
      }),
    ).toBeNull();
    expect(
      client({
        headers: { "sec-fetch-site": "cross-site", "sec-fetch-dest": "image" },
      }),
    ).toBeNull();
    expect(client({ headers: { referer: "https://evil.test/" } })).toBeNull();
    // A Tauri request without Origin or Referer is not recognised.
    expect(client({})).toBeNull();
    expect(
      client({ method: "POST", headers: { origin: "http://tauri.localhost" } }),
    ).toBeNull();
    expect(
      client({ headers: { ...tauri, authorization: "Bearer other" } }),
    ).toBeNull();
  });

  it("falls back to the request's own session when a cap is refused", async () => {
    const grant = issueMediaGrant("another-secret", "user-7", Date.now());
    const url = withMediaCap(grant, new URL(ATTACHMENT));
    const result = await requireMediaAuth(
      new Request(url, { headers: { authorization: "Bearer mine" } }),
    );
    expect(result).toEqual({ userId: "from-session" });
    expect(credentialsPassed()).toEqual({
      authorization: "Bearer mine",
      cookie: null,
    });
  });

  it("logs a refused ?token= without the token", async () => {
    await requireMediaAuth(new Request(`${ATTACHMENT}?token=leaked-token`));
    const logged = JSON.stringify(mocks.logs);
    expect(logged).toContain("Refused ?token=");
    expect(logged).not.toContain("leaked-token");
  });

  it("accepts a valid media capability without asking Kova", async () => {
    const grant = issueMediaGrant(SECRET, "user-7", Date.now());
    const url = withMediaCap(grant, new URL(ATTACHMENT));
    const result = await requireMediaAuth(new Request(url));
    expect(result).toEqual({ userId: "user-7" });
    expect(mocks.requireCredentials).not.toHaveBeenCalled();
  });

  it("prefers its own secret when one is set", async () => {
    mocks.env = {
      MEDIA_CAP_SECRET: "media-secret",
      REALTIME_TICKET_SECRET: SECRET,
    };
    const url = withMediaCap(
      issueMediaGrant(SECRET, "user-7", Date.now()),
      new URL(ATTACHMENT),
    );
    const result = await requireMediaAuth(new Request(url));
    expect((result as Response).status).toBe(401);
  });

  it("refuses a capability for another object", async () => {
    const grant = issueMediaGrant(SECRET, "user-7", Date.now());
    const cap = withMediaCap(grant, new URL(ATTACHMENT)).searchParams.get(
      "cap",
    );
    const result = await requireMediaAuth(
      new Request(
        `https://yap.test/api/attachments/c1/a2/g.png?cap=${encodeURIComponent(cap ?? "")}`,
      ),
    );
    expect((result as Response).status).toBe(401);
    expect(mocks.requireCredentials).not.toHaveBeenCalled();
  });

  it("refuses a capability for a suspended account", async () => {
    mocks.isSuspended.mockResolvedValue(true);
    const url = withMediaCap(
      issueMediaGrant(SECRET, "user-7", Date.now()),
      new URL(ATTACHMENT),
    );
    const result = await requireMediaAuth(new Request(url));
    expect((result as Response).status).toBe(403);
  });

  it("refuses every capability when no secret is configured", async () => {
    const url = withMediaCap(
      issueMediaGrant(SECRET, "user-7", Date.now()),
      new URL(ATTACHMENT),
    );
    mocks.env = {};
    const result = await requireMediaAuth(new Request(url));
    expect((result as Response).status).toBe(401);
  });
});
