import * as nodeCrypto from "node:crypto";
import { createFileRoute } from "@tanstack/react-router";
import * as cloudflareWorkers from "cloudflare:workers";
import {
  apiError,
  apiSuccess,
  getEnv,
  isDesktopOrigin,
  requireAuth,
} from "@/lib/api-helpers";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { cacheFetch, cacheGet, cacheSet } from "@/lib/cache";
import { clog } from "@/lib/console-logger";
import { resolveInstagramVideoMetadata } from "@/lib/instagram-video-resolver";
import {
  fetchInstagramOEmbedMetadata,
  fetchTikTokProxyMetadata,
} from "@/lib/share-preview-proxy";
import { isTikTokMediaHostname } from "@/lib/tiktok-hosts";
import type { EmbedInfo } from "@/lib/types";
import { serveGenericMedia } from "@/server/media-proxy/generic";
import { isSocialRefreshMediaUrl } from "@/server/media-proxy/rewrite";
import {
  issueMediaPass,
  MEDIA_PASS_TTL_SECONDS,
  mediaPassCookie,
  mediaProxyKey,
  readMediaPassCookie,
  verifyMediaPass,
  verifyMediaUrlSignature,
} from "@/server/media-proxy/signing";
import { createDohResolver, MEDIA_PORTS } from "@/server/media-proxy/ssrf";
import { extractAndProcessEmbeds } from "@/services/embed-fetcher";

const log = clog("proxy-media");

const ALLOWED_HOSTS = new Set([
  "video.twimg.com",
  "pbs.twimg.com",
  // vxtwitter.com is not listed: only its /tvid/ video paths are (below).
  "tenor.com",
  "lh3.googleusercontent.com",
  // Steam avatars on profile connections (services/connections/steam.ts).
  "avatars.steamstatic.com",
  "avatars.akamai.steamstatic.com",
  "avatars.fastly.steamstatic.com",
  "avatars.cloudflare.steamstatic.com",
]);

const KLIPY_STATIC_HOST_PATTERN = /^static\d*\.klipy\.com$/;
const TENOR_MEDIA_HOST_PATTERN = /^media\d*\.tenor\.com$/;
const INSTAGRAM_CDN_HOST_PATTERN =
  /^scontent-[a-z0-9]+(?:-[a-z0-9]+)*\.cdninstagram\.com$/;

const X_SOURCE_HOSTS = new Set([
  "x.com",
  "www.x.com",
  "mobile.x.com",
  "twitter.com",
  "www.twitter.com",
  "mobile.twitter.com",
  "fxtwitter.com",
  "d.fxtwitter.com",
  "fixupx.com",
  "d.fixupx.com",
  "vxtwitter.com",
  "d.vxtwitter.com",
  "fixvx.com",
  "d.fixvx.com",
]);

const TIKTOK_REFRESH_TTL = 50 * 60;
const TIKTOK_REFRESH_SAFETY_WINDOW = 5 * 60;
const X_REFRESH_TTL = 20 * 60;
const INSTAGRAM_REFRESH_TTL = 20 * 60;

interface RefreshableMediaCandidate {
  type: "image" | "video" | "audio";
  url: string;
  thumbnailUrl?: string;
}

interface ResolveRefreshedMediaOptions {
  forceRefresh?: boolean;
  bypassCache?: boolean;
}

const TIKTOK_PROXY_RESPONSE_TTL = 5 * 60;
const MAX_UPSTREAM_REDIRECTS = 3;

export function isAllowedMediaUrl(url: URL): boolean {
  if (url.protocol !== "https:") return false;
  const hostname = url.hostname.toLowerCase();
  if (hostname === "gif.fxtwitter.com") {
    return (
      url.pathname.startsWith("/tweet_video/") &&
      url.pathname.toLowerCase().endsWith(".webp")
    );
  }
  if (ALLOWED_HOSTS.has(hostname)) return true;

  if (hostname === "vxtwitter.com") {
    return url.pathname.startsWith("/tvid/");
  }

  return (
    KLIPY_STATIC_HOST_PATTERN.test(hostname) ||
    TENOR_MEDIA_HOST_PATTERN.test(hostname) ||
    INSTAGRAM_CDN_HOST_PATTERN.test(hostname) ||
    isTikTokMediaHostname(hostname) ||
    hostname === "tiktok.com" ||
    hostname.endsWith(".tiktok.com")
  );
}

async function makeSyntheticRangeResponse(
  upstream: Response,
  range: string,
): Promise<Response | null> {
  if (upstream.status !== 200) return null;

  const match = range.trim().match(/^bytes=(\d*)-(\d*)$/);
  if (!match) return null;

  const contentLength = Number(upstream.headers.get("Content-Length"));
  if (!Number.isFinite(contentLength) || contentLength <= 0) return null;

  const startText = match[1];
  const endText = match[2];
  let start: number;
  let end: number;

  if (!startText && endText) {
    const suffixLength = Number(endText);
    if (!Number.isFinite(suffixLength) || suffixLength <= 0) return null;
    start = Math.max(0, contentLength - suffixLength);
    end = contentLength - 1;
  } else {
    start = Number(startText);
    end = endText ? Number(endText) : contentLength - 1;
  }

  if (
    !Number.isFinite(start) ||
    !Number.isFinite(end) ||
    start < 0 ||
    end < start ||
    start >= contentLength
  ) {
    const headers = new Headers();
    headers.set("Content-Range", `bytes */${contentLength}`);
    headers.set("Accept-Ranges", "bytes");
    return new Response(null, { status: 416, headers });
  }

  end = Math.min(end, contentLength - 1);
  const rangeLength = end - start + 1;

  const headers = buildProxyHeaders(upstream.headers, upstream.url);
  headers.set("Accept-Ranges", "bytes");
  headers.set("Content-Range", `bytes ${start}-${end}/${contentLength}`);
  headers.set("Content-Length", rangeLength.toString());
  headers.set("Cache-Control", "no-store");

  const body = upstream.body;
  if (!body) return null;

  // Stream the upstream body and emit only the requested byte window, rather
  // than buffering the entire response into memory. Bytes before `start` are
  // discarded, bytes after `end` are never read (we cancel the source).
  const sliced = new ReadableStream<Uint8Array>({
    async start(controller) {
      const reader = body.getReader();
      let position = 0; // absolute offset of the next incoming byte
      let emitted = 0;
      try {
        while (emitted < rangeLength) {
          const { done, value } = await reader.read();
          if (done) break;
          if (!value || value.byteLength === 0) continue;

          const chunkStart = position;
          const chunkEnd = position + value.byteLength - 1;
          position += value.byteLength;

          // Skip chunks entirely before the requested window.
          if (chunkEnd < start) continue;

          // Intersect [chunkStart, chunkEnd] with [start, end].
          const sliceFrom = Math.max(0, start - chunkStart);
          const sliceTo = Math.min(
            value.byteLength,
            sliceFrom + (rangeLength - emitted),
          );
          const piece = value.subarray(sliceFrom, sliceTo);
          controller.enqueue(piece);
          emitted += piece.byteLength;
        }
        controller.close();
      } catch (err) {
        controller.error(err);
      } finally {
        // Stop pulling more bytes from the upstream once we have our window.
        await reader.cancel().catch(() => {});
        reader.releaseLock();
      }
    },
  });

  return new Response(sliced, {
    status: 206,
    statusText: "Partial Content",
    headers,
  });
}

