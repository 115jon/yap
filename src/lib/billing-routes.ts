/**
 * Shared answers for /api/billing/*: 501 until billing is configured, and
 * 501 for the parts that are configured but not built yet.
 */

import { apiError } from "@/lib/api-helpers";

export const BILLING_DISABLED_CODE = "BILLING_DISABLED";
export const BILLING_NOT_IMPLEMENTED_CODE = "BILLING_NOT_IMPLEMENTED";

export function billingDisabled(request: Request): Response {
  return apiError(
    "Yap Loud isn't on sale yet.",
    501,
    BILLING_DISABLED_CODE,
    request,
  );
}

export function billingNotImplemented(request: Request): Response {
  return apiError(
    "Yap Loud checkout isn't built yet.",
    501,
    BILLING_NOT_IMPLEMENTED_CODE,
    request,
  );
}
