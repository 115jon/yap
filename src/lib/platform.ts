// ============================================================================
// Platform & URL helpers. The web app is served by the Worker it talks to, so
// API and WebSocket URLs are same-origin ("/api/...").
// ============================================================================
import { isCapProtectedMediaPath, isMediaCap } from "@/lib/media-capability";
import { signMediaUrl } from "@/lib/media-grant";
import { getStoredKovaAuthSessionToken } from "./desktop-auth";
import { cleanUrlInput, isOwnZoneHostname } from "@/lib/media-url-class";
import { signedMediaPathFor } from "@/lib/signed-media-paths";
import {
  DEFAULT_ABUSE_EMAIL,
  DEFAULT_PUBLIC_WEB_URL,
} from "@/lib/site-defaults";
import type { PresencePlatform } from "@/lib/presence-platform";

/** The presence platform this client reports: the web app is always "web". */
export function getCurrentPresencePlatform(): PresencePlatform {
  return "web";
}

/** True when the client operating system is Windows */
export function isWindows(): boolean {
  if (typeof window === "undefined" || typeof navigator === "undefined") {
    return false;
  }
  // @ts-ignore
  const userAgentData = navigator.userAgentData;
  if (userAgentData?.platform) {
    return /windows/i.test(userAgentData.platform);
  }
  const platform = navigator.platform || "";
  const userAgent = navigator.userAgent || "";
  return /win/i.test(platform) || /windows/i.test(userAgent);
}

export type CapturePolicy = "wgc-enabled" | "hook-exclusive";

export function getCapturePolicy(): CapturePolicy {
  const value =
    typeof import.meta !== "undefined" && typeof import.meta.env !== "undefined"
      ? import.meta.env.VITE_RALPH_CAPTURE_POLICY
      : undefined;

  return value === "hook-exclusive" ? "hook-exclusive" : "wgc-enabled";
}

export function isWgcCaptureAllowed(): boolean {
  return getCapturePolicy() === "wgc-enabled";
}

/**
 * The API origin: "" so relative URLs like `/api/servers` work as-is (the
 * web app is same-origin with the Worker).
 */
export function getApiBaseUrl(): string {
  return "";
}

/**
 * The API's public origin, for URLs that must resolve outside this page.
 * - Local Dev: `http://localhost:5173` unless overridden
 * - Production: Custom env URL or the default public web URL
 */
export function getPublicApiUrl(): string {
  const envUrl =
    typeof import.meta !== "undefined" && typeof import.meta.env !== "undefined"
      ? import.meta.env.VITE_API_BASE_URL
      : undefined;

  if (envUrl) return envUrl;

  const isDev =
    typeof import.meta !== "undefined" &&
    (import.meta as any).env?.DEV === true;

  if (isDev) return "http://localhost:5173";

  return DEFAULT_PUBLIC_WEB_URL;
}

/**
 * Public web origin to open in a system browser for user-facing flows.
 *
 * This intentionally does not use the local dev server: user-facing pages
 * (device-code approval at /activate, invites) live on the deployed site.
 */
export function getPublicWebUrl(): string {
  const envUrl =
    typeof import.meta !== "undefined" && typeof import.meta.env !== "undefined"
      ? import.meta.env.VITE_PUBLIC_WEB_URL
      : undefined;

  return (envUrl || DEFAULT_PUBLIC_WEB_URL).replace(/\/$/, "");
}

/** User-Agent for server-side fetches that identify Yap to third parties. */
export function getBotUserAgent(): string {
  return `Mozilla/5.0 (compatible; YapBot/1.0; +${getPublicWebUrl()})`;
}

/**
 * Many sites (Reddit, Instagram's embed pages) only serve OpenGraph tags or a
 * server-rendered page to known link-preview crawlers and answer anything
 * else with a bot wall or an empty script shell, so preview fetches identify
 * as one. The contact URL comes from config.
 */
export function getLinkPreviewUserAgent(): string {
  return getBotUserAgent().replace(
    "YapBot/1.0;",
    "YapBot/1.0; Discordbot/2.0;",
  );
}

