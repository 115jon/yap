import { describe, expect, it } from "vitest";

import {
  isNativeAppDevice,
  NATIVE_APP_SCHEME,
  nativeSignInLink,
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
