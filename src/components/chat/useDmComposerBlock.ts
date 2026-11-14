import { useEffect } from "react";

import { dmComposerBlock, hasBlocked, type BlockState } from "@/lib/blocks";
import { useChatStore } from "@/stores/chat-store";
import { useDmBlockStore } from "@/stores/useDmBlockStore";

/** The DM composer's block and the recipient it is about. */
export interface DmComposerBlock {
  block: BlockState | null;
  recipientId: string | null;
}

/**
 * The block that closes this DM's composer, if any: you blocked the
 * recipient (live from your relationships), or the server refused a send
 * here because they blocked you. Leaving the DM forgets the refusal.
 */
export function useDmComposerBlock(
  channelId: string | null,
  isDM: boolean,
): DmComposerBlock {
  const recipientId = useChatStore((state) =>
    isDM && channelId
      ? (state.dmChannels.find((dm) => dm.id === channelId)?.recipient?.id ??
        null)
      : null,
  );
  const youBlocked = useChatStore((state) =>
    hasBlocked(state.relationships, recipientId),
  );
  const refusal = useDmBlockStore((state) =>
    channelId ? state.refusals[channelId] : undefined,
  );
  useEffect(() => {
    if (!channelId) return;
    return () => useDmBlockStore.getState().forget(channelId);
  }, [channelId]);
  return {
    block: isDM ? dmComposerBlock(youBlocked, refusal) : null,
    recipientId,
  };
}
