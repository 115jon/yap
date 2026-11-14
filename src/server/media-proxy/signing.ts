// Signatures for /api/proxy-media's generic mode.
//
// The proxy fetches any public http(s) image or video, so it must not be an
// open proxy. A request is let through when it carries one of:
//   - `sig` with `exp`: an HMAC over the exact target URL and its expiry
//     (seconds since the epoch), minted by this server when it builds a
//     payload. Every signature expires, so a URL anyone got the server to
//     sign (a link whose preview names it) stops working on its own:
//       - URLs the server read from data it fetched itself (link embeds, GIF
//         provider results, Listen Together search and resolve artwork,
//         radio station favicons, stored avatars) last a week, rounded up
//         to the day, so a URL stays the same (and cacheable) for a day.
//         Payloads are rewritten when they leave the server, so clients
//         always get a fresh one.
//       - URLs a client handed in, such as a Listen Together track's
//         artwork, last a day, rounded up to the hour.
//   - the media pass cookie (POST /api/proxy-media): a signed value bound to
//     the user and valid for an hour, which the web app renews (a session
//     check) while signed in, so URLs it builds itself need no sig.
//   - a session bearer (the native apps send it to the API's own URLs).
// A pass or bearer whose account waits for deletion is refused on every
// request (authorizeGenericMediaRequest in routes/api/proxy-media.ts), as
// requireAuth does: verifyMediaPass below only proves who the pass is for.
//
// node:crypto (nodejs_compat) keeps signing synchronous, so payload rewriting
// can run where messages are serialized without threading async through.

// A namespace import, for the same reason as origin.ts (server only).
import * as nodeCrypto from "node:crypto";

const EXPIRING_URL_SIGNATURE_DOMAIN = "yap/media-proxy/url-exp/v1";
const PASS_DOMAIN = "yap/media-proxy/pass/v2";
const KEY_DOMAIN = "yap/media-proxy/key/v1";
/** 128 bits of HMAC-SHA256, base64url: 22 characters. */
const SIGNATURE_BYTES = 16;
const PASS_VERSION = "mp2";

export const MEDIA_PASS_COOKIE = "yap_media_pass";
/** An hour: renewing it checks the session, so sign-out ends it soon. */
export const MEDIA_PASS_TTL_SECONDS = 60 * 60;
/** How long a signature over a URL the server read itself lasts. */
export const SERVER_URL_SIGNATURE_TTL_SECONDS = 7 * 24 * 60 * 60;
/** Server-read signatures expire on a day boundary (cacheable URLs). */
export const SERVER_URL_SIGNATURE_ROUND_SECONDS = 24 * 60 * 60;
/** How long a signature over a client-supplied URL lasts. */
export const CLIENT_URL_SIGNATURE_TTL_SECONDS = 24 * 60 * 60;
/** Client-supplied signatures expire on an hour boundary. */
export const CLIENT_URL_SIGNATURE_ROUND_SECONDS = 60 * 60;

/**
 * The `exp` for a signature made at `nowMs` that lasts at least
 * `ttlSeconds`, rounded up to a multiple of `roundSeconds`.
 */
export function signatureExpiry(
  ttlSeconds: number,
  roundSeconds: number,
  nowMs: number = Date.now(),
): number {
  const nowSeconds = Math.floor(nowMs / 1000);
  return Math.ceil((nowSeconds + ttlSeconds) / roundSeconds) * roundSeconds;
}

export interface MediaProxySecretEnv {
  MEDIA_PROXY_SECRET?: string;
  REALTIME_TICKET_SECRET?: string;
}

/**
 * The proxy's signing key. `MEDIA_PROXY_SECRET` when set; otherwise derived
 * from `REALTIME_TICKET_SECRET` under its own domain, so no new secret is
 * needed to deploy. Null when neither is set (generic mode is then off).
 */
export function mediaProxyKey(env: MediaProxySecretEnv): Buffer | null {
  const base = env.MEDIA_PROXY_SECRET || env.REALTIME_TICKET_SECRET;
  if (!base) return null;
  return nodeCrypto.createHmac("sha256", base).update(KEY_DOMAIN).digest();
}

function mac(key: Buffer, domain: string, value: string): Buffer {
  return nodeCrypto
    .createHmac("sha256", key)
    .update(domain)
    .update("\n")
    .update(value)
    .digest();
}

function equalBytes(a: Buffer, b: Buffer): boolean {
  return a.length === b.length && nodeCrypto.timingSafeEqual(a, b);
}

function decodeBase64Url(value: string): Buffer | null {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
  return Buffer.from(value, "base64url");
}

