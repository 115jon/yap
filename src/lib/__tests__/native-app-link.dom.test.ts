// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  canOfferNativeApp,
  isNativeAppDevice,
  launchNativeApp,
  NATIVE_APP_SCHEME,
  nativeInviteLink,
  nativeSignInLink,
  setNativeAppLauncherForTests,
} from "@/lib/native-app-link";

describe("native app links", () => {
  it("never uses yap:, which the Tauri app may still own", () => {
    expect(NATIVE_APP_SCHEME).toBe("yap-native");
    expect(nativeSignInLink("ABCD-EFGH").startsWith("yap://")).toBe(false);
  });

  it("names only the user code, formatted", () => {
    expect(nativeSignInLink("abcd-efgh")).toBe(
      "yap-native://signin?code=ABCD-EFGH",
    );
    expect(nativeSignInLink("ABCDEFGH")).toBe(
      "yap-native://signin?code=ABCD-EFGH",
    );
  });

  it("drops a code that is not complete", () => {
    expect(nativeSignInLink("ABCD")).toBe("yap-native://signin");
    expect(nativeSignInLink("")).toBe("yap-native://signin");
  });

  it("tells the native client from the other apps by its label", () => {
    expect(isNativeAppDevice({ label: "Yap" })).toBe(true);
    expect(isNativeAppDevice({ label: " Yap " })).toBe(true);
    // The label before the rename, from a client not updated yet.
    expect(isNativeAppDevice({ label: "Yap Native" })).toBe(true);
    expect(isNativeAppDevice({ label: "  Yap Native " })).toBe(true);
    expect(isNativeAppDevice({ label: "yap" })).toBe(false);
    expect(isNativeAppDevice({ label: "" })).toBe(false);
    expect(isNativeAppDevice({ label: "Yap Desktop" })).toBe(false);
    expect(isNativeAppDevice({ label: "Yap Mobile" })).toBe(false);
    expect(isNativeAppDevice({ label: null })).toBe(false);
    expect(isNativeAppDevice({})).toBe(false);
  });
});

describe("native app links for web pages", () => {
  it("builds invite links only for codes the app takes", () => {
    expect(nativeInviteLink("quiet-pro_7")).toBe(
      "yap-native://invite/quiet-pro_7",
    );
    expect(nativeInviteLink("")).toBeNull();
    expect(nativeInviteLink("a/b")).toBeNull();
    expect(nativeInviteLink("a b")).toBeNull();
    expect(nativeInviteLink("x".repeat(64))).not.toBeNull();
    expect(nativeInviteLink("x".repeat(65))).toBeNull();
  });
});

describe("launching the native app", () => {
  afterEach(() => setNativeAppLauncherForTests(null));

  it("hands over only yap-native links", () => {
    const launch = vi.fn();
    setNativeAppLauncherForTests(launch);
    launchNativeApp("https://yap.example.com/invite/abc");
    launchNativeApp("yap://invite/abc");
    expect(launch).not.toHaveBeenCalled();
    launchNativeApp("yap-native://invite/abc");
    expect(launch).toHaveBeenCalledWith("yap-native://invite/abc");
  });
});

describe("where the app is offered", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    delete (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
  });

  function stubPlatform(platform: string) {
    vi.stubGlobal("navigator", {
      ...navigator,
      userAgentData: { platform },
      platform: "",
      userAgent: "",
    });
  }

  it("offers it in a browser on Windows", () => {
    stubPlatform("Windows");
    expect(canOfferNativeApp()).toBe(true);
  });

  it("never offers it off Windows", () => {
    stubPlatform("macOS");
    expect(canOfferNativeApp()).toBe(false);
    stubPlatform("Android");
    expect(canOfferNativeApp()).toBe(false);
  });

  it("never offers it inside the Tauri apps", () => {
    stubPlatform("Windows");
    (window as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__ = {};
    expect(canOfferNativeApp()).toBe(false);
  });
});
