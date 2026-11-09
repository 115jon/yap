import { createFileRoute } from "@tanstack/react-router";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { createLinkState } from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import {
  callbackPath,
  LINK_START_LIMIT,
  linkCookie,
  linkOrigin,
} from "@/services/connections/flow";

type Args = { request: Request; params: { provider?: string } };

// POST /api/connections/:provider/start — begins a link. Answers the
// provider URL to navigate to, and sets the HttpOnly cookie the callback
// must come back with (so a link started by someone else can't be
// finished in this browser).
const POST = async ({ request, params }: Args) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
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
    callbackUrl: `${origin}${callbackPath(provider.id)}`,
  });
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
