// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { setStoredKovaAuthSessionToken } from "../desktop-auth";
import {
  ensureMediaPass,
  localPassExpiry,
  RENEW_MARGIN_MS,
} from "../media-pass";

const EXPIRY_KEY = "yap.mediaPass.expiresAt";

function stubPassEndpoint(body: Record<string, unknown>) {
  const fetchMock = vi.fn(async () => Response.json(body));
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

describe("ensureMediaPass", () => {
  it("renews within 20 minutes of the hour-long pass's expiry", async () => {
    expect(RENEW_MARGIN_MS).toBe(20 * 60 * 1000);
    setStoredKovaAuthSessionToken("session-token");
    vi.useFakeTimers({ toFake: ["Date"] });
    vi.setSystemTime(1_800_000_000_000);
    const fetchMock = stubPassEndpoint({
      expiresAt: 1_800_000_000 + 3600,
      expiresIn: 3600,
    });

    window.localStorage.setItem(EXPIRY_KEY, String(Date.now() + 30 * 60_000));
    await ensureMediaPass();
    expect(fetchMock).not.toHaveBeenCalled();

    window.localStorage.setItem(EXPIRY_KEY, String(Date.now() + 10 * 60_000));
    await ensureMediaPass();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(window.localStorage.getItem(EXPIRY_KEY)).toBe(
      String(Date.now() + 3600 * 1000),
    );
  });

  it("keeps renewing when this device's clock is far behind the server's", async () => {
    setStoredKovaAuthSessionToken("session-token");
    vi.useFakeTimers({ toFake: ["Date"] });
    const serverNow = 1_800_000_000_000;
    // Two hours behind: the server's absolute expiry would read as 3 h left
    // here, so a client trusting it would not renew for 2 h 40 min.
    const clientNow = serverNow - 2 * 60 * 60_000;
    vi.setSystemTime(clientNow);
    const fetchMock = stubPassEndpoint({
      expiresAt: serverNow / 1000 + 3600,
      expiresIn: 3600,
    });

    await ensureMediaPass(true);
    expect(window.localStorage.getItem(EXPIRY_KEY)).toBe(
      String(clientNow + 3600 * 1000),
    );

    // 45 minutes later on this clock, 15 minutes are left: it renews.
    vi.setSystemTime(clientNow + 45 * 60_000);
    await ensureMediaPass();
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it("stores no expiry when the answer has no TTL, so the next check asks", async () => {
    setStoredKovaAuthSessionToken("session-token");
    window.localStorage.setItem(EXPIRY_KEY, "1");
    const fetchMock = stubPassEndpoint({ expiresAt: 4_000_000_000 });
    await ensureMediaPass(true);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(window.localStorage.getItem(EXPIRY_KEY)).toBeNull();
  });

  it("asks for nothing without a session", async () => {
    const fetchMock = stubPassEndpoint({ expiresIn: 3600 });
    await ensureMediaPass(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});

describe("localPassExpiry", () => {
  it("is local now plus the server's TTL", () => {
    expect(localPassExpiry({ expiresIn: 3600 }, 1_000)).toBe(3_601_000);
  });

  it("is null for a missing or bad TTL", () => {
    expect(localPassExpiry(null)).toBeNull();
    expect(localPassExpiry({})).toBeNull();
    expect(localPassExpiry({ expiresIn: "3600" })).toBeNull();
    expect(localPassExpiry({ expiresIn: -1 })).toBeNull();
    expect(localPassExpiry({ expiresIn: Number.NaN })).toBeNull();
  });
});
