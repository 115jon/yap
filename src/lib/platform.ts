// ============================================================================
// Platform detection & URL helpers for cross-platform deployment
//
// Web:     relative URLs ("/api/...")  — SSR, same origin
// Desktop: absolute URLs ("https://ralph-meet.workers.dev/api/...")
//          running inside Tauri system webview as client-only SPA
// ============================================================================
import { isCapProtectedMediaPath, isMediaCap } from "@/lib/media-capability";
import { signMediaUrl } from "@/lib/media-grant";
import { clog } from "@/lib/console-logger";
import { getDesktopToken, getStoredKovaAuthSessionToken } from "./desktop-auth";
import { cleanUrlInput, isOwnZoneHostname } from "@/lib/media-url-class";
import { signedMediaPathFor } from "@/lib/signed-media-paths";
import { isDesktop, isMobile, isTauri } from "@/lib/runtime";
import {
  DEFAULT_ABUSE_EMAIL,
  DEFAULT_PUBLIC_WEB_URL,
} from "@/lib/site-defaults";
import type { PresencePlatform } from "@/lib/presence-platform";

const log = clog("platform");

// Detection lives in runtime.ts; re-exported for existing importers.
export { isDesktop, isMobile, isTauri };

/** True when running as a standard web app (SSR or SPA in browser). */
function isWeb(): boolean {
  return !isTauri();
}

