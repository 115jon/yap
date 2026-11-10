/**
 * The Deadlock widget's shared model: the digest the server keeps per
 * Steam account (a few KB reduced from deadlock-api.com's history, rank and
 * match metadata), what the board sends a viewer, rank names, the mirrored
 * art's paths and the live state presence can carry. Pure: no server-only
 * imports and no DOM, so the fetcher, the board route, the web widget and
 * the tests share one reduction.
 *
 * Counting: every match the history has counts (ranked, unranked, Street
 * Brawl); only the rank and its history come from ranked matches.
 */

// ── Identity ───────────────────────────────────────────────────────────────

/** The game catalog's id for Deadlock (a Discord application id). */
export const DEADLOCK_APP_ID = "1276737795012165766";
export const DEADLOCK_STEAM_APP_ID = "1422450";
/** The catalog icon (served from Yap's storage by /api/game-icons). */
export const DEADLOCK_ICON_PATH = `/api/game-icons/${DEADLOCK_APP_ID}/08e679bae42c738a66c1b2e0e546ecc0.png`;

/** Credit shown in the widget's tooltip and the owner's setup. */
export const DEADLOCK_CREDIT =
  "Stats from deadlock-api.com · Deadlock © Valve, not affiliated";

/** Whether a presence activity is Deadlock. */
export function isDeadlockActivity(
  activity: { app_id?: string; steam_app_id?: string; name?: string } | null,
): boolean {
  if (!activity) return false;
  return (
    activity.app_id === DEADLOCK_APP_ID ||
    activity.steam_app_id === DEADLOCK_STEAM_APP_ID ||
    activity.name?.trim().toLowerCase() === "deadlock"
  );
}

// ── Ranks ──────────────────────────────────────────────────────────────────

export const DEADLOCK_TIERS = [
  "Obscurus",
  "Initiate",
  "Seeker",
  "Acolyte",
  "Sentinel",
  "Mystic",
  "Ritualist",
  "Emissary",
  "Oracle",
  "Phantom",
  "Ascendant",
  "Eternus",
] as const;

const ROMAN = ["", "I", "II", "III", "IV", "V", "VI"];

/** "Emissary IV" for badge 74; "Unranked" for 0. */
export function rankName(badge: number): string {
  const tier = Math.floor(badge / 10);
  const sub = badge % 10;
  if (badge <= 0 || tier < 1 || tier >= DEADLOCK_TIERS.length)
    return "Unranked";
  return sub >= 1 && sub <= 6
    ? `${DEADLOCK_TIERS[tier]} ${ROMAN[sub]}`
    : DEADLOCK_TIERS[tier]!;
}

export function tierName(badge: number): string {
  const tier = Math.floor(badge / 10);
  return DEADLOCK_TIERS[tier] ?? "Unranked";
}

// ── Mirrored art ───────────────────────────────────────────────────────────

/** A mirrored asset's key: hero art by class name, ranks by tier, items by id. */
export const DEADLOCK_ASSET_KEY_PATTERN = /^[a-z0-9_]{1,64}$/;

export function heroAssetKey(heroKey: string, kind: "sm" | "card"): string {
  return `hero_${heroKey}_${kind}`;
}

/** The tier badge (`rank_7`) or one division's (`rank_7_4`). */
export function rankAssetKey(badge: number): string {
  const tier = Math.max(0, Math.floor(badge / 10));
  const sub = badge % 10;
  return tier > 0 && sub >= 1 && sub <= 6
    ? `rank_${tier}_${sub}`
    : `rank_${tier}`;
}

export function itemAssetKey(itemId: number): string {
  return `item_${itemId}`;
}

/**
 * Where a mirrored image is served from Yap's own storage. Clients never
 * load Deadlock art from a third-party host.
 */
export function deadlockAssetPath(key: string): string {
  return `/api/game-assets/${DEADLOCK_APP_ID}/${key}.png`;
}

// ── The digest ─────────────────────────────────────────────────────────────

export const DEADLOCK_RANGES = ["7d", "30d", "season"] as const;
export type DeadlockRange = (typeof DEADLOCK_RANGES)[number];

