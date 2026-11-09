import type { ConnectionProvider } from "@/lib/connections";

/** An external account the provider proved the user owns. */
export type VerifiedAccount = {
  /** The service's stable account id (a SteamID64 for Steam). */
  externalId: string;
  displayName: string;
  profileUrl: string | null;
  avatarUrl: string | null;
  /** Provider data for later consumers, stored as JSON. */
  metadata: Record<string, unknown>;
};

/**
 * Why a callback did not verify. `cancelled`: the user backed out at the
 * provider. `invalid`: the response failed a check (forged, wrong endpoint,
 * mismatched return_to, rejected by the provider). `unavailable`: the
 * provider could not be asked.
 */
export type VerifyFailure = "cancelled" | "invalid" | "unavailable";

export type VerifyResult =
  | { ok: true; account: VerifiedAccount }
  | { ok: false; reason: VerifyFailure; detail?: string };

/**
 * One external service. A module builds the URL the browser is sent to and
 * verifies what the browser comes back with; the routes and the service do
 * the rest (state, uniqueness, storage), so a new provider is one module.
 */
export interface ConnectionProviderModule {
  readonly id: ConnectionProvider;
  /** Where to send the browser; `returnTo` already carries the state. */
  authorizationUrl(input: { returnTo: string; realm: string }): string;
  /** Checks the callback request against the provider itself. */
  verifyCallback(input: {
    url: URL;
    expectedReturnTo: string;
    now?: number;
  }): Promise<VerifyResult>;
}
