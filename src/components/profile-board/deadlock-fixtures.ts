/**
 * Deadlock widget fixtures for /ui-fixtures and the DOM tests: Kiwi's
 * career built by the real reduction (`buildDigest`) from a made-up but
 * realistic 30 days in the API's own row shapes, plus every state and the
 * live activities presence would carry. Fixed clock (FIXTURE_NOW).
 */

import {
  buildDigest,
  DEADLOCK_APP_ID,
  type ApiHistoryRow,
  type DeadlockBoardData,
  type DeadlockDigest,
  type DeadlockManifest,
} from "@/lib/deadlock";
import type { GameActivity } from "@/lib/gateway-protocol";

import { FIXTURE_NOW } from "./fixtures";

const NOW_S = Math.floor(FIXTURE_NOW / 1000);
const DAY_S = 86_400;

export const FIXTURE_HEROES = {
  haze: 13,
  astro: 59,
  tengu: 20,
  chrono: 15,
  wraith: 7,
  inferno: 1,
  bebop: 3,
} as const;

export const FIXTURE_ITEMS = {
  fleetfoot: 2010,
  hunters_aura: 2011,
  point_blank: 2012,
  unstoppable: 2013,
  counterspell: 2014,
  slowing_hex: 2015,
} as const;

export const FIXTURE_MANIFEST: DeadlockManifest = {
  heroes: {
    [FIXTURE_HEROES.haze]: { key: "haze", name: "Haze", color: "#AC6133" },
    [FIXTURE_HEROES.astro]: {
      key: "astro",
      name: "Holliday",
      color: "#A35A1B",
    },
    [FIXTURE_HEROES.tengu]: { key: "tengu", name: "Ivy", color: "#9D8CA9" },
    [FIXTURE_HEROES.chrono]: {
      key: "chrono",
      name: "Paradox",
      color: "#983952",
    },
    [FIXTURE_HEROES.wraith]: {
      key: "wraith",
      name: "Wraith",
      color: "#944D78",
    },
    [FIXTURE_HEROES.inferno]: {
      key: "inferno",
      name: "Infernus",
      color: "#C93C26",
    },
    [FIXTURE_HEROES.bebop]: { key: "bebop", name: "Bebop", color: "#9F4734" },
  },
  items: {
    [FIXTURE_ITEMS.fleetfoot]: { name: "Fleetfoot" },
    [FIXTURE_ITEMS.hunters_aura]: { name: "Hunter's Aura" },
    [FIXTURE_ITEMS.point_blank]: { name: "Point Blank" },
    [FIXTURE_ITEMS.unstoppable]: { name: "Unstoppable" },
    [FIXTURE_ITEMS.counterspell]: { name: "Counterspell" },
    [FIXTURE_ITEMS.slowing_hex]: { name: "Slowing Hex" },
  },
  seasons: [
    {
      name: "Beta Season 1",
      start: NOW_S - 120 * DAY_S,
      calibration_matches: 8,
    },
    // The reset falls inside the 30-day chart, as in the mockup.
    { name: "Beta Season 2", start: NOW_S - 6 * DAY_S, calibration_matches: 8 },
  ],
};

/** End-of-day badges, oldest first (the mockup's 30 days). */
const RANK_DAYS = [
  63, 63, 64, 64, 63, 64, 65, 65, 64, 65, 66, 66, 71, 71, 66, 71, 72, 72, 71,
  72, 72, 73, 72, 0, 0, 65, 66, 71, 73, 74,
];

type HeroKey = keyof typeof FIXTURE_HEROES;

/** The last ten, newest first: [hero, win, k, d, a, souls, seconds]. */
const LAST_TEN: Array<
  [HeroKey, boolean, number, number, number, number, number]
> = [
  ["haze", true, 9, 3, 9, 35_400, 1812],
  ["astro", true, 7, 2, 14, 28_200, 1361],
  ["haze", false, 3, 14, 1, 19_400, 1445],
  ["tengu", true, 4, 15, 13, 54_100, 2710],
  ["haze", false, 2, 4, 2, 12_100, 902],
  ["chrono", true, 12, 3, 17, 74_200, 2791],
  ["haze", false, 7, 7, 12, 42_700, 1988],
  ["wraith", true, 11, 5, 6, 48_900, 2200],
  ["astro", true, 6, 6, 15, 39_000, 2052],
  ["haze", false, 1, 6, 8, 31_900, 1935],
];

/** Matches and wins per hero over the 30 days (the pool). */
const POOL: Array<[HeroKey, number, number]> = [
  ["haze", 38, 22],
  ["astro", 24, 12],
  ["tengu", 19, 10],
  ["chrono", 17, 8],
  ["wraith", 14, 7],
];

