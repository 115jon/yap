/**
 * One user's board for the profile modal: fetched with an abort on unmount
 * or user change, refetched when the gateway says that board was saved
 * (PROFILE_BOARD_UPDATE via `onProfileBoardUpdate`), with a retry for the
 * error state. `fixture` short-circuits the network for /ui-fixtures and
 * tests. `replace` swaps in a board the editor just saved.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import type { BoardResponse } from "@/lib/profile-board";
import { fetchProfileBoard } from "@/lib/profile-board-api";
import { onProfileBoardUpdate } from "@/lib/profile-board-events";

export type ProfileBoardStatus = "loading" | "ready" | "error";

export interface UseProfileBoard {
  status: ProfileBoardStatus;
  /** The last good board for this user (kept through a refetch or its error). */
  board: BoardResponse | null;
  error: string | null;
  retry(): void;
  replace(next: BoardResponse): void;
}

interface State {
  /** Which request this state belongs to: `${userId}:${nonce}`. */
  key: string;
  status: ProfileBoardStatus;
  board: BoardResponse | null;
  error: string | null;
  /** The fixture this state was seeded from. */
  base?: BoardResponse;
}

function messageOf(error: unknown): string {
  const status = (error as { status?: number } | null)?.status;
  if (status === 403) return "You can't see this board.";
  if (status === 404) return "This profile doesn't exist any more.";
  return "Couldn't load the board.";
}

export function useProfileBoard(
  userId: string,
  options: { fixture?: BoardResponse } = {},
): UseProfileBoard {
  const { fixture } = options;
  // Bumped by retry and by board updates: each bump is one fetch.
  const [nonce, setNonce] = useState(0);
  const key = `${userId}:${nonce}`;
  const [state, setState] = useState<State>(() => ({
    key,
    status: fixture ? "ready" : "loading",
    board: fixture ?? null,
    error: null,
    base: fixture,
  }));

  // A new request (user, retry, update) starts loading, keeping the board
  // while it is the same user's. Adjusted during render, not in an effect.
  if (!fixture && state.key !== key) {
    setState((prev) => ({
      key,
      status: "loading",
      board: prev.board?.user_id === userId ? prev.board : null,
      error: null,
    }));
  }
  // A new fixture (fixtures switching surfaces) replaces the board.
  if (fixture && state.base !== fixture) {
    setState({
      key,
      status: "ready",
      board: fixture,
      error: null,
      base: fixture,
    });
  }

  useEffect(() => {
    if (fixture) return;
    const controller = new AbortController();
    const requestKey = `${userId}:${nonce}`;
    fetchProfileBoard(userId, controller.signal).then(
      (board) => {
        if (controller.signal.aborted) return;
        setState({ key: requestKey, status: "ready", board, error: null });
      },
      (error: unknown) => {
        if (controller.signal.aborted) return;
        setState((prev) => ({
          key: requestKey,
          status: "error",
          board: prev.board?.user_id === userId ? prev.board : null,
          error: messageOf(error),
        }));
      },
    );
    return () => controller.abort();
  }, [userId, fixture, nonce]);

  const versionRef = useRef(-1);
  useEffect(() => {
    versionRef.current =
      state.board?.user_id === userId ? state.board.version : -1;
  }, [state.board, userId]);

  useEffect(() => {
    if (fixture) return;
    return onProfileBoardUpdate((update) => {
      if (update.user_id !== userId) return;
      // Our own save already replaced the board at this version.
      if (versionRef.current >= update.version) return;
      setNonce((value) => value + 1);
    });
  }, [userId, fixture]);

  const retry = useCallback(() => setNonce((value) => value + 1), []);
  const replace = useCallback((next: BoardResponse) => {
    setState((prev) => ({
      ...prev,
      status: "ready",
      board: next,
      error: null,
    }));
  }, []);

  return {
    status: state.status,
    board: state.board,
    error: state.error,
    retry,
    replace,
  };
}
