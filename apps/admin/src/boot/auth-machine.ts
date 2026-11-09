/**
 * The admin page's auth state machine. Pure: Boot.tsx runs the effects.
 *
 *   checking ──cookie ok──────────────────────────────▶ admin
 *      │ no cookie                                      ▲    │ session lost
 *      ▼                                                │    ▼ (console kept)
 *   signed-out ──Kova signed in──▶ verifying ──204─────┘  signed-out (lost)
 *      ▲                              │ refused (404 twice)
 *      └──── sign out ◀── denied ◀────┘ (mid-use: back to signed-out, lost)
 *
 * `session` outlives a lost session: the console stays mounted, with its
 * route, under a sign-in prompt, and picks up where it was once verified.
 * A refusal or a sign-out drops it, which unmounts the console.
 */

import type { AdminSessionInfo } from "../shared/console-contract";

export type AuthPhase =
  | "checking"
  | "signed-out"
  | "verifying"
  | "admin"
  | "denied";

export interface AuthState {
  phase: AuthPhase;
  /** The last admin session; kept while re-signing in mid-use. */
  session: AdminSessionInfo | null;
  /** Bumps on every new admin session (the console refetches). */
  epoch: number;
  /** signed-out because the session ended mid-use, not a fresh visit. */
  lost: boolean;
  /** The last verify could not reach the server (offer a retry, no loop). */
  unreachable: boolean;
  /** Mid-use, one refusal was already taken as a lapsed Kova session. */
  retried: boolean;
}

export type AuthEvent =
  | { type: "cookie-ok"; session: AdminSessionInfo }
  | { type: "cookie-missing" }
  | { type: "kova-signed-in" }
  | { type: "verified"; session: AdminSessionInfo }
  | { type: "refused" }
  | { type: "unreachable" }
  | { type: "session-lost" }
  | { type: "signed-out" };

export const initialAuthState: AuthState = {
  phase: "checking",
  session: null,
  epoch: 0,
  lost: false,
  unreachable: false,
  retried: false,
};

export function authReducer(state: AuthState, event: AuthEvent): AuthState {
  switch (event.type) {
    case "cookie-ok":
      if (state.phase !== "checking") return state;
      return {
        ...state,
        phase: "admin",
        session: event.session,
        epoch: state.epoch + 1,
      };
    case "cookie-missing":
      if (state.phase !== "checking") return state;
      return { ...state, phase: "signed-out" };
    case "kova-signed-in":
      if (state.phase !== "signed-out" && state.phase !== "denied") {
        return state;
      }
      return { ...state, phase: "verifying", unreachable: false };
    case "verified":
      if (state.phase !== "verifying") return state;
      return {
        phase: "admin",
        session: event.session,
        epoch: state.epoch + 1,
        lost: false,
        unreachable: false,
        retried: false,
      };
    case "refused":
      if (state.phase !== "verifying") return state;
      // Mid-use, a first refusal most likely means the Kova session lapsed
      // too: ask for a fresh sign-in in place and keep the console. A
      // refusal of that fresh sign-in is a real "no".
      if (state.lost && !state.retried) {
        return { ...state, phase: "signed-out", retried: true };
      }
      return {
        ...state,
        phase: "denied",
        session: null,
        lost: false,
        retried: false,
      };
    case "unreachable":
      if (state.phase !== "verifying") return state;
      return { ...state, phase: "signed-out", unreachable: true };
    case "session-lost":
      if (state.phase !== "admin") return state;
      return { ...state, phase: "signed-out", lost: true };
    case "signed-out":
      return {
        ...initialAuthState,
        phase: "signed-out",
        epoch: state.epoch,
      };
  }
}

/**
 * A same-origin app path to return to after a hosted sign-in: one or two
 * plain segments. Checked by shape, not by name, so the public bundle does
 * not list the console's routes; the router answers an unknown one.
 */
const RETURN_PATH = /^\/[a-z][a-z-]*(?:\/[A-Za-z0-9_%.-]+)?\/?$/;

export function isConsolePath(path: string): boolean {
  return RETURN_PATH.test(path);
}
