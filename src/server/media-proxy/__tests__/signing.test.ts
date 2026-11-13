import { describe, expect, it } from "vitest";
import {
  issueMediaPass,
  MEDIA_PASS_COOKIE,
  MEDIA_PASS_TTL_SECONDS,
  mediaPassCookie,
  mediaProxyKey,
  readMediaPassCookie,
  SERVER_URL_SIGNATURE_ROUND_SECONDS,
  SERVER_URL_SIGNATURE_TTL_SECONDS,
  signatureExpiry,
  signMediaUrl,
  verifyMediaPass,
  verifyMediaUrlSignature,
} from "../signing";

const key = mediaProxyKey({ MEDIA_PROXY_SECRET: "test-secret" })!;
const target = "https://images.example.com/a.png?w=200";

describe("media proxy key", () => {
  it("is off without a secret", () => {
    expect(mediaProxyKey({})).toBeNull();
  });

  it("derives from the realtime secret when no dedicated one is set", () => {
    const derived = mediaProxyKey({ REALTIME_TICKET_SECRET: "rt" });
    expect(derived).not.toBeNull();
    // Domain-separated: not the raw secret, and not the dedicated key.
    expect(derived!.toString()).not.toContain("rt");
    expect(derived!.equals(key)).toBe(false);
    expect(
      mediaProxyKey({
        MEDIA_PROXY_SECRET: "test-secret",
        REALTIME_TICKET_SECRET: "rt",
      })!.equals(key),
    ).toBe(true);
  });
});

describe("URL signatures", () => {
  const now = Date.UTC(2026, 10, 13, 12);
  const exp = now / 1000 + 3600;
  const verify = (
    url: string,
    sig: string | null,
    withKey = key,
    expires: string | null = String(exp),
    at = now,
  ) => verifyMediaUrlSignature(url, sig, withKey, expires, at);

  it("verifies the URL it was made for, until `exp`", () => {
    const sig = signMediaUrl(target, key, exp);
    expect(sig).toMatch(/^[A-Za-z0-9_-]{22}$/);
    expect(verify(target, sig)).toBe(true);
    expect(verify(target, sig, key, String(exp), exp * 1000 - 1)).toBe(true);
    expect(verify(target, sig, key, String(exp), exp * 1000)).toBe(false);
  });

  it("is the same for the same URL and expiry (cacheable)", () => {
    expect(signMediaUrl(target, key, exp)).toBe(signMediaUrl(target, key, exp));
  });

  it("is refused without `exp`: no signature is permanent", () => {
    const sig = signMediaUrl(target, key, exp);
    expect(verify(target, sig, key, null)).toBe(false);
  });

  it("refuses another URL, key or expiry, and malformed signatures", () => {
    const sig = signMediaUrl(target, key, exp);
    expect(verify(`${target}&x=1`, sig)).toBe(false);
    expect(verify("http://169.254.169.254/", sig)).toBe(false);
    const otherKey = mediaProxyKey({ MEDIA_PROXY_SECRET: "other" })!;
    expect(verify(target, sig, otherKey)).toBe(false);
    expect(verify(target, sig, key, String(exp + 1))).toBe(false);
    expect(verify(target, null)).toBe(false);
    expect(verify(target, "")).toBe(false);
    expect(verify(target, sig.slice(0, 10))).toBe(false);
    expect(verify(target, `${sig}AA`)).toBe(false);
    expect(verify(target, "!!!!")).toBe(false);
    const flipped = `${sig[0] === "A" ? "B" : "A"}${sig.slice(1)}`;
    expect(verify(target, flipped)).toBe(false);
    for (const bad of ["", "-1", "1e9", "12.5", "x"]) {
      expect(verify(target, sig, key, bad)).toBe(false);
    }
  });
});

