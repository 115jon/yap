import { createFileRoute } from "@tanstack/react-router";

import { apiSuccess, getEnv, requireAuth } from "@/lib/api-helpers";
import { billingDisabled } from "@/lib/billing-routes";
import { billingConfig } from "@/services/billing.service";

/**
 * GET /api/billing/status: 200 { enabled: true } once billing is configured,
 * 501 BILLING_DISABLED until then. The Yap Loud page keeps Upgrade disabled
 * ("Coming soon") on anything but a 200.
 */
export const GET = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  if (!billingConfig(getEnv())) return billingDisabled(request);
  return apiSuccess({ enabled: true }, 200, request);
};

export const Route = createFileRoute("/api/billing/status")({
  server: { handlers: { GET } },
});