export type DeadlockMode = "ranked" | "unranked" | "brawl" | "other";

export interface DeadlockMatch {
  id: number;
  hero: number;
  win: boolean;
  k: number;
  d: number;
  a: number;
  /** Net worth at the end (souls). */
  souls: number;
  last_hits: number;
  /** Seconds. */
  duration: number;
  /** Unix seconds the match started. */
  at: number;
  mode: DeadlockMode;
  /** Ranked: the badge shown for the match and the points it moved. */
  badge: number | null;
  delta: number | null;
}

export interface DeadlockLastMatch extends DeadlockMatch {
  level: number | null;
  hero_damage: number | null;
  objective_damage: number | null;
  /** Upgrade item ids still held at the end, purchase order, at most 6. */
  items: number[];
}

export interface DeadlockPoolEntry {
  hero: number;
  matches: number;
  wins: number;
}

export interface DeadlockRangeStats {
  matches: number;
  wins: number;
  kills: number;
  deaths: number;
  assists: number;
  seconds: number;
  /** Ranked points gained (net) in the range. */
  rank_delta: number;
  /** Most played first, at most 5. */
  pool: DeadlockPoolEntry[];
}

export interface DeadlockHeroInfo {
  /** The art key: `haze` for `hero_haze`. */
  key: string;
  name: string;
  /** "#AC6133": the hero's colour, for the art band. */
  color: string;
}

export interface DeadlockSeason {
  name: string;
  /** Unix seconds. */
  start: number;
  calibration_matches: number;
}

export interface DeadlockDigest {
  v: 1;
  /** Unix seconds the digest was computed. */
  computed_at: number;
  rank: {
    badge: number;
    /** Placement matches left this season; null when placed or unknown. */
    calibration_left: number | null;
  } | null;
  /** The last non-zero badge before this season began. */
  previous_badge: number | null;
  season: DeadlockSeason | null;
  ranges: Record<DeadlockRange, DeadlockRangeStats>;
  all_time: { matches: number; wins: number };
  /** Newest first, at most 10. */
  last10: DeadlockMatch[];
  last_match: DeadlockLastMatch | null;
  /**
   * The badge at the end of each of the last 30 UTC days, oldest first:
   * null before the first ranked match in view, 0 while placing.
   */
  rank_days: Array<number | null>;
  /** ISO day of `rank_days[0]`. */
  rank_days_start: string;
  /** Unix seconds of the newest match, and of the newest ranked one. */
  last_played_at: number | null;
  last_ranked_at: number | null;
  /** Every hero and item the digest names. */
  heroes: Record<string, DeadlockHeroInfo>;
  items: Record<string, { name: string }>;
}

// ── Reduction (deadlock-api shapes in, digest out) ─────────────────────────

/** One row of `/v1/players/{id}/match-history`, as far as Yap reads it. */
export interface ApiHistoryRow {
  match_id: number;
  hero_id: number;
  hero_level?: number;
  start_time: number;
  game_mode?: number;
  match_mode?: number;
  player_team?: number;
  player_kills?: number;
  player_deaths?: number;
  player_assists?: number;
  net_worth?: number;
  last_hits?: number;
  match_duration_s?: number;
  match_result?: number;
  player_match_outcome?: number;
  ranked_display_badge?: number | null;
  ranked_delta?: number | null;
  ranked_calibration_match?: number | boolean | null;
}

/** One `/v1/players/rank` entry. */
export interface ApiRank {
  account_id: number;
  badge: number;
  rank?: number;
  subrank?: number;
  last_match?: {
    match_id?: number;
    start_time?: number;
    player_rank_initial_calibration_games?: number;
  } | null;
}

/** The player's part of `/v1/matches/{id}/metadata`, as Yap keeps it. */
export interface MatchExtras {
  match_id: number;
  level: number | null;
  hero_damage: number | null;
  objective_damage: number | null;
  items: number[];
}

export interface DeadlockManifest {
  heroes: Record<string, DeadlockHeroInfo>;
  /** Upgrade (shop) items only. */
  items: Record<string, { name: string }>;
  seasons: DeadlockSeason[];
}

