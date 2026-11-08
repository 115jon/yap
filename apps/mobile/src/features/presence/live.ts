import { parseGameActivity, type GameActivity } from "@ralph/protocol";
import { useMemo } from "react";
import { create } from "zustand";

import type { LedState } from "@/ui";

// Who is connected now, and as what: the web's onlineUsers plus live status
// (src/stores/chat-gateway.ts PRESENCE_LIST and PRESENCE_UPDATE). The room
// sends the whole list on every channel subscribe and changes in between,
// so once a list has arrived anyone missing from it is offline.

type LiveStatus = "online" | "idle" | "dnd";

interface LivePresenceState {
  /** A list has arrived since launch: absence now means offline. */
  readonly listed: boolean;
  /** userId → their status, for people connected now. */
  readonly status: Readonly<Record<string, LiveStatus>>;
  /** userId → a custom status a live update carried (null: cleared). */
  readonly custom: Readonly<Record<string, string | null>>;
  /** userId → what they are playing ("Playing X"), for people connected now. */
  readonly activity: Readonly<Record<string, GameActivity>>;
}

const INITIAL: LivePresenceState = {
  listed: false,
  status: {},
  custom: {},
  activity: {},
};

const useLivePresenceStore = create<LivePresenceState>()(() => INITIAL);

export function resetLivePresence(): void {
  useLivePresenceStore.setState(INITIAL, true);
}

function live(status: string | null | undefined): LiveStatus | null {
  return status === "online" || status === "idle" || status === "dnd"
    ? status
    : null;
}

/**
 * A presence entry's `game_activity`: undefined when absent (status-only
 * updates and older servers: keep what is known), null when cleared or not
 * valid (an icon that is not our own path, an oversized name).
 */
export function readGameActivity(
  value: unknown,
): GameActivity | null | undefined {
  if (value === undefined) return undefined;
  return parseGameActivity(value) ?? null;
}

/** PRESENCE_LIST: everyone connected, replacing what was known. */
export function setPresenceList(
  users: readonly {
    user_id: string;
    status?: string | null;
    game_activity?: unknown;
  }[],
): void {
  useLivePresenceStore.setState((s) => {
    const status: Record<string, LiveStatus> = {};
    const activity: Record<string, GameActivity> = {};
    for (const user of users) {
      const value = live(user.status ?? "online");
      if (!value) continue;
      status[user.user_id] = value;
      const game = readGameActivity(user.game_activity);
      const kept = game === undefined ? s.activity[user.user_id] : game;
      if (kept) activity[user.user_id] = kept;
    }
    return { listed: true, status, activity };
  });
}

/** PRESENCE_UPDATE for anyone. */
export function applyPresence(
  userId: string,
  status: string,
  customStatus: string | null | undefined,
  gameActivity?: unknown,
): void {
  useLivePresenceStore.setState((s) => {
    const next = { ...s.status };
    const value = live(status);
    if (value) next[userId] = value;
    else delete next[userId];
    const game = value ? readGameActivity(gameActivity) : null;
    let activity = s.activity;
    if (game === null && userId in activity) {
      const { [userId]: _gone, ...rest } = activity;
      activity = rest;
    } else if (game) {
      activity = { ...activity, [userId]: game };
    }
    return {
      status: next,
      custom:
        customStatus === undefined
          ? s.custom
          : { ...s.custom, [userId]: customStatus },
      activity,
    };
  });
}

/** What someone is playing, while they are connected and sharing it. */
export function useGameActivity(userId: string): GameActivity | undefined {
  return useLivePresenceStore((s) => s.activity[userId]);
}

/** "Playing Minecraft". */
export function gameActivityLabel(activity: GameActivity): string {
  return `Playing ${activity.name}`;
}

/** Test and debug access to the live state. */
export function getLivePresence(): LivePresenceState {
  return useLivePresenceStore.getState();
}

export type Presence = LiveStatus | "offline";

/**
 * A person's presence: live once a list has arrived, else `fallback` (the
 * status an API row carried) so a list loaded before the socket is not all
 * offline.
 */
export function usePresence(
  userId: string,
  fallback?: string | null,
): Presence {
  return useLivePresenceStore((s) => {
    if (s.listed) return s.status[userId] ?? "offline";
    return live(fallback) ?? "offline";
  });
}

/** The same test for many people at once (the member list's grouping). */
export function useOnlineTest(): (userId: string) => boolean {
  const status = useLivePresenceStore((s) => s.status);
  const listed = useLivePresenceStore((s) => s.listed);
  return useMemo(
    () => (listed ? (id: string) => status[id] !== undefined : () => false),
    [listed, status],
  );
}

/** A live custom status when one arrived, else `fallback` from the API. */
export function useCustomStatus(
  userId: string,
  fallback: string | null | undefined,
): string | null {
  return useLivePresenceStore((s) => {
    const value = userId in s.custom ? s.custom[userId] : fallback;
    return value?.trim() || null;
  });
}

const LED: Record<Presence, LedState | undefined> = {
  online: "talk",
  idle: "idle",
  dnd: "danger",
  offline: undefined,
};

/** The avatar LED for someone else: offline shows none. */
export function presenceLedOf(presence: Presence): LedState | undefined {
  return LED[presence];
}

export const PRESENCE_LABEL: Record<Presence, string> = {
  online: "Online",
  idle: "Idle",
  dnd: "Do not disturb",
  offline: "Offline",
};
