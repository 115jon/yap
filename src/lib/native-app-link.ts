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
 * The web also offers the app's own links, never its own URLs: the invite
 * page's "Open in Yap Native" key, and, with the "Open links in Yap Native"
 * setting on, Yap message and invite links clicked in the web app (see
 * src/hooks/useNativeAppLinkHandler.ts). Those links name only ids that are
 * already in the web URL.
 *
 * The native parser (desktop-native/rust/crates/yap-core/src/links.rs) is
 * strict: one form per link, ids of letters, digits, `-` and `_`, and only
 * the query parameter each form names. A link it would refuse is never built
 * here; the builders return null instead.
 */

import {
  formatDeviceCode,
  isCompleteDeviceCode,
} from "@/lib/device-code-format";
import {
  getPublicWebUrl,
  isDesktop,
  isMobile,
  isTauri,
  isWindows,
} from "@/lib/platform";

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
/** The longest server, channel or message id it takes (`MAX_ID_LEN`). */
const MAX_ID_LENGTH = 128;
const ID_PATTERN = /^[A-Za-z0-9_-]+$/;

function isLinkToken(text: string, max: number): boolean {
  return text.length > 0 && text.length <= max && ID_PATTERN.test(text);
}

/** A server, channel, DM or message id the native parser takes. */
export function isNativeLinkId(text: string): boolean {
  return isLinkToken(text, MAX_ID_LENGTH);
}

/** `yap-native://invite/<code>`, or null for a code the app would refuse. */
export function nativeInviteLink(code: string): string | null {
  return isLinkToken(code, MAX_INVITE_CODE_LENGTH)
    ? `${NATIVE_APP_SCHEME}://invite/${code}`
    : null;
}

/**
 * `yap-native://chat/<server>[/<channel>][?message=<id>]`. `server` is
 * `@me` for Home and DMs. A message needs a channel. Null for anything the
 * app would refuse.
 */
export function nativeChatLink(target: {
  server: string;
  channel?: string | null;
  message?: string | null;
}): string | null {
  const { server, channel, message } = target;
  if (server !== "@me" && !isNativeLinkId(server)) return null;
  const base = `${NATIVE_APP_SCHEME}://chat/${server}`;
  if (!channel) return message ? null : base;
  if (!isNativeLinkId(channel)) return null;
  if (!message) return `${base}/${channel}`;
  if (!isNativeLinkId(message)) return null;
  return `${base}/${channel}?${new URLSearchParams({ message }).toString()}`;
}

function decodeSegment(segment: string): string | null {
  try {
    return decodeURIComponent(segment);
  } catch {
    return null;
  }
}

/**
 * The native link for a Yap web URL: `/invite/<code>` or
 * `/chat/<server>[/<channel>][?message=<id>]` on one of `origins` (by
 * default this page's origin and the public web origin). Null for any other
 * URL, including Yap pages the app has no link for. Other query parameters
 * and the fragment are dropped; the app has no use for them.
 */
export function nativeLinkForWebUrl(
  href: string,
  origins: readonly string[] = yapWebOrigins(),
): string | null {
  let url: URL;
  try {
    url = new URL(href);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  if (!origins.includes(url.origin)) return null;
  const decoded = url.pathname
    .replace(/\/+$/, "")
    .split("/")
    .slice(1)
    .map(decodeSegment);
  if (decoded.some((segment) => segment === null)) return null;
  const [kind, first, second, ...rest] = decoded as string[];
  if (kind === "invite") {
    return first && second === undefined ? nativeInviteLink(first) : null;
  }
  if (kind !== "chat" || !first || rest.length > 0) return null;
  return nativeChatLink({
    server: first,
    channel: second,
    message: url.searchParams.get("message"),
  });
}

/** This page's origin and the public web origin. */
function yapWebOrigins(): string[] {
  const origins = new Set<string>();
  if (typeof window !== "undefined") origins.add(window.location.origin);
  try {
    origins.add(new URL(getPublicWebUrl()).origin);
  } catch {
    // A malformed VITE_PUBLIC_WEB_URL only narrows what counts as ours.
  }
  return [...origins];
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
/* "Open links in Yap Native": per browser, off by default              */
/* ------------------------------------------------------------------ */

export const OPEN_LINKS_IN_NATIVE_KEY = "yap-native:open-links:v1";

const preferenceListeners = new Set<() => void>();

/** Whether this browser offers Yap links to Yap Native. Off by default. */
export function readOpenLinksInNative(): boolean {
  if (typeof window === "undefined") return false;
  try {
    return window.localStorage.getItem(OPEN_LINKS_IN_NATIVE_KEY) === "1";
  } catch {
    return false;
  }
}

export function writeOpenLinksInNative(enabled: boolean): void {
  try {
    if (enabled) window.localStorage.setItem(OPEN_LINKS_IN_NATIVE_KEY, "1");
    else window.localStorage.removeItem(OPEN_LINKS_IN_NATIVE_KEY);
  } catch {
    // Storage is blocked; nothing to remember.
  }
  for (const listener of preferenceListeners) listener();
}

/** For useSyncExternalStore: this tab's writes and other tabs' changes. */
export function subscribeOpenLinksInNative(listener: () => void): () => void {
  preferenceListeners.add(listener);
  const onStorage = (event: StorageEvent) => {
    if (event.key === OPEN_LINKS_IN_NATIVE_KEY || event.key === null) {
      listener();
    }
  };
  window.addEventListener("storage", onStorage);
  return () => {
    preferenceListeners.delete(listener);
    window.removeEventListener("storage", onStorage);
  };
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
