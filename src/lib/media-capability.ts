/**
 * Media capabilities (`?cap=m1.…`): short-lived, per-object proof that a
 * user may read one private media URL, for requests that cannot carry an
 * Authorization header (`<img>`, `<video>`, `<audio>`, download links).
 *
 * The server gives a signed-in client a grant key for (user, expiry):
 * `grantKey = HMAC(secret, user, expiry)`. The client signs each URL with
 * it, synchronously while rendering: `sig = HMAC(grantKey, path + query)`.
 * The server re-derives the grant key from its secret and checks the
 * signature, so a cap names exactly one object (its path and query, minus
 * the cap itself), one user and one expiry, and a stolen cap is worth one
 * object for a few hours, never the session.
 *
 * The session token never goes in a URL: URLs end up in logs, the
 * clipboard and other people's chats.
 */
import { hmac } from "@noble/hashes/hmac.js";
import { sha256 } from "@noble/hashes/sha2.js";

export const MEDIA_CAP_PARAM = "cap";
const MEDIA_CAP_VERSION = "m1";
const GRANT_KEY_DOMAIN = "yap/media-cap/v1/grant";

/** How long a grant (and every cap signed with it) lives. */
export const MEDIA_GRANT_TTL_MS = 4 * 60 * 60 * 1000;
/**
 * Grant expiries are rounded up to this step, so every grant issued in the
 * same hour has the same key and the same cap URLs: the browser cache keeps
 * working across refreshes.
 */
export const MEDIA_GRANT_STEP_MS = 60 * 60 * 1000;
/**
 * Clients refresh a grant with less than this left, so a URL they sign is
 * good for at least this long (a lazy `<img>` that loads late, a video seek).
 */
export const MEDIA_GRANT_REFRESH_MARGIN_MS = 2 * 60 * 60 * 1000;
/** A cap claiming an expiry further out than this was not issued by us. */
const MAX_CAP_LIFETIME_MS = MEDIA_GRANT_TTL_MS + MEDIA_GRANT_STEP_MS;

/**
 * The private media routes: they answer a session (Authorization header)
 * or a media capability, never the session token in the query string.
 */
export const CAP_PROTECTED_MEDIA_PREFIXES = [
  "/api/attachments/",
  "/api/soundboard/uploads/",
  "/api/camera-backgrounds/",
  "/api/listen-together/stream",
  "/api/voice-status-media/",
  "/api/emojis/assets/",
] as const;

export function isCapProtectedMediaPath(pathname: string): boolean {
  return CAP_PROTECTED_MEDIA_PREFIXES.some((prefix) =>
    pathname.startsWith(prefix),
  );
}

/** What a client needs to sign caps: the grant key and what it is bound to. */
export interface MediaGrant {
  readonly userId: string;
  /** Epoch milliseconds. */
  readonly expiresAt: number;
  /** base64url. */
  readonly key: string;
}

export type MediaCapVerification =
  | { ok: true; userId: string; expiresAt: number }
  | { ok: false; reason: "invalid" | "expired" | "signature" };

const encoder = new TextEncoder();

/** The grant's expiry for one issued at `now`: TTL ahead, rounded up to the step. */
export function mediaGrantExpiry(now: number): number {
  return (
    Math.ceil((now + MEDIA_GRANT_TTL_MS) / MEDIA_GRANT_STEP_MS) *
    MEDIA_GRANT_STEP_MS
  );
}

/** The grant key for (user, expiry). Server only: it needs the secret. */
export function deriveMediaGrantKey(
  secret: string,
  userId: string,
  expiresAt: number,
): Uint8Array {
  if (!secret) throw new Error("Media capability secret is not configured");
  return hmac(
    sha256,
    encoder.encode(`${GRANT_KEY_DOMAIN}\0${secret}`),
    encoder.encode(`${userId}\0${expiresAt}`),
  );
}

export function issueMediaGrant(
  secret: string,
  userId: string,
  now: number,
): MediaGrant & { readonly issuedAt: number } {
  const expiresAt = mediaGrantExpiry(now);
  return {
    userId,
    expiresAt,
    // The server's clock, so a client can count the lifetime on its own.
    issuedAt: now,
    key: base64UrlEncode(deriveMediaGrantKey(secret, userId, expiresAt)),
  };
}

