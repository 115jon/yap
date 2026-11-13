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
import { MEDIA_PORTS } from "./ssrf";
import {
  CLIENT_URL_SIGNATURE_ROUND_SECONDS,
  CLIENT_URL_SIGNATURE_TTL_SECONDS,
  mediaProxyKey,
  SERVER_URL_SIGNATURE_ROUND_SECONDS,
  SERVER_URL_SIGNATURE_TTL_SECONDS,
  signatureExpiry,
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
   * How long the signature lasts. Defaults to a week, for URLs the server
   * read from data it fetched; a URL a client handed in gets a day.
   */
  expiresInSeconds?: number;
  /**
   * The expiry is rounded up to a multiple of this (a day by default), so
   * the URL stays the same, and cacheable, for that long.
   */
  roundToSeconds?: number;
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
  // The proxy serves the default http(s) ports only: a signed URL for
  // another one could only answer 400.
  if (!MEDIA_PORTS.has(url.port)) return null;

  // Every signature expires (signing.ts): a week for what the server read,
  // a day for what a client handed in. Payloads are rewritten as they
  // leave the server, so clients always get a fresh one.
  const expiresAt = signatureExpiry(
    options.expiresInSeconds ?? SERVER_URL_SIGNATURE_TTL_SECONDS,
    options.roundToSeconds ?? SERVER_URL_SIGNATURE_ROUND_SECONDS,
    options.nowMs,
  );
  const params = new URLSearchParams({ url: target });
  params.set("exp", String(expiresAt));
  params.set("sig", signMediaUrl(target, ctx.key, expiresAt));
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
    roundToSeconds: CLIENT_URL_SIGNATURE_ROUND_SECONDS,
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
 * these from the providers, so their signatures last a week. Clients
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

/**
 * A proxy URL this server signed at some point (its signature valid for
 * its own `exp`, whether or not that has passed), signed again from now;
 * any other value as it is. For URLs kept in room state (a Listen Together
 * queue) and sent again later: the payload is rewritten as it leaves, like
 * embeds, so a stored signature that has since expired still loads. A
 * value we never signed is never signed here.
 */
export function resignedMediaUrl<T>(
  value: T,
  ctx: MediaRewriteContext,
  nowMs?: number,
): T {
  if (typeof value !== "string" || !ctx.key) return value;
  let url: URL;
  try {
    url = new URL(value);
  } catch {
    return value;
  }
  if (!isProxyMediaUrl(url) || url.searchParams.has("sourceUrl")) return value;
  const inner = url.searchParams.get("url");
  const exp = url.searchParams.get("exp");
  if (
    !inner ||
    !exp ||
    // Epoch 0: the MAC over `exp` is checked, its expiry is not.
    !verifyMediaUrlSignature(
      inner,
      url.searchParams.get("sig"),
      ctx.key,
      exp,
      0,
    )
  ) {
    return value;
  }
  return (proxiedMediaUrl(inner, ctx, null, "media", { nowMs }) ?? value) as T;
}

/**
 * `value` with every `artworkUrl` and `avatarUrl` inside it re-signed
 * ([`resignedMediaUrl`]): a Listen Together snapshot as it leaves the room.
 */
export function resignRoomMediaForDelivery<T>(
  value: T,
  ctx: MediaRewriteContext = getMediaRewriteContext(),
  nowMs?: number,
): T {
  if (Array.isArray(value)) {
    return value.map((item) =>
      resignRoomMediaForDelivery(item, ctx, nowMs),
    ) as T;
  }
  if (!value || typeof value !== "object") return value;
  const next: Record<string, unknown> = {};
  for (const [field, fieldValue] of Object.entries(value)) {
    next[field] =
      (field === "artworkUrl" || field === "avatarUrl") &&
      typeof fieldValue === "string"
        ? resignedMediaUrl(fieldValue, ctx, nowMs)
        : resignRoomMediaForDelivery(fieldValue, ctx, nowMs);
  }
  return next as T;
}
