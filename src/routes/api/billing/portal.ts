import { createFileRoute } from "@tanstack/react-router";

import { getEnv, requireAuth } from "@/lib/api-helpers";
import { billingDisabled, billingNotImplemented } from "@/lib/billing-routes";
import { billingConfig } from "@/services/billing.service";

/** POST /api/billing/portal: the manage-subscription link. 501 for now. */
export const POST = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  if (!billingConfig(getEnv())) return billingDisabled(request);
  return billingNotImplemented(request);
};

export const Route = createFileRoute("/api/billing/portal")({
  server: { handlers: { POST } },
});
