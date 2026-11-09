import { describe, expect, it } from "vitest";

import { BADGE_ART, BADGE_CELLS, badgeCellInk } from "@/lib/badge-art";
import {
  BADGE_IDS,
  BADGES,
  earnedAutoBadges,
  isManualBadge,
  isNightOwlMoment,
  localHour,
  monthsSince,
  parseUserBadges,
  profileBadges,
  VOICE_REGULAR_SECONDS,
} from "@/lib/badges";
import { isDailySlot } from "@/lib/scheduled-jobs";
import {
  cfTimeZone,
  isTimeZone,
  utcOffsetMinutesIn,
} from "@/lib/request-time-zone";
import { MAX_SESSION_SECONDS, sessionSeconds } from "@/lib/user-stats";

const NOW = Date.parse("2026-11-09T12:00:00Z");

describe("badge definitions", () => {
  it("defines the eight badges, founder and loud first", () => {
    expect(BADGES.map((badge) => badge.id)).toEqual([...BADGE_IDS]);
    expect(BADGE_IDS.slice(0, 2)).toEqual(["founder", "loud"]);
  });

  it("does not number the Founder badge", () => {
    const founder = BADGES.find((badge) => badge.id === "founder")!;
    expect(founder.name).toBe("Founder");
    expect(`${founder.name} ${founder.description}`).not.toMatch(/#|\d/);
  });

  it("lets admins hand out only the manual badges", () => {
    expect(BADGE_IDS.filter(isManualBadge)).toEqual(["yap-team", "bug-hunter"]);
  });

  it("hides Night owl until earned", () => {
    expect(BADGES.filter((badge) => badge.hidden).map((b) => b.id)).toEqual([
      "night-owl",
    ]);
  });

  it("has 16 x 16 art for every badge, in known inks", () => {
    for (const id of BADGE_IDS) {
      const art = BADGE_ART[id];
      expect(art, id).toHaveLength(BADGE_CELLS);
      for (const row of art) {
        expect(row, id).toHaveLength(BADGE_CELLS);
        expect(row, id).toMatch(/^[#@+.]+$/);
      }
      // Something is drawn.
      expect(art.join("").replace(/\./g, "").length, id).toBeGreaterThan(20);
    }
    expect(badgeCellInk("founder", 0, 0)).toBeNull();
    expect(badgeCellInk("founder", 7, 2)).toBe("detail");
  });
});

describe("automatic rules", () => {
  it("awards Voice regular at 100 hours", () => {
    expect(VOICE_REGULAR_SECONDS).toBe(360_000);
    expect(earnedAutoBadges({ voice_seconds: 359_999 })).toEqual([]);
    expect(earnedAutoBadges({ voice_seconds: 360_000 })).toEqual([
      "voice-regular",
    ]);
  });

  it("awards Early yapper to the first 100 accounts", () => {
    expect(earnedAutoBadges({ signup_rank: 1 })).toEqual(["early-yapper"]);
    expect(earnedAutoBadges({ signup_rank: 100 })).toEqual(["early-yapper"]);
    expect(earnedAutoBadges({ signup_rank: 101 })).toEqual([]);
    expect(earnedAutoBadges({ signup_rank: 0 })).toEqual([]);
    expect(earnedAutoBadges({ signup_rank: null })).toEqual([]);
  });

  it("awards Server owner at 10 members", () => {
    expect(earnedAutoBadges({ owned_server_members: 9 })).toEqual([]);
    expect(earnedAutoBadges({ owned_server_members: 10 })).toEqual([
      "server-owner",
    ]);
  });

  it("reads Night owl in the sender's local time", () => {
    const at = new Date("2026-11-09T08:30:00Z");
    // 08:30 UTC is 03:30 at UTC-5.
    expect(localHour(at, -300)).toBe(3);
    expect(isNightOwlMoment(at, -300)).toBe(true);
    expect(isNightOwlMoment(at, 0)).toBe(false);
  });

  it("falls back to UTC without a usable offset", () => {
    const threeUtc = new Date("2026-11-09T03:10:00Z");
    expect(isNightOwlMoment(threeUtc)).toBe(true);
    expect(isNightOwlMoment(threeUtc, null)).toBe(true);
    expect(isNightOwlMoment(threeUtc, Number.NaN)).toBe(true);
    // More than 14 hours is not an offset.
    expect(isNightOwlMoment(threeUtc, 15 * 60)).toBe(true);
    expect(isNightOwlMoment(new Date("2026-11-09T04:00:00Z"))).toBe(false);
  });
});

describe("awards from the API", () => {
  it("drops unknown ids and duplicates, in rank order", () => {
    expect(
      parseUserBadges([
        { id: "night-owl", awarded_at: "b" },
        { id: "mystery", awarded_at: "x" },
        { id: "founder", awarded_at: "a" },
        { id: "founder", awarded_at: "c" },
        null,
        "loud",
      ]),
    ).toEqual([
      { id: "founder", awarded_at: "a" },
      { id: "night-owl", awarded_at: "b" },
    ]);
    expect(parseUserBadges(undefined)).toEqual([]);
  });

  it("shows Loud only while the tier is Loud", () => {
    const stored = [{ id: "voice-regular" as const, awarded_at: "x" }];
    const loud = {
      tier: "loud" as const,
      started_at: "2026-08-09T00:00:00Z",
      expires_at: "2026-12-01T00:00:00Z",
    };
    expect(profileBadges(stored, loud, NOW).map((b) => b.id)).toEqual([
      "loud",
      "voice-regular",
    ]);
    expect(
      profileBadges(
        stored,
        { ...loud, expires_at: "2026-11-01T00:00:00Z" },
        NOW,
      ),
    ).toEqual(stored);
    // A stored loud row (there should be none) never shows on its own.
    expect(
      profileBadges([{ id: "loud", awarded_at: "x" }], { tier: "free" }, NOW),
    ).toEqual([]);
    expect(profileBadges([], { tier: "founder" }, NOW)).toEqual([]);
  });

  it("counts Loud months, at least one", () => {
    expect(monthsSince("2026-08-09T00:00:00Z", NOW)).toBe(3);
    expect(monthsSince("2026-11-01T00:00:00Z", NOW)).toBe(1);
    expect(monthsSince("not a date", NOW)).toBe(1);
  });
});

describe("stats arithmetic", () => {
  it("caps a session at 24 hours", () => {
    const start = NOW - 30 * 3_600_000;
    expect(sessionSeconds(start, NOW)).toBe(MAX_SESSION_SECONDS);
    expect(MAX_SESSION_SECONDS).toBe(86_400);
    expect(sessionSeconds(NOW - 90_500, NOW)).toBe(90);
  });

  it("counts nothing for missing or reversed times", () => {
    expect(sessionSeconds(undefined, NOW)).toBe(0);
    expect(sessionSeconds(NOW + 1000, NOW)).toBe(0);
    expect(sessionSeconds(Number.NaN, NOW)).toBe(0);
  });

  it("reads the time zone from Cloudflare, never the client", () => {
    const at = new Date("2026-07-01T12:00:00Z");
    expect(utcOffsetMinutesIn("America/New_York", at)).toBe(-240);
    expect(
      utcOffsetMinutesIn("America/New_York", new Date("2026-12-01T12:00:00Z")),
    ).toBe(-300);
    expect(utcOffsetMinutesIn("Asia/Kolkata", at)).toBe(330);
    expect(utcOffsetMinutesIn("UTC", at)).toBe(0);
    expect(utcOffsetMinutesIn("Not/AZone", at)).toBeNull();
    expect(utcOffsetMinutesIn(null, at)).toBeNull();
    expect(isTimeZone("Europe/Paris")).toBe(true);
    expect(isTimeZone(42)).toBe(false);

    const request = Object.assign(new Request("https://yap.test/"), {
      cf: { timezone: "Asia/Tokyo" },
    });
    expect(cfTimeZone(request)).toBe("Asia/Tokyo");
    expect(cfTimeZone(new Request("https://yap.test/"))).toBeNull();
    expect(
      cfTimeZone(
        Object.assign(new Request("https://yap.test/"), {
          cf: { timezone: "nope" },
        }),
      ),
    ).toBeNull();
    // 18:30 UTC is 03:30 in Tokyo.
    const tokyo = new Date("2026-11-09T18:30:00Z");
    expect(
      isNightOwlMoment(tokyo, utcOffsetMinutesIn("Asia/Tokyo", tokyo)),
    ).toBe(true);
  });
});

describe("isDailySlot", () => {
  const crons = ["0 */6 * * *", "17 * * * *"];
  it("runs daily work at the 00:00 UTC maintenance run only", () => {
    expect(
      isDailySlot("0 */6 * * *", Date.parse("2026-11-09T00:00:00Z"), crons),
    ).toBe(true);
    expect(
      isDailySlot("0 */6 * * *", Date.parse("2026-11-09T06:00:00Z"), crons),
    ).toBe(false);
  });

  it("runs it on a manual trigger", () => {
    expect(
      isDailySlot("manual", Date.parse("2026-11-09T13:00:00Z"), crons),
    ).toBe(true);
  });
});
