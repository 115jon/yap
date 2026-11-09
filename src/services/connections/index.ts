import {
  isConnectionProvider,
  type ConnectionProvider,
} from "@/lib/connections";

import { steamProvider } from "./steam";
import type { ConnectionProviderModule } from "./types";

export type { ConnectionProviderModule, VerifiedAccount } from "./types";

const PROVIDERS: Record<ConnectionProvider, ConnectionProviderModule> = {
  steam: steamProvider,
};

/** The module for a route's `$provider`, or null for anything unknown. */
export function getProviderModule(
  provider: unknown,
): ConnectionProviderModule | null {
  return isConnectionProvider(provider) ? PROVIDERS[provider] : null;
}