export function inferMediaContentType(
  contentType: string | null,
  sourceUrl?: string,
): string {
  const normalized = contentType?.split(";")[0].trim().toLowerCase();
  if (
    normalized?.startsWith("video/") ||
    normalized?.startsWith("image/") ||
    normalized?.startsWith("audio/")
  ) {
    return contentType || normalized;
  }

  if (sourceUrl) {
    try {
      const parsedUrl = new URL(sourceUrl);
      const hostname = parsedUrl.hostname.toLowerCase();
      const pathname = parsedUrl.pathname.toLowerCase();
      const mimeType = parsedUrl.searchParams.get("mime_type")?.toLowerCase();
      if (mimeType?.startsWith("audio/") || mimeType?.startsWith("video/")) {
        return mimeType;
      }
      if (hostname === "video.twimg.com") {
        return "video/mp4";
      }
      if (
        hostname === "vxtwitter.com" &&
        pathname.startsWith("/tvid/") &&
        (pathname.includes("/vid/") ||
          pathname.includes("/amplify_video/") ||
          pathname.includes("/ext_tw_video/") ||
          pathname.includes("/tweet_video/"))
      ) {
        return "video/mp4";
      }
      if (
        pathname.endsWith(".mp4") ||
        pathname.includes("/mp4/") ||
        pathname.includes("/avc1/")
      ) {
        return "video/mp4";
      }
      if (
        pathname.includes("/video/") ||
        pathname.includes("/aweme/v1/play/")
      ) {
        return "video/mp4";
      }
      if (pathname.endsWith(".mp3")) return "audio/mpeg";
      if (pathname.endsWith(".m4a")) return "audio/mp4";
      if (pathname.endsWith(".aac")) return "audio/aac";
      if (pathname.endsWith(".wav")) return "audio/wav";
      if (pathname.endsWith(".oga") || pathname.endsWith(".opus"))
        return "audio/ogg";
      if (pathname.includes("/audio/")) return "audio/mpeg";
      if (pathname.endsWith(".webm")) return "video/webm";
      if (pathname.endsWith(".ogg")) return "video/ogg";
      if (pathname.endsWith(".m3u8")) return "application/vnd.apple.mpegurl";
      if (pathname.match(/\.(jpe?g|png|gif|webp)$/)) {
        const extension = pathname.split(".").pop();
        return extension === "jpg" ? "image/jpeg" : `image/${extension}`;
      }
      if (hostname.endsWith(".googleusercontent.com")) {
        return "image/jpeg";
      }
    } catch {
      // Fall through to octet-stream.
    }
  }

  return "application/octet-stream";
}

export function normalizeRefreshableMediaKey(rawUrl: string): string {
  try {
    const parsed = new URL(rawUrl);
    const hostname = parsed.hostname.toLowerCase();

    if (
      hostname === "video.twimg.com" ||
      hostname === "pbs.twimg.com" ||
      hostname.endsWith(".cdninstagram.com") ||
      isTikTokMediaHostname(hostname)
    ) {
      return `${hostname}${parsed.pathname}`;
    }

    return parsed.toString();
  } catch {
    return rawUrl;
  }
}

function isLikelyTikTokImageRequest(rawUrl: string): boolean {
  try {
    const parsed = new URL(rawUrl);
    const hostname = parsed.hostname.toLowerCase();
    if (!isTikTokMediaHostname(hostname)) return false;

    const pathname = parsed.pathname.toLowerCase();
    const scene = parsed.searchParams.get("sc")?.toLowerCase();
    const mimeType = parsed.searchParams.get("mime_type")?.toLowerCase();

    return (
      mimeType?.startsWith("image/") === true ||
      scene === "avatar" ||
      scene === "cover" ||
      scene === "feed_cover" ||
      pathname.includes("origin.image") ||
      pathname.includes("photomode-image") ||
      pathname.includes("cropcenter") ||
      pathname.includes("shrink-aq")
    );
  } catch {
    return false;
  }
}

function instagramMediaFamilyKey(rawUrl: string): string | null {
  try {
    const parsed = new URL(rawUrl);
    if (!parsed.hostname.toLowerCase().endsWith(".cdninstagram.com")) {
      return null;
    }
    const parts = parsed.pathname.split("/").filter(Boolean);
    return parts[1] ?? parts[0] ?? null;
  } catch {
    return null;
  }
}

