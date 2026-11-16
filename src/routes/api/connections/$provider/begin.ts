import { createFileRoute } from "@tanstack/react-router";

import { apiError, getDB } from "@/lib/api-helpers";
import { isConnectionApp } from "@/lib/connections";
import { clog } from "@/lib/console-logger";
import { checkRateLimitDO } from "@/lib/rate-limit";
import { checkLinkBinding } from "@/services/connection.service";
import { getProviderModule } from "@/services/connections";
import {
  appRedirect,
  LINK_BEGIN_LIMIT,
  linkCookie,
  linkOrigin,
} from "@/services/connections/flow";

const log = clog("connections/begin");

type Args = { request: Request; params: { provider?: string } };

// GET /api/connections/:provider/begin?state=…&binding=…&app=… — where a
// native app sends the system browser (POST start with `app`). No session
// rides along: the state names the user. The binding in the URL becomes
// this browser's cookie, so the callback proves it is the browser that
// began; then the browser goes on to the provider. It may be loaded more
// than once (some browsers fetch a link twice). Opening it links nothing:
// the callback hands the app a code that only the account that started the
// link can claim. A begin that fails goes back to the app, leaving any
// cookie of a link already in flight alone.
const GET = async ({ request, params }: Args) => {
  const provider = getProviderModule(params.provider);
  if (!provider) {
    return apiError("Unknown connection", 404, "UNKNOWN_PROVIDER", request);
  }
  const url = new URL(request.url);
  const requested = url.searchParams.get("app");
  const app = isConnectionApp(requested) ? requested : null;
  if (!app) {
    return apiError("Unknown app", 400, "INVALID_APP", request);
  }
  const ip = request.headers.get("cf-connecting-ip") ?? "unknown";
  const limited = await checkRateLimitDO(
    `ip:${ip}`,
    "connection-link-begin",
    LINK_BEGIN_LIMIT,
  );
  if (limited) return appRedirect(app, provider.id, { status: "rate_limited" });

  const binding = url.searchParams.get("binding") ?? "";
  let checked: Awaited<ReturnType<typeof checkLinkBinding>>;
  try {
    checked = await checkLinkBinding(getDB(), {
      provider: provider.id,
      nonce: url.searchParams.get("state"),
      binding,
    });
  } catch (error) {
    log.error("Couldn't read the link state", { provider: provider.id, error });
    return appRedirect(app, provider.id, { status: "unavailable" });
  }
  if (!checked.ok)
    return appRedirect(app, provider.id, { status: checked.reason });

  const headers = new Headers({
    Location: provider.authorizationUrl({
      returnTo: checked.returnTo,
      realm: linkOrigin(request),
    }),
    "Cache-Control": "no-store",
    "Referrer-Policy": "no-referrer",
  });
  headers.append("Set-Cookie", linkCookie(provider.id, binding));
  return new Response(null, { status: 303, headers });
};

export const Route = createFileRoute("/api/connections/$provider/begin")({
  server: {
    handlers: {
      GET,
    },
  },
});
