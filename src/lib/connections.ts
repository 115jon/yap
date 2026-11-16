/**
 * Connections: verified links to accounts on other services, shown on the
 * profile. Shared by the Worker (services/connection.service.ts and the
 * provider modules in services/connections/) and the client (Settings ›
 * Connections, the profile's Connections block). Keep it dependency-free.
 *
 * Adding a provider: a module in services/connections/, an entry here, and
 * an icon concept in components/ui/icon/mapping.ts.
 */

import type { IconName } from "@/components/ui/icon/mapping";

export const CONNECTION_PROVIDERS = ["steam"] as const;

export type ConnectionProvider = (typeof CONNECTION_PROVIDERS)[number];

export function isConnectionProvider(
  value: unknown,
): value is ConnectionProvider {
  return (
    typeof value === "string" &&
    (CONNECTION_PROVIDERS as readonly string[]).includes(value)
  );
}

/** How the client names and draws a provider. */
export const CONNECTION_PROVIDER_INFO: Record<
  ConnectionProvider,
  {
    label: string;
    icon: IconName;
    /** The provider card's description in Settings › Connections. */
    blurb: string;
    /** Settings search words. */
    keywords: string[];
  }
> = {
  steam: {
    label: "Steam",
    icon: "steam",
    blurb:
      "Sign in through Steam to prove the account is yours. Yap never sees your Steam password.",
    keywords: ["steam", "valve", "game", "games", "deadlock", "pc"],
  },
};

/**
 * What anyone who can open the profile sees: no external id beyond what the
 * profile URL itself shows. Only connections with "Show on profile" on.
 */
export type PublicConnection = {
  provider: ConnectionProvider;
  display_name: string;
  profile_url: string | null;
  avatar_url: string | null;
};

/** The connections in an API answer; anything malformed is dropped. */
export function parsePublicConnections(value: unknown): PublicConnection[] {
  if (!Array.isArray(value)) return [];
  const nullableString = (field: unknown) =>
    typeof field === "string" ? field : null;
  return value.flatMap((item): PublicConnection[] => {
    if (!item || typeof item !== "object") return [];
    const record = item as Record<string, unknown>;
    if (
      !isConnectionProvider(record.provider) ||
      typeof record.display_name !== "string"
    ) {
      return [];
    }
    return [
      {
        provider: record.provider,
        display_name: record.display_name,
        profile_url: nullableString(record.profile_url),
        avatar_url: nullableString(record.avatar_url),
      },
    ];
  });
}

/** The owner's own view (Settings › Connections, GET /api/connections). */
export type OwnConnection = PublicConnection & {
  show_on_profile: boolean;
  verified_at: string;
};

/**
 * The outcome a provider callback lands back in the app with
 * (`/chat?connection=<provider>&connection_status=<status>`).
 */
export const CONNECTION_RESULT_STATUSES = [
  "linked",
  "cancelled",
  "taken",
  "expired",
  "invalid",
  "unavailable",
  "rate_limited",
] as const;

export type ConnectionResultStatus =
  (typeof CONNECTION_RESULT_STATUSES)[number];

export function isConnectionResultStatus(
  value: unknown,
): value is ConnectionResultStatus {
  return (
    typeof value === "string" &&
    (CONNECTION_RESULT_STATUSES as readonly string[]).includes(value)
  );
}

/**
 * The native apps that can start a link: the Windows app (native/) and the
 * Android app (native/mobile). Their session is not a browser cookie, so
 * the link runs in the system browser (Custom Tabs on Android) and the
 * callback hands the outcome back by the app's own link.
 */
export const CONNECTION_APPS = ["windows", "android"] as const;

export type ConnectionApp = (typeof CONNECTION_APPS)[number];

export function isConnectionApp(value: unknown): value is ConnectionApp {
  return (
    typeof value === "string" &&
    (CONNECTION_APPS as readonly string[]).includes(value)
  );
}

/**
 * The app link a native link ends with: `yap-native://connection?…` for
 * the Windows app (the web never makes `yap://` links there, which the
 * Tauri app may still own), `yap://connection?…` for the Android app.
 * A verified account comes back as a one-time `code`, which links nothing
 * until the app that started the link claims it with its own session
 * (POST /api/connections/:provider/complete); anything else as `status`.
 * Never an account or a token.
 */
export function connectionAppLink(
  app: ConnectionApp,
  provider: ConnectionProvider,
  outcome: { status: ConnectionResultStatus } | { code: string },
): string {
  const scheme = app === "windows" ? "yap-native" : "yap";
  const query = new URLSearchParams(
    "code" in outcome
      ? { provider, code: outcome.code }
      : { provider, status: outcome.status },
  );
  return `${scheme}://connection?${query.toString()}`;
}

/** The query keys the callback's redirect carries. */
export const CONNECTION_RESULT_PARAM = "connection";
export const CONNECTION_STATUS_PARAM = "connection_status";

/** The toast for a callback outcome. */
export function describeConnectionResult(
  provider: ConnectionProvider,
  status: ConnectionResultStatus,
): { tone: "success" | "info" | "error"; message: string } {
  const label = CONNECTION_PROVIDER_INFO[provider].label;
  switch (status) {
    case "linked":
      return { tone: "success", message: `${label} connected` };
    case "cancelled":
      return {
        tone: "info",
        message: `${label} wasn't connected. You cancelled the sign-in.`,
      };
    case "taken":
      return {
        tone: "error",
        message: `That ${label} account is already connected to another Yap account.`,
      };
    case "expired":
      return {
        tone: "error",
        message: `The ${label} sign-in expired. Start again from Connections.`,
      };
    case "unavailable":
      return {
        tone: "error",
        message: `Couldn't reach ${label}. Try again in a few minutes.`,
      };
    case "rate_limited":
      return {
        tone: "error",
        message: `Too many tries. Wait a few minutes, then connect ${label} again.`,
      };
    case "invalid":
      return {
        tone: "error",
        message: `${label} didn't confirm the sign-in. Try connecting again.`,
      };
  }
}
