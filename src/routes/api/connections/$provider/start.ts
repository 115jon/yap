import { createFileRoute } from "@tanstack/react-router";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { isConnectionApp, type ConnectionApp } from "@/lib/connections";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { createLinkState } from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import {
  beginUrl,
  callbackPath,
  LINK_START_LIMIT,
  linkCookie,
  linkOrigin,
} from "@/services/connections/flow";

type Args = { request: Request; params: { provider?: string } };

/**
 * The body's `app` (a native app starting the link), null for the web
 * (no body, `{}`, or no `app`), or a 400 for anything else.
 */
async function requestedApp(
  request: Request,
): Promise<ConnectionApp | null | Response> {
  const text = await request.text().catch(() => "");
  if (!text.trim()) return null;
  let body: unknown;
  try {
    body = JSON.parse(text);
  } catch {
    return apiError("Invalid JSON body", 400, "INVALID_JSON", request);
  }
  const app =
    body && typeof body === "object"
      ? (body as Record<string, unknown>).app
      : undefined;
  if (app === undefined || app === null) return null;
  if (!isConnectionApp(app)) {
    return apiError("Unknown app", 400, "INVALID_BODY", request);
  }
  return app;
}

// POST /api/connections/:provider/start — begins a link. The web gets the
// provider URL to navigate to, and the HttpOnly cookie the callback must
// come back with (so a link started by someone else can't be finished in
// this browser). A native app (`{ app: "windows" | "android" }`), whose
// session is not a browser cookie, gets the begin URL to open in the
// system browser instead: that route sets the cookie there, and the
// callback hands the app a code to claim (complete) rather than linking.
const POST = async ({ request, params }: Args) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const app = await requestedApp(request);
  if (app instanceof Response) return app;
  const limited = await checkRateLimitDO(
    authResult.userId,
    "connection-link-start",
    LINK_START_LIMIT,
  );
  if (limited) return limited;

  const origin = linkOrigin(request);
  const link = await createLinkState(getDB(), {
    userId: authResult.userId,
    provider: provider.id,
    callbackUrl: `${origin}${callbackPath(provider.id, app)}`,
  });
  if (app) {
    const response = apiSuccess(
      { url: beginUrl(origin, provider.id, link, app) },
      200,
      request,
    );
    response.headers.set("Cache-Control", "no-store");
    return response;
  }
  const response = apiSuccess(
    {
      url: provider.authorizationUrl({
        returnTo: link.returnTo,
        realm: origin,
      }),
    },
    200,
    request,
  );
  response.headers.set("Cache-Control", "no-store");
  response.headers.append("Set-Cookie", linkCookie(provider.id, link.binding));
  return response;
};

export const Route = createFileRoute("/api/connections/$provider/start")({
  server: {
    handlers: {
      POST,
    },
  },
});
