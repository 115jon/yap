// @vitest-environment jsdom
// @vitest-environment-options { "url": "https://yap.example/chat" }

// The web app on a real (non-localhost) origin, where getApiBaseUrl() is ""
// and only the page's origin and the public API name our own URLs.

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setStoredKovaAuthSessionToken } from "../desktop-auth";
import { getApiBaseUrl, getAuthAssetUrl, isTauri } from "../platform";

beforeEach(() => {
  vi.stubEnv("VITE_API_BASE_URL", "https://api.yap.example");
  setStoredKovaAuthSessionToken("session-token");
});

afterEach(() => {
  vi.unstubAllEnvs();
  window.localStorage.clear();
});

describe("getAuthAssetUrl on the web", () => {
  it("runs on the web origin", () => {
    expect(isTauri()).toBe(false);
    expect(getApiBaseUrl()).toBe("");
    expect(window.location.origin).toBe("https://yap.example");
  });

  it("loads the page's own absolute URLs directly", () => {
    for (const input of [
      "https://yap.example/og-image.png",
      "https://YAP.example./og-image.png",
    ]) {
      expect(getAuthAssetUrl(input), input).not.toContain("/api/proxy-media");
    }
    expect(getAuthAssetUrl("https://yap.example/og-image.png")).toBe(
      "https://yap.example/og-image.png",
    );
  });

  it("loads the public API's absolute URLs directly", () => {
    expect(
      getAuthAssetUrl("https://api.yap.example/api/emojis/assets/e1.png"),
    ).not.toContain("/api/proxy-media");
  });

  it("proxies other hosts, however the URL is written", () => {
    for (const input of [
      "https://cdn.example/a.png",
      "/\t/cdn.example/a.png",
      "//cdn.example/a.png",
      "\\\\cdn.example/a.png",
    ]) {
      expect(getAuthAssetUrl(input), JSON.stringify(input)).toBe(
        "/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fa.png",
      );
    }
  });

  it("reads https:host as a path on an https page, as the browser does", () => {
    // Same scheme as the page: relative, so it loads from us.
    expect(getAuthAssetUrl("https:cdn.example/a.png")).toBe(
      "https:cdn.example/a.png",
    );
    // Another scheme: absolute, so another host, so the proxy.
    expect(getAuthAssetUrl("http:cdn.example/a.png")).toBe(
      "/api/proxy-media?url=http%3A%2F%2Fcdn.example%2Fa.png",
    );
  });
});
