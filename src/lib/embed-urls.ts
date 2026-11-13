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

/** A URL written as `<https://...>`: linked, but asked for no embed. */
function isSuppressed(text: string, start: number, end: number): boolean {
  return text[start - 1] === "<" && text[end] === ">";
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
    const start = match.index ?? 0;
    if (isSuppressed(withoutCode, start, start + candidate.length)) continue;
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
    matches.push({ url: candidate, end: start + candidate.length });
    if (matches.length >= limit) break;
  }
  return matches;
}

/**
 * URLs in a message that should get embeds, in order. Mirrors how the client
 * linkifies (MarkdownRenderer): trailing punctuation and an unbalanced closing
 * bracket are not part of the URL (`(see https://a.b/c).` -> `https://a.b/c`),
 * and URLs inside inline code or code blocks are not linkified so they get no
 * embed either. A URL in angle brackets (`<https://a.b/c>`) asks for no embed.
 */
export function extractEmbedUrls(
  content: string,
  limit = MAX_EMBEDS_PER_MESSAGE,
): string[] {
  return matchEmbedUrls(content, limit).map((match) => match.url);
}

// ── Links not to fetch before they are sent ─────────────────────────────────

/**
 * Query (or fragment) keys that carry a credential: tokens, one-time codes,
 * signatures, keys, passwords. Matched as a whole key or a word inside one
 * (`access_token`, `X-Amz-Signature`, `api_key`).
 */
const SECRET_PARAM =
  /(?:^|[^a-z])(?:token|code|key|sig|signature|auth|magic|otp|password|passwd|pwd|pass|secret|credential|session|sid|ticket|nonce|hmac)s?(?:$|[^a-z])|token|signature|password|secret|apikey|authkey/i;

/** Path segments of sign-in, magic-link, verification and reset pages. */
const AUTH_PATH =
  /(?:^|\/)(?:magic[-_]?links?|magic|auth|oauth2?|sso|saml|login|log-?in|sign-?in|signin|callback|verify|verification|confirm|activate|reset[-_]?password|password[-_]?reset|reset|one[-_]?time)(?:$|[/._-])/i;

/** An opaque value this long is treated as a credential whatever its key. */
const OPAQUE_VALUE = /^[A-Za-z0-9_\-.~%+/=]{40,}$/;

function hasSecretParams(params: URLSearchParams): boolean {
  for (const [key, value] of params) {
    if (SECRET_PARAM.test(key) || OPAQUE_VALUE.test(value)) return true;
  }
  return false;
}

/**
 * Whether a link looks like it carries a credential or opens a sign-in step:
 * a magic link, a password reset, an OAuth callback, a signed URL. Pasting
 * one must not spend it, so the composer's prefetch skips these; sending the
 * message still embeds it as usual.
 */
export function looksPrivateLink(url: string): boolean {
  let parsed: URL;
  try {
    parsed = new URL(url);
  } catch {
    return false;
  }
  if (parsed.username || parsed.password) return true;
  if (AUTH_PATH.test(parsed.pathname)) return true;
  if (hasSecretParams(parsed.searchParams)) return true;
  const fragment = parsed.hash.slice(1);
  return (
    fragment.includes("=") && hasSecretParams(new URLSearchParams(fragment))
  );
}