function normalizeRefreshableMediaLooseKey(rawUrl: string): string {
  try {
    const parsed = new URL(rawUrl);
    const hostname = parsed.hostname.toLowerCase();

    if (isTikTokMediaHostname(hostname)) {
      return parsed.pathname;
    }

    return normalizeRefreshableMediaKey(rawUrl);
  } catch {
    return rawUrl;
  }
}

function pushUniqueUrl(
  target: string[],
  seen: Set<string>,
  url?: string | null,
) {
  if (!url || seen.has(url)) return;
  seen.add(url);
  target.push(url);
}

export function pickRefreshedMediaUrls(
  candidates: RefreshableMediaCandidate[],
  requestUrl: string,
): string[] {
  if (candidates.length === 0) return [];

  const results: string[] = [];
  const seen = new Set<string>();
  const requestKey = normalizeRefreshableMediaKey(requestUrl);
  const requestLooseKey = normalizeRefreshableMediaLooseKey(requestUrl);

  for (const candidate of candidates) {
    if (normalizeRefreshableMediaKey(candidate.url) === requestKey) {
      pushUniqueUrl(results, seen, candidate.url);
    }
    if (
      candidate.thumbnailUrl &&
      normalizeRefreshableMediaKey(candidate.thumbnailUrl) === requestKey
    ) {
      pushUniqueUrl(results, seen, candidate.thumbnailUrl);
    }
  }

  if (requestLooseKey !== requestKey) {
    for (const candidate of candidates) {
      if (
        normalizeRefreshableMediaLooseKey(candidate.url) === requestLooseKey
      ) {
        pushUniqueUrl(results, seen, candidate.url);
      }
      if (
        candidate.thumbnailUrl &&
        normalizeRefreshableMediaLooseKey(candidate.thumbnailUrl) ===
          requestLooseKey
      ) {
        pushUniqueUrl(results, seen, candidate.thumbnailUrl);
      }
    }
  }

  const requestedType = inferMediaContentType(null, requestUrl);
  const shouldPreferImageCandidates =
    requestedType.startsWith("image/") ||
    (requestedType === "application/octet-stream" &&
      isLikelyTikTokImageRequest(requestUrl));
  if (requestedType.startsWith("video/")) {
    for (const candidate of candidates) {
      if (candidate.type === "video") {
        pushUniqueUrl(results, seen, candidate.url);
      }
    }
    return results;
  }

  if (requestedType.startsWith("audio/")) {
    for (const candidate of candidates) {
      if (candidate.type === "audio") {
        pushUniqueUrl(results, seen, candidate.url);
      }
    }
    return results;
  }

  if (shouldPreferImageCandidates) {
    const requestFamily = instagramMediaFamilyKey(requestUrl);
    const familyMatches: string[] = [];
    const otherImages: string[] = [];
    for (const candidate of candidates) {
      if (candidate.type !== "image") continue;
      if (
        requestFamily &&
        instagramMediaFamilyKey(candidate.url) === requestFamily
      ) {
        familyMatches.push(candidate.url);
      } else {
        otherImages.push(candidate.url);
      }
    }
    for (const url of familyMatches) {
      pushUniqueUrl(results, seen, url);
    }
    for (const url of otherImages) {
      pushUniqueUrl(results, seen, url);
    }
    for (const candidate of candidates) {
      pushUniqueUrl(results, seen, candidate.thumbnailUrl);
    }
    return results;
  }

  for (const candidate of candidates) {
    pushUniqueUrl(results, seen, candidate.url);
    pushUniqueUrl(results, seen, candidate.thumbnailUrl);
  }

  return results;
}

export function pickRefreshedMediaUrl(
  candidates: RefreshableMediaCandidate[],
  requestUrl: string,
): string | null {
  return pickRefreshedMediaUrls(candidates, requestUrl)[0] ?? null;
}

function buildProxyHeaders(
  upstreamHeaders: Headers,
  sourceUrl?: string,
): Headers {
  const headers = new Headers();
  const passthroughHeaders = [
    "Accept-Ranges",
    "Cache-Control",
    "Content-Length",
    "Content-Range",
    "Content-Type",
    "ETag",
    "Last-Modified",
  ];

  for (const header of passthroughHeaders) {
    const value = upstreamHeaders.get(header);
    if (value) headers.set(header, value);
  }

  const contentType = inferMediaContentType(
    headers.get("Content-Type"),
    sourceUrl,
  );
  headers.set("Content-Type", contentType);

  headers.set("Access-Control-Allow-Origin", "*");
  headers.set("Cross-Origin-Resource-Policy", "cross-origin");
  headers.set("Referrer-Policy", "no-referrer");
  if (
    !contentType.startsWith("video/") &&
    !contentType.startsWith("image/") &&
    !contentType.startsWith("audio/")
  ) {
    headers.set("X-Content-Type-Options", "nosniff");
  }

  return headers;
}

/**
 * A URL for logs: host and query keys, never the path or values (the source
 * is a user-posted link; its path says what someone shared).
 */
function summarizeUrlForLog(value: string | URL) {
  try {
    const url = typeof value === "string" ? new URL(value) : value;
    return {
      protocol: url.protocol,
      hostname: url.hostname,
      queryKeys: Array.from(url.searchParams.keys()).slice(0, 12),
    };
  } catch {
    return { invalid: true };
  }
}

function readUrlExpiryEpochSeconds(
  rawUrl: string,
  queryKey: string,
): number | null {
  try {
    const parsed = new URL(rawUrl);
    const rawValue = parsed.searchParams.get(queryKey);
    if (!rawValue) return null;
    const parsedValue = Number(rawValue);
    return Number.isFinite(parsedValue) && parsedValue > 0 ? parsedValue : null;
  } catch {
    return null;
  }
}

function isTikTokUrlExpiringSoon(rawUrl: string): boolean {
  const expiry = readUrlExpiryEpochSeconds(rawUrl, "x-expires");
  if (!expiry) return false;
  return expiry - Math.floor(Date.now() / 1000) <= TIKTOK_REFRESH_SAFETY_WINDOW;
}

