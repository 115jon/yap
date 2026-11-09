import { createFileRoute } from "@tanstack/react-router";

import { getEnv } from "@/lib/api-helpers";
import { billingDisabled, billingNotImplemented } from "@/lib/billing-routes";
import { billingConfig } from "@/services/billing.service";

/**
 * POST /api/billing/webhook: Stripe's events (no session; Stripe signs the
 * body). A stub: 501 until billing is configured, then 501 until signature
 * checks and event handling (billing.service applySubscription) are built.
 * It never trusts an unsigned body, so it reads nothing yet.
 */
export const POST = async ({ request }: { request: Request }) => {
  if (!billingConfig(getEnv())) return billingDisabled(request);
  return billingNotImplemented(request);
};

export const Route = createFileRoute("/api/billing/webhook")({
  server: { handlers: { POST } },
});
