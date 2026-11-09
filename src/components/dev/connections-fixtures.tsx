/**
 * Dev-only fixtures for profile connections (mounted by UiFixtures):
 *
 *   ?surface=settings-connections&state=empty|linked|hidden|error
 *       the real Settings › Connections panel, its /api/connections answer
 *       stubbed by `installConnectionsStub` (no network, no Steam)
 *   ?surface=connections-profile
 *       the profile's Connections block on a card at popover, modal and
 *       sheet size, plus a card with nothing shown
 *
 * Each block carries a `data-testid` for the captures.
 */

import type { ReactNode } from "react";

import {
  ProfileCard,
  ProfileSections,
  resolveProfileCardTheme,
} from "@/components/chat/ProfileCard";
import avatarAda from "@/components/dev/assets/avatar-ada.webp";
import type { OwnConnection, PublicConnection } from "@/lib/connections";
import {
  clearStoredKovaAuthSessionToken,
  getStoredKovaAuthSessionToken,
  setStoredKovaAuthSessionToken,
} from "@/lib/desktop-auth";
import type { User } from "@/lib/types";

const STEAM: OwnConnection = {
  provider: "steam",
  display_name: "ada.lovelace",
  profile_url: "https://steamcommunity.com/profiles/76561197960287930",
  avatar_url: avatarAda,
  show_on_profile: true,
  verified_at: "2026-11-09T12:00:00.000Z",
};

export type ConnectionsFixtureState = "empty" | "linked" | "hidden" | "error";

export function connectionsFixtureState(
  raw: string | null,
): ConnectionsFixtureState {
  return raw === "linked" || raw === "hidden" || raw === "error"
    ? raw
    : "empty";
}

/**
 * Answers the panel's requests from the state above while the fixture is
 * mounted (start answers a Steam URL the fixture never follows). Every
 * other request passes through. Returns the uninstaller.
 */
export function installConnectionsStub(state: ConnectionsFixtureState) {
  const placeholderSession = getStoredKovaAuthSessionToken()
    ? null
    : "ui-fixture-session";
  if (placeholderSession) setStoredKovaAuthSessionToken(placeholderSession);
  const original = window.fetch;
  window.fetch = async (input, init) => {
    const url =
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url;
    if (url.includes("/api/connections")) {
      const method = (init?.method ?? "GET").toUpperCase();
      if (method === "GET" && state === "error") {
        return new Response(JSON.stringify({ error: "Service unavailable" }), {
          status: 503,
          headers: { "Content-Type": "application/json" },
        });
      }
      const body =
        method === "GET"
          ? {
              connections:
                state === "linked"
                  ? [STEAM]
                  : state === "hidden"
                    ? [{ ...STEAM, show_on_profile: false }]
                    : [],
            }
          : { ok: true };
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    }
    return original(input, init);
  };
  return () => {
    window.fetch = original;
    if (placeholderSession) clearStoredKovaAuthSessionToken();
  };
}

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

const SHOWN: PublicConnection[] = [
  {
    provider: STEAM.provider,
    display_name: STEAM.display_name,
    profile_url: STEAM.profile_url,
    avatar_url: STEAM.avatar_url,
  },
];

const LONG_NAME: PublicConnection[] = [
  {
    ...SHOWN[0]!,
    display_name: "a-very-long-steam-persona-name-that-keeps-going-on",
  },
];

const profileUser: User = {
  id: "u-ada",
  username: "ada",
  display_name: "Ada Lovelace",
  pronouns: "she/her",
  status: "online",
  custom_status: "compiling the analytical engine",
  bio: "First programmer. Plays Deadlock badly.",
  connections: SHOWN,
};

function Card({
  size,
  user,
  className,
}: {
  size: "popover" | "modal" | "sheet";
  user: User;
  className: string;
}) {
  return (
    <ProfileCard
      size={size}
      user={user}
      theme={resolveProfileCardTheme(user)}
      status="online"
      playAnimation={false}
      className={className}
    >
      <ProfileSections
        bio={user.bio}
        connections={user.connections}
        memberSince={{ label: "Member since", value: "Nov 9, 2026" }}
      />
    </ProfileCard>
  );
}

export function ConnectionsProfileFixture() {
  return (
    <div className="h-full overflow-y-auto bg-ply-0">
      <Section id="connections-profile-cards" title="Connections on a profile">
        <div className="flex flex-wrap items-start gap-6">
          <Card size="popover" user={profileUser} className="w-[340px]" />
          <Card
            size="sheet"
            user={{ ...profileUser, connections: LONG_NAME }}
            className="w-full max-w-[390px]"
          />
          <Card
            size="popover"
            user={{ ...profileUser, connections: [] }}
            className="w-[340px]"
          />
        </div>
      </Section>
      <Section id="connections-profile-modal" title="Full profile">
        <Card
          size="modal"
          user={profileUser}
          className="w-full max-w-[640px]"
        />
      </Section>
    </div>
  );
}
