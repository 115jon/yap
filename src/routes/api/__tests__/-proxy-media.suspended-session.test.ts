import { env } from "cloudflare:workers";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
// The request's own headers carry the credentials; no server context here.
vi.mock("@tanstack/react-start/server", () => ({
  getRequestHeader: () => undefined,
}));

import { invalidateSuspensionCache } from "@/lib/suspension";
import { mediaProxyKey } from "@/server/media-proxy/signing";
import { authorizeGenericMediaRequest, handleProxyMedia } from "../proxy-media";

// The sign-in check here is the real one: requireAuth asks Kova (stubbed at
// the network) and then the real suspension check reads users.suspended_at
// (a stub D1). Nothing hands the proxy a ready-made "suspended" answer.

const SECRET = "suspended-session-secret";
const key = mediaProxyKey({ MEDIA_PROXY_SECRET: SECRET })!;
const TARGET = "https://cdn.example.com/og.png";
const PROXY_URL = `https://yap.example/api/proxy-media?${new URLSearchParams({ url: TARGET })}`;

/**
 * The proxy's own suspension check says no, so only the sign-in check
 * (requireAuth, with the real suspension lookup) can refuse these requests.
 */
const neverSuspended = async () => false;

const SIGNED_IN = (userId: string) => ({
  user: { id: userId },
  session: { id: `session-${userId}` },
});

/** Kova says this user is signed in, whatever the session. */
function stubFetch(userId: string) {
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => Response.json(SIGNED_IN(userId))),
  );
}

/** A D1 whose users table holds these suspended ids. */
function suspendedAccounts(...ids: string[]) {
  (env as unknown as Record<string, unknown>).DB = {
    prepare: () => ({
      all: async () => ({ results: ids.map((id) => ({ id })) }),
    }),
  };
  invalidateSuspensionCache();
}

function bearerRequest(token: string): Request {
  return new Request(PROXY_URL, {
    headers: { Authorization: `Bearer ${token}` },
  });
}

beforeEach(() => {
  (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET = SECRET;
});

afterEach(() => {
  delete (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET;
  delete (env as unknown as Record<string, unknown>).DB;
  invalidateSuspensionCache();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("a suspended account's real sign-in check", () => {
  it("makes the generic media authorization come back suspended", async () => {
    stubFetch("user-suspended");
    suspendedAccounts("user-suspended");
    expect(
      await authorizeGenericMediaRequest(
        bearerRequest("suspended-session-a"),
        TARGET,
        key,
        undefined,
        undefined,
        neverSuspended,
      ),
    ).toEqual({ via: "suspended" });
  });

  it("refuses the media request with 403 ACCOUNT_SUSPENDED", async () => {
    stubFetch("user-suspended");
    suspendedAccounts("user-suspended");
    const response = await handleProxyMedia(
      bearerRequest("suspended-session-b"),
      true,
      false,
      { user: async () => null, suspended: neverSuspended },
    );
    expect(response.status).toBe(403);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    await expect(response.json()).resolves.toMatchObject({
      code: "ACCOUNT_SUSPENDED",
    });
  });

  it("lets the same session through while the account is not suspended", async () => {
    stubFetch("user-fine");
    suspendedAccounts("someone-else");
    expect(
      await authorizeGenericMediaRequest(
        bearerRequest("suspended-session-c"),
        TARGET,
        key,
      ),
    ).toEqual({ via: "bearer", userId: "user-fine" });
  });
});
