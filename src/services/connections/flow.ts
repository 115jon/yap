/**
 * The browser side of a link: where the provider sends the user back, the
 * cookie that ties the callback to the browser that started it, and the
 * redirect into Settings › Connections with the outcome.
 */

import {
  CONNECTION_RESULT_PARAM,
  CONNECTION_STATUS_PARAM,
  connectionAppLink,
  isConnectionApp,
  type ConnectionApp,
  type ConnectionProvider,
  type ConnectionResultStatus,
} from "@/lib/connections";
import { getPublicWebUrl } from "@/lib/platform";

import { LINK_STATE_TTL_MS } from "../connection.service";

/**
 * Starts: 10 per 10 minutes per user. Callbacks and a native app's begins:
 * 20 per 10 minutes per IP.
 */
export const LINK_START_LIMIT = { limit: 10, windowMs: 10 * 60_000 };
export const LINK_CALLBACK_LIMIT = { limit: 20, windowMs: 10 * 60_000 };
export const LINK_BEGIN_LIMIT = { limit: 20, windowMs: 10 * 60_000 };
/** A native app's claims: 10 per 10 minutes per user. */
export const LINK_COMPLETE_LIMIT = { limit: 10, windowMs: 10 * 60_000 };

/**
 * The origin the provider returns to: the Yap host, except local dev and
 * Worker previews, which get their own origin so the cookie and the
 * callback stay on one host.
 */
export function linkOrigin(request: Request): string {
  const url = new URL(request.url);
  const host = url.hostname;
  if (
    host === "localhost" ||
    host === "127.0.0.1" ||
    host.endsWith(".workers.dev")
  ) {
    return url.origin;
  }
  return getPublicWebUrl();
}

/** The callback path; a native app's carries `app` (appFromReturnTo). */
export function callbackPath(
  provider: ConnectionProvider,
  app: ConnectionApp | null = null,
): string {
  const path = `/api/connections/${provider}/callback`;
  return app ? `${path}?app=${app}` : path;
}

/**
 * Where a native app sends the system browser to begin a link: the state's
 * nonce and binding (both one-time, see rebindLinkState) and which app
 * asked. The app opens it; it never holds the provider's URL itself.
 */
export function beginUrl(
  origin: string,
  provider: ConnectionProvider,
  link: { nonce: string; binding: string },
  app: ConnectionApp,
): string {
  const query = new URLSearchParams({
    state: link.nonce,
    binding: link.binding,
    app,
  });
  return `${origin}/api/connections/${provider}/begin?${query.toString()}`;
}

/**
 * `__Host-`: the browser takes it only from this exact host, Secure, with
 * Path=/ and no Domain, so a sibling subdomain can't plant a binding of its
 * own (the cookie is the whole defence against a link started elsewhere).
 */
export function linkCookieName(provider: ConnectionProvider): string {
  return `__Host-yap_link_${provider}`;
}

function cookieAttributes(maxAge: number) {
  // Lax: sent on the provider's top-level redirect back, not on cross-site
  // subrequests.
  return `Path=/; Max-Age=${maxAge}; HttpOnly; Secure; SameSite=Lax`;
}

export function linkCookie(provider: ConnectionProvider, binding: string) {
  return `${linkCookieName(provider)}=${binding}; ${cookieAttributes(
    Math.floor(LINK_STATE_TTL_MS / 1000),
  )}`;
}

export function clearLinkCookie(provider: ConnectionProvider) {
  return `${linkCookieName(provider)}=; ${cookieAttributes(0)}`;
}

/** A cookie's value from a Cookie header, or null. */
export function readCookie(request: Request, name: string): string | null {
  const header = request.headers.get("cookie");
  if (!header) return null;
  for (const part of header.split(";")) {
    const index = part.indexOf("=");
    if (index < 0) continue;
    if (part.slice(0, index).trim() === name) {
      return part.slice(index + 1).trim() || null;
    }
  }
  return null;
}

/**
 * The app a link was started for, from the return_to the state stored
 * (callbackPath with `app`), or null for the web. Server-written, so it
 * can be trusted once the state is.
 */
export function appFromReturnTo(returnTo: string): ConnectionApp | null {
  try {
    const app = new URL(returnTo).searchParams.get("app");
    return isConnectionApp(app) ? app : null;
  } catch {
    return null;
  }
}

/**
 * The `app` a callback or begin URL names, before any state is read: only
 * where a failure's outcome goes, never what is linked.
 */
export function appHint(url: URL): ConnectionApp | null {
  const app = url.searchParams.get("app");
  return isConnectionApp(app) ? app : null;
}

/**
 * Back to a native app by its own link, without touching any cookie (a
 * begin that failed must not clear the binding of a link in flight).
 */
export function appRedirect(
  app: ConnectionApp,
  provider: ConnectionProvider,
  outcome: { status: ConnectionResultStatus } | { code: string },
  clearCookie = false,
): Response {
  const headers = new Headers({
    Location: connectionAppLink(app, provider, outcome),
    "Cache-Control": "no-store",
    "Referrer-Policy": "no-referrer",
  });
  if (clearCookie) headers.append("Set-Cookie", clearLinkCookie(provider));
  return new Response(null, { status: 303, headers });
}

/**
 * The redirect that ends every callback: back into the app, which opens
 * Settings › Connections and shows the outcome. For the web it is relative,
 * so it lands on the host that served the callback; for a link a native
 * app started (`app`), it is that app's own link (appRedirect). Always
 * clears the cookie.
 */
export function finishLink(
  provider: ConnectionProvider,
  status: ConnectionResultStatus,
  app: ConnectionApp | null = null,
): Response {
  if (app) return appRedirect(app, provider, { status }, true);
  const params = new URLSearchParams({
    [CONNECTION_RESULT_PARAM]: provider,
    [CONNECTION_STATUS_PARAM]: status,
  });
  const headers = new Headers({
    Location: `/chat?${params.toString()}`,
    "Cache-Control": "no-store",
    "Referrer-Policy": "no-referrer",
  });
  headers.append("Set-Cookie", clearLinkCookie(provider));
  return new Response(null, { status: 303, headers });
}
