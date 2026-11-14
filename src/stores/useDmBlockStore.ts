import { create } from "zustand";

import type { BlockState } from "@/lib/blocks";

/**
 * DMs the server refused over a block in this session (channel id ->
 * which side blocked). The composer reads it; leaving the DM forgets it, so
 * coming back tries again (the other person may have unblocked).
 */
interface DmBlockState {
  refusals: Readonly<Record<string, BlockState>>;
  refuse: (channelId: string, state: BlockState) => void;
  forget: (channelId: string) => void;
}

export const useDmBlockStore = create<DmBlockState>((set) => ({
  refusals: {},
  refuse: (channelId, state) =>
    set((current) =>
      current.refusals[channelId] === state
        ? current
        : { refusals: { ...current.refusals, [channelId]: state } },
    ),
  forget: (channelId) =>
    set((current) => {
      if (!(channelId in current.refusals)) return current;
      const { [channelId]: _dropped, ...rest } = current.refusals;
      return { refusals: rest };
    }),
}));
