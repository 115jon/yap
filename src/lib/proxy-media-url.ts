import { mapEmbedMediaUrls } from "@/lib/embed-media-urls";
import { apiUrl } from "@/lib/platform";
import { rememberSignedMediaPath } from "@/lib/signed-media-paths";
import type { EmbedInfo } from "@/lib/types";

export const PROXY_MEDIA_PATH = "/api/proxy-media";

export function buildProxyMediaPath(
  rawUrl: string,
  sourceUrl?: string | null,
): string {
  // Already one of ours (a server-built payload): keep it, never nest.
  const existing = proxyMediaPathOf(rawUrl);
  if (existing) return existing;
  const params = new URLSearchParams({ url: rawUrl });
  if (sourceUrl) {
    params.set("sourceUrl", sourceUrl);
  }

  return `${PROXY_MEDIA_PATH}?${params.toString()}`;
}

export function buildProxyMediaUrl(
  rawUrl: string,
  sourceUrl?: string | null,
): string {
  return apiUrl(buildProxyMediaPath(rawUrl, sourceUrl));
}

function parseMaybeRelative(url: string): URL | null {
  try {
    return new URL(
      url,
      typeof window !== "undefined"
        ? window.location.origin
        : "https://localhost",
    );
  } catch {
    return null;
  }
}

function isProxyMediaPathname(pathname: string): boolean {
  return pathname === PROXY_MEDIA_PATH || pathname.endsWith(PROXY_MEDIA_PATH);
}

/** True for one of our /api/proxy-media URLs (any host, or a path). */
export function isProxyMediaUrl(url: string): boolean {
  if (!url.includes(PROXY_MEDIA_PATH)) return false;
  const parsed = parseMaybeRelative(url);
  return !!parsed && isProxyMediaPathname(parsed.pathname);
}

/** `/api/proxy-media?...` for a proxy URL on any host (dev aliases too). */
export function proxyMediaPathOf(url: string): string | null {
  if (typeof url !== "string" || !url.includes(PROXY_MEDIA_PATH)) return null;
  const parsed = parseMaybeRelative(url);
  if (!parsed || !isProxyMediaPathname(parsed.pathname)) return null;
  return `${PROXY_MEDIA_PATH}${parsed.search}`;
}

export function unwrapProxyMediaUrl(url: string): string {
  if (typeof url !== "string" || !url.includes(PROXY_MEDIA_PATH)) return url;
  const parsed = parseMaybeRelative(url);
  if (!parsed || !isProxyMediaPathname(parsed.pathname)) return url;
  const proxied = parsed.searchParams.get("url");
  if (!proxied) return url;
  if (parsed.searchParams.has("sig")) {
    rememberSignedMediaPath(proxied, `${PROXY_MEDIA_PATH}${parsed.search}`);
  }
  return proxied;
}

/**
 * An embed with its media URLs back to the originals, as the server stored
 * them: the renderers detect providers by host and build the social
 * `url` + `sourceUrl` proxy form themselves. Signed URLs are remembered, so
 * loading an original still uses the server's signature.
 */
export function unwrapEmbedMediaUrls(embed: EmbedInfo): EmbedInfo {
  return mapEmbedMediaUrls(embed, (url) => unwrapProxyMediaUrl(url));
}
