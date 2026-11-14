/**
 * Who may read a private media URL (attachments, emoji, soundboard clips,
 * camera backgrounds, voice status media, Listen Together): the session in
 * the request's headers, or a media capability (`?cap=m1.…`, see
 * lib/media-capability) for the requests a browser makes on its own.
 *
 * The session token in the query string (`?token=`) is refused, except
 * from web tabs opened before media capabilities, for their own
 * subresources, until STALE_WEB_TAB_SUNSET. Those requests are logged as
 * deprecated, without the token; the Worker's own logs and traces drop query
 * strings (`observability.redact_query_string`).
 */
import { pendingDeletionDate } from "@/lib/account-deletion";
import {
  getEnv,
  pendingDeletionResponse,
  requireCredentials,
  suspendedResponse,
  unauthorizedResponse,
} from "@/lib/api-helpers";
import { clog } from "@/lib/console-logger";
import {
  CAP_PROTECTED_MEDIA_PREFIXES,
  isMediaCap,
  MEDIA_CAP_PARAM,
  verifyMediaCap,
} from "@/lib/media-capability";
import { isSuspended } from "@/lib/suspension";

const log = clog("media-auth");

/**
 * Until this date a web tab opened before this release keeps loading its
 * media with `?token=` (its own subresource requests only). A reload picks
 * up the bundle that signs URLs instead.
 */
export const STALE_WEB_TAB_SUNSET = Date.UTC(2026, 11, 1);

interface MediaSecrets {
  MEDIA_CAP_SECRET?: string;
  REALTIME_TICKET_SECRET?: string;
}

/** The secret caps are signed with: its own if set, else the ticket secret (domain-separated). */
export function mediaCapSecret(): string | null {
  const env = getEnv() as CloudflareEnv & MediaSecrets;
  return env.MEDIA_CAP_SECRET || env.REALTIME_TICKET_SECRET || null;
}

/** The media route family for logs: never the full path (file names) or query. */
function mediaRoute(url: URL): string {
  return (
    CAP_PROTECTED_MEDIA_PREFIXES.find((prefix) =>
      url.pathname.startsWith(prefix),
    ) ?? "other"
  );
}

/** What a same-origin page loads on its own: images, media, its fetches. */
const SUBRESOURCE_DESTINATIONS = new Set([
  "image",
  "video",
  "audio",
  "track",
  "empty",
]);

/**
 * A web tab still running the bundle from before media capabilities: its
 * own `<img>`/`<video>`/fetch requests (same origin, a subresource), never a
 * link opened from elsewhere (`Sec-Fetch-Site: none` or cross-site, a
 * document navigation).
 */
function isStaleWebTabRequest(request: Request): boolean {
  return (
    request.headers.get("sec-fetch-site") === "same-origin" &&
    SUBRESOURCE_DESTINATIONS.has(request.headers.get("sec-fetch-dest") ?? "")
  );
}

/**
 * Which legacy client a header-less `?token=` request is, if it is one still
 * allowed to send it: a stale web tab's own GET/HEAD subresource request
 * before STALE_WEB_TAB_SUNSET. Null for anything else.
 */
export function legacyQueryTokenClient(
  request: Request,
  now: number = Date.now(),
): "web-stale-tab" | null {
  if (
    (request.method !== "GET" && request.method !== "HEAD") ||
    request.headers.get("authorization")
  ) {
    return null;
  }
  if (now < STALE_WEB_TAB_SUNSET && isStaleWebTabRequest(request)) {
    return "web-stale-tab";
  }
  return null;
}

/**
 * The reader of a private media URL, or the response to send instead (401;
 * 403 for a suspended account; 503 when Kova cannot answer).
 */
export async function requireMediaAuth(
  request: Request,
): Promise<{ userId: string } | Response> {
  const url = new URL(request.url);
  const cap = url.searchParams.get(MEDIA_CAP_PARAM);
  const authorization = request.headers.get("authorization");
  const cookie = request.headers.get("cookie");
  if (isMediaCap(cap)) {
    const capResult = await authorizeCap(request, url, cap);
    // A refused cap on a request that also carries a session: use that.
    if (
      capResult instanceof Response &&
      capResult.status === 401 &&
      (authorization || cookie)
    ) {
      return requireCredentials(request, { authorization, cookie });
    }
    return capResult;
  }

  const queryToken = url.searchParams.get("token");
  if (queryToken && !authorization) {
    const legacyClient = legacyQueryTokenClient(request);
    if (legacyClient) {
      log.warn("Deprecated ?token= media request", {
        route: mediaRoute(url),
        client: legacyClient,
      });
      return requireCredentials(request, {
        authorization: `Bearer ${queryToken}`,
        cookie: null,
      });
    }
    log.warn("Refused ?token= media request", { route: mediaRoute(url) });
  }
  return requireCredentials(request, { authorization, cookie });
}

async function authorizeCap(
  request: Request,
  url: URL,
  cap: string,
): Promise<{ userId: string } | Response> {
  const secret = mediaCapSecret();
  if (!secret) {
    log.error("Media capability secret is not configured");
    return unauthorizedResponse(request);
  }
  const verification = verifyMediaCap(cap, url, secret, Date.now());
  if (!verification.ok) {
    log.info("Media capability refused", {
      route: mediaRoute(url),
      reason: verification.reason,
    });
    return unauthorizedResponse(request);
  }
  if (await isSuspended(verification.userId)) {
    return suspendedResponse(request);
  }
  // As requireAuth: an account waiting for deletion reads nothing.
  const deleteAfter = await pendingDeletionDate(verification.userId);
  if (deleteAfter) return pendingDeletionResponse(deleteAfter, request);
  return { userId: verification.userId };
}