function urlMac(targetUrl: string, key: Buffer, expiresAt: number): Buffer {
  return mac(
    key,
    EXPIRING_URL_SIGNATURE_DOMAIN,
    `${expiresAt}\n${targetUrl}`,
  ).subarray(0, SIGNATURE_BYTES);
}

/**
 * The `sig` for `targetUrl` (the exact string sent as `url=`), valid until
 * `expiresAt` (seconds since the epoch, sent as `exp=`).
 */
export function signMediaUrl(
  targetUrl: string,
  key: Buffer,
  expiresAt: number,
): string {
  return urlMac(targetUrl, key, expiresAt).toString("base64url");
}

/**
 * True when `signature` is ours for `targetUrl` and `expires` (the `exp`
 * parameter) has not passed yet. A signature without `exp` is refused:
 * none is minted without one.
 */
export function verifyMediaUrlSignature(
  targetUrl: string,
  signature: string | null,
  key: Buffer,
  expires: string | null,
  nowMs: number = Date.now(),
): boolean {
  if (!signature || expires === null) return false;
  const given = decodeBase64Url(signature);
  if (!given || given.length !== SIGNATURE_BYTES) return false;
  if (!/^\d{1,12}$/.test(expires)) return false;
  const expiresAt = Number(expires);
  if (Math.floor(nowMs / 1000) >= expiresAt) return false;
  return equalBytes(given, urlMac(targetUrl, key, expiresAt));
}

export interface MediaPass {
  /** The cookie value. */
  value: string;
  /** Seconds since the epoch. */
  expiresAt: number;
}

/**
 * A media pass for `userId`, valid until `now + ttl` (seconds since the
 * epoch). The user id is inside the MAC, so the proxy can limit per user.
 */
export function issueMediaPass(
  key: Buffer,
  userId: string,
  nowMs: number = Date.now(),
  ttlSeconds: number = MEDIA_PASS_TTL_SECONDS,
): MediaPass {
  const expiresAt = Math.floor(nowMs / 1000) + ttlSeconds;
  const user = Buffer.from(userId, "utf8").toString("base64url");
  const nonce = nodeCrypto.randomBytes(9).toString("base64url");
  const body = `${PASS_VERSION}.${expiresAt}.${user}.${nonce}`;
  const signature = mac(key, PASS_DOMAIN, body)
    .subarray(0, SIGNATURE_BYTES)
    .toString("base64url");
  return { value: `${body}.${signature}`, expiresAt };
}

/** The pass's user, or null when it is not a valid, unexpired pass. */
export function verifyMediaPass(
  value: string | null | undefined,
  key: Buffer,
  nowMs: number = Date.now(),
): { userId: string } | null {
  if (!value) return null;
  const parts = value.split(".");
  if (parts.length !== 5 || parts[0] !== PASS_VERSION) return null;
  const [version, expiry, user, nonce, signature] = parts;
  if (!/^\d{1,12}$/.test(expiry)) return null;
  if (Math.floor(nowMs / 1000) >= Number(expiry)) return null;
  const given = decodeBase64Url(signature);
  if (!given || given.length !== SIGNATURE_BYTES) return null;
  const expected = mac(
    key,
    PASS_DOMAIN,
    `${version}.${expiry}.${user}.${nonce}`,
  ).subarray(0, SIGNATURE_BYTES);
  if (!equalBytes(given, expected)) return null;
  const userId = decodeBase64Url(user)?.toString("utf8");
  return userId ? { userId } : null;
}

/** The media pass cookie's value from a Cookie header. */
export function readMediaPassCookie(
  cookieHeader: string | null,
): string | null {
  if (!cookieHeader) return null;
  for (const part of cookieHeader.split(";")) {
    const index = part.indexOf("=");
    if (index < 0) continue;
    if (part.slice(0, index).trim() === MEDIA_PASS_COOKIE) {
      return part.slice(index + 1).trim() || null;
    }
  }
  return null;
}

/**
 * Set-Cookie for the pass. Scoped to the proxy's path and HttpOnly.
 * SameSite=Lax for the web app (the same site as the API); SameSite=None
 * only with `crossSite`, for the desktop webview, whose origin is another
 * site and would never send a Lax cookie on its image requests.
 */
export function mediaPassCookie(
  value: string,
  maxAgeSeconds: number,
  options: { crossSite?: boolean } = {},
): string {
  return [
    `${MEDIA_PASS_COOKIE}=${value}`,
    "Path=/api/proxy-media",
    `Max-Age=${Math.max(0, Math.floor(maxAgeSeconds))}`,
    "HttpOnly",
    "Secure",
    options.crossSite ? "SameSite=None" : "SameSite=Lax",
  ].join("; ");
}
