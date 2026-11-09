import { Schema } from "effect";
import { describe, expect, it } from "vitest";

import {
  BADGE_ART,
  BADGE_CELLS,
  BADGE_IDS,
  bayerLit,
  badgeRuns,
  effectiveTier,
  parseEntitlement,
  parseUserBadges,
} from "./badges";
import { ProfileSchema } from "./schema";

const NOW = Date.parse("2026-11-09T12:00:00Z");

describe("parseUserBadges", () => {
  it("drops unknown ids", () => {
    const badges = parseUserBadges([
      { id: "founder", awarded_at: "2026-01-01T00:00:00Z" },
      { id: "from-the-future", awarded_at: "2026-01-02T00:00:00Z" },
      { id: 7 },
      null,
      "night-owl",
    ]);
    expect(badges.map((b) => b.id)).toEqual(["founder"]);
  });

  it("orders by rank, whatever the input order", () => {
    const badges = parseUserBadges(
      [...BADGE_IDS].reverse().map((id) => ({ id, awarded_at: "x" })),
    );
    expect(badges.map((b) => b.id)).toEqual([...BADGE_IDS]);
  });

  it("keeps the first of a duplicate", () => {
    const badges = parseUserBadges([
      { id: "loud", awarded_at: "first" },
      { id: "loud", awarded_at: "second" },
    ]);
    expect(badges).toEqual([{ id: "loud", awarded_at: "first" }]);
  });

  it("reads a non-array as none", () => {
    expect(parseUserBadges(undefined)).toEqual([]);
    expect(parseUserBadges({ id: "founder" })).toEqual([]);
  });
});

describe("entitlement", () => {
  it("is free when absent or unreadable", () => {
    expect(effectiveTier(parseEntitlement(undefined), NOW)).toBe("free");
    expect(effectiveTier(parseEntitlement(null), NOW)).toBe("free");
    expect(effectiveTier(parseEntitlement({ tier: "platinum" }), NOW)).toBe(
      "free",
    );
  });

  it("is free once expires_at has passed", () => {
    const expired = parseEntitlement({
      tier: "loud",
      source: "stripe",
      expires_at: "2026-11-01T00:00:00Z",
    });
    expect(expired.tier).toBe("loud");
    expect(effectiveTier(expired, NOW)).toBe("free");
  });

  it("keeps a running plan and a plan that never expires", () => {
    const running = parseEntitlement({
      tier: "loud",
      expires_at: "2026-12-01T00:00:00Z",
    });
    expect(effectiveTier(running, NOW)).toBe("loud");
    const founder = parseEntitlement({
      tier: "founder",
      source: "grant",
      expires_at: null,
    });
    expect(effectiveTier(founder, NOW)).toBe("founder");
  });
});

describe("badge art", () => {
  it("has a 16 x 16 drawing for every badge", () => {
    for (const id of BADGE_IDS) {
      const rows = BADGE_ART[id];
      expect(rows, id).toHaveLength(BADGE_CELLS);
      for (const row of rows) {
        expect(row, `${id}: ${row}`).toHaveLength(BADGE_CELLS);
        expect(row).toMatch(/^[.#@+]+$/);
      }
    }
  });

  it("paints inside the grid and lights something", () => {
    for (const id of BADGE_IDS) {
      const runs = badgeRuns(id);
      expect(runs.length, id).toBeGreaterThan(0);
      for (const run of runs) {
        expect(run.x + run.width).toBeLessThanOrEqual(BADGE_CELLS);
      }
    }
  });
});

describe("bayerLit", () => {
  it("lights everything at full coverage and nothing at none", () => {
    expect(bayerLit(3, 5, 1)).toBe(true);
    expect(bayerLit(3, 5, 0)).toBe(false);
  });

  it("thins as coverage drops", () => {
    const lit = (coverage: number) => {
      let n = 0;
      for (let y = 0; y < 8; y++)
        for (let x = 0; x < 8; x++) if (bayerLit(x, y, coverage)) n++;
      return n;
    };
    expect(lit(0.75)).toBeGreaterThan(lit(0.25));
  });
});

describe("ProfileSchema", () => {
  const base = {
    user: { id: "u1", username: "ann" },
    mutualServers: { count: 0, items: [] },
    mutualFriends: { count: 0, items: [] },
  };
  const decode = Schema.decodeUnknownSync(ProfileSchema);

  it("defaults to free and no badges when the fields are absent", () => {
    const { user } = decode(base);
    expect(user.entitlement.tier).toBe("free");
    expect(user.badges).toEqual([]);
  });

  it("normalizes both fields", () => {
    const { user } = decode({
      ...base,
      user: {
        ...base.user,
        entitlement: { tier: "loud", source: "stripe" },
        badges: [
          { id: "loud", awarded_at: "a" },
          { id: "mystery", awarded_at: "b" },
          { id: "founder", awarded_at: "c" },
        ],
      },
    });
    expect(user.entitlement.tier).toBe("loud");
    expect(user.badges.map((b) => b.id)).toEqual(["founder", "loud"]);
  });

  it("does not fail the profile on malformed fields", () => {
    const { user } = decode({
      ...base,
      user: { ...base.user, entitlement: 42, badges: "nope" },
    });
    expect(user.username).toBe("ann");
    expect(user.entitlement.tier).toBe("free");
    expect(user.badges).toEqual([]);
  });
});
