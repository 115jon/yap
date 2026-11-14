/**
 * The client's media grant (lib/media-capability): fetched with the session
 * (`POST /api/media/grant`), kept in memory and localStorage, and used to
 * sign private media URLs synchronously while rendering. The session token
 * itself never goes in a URL.
 *
 * A stored grant is tied to the session it was issued for (a hash of the
 * token, never the token), so another account's grant is never reused.
 */
import { sha256 } from "@noble/hashes/sha2.js";

import { clog } from "@/lib/console-logger";
import {
  getStoredKovaAuthSessionToken,
  subscribeDesktopTokenChanges,
} from "@/lib/desktop-auth";
import {
  MEDIA_GRANT_REFRESH_MARGIN_MS,
  type MediaGrant,
  withMediaCap,
} from "@/lib/media-capability";

const log = clog("media-grant");

const STORAGE_KEY = "yap:media-grant";
/** A grant with less left than this is not used: its URLs would die in flight. */
const MIN_USABLE_MS = 5 * 60 * 1000;
/**
 * A URL signed earlier is reused while its own cap has this long left, so a
 * grant refresh does not change every URL on screen (videos would restart,
 * images re-download).
 */
const REUSE_SIGNED_URL_MS = 30 * 60 * 1000;
/** After a failed fetch, wait this long before asking again. */
const RETRY_AFTER_FAILURE_MS = 30 * 1000;
/** A forced refresh (a signed URL was refused) at most this often. */
const FORCED_REFRESH_INTERVAL_MS = 60 * 1000;
const SIGNED_URL_CACHE_LIMIT = 2000;

interface StoredGrant extends MediaGrant {
  /** Which session it was issued for: a hash of the token. */
  readonly session: string;
  /**
   * When it expires by this device's clock: the server's lifetime counted
   * from when the answer arrived, so a wrong clock cannot make a live grant
   * look dead or a dead one look live.
   */
  readonly localExpiresAt: number;
}

interface SignedUrl {
  readonly url: string;
  readonly localExpiresAt: number;
}

let current: StoredGrant | null = null;
let loaded = false;
let inflight: Promise<MediaGrant | null> | null = null;
let lastFailureAt = 0;
let lastForcedAt = 0;
let lastTagToken: string | null = null;
let lastTag: string | null = null;
const listeners = new Set<() => void>();
const signedUrls = new Map<string, SignedUrl>();
let watchingTokens = false;

function sessionToken(): string | null {
  try {
    return getStoredKovaAuthSessionToken();
  } catch {
    return null;
  }
}

function sessionTag(token: string): string {
  if (token !== lastTagToken) {
    const digest = sha256(
      new TextEncoder().encode(`yap/media-grant/session\0${token}`),
    );
    let hex = "";
    for (const byte of digest.subarray(0, 16)) {
      hex += byte.toString(16).padStart(2, "0");
    }
    lastTagToken = token;
    lastTag = hex;
  }
  return lastTag as string;
}

function isFiniteNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isGrant(value: unknown): value is MediaGrant {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;
  return (
    typeof record.userId === "string" &&
    record.userId.length > 0 &&
    typeof record.key === "string" &&
    record.key.length > 0 &&
    isFiniteNumber(record.expiresAt)
  );
}

function loadStored(): void {
  if (loaded) return;
  loaded = true;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return;
    const parsed: unknown = JSON.parse(raw);
    if (
      isGrant(parsed) &&
      typeof (parsed as { session?: unknown }).session === "string" &&
      isFiniteNumber((parsed as { localExpiresAt?: unknown }).localExpiresAt)
    ) {
      current = parsed as StoredGrant;
    }
  } catch {
    // No storage (private window, tests): the grant lives in memory only.
  }
}

function store(next: StoredGrant | null): void {
  const changed =
    current?.key !== next?.key || current?.expiresAt !== next?.expiresAt;
  // URLs signed for another user or session must go; a refresh for the same
  // one keeps them (each is reused only while its own cap is good).
  if (
    !next ||
    next.userId !== current?.userId ||
    next.session !== current?.session
  ) {
    signedUrls.clear();
  }
  current = next;
  try {
    if (next) localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    // Memory only.
  }
  if (changed) for (const listener of listeners) listener();
}

function watchTokens(): void {
  if (watchingTokens || typeof window === "undefined") return;
  watchingTokens = true;
  subscribeDesktopTokenChanges((token) => {
    // Signed out: forget the grant. Another session: drop it and fetch one.
    if (!token) store(null);
    else if (current && current.session !== sessionTag(token)) {
      store(null);
      void ensureMediaGrant();
    }
  });
}

/** The stored grant if it belongs to the current session. */
function sessionGrant(): StoredGrant | null {
  if (typeof window === "undefined") return null;
  watchTokens();
  loadStored();
  const token = sessionToken();
  if (!token) return null;
  return current && current.session === sessionTag(token) ? current : null;
}

/**
 * The grant for this session, if one is usable now. Starts a fetch (in the
 * background) when there is none or it is due for refresh.
 */