/** Contact address shown for reporting abuse of shared content. */
export function getAbuseEmail(): string {
  const envEmail =
    typeof import.meta !== "undefined" && typeof import.meta.env !== "undefined"
      ? import.meta.env.VITE_ABUSE_EMAIL
      : undefined;
  return envEmail || DEFAULT_ABUSE_EMAIL;
}

/** The WebSocket base URL (protocol + host): this page's own origin. */
export function getWsBaseUrl(): string {
  const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
  return `${protocol}//${window.location.host}`;
}

/**
 * Prefix a relative path with the API base URL.
 *
 * @example
 *   apiUrl("/api/servers") // "/api/servers"
 */
export function apiUrl(path: string): string {
  return `${getApiBaseUrl()}${path}`;
}

/**
 * Prefix a relative path with the WebSocket base URL.
 *
 * @example
 *   wsUrl("/api/gateway") // "wss://yap.example/api/gateway"
 */
export function wsUrl(path: string): string {
  return `${getWsBaseUrl()}${path}`;
}

/**
 * The web app's shareable origin for URLs that users will open in a browser
 * (invite links, copy-image-link, etc.): this page's own origin.
 */
export function getWebOrigin(): string {
  return typeof window !== "undefined" ? window.location.origin : "";
}

/**
 * Private media (attachments, emoji, soundboard, camera backgrounds, voice
 * status media, Listen Together) signed with a short-lived media capability
 * for this one URL (lib/media-grant). Never the session token: URLs end up
 * in logs, the clipboard and shared links.
 */
function withMediaCapability(fullUrl: string): string {
  try {
    const urlObj = new URL(
      fullUrl,
      typeof window !== "undefined"
        ? window.location.origin
        : getPublicApiUrl(),
    );
    if (!isCapProtectedMediaPath(urlObj.pathname)) return fullUrl;

    const relative = fullUrl.startsWith("/");
    const format = (url: URL) =>
      relative ? `${url.pathname}${url.search}` : url.toString();

    // An issued capability (a DM call's soundboard clip) stands as it is;
    // a media capability is signed again (it may be stale or another user's).
    const existingCap = urlObj.searchParams.get("cap");
    if (existingCap !== null && !isMediaCap(existingCap)) {
      if (!urlObj.searchParams.has("token")) return fullUrl;
      urlObj.searchParams.delete("token");
      return format(urlObj);
    }
    // Only our own origin gets a capability (an embed can carry a
    // protocol-relative or foreign URL with one of our paths in it).
    if (!isOwnAssetOrigin(urlObj)) return fullUrl;

    const signed = signMediaUrl(urlObj);
    if (signed) return format(signed);
    if (urlObj.searchParams.has("token")) {
      urlObj.searchParams.delete("token");
      return format(urlObj);
    }
  } catch {
    // Keep the original URL if URL parsing is unavailable.
  }

  return fullUrl;
}

function isOwnAssetOrigin(url: URL): boolean {
  const origins = new Set<string>();
  if (typeof window !== "undefined") origins.add(window.location.origin);
  for (const base of [getApiBaseUrl(), getPublicApiUrl()]) {
    if (!base) continue;
    try {
      origins.add(new URL(base).origin);
    } catch {
      // Not a URL: nothing to add.
    }
  }
  return origins.has(url.origin);
}

function shouldProxyExternalAssetUrl(pathOrUrl: string): boolean {
  try {
    const url = new URL(pathOrUrl);
    return url.hostname.toLowerCase().endsWith(".googleusercontent.com");
  } catch {
    return false;
  }
}

function buildExternalAssetProxyUrl(pathOrUrl: string): string {
  const params = new URLSearchParams({ url: pathOrUrl });
  return apiUrl(`/api/proxy-media?${params.toString()}`);
}

/** `/api/proxy-media?...` when `url` is one of our proxy URLs on any host. */
function proxyMediaPathOf(url: URL): string | null {
  return url.pathname.endsWith("/api/proxy-media")
    ? `/api/proxy-media${url.search}`
    : null;
}

