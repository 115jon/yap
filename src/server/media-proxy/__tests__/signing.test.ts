import { describe, expect, it } from "vitest";
import {
  issueMediaPass,
  MEDIA_PASS_COOKIE,
  MEDIA_PASS_TTL_SECONDS,
  mediaPassCookie,
  mediaProxyKey,
  readMediaPassCookie,
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
  it("verifies the URL it was made for", () => {
    const sig = signMediaUrl(target, key);
    expect(sig).toMatch(/^[A-Za-z0-9_-]{22}$/);
    expect(verifyMediaUrlSignature(target, sig, key)).toBe(true);
  });

  it("is stable per URL (cacheable)", () => {
    expect(signMediaUrl(target, key)).toBe(signMediaUrl(target, key));
  });

  it("refuses another URL, another key, and malformed signatures", () => {
    const sig = signMediaUrl(target, key);
    expect(verifyMediaUrlSignature(`${target}&x=1`, sig, key)).toBe(false);
    expect(verifyMediaUrlSignature("http://169.254.169.254/", sig, key)).toBe(
      false,
    );
    const otherKey = mediaProxyKey({ MEDIA_PROXY_SECRET: "other" })!;
    expect(verifyMediaUrlSignature(target, sig, otherKey)).toBe(false);
    expect(verifyMediaUrlSignature(target, null, key)).toBe(false);
    expect(verifyMediaUrlSignature(target, "", key)).toBe(false);
    expect(verifyMediaUrlSignature(target, sig.slice(0, 10), key)).toBe(false);
    expect(verifyMediaUrlSignature(target, `${sig}AA`, key)).toBe(false);
    expect(verifyMediaUrlSignature(target, "!!!!", key)).toBe(false);
    const flipped = `${sig[0] === "A" ? "B" : "A"}${sig.slice(1)}`;
    expect(verifyMediaUrlSignature(target, flipped, key)).toBe(false);
  });
});

describe("expiring URL signatures", () => {
  const now = Date.UTC(2026, 10, 13, 12);
  const exp = now / 1000 + 3600;

  it("verify until `exp`, and only with it", () => {
    const sig = signMediaUrl(target, key, exp);
    expect(sig).not.toBe(signMediaUrl(target, key));
    expect(verifyMediaUrlSignature(target, sig, key, String(exp), now)).toBe(
      true,
    );
    expect(
      verifyMediaUrlSignature(target, sig, key, String(exp), exp * 1000),
    ).toBe(false);
    expect(verifyMediaUrlSignature(target, sig, key, null, now)).toBe(false);
    expect(
      verifyMediaUrlSignature(target, sig, key, String(exp + 1), now),
    ).toBe(false);
    // A stable signature does not pass as an expiring one.
    expect(
      verifyMediaUrlSignature(
        target,
        signMediaUrl(target, key),
        key,
        String(exp),
        now,
      ),
    ).toBe(false);
    for (const bad of ["", "-1", "1e9", "12.5", "x"]) {
      expect(verifyMediaUrlSignature(target, sig, key, bad, now)).toBe(false);
    }
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
