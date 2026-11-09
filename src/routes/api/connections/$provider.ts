import { createFileRoute } from "@tanstack/react-router";
import { Schema } from "effect";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { isConnectionProvider } from "@/lib/connections";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { validateBody } from "@/lib/validate-body";
import {
  setShowOnProfile,
  unlinkConnection,
} from "@/services/connection.service";

/** Toggles and unlinks: 30 a minute per user. */
export const CONNECTION_UPDATE_LIMIT = { limit: 30, windowMs: 60_000 };

type Args = { request: Request; params: { provider?: string } };

const updateSchema = Schema.Struct({
  showOnProfile: Schema.Boolean,
});

async function authorize({ request, params }: Args) {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  if (!isConnectionProvider(params.provider)) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const limited = await checkRateLimitDO(
    authResult.userId,
    "connection-update",
    CONNECTION_UPDATE_LIMIT,
  );
  if (limited) return limited;
  return { userId: authResult.userId, provider: params.provider };
}

// PATCH /api/connections/:provider { showOnProfile } — the profile switch.
const PATCH = async (args: Args) => {
  const who = await authorize(args);
  if (who instanceof Response) return who;
  const body = await validateBody(args.request, updateSchema);
  if (body instanceof Response) return body;

  const found = await setShowOnProfile(
    getDB(),
    who.userId,
    who.provider,
    body.showOnProfile,
  );
  if (!found) {
    return apiError("Nothing is connected", 404, "NOT_CONNECTED", args.request);
  }
  return apiSuccess(
    { provider: who.provider, show_on_profile: body.showOnProfile },
    200,
    args.request,
  );
};

// DELETE /api/connections/:provider — unlink. Idempotent.
const DELETE = async (args: Args) => {
  const who = await authorize(args);
  if (who instanceof Response) return who;

  const removed = await unlinkConnection(getDB(), who.userId, who.provider);
  return apiSuccess({ provider: who.provider, removed }, 200, args.request);
};

export const Route = createFileRoute("/api/connections/$provider")({
  server: {
    handlers: {
      PATCH,
      DELETE,
    },
  },
});
