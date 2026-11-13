// /api/proxy-media's generic mode: any public image, video or audio file,
// fetched with no viewer data (no cookies, no client headers, a fixed
// User-Agent), checked hop by hop against the SSRF guard, held to a content
// type allowlist and a size cap, streamed (never buffered), and kept in the
// Cloudflare cache. Callers authorize the request first (route file).

import { checkMediaTarget, type ResolveHost } from "./ssrf";

export type MediaKind = "image" | "video" | "audio";

export const MEDIA_PROXY_LIMITS = {
  /** Largest image (GIFs included) the proxy passes. */
  imageBytes: 25 * 1024 * 1024,
  /** Largest video or audio file, counted on the whole file for ranges. */
  videoBytes: 100 * 1024 * 1024,
  /** Time allowed for each hop's response headers. */
  headersTimeoutMs: 10_000,
  maxRedirects: 3,
} as const;

/** Edge and browser cache lifetime when upstream says nothing usable. */
const DEFAULT_TTL_SECONDS = 24 * 60 * 60;
const MIN_TTL_SECONDS = 60 * 60;
const MAX_TTL_SECONDS = 7 * 24 * 60 * 60;
const CACHE_KEY_PATH = "/__yap-media-cache/v1";
/** Largest file put in the edge cache (GIFs and pictures, short clips). */
const CACHE_MAX_BYTES = 8 * 1024 * 1024;

const USER_AGENT = "Mozilla/5.0 (compatible; YapMediaProxy/1.0)";
const UPSTREAM_ACCEPT =
  "image/avif,image/webp,image/apng,image/*,video/*,audio/*;q=0.9,*/*;q=0.5";

const ALLOWED_TYPES = new Map<string, { type: string; kind: MediaKind }>([
  ["image/jpeg", { type: "image/jpeg", kind: "image" }],
  ["image/jpg", { type: "image/jpeg", kind: "image" }],
  ["image/pjpeg", { type: "image/jpeg", kind: "image" }],
  ["image/png", { type: "image/png", kind: "image" }],
  ["image/apng", { type: "image/apng", kind: "image" }],
  ["image/gif", { type: "image/gif", kind: "image" }],
  ["image/webp", { type: "image/webp", kind: "image" }],
  ["image/avif", { type: "image/avif", kind: "image" }],
  ["image/bmp", { type: "image/bmp", kind: "image" }],
  ["image/x-icon", { type: "image/x-icon", kind: "image" }],
  ["image/vnd.microsoft.icon", { type: "image/x-icon", kind: "image" }],
  ["video/mp4", { type: "video/mp4", kind: "video" }],
  ["video/webm", { type: "video/webm", kind: "video" }],
  ["video/quicktime", { type: "video/quicktime", kind: "video" }],
  ["audio/mpeg", { type: "audio/mpeg", kind: "audio" }],
  ["audio/mp3", { type: "audio/mpeg", kind: "audio" }],
  ["audio/mp4", { type: "audio/mp4", kind: "audio" }],
  ["audio/x-m4a", { type: "audio/mp4", kind: "audio" }],
  ["audio/aac", { type: "audio/aac", kind: "audio" }],
  ["audio/ogg", { type: "audio/ogg", kind: "audio" }],
  ["audio/wav", { type: "audio/wav", kind: "audio" }],
  ["audio/x-wav", { type: "audio/wav", kind: "audio" }],
  ["audio/webm", { type: "audio/webm", kind: "audio" }],
]);

/** Types that say nothing; the first bytes decide. */
const UNTYPED = new Set([
  "",
  "application/octet-stream",
  "binary/octet-stream",
]);

/**
 * The allowed type for a Content-Type header, or null. SVG is not on the
 * list: served from our origin it could run script when opened directly.
 */
export function allowedMediaType(
  contentType: string | null,
): { type: string; kind: MediaKind } | null {
  const normalized = contentType?.split(";")[0].trim().toLowerCase() ?? "";
  return ALLOWED_TYPES.get(normalized) ?? null;
}

function ascii(bytes: Uint8Array, start: number, text: string): boolean {
  if (bytes.length < start + text.length) return false;
  for (let i = 0; i < text.length; i += 1) {
    if (bytes[start + i] !== text.charCodeAt(i)) return false;
  }
  return true;
}