describe("signatureExpiry", () => {
  it("rounds up to the boundary, at least the TTL away", () => {
    const day = 86_400;
    const now = Date.UTC(2026, 10, 13, 12);
    const exp = signatureExpiry(7 * day, day, now);
    expect(exp % day).toBe(0);
    expect(exp - now / 1000).toBeGreaterThanOrEqual(7 * day);
    expect(exp - now / 1000).toBeLessThan(8 * day);
    // Same all day long: the URL stays cacheable.
    expect(signatureExpiry(7 * day, day, now + 3_600_000)).toBe(exp);
    expect(
      signatureExpiry(
        SERVER_URL_SIGNATURE_TTL_SECONDS,
        SERVER_URL_SIGNATURE_ROUND_SECONDS,
        now,
      ),
    ).toBe(exp);
  });
});

describe("media pass", () => {
  const now = Date.UTC(2026, 10, 13, 12);

  it("lasts an hour by default", () => {
    expect(MEDIA_PASS_TTL_SECONDS).toBe(3600);
    const pass = issueMediaPass(key, "user-1", now);
    expect(pass.expiresAt).toBe(now / 1000 + 3600);
  });

  it("is valid until it expires and names its user", () => {
    const pass = issueMediaPass(key, "user-1", now, 60);
    expect(pass.expiresAt).toBe(now / 1000 + 60);
    expect(verifyMediaPass(pass.value, key, now)).toEqual({
      userId: "user-1",
    });
    expect(verifyMediaPass(pass.value, key, now + 59_000)).toEqual({
      userId: "user-1",
    });
    expect(verifyMediaPass(pass.value, key, now + 60_000)).toBeNull();
  });

  it("refuses a tampered, foreign or malformed pass", () => {
    const pass = issueMediaPass(key, "user-1", now, 60);
    const [version, expiry, user, nonce, sig] = pass.value.split(".");
    const extended = [
      version,
      String(Number(expiry) + 3600),
      user,
      nonce,
      sig,
    ].join(".");
    expect(verifyMediaPass(extended, key, now)).toBeNull();
    const otherUser = [
      version,
      expiry,
      Buffer.from("user-2").toString("base64url"),
      nonce,
      sig,
    ].join(".");
    expect(verifyMediaPass(otherUser, key, now)).toBeNull();
    const otherKey = mediaProxyKey({ MEDIA_PROXY_SECRET: "other" })!;
    expect(verifyMediaPass(pass.value, otherKey, now)).toBeNull();
    expect(verifyMediaPass("", key, now)).toBeNull();
    expect(verifyMediaPass(null, key, now)).toBeNull();
    expect(verifyMediaPass("mp1.x.y.z", key, now)).toBeNull();
    expect(verifyMediaPass("mp2.1.2.3.4", key, now)).toBeNull();
  });

  it("is read from the Cookie header", () => {
    expect(
      readMediaPassCookie(`a=1; ${MEDIA_PASS_COOKIE}=mp1.2.3.4; b=2`),
    ).toBe("mp1.2.3.4");
    expect(readMediaPassCookie("a=1")).toBeNull();
    expect(readMediaPassCookie(null)).toBeNull();
  });

  it("is an HttpOnly cookie scoped to the proxy", () => {
    const cookie = mediaPassCookie("v", 86400);
    expect(cookie).toContain(`${MEDIA_PASS_COOKIE}=v`);
    expect(cookie).toContain("Path=/api/proxy-media");
    expect(cookie).toContain("HttpOnly");
    expect(cookie).toContain("Secure");
    expect(cookie).toContain("Max-Age=86400");
    expect(mediaPassCookie("", 0)).toContain("Max-Age=0");
  });

  it("is SameSite=Lax, and None only for the desktop webview", () => {
    expect(mediaPassCookie("v", 60)).toContain("SameSite=Lax");
    expect(mediaPassCookie("v", 60)).not.toContain("SameSite=None");
    expect(mediaPassCookie("v", 60, { crossSite: true })).toContain(
      "SameSite=None",
    );
  });
});