export function getMediaGrant(now: number = Date.now()): MediaGrant | null {
  if (typeof window === "undefined" || !sessionToken()) return null;
  const grant = sessionGrant();
  const remaining = grant ? grant.localExpiresAt - now : 0;
  if (remaining < MEDIA_GRANT_REFRESH_MARGIN_MS) void ensureMediaGrant();
  return grant && remaining > MIN_USABLE_MS ? grant : null;
}

/**
 * Fetches a fresh grant unless one is already on the way. Resolves to the
 * grant, or null when signed out or the server could not issue one.
 * `force`: fetch even though the current grant looks good (a URL signed
 * with it was refused), at most once a minute.
 */
export function ensureMediaGrant(
  options: { force?: boolean } = {},
): Promise<MediaGrant | null> {
  if (inflight) return inflight;
  const token = sessionToken();
  if (!token || typeof window === "undefined") return Promise.resolve(null);
  const now = Date.now();
  if (now - lastFailureAt < RETRY_AFTER_FAILURE_MS) {
    return Promise.resolve(sessionGrant());
  }
  if (options.force) {
    if (now - lastForcedAt < FORCED_REFRESH_INTERVAL_MS) {
      return Promise.resolve(sessionGrant());
    }
    lastForcedAt = now;
  }

  inflight = (async () => {
    try {
      const { apiPost } = await import("@/lib/api-client");
      const grant = await apiPost<unknown>("/api/media/grant", {});
      if (!isGrant(grant)) throw new Error("Malformed media grant");
      const issuedAt = (grant as { issuedAt?: unknown }).issuedAt;
      const lifetime = isFiniteNumber(issuedAt)
        ? grant.expiresAt - issuedAt
        : grant.expiresAt - Date.now();
      // The session may have changed while the request was out.
      if (sessionToken() !== token) return null;
      store({
        userId: grant.userId,
        expiresAt: grant.expiresAt,
        key: grant.key,
        session: sessionTag(token),
        localExpiresAt: Date.now() + lifetime,
      });
      return current;
    } catch (error) {
      lastFailureAt = Date.now();
      log.warn("Could not fetch a media grant", {
        message: error instanceof Error ? error.message : String(error),
      });
      return null;
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}

/**
 * Waits (at most `timeoutMs`) for a usable grant, so the first render of the
 * app signs its media URLs instead of showing broken images.
 */
export async function primeMediaGrant(timeoutMs = 2500): Promise<void> {
  if (getMediaGrant()) return;
  await Promise.race([
    ensureMediaGrant(),
    new Promise((resolve) => setTimeout(resolve, timeoutMs)),
  ]);
}

/**
 * `url` signed for this session, or null when there is no usable grant yet
 * (lib/media-repair signs it again once one arrives).
 */
export function signMediaUrl(url: URL, now: number = Date.now()): URL | null {
  const grant = getMediaGrant(now) as StoredGrant | null;
  if (!grant) return null;
  const cacheKey = url.toString();
  const cached = signedUrls.get(cacheKey);
  if (cached && cached.localExpiresAt - now > REUSE_SIGNED_URL_MS) {
    // Least recently used goes first when the cache is full.
    signedUrls.delete(cacheKey);
    signedUrls.set(cacheKey, cached);
    return new URL(cached.url);
  }
  const signed = withMediaCap(grant, url);
  signedUrls.delete(cacheKey);
  if (signedUrls.size >= SIGNED_URL_CACHE_LIMIT) {
    const oldest = signedUrls.keys().next().value;
    if (oldest !== undefined) signedUrls.delete(oldest);
  }
  signedUrls.set(cacheKey, {
    url: signed.toString(),
    localExpiresAt: grant.localExpiresAt,
  });
  return signed;
}

/** Forgets the signature cached for `url`, so the next signing is fresh. */
export function forgetSignedMediaUrl(url: URL): void {
  signedUrls.delete(url.toString());
}

/** Called whenever the grant changes (fetched, refreshed, dropped). */
export function subscribeMediaGrant(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** A value that changes with the grant: a dependency for rebuilding signed URLs. */
export function getMediaGrantVersion(): string {
  const grant = getMediaGrant();
  return grant ? `${grant.userId}:${grant.expiresAt}` : "";
}

/** Tests only: `grant` as if fetched for the session `token` at `now`. */
export function seedMediaGrantForTests(
  grant: MediaGrant & { readonly issuedAt?: number },
  token: string,
  now: number = Date.now(),
): void {
  loaded = true;
  const issuedAt = grant.issuedAt ?? now;
  store({
    userId: grant.userId,
    expiresAt: grant.expiresAt,
    key: grant.key,
    session: sessionTag(token),
    localExpiresAt: now + (grant.expiresAt - issuedAt),
  });
}

/** Tests only. */
export function resetMediaGrantForTests(): void {
  current = null;
  loaded = false;
  inflight = null;
  lastFailureAt = 0;
  lastForcedAt = 0;
  signedUrls.clear();
  listeners.clear();
}
