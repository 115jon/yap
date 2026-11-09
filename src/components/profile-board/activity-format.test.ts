import { describe, expect, it } from "vitest";

import type { ActivityEvent } from "@/lib/profile-board";

import {
  activityGroupFor,
  activityPlace,
  activityText,
  activityWhen,
  describeActivity,
  formatActivityDuration,
  groupActivity,
  localDaysAgo,
  PLACE_JOIN,
  withinRetention,
} from "./activity-format";

const NOW = new Date(2026, 10, 9, 21, 47).getTime(); // Sun 9 Nov 2026, local

function local(day: number, hh = 12, mm = 0, month = 10, year = 2026) {
  return new Date(year, month, day, hh, mm).getTime();
}

function ev(
  partial: Partial<ActivityEvent> & Pick<ActivityEvent, "kind">,
): ActivityEvent {
  return {
    id: partial.id ?? "e",
    ref: null,
    data: {},
    started_at: new Date(local(9)).toISOString(),
    ended_at: null,
    seconds: null,
    hidden: false,
    ...partial,
  };
}

describe("formatActivityDuration", () => {
  it.each([
    [null, null],
    [-5, null],
    [30, "<1m"],
    [48 * 60, "48m"],
    [2 * 3600, "2h"],
    [3 * 3600 + 40 * 60, "3h 40m"],
    [2 * 3600 + 5 * 60 + 59, "2h 05m"],
  ])("%s -> %s", (seconds, expected) => {
    expect(formatActivityDuration(seconds)).toBe(expected);
  });
});

describe("day groups", () => {
  it("counts local days, not 24-hour spans", () => {
    expect(localDaysAgo(local(9, 0, 5), NOW)).toBe(0);
    expect(localDaysAgo(local(8, 23, 59), NOW)).toBe(1);
    expect(localDaysAgo(local(2, 12), NOW)).toBe(7);
  });

  it("labels Today, Yesterday, Earlier this week, then dates", () => {
    expect(activityGroupFor(local(9, 1), NOW).label).toBe("Today");
    expect(activityGroupFor(local(8, 23), NOW).label).toBe("Yesterday");
    expect(activityGroupFor(local(3), NOW)).toMatchObject({
      key: "week",
      label: "Earlier this week",
    });
    expect(activityGroupFor(local(2), NOW)).toMatchObject({
      key: "date:2026-11-02",
      kind: "date",
      label: "Mon 2 Nov",
    });
    expect(activityGroupFor(local(28, 12, 0, 11, 2025), NOW).label).toBe(
      "28 Dec 2025",
    );
  });

  it("keeps a clock slightly ahead of the server in Today", () => {
    expect(activityGroupFor(NOW + 60_000, NOW).key).toBe("today");
  });

  it("groups newest-first events in order with counts", () => {
    const events = [
      ev({
        id: "a",
        kind: "game",
        started_at: new Date(local(9, 20)).toISOString(),
      }),
      ev({
        id: "b",
        kind: "badge",
        started_at: new Date(local(9, 3)).toISOString(),
      }),
      ev({
        id: "c",
        kind: "voice",
        started_at: new Date(local(8, 22)).toISOString(),
      }),
      ev({
        id: "d",
        kind: "game",
        started_at: new Date(local(5)).toISOString(),
      }),
      ev({
        id: "e",
        kind: "game",
        started_at: new Date(local(4)).toISOString(),
      }),
      ev({
        id: "f",
        kind: "game",
        started_at: new Date(local(1)).toISOString(),
      }),
      ev({ id: "g", kind: "game", started_at: "not a date" }),
    ];
    const groups = groupActivity(events, NOW);
    expect(
      groups.map((g) => [g.label, g.events.map((e) => e.id).join("")]),
    ).toEqual([
      ["Today", "ab"],
      ["Yesterday", "c"],
      ["Earlier this week", "de"],
      ["Sun 1 Nov", "f"],
      ["Earlier", "g"],
    ]);
  });
});

describe("activityWhen", () => {
  const start = new Date(local(6, 19, 58)).toISOString();
  const end = new Date(local(6, 23, 38)).toISOString();

  it("prints a session's span and adds the weekday only in the week group", () => {
    const voice = { kind: "voice" as const, started_at: start, ended_at: end };
    expect(activityWhen(voice, "today")).toMatch(
      /^\d{1,2}[:.]58.*–.*\d{1,2}[:.]38/,
    );
    expect(activityWhen(voice, "week")).toMatch(/^Fri · /);
  });

  it("prints one time for moments", () => {
    const badge = { kind: "badge" as const, started_at: start, ended_at: end };
    expect(activityWhen(badge, "date")).not.toContain("–");
  });
});

describe("describeActivity", () => {
  it("names a known place and keeps private ones vague", () => {
    expect(
      activityPlace({ server_name: "Raid HQ", channel_name: "Lounge" }),
    ).toEqual({
      name: `Raid HQ${PLACE_JOIN}Lounge`,
      known: true,
    });
    expect(activityPlace({ channel_id: "dm" }).name).toBe("a call");
    expect(activityPlace({}).name).toBe("a private server");
  });

  it("writes each kind's sentence", () => {
    const text = (e: ActivityEvent) => activityText(describeActivity(e));
    expect(text(ev({ kind: "game", data: { name: "Deadlock" } }))).toBe(
      "Played Deadlock",
    );
    expect(text(ev({ kind: "game" }))).toBe("Played a game");
    expect(
      text(
        ev({
          kind: "voice",
          data: { server_name: "Raid HQ", channel_name: "Lounge" },
        }),
      ),
    ).toBe(`Hung out in Raid HQ${PLACE_JOIN}Lounge`);
    expect(text(ev({ kind: "voice" }))).toBe("Hung out in a private server");
    expect(
      text(
        ev({
          kind: "stream",
          data: { name: "Balatro", server_name: "Raid HQ" },
        }),
      ),
    ).toBe("Streamed Balatro in Raid HQ");
    expect(text(ev({ kind: "listen", data: { tracks: 1 } }))).toBe(
      "Listened together: 1 track in a private server",
    );
    expect(text(ev({ kind: "badge", ref: "night-owl" }))).toBe(
      "Earned the Night owl badge",
    );
    expect(text(ev({ kind: "badge", ref: "nope" }))).toBe("Earned a badge");
    expect(
      text(ev({ kind: "server_join", data: { server_name: "Cozy Corner" } })),
    ).toBe("Joined Cozy Corner");
  });

  it("marks names strong and puts session length in the readout", () => {
    const d = describeActivity(
      ev({ kind: "game", seconds: 6120, data: { name: "Deadlock" } }),
    );
    expect(d.runs).toContainEqual({ text: "Deadlock", strong: true });
    expect(d.readout).toBe("1h 42m");
    expect(
      describeActivity(ev({ kind: "badge", ref: "night-owl", seconds: 99 }))
        .readout,
    ).toBeNull();
    expect(
      describeActivity(
        ev({ kind: "listen", data: { tracks: 12, top_track: "Night Drive" } }),
      ).detail,
    ).toBe("Most played: Night Drive");
  });
});

describe("withinRetention", () => {
  it("drops events older than the retention window", () => {
    const events = [
      ev({
        id: "new",
        kind: "game",
        started_at: new Date(NOW - 3 * 86_400_000).toISOString(),
      }),
      ev({
        id: "old",
        kind: "game",
        started_at: new Date(NOW - 10 * 86_400_000).toISOString(),
      }),
    ];
    expect(withinRetention(events, 7, NOW).map((e) => e.id)).toEqual(["new"]);
    expect(withinRetention(events, 30, NOW)).toHaveLength(2);
  });
});
