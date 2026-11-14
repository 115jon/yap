// The media pass: an HttpOnly cookie, scoped to /api/proxy-media, that lets
// this signed-in browser load third-party images and videos through our
// media proxy (`getAuthAssetUrl` builds unsigned proxy URLs for them). The
// server issues it on POST /api/proxy-media for a valid session, bound to
// the user, and it lasts an hour; it is renewed when less than 20 minutes
// are left (checked every 10 minutes and when the tab comes back), on every
// app start, and cleared on sign-out. Each renewal checks the session, so a
// signed-out session's pass ends within the hour.
//
// The expiry is kept on this device's clock: the server says how long the
// pass lasts (`expiresIn`), and the client adds that to its own now. A
// clock that is off by more than the renewal margin would otherwise never
// renew (behind) or renew on every check (ahead).

import {
  getStoredKovaAuthSessionToken,
  subscribeDesktopTokenChanges,
} from "@/lib/desktop-auth";
import { getApiBaseUrl } from "@/lib/platform";

const EXPIRY_STORAGE_KEY = "yap.mediaPass.expiresAt";
/** Renew once less than this is left on the cookie (it lasts an hour). */
export const RENEW_MARGIN_MS = 20 * 60 * 1000;
const CHECK_INTERVAL_MS = 10 * 60 * 1000;

function currentToken(): string | null {
  try {
    return getStoredKovaAuthSessionToken();
  } catch {
    return null;
  }
}

function readExpiry(): number {
  try {
    return Number(localStorage.getItem(EXPIRY_STORAGE_KEY)) || 0;
  } catch {
    return 0;
  }
}

function writeExpiry(expiresAtMs: number | null): void {
  try {
    if (expiresAtMs) {
      localStorage.setItem(EXPIRY_STORAGE_KEY, String(expiresAtMs));
    } else {
      localStorage.removeItem(EXPIRY_STORAGE_KEY);
    }
  } catch {
    // Storage off: the pass is just requested again next time.
  }
}

function requestHeaders(token: string | null): Headers {
  const headers = new Headers();
  if (token) headers.set("Authorization", `Bearer ${token}`);
  return headers;
}

let inflight: Promise<void> | null = null;

/**
 * When a pass answered at `nowMs` expires, in this device's clock: now plus
 * the server's `expiresIn`. Null when the answer has none (an older server,
 * whose absolute `expiresAt` is on its own clock and is not used).
 */
export function localPassExpiry(
  body: { expiresIn?: unknown } | null,
  nowMs: number = Date.now(),
): number | null {
  const ttl = body?.expiresIn;
  if (typeof ttl !== "number" || !Number.isFinite(ttl) || ttl <= 0) {
    return null;
  }
  return nowMs + ttl * 1000;
}

/** Asks for a pass unless the current one has more than 20 minutes left. */
export function ensureMediaPass(force = false): Promise<void> {
  const token = currentToken();
  if (!token) return Promise.resolve();
  if (!force && readExpiry() - Date.now() > RENEW_MARGIN_MS) {
    return Promise.resolve();
  }
  inflight ??= fetch(`${getApiBaseUrl()}/api/proxy-media`, {
    method: "POST",
    credentials: "include",
    headers: requestHeaders(token),
  })
    .then(async (response) => {
      if (!response.ok) return;
      const body = (await response.json().catch(() => null)) as {
        expiresIn?: number;
      } | null;
      // Without a TTL the next check asks again (no expiry stored).
      writeExpiry(localPassExpiry(body));
    })
    .catch(() => {
      // Offline or refused: images fall back to signed URLs where the
      // server sent them, and the next check asks again.
    })
    .finally(() => {
      inflight = null;
    });
  return inflight;
}

/** Drops the pass (sign-out). */
export function clearMediaPass(): void {
  writeExpiry(null);
  void fetch(`${getApiBaseUrl()}/api/proxy-media`, {
    method: "DELETE",
    credentials: "include",
    headers: requestHeaders(null),
  }).catch(() => {});
}

/**
 * Keeps the pass while the app runs: now, on every token change, when the
 * tab comes back, and on a slow timer. Returns the cleanup.
 */
export function startMediaPass(): () => void {
  if (typeof window === "undefined") return () => undefined;
  let lastToken = currentToken();
  // Once per app start regardless of the stored expiry: the cookie may have
  // been cleared while the expiry stayed behind.
  void ensureMediaPass(true);

  const unsubscribe = subscribeDesktopTokenChanges((token) => {
    if (!token) {
      if (lastToken) clearMediaPass();
    } else if (token !== lastToken) {
      void ensureMediaPass(true);
    }
    lastToken = token;
  });
  const onVisible = () => {
    if (document.visibilityState === "visible") void ensureMediaPass();
  };
  document.addEventListener("visibilitychange", onVisible);
  const timer = window.setInterval(
    () => void ensureMediaPass(),
    CHECK_INTERVAL_MS,
  );
  return () => {
    unsubscribe();
    document.removeEventListener("visibilitychange", onVisible);
    window.clearInterval(timer);
  };
}