/** The media type from a file's first bytes, for untyped responses. */
export function sniffMediaType(bytes: Uint8Array): string | null {
  if (bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    return "image/jpeg";
  }
  if (ascii(bytes, 0, "\x89PNG")) return "image/png";
  if (ascii(bytes, 0, "GIF87a") || ascii(bytes, 0, "GIF89a")) {
    return "image/gif";
  }
  if (ascii(bytes, 0, "RIFF") && ascii(bytes, 8, "WEBP")) return "image/webp";
  if (ascii(bytes, 0, "RIFF") && ascii(bytes, 8, "WAVE")) return "audio/wav";
  if (ascii(bytes, 4, "ftyp")) {
    if (ascii(bytes, 8, "avif") || ascii(bytes, 8, "avis")) return "image/avif";
    if (ascii(bytes, 8, "qt  ")) return "video/quicktime";
    if (ascii(bytes, 8, "M4A ")) return "audio/mp4";
    return "video/mp4";
  }
  if (
    bytes[0] === 0x1a &&
    bytes[1] === 0x45 &&
    bytes[2] === 0xdf &&
    bytes[3] === 0xa3
  ) {
    return "video/webm";
  }
  if (ascii(bytes, 0, "ID3")) return "audio/mpeg";
  if (ascii(bytes, 0, "OggS")) return "audio/ogg";
  if (ascii(bytes, 0, "BM")) return "image/bmp";
  if (bytes[0] === 0 && bytes[1] === 0 && bytes[2] === 1 && bytes[3] === 0) {
    return "image/x-icon";
  }
  return null;
}

export function sizeCapFor(kind: MediaKind): number {
  return kind === "image"
    ? MEDIA_PROXY_LIMITS.imageBytes
    : MEDIA_PROXY_LIMITS.videoBytes;
}

/** The whole file's size from Content-Range ("bytes 0-99/1234"), if known. */
function totalFromContentRange(value: string | null): number | null {
  const total = value?.match(/\/(\d+)\s*$/)?.[1];
  return total ? Number(total) : null;
}

/** Edge/browser lifetime: upstream's max-age clamped, else a day. */
export function cacheTtlSeconds(upstreamCacheControl: string | null): number {
  const maxAge = upstreamCacheControl?.match(/(?:^|[,\s])max-age=(\d+)/i)?.[1];
  if (!maxAge) return DEFAULT_TTL_SECONDS;
  return Math.min(MAX_TTL_SECONDS, Math.max(MIN_TTL_SECONDS, Number(maxAge)));
}

/** A stream that errors once more than `limit` bytes went through. */
function capStream(limit: number): TransformStream<Uint8Array, Uint8Array> {
  let seen = 0;
  return new TransformStream({
    transform(chunk, controller) {
      seen += chunk.byteLength;
      if (seen > limit) {
        controller.error(new Error("Media exceeds the size cap"));
        return;
      }
      controller.enqueue(chunk);
    },
  });
}

/** Reads the first chunk so an untyped body can be sniffed, then replays it. */
async function peekBody(
  body: ReadableStream<Uint8Array>,
): Promise<{ head: Uint8Array; body: ReadableStream<Uint8Array> }> {
  const reader = body.getReader();
  const first = await reader.read();
  const head = first.value ?? new Uint8Array();
  const replay = new ReadableStream<Uint8Array>({
    start(controller) {
      if (head.byteLength > 0) controller.enqueue(head);
      if (first.done) controller.close();
    },
    async pull(controller) {
      const next = await reader.read();
      if (next.done) controller.close();
      else controller.enqueue(next.value);
    },
    cancel(reason) {
      return reader.cancel(reason);
    },
  });
  return { head, body: replay };
}

export interface GenericProxyDeps {
  resolve: ResolveHost;
  fetchImpl?: typeof fetch;
  /** caches.default in workerd; null skips the edge cache (tests, dev). */
  cache?: Cache | null;
  waitUntil?: (promise: Promise<unknown>) => void;
  /** This Worker's host: never fetched (no loops through ourselves). */
  ownHost?: string;
}

export type UpstreamResult =
  | { ok: true; response: Response; url: URL }
  | { ok: false; status: number; message: string };

