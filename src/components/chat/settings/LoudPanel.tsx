import { useEffect, useState } from "react";

import { useMyEntitlement } from "@/hooks/useEntitlement";
import { apiGet } from "@/lib/api-client";
import { effectiveTier, TIER_NAMES, type Tier } from "@/lib/entitlements";
import { useChatStore } from "@/stores/chat-store";
import { UserBadges } from "@/components/ui/user-badges";

import { SettingsCard, SettingsRow, ValueText } from "./primitives";
import { TierCards } from "./TierCards";

/**
 * Whether billing is on: /api/billing/status answers 200 once Stripe is set
 * up and 501 until then. Anything but a 200 (offline, an old server) keeps
 * the purchase buttons disabled.
 */
function useBillingEnabled(): boolean {
  const [enabled, setEnabled] = useState(false);
  useEffect(() => {
    let live = true;
    apiGet<{ enabled?: boolean }>("/api/billing/status")
      .then((status) => {
        if (live) setEnabled(status?.enabled === true);
      })
      .catch(() => {
        if (live) setEnabled(false);
      });
    return () => {
      live = false;
    };
  }, []);
  return enabled;
}

const PLAN_DESCRIPTION: Record<Tier, string> = {
  founder: "Here before launch: everything in Yap Loud, for life.",
  loud: "Bigger uploads, HD streams and every name style.",
  free: "The free plan. Yap Loud adds bigger uploads and HD streams.",
};

/** Settings > Yap Loud: the plan in force and the three plans. */
export default function LoudPanel() {
  const entitlement = useMyEntitlement();
  const badges = useChatStore((state) => state.user?.badges);
  const billingEnabled = useBillingEnabled();
  const tier = effectiveTier(entitlement);
  const planBadges = (badges ?? []).filter(
    (badge) => badge.id === "founder" || badge.id === "loud",
  );

  return (
    <>
      <SettingsCard id="your-plan">
        <SettingsRow
          id="current-plan"
          layout="inline"
          description={PLAN_DESCRIPTION[tier]}
        >
          {() => (
            <span className="flex items-center gap-2">
              <UserBadges badges={planBadges} size="md" />
              <ValueText>
                {tier === "founder" ? "Founder · lifetime" : TIER_NAMES[tier]}
              </ValueText>
            </span>
          )}
        </SettingsRow>
      </SettingsCard>
      <SettingsCard id="plans">
        <li className="list-none py-4">
          <TierCards
            entitlement={entitlement}
            billingEnabled={billingEnabled}
          />
        </li>
      </SettingsCard>
    </>
  );
}