function getTikTokRefreshCacheTtl(
  candidates: RefreshableMediaCandidate[],
): number {
  const expiries = candidates.flatMap((candidate) => {
    const nextExpiries = [
      readUrlExpiryEpochSeconds(candidate.url, "x-expires"),
      candidate.thumbnailUrl
        ? readUrlExpiryEpochSeconds(candidate.thumbnailUrl, "x-expires")
        : null,
    ].filter((value): value is number => value !== null);
    return nextExpiries;
  });

  if (expiries.length === 0) return TIKTOK_REFRESH_TTL;

  const earliestExpiry = Math.min(...expiries);
  const secondsUntilRefresh =
    earliestExpiry -
    Math.floor(Date.now() / 1000) -
    TIKTOK_REFRESH_SAFETY_WINDOW;
  return Math.max(60, Math.min(TIKTOK_REFRESH_TTL, secondsUntilRefresh));
}

function areTikTokRefreshCandidatesStillFresh(
  candidates: RefreshableMediaCandidate[],
): boolean {
  if (candidates.length === 0) return false;
  return getTikTokRefreshCacheTtl(candidates) > 60;
}

function getTikTokProxyCacheControl(
  rawUrl: string,
  status: number,
  contentType?: string | null,
): string {
  if (status >= 400) return "no-store";
  if (contentType?.startsWith("video/")) return "no-store";

  const expiry = readUrlExpiryEpochSeconds(rawUrl, "x-expires");
  if (!expiry) {
    return "private, max-age=60";
  }

  const secondsUntilRefresh =
    expiry - Math.floor(Date.now() / 1000) - TIKTOK_REFRESH_SAFETY_WINDOW;
  const ttl = Math.max(
    30,
    Math.min(TIKTOK_PROXY_RESPONSE_TTL, secondsUntilRefresh),
  );
  return `private, max-age=${ttl}`;
}

function isTikTokMediaUrl(url: URL): boolean {
  return isTikTokMediaHostname(url.hostname);
}

function isTikTokSourceUrl(url: URL): boolean {
  return url.hostname.toLowerCase().includes("tiktok.com");
}

function isXSourceUrl(url: URL): boolean {
  return X_SOURCE_HOSTS.has(url.hostname.toLowerCase());
}

function isInstagramSourceUrl(url: URL): boolean {
  return url.hostname.toLowerCase().includes("instagram.com");
}

function canonicalizeTikTokUrl(url: URL): string {
  return `https://www.tiktok.com${url.pathname}`;
}

function canonicalizeXUrl(url: URL): string {
  const parts = url.pathname.split("/").filter(Boolean);
  const statusIndex = parts.findIndex((part) => part === "status");
  if (statusIndex <= 0 || !parts[statusIndex + 1]) {
    return url.toString();
  }
  return `https://x.com/${parts[statusIndex - 1]}/status/${parts[statusIndex + 1]}`;
}

function canonicalizeInstagramUrl(url: URL): string {
  const pathname = url.pathname.endsWith("/")
    ? url.pathname
    : `${url.pathname}/`;
  return `https://www.instagram.com${pathname}`;
}

function collectXRefreshCandidates(
  embeds: EmbedInfo[],
): RefreshableMediaCandidate[] {
  const candidates: RefreshableMediaCandidate[] = [];
  const seen = new Set<string>();

  const push = (
    type: "image" | "video",
    url?: string,
    thumbnailUrl?: string,
  ) => {
    if (!url) return;
    const dedupeKey = `${type}:${normalizeRefreshableMediaKey(url)}`;
    if (seen.has(dedupeKey)) return;
    seen.add(dedupeKey);
    candidates.push({ type, url, thumbnailUrl });
  };

  for (const embed of embeds) {
    if (Array.isArray(embed.media) && embed.media.length > 0) {
      for (const media of embed.media) {
        push(media.type, media.url, media.thumbnailUrl ?? embed.thumbnail?.url);
      }
    } else if (embed.video?.url && embed.video.kind !== "player") {
      push("video", embed.video.url, embed.thumbnail?.url);
    }

    if (embed.thumbnail?.url) {
      push("image", embed.thumbnail.url);
    }

    for (const media of embed.referencedTweet?.media ?? []) {
      push(media.type, media.url, media.thumbnailUrl);
    }
  }

  return candidates;
}

function collectInstagramRefreshCandidates(
  metadata: Awaited<ReturnType<typeof resolveInstagramVideoMetadata>>,
): RefreshableMediaCandidate[] {
  if (!metadata) return [];

  const candidates: RefreshableMediaCandidate[] = [];
  const seen = new Set<string>();

  const push = (
    type: "image" | "video" | "audio",
    url?: string,
    thumbnailUrl?: string,
  ) => {
    if (!url) return;
    const dedupeKey = `${type}:${normalizeRefreshableMediaKey(url)}`;
    if (seen.has(dedupeKey)) return;
    seen.add(dedupeKey);
    candidates.push({ type, url, thumbnailUrl });
  };

  for (const media of metadata.media ?? []) {
    push(media.type, media.url, media.thumbnailUrl);
  }

  if (metadata.videoUrl) {
    push("video", metadata.videoUrl, metadata.thumbnailUrl ?? undefined);
  }

  if (metadata.thumbnailUrl) {
    push("image", metadata.thumbnailUrl);
  }

  if (metadata.authorAvatarUrl) {
    push("image", metadata.authorAvatarUrl);
  }

  if (metadata.audio?.url) {
    push("audio", metadata.audio.url);
  }

  if (metadata.audio?.artworkUrl) {
    push("image", metadata.audio.artworkUrl);
  }

  return candidates;
}