const DAY_S = 86_400;

function num(value: unknown, fallback = 0): number {
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}

export function matchMode(row: ApiHistoryRow): DeadlockMode {
  if (row.game_mode === 4) return "brawl";
  if (row.match_mode === 4) return "ranked";
  if (row.match_mode === 1) return "unranked";
  return "other";
}

export function didWin(row: ApiHistoryRow): boolean {
  if (row.player_match_outcome === 1) return true;
  if (row.player_match_outcome === 2) return false;
  return (
    row.match_result !== undefined &&
    row.player_team !== undefined &&
    row.match_result === row.player_team
  );
}

function toMatch(row: ApiHistoryRow): DeadlockMatch {
  const mode = matchMode(row);
  const badge = num(row.ranked_display_badge, 0);
  return {
    id: num(row.match_id),
    hero: num(row.hero_id),
    win: didWin(row),
    k: num(row.player_kills),
    d: num(row.player_deaths),
    a: num(row.player_assists),
    souls: num(row.net_worth),
    last_hits: num(row.last_hits),
    duration: num(row.match_duration_s),
    at: num(row.start_time),
    mode,
    badge: mode === "ranked" && badge > 0 ? badge : null,
    delta:
      mode === "ranked" && typeof row.ranked_delta === "number"
        ? row.ranked_delta
        : null,
  };
}

function emptyRange(): DeadlockRangeStats {
  return {
    matches: 0,
    wins: 0,
    kills: 0,
    deaths: 0,
    assists: 0,
    seconds: 0,
    rank_delta: 0,
    pool: [],
  };
}

function rangeOf(rows: DeadlockMatch[]): DeadlockRangeStats {
  const range = emptyRange();
  const byHero = new Map<number, DeadlockPoolEntry>();
  for (const match of rows) {
    range.matches += 1;
    if (match.win) range.wins += 1;
    range.kills += match.k;
    range.deaths += match.d;
    range.assists += match.a;
    range.seconds += match.duration;
    range.rank_delta += match.delta ?? 0;
    const hero = byHero.get(match.hero) ?? {
      hero: match.hero,
      matches: 0,
      wins: 0,
    };
    hero.matches += 1;
    if (match.win) hero.wins += 1;
    byHero.set(match.hero, hero);
  }
  range.pool = [...byHero.values()]
    .sort((a, b) => b.matches - a.matches || b.wins - a.wins || a.hero - b.hero)
    .slice(0, 5);
  return range;
}

function isoDay(unixSeconds: number): string {
  return new Date(unixSeconds * 1000).toISOString().slice(0, 10);
}

/** The season `at` falls in (the latest one started by then). */
export function seasonAt(
  seasons: readonly DeadlockSeason[],
  at: number,
): DeadlockSeason | null {
  let found: DeadlockSeason | null = null;
  for (const season of seasons) {
    if (season.start <= at && (!found || season.start > found.start)) {
      found = season;
    }
  }
  return found;
}

/**
 * Reduces one account's API answers to the digest. `history` is the whole
 * match history (any order; the API sends newest first); `extras` is the
 * newest match's metadata, when fetched. Nothing else from those answers
 * is kept: not other players, not the full history.
 */
