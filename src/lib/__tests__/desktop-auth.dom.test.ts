// @vitest-environment jsdom

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/kova-auth-config", () => ({
  KOVA_AUTH_PUBLISHABLE_KEY: "pk_live_new",
}));

import {
  clearDesktopAuthSession,
  clearStoredKovaAuthSessionToken,
  consumeAuthLogoutIntent,
  getDesktopAuthHandoffToken,
  getStoredKovaAuthSessionToken,
  handleKovaSessionTokenChange,
  setStoredKovaAuthSessionToken,
  subscribeDesktopTokenChanges,
} from "../desktop-auth";

const CURRENT = "kova-auth:pk_live_new:session-token";
const OLD = "kova-auth:pk_dev_old:session-token";
const LEGACY_CURRENT = "ralph-auth:pk_live_new:session-token";
const LEGACY_OLD = "ralph-auth:pk_dev_old:session-token";

beforeEach(() => {
  localStorage.clear();
  sessionStorage.clear();
});

describe("stored Kova session token", () => {
  it("prefers the current publishable key's token", () => {
    localStorage.setItem(CURRENT, "tok_current");
    localStorage.setItem(OLD, "tok_old");
    expect(getStoredKovaAuthSessionToken()).toBe("tok_current");
    expect(localStorage.getItem(OLD)).toBe("tok_old");
  });

  it("adopts a token left under another publishable key and announces it", () => {
    localStorage.setItem(OLD, "tok_old");
    const listener = vi.fn();
    const unsubscribe = subscribeDesktopTokenChanges(listener);

    expect(getStoredKovaAuthSessionToken()).toBe("tok_old");

    expect(localStorage.getItem(CURRENT)).toBe("tok_old");
    expect(localStorage.getItem(OLD)).toBeNull();
    expect(listener).toHaveBeenCalledWith("tok_old");
    unsubscribe();
  });

  it("adopts ralph-auth keys, the current pk's first", () => {
    localStorage.setItem(LEGACY_OLD, "tok_legacy_old");
    localStorage.setItem(LEGACY_CURRENT, "tok_legacy_current");
    expect(getStoredKovaAuthSessionToken()).toBe("tok_legacy_current");
    expect(localStorage.getItem(LEGACY_OLD)).toBeNull();
    expect(localStorage.getItem(LEGACY_CURRENT)).toBeNull();
  });

  it("ignores keys that aren't session tokens", () => {
    localStorage.setItem("kova-auth:pk_dev_old:pending-state", "x");
    localStorage.setItem("session_token_backup", "x");
    expect(getStoredKovaAuthSessionToken()).toBeNull();
    expect(localStorage.getItem(CURRENT)).toBeNull();
  });

  it("reaches the handoff token through adoption", () => {
    localStorage.setItem(OLD, "tok_old");
    expect(getDesktopAuthHandoffToken()).toBe("tok_old");
  });

  it("clears every publishable key's token on sign-out, so none is re-adopted", () => {
    localStorage.setItem(CURRENT, "tok_current");
    localStorage.setItem(OLD, "tok_old");
    localStorage.setItem(LEGACY_OLD, "tok_legacy");

    clearDesktopAuthSession();

    expect(localStorage.length).toBe(0);
    expect(getDesktopAuthHandoffToken()).toBeNull();
  });

  it("drops other keys' tokens when a new one is stored", () => {
    localStorage.setItem(OLD, "tok_old");
    setStoredKovaAuthSessionToken("tok_new");
    expect(localStorage.getItem(CURRENT)).toBe("tok_new");
    expect(localStorage.getItem(OLD)).toBeNull();
    clearStoredKovaAuthSessionToken();
    expect(getStoredKovaAuthSessionToken()).toBeNull();
  });
});

describe("handleKovaSessionTokenChange", () => {
  it("stores an SDK token under the current publishable key", () => {
    handleKovaSessionTokenChange("tok_sdk", { seenSessionToken: false });
    expect(localStorage.getItem(CURRENT)).toBe("tok_sdk");
  });

  it("clears only after a token was seen (a sign-out), never on a bare null", () => {
    localStorage.setItem(CURRENT, "tok");
    const state = { seenSessionToken: false };

    handleKovaSessionTokenChange(null, state);
    expect(localStorage.getItem(CURRENT)).toBe("tok");
    expect(consumeAuthLogoutIntent()).toBe(false);

    handleKovaSessionTokenChange("tok", state);
    handleKovaSessionTokenChange(null, state);
    expect(localStorage.getItem(CURRENT)).toBeNull();
    expect(consumeAuthLogoutIntent()).toBe(true);
  });
});

describe("token change announcements", () => {
  it("announces a token the SDK already wrote to its pk key", () => {
    // KovaAuthProvider writes kova-auth:<pk>:session-token, then reports it.
    localStorage.setItem(CURRENT, "tok_sdk");
    const listener = vi.fn();
    const unsubscribe = subscribeDesktopTokenChanges(listener);

    handleKovaSessionTokenChange("tok_sdk", { seenSessionToken: false });

    expect(listener).toHaveBeenCalledWith("tok_sdk");
    unsubscribe();
  });

  it("announces a stored token even when the slot already holds it", () => {
    localStorage.setItem(CURRENT, "tok_same");
    const listener = vi.fn();
    const unsubscribe = subscribeDesktopTokenChanges(listener);

    setStoredKovaAuthSessionToken("tok_same");

    expect(listener).toHaveBeenCalledWith("tok_same");
    expect(localStorage.getItem(CURRENT)).toBe("tok_same");
    unsubscribe();
  });
});
