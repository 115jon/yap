/**
 * Dev-only fixtures for badges and plans (mounted by UiFixtures):
 *
 *   ?surface=badges  every badge at each size, the overflow rules (wrap in a
 *                    narrow column, "+N" in a tight row), and the badges on a
 *                    profile card at popover and sheet size
 *   ?surface=tiers   the Yap Loud page's tier cards in every state: founder,
 *                    loud, free with billing off ("Coming soon") and free
 *                    with billing on
 *
 * Each block carries a `data-testid` for the captures.
 */

import type { ReactNode } from "react";

import {
  ProfileCard,
  resolveProfileCardTheme,
} from "@/components/chat/ProfileCard";
import { TierCards } from "@/components/chat/settings/TierCards";
import {
  BadgeArt,
  UserBadges,
  type BadgeSize,
} from "@/components/ui/user-badges";
import { BADGES, type UserBadge } from "@/lib/badges";
import type { Entitlement } from "@/lib/entitlements";
import type { User } from "@/lib/types";

/** A fixed clock, so the Loud months and dates never drift in captures. */
const NOW = Date.parse("2026-11-09T12:00:00Z");

const ALL: UserBadge[] = BADGES.map((badge, index) => ({
  id: badge.id,
  awarded_at: new Date(NOW - (index + 1) * 40 * 86_400_000).toISOString(),
}));

const FOUNDER_SET: UserBadge[] = ALL.filter((badge) =>
  ["founder", "early-yapper", "voice-regular", "night-owl"].includes(badge.id),
);

function Section({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: ReactNode;
}) {
  return (
    <section data-testid={id} className="flex flex-col gap-3 p-4">
      <h2 className="silkscreen text-ink-3">{title}</h2>
      {children}
    </section>
  );
}

const SIZES: BadgeSize[] = ["sm", "md", "lg"];

const profileUser: User = {
  id: "u-ada",
  username: "ada",
  display_name: "Ada Lovelace",
  pronouns: "she/her",
  status: "online",
  custom_status: "compiling the analytical engine",
  badges: FOUNDER_SET,
};

export function BadgesFixture() {
  const theme = resolveProfileCardTheme(profileUser);
  return (
    <div className="h-full overflow-y-auto bg-ply-0">
      <Section id="badges-gallery" title="Badges">
        <table className="w-fit border-separate border-spacing-x-4 border-spacing-y-2 text-ui-sm text-ink-2">
          <tbody>
            {BADGES.map((badge) => (
              <tr key={badge.id} data-badge-row={badge.id}>
                {SIZES.map((size) => (
                  <td key={size} className="align-middle">
                    <BadgeArt id={badge.id} size={size} />
                  </td>
                ))}
                <td className="align-middle">
                  <span className="font-medium text-ink-1">{badge.name}</span>
                  <span className="ms-2">{badge.description}</span>
                  {badge.hidden ? (
                    <span className="silkscreen ms-2 text-ink-3">Hidden</span>
                  ) : null}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </Section>

      <Section id="badges-on-plies" title="On each ply">
        <div className="flex flex-wrap gap-2">
          {(["bg-ply-0", "bg-ply-1", "bg-ply-2", "bg-ply-3"] as const).map(
            (ply) => (
              <div
                key={ply}
                className={`${ply} rounded-[2px] border border-line p-3`}
              >
                <UserBadges badges={ALL} size="md" now={NOW} />
              </div>
            ),
          )}
        </div>
      </Section>

      <Section id="badges-overflow" title="Overflow: wrap and +N">
        <div className="flex flex-wrap items-start gap-6">
          <div className="flex flex-col gap-2">
            <span className="text-ui-xs text-ink-3">Wraps at 120px</span>
            <div className="w-[120px] rounded-[2px] border border-dashed border-line-strong p-1">
              <UserBadges badges={ALL} size="md" now={NOW} />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-ui-xs text-ink-3">maxVisible 4</span>
            <div className="w-fit rounded-[2px] border border-dashed border-line-strong p-1">
              <UserBadges badges={ALL} size="md" maxVisible={4} now={NOW} />
            </div>
          </div>
          <div className="flex flex-col gap-2">
            <span className="text-ui-xs text-ink-3">sm, maxVisible 3</span>
            <div className="w-fit rounded-[2px] border border-dashed border-line-strong p-1">
              <UserBadges badges={ALL} size="sm" maxVisible={3} now={NOW} />
            </div>
          </div>
        </div>
      </Section>

      <Section id="badges-profile" title="On a profile">
        <div className="flex flex-wrap items-start gap-6">
          <ProfileCard
            size="popover"
            user={profileUser}
            theme={theme}
            status="online"
            playAnimation={false}
            className="w-[300px]"
          />
          <ProfileCard
            size="sheet"
            user={{ ...profileUser, badges: ALL }}
            theme={theme}
            status="online"
            playAnimation={false}
            className="w-[360px]"
          />
        </div>
      </Section>
    </div>
  );
}

const STATES: Array<{
  id: string;
  title: string;
  entitlement: Entitlement;
  billingEnabled: boolean;
}> = [
  {
    id: "tiers-founder",
    title: "Founder",
    entitlement: {
      tier: "founder",
      source: "grant",
      started_at: "2026-11-09T00:00:00Z",
      expires_at: null,
    },
    billingEnabled: false,
  },
  {
    id: "tiers-loud",
    title: "Yap Loud",
    entitlement: {
      tier: "loud",
      source: "stripe",
      started_at: "2026-09-01T00:00:00Z",
      expires_at: "2026-12-01T00:00:00Z",
    },
    billingEnabled: true,
  },
  {
    id: "tiers-free",
    title: "Free, billing off",
    entitlement: { tier: "free" },
    billingEnabled: false,
  },
  {
    id: "tiers-free-billing",
    title: "Free, billing on",
    entitlement: { tier: "free" },
    billingEnabled: true,
  },
];

export function TiersFixture() {
  return (
    <div className="h-full overflow-y-auto bg-ply-0">
      {STATES.map((state) => (
        <Section key={state.id} id={state.id} title={state.title}>
          <div className="@container max-w-[880px]">
            <TierCards
              entitlement={state.entitlement}
              billingEnabled={state.billingEnabled}
              now={NOW}
            />
          </div>
        </Section>
      ))}
      <Section id="tiers-narrow" title="Narrow (phone)">
        <div className="@container w-[360px]">
          <TierCards
            entitlement={{ tier: "free" }}
            billingEnabled={false}
            now={NOW}
          />
        </div>
      </Section>
    </div>
  );
}
