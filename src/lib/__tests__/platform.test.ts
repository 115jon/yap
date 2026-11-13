// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  getApiBaseUrl,
  getAuthAssetUrl,
  getMediaUrl,
  getPublicApiUrl,
  getWsBaseUrl,
  isTauri,
  isDesktop,
  isMobile,
} from "../platform";
import { issueMediaGrant, verifyMediaCap } from "../media-capability";
import {
  resetMediaGrantForTests,
  seedMediaGrantForTests,
} from "../media-grant";

afterEach(() => {
  vi.unstubAllGlobals();
  if (typeof window !== "undefined") delete window.__TAURI_INTERNALS__;
  window.localStorage.clear();
  resetMediaGrantForTests();
});

const SECRET = "test-secret";

/** A signed-in session with a media grant for it. */
function signInWithGrant(token = "session-token") {
  window.localStorage.setItem("desktop_auth_token", token);
  Object.defineProperty(window, "__TAURI_INTERNALS__", {
    configurable: true,
    value: {},
  });
  seedMediaGrantForTests(issueMediaGrant(SECRET, "user-1", Date.now()), token);
}

describe("platform asset urls", () => {
  it("signs private media with a capability, never the session token", () => {
    signInWithGrant();

    for (const path of [
      "/api/attachments/c1/a1/f.png",
      "/api/emojis/assets/e1",
      "/api/voice-status-media/s1/a1/clip.gif",
      "/api/camera-backgrounds/beach.png",
    ]) {
      const url = new URL(getAuthAssetUrl(path), window.location.origin);
      expect(url.searchParams.has("token"), path).toBe(false);
      expect(url.toString(), path).not.toContain("session-token");
      const verification = verifyMediaCap(
        url.searchParams.get("cap") ?? "",
        url,
        SECRET,
        Date.now(),
      );
      expect(verification, path).toMatchObject({ ok: true, userId: "user-1" });
    }
  });

  it("signs a stale media capability again instead of passing it through", () => {
    signInWithGrant();

    const url = new URL(
      getAuthAssetUrl("/api/attachments/c1/a1/f.png?cap=m1.b2xk.1.stale"),
      window.location.origin,
    );
    expect(url.searchParams.getAll("cap")).toHaveLength(1);
    expect(
      verifyMediaCap(
        url.searchParams.get("cap") ?? "",
        url,
        SECRET,
        Date.now(),
      ),
    ).toMatchObject({ ok: true });
  });

  it("adds nothing to private media while there is no grant", () => {
    window.localStorage.setItem("desktop_auth_token", "session-token");
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => undefined)),
    );

    const url = new URL(
      getMediaUrl("/api/attachments/c1/a1/f.png?token=stale-token"),
    );
    expect(url.searchParams.has("token")).toBe(false);
    expect(url.searchParams.has("cap")).toBe(false);
  });

  it("does not use a grant issued for another session", () => {
    signInWithGrant("first-session");
    window.localStorage.setItem("desktop_auth_token", "second-session");
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => undefined)),
    );

    const url = new URL(getMediaUrl("/api/attachments/c1/a1/f.png"));
    expect(url.searchParams.has("cap")).toBe(false);
  });

  it("does not append a session token to capability media URLs", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    window.localStorage.setItem("desktop_auth_token", "session-token");

    const url = new URL(
      getMediaUrl(
        "/api/soundboard/uploads/sound-1?cap=capability&room_slug=room-1&playback_id=playback-1",
      ),
    );

    expect(url.searchParams.get("cap")).toBe("capability");
    expect(url.searchParams.has("token")).toBe(false);
  });

  it("removes a stale session token from capability media URLs", () => {
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });
    window.localStorage.setItem("desktop_auth_token", "session-token");

    const url = new URL(
      getMediaUrl(
        "/api/soundboard/uploads/sound-1?cap=capability&token=stale-token",
      ),
    );

    expect(url.searchParams.get("cap")).toBe("capability");
    expect(url.searchParams.has("token")).toBe(false);
  });

  it("proxies Google user-content avatar urls through proxy-media", () => {
    expect(
      getAuthAssetUrl(
        "https://lh3.googleusercontent.com/a/example-avatar=s96-c",
      ),
    ).toBe(
      "/api/proxy-media?url=https%3A%2F%2Flh3.googleusercontent.com%2Fa%2Fexample-avatar%3Ds96-c",
    );
  });

  it("leaves other external asset urls unchanged", () => {
    expect(getAuthAssetUrl("https://example.com/image.png")).toBe(
      "https://example.com/image.png",
    );
  });

  it("uses the public API origin for the mobile Tauri runtime", () => {
    vi.stubGlobal("__IS_MOBILE__", true);
    Object.defineProperty(globalThis, "__IS_MOBILE__", {
      configurable: true,
      value: true,
    });
    Object.defineProperty(window, "__TAURI_INTERNALS__", {
      configurable: true,
      value: {},
    });

    expect(isMobile()).toBe(true);
    expect(isTauri()).toBe(true);
    expect(getApiBaseUrl()).toBe(getPublicApiUrl());
    expect(getWsBaseUrl()).toBe(
      getPublicApiUrl()
        .replace(/^https:/, "wss:")
        .replace(/^http:/, "ws:"),
    );
  });

  it("keeps desktop-only native APIs behind the desktop build flag", () => {
    vi.stubGlobal("__IS_DESKTOP__", true);
    Object.defineProperty(globalThis, "__IS_DESKTOP__", {
      configurable: true,
      value: true,
    });
    expect(isDesktop()).toBe(true);
    expect(isMobile()).toBe(false);
  });
});
