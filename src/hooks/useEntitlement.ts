import { useMemo } from "react";

import {
  FREE_ENTITLEMENT,
  hasPerk,
  type Entitlement,
  type Perk,
} from "@/lib/entitlements";
import { getAvailableStreamQualities } from "@/lib/voice/utils";
import { useChatStore } from "@/stores/chat-store";

/**
 * The signed-in user's plan (from users/me). Free until it loads. The
 * client hides and labels; the server enforces upload size from its own
 * read, while HD stream gating is client-side only for now (entitlements.ts).
 */
export function useMyEntitlement(): Entitlement {
  return useChatStore((state) => state.user?.entitlement ?? FREE_ENTITLEMENT);
}

/** Whether the signed-in user has `perk`. */
export function useHasPerk(perk: Perk): boolean {
  const entitlement = useMyEntitlement();
  return hasPerk(entitlement, perk);
}

/**
 * The screen share qualities the signed-in user may pick: 1440p and 4K only
 * with `hdStreams`. Stable while the plan does not change.
 */
export function useStreamQualities(): string[] {
  const entitlement = useMyEntitlement();
  return useMemo(() => getAvailableStreamQualities(entitlement), [entitlement]);
}
