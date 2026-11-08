import { createFileRoute } from "@tanstack/react-router";
import { GAME_ICON_PATH_PATTERN } from "@ralph/protocol";

import { apiError, getBucket, getEnv } from "@/lib/api-helpers";
import { logger } from "@/lib/logger";

// GET /api/game-icons/{appId}/{iconHash}.png
// Serves catalog game icons — publicly accessible (no auth) so they can be
// displayed in <img> tags. Clients never load icons from Discord: on the first
// request for an icon this route fetches it from Discord's CDN server-side,
// keeps it in R2 and serves it from there from then on. Only icons of apps
// in our own catalog (public/desktop/game-catalog.json) are ever fetched, and
// a miss is remembered for a few minutes, so the route cannot be used to pull
// arbitrary Discord application icons into our storage.
const MAX_ICON_BYTES = 256 * 1024;
const UPSTREAM_TIMEOUT_MS = 5_000;
const UPSTREAM_ICON_SIZE = 64;
const ALLOWED_CONTENT_TYPES = new Set([
  "image/png",
  "image/webp",
  "image/jpeg",
]);

const CATALOG_PATH = "/desktop/game-catalog.json";
/** How long the catalog's icon set is reused before it is read again. */
const CATALOG_TTL_MS = 60 * 60 * 1000;
/** A failed catalog read is retried after this. */
const CATALOG_RETRY_MS = 30 * 1000;
/** How long an icon that is not in the catalog (or 404s upstream) is refused. */
const MISS_TTL_MS = 5 * 60 * 1000;
const MAX_MISSES = 5_000;

interface CatalogIcons {
  icons: Set<string> | null;
  expiresAt: number;
}

let catalogIcons: CatalogIcons | null = null;
let catalogLoad: Promise<Set<string> | null> | null = null;
const misses = new Map<string, number>();

/** Clears the catalog and miss caches (tests). */
export function resetGameIconCaches() {
  catalogIcons = null;
  catalogLoad = null;
  misses.clear();
}

interface StaticAssets {
  fetch(request: Request): Promise<Response>;
}

/** "{appId}/{iconHash}" for every catalog app that has an icon. */
async function readCatalogIcons(origin: string): Promise<Set<string> | null> {
  const url = new URL(CATALOG_PATH, origin);
  // The site's own static file (the ASSETS binding when one is configured,
  // else the same origin); never Discord.
  const assets = (getEnv() as unknown as { ASSETS?: StaticAssets }).ASSETS;
  try {
    const response = assets
      ? await assets.fetch(new Request(url))
      : await fetch(url, { signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS) });
    if (!response.ok) return null;
    const body = (await response.json()) as {
      apps?: Array<{ id?: unknown; icon_hash?: unknown }>;
    };
    const icons = new Set<string>();
    for (const app of body.apps ?? []) {
      if (typeof app.id === "string" && typeof app.icon_hash === "string") {
        icons.add(`${app.id}/${app.icon_hash}`);
      }
    }
    return icons;
  } catch (error) {
    logger.warn("game catalog read failed", {
      error: error instanceof Error ? error.message : String(error),
    });
    return null;
  }
}

/** The catalog's icon set, or null when it cannot be read right now. */
async function catalogIconSet(origin: string): Promise<Set<string> | null> {
  if (catalogIcons && catalogIcons.expiresAt > Date.now()) {
    return catalogIcons.icons;
  }
  catalogLoad ??= readCatalogIcons(origin).then((icons) => {
    catalogIcons = {
      icons,
      expiresAt: Date.now() + (icons ? CATALOG_TTL_MS : CATALOG_RETRY_MS),
    };
    catalogLoad = null;
    return icons;
  });
  return catalogLoad;
}

function isRecentMiss(iconId: string): boolean {
  const until = misses.get(iconId);
  if (until === undefined) return false;
  if (until > Date.now()) return true;
  misses.delete(iconId);
  return false;
}

function rememberMiss(iconId: string) {
  if (misses.size >= MAX_MISSES) misses.clear();
  misses.set(iconId, Date.now() + MISS_TTL_MS);
}

type UpstreamIcon =
  | { ok: true; body: ArrayBuffer; contentType: string }
  | { ok: false; status: 404 | 502 };

/** Reads at most `limit` bytes; null when the body is larger. */
async function readLimited(
  body: ReadableStream<Uint8Array> | null,
  limit: number,
): Promise<ArrayBuffer | null> {
  if (!body) return new ArrayBuffer(0);
  const reader = body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const merged = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return merged.buffer;
}

