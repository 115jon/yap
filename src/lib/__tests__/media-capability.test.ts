import { describe, expect, it } from "vitest";

import {
  deriveMediaGrantKey,
  isCapProtectedMediaPath,
  isMediaCap,
  issueMediaGrant,
  MEDIA_GRANT_REFRESH_MARGIN_MS,
  MEDIA_GRANT_STEP_MS,
  MEDIA_GRANT_TTL_MS,
  mediaCapSubject,
  mediaGrantExpiry,
  signMediaCap,
  verifyMediaCap,
  withMediaCap,
} from "../media-capability";

const SECRET = "test-secret";
const NOW = Date.UTC(2026, 10, 13, 10, 17, 0);
const ATTACHMENT = "https://yap.test/api/attachments/c1/a1/photo%20one.png";

function signed(path: string = ATTACHMENT, userId = "user-1", now = NOW) {
  const grant = issueMediaGrant(SECRET, userId, now);
  return { grant, url: withMediaCap(grant, new URL(path)) };
}

function capOf(url: URL): string {
  const cap = url.searchParams.get("cap");
  if (!cap) throw new Error("no cap");
  return cap;
}

describe("media grants", () => {
  it("expire a few hours out, on the hour, so a refresh keeps the same URLs", () => {
    const expiresAt = mediaGrantExpiry(NOW);
    expect(expiresAt % MEDIA_GRANT_STEP_MS).toBe(0);
    expect(expiresAt - NOW).toBeGreaterThanOrEqual(MEDIA_GRANT_TTL_MS);
    expect(expiresAt - NOW).toBeLessThan(
      MEDIA_GRANT_TTL_MS + MEDIA_GRANT_STEP_MS,
    );
    expect(mediaGrantExpiry(NOW + 60_000)).toBe(expiresAt);
    expect(MEDIA_GRANT_REFRESH_MARGIN_MS).toBeLessThan(MEDIA_GRANT_TTL_MS);
  });

  it("derive a different key per user and per expiry", () => {
    const a = deriveMediaGrantKey(SECRET, "user-1", 1000);
    expect(deriveMediaGrantKey(SECRET, "user-2", 1000)).not.toEqual(a);
    expect(deriveMediaGrantKey(SECRET, "user-1", 2000)).not.toEqual(a);
    expect(deriveMediaGrantKey("other-secret", "user-1", 1000)).not.toEqual(a);
  });

  it("refuse to issue without a secret", () => {
    expect(() => issueMediaGrant("", "user-1", NOW)).toThrow();
  });
});

describe("media capabilities", () => {
  it("verify for the user, path and expiry they were signed for", () => {
    const { grant, url } = signed();
    expect(verifyMediaCap(capOf(url), url, SECRET, NOW + 1000)).toEqual({
      ok: true,
      userId: "user-1",
      expiresAt: grant.expiresAt,
    });
  });

  it("never carry the grant key or a session token", () => {
    const { grant, url } = signed(`${ATTACHMENT}?token=session-token`);
    expect(url.searchParams.has("token")).toBe(false);
    expect(url.toString()).not.toContain(grant.key);
    expect(url.toString()).not.toContain("session-token");
    expect(isMediaCap(capOf(url))).toBe(true);
  });

  it("are refused once expired", () => {
    const { grant, url } = signed();
    expect(verifyMediaCap(capOf(url), url, SECRET, grant.expiresAt)).toEqual({
      ok: false,
      reason: "expired",
    });
  });

  it("are refused for another path", () => {
    const { url } = signed();
    const other = new URL(
      `https://yap.test/api/attachments/c1/a2/other.png?cap=${encodeURIComponent(capOf(url))}`,
    );
    expect(verifyMediaCap(capOf(url), other, SECRET, NOW)).toEqual({
      ok: false,
      reason: "signature",
    });
  });

  it("are refused when the signed query changes", () => {
    const { url } = signed(
      "https://yap.test/api/listen-together/stream?videoId=v1&roomSlug=r1",
    );
    const swapped = new URL(url.toString().replace("videoId=v1", "videoId=v2"));
    expect(verifyMediaCap(capOf(swapped), swapped, SECRET, NOW)).toEqual({
      ok: false,
      reason: "signature",
    });
    const appended = new URL(`${url.toString()}&extra=1`);
    expect(verifyMediaCap(capOf(appended), appended, SECRET, NOW).ok).toBe(
      false,
    );
  });

  it("are refused under another secret", () => {
    const { url } = signed();
    expect(verifyMediaCap(capOf(url), url, "other-secret", NOW).ok).toBe(false);
  });

  it("are refused when the user, expiry or signature is tampered with", () => {
    const { url } = signed();
    const [version, user, expires, signature] = capOf(url).split(".");
    const otherUser = btoa("user-2").replace(/=+$/, "");
    const flipped = (signature[0] === "A" ? "B" : "A") + signature.slice(1);
    for (const cap of [
      [version, otherUser, expires, signature].join("."),
      [
        version,
        user,
        String(Number(expires) + MEDIA_GRANT_STEP_MS),
        signature,
      ].join("."),
      [version, user, expires, flipped].join("."),
      [version, user, expires].join("."),
      ["m2", user, expires, signature].join("."),
      "garbage",
      "",
    ]) {
      expect(verifyMediaCap(cap, url, SECRET, NOW).ok, cap).toBe(false);
    }
  });

  it("are refused when they claim an expiry no grant would have", () => {
    const farGrant = issueMediaGrant(SECRET, "user-1", NOW + 24 * 3600_000);
    const url = withMediaCap(farGrant, new URL(ATTACHMENT));
    expect(verifyMediaCap(capOf(url), url, SECRET, NOW)).toEqual({
      ok: false,
      reason: "invalid",
    });
  });

  it("sign the path and query without cap or token, as the URL encodes them", () => {
    expect(
      mediaCapSubject(
        new URL("https://x.test/api/a/b%20c.png?cap=1&v=2&token=t&w=a+b"),
      ),
    ).toBe("/api/a/b%20c.png?v=2&w=a+b");
    expect(mediaCapSubject(new URL("https://x.test/api/a?cap=1"))).toBe(
      "/api/a",
    );
  });

  it("are the same for the same grant and URL", () => {
    const grant = issueMediaGrant(SECRET, "user-1", NOW);
    expect(signMediaCap(grant, new URL(ATTACHMENT))).toBe(
      signMediaCap(grant, new URL(ATTACHMENT)),
    );
  });

  it("cover the private media routes and nothing else", () => {
    for (const path of [
      "/api/attachments/c/a/f.png",
      "/api/soundboard/uploads/s1",
      "/api/camera-backgrounds/b.png",
      "/api/listen-together/stream",
      "/api/voice-status-media/s/a/f.gif",
      "/api/emojis/assets/e1",
    ]) {
      expect(isCapProtectedMediaPath(path), path).toBe(true);
    }
    expect(isCapProtectedMediaPath("/api/avatars/u1")).toBe(false);
    expect(isCapProtectedMediaPath("/api/users/me")).toBe(false);
  });

  it("are told apart from soundboard capabilities", () => {
    expect(isMediaCap("sbm1.payload.sig")).toBe(false);
    expect(isMediaCap(null)).toBe(false);
  });
});
