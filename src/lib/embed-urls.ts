// ── embed-urls ──────────────────────────────────────────────────────────────
// Which URLs in a message get link embeds. Shared by the server (embed
// fetcher, send path) and the composer (prefetch), so both agree on exactly
// the same URLs. No imports: safe for client bundles.

/** Most embeds a single message gets. */
export const MAX_EMBEDS_PER_MESSAGE = 3;

const TRAILING_PUNCTUATION = /[.,;:!?'"*_~]+$/;

function trimUnbalancedClosers(url: string): string {
  let trimmed = url;
  const pairs: Array<[string, string]> = [
    ["(", ")"],
    ["[", "]"],
    ["{", "}"],
  ];
  for (let guard = 0; guard < 8; guard += 1) {
    const before = trimmed;
    trimmed = trimmed.replace(TRAILING_PUNCTUATION, "");
    for (const [open, close] of pairs) {
      if (!trimmed.endsWith(close)) continue;
      const opens = trimmed.split(open).length - 1;
      const closes = trimmed.split(close).length - 1;
      if (closes > opens) trimmed = trimmed.slice(0, -1);
    }
    if (trimmed === before) break;
  }
  return trimmed;
}

export interface EmbedUrlMatch {
  /** The URL as typed (stored embeds key on it). */
  url: string;
  /** Offset just past the URL in the source text. */
  end: number;
}

/** {@link extractEmbedUrls} with where each URL ends in `content`. */
export function matchEmbedUrls(
  content: string,
  limit = MAX_EMBEDS_PER_MESSAGE,
): EmbedUrlMatch[] {
  // Blank out code spans (same length) so offsets still line up.
  const blank = (match: string) => " ".repeat(match.length);
  const withoutCode = content
    .replace(/```[\s\S]*?```/g, blank)
    .replace(/`[^`\n]*`/g, blank);
  const matches: EmbedUrlMatch[] = [];
  const seen = new Set<string>();
  for (const match of withoutCode.matchAll(/https?:\/\/[^\s<>"'`]+/gi)) {
    const candidate = trimUnbalancedClosers(match[0]);
    let normalized: string;
    try {
      const parsed = new URL(candidate);
      if (!parsed.hostname.includes(".")) continue;
      normalized = parsed.toString();
    } catch {
      continue;
    }
    // Keep what the user typed, dedupe on the parsed form.
    if (seen.has(normalized)) continue;
    seen.add(normalized);
    matches.push({
      url: candidate,
      end: (match.index ?? 0) + candidate.length,
    });
    if (matches.length >= limit) break;
  }
  return matches;
}

/**
 * URLs in a message that should get embeds, in order. Mirrors how the client
 * linkifies (MarkdownRenderer): trailing punctuation and an unbalanced closing
 * bracket are not part of the URL (`(see https://a.b/c).` -> `https://a.b/c`),
 * and URLs inside inline code or code blocks are not linkified so they get no
 * embed either.
 */
export function extractEmbedUrls(
  content: string,
  limit = MAX_EMBEDS_PER_MESSAGE,
): string[] {
  return matchEmbedUrls(content, limit).map((match) => match.url);
}