function collectTikTokRefreshCandidates(
  metadata: Awaited<ReturnType<typeof fetchTikTokProxyMetadata>>,
): RefreshableMediaCandidate[] {
  if (!metadata) return [];

  const candidates: RefreshableMediaCandidate[] = [];
  const seen = new Set<string>();

  const push = (
    type: "image" | "video" | "audio",
    url?: string,
    thumbnailUrl?: string,
  ) => {
    if (!url) return;
    const dedupeKey = `${type}:${normalizeRefreshableMediaKey(url)}`;
    if (seen.has(dedupeKey)) return;
    seen.add(dedupeKey);
    candidates.push({ type, url, thumbnailUrl });
  };

  for (const media of metadata.media ?? []) {
    push(media.type, media.url, media.thumbnailUrl);
  }

  if (metadata.videoUrl) {
    push("video", metadata.videoUrl, metadata.coverUrl ?? undefined);
  }

  if (metadata.coverUrl) {
    push("image", metadata.coverUrl);
  }

  if (metadata.authorAvatarUrl) {
    push("image", metadata.authorAvatarUrl);
  }

  if (metadata.audio?.url) {
    push("audio", metadata.audio.url);
  }

  if (metadata.audio?.artworkUrl) {
    push("image", metadata.audio.artworkUrl);
  }

  return candidates;
}

async function getTikTokRefreshCandidates(
  canonicalUrl: string,
  options?: ResolveRefreshedMediaOptions,
): Promise<RefreshableMediaCandidate[]> {
  const cacheKey = `v1:proxy-media:tiktok:${canonicalUrl}`;
  const cached = await cacheGet<RefreshableMediaCandidate[]>(cacheKey);
  const cachedIsFresh =
    cached !== null && areTikTokRefreshCandidatesStillFresh(cached);

  if (cachedIsFresh && !options?.bypassCache) {
    return cached;
  }

  const freshCandidates = collectTikTokRefreshCandidates(
    await fetchTikTokProxyMetadata(canonicalUrl),
  );
  if (freshCandidates.length > 0) {
    cacheSet(
      cacheKey,
      freshCandidates,
      getTikTokRefreshCacheTtl(freshCandidates),
    ).catch(() => {});
    return freshCandidates;
  }

  return cachedIsFresh ? cached : [];
}

async function resolveRefreshedMediaUrl(
  sourceUrlText: string,
  requestUrl: string,
  options?: ResolveRefreshedMediaOptions,
): Promise<string | null> {
  let sourceUrl: URL;
  try {
    sourceUrl = new URL(sourceUrlText);
  } catch {
    return null;
  }

  if (isTikTokSourceUrl(sourceUrl)) {
    const canonicalUrl = canonicalizeTikTokUrl(sourceUrl);
    const candidates = await getTikTokRefreshCandidates(canonicalUrl, options);
    return pickRefreshedMediaUrl(candidates, requestUrl);
  }

  if (isInstagramSourceUrl(sourceUrl)) {
    const canonicalUrl = canonicalizeInstagramUrl(sourceUrl);
    const cacheKey = `v1:proxy-media:instagram:${canonicalUrl}`;
    const fetchCandidates = async () => {
      const video = await resolveInstagramVideoMetadata(canonicalUrl, {
        bypassCache: options?.bypassCache,
      });
      const refreshed = await fetchInstagramOEmbedMetadata(canonicalUrl);
      const nextCandidates = collectInstagramRefreshCandidates(video);
      if (refreshed?.thumbnailUrl) {
        nextCandidates.push({
          type: "image",
          url: refreshed.thumbnailUrl,
        });
      }
      return nextCandidates;
    };
    const candidates = options?.bypassCache
      ? await fetchCandidates()
      : await cacheFetch<RefreshableMediaCandidate[]>(
          cacheKey,
          INSTAGRAM_REFRESH_TTL,
          fetchCandidates,
        );
    if (options?.bypassCache && candidates.length > 0) {
      void Promise.resolve(
        cacheSet(cacheKey, candidates, INSTAGRAM_REFRESH_TTL),
      ).catch(() => {});
    }
    return pickRefreshedMediaUrl(candidates, requestUrl);
  }

  if (isXSourceUrl(sourceUrl)) {
    const canonicalUrl = canonicalizeXUrl(sourceUrl);
    const cacheKey = `v1:proxy-media:x:${canonicalUrl}`;
    const candidates = await cacheFetch<RefreshableMediaCandidate[]>(
      cacheKey,
      X_REFRESH_TTL,
      async () =>
        collectXRefreshCandidates(await extractAndProcessEmbeds(canonicalUrl)),
    );
    return pickRefreshedMediaUrl(candidates, requestUrl);
  }

  return null;
}

async function resolveRefreshedMediaUrls(
  sourceUrlText: string,
  requestUrl: string,
  options?: ResolveRefreshedMediaOptions,
): Promise<string[]> {
  let sourceUrl: URL;
  try {
    sourceUrl = new URL(sourceUrlText);
  } catch {
    return [];
  }

  if (isTikTokSourceUrl(sourceUrl)) {
    const canonicalUrl = canonicalizeTikTokUrl(sourceUrl);
    const candidates = await getTikTokRefreshCandidates(canonicalUrl, options);
    return pickRefreshedMediaUrls(candidates, requestUrl);
  }

  const refreshedUrl = await resolveRefreshedMediaUrl(
    sourceUrlText,
    requestUrl,
    options,
  );
  return refreshedUrl ? [refreshedUrl] : [];
}