async function fetchUpstreamIcon(
  appId: string,
  iconHash: string,
): Promise<UpstreamIcon> {
  const url = `https://cdn.discordapp.com/app-icons/${appId}/${iconHash}.png?size=${UPSTREAM_ICON_SIZE}`;
  let response: Response;
  try {
    // A bare request: no cookies, no auth, nothing forwarded from the client.
    response = await fetch(url, {
      method: "GET",
      redirect: "error",
      signal: AbortSignal.timeout(UPSTREAM_TIMEOUT_MS),
    });
  } catch (error) {
    logger.warn("game icon upstream fetch failed", {
      appId,
      error: error instanceof Error ? error.message : String(error),
    });
    return { ok: false, status: 502 };
  }

  if (response.status === 404) {
    await response.body?.cancel();
    return { ok: false, status: 404 };
  }

  const contentType = (response.headers.get("content-type") ?? "")
    .split(";")[0]
    .trim()
    .toLowerCase();
  if (response.status !== 200 || !ALLOWED_CONTENT_TYPES.has(contentType)) {
    await response.body?.cancel();
    logger.warn("game icon upstream response rejected", {
      appId,
      status: response.status,
      contentType,
    });
    return { ok: false, status: 502 };
  }

  const declaredLength = Number(response.headers.get("content-length"));
  if (Number.isFinite(declaredLength) && declaredLength > MAX_ICON_BYTES) {
    await response.body?.cancel();
    return { ok: false, status: 502 };
  }

  let body: ArrayBuffer | null;
  try {
    body = await readLimited(response.body, MAX_ICON_BYTES);
  } catch {
    return { ok: false, status: 502 };
  }
  if (!body || body.byteLength === 0) return { ok: false, status: 502 };
  return { ok: true, body, contentType };
}

function iconResponse(body: BodyInit | null, contentType: string): Response {
  const headers = new Headers();
  headers.set("Content-Type", contentType);
  headers.set("Cache-Control", "public, max-age=31536000, immutable");
  // Security headers — defense-in-depth against MIME sniffing and embedded scripts
  headers.set("X-Content-Type-Options", "nosniff");
  headers.set(
    "Content-Security-Policy",
    "default-src 'none'; style-src 'unsafe-inline'",
  );
  headers.set("Content-Disposition", "inline");
  return new Response(body, { status: 200, headers });
}

export const GET = async ({
  request,
  params,
}: {
  request: Request;
  params?: { _splat?: string };
}) => {
  const splat = params?._splat ?? "";
  const path = `/api/game-icons/${splat}`;
  if (!GAME_ICON_PATH_PATTERN.test(path)) {
    return apiError("Icon not found", 404);
  }

  // /api/game-icons/{appId}/{iconHash}.png, validated above.
  const [appId, file] = splat.split("/");
  const iconHash = file.slice(0, -".png".length);
  const key = `game-icons/${appId}/${iconHash}.png`;

  const bucket = getBucket();
  const object = await bucket.get(key);
  if (object) {
    return iconResponse(
      object.body as ReadableStream,
      object.httpMetadata?.contentType || "image/png",
    );
  }

  const iconId = `${appId}/${iconHash}`;
  if (isRecentMiss(iconId)) return apiError("Icon not found", 404);
  const icons = await catalogIconSet(new URL(request.url).origin);
  if (!icons) return apiError("Icon unavailable", 502);
  if (!icons.has(iconId)) {
    rememberMiss(iconId);
    return apiError("Icon not found", 404);
  }

  const upstream = await fetchUpstreamIcon(appId, iconHash);
  if (!upstream.ok) {
    if (upstream.status === 404) {
      rememberMiss(iconId);
      return apiError("Icon not found", 404);
    }
    return apiError("Icon unavailable", 502);
  }

  try {
    await bucket.put(key, upstream.body, {
      httpMetadata: { contentType: upstream.contentType },
    });
  } catch (error) {
    // Still serve it; the next request retries the store.
    logger.warn("game icon store failed", {
      appId,
      error: error instanceof Error ? error.message : String(error),
    });
  }
  return iconResponse(upstream.body, upstream.contentType);
};

export const Route = createFileRoute("/api/game-icons/$")({
  server: {
    handlers: {
      GET,
    },
  },
});