/**
 * True for a URL that would loop back into this Worker: our own host (any
 * case, with or without the root dot) or a proxy path on any host
 * (trailing slashes and case ignored). Our own zones are refused by the
 * SSRF guard.
 */
export function isSelfTarget(url: URL, ownHost?: string): boolean {
  const path = url.pathname.replace(/\/+$/, "").toLowerCase();
  if (path === "/api/proxy-media") return true;
  if (!ownHost) return false;
  let own: URL;
  try {
    own = new URL(`https://${ownHost}`);
  } catch {
    return false;
  }
  const name = (value: URL) => value.hostname.replace(/\.+$/, "");
  return name(url) === name(own);
}

/**
 * Fetches `target` with redirects followed by hand (at most
 * `maxRedirects`), every hop re-checked against the SSRF guard. Only our own
 * headers go upstream: no cookies, no client headers, no Referer.
 */
export async function fetchPublicMedia(
  target: URL,
  options: { method: "GET" | "HEAD"; range?: string | null },
  deps: GenericProxyDeps,
): Promise<UpstreamResult> {
  const fetchImpl = deps.fetchImpl ?? fetch;
  let current = target;

  for (let hop = 0; hop <= MEDIA_PROXY_LIMITS.maxRedirects; hop += 1) {
    if (isSelfTarget(current, deps.ownHost)) {
      return { ok: false, status: 403, message: "Blocked media URL (self)" };
    }
    const check = await checkMediaTarget(current, deps.resolve);
    if (!check.ok) {
      return {
        ok: false,
        status: 403,
        message: `Blocked media URL (${check.reason})`,
      };
    }

    const headers = new Headers({
      "User-Agent": USER_AGENT,
      Accept: UPSTREAM_ACCEPT,
    });
    if (options.range) headers.set("Range", options.range);

    // The timer covers the headers only; the body may stream for longer.
    const controller = new AbortController();
    const timer = setTimeout(
      () => controller.abort(),
      MEDIA_PROXY_LIMITS.headersTimeoutMs,
    );
    let response: Response;
    try {
      response = await fetchImpl(check.url.toString(), {
        method: options.method,
        headers,
        redirect: "manual",
        signal: controller.signal,
      });
    } catch {
      return controller.signal.aborted
        ? { ok: false, status: 504, message: "Upstream timed out" }
        : { ok: false, status: 502, message: "Upstream fetch failed" };
    } finally {
      clearTimeout(timer);
    }

    if (![301, 302, 303, 307, 308].includes(response.status)) {
      return { ok: true, response, url: check.url };
    }

    await response.body?.cancel().catch(() => {});
    const location = response.headers.get("Location");
    if (!location) {
      return { ok: false, status: 502, message: "Redirect without Location" };
    }
    if (hop === MEDIA_PROXY_LIMITS.maxRedirects) break;
    try {
      current = new URL(location, check.url);
    } catch {
      return { ok: false, status: 502, message: "Invalid upstream redirect" };
    }
  }

  return { ok: false, status: 508, message: "Too many upstream redirects" };
}

function textResponse(status: number, message: string): Response {
  return new Response(message, {
    status,
    headers: {
      "Content-Type": "text/plain; charset=utf-8",
      "Cache-Control": "no-store",
      "X-Content-Type-Options": "nosniff",
    },
  });
}

function responseHeaders(
  upstream: Headers,
  contentType: string,
  ttlSeconds: number,
): Headers {
  const headers = new Headers();
  for (const name of [
    "Content-Length",
    "Content-Range",
    "Accept-Ranges",
    "ETag",
    "Last-Modified",
  ]) {
    const value = upstream.get(name);
    if (value) headers.set(name, value);
  }
  headers.set("Content-Type", contentType);
  headers.set("Cache-Control", `public, max-age=${ttlSeconds}`);
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set("Content-Security-Policy", "default-src 'none'; sandbox");
  headers.set("Content-Disposition", "inline");
  headers.set("Cross-Origin-Resource-Policy", "cross-origin");
  headers.set("Access-Control-Allow-Origin", "*");
  headers.set("Referrer-Policy", "no-referrer");
  return headers;
}

/** The Cache API key for a target: per URL, never per viewer. */
export function mediaCacheKey(origin: string, target: string): string {
  return `${origin}${CACHE_KEY_PATH}?u=${encodeURIComponent(target)}`;
}

