// ── embed-prefetch ──────────────────────────────────────────────────────────
// Warm the server's embed cache while a link is still in the composer, so the
// embed is ready (and rides in MESSAGE_CREATE) when the message is sent.
//
// A URL is prefetched when it is complete: pasted, or followed by whitespace
// (the user typed past it). A URL still being typed at the end of the field
// is left alone, so `https://youtu` never reaches the server. Each URL is
// asked for once per session. Failures are ignored: the send path resolves
// the embed as before. Links that look like they carry a credential (magic
// links, resets, signed URLs; looksPrivateLink) are never prefetched: fetching
// one could spend it before the user decides to send it.

import { apiPost } from "@/lib/api-client";
import { looksPrivateLink, matchEmbedUrls } from "@/lib/embed-urls";

/** How long typing must pause before a finished URL is prefetched. */
export const EMBED_PREFETCH_DEBOUNCE_MS = 350;
const MAX_REMEMBERED_URLS = 200;

const requested = new Set<string>();

/** Test hook. */
export function resetEmbedPrefetchState(): void {
  requested.clear();
}

/**
 * URLs in `text` worth prefetching: every embeddable URL when the text was
 * pasted, otherwise only those the user has typed past.
 */
export function findPrefetchableUrls(
  text: string,
  options: { pasted?: boolean } = {},
): string[] {
  return (
    matchEmbedUrls(text)
      // Typed past: whitespace somewhere after it (`(link). ` counts).
      .filter(({ end }) => options.pasted || /\s/.test(text.slice(end)))
      .map(({ url }) => url)
      .filter((url) => !looksPrivateLink(url))
  );
}

type Post = (url: string, body: { urls: string[] }) => Promise<void>;

/**
 * Ask the server to resolve `urls` that were not asked for before. Resolves
 * with the URLs actually sent (for tests); never rejects.
 */
export async function prefetchEmbeds(
  urls: readonly string[],
  post: Post = async (url, body) => {
    await apiPost(url, body);
  },
): Promise<string[]> {
  const fresh = urls.filter((url) => !requested.has(url));
  if (fresh.length === 0) return [];
  for (const url of fresh) {
    if (requested.size >= MAX_REMEMBERED_URLS) {
      const oldest = requested.values().next().value;
      if (oldest !== undefined) requested.delete(oldest);
    }
    requested.add(url);
  }
  try {
    await post("/api/embeds/prefetch", { urls: fresh });
  } catch {
    // Best effort: the send path resolves the embed anyway.
  }
  return fresh;
}