function hasSignedInSession(): boolean {
  try {
    return !!getStoredKovaAuthSessionToken();
  } catch {
    return false;
  }
}

/**
 * A third-party image or video, loaded through /api/proxy-media so its host
 * never sees the viewer's address: with the server's signature when the
 * server sent one for this URL, else on the media pass cookie a signed-in
 * session holds (lib/media-pass). With no session there is nothing to
 * authorize with, so only the always-proxied Google avatars go through.
 */
function externalAssetUrl(rawUrl: string): string {
  const signed = signedMediaPathFor(rawUrl);
  if (signed) return apiUrl(signed);
  if (!hasSignedInSession() && !shouldProxyExternalAssetUrl(rawUrl)) {
    return rawUrl;
  }
  return buildExternalAssetProxyUrl(rawUrl);
}

/**
 * True for a URL this app serves or may load directly: the page's own
 * origin, the API (`getApiBaseUrl`, which is "" on the web, and
 * `getPublicApiUrl`), localhost, and our own DNS zones. Host names compare without case and without the root dot.
 */
function isOwnAssetUrl(url: URL): boolean {
  const hostname = url.hostname.replace(/\.+$/, "");
  if (hostname === "localhost") return true;
  if (isOwnZoneHostname(hostname)) return true;
  if (isOwnAssetOrigin(url)) return true;
  if (hostname === url.hostname) return false;
  const bare = new URL(url.href);
  bare.hostname = hostname;
  return isOwnAssetOrigin(bare);
}

/** Schemes an asset may use that load nothing from a host. */
const INLINE_ASSET_PROTOCOLS = new Set(["data:", "blob:"]);

/** What an <img>/<video> resolves a relative URL against. */
function documentBaseUrl(): string {
  if (typeof window !== "undefined" && window.location?.href) {
    return window.location.href;
  }
  return getApiBaseUrl() || getPublicApiUrl();
}

/**
 * Returns a URL for an asset. Private media carries a short-lived media
 * capability for that one URL, because raw <img>/<video>/<a> requests cannot
 * set auth headers (lib/media-capability). Third-party media go through the
 * media proxy (externalAssetUrl).
 *
 * The input is read the way the browser will read the result: cleaned of
 * what the URL parser drops (tab, newline, C0 controls), then resolved
 * against the page. Whatever host that names decides: ours loads directly,
 * any other one through the proxy. So "/\t/evil.example/a.png" (which a
 * browser loads from evil.example) and "https:evil.example/a.png" (another
 * host from a page served over http) never reach their host.
 */
export function getAuthAssetUrl(input: string): string {
  if (typeof input !== "string") return input;
  const value = cleanUrlInput(input);
  let resolved: URL;
  try {
    resolved = new URL(value, documentBaseUrl());
  } catch {
    return withMediaCapability(value);
  }
  if (resolved.protocol !== "http:" && resolved.protocol !== "https:") {
    // data: and blob: load nothing from a host. Any other scheme (file:,
    // ftp:, smb:, ...) could reach one outside the proxy, or the local
    // network, so it loads nothing.
    return INLINE_ASSET_PROTOCOLS.has(resolved.protocol) ? value : "";
  }

  // Our proxy path on any host (a server-built payload): from our API,
  // never from that host.
  const proxyPath = proxyMediaPathOf(resolved);
  if (proxyPath) return apiUrl(proxyPath);
  if (!isOwnAssetUrl(resolved)) return externalAssetUrl(resolved.href);

  // A path of ours (one slash, not two: `//host` was another host above).
  if (/^\/(?![\\/])/.test(value)) return withMediaCapability(apiUrl(value));
  return withMediaCapability(value);
}

/** A download URL for an attachment: signed like `getAuthAssetUrl`. */
export function getDownloadUrl(pathOrUrl: string): string {
  return getAuthAssetUrl(pathOrUrl);
}

/** A URL for media (video/audio) sources; the same as `getAuthAssetUrl`. */
export const getMediaUrl = getDownloadUrl;
