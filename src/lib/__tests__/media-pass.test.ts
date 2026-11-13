// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";
import { setStoredKovaAuthSessionToken } from "../desktop-auth";
import { ensureMediaPass, RENEW_MARGIN_MS } from "../media-pass";

const EXPIRY_KEY = "yap.mediaPass.expiresAt";

function stubPassEndpoint(expiresAtSeconds: number) {
  const fetchMock = vi.fn(async () =>
    Response.json({ expiresAt: expiresAtSeconds }),
  );
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

afterEach(() => {
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

describe("ensureMediaPass", () => {
  it("renews within 20 minutes of the hour-long pass's expiry", async () => {
    expect(RENEW_MARGIN_MS).toBe(20 * 60 * 1000);
    setStoredKovaAuthSessionToken("session-token");
    const next = Math.floor(Date.now() / 1000) + 3600;
    const fetchMock = stubPassEndpoint(next);

    window.localStorage.setItem(EXPIRY_KEY, String(Date.now() + 30 * 60_000));
    await ensureMediaPass();
    expect(fetchMock).not.toHaveBeenCalled();

    window.localStorage.setItem(EXPIRY_KEY, String(Date.now() + 10 * 60_000));
    await ensureMediaPass();
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(window.localStorage.getItem(EXPIRY_KEY)).toBe(String(next * 1000));
  });

  it("asks for nothing without a session", async () => {
    const fetchMock = stubPassEndpoint(0);
    await ensureMediaPass(true);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
