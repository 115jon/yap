/**
 * Entitlement Service: reads and writes `user_entitlements`. No row means
 * free. Billing (services/billing.service.ts) and admin grants write through
 * `setEntitlement`; feature code reads `getEntitlement` and asks `hasPerk`.
 */

import type { D1Database } from "@cloudflare/workers-types";

import { profileBadges, type UserBadge } from "@/lib/badges";
import {
  FREE_ENTITLEMENT,
  parseEntitlement,
  type Entitlement,
  type EntitlementSource,
  type Tier,
} from "@/lib/entitlements";
import { uploadLimitFor, type UploadKind } from "@/lib/upload-limits";

type EntitlementRow = {
  tier: string;
  source: string;
  started_at: string;
  expires_at: string | null;
};

const SELECT_ENTITLEMENT = `SELECT tier, source, started_at, expires_at FROM user_entitlements WHERE user_id = ?`;
const SELECT_BADGES = `SELECT badge_id AS id, awarded_at FROM user_badges WHERE user_id = ?`;

export async function getEntitlement(
  db: D1Database,
  userId: string,
): Promise<Entitlement> {
  const row = await db
    .prepare(SELECT_ENTITLEMENT)
    .bind(userId)
    .first<EntitlementRow>();
  return row ? parseEntitlement(row) : FREE_ENTITLEMENT;
}

/**
 * The single-request limit for `kind` under the caller's plan, read here and
 * never from the client. The plan is only looked up for a file over the free
 * limit (most are not); if it cannot be read, the free limit applies.
 */
export async function uploadLimitForUser(
  db: D1Database,
  userId: string,
  kind: UploadKind,
  size: number,
): Promise<number> {
  const free = uploadLimitFor(FREE_ENTITLEMENT, kind);
  if (size <= free) return free;
  const entitlement = await getEntitlement(db, userId).catch(
    () => FREE_ENTITLEMENT,
  );
  return uploadLimitFor(entitlement, kind);
}

/**
 * What a profile carries besides the `users` row: the plan and the badges
 * (stored awards plus Loud while the tier is Loud), in one round trip.
 */
export async function getProfileExtras(
  db: D1Database,
  userId: string,
): Promise<{ entitlement: Entitlement; badges: UserBadge[] }> {
  const [entitlementResult, badgeResult] = await db.batch([
    db.prepare(SELECT_ENTITLEMENT).bind(userId),
    db.prepare(SELECT_BADGES).bind(userId),
  ]);
  const row = (entitlementResult?.results?.[0] ??
    null) as EntitlementRow | null;
  const entitlement = row ? parseEntitlement(row) : FREE_ENTITLEMENT;
  const stored = ((badgeResult?.results ?? []) as UserBadge[]).filter(
    (badge) => typeof badge.id === "string",
  );
  return { entitlement, badges: profileBadges(stored, entitlement) };
}

export interface EntitlementWrite {
  userId: string;
  tier: Tier;
  source: EntitlementSource;
  startedAt: string;
  expiresAt?: string | null;
  stripeCustomerId?: string | null;
  stripeSubscriptionId?: string | null;
  now?: string;
}

/**
 * Sets a user's plan (insert or replace). A founder stays a founder: a
 * Stripe event never demotes a lifetime grant.
 */
export async function setEntitlement(
  db: D1Database,
  write: EntitlementWrite,
): Promise<void> {
  const now = write.now ?? new Date().toISOString();
  await db
    .prepare(
      `INSERT INTO user_entitlements
         (user_id, tier, source, started_at, expires_at, stripe_customer_id, stripe_subscription_id, updated_at)
       VALUES (?, ?, ?, ?, ?, ?, ?, ?)
       ON CONFLICT(user_id) DO UPDATE SET
         tier = excluded.tier,
         source = excluded.source,
         started_at = excluded.started_at,
         expires_at = excluded.expires_at,
         stripe_customer_id = COALESCE(excluded.stripe_customer_id, user_entitlements.stripe_customer_id),
         stripe_subscription_id = excluded.stripe_subscription_id,
         updated_at = excluded.updated_at
       WHERE user_entitlements.tier <> 'founder' OR excluded.tier = 'founder'`,
    )
    .bind(
      write.userId,
      write.tier,
      write.source,
      write.startedAt,
      write.expiresAt ?? null,
      write.stripeCustomerId ?? null,
      write.stripeSubscriptionId ?? null,
      now,
    )
    .run();
}
