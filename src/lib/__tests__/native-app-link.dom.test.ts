// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  androidSignInLink,
  appSignInLink,
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

describe("the sign-in link for the app that asked", () => {
  it("brings back the Android app with an intent naming it", () => {
    expect(androidSignInLink("abcd-efgh")).toBe(
      "intent://signin?code=ABCD-EFGH#Intent;scheme=yap;package=dev.yap.app;end",
    );
    expect(androidSignInLink("ABCD")).toBe(
      "intent://signin#Intent;scheme=yap;package=dev.yap.app;end",
    );
  });

  it("picks the link by the platform the app runs on", () => {
    expect(
      appSignInLink({ label: "Yap", platform: "windows" }, "ABCDEFGH"),
    ).toBe("yap-native://signin?code=ABCD-EFGH");
    expect(
      appSignInLink({ label: "Yap", platform: " Android " }, "ABCDEFGH"),
    ).toBe(
      "intent://signin?code=ABCD-EFGH#Intent;scheme=yap;package=dev.yap.app;end",
    );
  });

  it("has no link for iOS, an unknown platform, or another app", () => {
    expect(
      appSignInLink({ label: "Yap", platform: "ios" }, "ABCDEFGH"),
    ).toBeNull();
    expect(
      appSignInLink({ label: "Yap", platform: "linux" }, "ABCDEFGH"),
    ).toBeNull();
    expect(
      appSignInLink({ label: "Yap", platform: null }, "ABCDEFGH"),
    ).toBeNull();
    expect(appSignInLink({ label: "Yap" }, "ABCDEFGH")).toBeNull();
    expect(
      appSignInLink({ label: "Yap Desktop", platform: "windows" }, "ABCDEFGH"),
    ).toBeNull();
    expect(
      appSignInLink({ label: "Yap Mobile", platform: "android" }, "ABCDEFGH"),
    ).toBeNull();
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

  it("hands over only yap-native links and the Android app's intent", () => {
    const launch = vi.fn();
    setNativeAppLauncherForTests(launch);
    launchNativeApp("https://yap.example.com/invite/abc");
    launchNativeApp("yap://invite/abc");
    launchNativeApp("intent://signin#Intent;scheme=yap;package=com.other;end");
    expect(launch).not.toHaveBeenCalled();
    launchNativeApp("yap-native://invite/abc");
    expect(launch).toHaveBeenCalledWith("yap-native://invite/abc");
    const android = androidSignInLink("ABCD-EFGH");
    launchNativeApp(android);
    expect(launch).toHaveBeenLastCalledWith(android);
  });
});

describe("where the app is offered", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
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
});