export async function proxyMedia(
  request: Request,
  includeBody: boolean,
): Promise<Response> {
  const requestUrl = new URL(request.url);
  const mediaUrlParam = requestUrl.searchParams.get("url");
  const sourceUrlParam = requestUrl.searchParams.get("sourceUrl");

  if (!mediaUrlParam) {
    return new Response("Missing media URL", { status: 400 });
  }

  let mediaUrl: URL;
  try {
    mediaUrl = new URL(mediaUrlParam);
  } catch {
    return new Response("Invalid media URL", { status: 400 });
  }

  if (!isAllowedMediaUrl(mediaUrl)) {
    return new Response("Unsupported media URL", { status: 400 });
  }

  const upstreamHeaders = new Headers();
  upstreamHeaders.set(
    "User-Agent",
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
  );
  upstreamHeaders.set(
    "Accept",
    request.headers.get("Accept") || "video/*,*/*;q=0.8",
  );
  upstreamHeaders.set("Accept-Language", "en-US,en;q=0.9");

  const requestedContentType = inferMediaContentType(null, mediaUrl.toString());
  const isTikTokMediaRequest = isTikTokMediaUrl(mediaUrl);
  const isTikTokSourceRequest = isTikTokSourceUrl(mediaUrl);
  const isTikTokProxyRequest = isTikTokMediaRequest || isTikTokSourceRequest;
  let shouldStreamTikTokMedia = false;
  let hostname = mediaUrl.hostname.toLowerCase();
  if (hostname.includes("tiktok")) {
    upstreamHeaders.set("Referer", "https://www.tiktok.com/");
    upstreamHeaders.set("Origin", "https://www.tiktok.com");
  } else if (hostname === "video.twimg.com" || hostname === "pbs.twimg.com") {
    upstreamHeaders.set("Referer", "https://x.com/");
    upstreamHeaders.set("Origin", "https://x.com");
  }

  const range = request.headers.get("Range");
  if (range) {
    upstreamHeaders.set("Range", range);
  }

  const fetchUpstream = async (targetUrl: string): Promise<Response> => {
    let currentUrl = targetUrl;

    for (let hop = 0; hop <= MAX_UPSTREAM_REDIRECTS; hop += 1) {
      let parsedUrl: URL;
      try {
        parsedUrl = new URL(currentUrl);
      } catch {
        return new Response("Invalid upstream redirect", { status: 502 });
      }

      if (!isAllowedMediaUrl(parsedUrl)) {
        return new Response("Unsupported upstream redirect", { status: 502 });
      }

      const response = await fetch(currentUrl, {
        method: includeBody || shouldStreamTikTokMedia ? "GET" : "HEAD",
        headers: upstreamHeaders,
        redirect: "manual",
      });
      if (![301, 302, 303, 307, 308].includes(response.status)) {
        return response;
      }

      const location = response.headers.get("Location");
      if (!location) return response;
      if (hop === MAX_UPSTREAM_REDIRECTS) {
        return new Response("Too many upstream redirects", { status: 508 });
      }

      try {
        currentUrl = new URL(location, parsedUrl).toString();
      } catch {
        return new Response("Invalid upstream redirect", { status: 502 });
      }
    }

    return new Response("Too many upstream redirects", { status: 508 });
  };

  let refreshedTikTokUrls: string[] = [];
  const attemptedTikTokUrls = new Set<string>();

  if (isTikTokProxyRequest) {
    const refreshSource = sourceUrlParam || mediaUrl.toString();
    refreshedTikTokUrls = await resolveRefreshedMediaUrls(
      refreshSource,
      mediaUrl.toString(),
      {
        forceRefresh:
          isTikTokMediaRequest && isTikTokUrlExpiringSoon(mediaUrl.toString()),
      },
    );
    const resolvedUrl = refreshedTikTokUrls[0] ?? mediaUrl.toString();
    let resolvedMediaUrl: URL;
    try {
      resolvedMediaUrl = new URL(resolvedUrl);
    } catch {
      return new Response("Unsupported refreshed media URL", { status: 502 });
    }
    if (!isAllowedMediaUrl(resolvedMediaUrl)) {
      return new Response("Unsupported refreshed media URL", { status: 502 });
    }

    const resolvedContentType = inferMediaContentType(null, resolvedUrl);
    shouldStreamTikTokMedia =
      requestedContentType.startsWith("image/") ||
      requestedContentType.startsWith("video/") ||
      requestedContentType.startsWith("audio/") ||
      resolvedContentType.startsWith("image/") ||
      resolvedContentType.startsWith("video/") ||
      resolvedContentType.startsWith("audio/");

    if (
      !shouldStreamTikTokMedia &&
      (isTikTokMediaRequest || refreshedTikTokUrls.length > 0)
    ) {
      return Response.redirect(resolvedUrl, 307);
    }

    mediaUrl = resolvedMediaUrl;
    hostname = mediaUrl.hostname.toLowerCase();
  }

  attemptedTikTokUrls.add(mediaUrl.toString());
  let upstream = await fetchUpstream(mediaUrl.toString());

  if (!upstream.ok) {
    const refreshSource = sourceUrlParam || mediaUrl.toString();

    const candidateUrls =
      refreshedTikTokUrls.length > 0
        ? refreshedTikTokUrls
        : await resolveRefreshedMediaUrls(refreshSource, mediaUrl.toString());

    for (const candidateUrl of candidateUrls) {
      if (attemptedTikTokUrls.has(candidateUrl)) continue;
      attemptedTikTokUrls.add(candidateUrl);
      mediaUrl = new URL(candidateUrl);
      hostname = mediaUrl.hostname.toLowerCase();
      upstream = await fetchUpstream(candidateUrl);
      if (upstream.ok) break;
    }

    let shouldBypassRefreshCache = isTikTokProxyRequest;
    if (!shouldBypassRefreshCache) {
      try {
        const refreshSourceUrl = new URL(refreshSource);
        shouldBypassRefreshCache =
          isInstagramSourceUrl(refreshSourceUrl) ||
          isXSourceUrl(refreshSourceUrl);
      } catch {
        shouldBypassRefreshCache = false;
      }
    }

    if (!upstream.ok && shouldBypassRefreshCache) {
      const forcedRefreshUrls = await resolveRefreshedMediaUrls(
        refreshSource,
        mediaUrl.toString(),
        {
          forceRefresh: true,
          bypassCache: true,
        },
      );

      for (const candidateUrl of forcedRefreshUrls) {
        if (attemptedTikTokUrls.has(candidateUrl)) continue;
        attemptedTikTokUrls.add(candidateUrl);
        mediaUrl = new URL(candidateUrl);
        hostname = mediaUrl.hostname.toLowerCase();
        upstream = await fetchUpstream(candidateUrl);
        if (upstream.ok) break;
      }
    }
  }

  if (
    (hostname.includes("tiktok") ||
      hostname.includes("cdninstagram.com") ||
      hostname === "video.twimg.com" ||
      hostname === "pbs.twimg.com") &&
    !upstream.ok
  ) {
    log.warn("External media upstream failure", {
      status: upstream.status,
      statusText: upstream.statusText,
      method: includeBody ? "GET" : "HEAD",
      range,
      source: summarizeUrlForLog(mediaUrl),
      refreshSource: sourceUrlParam ? summarizeUrlForLog(sourceUrlParam) : null,
      finalUrl: upstream.url ? summarizeUrlForLog(upstream.url) : null,
      contentType: upstream.headers.get("Content-Type"),
      contentLength: upstream.headers.get("Content-Length"),
    });
  }

  if (includeBody && range) {
    const syntheticRange = await makeSyntheticRangeResponse(
      upstream.clone(),
      range,
    );
    if (syntheticRange) return syntheticRange;
  }

  const headers = buildProxyHeaders(
    upstream.headers,
    upstream.url || mediaUrl.toString(),
  );
  if (isTikTokProxyRequest) {
    headers.set(
      "Cache-Control",
      getTikTokProxyCacheControl(
        upstream.url || mediaUrl.toString(),
        upstream.status,
        headers.get("Content-Type"),
      ),
    );
  } else if (range || headers.get("Content-Type")?.startsWith("video/")) {
    headers.set("Cache-Control", "no-store");
  }

  return new Response(includeBody ? upstream.body : null, {
    status: upstream.status,
    statusText: upstream.statusText,
    headers,
  });
}

