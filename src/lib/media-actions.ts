import {
  isCapProtectedMediaPath,
  stripMediaCredentials,
} from "@/lib/media-capability";
import { getPublicApiUrl, getWebOrigin } from "@/lib/platform";

export interface MediaActionTarget {
  url: string;
  downloadUrl?: string;
  filename?: string;
}

/** Hosts that are Yap itself: the page, the web origin, the API. */
function isOwnHost(hostname: string): boolean {
  const own = new Set(["localhost"]);
  for (const origin of [
    typeof window !== "undefined" ? window.location.origin : "",
    getWebOrigin(),
    getPublicApiUrl(),
  ]) {
    try {
      if (origin) own.add(new URL(origin).hostname);
    } catch {
      // Not a URL: nothing to add.
    }
  }
  return own.has(hostname);
}

/**
 * A link that is safe to hand to someone else (clipboard, share sheet,
 * system browser). Yap's own private media becomes its canonical address on
 * the web origin, without the media capability or a session token: whoever
 * opens it needs their own sign-in. Anything else is returned as it is.
 */
export function getShareableUrl(url: string): string {
  const relative = !/^[a-z][a-z0-9+.-]*:/i.test(url);
  let parsed: URL;
  try {
    parsed = new URL(
      relative && !url.startsWith("/") ? `/${url}` : url,
      getWebOrigin() || "https://localhost",
    );
  } catch {
    return url;
  }
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return url;
  if (!relative && !isOwnHost(parsed.hostname)) return url;
  if (!isCapProtectedMediaPath(parsed.pathname)) {
    return relative
      ? `${getWebOrigin()}${parsed.pathname}${parsed.search}${parsed.hash}`
      : url;
  }

  stripMediaCredentials(parsed);
  return `${getWebOrigin()}${parsed.pathname}${parsed.search}`;
}

export function getPublicMediaUrl(url: string): string {
  return getShareableUrl(url);
}

function getMediaFilename(url: string, fallback = "media"): string {
  try {
    const pathname = new URL(url).pathname;
    const filename = pathname.split("/").pop()?.trim();
    if (filename) return decodeURIComponent(filename);
  } catch {
    // Use the fallback when the media URL is not parseable.
  }

  return fallback;
}

function isHttpUrl(value: string): boolean {
  try {
    const parsed = new URL(
      value,
      typeof window !== "undefined"
        ? window.location.href
        : "https://localhost",
    );
    return parsed.protocol === "http:" || parsed.protocol === "https:";
  } catch {
    return false;
  }
}

export async function saveMedia(target: MediaActionTarget): Promise<void> {
  const downloadUrl = target.downloadUrl || target.url;
  const filename = target.filename || getMediaFilename(target.url);
  if (!isHttpUrl(downloadUrl)) return;

  try {
    const response = await fetch(downloadUrl);
    if (!response.ok)
      throw new Error(`Media request failed: ${response.status}`);

    const objectUrl = URL.createObjectURL(await response.blob());
    const anchor = document.createElement("a");
    anchor.href = objectUrl;
    anchor.download = filename;
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
    window.setTimeout(() => URL.revokeObjectURL(objectUrl), 0);
    return;
  } catch {
    const anchor = document.createElement("a");
    anchor.href = downloadUrl;
    anchor.download = filename;
    anchor.target = "_blank";
    anchor.rel = "noopener noreferrer";
    document.body.appendChild(anchor);
    anchor.click();
    anchor.remove();
  }
}

export async function copyMediaLink(url: string): Promise<void> {
  await navigator.clipboard.writeText(getShareableUrl(url));
}