export function getCurrentPresencePlatform(): PresencePlatform {
  if (isDesktop()) return "desktop";
  if (isMobile()) return "mobile";
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
 * The API origin used by desktop/mobile clients.
 *
 * Web mode:   returns "" so relative URLs like `/api/servers` work as-is.
 * Tauri dev:  desktop fetches through its Vite proxy; mobile fetches the public
 *             backend because Android localhost points at the device itself.
 * Tauri prod: returns the deployed Cloudflare Workers origin.
 *
 * Override at build time with `VITE_API_BASE_URL`.
 */
export function getApiBaseUrl(): string {
  if (isWeb()) return "";

  const isDev =
    typeof import.meta !== "undefined" &&
    (import.meta as any).env?.DEV === true;

  if (isDev) {
    if (isMobile()) {
      return getPublicApiUrl();
    }
    if (isDesktop()) {
      // In desktop dev, fetch through the desktop Vite server. Its proxy target
      // is controlled by VITE_API_BASE_URL, so the same app can test against
      // local Meet or the deployed Worker without changing runtime code.
      return typeof window !== "undefined" ? window.location.origin : "";
    }
    return "http://localhost:5173";
  }

  // Allow build-time override via Vite env var for production
  const envUrl =
    typeof import.meta !== "undefined" && typeof import.meta.env !== "undefined"
      ? import.meta.env.VITE_API_BASE_URL
      : undefined;
  if (envUrl) return envUrl;

  // Production Tauri build — point to the deployed Cloudflare Workers backend
  return DEFAULT_PUBLIC_WEB_URL;
}

/**
 * Returns a resolvable public origin for the API that can be accessed by the system OS (out of Tauri context).
 * - Local Dev: `http://localhost:5173` on desktop/web, deployed backend on mobile unless overridden
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

  if (isDev) {
    if (isMobile()) {
      return DEFAULT_PUBLIC_WEB_URL;
    }
    return "http://localhost:5173";
  }

  return DEFAULT_PUBLIC_WEB_URL;
}

/**
 * Public web origin to open in a system browser for user-facing flows.
 *
 * This intentionally does not use the local dev server: user-facing pages a
 * native app opens (device-code approval at /activate, invites) live on the
 * deployed Ralph Meet site.
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

/**
 * Returns the WebSocket base URL (protocol + host) for real-time connections.

 *
 * Web mode:  derives from `window.location` (same origin).
 * Tauri mode: derives from `getApiBaseUrl()`, swapping http→ws / https→wss.
 */
export function getWsBaseUrl(): string {
  if (isWeb()) {
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${window.location.host}`;
  }

  const isDev =
    typeof import.meta !== "undefined" &&
    (import.meta as any).env?.DEV === true;

  if (isDev) {
    if (isDesktop()) {
      // Connect to the desktop Vite server; it proxies /api WebSockets to the
      // VITE_API_BASE_URL target configured for this dev run.
      const url = "ws://localhost:1420";
      log.info("Using desktop dev ws URL:", url);
      return url;
    }
    if (isMobile()) {
      return getPublicApiUrl()
        .replace(/^https:/, "wss:")
        .replace(/^http:/, "ws:");
    }
  }

  const apiBase = getPublicApiUrl();
  log.info("Using ws base from apiBase:", apiBase);

  return apiBase.replace(/^https:/, "wss:").replace(/^http:/, "ws:");
}

/**
 * Prefix a relative path with the API base URL.
 * Use this for all fetch/WebSocket paths to ensure cross-platform compatibility.
 *
 * @example
 *   apiUrl("/api/servers")
 *   // Web:   "/api/servers"
 *   // Tauri: "https://ralph-meet.workers.dev/api/servers"
 */
export function apiUrl(path: string): string {
  return `${getApiBaseUrl()}${path}`;
}

/**
 * Prefix a relative path with the WebSocket base URL.
 *
 * @example
 *   wsUrl("/api/gateway")
 *   // Web:   "wss://localhost:8888/api/gateway"
 *   // Tauri: "wss://ralph-meet.workers.dev/api/gateway"
 */
export function wsUrl(path: string): string {
  return `${getWsBaseUrl()}${path}`;
}

/**
 * Returns the web app's shareable origin for constructing URLs that users will
 * open in a regular browser (invite links, copy-image-link, etc.).
 *
 * Web:        `window.location.origin`  (same origin — e.g. the Workers URL)
 * Tauri dev:  `http://localhost:5173`   (Vite dev server proxying to Workers)
 * Tauri prod: the deployed Workers URL
 */
export function getWebOrigin(): string {
  if (isWeb()) {
    return typeof window !== "undefined" ? window.location.origin : "";
  }

  return getPublicApiUrl();
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
    return !!(getDesktopToken() ?? getStoredKovaAuthSessionToken());
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
 * `getPublicApiUrl`), localhost and the Tauri webview, and our own DNS
 * zones. Host names compare without case and without the root dot.
 */
function isOwnAssetUrl(url: URL): boolean {
  const hostname = url.hostname.replace(/\.+$/, "");
  if (hostname === "localhost" || hostname === "tauri.localhost") return true;
  if (isOwnZoneHostname(hostname)) return true;
  if (isOwnAssetOrigin(url)) return true;
  if (hostname === url.hostname) return false;
  const bare = new URL(url.href);
  bare.hostname = hostname;
  return isOwnAssetOrigin(bare);
}

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
 * host from a page on http://tauri.localhost) never reach their host.
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
  // data:, blob: and the like load nothing from a host.
  if (resolved.protocol !== "http:" && resolved.protocol !== "https:") {
    return withMediaCapability(value);
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

/**
 * Returns a download URL for an attachment that resolves in a system browser.
 *
 * In Tauri, `http://tauri.localhost` URLs don't resolve outside the webview,
 * so we reconstruct the URL using the real backend origin (the dev server or
 * the deployed Workers URL), signed just like `getAuthAssetUrl`.
 */
export function getDownloadUrl(pathOrUrl: string): string {
  if (!isTauri()) {
    // On web, same-origin URLs work fine
    return getAuthAssetUrl(pathOrUrl);
  }

  // In Tauri, build a URL that points to the real backend
  let path = pathOrUrl;
  // Strip any existing origin (e.g. http://tauri.localhost/api/...)
  if (path.startsWith("http")) {
    try {
      const u = new URL(path);
      const isInternal =
        u.hostname === "tauri.localhost" ||
        u.hostname === "localhost" ||
        (getApiBaseUrl() && u.hostname === new URL(getApiBaseUrl()).hostname) ||
        u.hostname === new URL(getPublicApiUrl()).hostname;

      if (!isInternal) {
        return pathOrUrl;
      }
      path = u.pathname + u.search;
    } catch {
      /* keep as-is */
    }
  }

  // The real backend origin that resolves outside Tauri
  const backendOrigin = getPublicApiUrl();

  const fullUrl = `${backendOrigin}${path}`;

  return withMediaCapability(fullUrl);
}

/**
 * Returns a URL for media (video/audio) sources that supports range requests.
 *
 * In Tauri, the `tauri.localhost` custom protocol caches responses and breaks
 * HTTP Range requests, which prevents video seeking/scrubbing. By routing
 * media through the real backend origin (same as `getDownloadUrl`), we ensure
 * standard HTTP semantics work correctly.
 *
 * On web, this is identical to `getAuthAssetUrl`.
 */
export const getMediaUrl = getDownloadUrl;
