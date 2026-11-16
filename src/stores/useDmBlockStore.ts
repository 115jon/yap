import { create } from "zustand";

import type { BlockState } from "@/lib/blocks";

/**
 * DMs the server refused in this session: over a block (channel id ->
 * which side blocked), or over the recipient's "who can DM you" setting
 * (channel id -> the server's sentence, "Alex isn't accepting messages from
 * you."). The composer reads both; leaving the DM forgets them, so coming
 * back tries again (the other person may have unblocked or changed the
 * setting).
 */
interface DmBlockState {
  refusals: Readonly<Record<string, BlockState>>;
  notAccepting: Readonly<Record<string, string>>;
  refuse: (channelId: string, state: BlockState) => void;
  refuseNotAccepting: (channelId: string, message: string) => void;
  /** The recipient wrote in the DM, which opens it to us again. */
  clearNotAccepting: (channelId: string) => void;
  forget: (channelId: string) => void;
}

function without<T>(
  record: Readonly<Record<string, T>>,
  key: string,
): Readonly<Record<string, T>> {
  if (!(key in record)) return record;
  const { [key]: _dropped, ...rest } = record;
  return rest;
}

export const useDmBlockStore = create<DmBlockState>((set) => ({
  refusals: {},
  notAccepting: {},
  refuse: (channelId, state) =>
    set((current) =>
      current.refusals[channelId] === state
        ? current
        : { refusals: { ...current.refusals, [channelId]: state } },
    ),
  refuseNotAccepting: (channelId, message) =>
    set((current) =>
      current.notAccepting[channelId] === message
        ? current
        : { notAccepting: { ...current.notAccepting, [channelId]: message } },
    ),
  clearNotAccepting: (channelId) =>
    set((current) => {
      const notAccepting = without(current.notAccepting, channelId);
      return notAccepting === current.notAccepting ? current : { notAccepting };
    }),
  forget: (channelId) =>
    set((current) => {
      const refusals = without(current.refusals, channelId);
      const notAccepting = without(current.notAccepting, channelId);
      if (
        refusals === current.refusals &&
        notAccepting === current.notAccepting
      ) {
        return current;
      }
      return { refusals, notAccepting };
    }),
}));
