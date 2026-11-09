/**
 * PROFILE_BOARD_UPDATE from the gateway (a board was saved): open profiles
 * of that user refetch. A tiny bus, so the gateway does not import the
 * profile UI and the board components do not import the gateway.
 */

export interface ProfileBoardUpdate {
  user_id: string;
  version: number;
}

type Listener = (update: ProfileBoardUpdate) => void;

const listeners = new Set<Listener>();

export function emitProfileBoardUpdate(value: unknown): void {
  if (!value || typeof value !== "object") return;
  const raw = value as Record<string, unknown>;
  if (typeof raw.user_id !== "string" || typeof raw.version !== "number") {
    return;
  }
  const update = { user_id: raw.user_id, version: raw.version };
  for (const listener of listeners) listener(update);
}

/** Calls `listener` for every board update; returns the unsubscribe. */
export function onProfileBoardUpdate(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}
