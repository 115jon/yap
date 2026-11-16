import { createFileRoute } from "@tanstack/react-router";

import { apiError, getDB } from "@/lib/api-helpers";
import { clog } from "@/lib/console-logger";
import { checkRateLimitDO } from "@/lib/rate-limit";
import {
  consumeLinkState,
  holdLinkResult,
  linkConnection,
} from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import {
  finishLink,
  LINK_CALLBACK_LIMIT,
  linkCookieName,
  appFromReturnTo,
  appHint,
  appRedirect,
  readCookie,
} from "@/services/connections/flow";

const log = clog("connections/callback");

type Args = { request: Request; params: { provider?: string } };

// GET /api/connections/:provider/callback — where the provider sends the
// browser back. No Yap session rides along (the app's token is not a
// cookie): the one-time state names the user, and the cookie set by the
// start route (or, for a native app, its begin route) proves this is the
// browser that started. Every outcome redirects into Settings ›
// Connections: the web's, or the native app's by its own link, where a
// verified account is a one-time code for the app to claim.
const GET = async ({ request, params }: Args) => {
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const url = new URL(request.url);
  // Where a failure's outcome goes until the state says (never what links).
  let app = appHint(url);
  const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
  const limited = await checkRateLimitDO(
    `ip:${ip}`,
    "connection-link-callback",
    LINK_CALLBACK_LIMIT,
  );
  if (limited) return finishLink(provider.id, "rate_limited", app);

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
    return finishLink(provider.id, "unavailable", app);
  }
  if (!state.ok) return finishLink(provider.id, state.reason, app);
  app = appFromReturnTo(state.returnTo);

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
    return finishLink(provider.id, verified.reason, app);
  }

  if (app) {
    // A native app's link: nothing links until that app, signed in as the
    // account that started it, claims the code (complete).
    try {
      const code = await holdLinkResult(db, {
        userId: state.userId,
        provider: provider.id,
        account: verified.account,
      });
      return appRedirect(app, provider.id, { code }, true);
    } catch (error) {
      log.error("Couldn't hold the verified account", {
        provider: provider.id,
        error,
      });
      return finishLink(provider.id, "unavailable", app);
    }
  }

  try {
    const result = await linkConnection(
      db,
      state.userId,
      provider.id,
      verified.account,
    );
    return finishLink(provider.id, result.ok ? "linked" : result.reason, app);
  } catch (error) {
    log.error("Couldn't store the connection", {
      provider: provider.id,
      error,
    });
    return finishLink(provider.id, "unavailable", app);
  }
};

export const Route = createFileRoute("/api/connections/$provider/callback")({
  server: {
    handlers: {
      GET,
    },
  },
});