type WaitUntil = (promise: Promise<unknown>) => void;

/** workerd exports waitUntil from cloudflare:workers; absent in tests. */
function getWaitUntil(): WaitUntil | undefined {
  const candidate = (cloudflareWorkers as { waitUntil?: unknown }).waitUntil;
  return typeof candidate === "function" ? (candidate as WaitUntil) : undefined;
}

function getDefaultCache(): Cache | null {
  try {
    return (
      (globalThis.caches as { default?: Cache } | undefined)?.default ?? null
    );
  } catch {
    return null;
  }
}

/** Short, so a signed-out or suspended session stops working quickly. */
export const BEARER_MEMO_TTL_MS = 30_000;
const BEARER_MEMO_MAX = 2_000;
const bearerMemo = new Map<string, { userId: string; until: number }>();

/**
 * Expo and the native apps send their session bearer to the API's own URLs.
 * A valid session is remembered per isolate for BEARER_MEMO_TTL_MS, keyed
 * by a hash of the token, so a screen of images costs one session lookup.
 * Returns the session's user, or null.
 */
async function bearerUserId(request: Request): Promise<string | null> {
  const authorization = request.headers.get("authorization");
  if (!authorization?.startsWith("Bearer ")) return null;
  const memoKey = nodeCrypto
    .createHash("sha256")
    .update(authorization)
    .digest("base64url");
  const now = Date.now();
  const memo = bearerMemo.get(memoKey);
  if (memo && memo.until > now) return memo.userId;
  bearerMemo.delete(memoKey);

  const result = await requireAuth(request);
  if (result instanceof Response) return null;
  if (bearerMemo.size >= BEARER_MEMO_MAX) {
    const oldest = bearerMemo.keys().next().value;
    if (oldest !== undefined) bearerMemo.delete(oldest);
  }
  bearerMemo.set(memoKey, {
    userId: result.userId,
    until: now + BEARER_MEMO_TTL_MS,
  });
  return result.userId;
}

export type GenericMediaAuthorization =
  | { via: "signature" }
  | { via: "pass" | "bearer"; userId: string }
  | null;

/** How (if at all) a generic-mode request is allowed to use the proxy. */
export async function authorizeGenericMediaRequest(
  request: Request,
  targetUrl: string,
  key: Buffer,
  checkBearer: (request: Request) => Promise<string | null> = bearerUserId,
): Promise<GenericMediaAuthorization> {
  const params = new URL(request.url).searchParams;
  const signature = params.get("sig");
  // A bad or expired signature (a rotated key) still lets a pass or
  // session through.
  if (
    signature &&
    verifyMediaUrlSignature(targetUrl, signature, key, params.get("exp"))
  ) {
    return { via: "signature" };
  }
  const pass = verifyMediaPass(
    readMediaPassCookie(request.headers.get("cookie")),
    key,
  );
  if (pass) return { via: "pass", userId: pass.userId };
  const userId = await checkBearer(request);
  return userId ? { via: "bearer", userId } : null;
}

/**
 * Per-user limit for URLs a client chose (pass or bearer), on top of the
 * per-IP one: global, through the RATE_LIMITER Durable Object. Fails open,
 * like the other users of the limiter.
 */
export const PROXY_MEDIA_USER_RATE_LIMIT = { limit: 600, windowMs: 60_000 };

/**
 * Signed requests carry no user, so they get a per-IP bucket of their own
 * (global, through the same Durable Object), counted only when the request
 * reaches upstream: edge cache hits cost nothing. Without it a signature
 * anyone can get (a link whose preview names the URL) would make the proxy
 * fetch for them limited only by the per-isolate IP limit.
 */
export const PROXY_MEDIA_SIGNED_IP_RATE_LIMIT = {
  limit: 600,
  windowMs: 60_000,
};

export type UserRateCheck = (userId: string) => Promise<Response | null>;
export type SignedIpRateCheck = (clientIp: string) => Promise<Response | null>;

const checkProxyUserRate: UserRateCheck = async (userId) => {
  try {
    return await checkRateLimitDO(
      userId,
      "proxy-media",
      PROXY_MEDIA_USER_RATE_LIMIT,
    );
  } catch {
    return null;
  }
};