/**
 * What a cap signs: the URL's path and its query without any `cap` or
 * `token` parameter, in the order and encoding the URL has them. Both ends
 * compute it from a parsed URL, so they agree on percent-encoding.
 */
export function mediaCapSubject(url: URL): string {
  const query = url.search
    .replace(/^\?/, "")
    .split("&")
    .filter((part) => part !== "" && !isCredentialParam(part))
    .join("&");
  return query ? `${url.pathname}?${query}` : url.pathname;
}

function isCredentialParam(part: string): boolean {
  const name = part.split("=", 1)[0];
  return name === MEDIA_CAP_PARAM || name === "token";
}

/** The cap for `url` under `grant`. Pure and synchronous: safe in render. */
export function signMediaCap(grant: MediaGrant, url: URL): string | null {
  const key = base64UrlDecode(grant.key);
  if (!key) return null;
  const signature = hmac(sha256, key, encoder.encode(mediaCapSubject(url)));
  return [
    MEDIA_CAP_VERSION,
    base64UrlEncode(encoder.encode(grant.userId)),
    String(grant.expiresAt),
    base64UrlEncode(signature),
  ].join(".");
}

/**
 * `url` with a cap for it (any old `cap`/`token` dropped). The cap goes
 * last, so anything appended later breaks the signature rather than
 * slipping past it.
 */
export function withMediaCap(grant: MediaGrant, url: URL): URL {
  const signed = new URL(url.toString());
  stripMediaCredentials(signed);
  const cap = signMediaCap(grant, signed);
  if (cap) signed.searchParams.append(MEDIA_CAP_PARAM, cap);
  return signed;
}

/** Whether `cap` looks like a media capability (not a soundboard one). */
export function isMediaCap(cap: string | null): cap is string {
  return !!cap && cap.startsWith(`${MEDIA_CAP_VERSION}.`);
}

export function verifyMediaCap(
  cap: string,
  url: URL,
  secret: string,
  now: number,
): MediaCapVerification {
  if (!secret) return { ok: false, reason: "invalid" };
  const parts = cap.split(".");
  if (parts.length !== 4 || parts[0] !== MEDIA_CAP_VERSION) {
    return { ok: false, reason: "invalid" };
  }
  const [, encodedUser, expiresRaw, encodedSignature] = parts;
  const userBytes = base64UrlDecode(encodedUser);
  const signature = base64UrlDecode(encodedSignature);
  if (!/^\d{1,16}$/.test(expiresRaw) || !userBytes || !signature) {
    return { ok: false, reason: "invalid" };
  }
  const userId = new TextDecoder().decode(userBytes);
  const expiresAt = Number(expiresRaw);
  if (!userId || userId.length > 256 || !Number.isSafeInteger(expiresAt)) {
    return { ok: false, reason: "invalid" };
  }
  if (now >= expiresAt) return { ok: false, reason: "expired" };
  if (expiresAt - now > MAX_CAP_LIFETIME_MS) {
    return { ok: false, reason: "invalid" };
  }

  const key = deriveMediaGrantKey(secret, userId, expiresAt);
  const expected = hmac(sha256, key, encoder.encode(mediaCapSubject(url)));
  if (!constantTimeEqual(expected, signature)) {
    return { ok: false, reason: "signature" };
  }
  return { ok: true, userId, expiresAt };
}

/** Drops `cap` and `token` from `url`'s query, in place. */
export function stripMediaCredentials(url: URL): void {
  url.searchParams.delete(MEDIA_CAP_PARAM);
  url.searchParams.delete("token");
}

function constantTimeEqual(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i += 1) diff |= a[i] ^ b[i];
  return diff === 0;
}

function base64UrlEncode(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary)
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function base64UrlDecode(value: string): Uint8Array | null {
  if (!value || !/^[A-Za-z0-9_-]+$/.test(value)) return null;
  try {
    const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
    const padded = normalized.padEnd(Math.ceil(normalized.length / 4) * 4, "=");
    const binary = atob(padded);
    const bytes = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
    return bytes;
  } catch {
    return null;
  }
}
