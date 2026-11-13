// Rewrites third-party media URLs in the payloads the server builds (link
// embeds, GIF results, Listen Together artwork) to signed /api/proxy-media
// URLs, so every client, older ones included, loads them from us and the
// third party never sees a viewer's address.
//
// Stored data keeps the original URLs: rewriting happens where payloads leave
// the server (clients never write embeds back: lib/embed-removal). Values no
// client may load are dropped on the way out. Social CDN hosts (X, TikTok, Instagram) keep the existing
// allowlisted `url` + `sourceUrl` form, since the proxy re-resolves their
// expiring links from the post.

import { env } from "cloudflare:workers";
import {
  getAttachmentUrl,
  isExternalAttachmentUrl,
} from "@/lib/attachment-url";
import { isTikTokMediaHostname } from "@/lib/tiktok-hosts";
import {
  type EmbedMediaField,
  mapEmbedMediaUrls,
} from "@/lib/embed-media-urls";
import { isOwnZoneHostname, classifyMediaUrl } from "@/lib/media-url-class";
import type { EmbedInfo } from "@/lib/types";
import { currentPublicOrigin } from "./origin";
import {
  CLIENT_URL_SIGNATURE_TTL_SECONDS,
  mediaProxyKey,
  signMediaUrl,
  verifyMediaUrlSignature,
} from "./signing";

export const PROXY_MEDIA_PATH = "/api/proxy-media";

export interface MediaRewriteContext {
  /** Signing key; null leaves generic URLs as they are (proxy off). */
  key: Buffer | null;
  /** Absolute origin for the rewritten URLs, without a trailing slash. */
  origin: string;
}

const INSTAGRAM_CDN_HOST_PATTERN =
  /^scontent-[a-z0-9]+(?:-[a-z0-9]+)*\.cdninstagram\.com$/;

/**
 * Hosts the legacy proxy path handles itself (Referer rules, link refresh
 * from the post): X/Twitter media, fxtwitter/vxtwitter mirrors, Instagram
 * and TikTok CDNs.
 */
export function isSocialRefreshMediaUrl(url: URL): boolean {
  if (url.protocol !== "https:") return false;
  const hostname = url.hostname.toLowerCase();
  if (hostname === "video.twimg.com" || hostname === "pbs.twimg.com") {
    return true;
  }
  if (hostname === "gif.fxtwitter.com") {
    return url.pathname.startsWith("/tweet_video/");
  }
  if (hostname === "vxtwitter.com") return url.pathname.startsWith("/tvid/");
  return (
    INSTAGRAM_CDN_HOST_PATTERN.test(hostname) ||
    isTikTokMediaHostname(hostname) ||
    hostname === "tiktok.com" ||
    hostname.endsWith(".tiktok.com")
  );
}

export function isProxyMediaUrl(url: URL): boolean {
  return url.pathname === PROXY_MEDIA_PATH;
}

let memoSecret: string | undefined;
let memoKey: Buffer | null = null;

/** The rewrite context from the Worker's env and the current request. */
export function getMediaRewriteContext(): MediaRewriteContext {
  let key: Buffer | null = null;
  try {
    const secrets = env as unknown as {
      MEDIA_PROXY_SECRET?: string;
      REALTIME_TICKET_SECRET?: string;
    };
    const base = secrets.MEDIA_PROXY_SECRET || secrets.REALTIME_TICKET_SECRET;
    if (base !== memoSecret) {
      memoSecret = base;
      memoKey = mediaProxyKey(secrets);
    }
    key = memoKey;
  } catch {
    key = null;
  }
  return { key, origin: currentPublicOrigin() };
}

export interface ProxiedMediaUrlOptions {
  /**
   * For a URL a client handed in (not one the server read from data it
   * fetched): the signature expires this many seconds from now, rounded up
   * to the hour so the URL stays cacheable for a while.
   */
  expiresInSeconds?: number;
  /** Tests only. */
  nowMs?: number;
}

function sameHost(a: URL, b: URL): boolean {
  const name = (url: URL) => url.hostname.replace(/\.+$/, "");
  return name(a) === name(b) && a.port === b.port;
}

