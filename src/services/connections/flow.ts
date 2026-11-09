/**
 * The browser side of a link: where the provider sends the user back, the
 * cookie that ties the callback to the browser that started it, and the
 * redirect into Settings › Connections with the outcome.
 */

import {
  CONNECTION_RESULT_PARAM,
  CONNECTION_STATUS_PARAM,
  type ConnectionProvider,
  type ConnectionResultStatus,
} from "@/lib/connections";
import { getPublicWebUrl } from "@/lib/platform";

import { LINK_STATE_TTL_MS } from "../connection.service";

/** Starts: 10 per 10 minutes per user. Callbacks: 20 per 10 minutes per IP. */
export const LINK_START_LIMIT = { limit: 10, windowMs: 10 * 60_000 };
export const LINK_CALLBACK_LIMIT = { limit: 20, windowMs: 10 * 60_000 };

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

export function callbackPath(provider: ConnectionProvider): string {
  return `/api/connections/${provider}/callback`;
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
 * The redirect that ends every callback: back into the app, which opens
 * Settings › Connections and shows the outcome. Relative, so it lands on
 * the host that served the callback. Always clears the cookie.
 */
export function finishLink(
  provider: ConnectionProvider,
  status: ConnectionResultStatus,
): Response {
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
