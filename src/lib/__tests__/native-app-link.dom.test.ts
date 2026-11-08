// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import {
  canOfferNativeApp,
  isNativeAppDevice,
  launchNativeApp,
  NATIVE_APP_SCHEME,
  nativeChatLink,
  nativeInviteLink,
  nativeLinkForWebUrl,
  nativeSignInLink,
  OPEN_LINKS_IN_NATIVE_KEY,
  readOpenLinksInNative,
  setNativeAppLauncherForTests,
  subscribeOpenLinksInNative,
  writeOpenLinksInNative,
} from "@/lib/native-app-link";

describe("native app links", () => {
  it("never uses the desktop app's yap: scheme", () => {
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
    expect(isNativeAppDevice({ label: "Yap Native" })).toBe(true);
    expect(isNativeAppDevice({ label: "  Yap Native " })).toBe(true);
    expect(isNativeAppDevice({ label: "Yap Desktop" })).toBe(false);
    expect(isNativeAppDevice({ label: "Yap Mobile" })).toBe(false);
    expect(isNativeAppDevice({ label: null })).toBe(false);
    expect(isNativeAppDevice({})).toBe(false);
  });
});

describe("native app links for web pages", () => {
  const origins = ["https://yap.example.com"];

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

  it("builds chat links in the app's forms", () => {
    expect(nativeChatLink({ server: "@me" })).toBe("yap-native://chat/@me");
    expect(nativeChatLink({ server: "@me", channel: "dm1" })).toBe(
      "yap-native://chat/@me/dm1",
    );
    expect(nativeChatLink({ server: "s1" })).toBe("yap-native://chat/s1");
    expect(nativeChatLink({ server: "s1", channel: "c1", message: "m1" })).toBe(
      "yap-native://chat/s1/c1?message=m1",
    );
  });

  it("refuses chat links the app would refuse", () => {
    expect(nativeChatLink({ server: "s1", message: "m1" })).toBeNull();
    expect(nativeChatLink({ server: "@you" })).toBeNull();
    expect(nativeChatLink({ server: "s1", channel: "c.1" })).toBeNull();
    expect(
      nativeChatLink({ server: "s1", channel: "c1", message: "m?1" }),
    ).toBeNull();
    expect(nativeChatLink({ server: "x".repeat(128) })).not.toBeNull();
    expect(nativeChatLink({ server: "x".repeat(129) })).toBeNull();
  });

  it("maps Yap web URLs to app links", () => {
    const map = (href: string) => nativeLinkForWebUrl(href, origins);
    expect(map("https://yap.example.com/invite/quiet-pro")).toBe(
      "yap-native://invite/quiet-pro",
    );
    expect(map("https://yap.example.com/chat/@me")).toBe(
      "yap-native://chat/@me",
    );
    expect(map("https://yap.example.com/chat/%40me/dm-1?message=m-2")).toBe(
      "yap-native://chat/@me/dm-1?message=m-2",
    );
    expect(map("https://yap.example.com/chat/s1/c1/")).toBe(
      "yap-native://chat/s1/c1",
    );
    // Other parameters and the fragment are dropped, never passed on.
    expect(
      map("https://yap.example.com/chat/s1/c1?message=m1&token=secret#x"),
    ).toBe("yap-native://chat/s1/c1?message=m1");
  });

  it("leaves every other URL alone", () => {
    const map = (href: string) => nativeLinkForWebUrl(href, origins);
    expect(map("https://evil.example.com/invite/quiet-pro")).toBeNull();
    expect(map("https://yap.example.com/")).toBeNull();
    expect(map("https://yap.example.com/chat")).toBeNull();
    expect(map("https://yap.example.com/invite")).toBeNull();
    expect(map("https://yap.example.com/invite/a/b")).toBeNull();
    expect(map("https://yap.example.com/chat/s1/c1/extra")).toBeNull();
    expect(map("https://yap.example.com/chat/s1?message=m1")).toBeNull();
    expect(map("https://yap.example.com/share/abc")).toBeNull();
    expect(map("https://yap.example.com/chat/s%2F1")).toBeNull();
    expect(map("https://yap.example.com/chat/%E0%A4%A")).toBeNull();
    expect(map("javascript:alert(1)")).toBeNull();
    expect(map("not a url")).toBeNull();
  });

  it("counts this page's origin as Yap by default", () => {
    expect(nativeLinkForWebUrl(`${window.location.origin}/invite/abc`)).toBe(
      "yap-native://invite/abc",
    );
  });
});

describe("open links in Yap Native setting", () => {
  afterEach(() => {
    window.localStorage.clear();
  });

  it("is off by default and remembers a change", () => {
    expect(readOpenLinksInNative()).toBe(false);
    const listener = vi.fn();
    const unsubscribe = subscribeOpenLinksInNative(listener);
    writeOpenLinksInNative(true);
    expect(readOpenLinksInNative()).toBe(true);
    expect(window.localStorage.getItem(OPEN_LINKS_IN_NATIVE_KEY)).toBe("1");
    writeOpenLinksInNative(false);
    expect(readOpenLinksInNative()).toBe(false);
    expect(listener).toHaveBeenCalledTimes(2);
    unsubscribe();
    writeOpenLinksInNative(true);
    expect(listener).toHaveBeenCalledTimes(2);
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
