// Rewrites third-party media URLs in the payloads the server builds (link
// embeds, GIF results, Listen Together artwork) to signed /api/proxy-media
// URLs, so every client, older ones included, loads them from us and the
// third party never sees a viewer's address.
//
// Stored data keeps the original URLs: rewriting happens where payloads leave
// the server, and `unwrapEmbedsMedia` undoes it for embeds a client sends
// back. Social CDN hosts (X, TikTok, Instagram) keep the existing
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
import type { EmbedInfo } from "@/lib/types";
import { currentPublicOrigin } from "./origin";
import {
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

/**
 * `raw` as a proxy URL. Unchanged: our own URLs (host or path), non-http
 * values, and an HLS playlist in a video field (its segments are named by
 * URL, so the playlist cannot be proxied on its own). A protocol-relative
 * URL is another host's and is proxied like an https one.
 */
export function proxiedMediaUrl(
  raw: string,
  ctx: MediaRewriteContext,
  sourceUrl?: string | null,
  field: EmbedMediaField = "media",
): string {
  if (typeof raw !== "string") return raw;
  let target = raw.trim();
  if (/^[\\/]{2}/.test(target)) {
    target = `https://${target.replace(/^[\\/]+/, "")}`;
  }
  if (!/^https?:\/\//i.test(target)) return raw;
  let url: URL;
  try {
    url = new URL(target);
  } catch {
    return raw;
  }
  let origin: URL | null = null;
  try {
    origin = new URL(ctx.origin);
  } catch {
    origin = null;
  }
  // Ours by host, or by our own signature (another host of this
  // deployment). A path that merely looks like our proxy on another host is
  // that host's, and is proxied like any other.
  if (origin && url.host === origin.host) return raw;
  if (isProxyMediaUrl(url) && ctx.key) {
    const inner = url.searchParams.get("url");
    if (
      inner &&
      verifyMediaUrlSignature(inner, url.searchParams.get("sig"), ctx.key)
    ) {
      return raw;
    }
  }

  if (isSocialRefreshMediaUrl(url)) {
    const params = new URLSearchParams({ url: target });
    if (sourceUrl) params.set("sourceUrl", sourceUrl);
    return `${ctx.origin}${PROXY_MEDIA_PATH}?${params}`;
  }

  if (!ctx.key) return raw;
  if (field === "video" && url.pathname.toLowerCase().endsWith(".m3u8")) {
    return raw;
  }

  const params = new URLSearchParams({
    url: target,
    sig: signMediaUrl(target, ctx.key),
  });
  return `${ctx.origin}${PROXY_MEDIA_PATH}?${params}`;
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

/** Embeds a client sent back, with our proxy URLs turned into originals. */
export function unwrapEmbedsMedia(embeds: EmbedInfo[]): EmbedInfo[] {
  if (!Array.isArray(embeds)) return embeds;
  return embeds.map((embed) =>
    mapEmbedMediaUrls(embed, (url) => unwrapProxiedMediaUrl(url)),
  );
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
    preview: { ...item.preview, url: proxiedMediaUrl(item.preview.url, ctx) },
    send: { ...item.send, url: proxiedMediaUrl(item.send.url, ctx) },
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
  return isExternalAttachmentUrl(url) ? proxiedMediaUrl(url, ctx) : url;
}

/** A user's avatar for a payload: an OAuth provider's picture is proxied. */
export function proxyAvatarUrlForDelivery<T>(avatarUrl: T): T {
  if (typeof avatarUrl !== "string" || !avatarUrl) return avatarUrl;
  return proxiedMediaUrl(avatarUrl, getMediaRewriteContext()) as T;
}

/**
 * A Listen Together response with every `artworkUrl` (YouTube and Spotify
 * thumbnails) proxied. Clients enqueue tracks as they got them, so the room's
 * queue carries the proxied URL to everyone in it.
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
