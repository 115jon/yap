/**
 * The native Windows client ("Yap", native/; "Yap Native" until it
 * replaced the Tauri desktop app) and the link scheme only it reads,
 * `yap-native://`. The native app owns `yap://` too now, but the Tauri app
 * still has `yap://` on a PC that has not moved over yet, so the web keeps
 * making `yap-native://` links, which the native app reads as its own
 * `yap://` ones. Nothing here makes a `yap://` link.
 *
 * The one hand-off the web makes is after a device sign-in: the native app
 * signs in with a Kova device code and opens /activate in the browser. Once
 * the code is approved, the page opens the app that asked, by the platform
 * it said it runs on (`appSignInLink`): `yap-native://signin?code=XXXX-XXXX`
 * for the Windows app, an Android intent for `yap://signin?code=…` in the
 * Android app (native/mobile, `dev.yap.app`), and nothing elsewhere. Either
 * only brings the app forward (the Windows app also polls that code at
 * once). The link never carries a token or a session; the app gets its
 * session from its own poll, as it would without the link.
 *
 * The web also offers the invite page's "Open in Yap Native" key, an explicit
 * `yap-native://invite/<code>` link the user clicks. It names only the invite
 * code that is already in the web URL. Other links are left to the browser's
 * own "Open Yap?" prompt.
 *
 * The native parser (native/rust/crates/yap-core/src/links.rs) is
 * strict: invite codes of letters, digits, `-` and `_`. A link it would
 * refuse is never built here; the builder returns null instead.
 */

import {
  formatDeviceCode,
  isCompleteDeviceCode,
} from "@/lib/device-code-format";
import { isDesktop, isMobile, isTauri, isWindows } from "@/lib/platform";

/** The native client's own scheme. Never `yap:`, which the Tauri app may still own. */
export const NATIVE_APP_SCHEME = "yap-native";

/**
 * The label the native client sends when it starts a device sign-in
 * (native/rust/crates/yap-core/src/auth.rs `DEVICE_LABEL`). The
 * Tauri app sends "Yap Desktop", the mobile app "Yap Mobile".
 */
export const NATIVE_APP_DEVICE_LABEL = "Yap";

/** The labels the native client has sent: today's, then "Yap Native". */
const NATIVE_APP_DEVICE_LABELS: readonly string[] = [
  NATIVE_APP_DEVICE_LABEL,
  "Yap Native",
];

/**
 * Whether a device request came from the native client. The label is what
 * the device says about itself, so this only decides whether to offer the
 * app's link; a wrong guess opens nothing that matters.
 */
export function isNativeAppDevice(device: { label?: string | null }): boolean {
  const label = device.label?.trim();
  return label !== undefined && NATIVE_APP_DEVICE_LABELS.includes(label);
}

/** `?code=XXXX-XXXX` for a complete code, else nothing. */
function signInQuery(userCode: string): string {
  const code = formatDeviceCode(userCode);
  return isCompleteDeviceCode(code)
    ? `?${new URLSearchParams({ code }).toString()}`
    : "";
}

/**
 * `yap-native://signin?code=XXXX-XXXX` for an approved code, or the bare
 * `yap-native://signin` when the code is not a complete one.
 */
export function nativeSignInLink(userCode: string): string {
  return `${NATIVE_APP_SCHEME}://signin${signInQuery(userCode)}`;
}

/** The Android app's id (native/mobile CMakeLists.txt `YAP_ANDROID_APP_ID`). */
export const ANDROID_APP_PACKAGE = "dev.yap.app";

/**
 * The Android app's `yap://signin?code=XXXX-XXXX` as an intent link naming
 * the app, so Chrome hands it to Yap and no other app.
 */
export function androidSignInLink(userCode: string): string {
  return `intent://signin${signInQuery(userCode)}#Intent;scheme=yap;package=${ANDROID_APP_PACKAGE};end`;
}

/**
 * The link that brings back the app that asked for a device code, by the
 * platform it said it runs on, or null when no app takes one (iOS, an
 * unknown platform, an app that is not the native client).
 */
export function appSignInLink(
  device: { label?: string | null; platform?: string | null },
  userCode: string,
): string | null {
  if (!isNativeAppDevice(device)) return null;
  switch (device.platform?.trim().toLowerCase()) {
    case "windows":
      return nativeSignInLink(userCode);
    case "android":
      return androidSignInLink(userCode);
    default:
      return null;
  }
}

/** The longest invite code the native parser takes (`MAX_CODE_LEN`). */
const MAX_INVITE_CODE_LENGTH = 64;
const INVITE_CODE_PATTERN = /^[A-Za-z0-9_-]+$/;

/** `yap-native://invite/<code>`, or null for a code the app would refuse. */
export function nativeInviteLink(code: string): string | null {
  return code.length > 0 &&
    code.length <= MAX_INVITE_CODE_LENGTH &&
    INVITE_CODE_PATTERN.test(code)
    ? `${NATIVE_APP_SCHEME}://invite/${code}`
    : null;
}

/**
 * Whether this browser could hand a link to the Yap app: a browser on
 * Windows, not the Tauri desktop app (it renders this same UI and owns
 * `yap://`) and not the mobile app. False during server rendering.
 */
export function canOfferNativeApp(): boolean {
  if (typeof window === "undefined") return false;
  if (isTauri() || isDesktop() || isMobile()) return false;
  return isWindows();
}

/* ------------------------------------------------------------------ */
/* Launch                                                               */
/* ------------------------------------------------------------------ */

function assignLocation(url: string): void {
  window.location.assign(url);
}

let launcher: (url: string) => void = assignLocation;

/**
 * Hands a `yap-native://` link (or the Android app's intent link) to the
 * browser, which asks "Open Yap?" (or opens it at once where the user chose
 * to always allow it). The page stays where it is. Anything else is ignored.
 */
export function launchNativeApp(url: string): void {
  const ours =
    url.startsWith(`${NATIVE_APP_SCHEME}://`) ||
    (url.startsWith("intent://") &&
      url.endsWith(`;package=${ANDROID_APP_PACKAGE};end`));
  if (!ours) return;
  launcher(url);
}

/**
 * Tests and dev fixtures only: replace how a link reaches the browser.
 * Null restores `location.assign`.
 */
export function setNativeAppLauncherForTests(
  next: ((url: string) => void) | null,
): void {
  launcher = next ?? assignLocation;
}