function row(
  id: number,
  hero: HeroKey,
  win: boolean,
  at: number,
  stats: { k: number; d: number; a: number; souls: number; seconds: number },
  ranked: { badge: number | null; delta: number | null; calibration: boolean },
): ApiHistoryRow {
  return {
    match_id: id,
    hero_id: FIXTURE_HEROES[hero],
    hero_level: 30,
    start_time: at,
    game_mode: 1,
    match_mode: ranked.badge !== null || ranked.calibration ? 4 : 1,
    player_team: 0,
    player_kills: stats.k,
    player_deaths: stats.d,
    player_assists: stats.a,
    net_worth: stats.souls,
    last_hits: 152,
    match_duration_s: stats.seconds,
    match_result: win ? 0 : 1,
    player_match_outcome: win ? 1 : 2,
    ranked_display_badge: ranked.badge ?? 0,
    ranked_delta: ranked.delta,
    ranked_calibration_match: ranked.calibration ? 1 : 0,
  };
}

/**
 * Kiwi's history in the API's row shape: 112 matches in 30 days (the pool
 * above), ranked on most days along RANK_DAYS, plus 528 older matches so
 * all time reads 640.
 */
export function fixtureHistory(): ApiHistoryRow[] {
  const rows: ApiHistoryRow[] = [];
  let id = 134_400_000;
  // Remaining per hero after the last ten.
  const left = new Map(
    POOL.map(([hero, matches, wins]) => [hero, { matches, wins }]),
  );
  for (const [hero, win] of LAST_TEN) {
    const entry = left.get(hero)!;
    entry.matches -= 1;
    if (win) entry.wins -= 1;
  }
  const queue: Array<[HeroKey, boolean]> = [];
  for (const [hero, entry] of left) {
    for (let i = 0; i < entry.matches; i += 1)
      queue.push([hero, i < entry.wins]);
  }
  // Interleave heroes so every day mixes them.
  queue.sort(
    (a, b) =>
      ((POOL.findIndex((p) => p[0] === a[0]) * 7) % 5) -
      ((POOL.findIndex((p) => p[0] === b[0]) * 7) % 5),
  );
  const todayStart = Math.floor(NOW_S / DAY_S) * DAY_S;

  // Older days (29 → 1 days ago), ~3.5 a day.
  let cursor = 0;
  for (let day = 29; day >= 1 && cursor < queue.length; day -= 1) {
    const perDay = day % 3 === 0 ? 4 : 3;
    for (let n = 0; n < perDay && cursor < queue.length; n += 1) {
      const [hero, win] = queue[cursor]!;
      cursor += 1;
      const at = todayStart - day * DAY_S + 14 * 3600 + n * 2400;
      const badge = RANK_DAYS[29 - day]!;
      const ranked = n % 3 !== 2;
      rows.push(
        row(
          (id += 1),
          hero,
          win,
          at,
          { k: 6, d: 5, a: 9, souls: 30_000, seconds: 1900 },
          ranked
            ? { badge: badge || null, delta: null, calibration: badge === 0 }
            : { badge: null, delta: null, calibration: false },
        ),
      );
    }
  }
  // Whatever did not fit goes two days back.
  while (cursor < queue.length) {
    const [hero, win] = queue[cursor]!;
    cursor += 1;
    rows.push(
      row(
        (id += 1),
        hero,
        win,
        todayStart - 2 * DAY_S + 8 * 3600 + cursor * 60,
        {
          k: 5,
          d: 5,
          a: 8,
          souls: 28_000,
          seconds: 1800,
        },
        { badge: null, delta: null, calibration: false },
      ),
    );
  }
  // The last ten: today and yesterday, newest last in time.
  LAST_TEN.forEach(([hero, win, k, d, a, souls, seconds], index) => {
    const at = NOW_S - 2 * 3600 - index * 2700 - seconds;
    rows.push(
      row(
        (id += 1) + 1000,
        hero,
        win,
        at,
        { k, d, a, souls, seconds },
        {
          badge: index < 3 ? 74 : 73,
          delta: index === 0 ? 440 : win ? 90 : -110,
          calibration: false,
        },
      ),
    );
  });
  // Older history (outside every range).
  for (let i = 0; i < 528; i += 1) {
    rows.push(
      row(
        100_000_000 + i,
        "haze",
        i % 2 === 0,
        NOW_S - (60 + i) * DAY_S,
        {
          k: 5,
          d: 5,
          a: 5,
          souls: 25_000,
          seconds: 1800,
        },
        { badge: 63, delta: null, calibration: false },
      ),
    );
  }
  return rows.sort((a, b) => b.start_time - a.start_time);
}

