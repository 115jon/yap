import { createFileRoute } from "@tanstack/react-router";
import * as cloudflareWorkers from "cloudflare:workers";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { getCollectiblesCatalog } from "@/lib/collectibles-catalog";

type WaitUntil = (promise: Promise<unknown>) => void;

/** workerd exports waitUntil from cloudflare:workers; absent in tests/shims. */
function getWaitUntil(): WaitUntil | undefined {
  const candidate = (cloudflareWorkers as { waitUntil?: unknown }).waitUntil;
  return typeof candidate === "function" ? (candidate as WaitUntil) : undefined;
}

/**
 * The cached catalog, for everyone alike. The hourly cron and the
 * off-Cloudflare fetcher keep it current; a stale copy is refreshed in the
 * background. Forcing a refresh lives in the admin console only.
 */
export const GET = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;

  try {
    const catalog = await getCollectiblesCatalog(getDB(), {
      waitUntil: getWaitUntil(),
    });
    return apiSuccess({ catalog }, 200, request);
  } catch {
    return apiError(
      "Unable to load collectibles catalog",
      503,
      "COLLECTIBLES_CATALOG_UNAVAILABLE",
      request,
    );
  }
};

export const Route = createFileRoute("/api/collectibles/catalog")({
  server: {
    handlers: {
      GET,
    },
  },
});