export function buildDigest(input: {
  history: readonly ApiHistoryRow[];
  rank: ApiRank | null;
  extras: MatchExtras | null;
  manifest: DeadlockManifest;
  /** Unix seconds. */
  now: number;
}): DeadlockDigest {
  const { manifest, now } = input;
  const rows = input.history.filter(
    (row): row is ApiHistoryRow => !!row && typeof row === "object",
  );
  const matches = rows
    .filter((row) => num(row.match_id) > 0 && num(row.start_time) > 0)
    .map(toMatch)
    .sort((a, b) => b.at - a.at || b.id - a.id);
  const season = seasonAt(manifest.seasons, now);
  const within = (seconds: number) =>
    matches.filter((match) => match.at >= now - seconds);
  const ranges: Record<DeadlockRange, DeadlockRangeStats> = {
    "7d": rangeOf(within(7 * DAY_S)),
    "30d": rangeOf(within(30 * DAY_S)),
    season: season
      ? rangeOf(matches.filter((match) => match.at >= season.start))
      : emptyRange(),
  };

  const ranked = matches.filter((match) => match.mode === "ranked");
  const seasonRanked = season
    ? ranked.filter((match) => match.at >= season.start)
    : ranked;
  const placed = seasonRanked.filter((match) => match.badge !== null);
  const calibrationPlayed = rows.filter(
    (row) =>
      matchMode(row) === "ranked" &&
      (!season || num(row.start_time) >= season.start) &&
      Boolean(row.ranked_calibration_match),
  ).length;
  const badge = Math.max(0, num(input.rank?.badge));
  let calibrationLeft: number | null = null;
  if (badge === 0 && season && placed.length === 0) {
    calibrationLeft = Math.max(
      0,
      season.calibration_matches - calibrationPlayed,
    );
  }
  const previous = season
    ? ranked.find((match) => match.at < season.start && match.badge)
    : undefined;

  // Rank at the end of each of the last 30 UTC days.
  const todayStart = Math.floor(now / DAY_S) * DAY_S;
  const firstDay = todayStart - 29 * DAY_S;
  const oldestFirst = [...ranked].reverse();
  const rankDays: Array<number | null> = [];
  let cursor = 0;
  let current: number | null = null;
  for (const before of oldestFirst) {
    if (before.at >= firstDay) break;
    current = before.badge ?? current;
    cursor += 1;
  }
  for (let day = 0; day < 30; day += 1) {
    const dayStart = firstDay + day * DAY_S;
    const dayEnd = dayStart + DAY_S;
    if (season && dayStart <= season.start && season.start < dayEnd) {
      current = current === null ? null : 0;
    }
    while (cursor < oldestFirst.length && oldestFirst[cursor]!.at < dayEnd) {
      const match = oldestFirst[cursor]!;
      current =
        match.badge ?? (season && match.at >= season.start ? 0 : current);
      cursor += 1;
    }
    rankDays.push(current);
  }
  // Today ends at the live rank when the API knows a newer one.
  if (badge > 0 && rankDays.length > 0) rankDays[rankDays.length - 1] = badge;

  const last10 = matches.slice(0, 10);
  const newest = matches[0] ?? null;
  const extras =
    newest && input.extras && input.extras.match_id === newest.id
      ? input.extras
      : null;
  const lastMatch: DeadlockLastMatch | null = newest
    ? {
        ...newest,
        level: extras?.level ?? null,
        hero_damage: extras?.hero_damage ?? null,
        objective_damage: extras?.objective_damage ?? null,
        items: (extras?.items ?? [])
          .filter((id) => manifest.items[String(id)])
          .slice(-6),
      }
    : null;

  const heroes: Record<string, DeadlockHeroInfo> = {};
  const nameHero = (id: number) => {
    const info = manifest.heroes[String(id)];
    if (info) heroes[String(id)] = info;
  };
  for (const range of Object.values(ranges)) {
    for (const entry of range.pool) nameHero(entry.hero);
  }
  for (const match of last10) nameHero(match.hero);
  const items: Record<string, { name: string }> = {};
  for (const id of lastMatch?.items ?? []) {
    const info = manifest.items[String(id)];
    if (info) items[String(id)] = info;
  }

  return {
    v: 1,
    computed_at: now,
    rank: input.rank ? { badge, calibration_left: calibrationLeft } : null,
    previous_badge: previous?.badge ?? null,
    season,
    ranges,
    all_time: {
      matches: matches.length,
      wins: matches.filter((match) => match.win).length,
    },
    last10,
    last_match: lastMatch,
    rank_days: rankDays,
    rank_days_start: isoDay(firstDay),
    last_played_at: newest?.at ?? null,
    last_ranked_at: ranked[0]?.at ?? null,
    heroes,
    items,
  };
}

/**
 * The player's numbers from `/v1/matches/{id}/metadata`: level, damage and
 * the upgrades still held at the end. Every other player in the answer is
 * dropped. Null when the account is not in the match.
 */