/**
 * `raw` as payloads may carry it, or null when no payload may carry it.
 *
 * - Our own URLs (this deployment's host, any host in our own zones, or a
 *   path) stay, in their cleaned form.
 * - Our proxy URL on another host with our valid signature is rebuilt on
 *   `ctx.origin`, so no client fetches it from that host.
 * - Other http(s) media becomes a signed proxy URL; social CDN media the
 *   allowlisted form with its post; an HLS playlist in a video field stays
 *   (its segments are named by URL, so it cannot be proxied on its own).
 * - Anything else is null: other schemes, and input that parsers read
 *   differently (`https:host/x`, control characters a browser strips).
 *
 * Protocol-relative input (`//host/x`, `/\host/x`) is another host's and is
 * proxied like an https URL.
 */
export function proxiedMediaUrl(
  raw: string,
  ctx: MediaRewriteContext,
  sourceUrl?: string | null,
  field: EmbedMediaField = "media",
  options: ProxiedMediaUrlOptions = {},
): string | null {
  if (typeof raw !== "string") return null;
  const classified = classifyMediaUrl(raw);
  if (classified.kind === "path") return classified.value;
  if (classified.kind === "other") return classified.value === "" ? "" : null;
  const { url } = classified;
  const target = url.href;

  let origin: URL | null = null;
  try {
    origin = new URL(ctx.origin);
  } catch {
    origin = null;
  }
  if (origin && sameHost(url, origin)) return target;
  if (isOwnZoneHostname(url.hostname)) return target;

  // Our proxy URL on another host: a lookalike path is that host's own and
  // is proxied like any other URL, but one we signed (an older payload, or
  // another host of this deployment) loads from us.
  if (isProxyMediaUrl(url) && ctx.key) {
    const inner = url.searchParams.get("url");
    if (
      inner &&
      verifyMediaUrlSignature(
        inner,
        url.searchParams.get("sig"),
        ctx.key,
        url.searchParams.get("exp"),
        options.nowMs,
      )
    ) {
      return `${ctx.origin}${PROXY_MEDIA_PATH}${url.search}`;
    }
  }

  if (isSocialRefreshMediaUrl(url)) {
    const params = new URLSearchParams({ url: target });
    if (sourceUrl) params.set("sourceUrl", sourceUrl);
    return `${ctx.origin}${PROXY_MEDIA_PATH}?${params}`;
  }

  if (!ctx.key) return target;
  if (field === "video" && url.pathname.toLowerCase().endsWith(".m3u8")) {
    return target;
  }

  const params = new URLSearchParams({ url: target });
  if (options.expiresInSeconds !== undefined) {
    const nowSeconds = Math.floor((options.nowMs ?? Date.now()) / 1000);
    const expiresAt =
      Math.ceil((nowSeconds + options.expiresInSeconds) / 3600) * 3600;
    params.set("exp", String(expiresAt));
    params.set("sig", signMediaUrl(target, ctx.key, expiresAt));
  } else {
    params.set("sig", signMediaUrl(target, ctx.key));
  }
  return `${ctx.origin}${PROXY_MEDIA_PATH}?${params}`;
}

/**
 * `proxiedMediaUrl` for a URL a client handed in (Listen Together artwork):
 * its signature expires (CLIENT_URL_SIGNATURE_TTL_SECONDS).
 */
export function proxiedClientMediaUrl(
  raw: string,
  ctx: MediaRewriteContext,
  nowMs?: number,
): string | null {
  return proxiedMediaUrl(raw, ctx, null, "media", {
    expiresInSeconds: CLIENT_URL_SIGNATURE_TTL_SECONDS,
    nowMs,
  });
}

/**
 * The original URL inside one of our proxy URLs (relative, or absolute on
 * `ownHost` when given), else `value` itself. With `keepSocialForm`, a
 * proxy URL that carries its post (`sourceUrl`) is kept: the proxy needs
 * that post to refresh an expired X, TikTok or Instagram link.
 */
