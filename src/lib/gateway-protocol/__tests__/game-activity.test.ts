import { describe, expect, it } from "vitest";
import {
  decodeMainGatewayClientFrame,
  GAME_ACTIVITY_TEXT_MAX,
  gameIconPath,
  GatewayOp,
  parseGameActivity,
} from "@/lib/gateway-protocol";

const base = {
  kind: "playing",
  name: "Minecraft",
  started_at: 1_700_000_000_000,
  source: "detected",
};
const APP_ID = "356875570916753438";
const HASH = "166fbad351ecdd02d11a0b464b0cbd02";
const icon = `/api/game-icons/${APP_ID}/${HASH}.png`;

describe("GameActivity", () => {
  it("accepts a full activity and keeps only the declared fields", () => {
    const parsed = parseGameActivity({
      ...base,
      name: "  Minecraft  ",
      app_id: APP_ID,
      steam_app_id: "1672970",
      icon_url: icon,
      details: "Survival",
      state: "In a world",
      large_image: `/api/game-assets/${APP_ID}/123456.png`,
      started_at: 1_700_000_000_000.7,
      extra: "dropped",
    });
    expect(parsed).toEqual({
      ...base,
      app_id: APP_ID,
      steam_app_id: "1672970",
      icon_url: icon,
      details: "Survival",
      state: "In a world",
      large_image: `/api/game-assets/${APP_ID}/123456.png`,
    });
  });

  it("treats null as a clear and anything invalid as undefined", () => {
    expect(parseGameActivity(null)).toBeNull();
    for (const bad of [
      undefined,
      "Minecraft",
      { ...base, kind: "watching" },
      { ...base, name: "   " },
      { ...base, name: "x".repeat(GAME_ACTIVITY_TEXT_MAX + 1) },
      { ...base, details: "x".repeat(GAME_ACTIVITY_TEXT_MAX + 1) },
      { ...base, source: "other" },
      { ...base, started_at: -1 },
      { ...base, started_at: Number.NaN },
      { ...base, app_id: "abc" },
      // Images are our own paths, never an outside URL.
      {
        ...base,
        icon_url: `https://cdn.discordapp.com/app-icons/1/${HASH}.png`,
      },
      { ...base, icon_url: `//evil.example/api/game-icons/1/${HASH}.png` },
      { ...base, icon_url: "/api/game-icons/1/../../avatars/x.png" },
      { ...base, small_image: "https://example.com/a.png" },
    ]) {
      expect(parseGameActivity(bad)).toBeUndefined();
    }
  });

  it("builds icon paths only from a valid id and hash", () => {
    expect(gameIconPath(APP_ID, HASH)).toBe(icon);
    expect(gameIconPath("1/../2", HASH)).toBeNull();
    expect(gameIconPath("1", "not-a-hash")).toBeNull();
  });

  it("decodes op 43 with an activity or null, and refuses a bad one", () => {
    expect(
      decodeMainGatewayClientFrame({
        op: GatewayOp.ActivityUpdate,
        d: { game_activity: base },
      }),
    ).toMatchObject({ ok: true });
    expect(
      decodeMainGatewayClientFrame({
        op: GatewayOp.ActivityUpdate,
        d: { game_activity: null },
      }),
    ).toMatchObject({ ok: true });
    expect(
      decodeMainGatewayClientFrame({
        op: GatewayOp.ActivityUpdate,
        d: { game_activity: { ...base, icon_url: "https://x.example/i.png" } },
      }),
    ).toMatchObject({ ok: false, invalid: "Invalid activity payload" });
    expect(
      decodeMainGatewayClientFrame({ op: GatewayOp.ActivityUpdate, d: {} }),
    ).toMatchObject({ ok: false });
  });
});