/**
 * Serves `target` (already authorized): edge cache first, then upstream.
 * Full GETs that fit the cap are stored; ranges are served from a stored
 * copy when there is one and passed through otherwise.
 */
export async function serveGenericMedia(
  request: Request,
  target: URL,
  includeBody: boolean,
  deps: GenericProxyDeps,
): Promise<Response> {
  const range = request.headers.get("Range");
  const cacheKey = mediaCacheKey(new URL(request.url).origin, target.href);
  const cache = deps.cache ?? null;

  if (cache && includeBody) {
    try {
      const cached = await cache.match(
        new Request(cacheKey, range ? { headers: { Range: range } } : {}),
      );
      if (cached) return cached;
    } catch {
      // A cache failure is a miss.
    }
  }

  const upstreamResult = await fetchPublicMedia(
    target,
    { method: includeBody ? "GET" : "HEAD", range },
    deps,
  );
  if (!upstreamResult.ok) {
    return textResponse(upstreamResult.status, upstreamResult.message);
  }
  const upstream = upstreamResult.response;

  if (upstream.status === 416) {
    await upstream.body?.cancel().catch(() => {});
    const headers = new Headers({ "Cache-Control": "no-store" });
    const contentRange = upstream.headers.get("Content-Range");
    if (contentRange) headers.set("Content-Range", contentRange);
    return new Response(null, { status: 416, headers });
  }
  if (upstream.status !== 200 && upstream.status !== 206) {
    await upstream.body?.cancel().catch(() => {});
    return textResponse(
      upstream.status === 404 || upstream.status === 410 ? 404 : 502,
      `Upstream answered ${upstream.status}`,
    );
  }

  let body: ReadableStream<Uint8Array> | null = upstream.body;
  let allowed = allowedMediaType(upstream.headers.get("Content-Type"));
  const declared =
    upstream.headers.get("Content-Type")?.split(";")[0].trim().toLowerCase() ??
    "";
  if (!allowed && UNTYPED.has(declared) && body) {
    const peeked = await peekBody(body);
    body = peeked.body;
    allowed = allowedMediaType(sniffMediaType(peeked.head));
  }
  if (!allowed) {
    await body?.cancel().catch(() => {});
    return textResponse(415, "Unsupported media type");
  }

  const cap = sizeCapFor(allowed.kind);
  const declaredLength = Number(upstream.headers.get("Content-Length"));
  const wholeSize =
    totalFromContentRange(upstream.headers.get("Content-Range")) ??
    (upstream.status === 200 && Number.isFinite(declaredLength)
      ? declaredLength
      : null);
  if (wholeSize !== null && wholeSize > cap) {
    await body?.cancel().catch(() => {});
    return textResponse(413, "Media is too large");
  }

  const ttl = cacheTtlSeconds(upstream.headers.get("Cache-Control"));
  const headers = responseHeaders(upstream.headers, allowed.type, ttl);
  if (!includeBody || !body) {
    await body?.cancel().catch(() => {});
    return new Response(null, { status: upstream.status, headers });
  }

  // A declared length within the cap passes the runtime's own stream
  // through untouched (the length goes out too, and the runtime refuses a
  // body longer than it declared); without one the bytes are counted.
  const lengthKnown =
    Number.isFinite(declaredLength) &&
    declaredLength >= 0 &&
    upstream.headers.has("Content-Length") &&
    declaredLength <= cap;
  const out = lengthKnown ? body : body.pipeThrough(capStream(cap));
  if (!lengthKnown) headers.delete("Content-Length");

  // Only small, complete files go to the edge cache: tee() buffers the
  // cache's branch ahead of a slow viewer, so it is held to a few MB.
  const storable =
    cache &&
    deps.waitUntil &&
    !range &&
    upstream.status === 200 &&
    lengthKnown &&
    declaredLength <= CACHE_MAX_BYTES;
  if (storable) {
    const [toClient, toCache] = out.tee();
    deps.waitUntil?.(
      cache
        .put(
          cacheKey,
          new Response(toCache, { status: 200, headers: new Headers(headers) }),
        )
        .catch(() => {}),
    );
    return new Response(toClient, { status: 200, headers });
  }

  return new Response(out, { status: upstream.status, headers });
}
