import { describe, expect, it } from "vitest";

import {
  buildDigest,
  digestForRange,
  isDeadlockActivity,
  isOwnerOnlyDeadlockState,
  rankAssetKey,
  rankName,
  reduceMatchMetadata,
  type ApiHistoryRow,
  type DeadlockManifest,
} from "@/lib/deadlock";
import { parseBoard, parseWidgetConfig } from "@/lib/profile-board";

const MANIFEST: DeadlockManifest = {
  heroes: { "80": { key: "haze", name: "Haze", color: "#AC6133" } },
  items: { "2717130900": { name: "Monster Rounds" } },
  seasons: [
    { name: "Beta Season 2", start: 1793908800, calibration_matches: 8 },
  ],
};

// The saved sample's first rows (GET /v1/players/189501088/match-history).
const ROWS: ApiHistoryRow[] = [
  {
    match_id: 134328443,
    hero_id: 80,
    start_time: 1794236427,
    game_mode: 1,
    match_mode: 4,
    player_team: 1,
    player_kills: 1,
    player_deaths: 6,
    player_assists: 8,
    net_worth: 31922,
    last_hits: 112,
    match_duration_s: 1935,
    match_result: 0,
    player_match_outcome: 2,
    ranked_display_badge: 93,
    ranked_delta: -350,
    ranked_calibration_match: 0,
  },
  {
    match_id: 134298825,
    hero_id: 80,
    start_time: 1794231529,
    game_mode: 1,
    match_mode: 4,
    player_team: 0,
    player_kills: 4,
    player_deaths: 10,
    player_assists: 21,
    net_worth: 47294,
    last_hits: 134,
    match_duration_s: 2436,
    match_result: 0,
    player_match_outcome: 1,
    ranked_display_badge: 94,
    ranked_delta: 350,
    ranked_calibration_match: 0,
  },
  {
    match_id: 134000001,
    hero_id: 80,
    start_time: 1794200000,
    game_mode: 4,
    match_mode: 1,
    player_team: 0,
    match_result: 0,
    match_duration_s: 600,
  },
];

describe("deadlock reduction", () => {
  it("counts every mode, ranks from ranked matches only", () => {
    const digest = buildDigest({
      history: ROWS,
      rank: { account_id: 189501088, badge: 93 },
      extras: null,
      manifest: MANIFEST,
      now: 1794240000,
    });
    expect(digest.ranges["7d"].matches).toBe(3);
    expect(digest.ranges["7d"].wins).toBe(2);
    expect(digest.ranges["7d"].rank_delta).toBe(0);
    expect(digest.last10.map((m) => m.mode)).toEqual([
      "ranked",
      "ranked",
      "brawl",
    ]);
    expect(digest.last_match?.win).toBe(false);
    expect(digest.rank_days.at(-1)).toBe(93);
    expect(digest.heroes).toEqual({ "80": MANIFEST.heroes["80"] });
  });

  it("keeps only the player's upgrades still held from the metadata", () => {
    const extras = reduceMatchMetadata(
      {
        match_info: {
          match_id: 134328443,
          players: [
            { account_id: 1, items: [{ item_id: 5, sold_time_s: 0 }] },
            {
              account_id: 189501088,
              level: 30,
              items: [
                { item_id: 2717130900, sold_time_s: 0 },
                { item_id: 1385849824, sold_time_s: 0 },
                { item_id: 99, sold_time_s: 400 },
              ],
              stats: [
                { player_damage: 1 },
                { player_damage: 12056, boss_damage: 1654 },
              ],
            },
          ],
        },
      },
      189501088,
    );
    expect(extras).toEqual({
      match_id: 134328443,
      level: 30,
      hero_damage: 12056,
      objective_damage: 1654,
      items: [2717130900, 1385849824],
    });
    const digest = buildDigest({
      history: ROWS,
      rank: null,
      extras,
      manifest: MANIFEST,
      now: 1794240000,
    });
    // Only shop upgrades the manifest knows.
    expect(digest.last_match?.items).toEqual([2717130900]);
  });

  it("cuts a digest to one range for other viewers", () => {
    const digest = buildDigest({
      history: ROWS,
      rank: null,
      extras: null,
      manifest: MANIFEST,
      now: 1794240000,
    });
    const cut = digestForRange(digest, "season");
    expect(cut.ranges["7d"].matches).toBe(0);
    expect(cut.ranges.season.matches).toBe(3);
  });

  it("names ranks and their art", () => {
    expect(rankName(74)).toBe("Emissary IV");
    expect(rankName(0)).toBe("Unranked");
    expect(rankAssetKey(74)).toBe("rank_7_4");
    expect(rankAssetKey(70)).toBe("rank_7");
    expect(isOwnerOnlyDeadlockState("empty")).toBe(true);
    expect(isOwnerOnlyDeadlockState("pending")).toBe(false);
    expect(isDeadlockActivity({ app_id: "1276737795012165766" })).toBe(true);
    expect(isDeadlockActivity({ name: "Balatro" })).toBe(false);
  });
});

describe("the deadlock widget in the board model", () => {
  it("normalises its config", () => {
    expect(parseWidgetConfig("deadlock", {})).toEqual({
      at_rest: "career",
      range: "30d",
      show_party: true,
    });
    expect(
      parseWidgetConfig("deadlock", {
        at_rest: "last_match",
        range: "ranked",
        show_party: false,
        modes: "ranked",
      }),
    ).toEqual({ at_rest: "last_match", range: "30d", show_party: false });
  });

  it("allows one per board", () => {
    const { board, dropped } = parseBoard({
      widgets: [
        { id: "dl-one", type: "deadlock", size: "M" },
        { id: "dl-two", type: "deadlock", size: "S" },
      ],
    });
    expect(board.widgets).toHaveLength(1);
    expect(dropped).toEqual([
      { id: "dl-two", type: "deadlock", reason: "type_limit" },
    ]);
  });
});