export function unwrapProxiedMediaUrl(
  value: string,
  options: { keepSocialForm?: boolean } = {},
): string {
  if (typeof value !== "string" || !value.includes(PROXY_MEDIA_PATH)) {
    return value;
  }
  try {
    const url = new URL(value, "https://placeholder.invalid");
    if (!isProxyMediaUrl(url)) return value;
    if (options.keepSocialForm && url.searchParams.has("sourceUrl")) {
      return value;
    }
    return url.searchParams.get("url") || value;
  } catch {
    return value;
  }
}

/** Embeds with every media URL proxied, for a payload that leaves us. */
export function rewriteEmbedsMedia(
  embeds: EmbedInfo[],
  ctx: MediaRewriteContext,
): EmbedInfo[] {
  if (!Array.isArray(embeds)) return embeds;
  return embeds.map((embed) =>
    mapEmbedMediaUrls(embed, (url, sourceUrl, field) =>
      proxiedMediaUrl(url, ctx, sourceUrl, field),
    ),
  );
}

/** `rewriteEmbedsMedia` with the Worker's context; passes undefined through. */
export function proxyEmbedsForDelivery<T extends EmbedInfo[] | undefined>(
  embeds: T,
): T {
  if (!Array.isArray(embeds) || embeds.length === 0) return embeds;
  return rewriteEmbedsMedia(embeds, getMediaRewriteContext()) as T;
}

interface GifRenditionLike {
  url: string;
}

interface GifItemLike {
  preview: GifRenditionLike;
  send: GifRenditionLike;
}

/**
 * A GIF picker item with its preview and send renditions proxied. The send
 * URL clients post back is unwrapped by the GIF upload route, so messages
 * keep the provider's original URL.
 */
export function rewriteGifItemMedia<T extends GifItemLike>(
  item: T,
  ctx: MediaRewriteContext,
): T {
  return {
    ...item,
    preview: {
      ...item.preview,
      url: proxiedMediaUrl(item.preview.url, ctx) ?? "",
    },
    send: { ...item.send, url: proxiedMediaUrl(item.send.url, ctx) ?? "" },
  };
}

export function proxyGifItemsForDelivery<T extends GifItemLike>(
  items: T[],
): T[] {
  const ctx = getMediaRewriteContext();
  return items.map((item) => rewriteGifItemMedia(item, ctx));
}

/**
 * An attachment's URL for a payload: the API path for stored files, and a
 * proxy URL for the external ones (sent GIFs keep the provider's URL as
 * their file key).
 */
export function proxyAttachmentUrlForDelivery(
  fileKey: string,
  ctx: MediaRewriteContext = getMediaRewriteContext(),
): string {
  const url = getAttachmentUrl(fileKey);
  return isExternalAttachmentUrl(url) ? (proxiedMediaUrl(url, ctx) ?? "") : url;
}

/**
 * A user's avatar for a payload: an OAuth provider's picture is proxied,
 * and a value no client may load is null.
 */
export function proxyAvatarUrlForDelivery<T>(avatarUrl: T): T | null {
  if (typeof avatarUrl !== "string" || !avatarUrl) return avatarUrl;
  return proxiedMediaUrl(avatarUrl, getMediaRewriteContext()) as T | null;
}

/**
 * A Listen Together response with every `artworkUrl` (YouTube and Spotify
 * thumbnails) proxied, or null when no client may load it. The server read
 * these from the providers, so their signatures do not expire. Clients
 * enqueue tracks as they got them, so the room's queue carries the proxied
 * URL to everyone in it.
 */
export function proxyArtworkUrlsForDelivery<T>(
  value: T,
  ctx: MediaRewriteContext = getMediaRewriteContext(),
): T {
  if (Array.isArray(value)) {
    return value.map((item) => proxyArtworkUrlsForDelivery(item, ctx)) as T;
  }
  if (!value || typeof value !== "object") return value;
  const next: Record<string, unknown> = {};
  for (const [field, fieldValue] of Object.entries(value)) {
    next[field] =
      field === "artworkUrl" && typeof fieldValue === "string"
        ? proxiedMediaUrl(fieldValue, ctx)
        : proxyArtworkUrlsForDelivery(fieldValue, ctx);
  }
  return next as T;
}