const checkProxySignedIpRate: SignedIpRateCheck = async (clientIp) => {
  try {
    return await checkRateLimitDO(
      `ip:${clientIp}`,
      "proxy-media-signed",
      PROXY_MEDIA_SIGNED_IP_RATE_LIMIT,
    );
  } catch {
    return null;
  }
};

export interface ProxyMediaRateChecks {
  user?: UserRateCheck;
  signedIp?: SignedIpRateCheck;
}

function hostKey(url: URL): string {
  return `${url.hostname.replace(/\.+$/, "")}:${url.port}`;
}

/**
 * The route: social CDN links (X, TikTok, Instagram) keep the allowlisted
 * path with its link refresh; any other http(s) media goes through the
 * generic mode when the request is signed, carries the media pass or a
 * signed-in bearer (a `sourceUrl` is then ignored). Unsigned requests for the remaining allowlisted hosts
 * (KLIPY, Tenor, Google and Steam avatars) still work as before.
 */
export async function handleProxyMedia(
  request: Request,
  includeBody: boolean,
  nested = false,
  rateChecks: ProxyMediaRateChecks = {},
): Promise<Response> {
  const checkUserRate = rateChecks.user ?? checkProxyUserRate;
  const checkSignedIpRate = rateChecks.signedIp ?? checkProxySignedIpRate;
  const requestUrl = new URL(request.url);
  const target = requestUrl.searchParams.get("url");
  if (!target) return new Response("Missing media URL", { status: 400 });
  let targetUrl: URL;
  try {
    targetUrl = new URL(target);
  } catch {
    return new Response("Invalid media URL", { status: 400 });
  }

  // A proxy URL inside a proxy URL: clients deployed before the server
  // rewrote payloads wrap the delivered form again. Serve the inner one
  // (its own `url`, `sig` and `sourceUrl`), one level deep.
  if (!nested && targetUrl.pathname === "/api/proxy-media") {
    const inner = new URL(requestUrl);
    inner.search = targetUrl.search;
    return handleProxyMedia(
      new Request(inner, { method: request.method, headers: request.headers }),
      includeBody,
      true,
      rateChecks,
    );
  }

  if (isSocialRefreshMediaUrl(targetUrl)) {
    return proxyMedia(request, includeBody);
  }

  const key = mediaProxyKey(
    getEnv() as unknown as {
      MEDIA_PROXY_SECRET?: string;
      REALTIME_TICKET_SECRET?: string;
    },
  );
  const authorization = key
    ? await authorizeGenericMediaRequest(request, target, key)
    : null;

  if (!authorization) {
    if (isAllowedMediaUrl(targetUrl)) return proxyMedia(request, includeBody);
    return new Response("Media proxy request is not authorized", {
      status: 401,
      headers: { "Cache-Control": "no-store" },
    });
  }

  // Never proxy ourselves (a loop that would only cost requests).
  if (hostKey(targetUrl) === hostKey(requestUrl)) {
    return new Response("Unsupported media URL", { status: 400 });
  }

  // The default http(s) ports only, signed or not; redirect hops are held
  // to the same rule (allowedPorts below).
  if (!MEDIA_PORTS.has(targetUrl.port)) {
    return new Response("Unsupported media port", {
      status: 400,
      headers: { "Cache-Control": "no-store" },
    });
  }

  let beforeUpstream: (() => Promise<Response | null>) | undefined;
  if (authorization.via === "signature") {
    const clientIp = request.headers.get("CF-Connecting-IP") ?? "unknown";
    beforeUpstream = () => checkSignedIpRate(clientIp);
  } else {
    const limited = await checkUserRate(authorization.userId);
    if (limited) return limited;
  }

  return serveGenericMedia(request, targetUrl, includeBody, {
    resolve: dohResolver,
    ownHost: requestUrl.host,
    cache: getDefaultCache(),
    waitUntil: getWaitUntil(),
    allowedPorts: MEDIA_PORTS,
    beforeUpstream,
  });
}

const dohResolver = createDohResolver();

const GET = async ({ request }: { request: Request }) =>
  handleProxyMedia(request, true);
const HEAD = async ({ request }: { request: Request }) =>
  handleProxyMedia(request, false);

/**
 * POST: the media pass for a signed-in web session, as an HttpOnly cookie
 * scoped to this path (the web app's own <img> requests then carry it).
 * DELETE: clears it on sign-out.
 */
const POST = async ({ request }: { request: Request }) => {
  const auth = await requireAuth(request);
  if (auth instanceof Response) return auth;
  const key = mediaProxyKey(
    getEnv() as unknown as {
      MEDIA_PROXY_SECRET?: string;
      REALTIME_TICKET_SECRET?: string;
    },
  );
  if (!key) return apiError("Media proxy is not configured", 503);
  const pass = issueMediaPass(key, auth.userId);
  // `expiresIn` (seconds) lets the client time its renewal on its own
  // clock; `expiresAt` stays for clients that read it.
  const response = apiSuccess(
    { expiresAt: pass.expiresAt, expiresIn: MEDIA_PASS_TTL_SECONDS },
    200,
    request,
  );
  response.headers.set(
    "Set-Cookie",
    mediaPassCookie(pass.value, MEDIA_PASS_TTL_SECONDS, {
      crossSite: isDesktopOrigin(request.headers.get("origin")),
    }),
  );
  response.headers.set("Cache-Control", "no-store");
  return response;
};

const DELETE = async ({ request }: { request: Request }) => {
  const response = apiSuccess({ ok: true }, 200, request);
  response.headers.set(
    "Set-Cookie",
    mediaPassCookie("", 0, {
      crossSite: isDesktopOrigin(request.headers.get("origin")),
    }),
  );
  response.headers.set("Cache-Control", "no-store");
  return response;
};

export const Route = createFileRoute("/api/proxy-media")({
  server: {
    handlers: {
      GET,
      HEAD,
      POST,
      DELETE,
    },
  },
});
