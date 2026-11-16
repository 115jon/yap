import { createFileRoute } from "@tanstack/react-router";
import { Schema } from "effect";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { clog } from "@/lib/console-logger";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { validateBody } from "@/lib/validate-body";
import { linkConnection, takeLinkResult } from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import { LINK_COMPLETE_LIMIT } from "@/services/connections/flow";

const log = clog("connections/complete");

type Args = { request: Request; params: { provider?: string } };

const completeSchema = Schema.Struct({ code: Schema.String });

// POST /api/connections/:provider/complete { code } — a native app claims
// the account its link verified (the code its own link brought back from
// the browser). Only the account that started the link can claim it, so a
// begin URL opened by someone else links nothing. Answers the outcome as
// the web's callback names it: `linked` or `taken`.
const POST = async ({ request, params }: Args) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const body = await validateBody(request, completeSchema);
  if (body instanceof Response) return body;
  const limited = await checkRateLimitDO(
    authResult.userId,
    "connection-link-complete",
    LINK_COMPLETE_LIMIT,
  );
  if (limited) return limited;

  const db = getDB();
  const taken = await takeLinkResult(db, {
    userId: authResult.userId,
    provider: provider.id,
    code: body.code,
  });
  if (!taken.ok) {
    if (taken.reason === "other_account") {
      log.warn("A link result claimed by another account", {
        provider: provider.id,
      });
      return apiError(
        "This sign-in was started from another Yap account.",
        403,
        "OTHER_ACCOUNT",
        request,
      );
    }
    return apiError(
      "The sign-in expired. Start again from Connections.",
      410,
      "LINK_EXPIRED",
      request,
    );
  }
  const result = await linkConnection(
    db,
    authResult.userId,
    provider.id,
    taken.account,
  );
  const response = apiSuccess(
    { provider: provider.id, status: result.ok ? "linked" : result.reason },
    200,
    request,
  );
  response.headers.set("Cache-Control", "no-store");
  return response;
};

export const Route = createFileRoute("/api/connections/$provider/complete")({
  server: {
    handlers: {
      POST,
    },
  },
});
