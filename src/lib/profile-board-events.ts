/**
 * PROFILE_BOARD_UPDATE from the gateway (a board was saved): open profiles
 * of that user refetch. A tiny bus, so the gateway does not import the
 * profile UI and the board components do not import the gateway.
 */

export interface ProfileBoardUpdate {
  user_id: string;
  version: number;
  /**
   * Only the widgets' data changed (the Deadlock stats landed): refetch
   * whatever the version.
   */
  data?: boolean;
}

type Listener = (update: ProfileBoardUpdate) => void;

const listeners = new Set<Listener>();

export function emitProfileBoardUpdate(value: unknown): void {
  if (!value || typeof value !== "object") return;
  const raw = value as Record<string, unknown>;
  if (typeof raw.user_id !== "string" || typeof raw.version !== "number") {
    return;
  }
  const update: ProfileBoardUpdate = {
    user_id: raw.user_id,
    version: raw.version,
  };
  if (raw.data === true) update.data = true;
  for (const listener of listeners) listener(update);
}

/** Calls `listener` for every board update; returns the unsubscribe. */
export function onProfileBoardUpdate(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/**
 * Opens a pinned message (the Quote plate's Jump) in the chat, through the
 * same `navigate-channel` event the notification inbox uses. The host
 * closes the profile first.
 */
export function navigateToBoardMessage(
  messageId: string,
  snapshot: { channel_id: string; server_id: string | null },
): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent("navigate-channel", {
      detail: {
        channelId: snapshot.channel_id,
        messageId,
        serverId: snapshot.server_id,
      },
    }),
  );
}
