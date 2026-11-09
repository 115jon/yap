/**
 * The three plans, side by side (stacked on a narrow page): Yap, Yap Loud
 * and Founder, from `TIER_PLANS`. The card for the plan in force says so.
 *
 *   founder  "Founder · lifetime" on its card; no purchase buttons anywhere
 *   loud     its card is the plan; Manage (disabled until billing ships)
 *   free     Upgrade on the Loud card, disabled with "Coming soon" until
 *            billing is enabled
 *
 * The Founder card shows for everyone (what founders got); it is never for
 * sale.
 *
 * Presentational: the panel passes the entitlement and whether billing is
 * on, so the fixtures and tests can show every state.
 */

import { Button } from "@/components/ui/button";
import { Icon } from "@/components/ui/icon";
import { BadgeArt } from "@/components/ui/user-badges";
import { YapBark } from "@/components/ui/yap-bark";
import {
  effectiveTier,
  TIER_PLANS,
  type Entitlement,
  type Tier,
  type TierPlan,
  UPLOADS_FEATURE,
} from "@/lib/entitlements";
import { formatProfileReferenceDate } from "@/lib/profile-dates";
import { clientUploadLimitFor, formatUploadLimit } from "@/lib/upload-limits";
import { cn } from "@/lib/utils";

/**
 * A plan's feature line as the card prints it: the uploads line carries the
 * size that plan is held to today, never more than the server takes.
 */
function featureText(feature: string, tier: Tier): string {
  if (feature !== UPLOADS_FEATURE) return feature;
  const limit = clientUploadLimitFor({ tier }, "attachment");
  return `${formatUploadLimit(limit)} uploads`;
}

function PlanMark({ tier }: { tier: Tier }) {
  if (tier === "free") return <YapBark cell={1} />;
  return <BadgeArt id={tier === "founder" ? "founder" : "loud"} size="lg" />;
}

/** The status line on the card of the plan in force. */
function currentLine(entitlement: Entitlement, tier: Tier): string {
  if (tier === "founder") return "Founder · lifetime";
  if (tier === "loud") {
    const renews = formatProfileReferenceDate(entitlement.expires_at ?? null);
    return renews ? `Your plan · renews ${renews}` : "Your plan";
  }
  return "Your plan";
}

function PlanAction({
  plan,
  tier,
  billingEnabled,
  onUpgrade,
  onManage,
}: {
  plan: TierPlan;
  tier: Tier;
  billingEnabled: boolean;
  onUpgrade?: () => void;
  onManage?: () => void;
}) {
  // Founders have everything for life: nothing to buy, nothing to manage.
  if (tier === "founder") return null;
  if (plan.tier === "loud" && tier === "free") {
    return (
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Button
          variant="primary"
          size="md"
          disabled={!billingEnabled}
          onClick={onUpgrade}
        >
          Upgrade
        </Button>
        {billingEnabled ? null : (
          <span className="silkscreen text-ink-3">Coming soon</span>
        )}
      </div>
    );
  }
  if (plan.tier === "loud" && tier === "loud") {
    return (
      <div className="flex flex-wrap items-center gap-x-3 gap-y-2">
        <Button
          variant="secondary"
          size="md"
          disabled={!billingEnabled}
          onClick={onManage}
        >
          Manage
        </Button>
        {billingEnabled ? null : (
          <span className="silkscreen text-ink-3">Coming soon</span>
        )}
      </div>
    );
  }
  return null;
}

export function TierCards({
  entitlement,
  billingEnabled,
  onUpgrade,
  onManage,
  now,
  className,
}: {
  entitlement: Entitlement;
  billingEnabled: boolean;
  onUpgrade?: () => void;
  onManage?: () => void;
  /** For fixtures and tests: the clock an expiry is read against. */
  now?: number;
  className?: string;
}) {
  const tier = effectiveTier(entitlement, now);
  return (
    <div
      data-slot="tier-cards"
      data-tier={tier}
      className={cn(
        "grid grid-cols-1 gap-3 @min-[720px]:grid-cols-3",
        className,
      )}
    >
      {TIER_PLANS.map((plan) => {
        const current = plan.tier === tier;
        // Founders have everything Loud has: say so on the Loud card.
        const included = tier === "founder" && plan.tier !== "founder";
        return (
          <article
            key={plan.tier}
            data-plan={plan.tier}
            data-current={current || undefined}
            aria-label={current ? `${plan.name}, your plan` : plan.name}
            className={cn(
              "flex min-w-0 flex-col gap-4 rounded-[2px] border bg-ply-2 p-4",
              current ? "border-accent-ink" : "border-line",
            )}
          >
            <header className="flex items-start justify-between gap-3">
              <div className="flex min-w-0 flex-col gap-1">
                <span
                  className={cn(
                    "silkscreen",
                    current ? "text-accent-ink" : "text-ink-3",
                  )}
                >
                  {current
                    ? currentLine(entitlement, tier)
                    : included
                      ? "Included"
                      : plan.tier === "founder"
                        ? "Before launch"
                        : plan.price}
                </span>
                <h4 className="font-display text-[20px] leading-6 font-bold text-balance text-ink-1">
                  {plan.name}
                </h4>
                {current || included || plan.tier === "founder" ? (
                  <span className="text-num-sm text-ink-2">{plan.price}</span>
                ) : null}
              </div>
              <PlanMark tier={plan.tier} />
            </header>
            <p className="text-ui-sm text-pretty text-ink-2">{plan.blurb}</p>
            <ul className="flex flex-1 flex-col gap-2">
              {plan.features.map((feature) => (
                <li
                  key={feature}
                  className="flex items-start gap-2 text-ui-sm text-ink-1"
                >
                  <Icon
                    name="check"
                    size="sm"
                    className={cn(
                      "mt-0.5 shrink-0",
                      current ? "text-accent-ink" : "text-ink-3",
                    )}
                  />
                  <span className="min-w-0">
                    {featureText(feature, plan.tier)}
                  </span>
                </li>
              ))}
            </ul>
            <PlanAction
              plan={plan}
              tier={tier}
              billingEnabled={billingEnabled}
              onUpgrade={onUpgrade}
              onManage={onManage}
            />
          </article>
        );
      })}
    </div>
  );
}
