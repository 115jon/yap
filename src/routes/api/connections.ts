import { createFileRoute } from "@tanstack/react-router";

import { apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { listOwnConnections } from "@/services/connection.service";

/** Settings reads: 60 a minute per user. */
const CONNECTION_LIST_LIMIT = { limit: 60, windowMs: 60_000 };

// GET /api/connections — the signed-in user's linked accounts, hidden ones
// included (Settings › Connections). Never carries external ids.
const GET = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const limited = await checkRateLimitDO(
    authResult.userId,
    "connection-list",
    CONNECTION_LIST_LIMIT,
  );
  if (limited) return limited;

  const connections = await listOwnConnections(getDB(), authResult.userId);
  const response = apiSuccess({ connections }, 200, request);
  response.headers.set("Cache-Control", "no-store");
  return response;
};

export const Route = createFileRoute("/api/connections")({
  server: {
    handlers: {
      GET,
    },
  },
});
