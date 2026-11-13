// The media pass: an HttpOnly cookie, scoped to /api/proxy-media, that lets
// this signed-in browser load third-party images and videos through our
// media proxy (`getAuthAssetUrl` builds unsigned proxy URLs for them). The
// server issues it on POST /api/proxy-media for a valid session, bound to
// the user, and it lasts an hour; it is renewed when less than 20 minutes
// are left (checked every 10 minutes and when the tab comes back), on every
// app start, and cleared on sign-out. Each renewal checks the session, so a
// signed-out session's pass ends within the hour.

import {
  getDesktopToken,
  getStoredKovaAuthSessionToken,
  subscribeDesktopTokenChanges,
} from "@/lib/desktop-auth";
import { KOVA_AUTH_PUBLISHABLE_KEY } from "@/lib/kova-auth-config";
import { getApiBaseUrl, isTauri } from "@/lib/platform";

const EXPIRY_STORAGE_KEY = "yap.mediaPass.expiresAt";
/** Renew once less than this is left on the cookie (it lasts an hour). */
export const RENEW_MARGIN_MS = 20 * 60 * 1000;
const CHECK_INTERVAL_MS = 10 * 60 * 1000;

function currentToken(): string | null {
  try {
    return getDesktopToken() ?? getStoredKovaAuthSessionToken();
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
  if (isTauri() && KOVA_AUTH_PUBLISHABLE_KEY) {
    headers.set("X-Publishable-Key", KOVA_AUTH_PUBLISHABLE_KEY);
  }
  return headers;
}

let inflight: Promise<void> | null = null;

/** Asks for a pass unless the current one has more than a few hours left. */
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
        expiresAt?: number;
      } | null;
      if (typeof body?.expiresAt === "number") {
        writeExpiry(body.expiresAt * 1000);
      }
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
