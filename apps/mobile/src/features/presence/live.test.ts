import { beforeEach, describe, expect, it } from "vitest";

import {
  applyPresence,
  gameActivityLabel,
  getLivePresence,
  readGameActivity,
  resetLivePresence,
  setPresenceList,
} from "./live";

const hades = {
  kind: "playing",
  name: "Hades",
  started_at: 1_700_000_000_000,
  source: "detected",
} as const;

describe("live presence: game activity", () => {
  beforeEach(() => resetLivePresence());

  it("reads only valid activities, with our own icon paths", () => {
    expect(readGameActivity(undefined)).toBeUndefined();
    expect(readGameActivity(null)).toBeNull();
    expect(readGameActivity(hades)).toEqual(hades);
    expect(
      readGameActivity({ ...hades, icon_url: "https://example.com/a.png" }),
    ).toBeNull();
    expect(gameActivityLabel(hades)).toBe("Playing Hades");
  });

  it("sets it from PRESENCE_UPDATE, keeps it on status-only ones, clears it offline", () => {
    applyPresence("u1", "online", undefined, hades);
    expect(getLivePresence().activity.u1).toEqual(hades);
    applyPresence("u1", "idle", undefined);
    expect(getLivePresence().activity.u1).toEqual(hades);
    applyPresence("u1", "offline", undefined);
    expect(getLivePresence().activity.u1).toBeUndefined();
    applyPresence("u1", "online", undefined, hades);
    applyPresence("u1", "online", undefined, null);
    expect(getLivePresence().activity.u1).toBeUndefined();
  });

  it("replaces activities from PRESENCE_LIST, keeping entries with no field", () => {
    applyPresence("u1", "online", undefined, hades);
    applyPresence("u2", "online", undefined, hades);
    setPresenceList([
      { user_id: "u1", status: "online" },
      { user_id: "u2", status: "online", game_activity: null },
      { user_id: "u3", status: "dnd", game_activity: hades },
      { user_id: "u4", status: "offline", game_activity: hades },
    ]);
    expect(getLivePresence().activity).toEqual({ u1: hades, u3: hades });
  });
});
