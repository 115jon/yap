// Signatures for /api/proxy-media's generic mode.
//
// The proxy fetches any public http(s) image or video, so it must not be an
// open proxy. A request is let through when it carries one of:
//   - `sig`: an HMAC over the exact target URL, minted by this server when it
//     builds a payload (embeds, GIF results, Listen Together artwork). Stable
//     per URL, so browsers and the edge cache can keep the response.
//   - the media pass cookie (`/api/media-pass`): a signed, expiring value the
//     web app gets while signed in, so URLs it builds itself need no sig.
//   - a session bearer (Expo and native send it to the API's own URLs).
//
// node:crypto (nodejs_compat) keeps signing synchronous, so payload rewriting
// can run where messages are serialized without threading async through.

// A namespace import, for the same reason as origin.ts: the desktop build
// bundles this against an empty browser stub; it only runs on the Worker.
import * as nodeCrypto from "node:crypto";

const URL_SIGNATURE_DOMAIN = "yap/media-proxy/url/v1";
const PASS_DOMAIN = "yap/media-proxy/pass/v1";
const KEY_DOMAIN = "yap/media-proxy/key/v1";
/** 128 bits of HMAC-SHA256, base64url: 22 characters. */
const SIGNATURE_BYTES = 16;
const PASS_VERSION = "mp1";

export const MEDIA_PASS_COOKIE = "yap_media_pass";
export const MEDIA_PASS_TTL_SECONDS = 7 * 24 * 60 * 60;

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

/** The `sig` for `targetUrl` (the exact string sent as `url=`). */
export function signMediaUrl(targetUrl: string, key: Buffer): string {
  return mac(key, URL_SIGNATURE_DOMAIN, targetUrl)
    .subarray(0, SIGNATURE_BYTES)
    .toString("base64url");
}

export function verifyMediaUrlSignature(
  targetUrl: string,
  signature: string | null,
  key: Buffer,
): boolean {
  if (!signature) return false;
  const given = decodeBase64Url(signature);
  if (!given || given.length !== SIGNATURE_BYTES) return false;
  const expected = mac(key, URL_SIGNATURE_DOMAIN, targetUrl).subarray(
    0,
    SIGNATURE_BYTES,
  );
  return equalBytes(given, expected);
}

export interface MediaPass {
  /** The cookie value. */
  value: string;
  /** Seconds since the epoch. */
  expiresAt: number;
}

/** A media pass valid until `now + ttl` (seconds since the epoch). */
export function issueMediaPass(
  key: Buffer,
  nowMs: number = Date.now(),
  ttlSeconds: number = MEDIA_PASS_TTL_SECONDS,
): MediaPass {
  const expiresAt = Math.floor(nowMs / 1000) + ttlSeconds;
  const nonce = nodeCrypto.randomBytes(9).toString("base64url");
  const body = `${PASS_VERSION}.${expiresAt}.${nonce}`;
  const signature = mac(key, PASS_DOMAIN, body)
    .subarray(0, SIGNATURE_BYTES)
    .toString("base64url");
  return { value: `${body}.${signature}`, expiresAt };
}

export function verifyMediaPass(
  value: string | null | undefined,
  key: Buffer,
  nowMs: number = Date.now(),
): boolean {
  if (!value) return false;
  const parts = value.split(".");
  if (parts.length !== 4 || parts[0] !== PASS_VERSION) return false;
  const expiresAt = Number(parts[1]);
  if (!Number.isInteger(expiresAt)) return false;
  if (Math.floor(nowMs / 1000) >= expiresAt) return false;
  const given = decodeBase64Url(parts[3]);
  if (!given || given.length !== SIGNATURE_BYTES) return false;
  const expected = mac(
    key,
    PASS_DOMAIN,
    `${parts[0]}.${parts[1]}.${parts[2]}`,
  ).subarray(0, SIGNATURE_BYTES);
  return equalBytes(given, expected);
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
 * Set-Cookie for the pass. Scoped to the proxy's path, HttpOnly, and
 * SameSite=None so the desktop webview (another site) sends it too: the pass
 * only unlocks fetching public media, so a cross-site send leaks nothing.
 */
export function mediaPassCookie(value: string, maxAgeSeconds: number): string {
  return [
    `${MEDIA_PASS_COOKIE}=${value}`,
    "Path=/api/proxy-media",
    `Max-Age=${Math.max(0, Math.floor(maxAgeSeconds))}`,
    "HttpOnly",
    "Secure",
    "SameSite=None",
  ].join("; ");
}
