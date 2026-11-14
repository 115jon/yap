// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  getApiBaseUrl,
  getAuthAssetUrl,
  getMediaUrl,
  getPublicApiUrl,
  getWsBaseUrl,
} from "../platform";
import { KOVA_AUTH_PUBLISHABLE_KEY } from "../kova-auth-config";
import { issueMediaGrant, verifyMediaCap } from "../media-capability";
import {
  resetMediaGrantForTests,
  seedMediaGrantForTests,
} from "../media-grant";
import {
  buildProxyMediaPath,
  unwrapEmbedMediaUrls,
  unwrapProxyMediaUrl,
} from "../proxy-media-url";

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
  resetMediaGrantForTests();
});

const SECRET = "test-secret";
/** Where the signed-in session's token lives (lib/desktop-auth). */
const SESSION_KEY = `kova-auth:${KOVA_AUTH_PUBLISHABLE_KEY}:session-token`;

/** A signed-in session with a media grant for it. */
function signInWithGrant(token = "session-token") {
  window.localStorage.setItem(SESSION_KEY, token);
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
    window.localStorage.setItem(SESSION_KEY, "session-token");
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => undefined)),
    );

    const url = new URL(
      getMediaUrl("/api/attachments/c1/a1/f.png?token=stale-token"),
      window.location.origin,
    );
    expect(url.searchParams.has("token")).toBe(false);
    expect(url.searchParams.has("cap")).toBe(false);
  });

  it("does not use a grant issued for another session", () => {
    signInWithGrant("first-session");
    window.localStorage.setItem(SESSION_KEY, "second-session");
    vi.stubGlobal(
      "fetch",
      vi.fn(() => new Promise<Response>(() => undefined)),
    );

    const url = new URL(
      getMediaUrl("/api/attachments/c1/a1/f.png"),
      window.location.origin,
    );
    expect(url.searchParams.has("cap")).toBe(false);
  });

  it("does not append a session token to capability media URLs", () => {
    window.localStorage.setItem(SESSION_KEY, "session-token");

    const url = new URL(
      getMediaUrl(
        "/api/soundboard/uploads/sound-1?cap=capability&room_slug=room-1&playback_id=playback-1",
      ),
      window.location.origin,
    );

    expect(url.searchParams.get("cap")).toBe("capability");
    expect(url.searchParams.has("token")).toBe(false);
  });

  it("removes a stale session token from capability media URLs", () => {
    window.localStorage.setItem(SESSION_KEY, "session-token");

    const url = new URL(
      getMediaUrl(
        "/api/soundboard/uploads/sound-1?cap=capability&token=stale-token",
      ),
      window.location.origin,
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

  it("leaves other external asset urls unchanged without a session", () => {
    window.localStorage.removeItem(SESSION_KEY);
    expect(getAuthAssetUrl("https://example.com/image.png")).toBe(
      "https://example.com/image.png",
    );
  });

  it("routes external media through the proxy for a signed-in session", () => {
    window.localStorage.setItem(SESSION_KEY, "session-token");

    const url = new URL(
      getAuthAssetUrl("https://tracker.example/pixel.png?id=1"),
      "https://app.invalid",
    );
    expect(url.pathname).toBe("/api/proxy-media");
    expect(url.searchParams.get("url")).toBe(
      "https://tracker.example/pixel.png?id=1",
    );
    // The media pass cookie authorizes it: no session token in the URL.
    expect(url.searchParams.has("token")).toBe(false);
    window.localStorage.removeItem(SESSION_KEY);
  });

  it("never puts the session on another host's URL", () => {
    signInWithGrant();
    for (const input of [
      "//evil.example/api/attachments/x.png",
      "/\\evil.example/api/attachments/x.png",
      " //evil.example/api/attachments/x.png",
      "https://evil.example/api/attachments/x.png",
      // Characters the URL parser drops before it reads the host.
      "/\t/evil.example/api/attachments/x.png",
      "/\n/evil.example/api/attachments/x.png",
      "\r\n//evil.example/api/attachments/x.png",
      "\u0000https://evil.example/api/attachments/x.png",
      "ht\ttps://evil.example/api/attachments/x.png",
      "https:\\\\evil.example/api/attachments/x.png",
    ]) {
      const out = getAuthAssetUrl(input);
      expect(out, JSON.stringify(input)).not.toContain("session-token");
      // Nor a media capability: another host never gets one.
      expect(out, JSON.stringify(input)).not.toContain("cap=");
      const url = new URL(out, "https://app.invalid");
      expect(url.pathname, JSON.stringify(input)).toBe("/api/proxy-media");
      // The host the browser would have loaded, with the page's scheme
      // for a protocol-relative URL.
      const target = new URL(url.searchParams.get("url")!);
      expect(target.host, JSON.stringify(input)).toBe("evil.example");
      expect(target.pathname).toBe("/api/attachments/x.png");
    }
    window.localStorage.removeItem(SESSION_KEY);
  });

  it("reads a scheme-relative URL as the page does", () => {
    signInWithGrant();
    // On an http page this names another host; it must not load from it.
    const out = new URL(
      getAuthAssetUrl("https:evil.example/x.png"),
      "https://app.invalid",
    );
    expect(out.pathname).toBe("/api/proxy-media");
    expect(new URL(out.searchParams.get("url")!).host).toBe("evil.example");
    window.localStorage.removeItem(SESSION_KEY);
  });

  it("loads absolute URLs of this page's origin and our zones directly", () => {
    signInWithGrant();
    const origin = window.location.origin;
    expect(getAuthAssetUrl(`${origin}/og-image.png`)).toBe(
      `${origin}/og-image.png`,
    );
    for (const input of [
      "https://yap.115jon.com/og-image.png",
      "https://YAP.115jon.com./og-image.png",
      "https://cdn.115jon.com/f/a.png",
    ]) {
      expect(getAuthAssetUrl(input), input).not.toContain("/api/proxy-media");
    }
    window.localStorage.removeItem(SESSION_KEY);
  });

  it("loads the public API's own absolute URLs directly", () => {
    signInWithGrant();
    const api = getPublicApiUrl();
    const out = getAuthAssetUrl(`${api}/api/emojis/assets/e1.png`);
    expect(out.startsWith(`${api}/api/emojis/assets/e1.png`)).toBe(true);
    expect(out).not.toContain("/api/proxy-media");
    window.localStorage.removeItem(SESSION_KEY);
  });

  it("leaves data and blob URLs alone", () => {
    expect(getAuthAssetUrl("data:image/png;base64,AA")).toBe(
      "data:image/png;base64,AA",
    );
    expect(getAuthAssetUrl("blob:http://localhost/1")).toBe(
      "blob:http://localhost/1",
    );
  });

  it("loads nothing for a scheme that could reach a host outside the proxy", () => {
    for (const value of [
      "file://attacker.example/x.png",
      "file:///c:/x.png",
      "smb://attacker.example/share/x.png",
      "ftp://attacker.example/x.png",
      "javascript:alert(1)",
      "gopher://attacker.example/x",
    ]) {
      expect(getAuthAssetUrl(value), value).toBe("");
    }
  });

  it("loads an unwrapped original with the server's signature", () => {
    window.localStorage.removeItem(SESSION_KEY);
    const signed =
      "https://yap.example/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fog.png&sig=abc123";
    expect(unwrapProxyMediaUrl(signed)).toBe("https://cdn.example/og.png");
    expect(getAuthAssetUrl("https://cdn.example/og.png")).toBe(
      "/api/proxy-media?url=https%3A%2F%2Fcdn.example%2Fog.png&sig=abc123",
    );
  });

  it("loads a proxy URL built for another host of ours from this API", () => {
    expect(
      getAuthAssetUrl(
        "https://yap-alias.example/api/proxy-media?url=https%3A%2F%2Fa.example%2Fb.png&sig=s",
      ),
    ).toBe("/api/proxy-media?url=https%3A%2F%2Fa.example%2Fb.png&sig=s");
  });
});

describe("platform base urls", () => {
  it("uses this page's origin for the API and sockets", () => {
    expect(getApiBaseUrl()).toBe("");
    expect(getWsBaseUrl()).toBe(
      `${window.location.protocol === "https:" ? "wss:" : "ws:"}//${window.location.host}`,
    );
  });
});

describe("proxy media urls", () => {
  it("never nests a proxy URL in another", () => {
    const path =
      "/api/proxy-media?url=https%3A%2F%2Fpbs.twimg.com%2Fa.jpg&sourceUrl=https%3A%2F%2Fx.com%2Fu%2Fstatus%2F1";
    expect(
      buildProxyMediaPath(
        `https://yap.example${path}`,
        "https://x.com/u/status/1",
      ),
    ).toBe(path);
  });

  it("unwraps an embed's media to the originals", () => {
    const wrap = (url: string) =>
      `https://yap.example/api/proxy-media?${new URLSearchParams({ url, sig: "s" })}`;
    const embed = unwrapEmbedMediaUrls({
      id: "e",
      url: "https://news.example/a",
      type: "link",
      thumbnail: { url: wrap("https://cdn.example/t.jpg") },
      media: [{ type: "image", url: wrap("https://cdn.example/1.jpg") }],
      author: { name: "n", iconURL: wrap("https://cdn.example/i.png") },
      fields: [],
    });
    expect(embed.thumbnail?.url).toBe("https://cdn.example/t.jpg");
    expect(embed.media?.[0].url).toBe("https://cdn.example/1.jpg");
    expect(embed.author?.iconURL).toBe("https://cdn.example/i.png");
    expect(embed.url).toBe("https://news.example/a");
  });
});
