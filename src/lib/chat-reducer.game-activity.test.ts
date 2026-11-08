import { describe, expect, it } from "vitest";
import { chatReducer, initialState, type ChatState } from "./chat-reducer";
import type { GameActivity } from "./gateway-protocol";

const minecraft: GameActivity = {
  kind: "playing",
  name: "Minecraft",
  started_at: 1_700_000_000_000,
  source: "detected",
};
const celeste: GameActivity = { ...minecraft, name: "Celeste" };

function withActivity(userId: string, activity: GameActivity): ChatState {
  return chatReducer(initialState, {
    type: "UPDATE_USER_STATUS",
    userId,
    status: "online",
    gameActivity: activity,
  });
}

describe("chat reducer: game activity", () => {
  it("sets, keeps and clears a user's activity from PRESENCE_UPDATE", () => {
    const playing = withActivity("u1", minecraft);
    expect(playing.gameActivityByUserId).toEqual({ u1: minecraft });

    // A status-only update (no game_activity field) keeps it.
    const idle = chatReducer(playing, {
      type: "UPDATE_USER_STATUS",
      userId: "u1",
      status: "idle",
    });
    expect(idle.gameActivityByUserId).toBe(playing.gameActivityByUserId);

    const cleared = chatReducer(idle, {
      type: "UPDATE_USER_STATUS",
      userId: "u1",
      status: "idle",
      gameActivity: null,
    });
    expect(cleared.gameActivityByUserId).toEqual({});
  });

  it("drops the activity when the user goes offline or invisible", () => {
    const playing = withActivity("u1", minecraft);
    const offline = chatReducer(playing, {
      type: "UPDATE_USER_STATUS",
      userId: "u1",
      status: "offline",
    });
    expect(offline.gameActivityByUserId).toEqual({});
    expect(
      chatReducer(playing, { type: "USER_OFFLINE", userId: "u1" })
        .gameActivityByUserId,
    ).toEqual({});
  });

  it("takes PRESENCE_LIST as the whole picture, keeping entries with no field", () => {
    let state = withActivity("u1", minecraft);
    for (const userId of ["u2", "u3"]) {
      state = chatReducer(state, {
        type: "UPDATE_USER_STATUS",
        userId,
        status: "online",
        gameActivity: celeste,
      });
    }

    const next = chatReducer(state, {
      type: "SET_PRESENCE_USERS",
      users: [
        // An older server's list: no field, keep what is known.
        { userId: "u1", status: "online" },
        // Cleared while this client was away.
        { userId: "u2", status: "online", gameActivity: null },
        { userId: "u4", status: "dnd", gameActivity: minecraft },
        // Invisible: never shown.
        { userId: "u5", status: "offline", gameActivity: minecraft },
        // u3 is missing: gone.
      ],
    });
    expect(next.gameActivityByUserId).toEqual({
      u1: minecraft,
      u4: minecraft,
    });
  });

  it("stores the own share_activity flag on the signed-in user", () => {
    const signedIn: ChatState = {
      ...initialState,
      user: { id: "me", username: "me" },
    };
    const off = chatReducer(signedIn, {
      type: "UPDATE_MEMBER_PROFILE",
      userId: "me",
      share_activity: false,
    });
    expect(off.user?.share_activity).toBe(false);
  });
});
