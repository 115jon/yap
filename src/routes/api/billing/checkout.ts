import { createFileRoute } from "@tanstack/react-router";

import { getEnv, requireAuth } from "@/lib/api-helpers";
import { billingDisabled, billingNotImplemented } from "@/lib/billing-routes";
import { billingConfig } from "@/services/billing.service";

/**
 * POST /api/billing/checkout: starts a Yap Loud checkout. 501 until billing
 * is configured, and 501 after that until checkout is built.
 */
export const POST = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  if (!billingConfig(getEnv())) return billingDisabled(request);
  return billingNotImplemented(request);
};

export const Route = createFileRoute("/api/billing/checkout")({
  server: { handlers: { POST } },
});