const HISTORY = fixtureHistory();

/** Kiwi's digest: Emissary IV, 112 matches, the mockup's numbers. */
export const FIXTURE_DIGEST: DeadlockDigest = buildDigest({
  history: HISTORY,
  rank: { account_id: 1, badge: 74 },
  extras: {
    // The newest match (HISTORY is newest first).
    match_id: HISTORY[0]!.match_id,
    level: 30,
    hero_damage: 24_100,
    objective_damage: 9_800,
    items: Object.values(FIXTURE_ITEMS),
  },
  manifest: FIXTURE_MANIFEST,
  now: NOW_S,
});

/** Season 2 placement: 5 of 8 played, Ritualist VI last season. */
export const FIXTURE_DIGEST_PLACEMENT: DeadlockDigest = {
  ...FIXTURE_DIGEST,
  rank: { badge: 0, calibration_left: 3 },
  previous_badge: 66,
};

/** Last match 41 days ago: all-time numbers, Ritualist III. */
export const FIXTURE_DIGEST_LAPSED: DeadlockDigest = {
  ...FIXTURE_DIGEST,
  rank: { badge: 63, calibration_left: null },
  last_played_at: NOW_S - 41 * DAY_S,
  last_ranked_at: Date.parse("2026-09-29T19:00:00Z") / 1000,
  season: FIXTURE_MANIFEST.seasons[0]!,
  all_time: { matches: 640, wins: 326 },
};

const fetchedAt = (minutesAgo: number) =>
  new Date(FIXTURE_NOW - minutesAgo * 60_000).toISOString();

export const FIXTURE_DEADLOCK: Record<
  | "ok"
  | "notLinked"
  | "pending"
  | "empty"
  | "protected"
  | "stale"
  | "placement"
  | "lapsed",
  DeadlockBoardData
> = {
  ok: {
    state: "ok",
    digest: FIXTURE_DIGEST,
    fetched_at: fetchedAt(4),
    stale: false,
    persona: "kiwi_yaps",
  },
  notLinked: {
    state: "not_linked",
    digest: null,
    fetched_at: null,
    stale: false,
  },
  pending: {
    state: "pending",
    digest: null,
    fetched_at: null,
    stale: false,
    persona: "kiwi_yaps",
  },
  empty: {
    state: "empty",
    digest: null,
    fetched_at: fetchedAt(3),
    stale: false,
    persona: "kiwi_yaps",
  },
  protected: {
    state: "protected",
    digest: null,
    fetched_at: fetchedAt(30),
    stale: false,
    persona: "kiwi_yaps",
  },
  stale: {
    state: "ok",
    digest: FIXTURE_DIGEST,
    fetched_at: fetchedAt(6 * 60),
    stale: true,
  },
  placement: {
    state: "ok",
    digest: FIXTURE_DIGEST_PLACEMENT,
    fetched_at: fetchedAt(4),
    stale: false,
  },
  lapsed: {
    state: "ok",
    digest: FIXTURE_DIGEST_LAPSED,
    fetched_at: fetchedAt(60),
    stale: false,
  },
};

/** Yap friends in Kiwi's party (seed them as server members). */
export const FIXTURE_PARTY = [
  { id: "u-mo", username: "mo", display_name: "Mo", avatar_url: null },
  { id: "u-ash", username: "ash", display_name: "Ash", avatar_url: null },
] as const;

const base = (minutes: number): GameActivity => ({
  kind: "playing",
  name: "Deadlock",
  app_id: DEADLOCK_APP_ID,
  started_at: FIXTURE_NOW - minutes * 60_000,
  source: "detected",
});

/** What presence carries for each live variant. */
export const FIXTURE_LIVE: Record<
  "match" | "queue" | "hideout" | "playing",
  GameActivity
> = {
  match: {
    ...base(72),
    details: "Ranked · Haze",
    state: "In match",
    game: {
      kind: "deadlock",
      phase: "in_match",
      hero: "haze",
      mode: "ranked",
      phase_started_at: FIXTURE_NOW - (23 * 60 + 14) * 1000,
      party_user_ids: ["u-mo", "u-ash"],
    },
  },
  queue: {
    ...base(40),
    game: {
      kind: "deadlock",
      phase: "queue",
      hero: "haze",
      mode: "ranked",
      phase_started_at: FIXTURE_NOW - 161_000,
      party_user_ids: ["u-mo"],
    },
  },
  hideout: {
    ...base(72),
    game: { kind: "deadlock", phase: "hideout", hero: "haze" },
  },
  playing: base(72),
};
