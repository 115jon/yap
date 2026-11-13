import { createFileRoute } from "@tanstack/react-router";

import { apiError, apiSuccess, requireAuth } from "@/lib/api-helpers";
import { mediaCapSecret } from "@/lib/media-auth";
import { issueMediaGrant } from "@/lib/media-capability";

/**
 * POST /api/media/grant: the signed-in client's media grant, the key it
 * signs private media URLs with (lib/media-capability). Bound to the caller
 * and an expiry a few hours out; the session itself never goes in a URL.
 * `issuedAt` is the server's clock, so the client counts the lifetime
 * without trusting its own.
 */
export const POST = async ({ request }: { request: Request }) => {
  const auth = await requireAuth(request);
  if (auth instanceof Response) return auth;

  const secret = mediaCapSecret();
  if (!secret) {
    return apiError(
      "Media links are unavailable",
      503,
      "MEDIA_GRANT_UNAVAILABLE",
      request,
    );
  }

  const response = apiSuccess(
    issueMediaGrant(secret, auth.userId, Date.now()),
    200,
    request,
  );
  response.headers.set("Cache-Control", "no-store");
  return response;
};

export const Route = createFileRoute("/api/media/grant")({
  server: {
    handlers: {
      POST,
    },
  },
});
