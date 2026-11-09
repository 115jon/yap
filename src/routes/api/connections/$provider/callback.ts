import { createFileRoute } from "@tanstack/react-router";

import { apiError, getDB } from "@/lib/api-helpers";
import { clog } from "@/lib/console-logger";
import { checkRateLimitDO } from "@/lib/rate-limit";
import {
  consumeLinkState,
  linkConnection,
} from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import {
  finishLink,
  LINK_CALLBACK_LIMIT,
  linkCookieName,
  readCookie,
} from "@/services/connections/flow";

const log = clog("connections/callback");

type Args = { request: Request; params: { provider?: string } };

// GET /api/connections/:provider/callback — where the provider sends the
// browser back. No Yap session rides along (the app's token is not a
// cookie): the one-time state names the user, and the cookie set by the
// start route proves this is the browser that started. Every outcome
// redirects into Settings › Connections.
const GET = async ({ request, params }: Args) => {
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
  const limited = await checkRateLimitDO(
    `ip:${ip}`,
    "connection-link-callback",
    LINK_CALLBACK_LIMIT,
  );
  if (limited) return finishLink(provider.id, "rate_limited");

  const url = new URL(request.url);
  const db = getDB();
  let state: Awaited<ReturnType<typeof consumeLinkState>>;
  try {
    state = await consumeLinkState(db, {
      provider: provider.id,
      nonce: url.searchParams.get("state"),
      binding: readCookie(request, linkCookieName(provider.id)),
    });
  } catch (error) {
    log.error("Couldn't read the link state", { provider: provider.id, error });
    return finishLink(provider.id, "unavailable");
  }
  if (!state.ok) return finishLink(provider.id, state.reason);

  const verified = await provider.verifyCallback({
    url,
    expectedReturnTo: state.returnTo,
  });
  if (!verified.ok) {
    if (verified.reason !== "cancelled") {
      log.warn("Link not verified", {
        provider: provider.id,
        reason: verified.reason,
        detail: verified.detail,
      });
    }
    return finishLink(provider.id, verified.reason);
  }

  try {
    const result = await linkConnection(
      db,
      state.userId,
      provider.id,
      verified.account,
    );
    return finishLink(provider.id, result.ok ? "linked" : result.reason);
  } catch (error) {
    log.error("Couldn't store the connection", {
      provider: provider.id,
      error,
    });
    return finishLink(provider.id, "unavailable");
  }
};

export const Route = createFileRoute("/api/connections/$provider/callback")({
  server: {
    handlers: {
      GET,
    },
  },
});
