/**
 * Billing Service: the provider seam for Yap Loud. Stripe is not set up:
 * there are no keys, no account and no Stripe calls here. What exists is the
 * shape the routes and the webhook will fill in:
 *
 *   billingConfig(env)        null until every STRIPE_* secret is set
 *   createCheckout / openPortal   throw BillingNotImplemented for now
 *   applySubscription         the one write a verified webhook makes
 *
 * Until `billingConfig` returns a config, /api/billing/* answers 501
 * (BILLING_DISABLED). The client shows Upgrade disabled ("Coming soon")
 * whenever /api/billing/status is not a 200.
 */

import type { D1Database } from "@cloudflare/workers-types";

import { setEntitlement } from "@/services/entitlement.service";

export interface BillingConfig {
  secretKey: string;
  webhookSecret: string;
  loudPriceId: string;
}

/** The Stripe settings, or null while any of them is missing. */
export function billingConfig(env: unknown): BillingConfig | null {
  const vars = (env ?? {}) as Record<string, unknown>;
  const text = (value: unknown) =>
    typeof value === "string" && value.trim() ? value.trim() : null;
  const secretKey = text(vars.STRIPE_SECRET_KEY);
  const webhookSecret = text(vars.STRIPE_WEBHOOK_SECRET);
  const loudPriceId = text(vars.STRIPE_PRICE_LOUD);
  if (!secretKey || !webhookSecret || !loudPriceId) return null;
  return { secretKey, webhookSecret, loudPriceId };
}

export class BillingNotImplemented extends Error {
  constructor(what: string) {
    super(`${what} is not implemented yet`);
    this.name = "BillingNotImplemented";
  }
}

/** A Checkout session for Yap Loud; returns the URL to send the user to. */
export async function createCheckout(
  _config: BillingConfig,
  _input: { userId: string; returnUrl: string },
): Promise<{ url: string }> {
  throw new BillingNotImplemented("Checkout");
}

/** The customer portal (cancel, change card); returns its URL. */
export async function openPortal(
  _config: BillingConfig,
  _input: { userId: string; returnUrl: string },
): Promise<{ url: string }> {
  throw new BillingNotImplemented("The billing portal");
}

/** A subscription's state, as a verified webhook event reports it. */
export interface SubscriptionChange {
  userId: string;
  customerId: string;
  subscriptionId: string;
  /** Active or trialing: Loud until `currentPeriodEnd`. Otherwise free. */
  active: boolean;
  startedAt: string;
  /** End of the paid period (ISO). */
  currentPeriodEnd: string;
}

/**
 * Applies a subscription change to the user's entitlement. An active one is
 * Loud until the end of its paid period; a lapsed one returns them to free.
 * A founder is never changed (setEntitlement keeps lifetime grants).
 */
export async function applySubscription(
  db: D1Database,
  change: SubscriptionChange,
): Promise<void> {
  await setEntitlement(db, {
    userId: change.userId,
    tier: change.active ? "loud" : "free",
    source: "stripe",
    startedAt: change.startedAt,
    expiresAt: change.currentPeriodEnd,
    stripeCustomerId: change.customerId,
    stripeSubscriptionId: change.active ? change.subscriptionId : null,
  });
}
