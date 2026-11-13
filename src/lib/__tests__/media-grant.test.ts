// @vitest-environment jsdom

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ apiPost: vi.fn() }));

vi.mock("@/lib/api-client", () => ({ apiPost: mocks.apiPost }));

import { issueMediaGrant, verifyMediaCap } from "../media-capability";
import {
  ensureMediaGrant,
  getMediaGrant,
  resetMediaGrantForTests,
  seedMediaGrantForTests,
  signMediaUrl,
} from "../media-grant";

const SECRET = "test-secret";
const URL_A = "https://yap.test/api/attachments/c1/a1/f.png";

function signIn(token: string) {
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  window.localStorage.setItem("desktop_auth_token", token);
}

function announceToken(token: string | null) {
  window.dispatchEvent(
    new CustomEvent("ralphmeet:desktop-token-change", { detail: token }),
  );
}

beforeEach(() => {
  mocks.apiPost.mockReset();
  signIn("session-a");
});

afterEach(() => {
  vi.useRealTimers();
  window.localStorage.clear();
  delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  resetMediaGrantForTests();
});

describe("media grant", () => {
  it("is fetched with the session and stored without the token", async () => {
    mocks.apiPost.mockResolvedValue(
      issueMediaGrant(SECRET, "user-1", Date.now()),
    );

    const grant = await ensureMediaGrant();

    expect(mocks.apiPost).toHaveBeenCalledWith("/api/media/grant", {});
    expect(grant).toMatchObject({ userId: "user-1" });
    const stored = window.localStorage.getItem("yap:media-grant") ?? "";
    expect(stored).toContain("user-1");
    expect(stored).not.toContain("session-a");
  });

  it("counts its lifetime from the server's clock, not this device's", async () => {
    const serverNow = Date.UTC(2026, 10, 13, 12, 0, 0);
    // This device's clock runs eight hours ahead of the server's.
    vi.useFakeTimers({ now: serverNow + 8 * 3600_000 });
    mocks.apiPost.mockResolvedValue(
      issueMediaGrant(SECRET, "user-1", serverNow),
    );

    await ensureMediaGrant();

    expect(getMediaGrant()).not.toBeNull();
    const signed = signMediaUrl(new URL(URL_A));
    expect(
      verifyMediaCap(
        signed?.searchParams.get("cap") ?? "",
        signed as URL,
        SECRET,
        serverNow,
      ),
    ).toMatchObject({ ok: true });
  });

  it("is refreshed in the background once under two hours remain", async () => {
    const now = Date.now();
    seedMediaGrantForTests(
      { ...issueMediaGrant(SECRET, "user-1", now), issuedAt: now },
      "session-a",
      now - 3 * 3600_000,
    );
    mocks.apiPost.mockReturnValue(new Promise(() => undefined));

    expect(getMediaGrant()).not.toBeNull();

    await vi.waitFor(() => expect(mocks.apiPost).toHaveBeenCalledTimes(1));
  });

  it("waits after a failure before asking again", async () => {
    mocks.apiPost.mockRejectedValue(new Error("503"));

    await ensureMediaGrant();
    await ensureMediaGrant();

    expect(mocks.apiPost).toHaveBeenCalledTimes(1);
  });

  it("is forgotten on sign-out", () => {
    seedMediaGrantForTests(
      issueMediaGrant(SECRET, "user-1", Date.now()),
      "session-a",
    );
    expect(getMediaGrant()).not.toBeNull();

    window.localStorage.removeItem("desktop_auth_token");
    announceToken(null);

    expect(window.localStorage.getItem("yap:media-grant")).toBeNull();
    expect(getMediaGrant()).toBeNull();
  });

  it("is never used for another session", () => {
    seedMediaGrantForTests(
      issueMediaGrant(SECRET, "user-1", Date.now()),
      "session-a",
    );
    mocks.apiPost.mockReturnValue(new Promise(() => undefined));

    window.localStorage.setItem("desktop_auth_token", "session-b");

    expect(getMediaGrant()).toBeNull();
    expect(signMediaUrl(new URL(URL_A))).toBeNull();
  });

  it("keeps a signed URL across a refresh for the same user", () => {
    const now = Date.now();
    seedMediaGrantForTests(
      issueMediaGrant(SECRET, "user-1", now),
      "session-a",
      now,
    );
    const first = signMediaUrl(new URL(URL_A), now)?.toString();

    seedMediaGrantForTests(
      issueMediaGrant(SECRET, "user-1", now + 3 * 3600_000),
      "session-a",
      now,
    );

    expect(signMediaUrl(new URL(URL_A), now)?.toString()).toBe(first);
  });

  it("signs again once a cached URL nears its own expiry", () => {
    const now = Date.now();
    const grant = issueMediaGrant(SECRET, "user-1", now);
    seedMediaGrantForTests(grant, "session-a", now);
    const first = signMediaUrl(new URL(URL_A), now)?.toString();
    mocks.apiPost.mockReturnValue(new Promise(() => undefined));

    const later = grant.expiresAt - 20 * 60_000;
    seedMediaGrantForTests(
      issueMediaGrant(SECRET, "user-1", later),
      "session-a",
      later,
    );

    expect(signMediaUrl(new URL(URL_A), later)?.toString()).not.toBe(first);
  });
});