export function reduceMatchMetadata(
  body: unknown,
  accountId: number,
): MatchExtras | null {
  const root = (body ?? {}) as Record<string, unknown>;
  const info = (root.match_info ?? root.match ?? root) as Record<
    string,
    unknown
  >;
  const players = Array.isArray(info.players)
    ? info.players
    : root.player
      ? [root.player]
      : [];
  const player = players.find(
    (entry) =>
      entry &&
      typeof entry === "object" &&
      (entry as { account_id?: unknown }).account_id === accountId,
  ) as Record<string, unknown> | undefined;
  if (!player) return null;
  const stats = Array.isArray(player.stats)
    ? (player.stats.at(-1) as Record<string, unknown> | undefined)
    : ((player.last_stats ?? player.final_stats) as
        | Record<string, unknown>
        | undefined);
  const held: number[] = [];
  for (const raw of Array.isArray(player.items) ? player.items : []) {
    if (!raw || typeof raw !== "object") continue;
    const item = raw as Record<string, unknown>;
    const id = num(item.item_id);
    if (!id || num(item.sold_time_s) > 0) continue;
    const at = held.indexOf(id);
    if (at >= 0) held.splice(at, 1);
    held.push(id);
  }
  const matchId = num(info.match_id, num(root.match_id));
  return {
    match_id: matchId,
    level: typeof player.level === "number" ? player.level : null,
    hero_damage:
      stats && typeof stats.player_damage === "number"
        ? stats.player_damage
        : null,
    objective_damage:
      stats && typeof stats.boss_damage === "number" ? stats.boss_damage : null,
    items: held,
  };
}

// ── What a viewer gets ─────────────────────────────────────────────────────

export type DeadlockState =
  /** No Steam connection (the owner sees "Link Steam"). */
  | "not_linked"
  /** Linked; the first fetch has not landed yet. */
  | "pending"
  | "ok"
  /** The API has no matches for the account. */
  | "empty"
  /** The account asked deadlock-api.com to delete its data (403). */
  | "protected";

export interface DeadlockBoardData {
  state: DeadlockState;
  /** The last good digest (kept through failures). */
  digest: DeadlockDigest | null;
  /** ISO time the digest was fetched. */
  fetched_at: string | null;
  /** The latest refresh failed and the copy is old: the legend shows its age. */
  stale: boolean;
  /** Owner only: the Steam name, for "Play a match on kiwi_yaps". */
  persona?: string | null;
}

/** States only the owner sees; everyone else gets no plate. */
export function isOwnerOnlyDeadlockState(state: DeadlockState): boolean {
  return state === "not_linked" || state === "empty" || state === "protected";
}

/**
 * The digest as another viewer receives it: only the owner's range (the
 * others emptied), and no real match ids. A match id finds the match's
 * public metadata, which names every player's Steam account, so others get
 * positions (1 is the newest) that only key the list.
 */
export function digestForRange(
  digest: DeadlockDigest,
  range: DeadlockRange,
): DeadlockDigest {
  const ranges = { ...digest.ranges };
  for (const key of DEADLOCK_RANGES) {
    if (key !== range) ranges[key] = emptyRange();
  }
  return {
    ...digest,
    ranges,
    last10: digest.last10.map((match, index) => ({ ...match, id: index + 1 })),
    last_match: digest.last_match ? { ...digest.last_match, id: 1 } : null,
  };
}

/** (K + A) / D with one decimal, D at least 1. */
export function kdaRatio(stats: {
  kills: number;
  deaths: number;
  assists: number;
}): number {
  return (stats.kills + stats.assists) / Math.max(1, stats.deaths);
}

export function winRate(wins: number, matches: number): number {
  return matches > 0 ? Math.round((wins / matches) * 100) : 0;
}

// ── Live state (presence) ──────────────────────────────────────────────────

export const DEADLOCK_PHASES = [
  "menu",
  "hideout",
  "queue",
  "match_intro",
  "in_match",
  "post_match",
  "spectating",
] as const;
export type DeadlockPhase = (typeof DEADLOCK_PHASES)[number];
