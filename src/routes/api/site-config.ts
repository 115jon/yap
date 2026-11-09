import { createFileRoute } from "@tanstack/react-router";

import { apiSuccess } from "@/lib/api-helpers";
import { publicSiteConfig } from "@/services/site-flags.service";

/**
 * GET /api/site-config: what every client needs from the site switches, the
 * switches themselves and the announcement banner while it runs. Public and
 * the same for everyone (no account data), so it may be cached briefly.
 */
export const GET = async ({ request }: { request: Request }) => {
  const response = apiSuccess(await publicSiteConfig(), 200, request);
  response.headers.set("Cache-Control", "public, max-age=30");
  return response;
};

export const Route = createFileRoute("/api/site-config")({
  server: {
    handlers: {
      GET,
    },
  },
});
