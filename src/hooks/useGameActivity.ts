import type { GameActivity } from "@/lib/gateway-protocol";
import { useChatStore } from "@/stores/chat-store";

/**
 * What the user is playing, from presence; undefined when nothing is shown
 * (not playing, offline, invisible, or not sharing their activity).
 */
export function useGameActivity(
  userId: string | null | undefined,
): GameActivity | undefined {
  return useChatStore((state) =>
    userId ? state.gameActivityByUserId[userId] : undefined,
  );
}
