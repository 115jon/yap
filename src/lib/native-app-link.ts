/**
 * The native Windows client ("Yap Native", desktop-native/) and its own link
 * scheme, `yap-native://`. It is a separate app from the Tauri desktop app,
 * which owns `yap://`; nothing here touches `yap://`.
 *
 * The one hand-off the web makes is after a device sign-in: the native app
 * signs in with a Kova device code and opens /activate in the browser. Once
 * the code is approved, the page opens `yap-native://signin?code=XXXX-XXXX`,
 * which only brings the app forward and lets it poll that code at once. The
 * link never carries a token or a session; the app gets its session from
 * its own poll, as it would without the link.
 *
 * The web also offers the invite page's "Open in Yap Native" key, an explicit
 * `yap-native://invite/<code>` link the user clicks. It names only the invite
 * code that is already in the web URL. Other links are left to the browser's
 * own "Open Yap Native?" prompt.
 *
 * The native parser (desktop-native/rust/crates/yap-core/src/links.rs) is
 * strict: invite codes of letters, digits, `-` and `_`. A link it would
 * refuse is never built here; the builder returns null instead.
 */

import {
  formatDeviceCode,
  isCompleteDeviceCode,
} from "@/lib/device-code-format";
import { isDesktop, isMobile, isTauri, isWindows } from "@/lib/platform";

/** The native client's scheme. Never the Tauri app's `yap:`. */
export const NATIVE_APP_SCHEME = "yap-native";

/**
 * The label the native client sends when it starts a device sign-in
 * (desktop-native/rust/crates/yap-core/src/auth.rs `start`). The Tauri app
 * sends "Yap Desktop", the mobile app "Yap Mobile".
 */
export const NATIVE_APP_DEVICE_LABEL = "Yap Native";

/**
 * Whether a device request came from the native client. The label is what
 * the device says about itself, so this only decides whether to offer the
 * app's link; a wrong guess opens nothing that matters.
 */
export function isNativeAppDevice(device: { label?: string | null }): boolean {
  return device.label?.trim() === NATIVE_APP_DEVICE_LABEL;
}

/**
 * `yap-native://signin?code=XXXX-XXXX` for an approved code, or the bare
 * `yap-native://signin` when the code is not a complete one.
 */
export function nativeSignInLink(userCode: string): string {
  const code = formatDeviceCode(userCode);
  const base = `${NATIVE_APP_SCHEME}://signin`;
  return isCompleteDeviceCode(code)
    ? `${base}?${new URLSearchParams({ code }).toString()}`
    : base;
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
 * Whether this browser could hand a link to Yap Native: a browser on
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
 * Hands a `yap-native://` link to the browser, which asks "Open Yap
 * Native?" (or opens it at once where the user chose to always allow it).
 * The page stays where it is. Anything but our scheme is ignored.
 */
export function launchNativeApp(url: string): void {
  if (!url.startsWith(`${NATIVE_APP_SCHEME}://`)) return;
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
