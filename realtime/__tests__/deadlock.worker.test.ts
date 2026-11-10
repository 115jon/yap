// The Deadlock stats fetcher against a real D1, KV and R2 (workerd) with
// deadlock-api.com mocked at the fetch edge: batching, the digest reduction
// (small, and nothing of other players or the friends list), change
// detection, Retry-After and backoff, protected accounts, stale serving, the
// R2 art copy and its route, the refresh triggers, what board viewers get,
// the live-state check, and the cleanup on unlink. Nothing here talks to the
// real API; every request the fetcher makes is recorded and asserted.

import { env } from "cloudflare:workers";
import type { GameActivity } from "@ralph/protocol";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";

import schemaSql from "../../d1_schema.sql?raw";
import {
  DEADLOCK_APP_ID,
  type DeadlockDigest,
  type DeadlockState,
} from "../../src/lib/deadlock";
import type {
  BoardResponse,
  BoardSaveResponse,
  ProfileBoard,
} from "../../src/lib/profile-board";
import {
  linkConnection,
  unlinkConnection,
} from "../../src/services/connection.service";
import { createDeadlockApi } from "../../src/services/deadlock/api";
import {
  assetObjectKey,
  MANIFEST_KV_KEY,
  mirrorDeadlockAssets,
  syncDeadlockAssets,
  type StoredManifest,
} from "../../src/services/deadlock/assets";
import { deadlockBoardData } from "../../src/services/deadlock/board";
import {
  DEADLOCK_CADENCE,
  nextDueAt,
  reconcileDeadlockTracking,
  refreshDeadlockAccounts,
  type FetcherDeps,
  type FetchStateRow,
  type RefreshOutcome,
} from "../../src/services/deadlock/fetcher";
import {
  noteDeadlockActivity,
  resolveLiveState,
} from "../../src/services/deadlock/live";
import {
  createProfileBoardHandlers,
  type ProfileBoardDeps,
} from "../../src/services/profile-board-handlers";

type RouteModule = {
  GET: (args: {
    request: Request;
    params?: { _splat?: string };
  }) => Promise<Response>;
};
const loadRoute = (path: string) =>
  import(/* @vite-ignore */ path) as Promise<RouteModule>;
const { GET: gameAssetsGet } = await loadRoute(
  "../../src/routes/api/game-assets/$",
);

const db = env.DB as unknown as D1Database;
const kv = env.CACHE as unknown as KVNamespace;
const bucket = env.BUCKET as unknown as R2Bucket;
const BASE = "https://yap.test";
const NOW = Date.parse("2026-11-09T12:00:00.000Z");
const NOW_S = NOW / 1000;
const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;
const STEAM_BASE = 76561197960265728n;

const ART = "https://assets-bucket.deadlock-api.com/assets-api-res/images";
/** Players in a match that must never reach a digest. */
const OTHER_A = 987_654_321;
const OTHER_B = 123_123_123;
const FRIENDS = [OTHER_A, OTHER_B, 555_000_111];
const UPGRADE = 2_717_130_900;
const UPGRADE_B = 600_000_002;
const UPGRADE_SOLD = 600_000_001;
/** An ability id in the item list: not a shop upgrade. */
const NOT_AN_UPGRADE = 1_385_849_824;

const PNG_BYTES = Uint8Array.from(
  atob(
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAHnOcQAAAAABJRU5ErkJggg==",
  ),
  (char) => char.charCodeAt(0),
);

function statements(sql: string): string[] {
  return sql
    .split("\n")
    .filter((line) => !line.trim().startsWith("--"))
    .join("\n")
    .split(/;\s*\n/)
    .map((statement) => statement.trim().replace(/;$/, ""))
    .filter(Boolean);
}

async function ensureColumn(table: string, column: string, type: string) {
  const { results } = await db
    .prepare(`PRAGMA table_info(${table})`)
    .all<{ name: string }>();
  if (!(results ?? []).some((row) => row.name === column)) {
    await db.prepare(`ALTER TABLE ${table} ADD COLUMN ${column} ${type}`).run();
  }
}

function manifestFixture(): StoredManifest {
  return {
    version: 1,
    read_at: NOW,
    heroes: {
      "13": { key: "haze", name: "Haze", color: "#AC6133" },
      "14": { key: "mirage", name: "Mirage", color: "#3366AA" },
    },
    items: {
      [String(UPGRADE)]: { name: "Extra Spirit" },
      [String(UPGRADE_B)]: { name: "Fleetfoot" },
      [String(UPGRADE_SOLD)]: { name: "Sold Item" },
    },
    seasons: [
      {
        name: "Beta Season 2",
        start: NOW_S - 20 * 86_400,
        calibration_matches: 8,
      },
    ],
    sources: {
      hero_haze_sm: `${ART}/heroes/haze_sm.png`,
      item_2717130900: `${ART}/items/extra_spirit.png`,
      item_600000002: `${ART}/items/fleetfoot.png`,
    },
  };
}

beforeAll(async () => {
  for (const statement of statements(schemaSql)) {
    await db.prepare(statement).run();
  }
  // Other worker test files may have made trimmed copies of these tables
  // in the same database first.
  await ensureColumn("servers", "icon_url", "TEXT");
  await ensureColumn("users", "display_name", "TEXT");
  await ensureColumn("users", "avatar_url", "TEXT");
  await ensureColumn("users", "share_activity", "INTEGER NOT NULL DEFAULT 1");
});

beforeEach(async () => {
  await db.batch([
    db.prepare(`DELETE FROM deadlock_fetch_state`),
    db.prepare(`DELETE FROM deadlock_snapshots`),
  ]);
  await kv.put(MANIFEST_KV_KEY, JSON.stringify(manifestFixture()));
});

// ── deadlock-api.com, mocked ───────────────────────────────────────────────

type Json = Record<string, unknown>;

function metadataBody(matchId: number, accountId: number) {
  const held = (id: number, sold = 0) => ({
    game_time_s: 90,
    item_id: id,
    upgrade_id: 0,
    sold_time_s: sold,
  });
  const player = (account: number, items: Json[]) => ({
    account_id: account,
    hero_id: 13,
    level: 28,
    items,
    stats: [
      { time_stamp_s: 60, player_damage: 10, boss_damage: 0 },
      { time_stamp_s: 1800, player_damage: 41_000, boss_damage: 5200 },
    ],
  });
  return {
    match_info: {
      match_id: matchId,
      duration_s: 1800,
      players: [
        player(OTHER_A, [held(UPGRADE)]),
        player(accountId, [
          held(NOT_AN_UPGRADE),
          held(UPGRADE),
          held(UPGRADE_SOLD, 900),
          held(UPGRADE_B),
        ]),
        player(OTHER_B, [held(UPGRADE_B)]),
      ],
    },
  };
}

function historyRows(
  accountId: number,
  count: number,
  newestMatchId: number,
  newestStart: number,
): Json[] {
  return Array.from({ length: count }, (_, i) => {
    const win = i % 3 !== 0;
    const ranked = i % 2 === 0;
    return {
      account_id: accountId,
      match_id: newestMatchId - i,
      hero_id: i % 2 === 0 ? 13 : 14,
      hero_level: 25,
      start_time: newestStart - i * 3 * 3600,
      game_mode: 1,
      match_mode: ranked ? 4 : 1,
      player_team: 1,
      player_kills: 5,
      player_deaths: 4,
      player_assists: 9,
      denies: 3,
      net_worth: 30_000,
      last_hits: 120,
      match_duration_s: 1800,
      match_result: win ? 1 : 0,
      objectives_mask_team0: 44_613,
      player_match_outcome: win ? 1 : 2,
      ranked_display_badge: ranked ? 74 : null,
      ranked_delta: ranked ? (win ? 100 : -100) : null,
      ranked_calibration_match: 0,
    };
  });
}

interface PlayerSetup {
  newestMatchId: number;
  newestStart?: number;
  count?: number;
  badge?: number;
  played?: number;
}

function createMock() {
  const calls: URL[] = [];
  const state = {
    ranks: new Map<number, Json>(),
    heroStats: new Map<number, Json[]>(),
    history: new Map<number, Json[]>(),
    metadata: new Map<number, unknown>(),
    protectedIds: new Set<number>(),
    versions: [1] as number[],
    heroes: [] as unknown[],
    items: [] as unknown[],
    rankAssets: [] as unknown[],
    seasons: [] as unknown[],
    /** A test's own answer for a URL; null falls through to the default. */
    override: null as null | ((url: URL) => Response | null),
  };

  const png = () =>
    new Response(PNG_BYTES.slice(), {
      status: 200,
      headers: { "content-type": "image/png" },
    });
  const notFound = () => new Response("not found", { status: 404 });
  const ids = (url: URL) =>
    (url.searchParams.get("account_ids") ?? "")
      .split(",")
      .filter(Boolean)
      .map(Number);

  const handle = (url: URL): Response => {
    if (url.hostname === "assets-bucket.deadlock-api.com") return png();
    const path = url.pathname;
    if (path === "/v1/players/rank") {
      return Response.json(
        ids(url)
          .map((id) => state.ranks.get(id))
          .filter((rank): rank is Json => !!rank),
      );
    }
    if (path === "/v1/players/hero-stats") {
      return Response.json(
        ids(url).flatMap((id) => state.heroStats.get(id) ?? []),
      );
    }
    if (path === "/v1/players/steam") {
      return Response.json(
        ids(url).map((id) => ({ account_id: id, friends: FRIENDS })),
      );
    }
    const single = /^\/v1\/players\/(\d+)\/rank$/.exec(path);
    if (single) {
      const id = Number(single[1]);
      if (state.protectedIds.has(id))
        return new Response("gone", { status: 403 });
      const rank = state.ranks.get(id);
      if (!rank) return notFound();
      const { account_id: _id, ...body } = rank;
      void _id;
      return Response.json(body);
    }
    const history = /^\/v1\/players\/(\d+)\/match-history$/.exec(path);
    if (history) {
      const rows = state.history.get(Number(history[1]));
      return rows ? Response.json(rows) : notFound();
    }
    const metadata = /^\/v1\/matches\/(\d+)\/metadata$/.exec(path);
    if (metadata) {
      const body = state.metadata.get(Number(metadata[1]));
      return body ? Response.json(body) : notFound();
    }
    if (/^\/v1\/assets\/ranks\/\d+\/\d+\/image$/.test(path)) return png();
    if (path === "/v1/assets/client-versions")
      return Response.json(state.versions);
    if (path === "/v1/assets/heroes") return Response.json(state.heroes);
    if (path === "/v1/assets/items/by-type/upgrade")
      return Response.json(state.items);
    if (path === "/v1/assets/ranks") return Response.json(state.rankAssets);
    if (path === "/v1/assets/ranked-seasons")
      return Response.json(state.seasons);
    return notFound();
  };

  const fetchImpl = async (input: RequestInfo | URL): Promise<Response> => {
    const url = new URL(
      typeof input === "string"
        ? input
        : input instanceof URL
          ? input.href
          : input.url,
    );
    calls.push(url);
    return state.override?.(url) ?? handle(url);
  };

  return {
    fetch: fetchImpl as typeof fetch,
    calls,
    state,
    /** Calls whose path matches. */
    count: (pattern: RegExp) =>
      calls.filter((url) => pattern.test(url.pathname)).length,
    /** Sets up everything the API would answer for one account. */
    setPlayer(accountId: number, setup: PlayerSetup) {
      const newestStart = setup.newestStart ?? NOW_S - 3600;
      const played = setup.played ?? 20;
      state.ranks.set(accountId, {
        account_id: accountId,
        badge: setup.badge ?? 74,
        rank: 7,
        subrank: 4,
        last_match: {
          match_id: setup.newestMatchId,
          start_time: newestStart,
          player_rank_initial_calibration_games: 0,
        },
        friends: FRIENDS,
      });
      state.heroStats.set(accountId, [
        {
          account_id: accountId,
          hero_id: 13,
          matches_played: played,
          last_played: newestStart,
          wins: 10,
          friends: FRIENDS,
        },
      ]);
      state.history.set(
        accountId,
        historyRows(
          accountId,
          setup.count ?? 60,
          setup.newestMatchId,
          newestStart,
        ),
      );
      state.metadata.set(
        setup.newestMatchId,
        metadataBody(setup.newestMatchId, accountId),
      );
    },
  };
}

type Mock = ReturnType<typeof createMock>;

function createFetcher(mock: Mock, clock: { now: number }) {
  const notified: string[] = [];
  const now = () => clock.now;
  const deps: FetcherDeps = {
    db,
    kv,
    bucket,
    api: createDeadlockApi({ fetch: mock.fetch, now }),
    fetch: mock.fetch,
    notify: async (userId) => {
      notified.push(userId);
    },
    now,
  };
  return { deps, notified };
}

// ── Fixtures ───────────────────────────────────────────────────────────────

let accountCounter = 40_000_000 + Math.floor(Math.random() * 1_000_000) * 10;
let matchCounter = 9_000_000 + Math.floor(Math.random() * 1_000_000) * 100;

function nextAccountId(): number {
  accountCounter += 1;
  return accountCounter;
}

function nextMatchId(): number {
  matchCounter += 100;
  return matchCounter;
}

function steamId64(accountId: number): string {
  return String(STEAM_BASE + BigInt(accountId));
}

async function addUser(prefix = "dl") {
  const id = `${prefix}-${crypto.randomUUID().slice(0, 8)}`;
  await db
    .prepare(
      `INSERT INTO users (id, username, created_at) VALUES (?, ?, '2026-01-01T00:00:00.000Z')`,
    )
    .bind(id, id)
    .run();
  return id;
}

async function linkSteam(
  userId: string,
  accountId: number,
  persona = "kiwi_yaps",
) {
  const result = await linkConnection(db, userId, "steam", {
    externalId: steamId64(accountId),
    displayName: persona,
    profileUrl: null,
    avatarUrl: null,
    metadata: { steam_id64: steamId64(accountId), account_id: accountId },
  });
  expect(result.ok).toBe(true);
}

/** A user with a linked Steam account the fetcher tracks (due now). */
async function addPlayer(
  options: { tracked?: boolean; persona?: string } = {},
) {
  const userId = await addUser("player");
  const accountId = nextAccountId();
  await linkSteam(userId, accountId, options.persona);
  if (options.tracked !== false) await trackAccount(userId, accountId);
  return { userId, accountId };
}

async function trackAccount(
  userId: string,
  accountId: number,
  dueAt = NOW - 1000,
) {
  await db
    .prepare(
      `INSERT INTO deadlock_fetch_state (account_id, user_id, next_due_at) VALUES (?, ?, ?)`,
    )
    .bind(accountId, userId, dueAt)
    .run();
}

async function fetchStateOf(accountId: number) {
  return db
    .prepare(`SELECT * FROM deadlock_fetch_state WHERE account_id = ?`)
    .bind(accountId)
    .first<FetchStateRow>();
}

interface SnapshotRow {
  account_id: number;
  status: string;
  digest: string | null;
  fetched_at: number | null;
  newest_match_id: number | null;
}

async function snapshotOf(accountId: number) {
  return db
    .prepare(`SELECT * FROM deadlock_snapshots WHERE account_id = ?`)
    .bind(accountId)
    .first<SnapshotRow>();
}

function digestOf(row: SnapshotRow | null): DeadlockDigest {
  expect(row?.digest).toBeTruthy();
  return JSON.parse(row!.digest!) as DeadlockDigest;
}

function handlers(overrides: Partial<ProfileBoardDeps> = {}) {
  return createProfileBoardHandlers({
    db,
    bucket,
    authenticate: async (request) => {
      const user = request.headers.get("x-test-user");
      return user
        ? { userId: user }
        : Response.json({ error: "Unauthorized" }, { status: 401 });
    },
    rateLimit: async () => null,
    now: () => NOW,
    ...overrides,
  });
}

function req(method: string, path: string, user: string, body?: unknown) {
  const headers = new Headers({ "x-test-user": user });
  if (body !== undefined) headers.set("Content-Type", "application/json");
  return new Request(`${BASE}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

function deadlockWidget(
  id = "deadlock1",
  range: "7d" | "30d" | "season" = "30d",
) {
  return {
    id,
    type: "deadlock",
    cfg: { at_rest: "career", range, show_party: true },
  };
}

async function saveBoard(
  user: string,
  widgets: unknown[],
  version = 0,
  deps: Partial<ProfileBoardDeps> = {},
) {
  const response = await handlers(deps).putBoard(
    req("PUT", "/api/users/me/board", user, { board: { widgets }, version }),
    "me",
  );
  expect(response.status).toBe(200);
  return (await response.json()) as BoardSaveResponse;
}

async function viewBoard(
  owner: string,
  viewer: string,
  deps: Partial<ProfileBoardDeps> = {},
) {
  const response = await handlers(deps).getBoard(
    req("GET", `/api/users/${owner}/board`, viewer),
    owner,
  );
  expect(response.status).toBe(200);
  return (await response.json()) as BoardResponse;
}

/** A player with the widget on a saved board and a first good fetch at `at`. */
async function addFetchedPlayer(
  mock: Mock,
  clock: { now: number },
  options: { range?: "7d" | "30d" | "season"; persona?: string } = {},
) {
  const player = await addPlayer(options);
  await saveBoard(player.userId, [deadlockWidget("deadlock1", options.range)]);
  mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
  const { deps } = createFetcher(mock, clock);
  const outcome = onlyOutcome(
    await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
  );
  expect(outcome.result).toBe("updated");
  return player;
}

function onlyOutcome(outcomes: RefreshOutcome[]): RefreshOutcome {
  expect(outcomes).toHaveLength(1);
  return outcomes[0]!;
}

function widgetTypes(board: ProfileBoard): string[] {
  return board.widgets.map((widget) => widget.type);
}

// ── Fetching ───────────────────────────────────────────────────────────────

describe("batching", () => {
  it("fetches three due accounts with one rank call and one hero-stats call per mode", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const players = [await addPlayer(), await addPlayer(), await addPlayer()];
    for (const player of players) {
      mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
    }
    const { deps } = createFetcher(mock, clock);

    const outcomes = await refreshDeadlockAccounts(deps);

    expect(outcomes).toHaveLength(3);
    expect(outcomes.map((o) => o.result)).toEqual([
      "updated",
      "updated",
      "updated",
    ]);
    const wanted = players.map((p) => p.accountId).sort();
    const ids = (url: URL) =>
      url.searchParams.get("account_ids")!.split(",").map(Number).sort();
    const ranks = mock.calls.filter((url) =>
      /^\/v1\/players\/rank$/.test(url.pathname),
    );
    expect(ranks).toHaveLength(1);
    expect(ids(ranks[0]!)).toEqual(wanted);
    // Hero stats: normal games, and Street Brawl apart (both batched).
    const stats = mock.calls.filter((url) =>
      /^\/v1\/players\/hero-stats$/.test(url.pathname),
    );
    expect(stats.map((url) => url.searchParams.get("game_mode"))).toEqual([
      null,
      "street_brawl",
    ]);
    for (const url of stats) expect(ids(url)).toEqual(wanted);
    // The history is per account.
    const historyCalls = mock.calls
      .filter((url) => /^\/v1\/players\/\d+\/match-history$/.test(url.pathname))
      .map((url) => Number(url.pathname.split("/")[3]))
      .sort();
    expect(historyCalls).toEqual(wanted);
    // No single rank lookups: every account was in the batch.
    expect(mock.count(/^\/v1\/players\/\d+\/rank$/)).toBe(0);
    for (const player of players) {
      expect((await snapshotOf(player.accountId))?.status).toBe("ok");
    }
  });
});

describe("the digest", () => {
  it("stores a small digest with the player's own numbers and nothing of anyone else", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    const matchId = nextMatchId();
    mock.setPlayer(player.accountId, { newestMatchId: matchId, count: 60 });
    const { deps } = createFetcher(mock, clock);

    await refreshDeadlockAccounts(deps);

    const row = await snapshotOf(player.accountId);
    expect(row?.status).toBe("ok");
    expect(row?.newest_match_id).toBe(matchId);
    expect(new TextEncoder().encode(row!.digest!).byteLength).toBeLessThan(
      8 * 1024,
    );
    const digest = digestOf(row);
    expect(digest.rank?.badge).toBe(74);
    expect(Object.keys(digest.ranges).sort()).toEqual(["30d", "7d", "season"]);
    expect(digest.ranges["7d"].matches).toBeGreaterThan(0);
    expect(digest.ranges.season.matches).toBe(60);
    expect(digest.all_time.matches).toBe(60);
    expect(digest.last10.length).toBeLessThanOrEqual(10);
    expect(digest.last10).toHaveLength(10);
    expect(digest.last_match?.id).toBe(matchId);
    expect(digest.last_match?.level).toBe(28);
    expect(digest.last_match?.hero_damage).toBe(41_000);
    // The items the player held at the end, shop upgrades only.
    expect(digest.last_match?.items).toEqual([UPGRADE, UPGRADE_B]);
    expect(Object.keys(digest.items).sort()).toEqual(
      [String(UPGRADE), String(UPGRADE_B)].sort(),
    );

    const raw = row!.digest!;
    for (const other of [...FRIENDS, OTHER_A, OTHER_B]) {
      expect(raw).not.toContain(String(other));
    }
    expect(raw).not.toContain("friends");
    expect(raw).not.toContain("account_id");
  });

  it("never asks for the Steam friends list", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const normal = await addPlayer();
    const hidden = await addPlayer();
    mock.setPlayer(normal.accountId, { newestMatchId: nextMatchId() });
    mock.state.protectedIds.add(hidden.accountId);
    const { deps } = createFetcher(mock, clock);

    await refreshDeadlockAccounts(deps, { reason: "cron" });
    clock.now = NOW + HOUR;
    await refreshDeadlockAccounts(deps, {
      accountIds: [normal.accountId],
      reason: "manual",
    });
    await refreshDeadlockAccounts(deps, {
      accountIds: [hidden.accountId],
      reason: "post_game",
    });

    expect(mock.calls.length).toBeGreaterThan(0);
    expect(
      mock.calls.some((url) => url.pathname.includes("/players/steam")),
    ).toBe(false);
    expect(mock.calls.some((url) => url.search.includes("friends"))).toBe(
      false,
    );
    expect(digestOf(await snapshotOf(normal.accountId))).not.toHaveProperty(
      "friends",
    );
  });
});

describe("change detection", () => {
  it("refetches the history and metadata only when the signal moves", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    const first = nextMatchId();
    mock.setPlayer(player.accountId, { newestMatchId: first, played: 20 });
    const { deps } = createFetcher(mock, clock);
    const historyRoute = /^\/v1\/players\/\d+\/match-history$/;
    const metadataRoute = /^\/v1\/matches\/\d+\/metadata$/;

    await refreshDeadlockAccounts(deps);
    expect(mock.count(historyRoute)).toBe(1);
    expect(mock.count(metadataRoute)).toBe(1);
    const firstFetchedAt = (await snapshotOf(player.accountId))!.fetched_at;
    expect(firstFetchedAt).toBe(NOW);

    // Same rank and hero stats, history read 7 hours ago: no history call.
    clock.now = NOW + 7 * HOUR;
    const quiet = onlyOutcome(
      await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
    );
    expect(quiet.result).toBe("unchanged");
    expect(mock.count(/^\/v1\/players\/rank$/)).toBe(2);
    expect(mock.count(/^\/v1\/players\/hero-stats$/)).toBe(4);
    expect(mock.count(historyRoute)).toBe(1);
    expect(mock.count(metadataRoute)).toBe(1);

    // A new match: the signal moves, history and its metadata are read.
    const second = first + 1;
    clock.now = NOW + 8 * HOUR;
    mock.setPlayer(player.accountId, {
      newestMatchId: second,
      newestStart: NOW_S + 7 * 3600,
      played: 21,
    });
    const moved = onlyOutcome(
      await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
    );
    expect(moved.result).toBe("updated");
    expect(mock.count(historyRoute)).toBe(2);
    expect(mock.count(metadataRoute)).toBe(2);
    const afterMove = await snapshotOf(player.accountId);
    expect(afterMove?.newest_match_id).toBe(second);
    expect(digestOf(afterMove).last_match?.id).toBe(second);

    // The signal moves again (more matches played) with the same newest
    // match: the history is read, the metadata is not.
    clock.now = NOW + 9 * HOUR;
    mock.state.heroStats.set(player.accountId, [
      {
        account_id: player.accountId,
        hero_id: 13,
        matches_played: 22,
        last_played: NOW_S + 8 * 3600,
        wins: 11,
      },
    ]);
    await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] });
    expect(mock.count(historyRoute)).toBe(3);
    expect(mock.count(metadataRoute)).toBe(2);
    // The reused metadata kept the match's items.
    expect(
      digestOf(await snapshotOf(player.accountId)).last_match?.items,
    ).toEqual([UPGRADE, UPGRADE_B]);
  });
});

describe("failures and backoff", () => {
  /** A player with one good fetch at NOW, then `answer` for the rank batch. */
  async function failing(answer: (now: number) => Response, at = NOW + HOUR) {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
    const { deps } = createFetcher(mock, clock);
    await refreshDeadlockAccounts(deps);
    const good = await snapshotOf(player.accountId);
    clock.now = at;
    mock.state.override = (url) =>
      url.pathname === "/v1/players/rank" ? answer(clock.now) : null;
    const outcome = onlyOutcome(
      await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
    );
    return { mock, clock, deps, player, good, outcome };
  }

  it("backs off after a 429 by the longer of the backoff and Retry-After and keeps the last good digest", async () => {
    const { mock, clock, deps, player, good, outcome } = await failing(
      () =>
        new Response(null, { status: 429, headers: { "Retry-After": "120" } }),
    );
    expect(outcome).toEqual({
      account_id: player.accountId,
      result: "failed",
      error: "rate_limited",
    });
    const T1 = NOW + HOUR;
    let state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(1);
    expect(state?.last_error).toBe("rate_limited");
    // 5 minutes of backoff beats 2 minutes of Retry-After.
    expect(state?.retry_at).toBe(T1 + DEADLOCK_CADENCE.backoffBase);
    expect(await snapshotOf(player.accountId)).toEqual(good);

    // Before retry_at: neither a named account nor the cron touches the API.
    const before = mock.calls.length;
    clock.now = T1 + MINUTE;
    expect(
      await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
    ).toEqual([]);
    expect(await refreshDeadlockAccounts(deps)).toEqual([]);
    expect(mock.calls.length).toBe(before);

    // The second failure waits the longer of 10 minutes and a 30 minute Retry-After.
    const T2 = state!.retry_at!;
    clock.now = T2;
    mock.state.override = (url) =>
      url.pathname === "/v1/players/rank"
        ? new Response(null, {
            status: 429,
            headers: { "Retry-After": "1800" },
          })
        : null;
    await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] });
    state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(2);
    expect(state?.retry_at).toBe(T2 + 30 * MINUTE);
    expect(await snapshotOf(player.accountId)).toEqual(good);

    // A success resets the count and the wait.
    clock.now = state!.retry_at!;
    mock.state.override = null;
    const healed = onlyOutcome(
      await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] }),
    );
    expect(healed.result).not.toBe("failed");
    state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(0);
    expect(state?.retry_at).toBeNull();
    expect(state?.last_error).toBeNull();
    expect((await snapshotOf(player.accountId))?.fetched_at).toBe(clock.now);
  });

  it("reads a 503 Retry-After sent as an HTTP date", async () => {
    const T1 = NOW + HOUR;
    const { player, outcome, good } = await failing(
      (now) =>
        new Response(null, {
          status: 503,
          headers: { "Retry-After": new Date(now + 10 * MINUTE).toUTCString() },
        }),
      T1,
    );
    expect(outcome.error).toBe("rate_limited");
    const state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(1);
    expect(state?.retry_at).toBe(T1 + 10 * MINUTE);
    expect(state?.next_due_at).toBe(T1 + 10 * MINUTE);
    expect(await snapshotOf(player.accountId)).toEqual(good);
  });

  it("waits 10 minutes after a 504 without a Retry-After", async () => {
    const T1 = NOW + HOUR;
    const { player, outcome, good } = await failing(
      () => new Response(null, { status: 504 }),
      T1,
    );
    expect(outcome.error).toBe("timeout");
    const state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(1);
    expect(state?.last_error).toBe("timeout");
    expect(state?.retry_at).toBe(T1 + 10 * MINUTE);
    expect(await snapshotOf(player.accountId)).toEqual(good);
  });
});

describe("protected accounts", () => {
  it("marks an account the API will not answer for as protected and shows the plate to its owner only", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const owner = await addPlayer();
    const viewer = await addUser("viewer");
    await saveBoard(owner.userId, [deadlockWidget()]);
    // Not in the rank batch or the hero stats; the single lookup says 403.
    mock.state.protectedIds.add(owner.accountId);
    const { deps, notified } = createFetcher(mock, clock);

    const outcome = onlyOutcome(await refreshDeadlockAccounts(deps));

    expect(outcome.result).toBe("protected");
    expect(
      mock.count(new RegExp(`^/v1/players/${owner.accountId}/rank$`)),
    ).toBe(1);
    expect(mock.count(/match-history/)).toBe(0);
    const snapshot = await snapshotOf(owner.accountId);
    expect(snapshot?.status).toBe("protected");
    expect(snapshot?.digest).toBeNull();
    const state = await fetchStateOf(owner.accountId);
    expect(state?.last_error).toBe("protected");
    expect(state?.next_due_at).toBe(NOW + DEADLOCK_CADENCE.lapsed);
    expect(notified).toEqual([owner.userId]);

    const others = await viewBoard(owner.userId, viewer);
    expect(widgetTypes(others.board)).not.toContain("deadlock");
    expect(others.data.deadlock).toBeUndefined();

    const mine = await viewBoard(owner.userId, owner.userId);
    expect(widgetTypes(mine.board)).toContain("deadlock");
    expect(mine.data.deadlock?.state).toBe<DeadlockState>("protected");
    expect(mine.data.deadlock?.digest).toBeNull();
  });
});

describe("stale serving", () => {
  it("keeps showing the last good digest, marked stale, when refreshes keep failing", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addFetchedPlayer(mock, clock);
    const viewer = await addUser("viewer");
    const good = digestOf(await snapshotOf(player.accountId));

    // The next refresh fails.
    clock.now = NOW + 10 * MINUTE;
    mock.state.override = (url) =>
      url.pathname === "/v1/players/rank"
        ? new Response(null, { status: 503 })
        : null;
    const { deps } = createFetcher(mock, clock);
    await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] });
    expect((await fetchStateOf(player.accountId))?.fail_count).toBe(1);

    // Under 30 minutes old: failing but not yet marked.
    const fresh = await deadlockBoardData(db, {
      ownerId: player.userId,
      isOwner: false,
      range: "30d",
      track: false,
      now: NOW + 20 * MINUTE,
    });
    expect(fresh.data.state).toBe("ok");
    expect(fresh.data.stale).toBe(false);

    // Older than 30 minutes: the old digest, flagged.
    const later = NOW + 31 * MINUTE;
    const view = await deadlockBoardData(db, {
      ownerId: player.userId,
      isOwner: true,
      range: "30d",
      track: false,
      now: later,
    });
    expect(view.data.state).toBe("ok");
    expect(view.data.stale).toBe(true);
    expect(view.data.digest).toEqual(good);
    expect(view.data.fetched_at).toBe(new Date(NOW).toISOString());

    const board = await viewBoard(player.userId, viewer, {
      now: () => later,
    });
    expect(widgetTypes(board.board)).toContain("deadlock");
    expect(board.data.deadlock?.state).toBe("ok");
    expect(board.data.deadlock?.stale).toBe(true);
    expect(board.data.deadlock?.digest?.computed_at).toBe(good.computed_at);
    // Others never get real match ids (they find the match's players).
    expect(board.data.deadlock?.digest?.last_match?.id).toBe(1);
    expect(board.data.deadlock?.digest?.last10.map((m) => m.id)).toEqual(
      good.last10.map((_, index) => index + 1),
    );
    expect(JSON.stringify(board.data.deadlock)).not.toContain(
      String(good.last_match?.id),
    );
  });
});

// ── Art ────────────────────────────────────────────────────────────────────

describe("the R2 art copy", () => {
  it("stores the manifest in KV and the images in R2, skips other hosts, and serves them from the route", async () => {
    const listed = await bucket.list({
      prefix: `game-assets/${DEADLOCK_APP_ID}/`,
    });
    for (const object of listed.objects) await bucket.delete(object.key);
    await kv.delete(MANIFEST_KV_KEY);

    const mock = createMock();
    mock.state.versions = [100, 104];
    mock.state.heroes = [
      {
        id: 13,
        class_name: "hero_haze",
        name: "Haze",
        colors: { style_hex: "#ac6133" },
        images: {
          icon_image_small: `${ART}/heroes/haze_sm.png`,
          icon_hero_card: `${ART}/heroes/haze_card.png`,
        },
      },
      {
        id: 99,
        class_name: "hero_evil",
        name: "Evil",
        colors: { style_hex: "#000000" },
        images: {
          icon_image_small: "https://evil.example/evil_sm.png",
          icon_hero_card: "http://assets-bucket.deadlock-api.com/not_https.png",
        },
      },
    ];
    mock.state.items = [
      {
        id: UPGRADE,
        name: "Extra Spirit",
        type: "upgrade",
        shop_image: `${ART}/items/extra_spirit.png`,
      },
      {
        id: UPGRADE_B,
        name: "Fleetfoot",
        type: "upgrade",
        image: `${ART}/items/fleetfoot.png`,
      },
    ];
    mock.state.rankAssets = [
      {
        tier: 7,
        name: "Emissary",
        images: {
          large: `${ART}/ranks/rank07_lg.png`,
          large_subrank4:
            "https://api.deadlock-api.com/v1/assets/ranks/7/4/image",
        },
      },
    ];
    mock.state.seasons = [
      {
        class_name: "season_2",
        name: "Beta Season 2",
        calibration_matches: 8,
        intervals: [{ interval: 2, start_timestamp: 1_793_908_800 }],
      },
    ];
    const deps = {
      api: createDeadlockApi({ fetch: mock.fetch, now: () => NOW }),
      kv,
      bucket,
      fetch: mock.fetch,
      now: () => NOW,
    };

    const result = await syncDeadlockAssets(deps);

    expect(result.version).toBe(104);
    expect(result.refreshed).toBe(true);
    const stored = (await kv.get<StoredManifest>(MANIFEST_KV_KEY, "json"))!;
    expect(stored.version).toBe(104);
    expect(stored.heroes["13"]).toEqual({
      key: "haze",
      name: "Haze",
      color: "#AC6133",
    });
    expect(stored.items[String(UPGRADE)]?.name).toBe("Extra Spirit");
    expect(stored.seasons).toEqual([
      { name: "Beta Season 2", start: 1_793_908_800, calibration_matches: 8 },
    ]);
    expect(stored.sources.hero_haze_sm).toBe(`${ART}/heroes/haze_sm.png`);
    expect(stored.sources.rank_7_4).toBe(
      "https://api.deadlock-api.com/v1/assets/ranks/7/4/image",
    );
    // Art on another host (or over http) never becomes a source.
    expect(stored.sources.hero_evil_sm).toBeUndefined();
    expect(stored.sources.hero_evil_card).toBeUndefined();

    expect(result.mirrored.sort()).toEqual(
      ["hero_haze_card", "hero_haze_sm", "rank_7", "rank_7_4"].sort(),
    );
    const object = await bucket.get(assetObjectKey("hero_haze_sm"));
    expect(assetObjectKey("hero_haze_sm")).toBe(
      `game-assets/1276737795012165766/hero_haze_sm.png`,
    );
    expect(object?.httpMetadata?.contentType).toBe("image/png");
    expect(new Uint8Array(await object!.arrayBuffer())).toEqual(PNG_BYTES);
    expect(await bucket.head(assetObjectKey("hero_evil_sm"))).toBeNull();
    expect(
      mock.calls.some(
        (url) =>
          url.hostname === "evil.example" ||
          (url.protocol === "http:" && url.hostname.includes("deadlock-api")),
      ),
    ).toBe(false);

    // Items are copied on demand; the same version is not read again.
    expect(await bucket.head(assetObjectKey(`item_${UPGRADE}`))).toBeNull();
    const items = await mirrorDeadlockAssets(deps, stored, [
      `item_${UPGRADE}`,
      "item_999",
      "../escape",
    ]);
    expect(items).toEqual([`item_${UPGRADE}`]);
    expect(await bucket.head(assetObjectKey(`item_${UPGRADE}`))).not.toBeNull();
    const again = await syncDeadlockAssets(deps);
    expect(again).toEqual({ version: 104, refreshed: false, mirrored: [] });

    // The route serves what was stored and nothing else.
    const hit = await gameAssetsGet({
      request: new Request(
        `${BASE}/api/game-assets/${DEADLOCK_APP_ID}/hero_haze_sm.png`,
      ),
      params: { _splat: `${DEADLOCK_APP_ID}/hero_haze_sm.png` },
    });
    expect(hit.status).toBe(200);
    expect(hit.headers.get("content-type")).toBe("image/png");
    expect(new Uint8Array(await hit.arrayBuffer())).toEqual(PNG_BYTES);

    for (const splat of [
      `${DEADLOCK_APP_ID}/hero_unknown_sm.png`,
      `123456789/hero_haze_sm.png`,
      `${DEADLOCK_APP_ID}/../hero_haze_sm.png`,
      `${DEADLOCK_APP_ID}/hero_haze_sm.gif`,
    ]) {
      const miss = await gameAssetsGet({
        request: new Request(`${BASE}/api/game-assets/${splat}`),
        params: { _splat: splat },
      });
      expect(miss.status, splat).toBe(404);
    }
  });
});

// ── Refresh triggers ───────────────────────────────────────────────────────

describe("refresh triggers", () => {
  it("queues one fetch when a viewed plate is old and not another within five minutes", async () => {
    const mock = createMock();
    // The last fetch was 20 minutes before the first view.
    const fetchClock = { now: NOW - 20 * MINUTE };
    const owner = await addFetchedPlayer(mock, fetchClock);
    const viewer = await addUser("viewer");
    const requested: number[] = [];
    const pending: Promise<unknown>[] = [];
    const deps = (now: number): Partial<ProfileBoardDeps> => ({
      now: () => now,
      deadlockRefresh: async (accountId) => {
        requested.push(accountId);
      },
      waitUntil: (promise) => {
        pending.push(promise);
      },
    });

    const first = await viewBoard(owner.userId, viewer, deps(NOW));
    await Promise.all(pending);
    expect(first.data.deadlock?.state).toBe("ok");
    expect(requested).toEqual([owner.accountId]);
    expect((await fetchStateOf(owner.accountId))?.queued_at).toBe(NOW);

    await viewBoard(owner.userId, viewer, deps(NOW + 2 * MINUTE));
    await Promise.all(pending);
    expect(requested).toEqual([owner.accountId]);

    await viewBoard(owner.userId, viewer, deps(NOW + 6 * MINUTE));
    await Promise.all(pending);
    expect(requested).toEqual([owner.accountId, owner.accountId]);
  });

  it("does not queue a fetch for a plate that is still fresh", async () => {
    const mock = createMock();
    const owner = await addFetchedPlayer(mock, { now: NOW - 5 * MINUTE });
    const viewer = await addUser("viewer");
    const requested: number[] = [];
    await viewBoard(owner.userId, viewer, {
      deadlockRefresh: async (accountId) => {
        requested.push(accountId);
      },
    });
    expect(requested).toEqual([]);
  });

  it("starts tracking and queues a fetch when a linked owner adds the widget", async () => {
    const linked = await addPlayer({ tracked: false });
    const unlinked = await addUser("unlinked");
    const requested: number[] = [];
    const pending: Promise<unknown>[] = [];
    const deps: Partial<ProfileBoardDeps> = {
      deadlockRefresh: async (accountId) => {
        requested.push(accountId);
      },
      waitUntil: (promise) => {
        pending.push(promise);
      },
    };
    expect(await fetchStateOf(linked.accountId)).toBeNull();

    await saveBoard(linked.userId, [deadlockWidget()], 0, deps);
    await Promise.all(pending);

    expect(requested).toEqual([linked.accountId]);
    const state = await fetchStateOf(linked.accountId);
    expect(state?.user_id).toBe(linked.userId);
    expect(state?.queued_at).toBe(NOW);

    // Nothing to fetch without a linked Steam account.
    await saveBoard(unlinked, [deadlockWidget()], 0, deps);
    await Promise.all(pending);
    expect(requested).toEqual([linked.accountId]);
  });

  it("moves the cadence when Deadlock starts and ends", async () => {
    const player = await addPlayer({ tracked: false });
    await trackAccount(player.userId, player.accountId, NOW + 6 * HOUR);
    const sent: Array<{
      body: unknown;
      options: QueueSendOptions | undefined;
    }> = [];
    const liveEnv = {
      DB: db,
      BACKGROUND_TASKS: {
        async send(body: never, options?: QueueSendOptions) {
          sent.push({ body, options });
        },
      },
    };
    const playing = (): GameActivity => ({
      kind: "playing",
      name: "Deadlock",
      app_id: DEADLOCK_APP_ID,
      started_at: NOW,
      source: "detected",
    });

    // Something that is not Deadlock moves nothing.
    expect(
      await noteDeadlockActivity(
        liveEnv,
        player.userId,
        null,
        { kind: "playing", name: "Chess", started_at: NOW, source: "detected" },
        NOW,
      ),
    ).toBeNull();
    expect((await fetchStateOf(player.accountId))?.live_since).toBeNull();

    expect(
      await noteDeadlockActivity(liveEnv, player.userId, null, playing(), NOW),
    ).toBe("started");
    let state = await fetchStateOf(player.accountId);
    expect(state?.live_since).toBe(NOW);
    expect(state?.live_until).toBeNull();
    expect(state?.next_due_at).toBeLessThanOrEqual(NOW + 10 * MINUTE);
    expect(sent).toHaveLength(0);

    // The game closes 90 minutes later; the next fetch is pulled in.
    const ended = NOW + 90 * MINUTE;
    await db
      .prepare(
        `UPDATE deadlock_fetch_state SET next_due_at = ? WHERE account_id = ?`,
      )
      .bind(ended + 6 * HOUR, player.accountId)
      .run();
    expect(
      await noteDeadlockActivity(
        liveEnv,
        player.userId,
        playing(),
        null,
        ended,
      ),
    ).toBe("ended");
    state = await fetchStateOf(player.accountId);
    expect(state?.live_until).toBe(ended);
    expect(state?.next_due_at).toBeLessThanOrEqual(ended + 3 * MINUTE);
    expect(sent.map((message) => message.options?.delaySeconds)).toEqual([
      180, 900,
    ]);
    for (const message of sent) {
      expect(message.options?.contentType).toBe("json");
      expect(message.body).toEqual({
        version: 1,
        task: {
          type: "deadlock.refresh",
          accountIds: [player.accountId],
          reason: "post_game",
        },
      });
    }
  });

  it("schedules the next fetch by whether the owner plays", () => {
    const idle = { live_since: null, live_until: null };
    const recently = (NOW - HOUR) / 1000;
    // Playing now: every 10 minutes.
    expect(
      nextDueAt(
        NOW,
        { live_since: NOW - 5 * MINUTE, live_until: null },
        recently,
      ),
    ).toBe(NOW + 10 * MINUTE);
    // Closed an hour ago: still inside the two hour tail.
    expect(
      nextDueAt(
        NOW,
        { live_since: NOW - 3 * HOUR, live_until: NOW - HOUR },
        recently,
      ),
    ).toBe(NOW + 10 * MINUTE);
    // Closed three hours ago, and a session with no end past 12 hours: idle.
    expect(
      nextDueAt(
        NOW,
        { live_since: NOW - 4 * HOUR, live_until: NOW - 3 * HOUR },
        recently,
      ),
    ).toBe(NOW + 6 * HOUR);
    expect(
      nextDueAt(
        NOW,
        { live_since: NOW - 13 * HOUR, live_until: null },
        recently,
      ),
    ).toBe(NOW + 6 * HOUR);
    expect(nextDueAt(NOW, idle, recently)).toBe(NOW + 6 * HOUR);
    expect(nextDueAt(NOW, idle, null)).toBe(NOW + 6 * HOUR);
    // Nothing played in 30 days: daily.
    expect(nextDueAt(NOW, idle, (NOW - 31 * DAY) / 1000)).toBe(NOW + 24 * HOUR);
    expect(nextDueAt(NOW, idle, (NOW - 29 * DAY) / 1000)).toBe(NOW + 6 * HOUR);
  });

  it("tracks a linked owner while the widget is on the board and drops it after", async () => {
    const withWidget = await addPlayer({ tracked: false });
    const withoutWidget = await addPlayer({ tracked: false });
    const unlinked = await addUser("unlinked");
    const widgetBoard = await saveBoard(withWidget.userId, [deadlockWidget()]);
    await saveBoard(withoutWidget.userId, [
      { id: "mood-a", type: "mood", cfg: { text: "hi" } },
    ]);
    await saveBoard(unlinked, [deadlockWidget()]);

    const first = await reconcileDeadlockTracking(db, NOW);

    expect(first.added).toBeGreaterThanOrEqual(1);
    const row = await fetchStateOf(withWidget.accountId);
    expect(row?.user_id).toBe(withWidget.userId);
    expect(row?.next_due_at).toBe(NOW);
    expect(await fetchStateOf(withoutWidget.accountId)).toBeNull();

    // Running it again adds nothing new for this account.
    await db
      .prepare(
        `INSERT INTO deadlock_snapshots (account_id, user_id, status) VALUES (?, ?, 'pending')`,
      )
      .bind(withWidget.accountId, withWidget.userId)
      .run();
    await reconcileDeadlockTracking(db, NOW + MINUTE);
    expect((await fetchStateOf(withWidget.accountId))?.next_due_at).toBe(NOW);
    expect(await snapshotOf(withWidget.accountId)).not.toBeNull();

    // The widget comes off the board: the row and its snapshot go.
    await saveBoard(withWidget.userId, [], widgetBoard.version);
    const second = await reconcileDeadlockTracking(db, NOW + 2 * MINUTE);
    expect(second.dropped).toBeGreaterThanOrEqual(1);
    expect(await fetchStateOf(withWidget.accountId)).toBeNull();
    expect(await snapshotOf(withWidget.accountId)).toBeNull();
  });
});

// ── The board ──────────────────────────────────────────────────────────────

describe("what board viewers get", () => {
  it("gives other viewers the owner's range only and no Steam name", async () => {
    const mock = createMock();
    const owner = await addFetchedPlayer(mock, { now: NOW }, { range: "7d" });
    const friend = await addUser("friend");

    const theirs = await viewBoard(owner.userId, friend);
    expect(widgetTypes(theirs.board)).toContain("deadlock");
    const shown = theirs.data.deadlock;
    expect(shown?.state).toBe("ok");
    expect(shown?.persona).toBeUndefined();
    expect(shown?.digest?.ranges["7d"].matches).toBeGreaterThan(0);
    for (const range of ["30d", "season"] as const) {
      expect(shown?.digest?.ranges[range]).toEqual({
        matches: 0,
        wins: 0,
        kills: 0,
        deaths: 0,
        assists: 0,
        seconds: 0,
        rank_delta: 0,
        pool: [],
      });
    }
    expect(shown?.digest?.rank?.badge).toBe(74);
    expect(shown?.digest?.last_match).not.toBeNull();

    const mine = await viewBoard(owner.userId, owner.userId);
    expect(mine.data.deadlock?.persona).toBe("kiwi_yaps");
    expect(mine.data.deadlock?.digest?.ranges["30d"].matches).toBeGreaterThan(
      0,
    );
    expect(mine.data.deadlock?.digest?.ranges.season.matches).toBeGreaterThan(
      0,
    );
  });

  it("shows an owner with no Steam link a Link Steam state and hides the plate from everyone else", async () => {
    const owner = await addUser("owner");
    const viewer = await addUser("viewer");
    await saveBoard(owner, [deadlockWidget()]);

    const others = await viewBoard(owner, viewer);
    expect(widgetTypes(others.board)).not.toContain("deadlock");
    expect(others.data.deadlock).toBeUndefined();

    const mine = await viewBoard(owner, owner);
    expect(widgetTypes(mine.board)).toContain("deadlock");
    expect(mine.data.deadlock?.state).toBe<DeadlockState>("not_linked");
    expect(mine.data.deadlock?.digest).toBeNull();
  });

  it("keeps one Deadlock widget per board", async () => {
    const owner = await addUser("owner");
    const saved = await saveBoard(owner, [
      deadlockWidget("deadlock1", "7d"),
      deadlockWidget("deadlock2", "season"),
    ]);
    expect(
      saved.board.widgets.filter((w) => w.type === "deadlock"),
    ).toHaveLength(1);
    expect(saved.board.widgets[0]?.id).toBe("deadlock1");
    expect(saved.dropped).toEqual([
      { id: "deadlock2", type: "deadlock", reason: "type_limit" },
    ]);
  });
});

describe("live state", () => {
  const activityWith = (
    game: NonNullable<GameActivity["game"]>,
  ): GameActivity => ({
    kind: "playing",
    name: "Deadlock",
    app_id: DEADLOCK_APP_ID,
    started_at: NOW,
    source: "detected",
    game,
  });

  it("keeps the game for the linked account and maps the party to Yap users", async () => {
    const a = await addPlayer({ tracked: false });
    const b = await addPlayer({ tracked: false });
    const stranger = nextAccountId();

    const resolved = await resolveLiveState(
      db,
      a.userId,
      activityWith({
        kind: "deadlock",
        phase: "in_match",
        hero: "haze",
        mode: "ranked",
        phase_started_at: 1234,
        account_id: a.accountId,
        // Itself, a linked friend (twice), and an account nobody linked.
        party: [a.accountId, b.accountId, stranger, b.accountId],
      }),
    );

    expect(resolved?.game).toEqual({
      kind: "deadlock",
      phase: "in_match",
      hero: "haze",
      mode: "ranked",
      phase_started_at: 1234,
      party_user_ids: [b.userId],
    });
    expect(JSON.stringify(resolved)).not.toContain("account_id");
    expect(resolved?.name).toBe("Deadlock");

    // Alone: no party at all.
    const alone = await resolveLiveState(
      db,
      a.userId,
      activityWith({
        kind: "deadlock",
        phase: "menu",
        account_id: a.accountId,
        party: [stranger],
      }),
    );
    expect(alone?.game).toEqual({ kind: "deadlock", phase: "menu" });
  });

  it("drops the live state when the account is not the user's own", async () => {
    const a = await addPlayer({ tracked: false });
    const b = await addPlayer({ tracked: false });
    const noSteam = await addUser("nosteam");

    const mismatch = await resolveLiveState(
      db,
      a.userId,
      activityWith({
        kind: "deadlock",
        phase: "in_match",
        account_id: b.accountId,
        party: [a.accountId],
      }),
    );
    expect(mismatch).not.toBeNull();
    expect(mismatch).not.toHaveProperty("game");
    expect(mismatch?.name).toBe("Deadlock");

    const unlinked = await resolveLiveState(
      db,
      noSteam,
      activityWith({
        kind: "deadlock",
        phase: "queue",
        account_id: a.accountId,
      }),
    );
    expect(unlinked).not.toHaveProperty("game");

    // Without an account id there is nothing to check: stripped as well.
    const anonymous = await resolveLiveState(
      db,
      a.userId,
      activityWith({ kind: "deadlock", phase: "queue" }),
    );
    expect(anonymous).not.toHaveProperty("game");
  });
});

describe("unlinking Steam", () => {
  it("deletes the player's Deadlock rows and no one else's", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const leaving = await addFetchedPlayer(mock, clock);
    const staying = await addFetchedPlayer(mock, clock);
    expect(await fetchStateOf(leaving.accountId)).not.toBeNull();
    expect(await snapshotOf(leaving.accountId)).not.toBeNull();

    expect(await unlinkConnection(db, leaving.userId, "steam")).toBe(true);

    expect(await fetchStateOf(leaving.accountId)).toBeNull();
    expect(await snapshotOf(leaving.accountId)).toBeNull();
    expect(
      await db
        .prepare(`SELECT 1 AS hit FROM user_connections WHERE user_id = ?`)
        .bind(leaving.userId)
        .first(),
    ).toBeNull();
    expect(await fetchStateOf(staying.accountId)).not.toBeNull();
    expect(await snapshotOf(staying.accountId)).not.toBeNull();
    // Nothing left to unlink.
    expect(await unlinkConnection(db, leaving.userId, "steam")).toBe(false);
  });
});

// ── Review regressions ─────────────────────────────────────────────────────

describe("review regressions", () => {
  it("keeps the good digest when the history answers 429 with nothing, or junk", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addFetchedPlayer(mock, clock);
    const good = digestOf(await snapshotOf(player.accountId));

    for (const answer of [
      () =>
        new Response("[]", {
          status: 429,
          headers: { "retry-after": "120" },
        }),
      () => Response.json({ error: "not a list" }),
    ]) {
      clock.now += 30 * MINUTE;
      await db
        .prepare(
          `UPDATE deadlock_fetch_state SET retry_at = NULL, next_due_at = ? WHERE account_id = ?`,
        )
        .bind(clock.now, player.accountId)
        .run();
      mock.state.override = (url) =>
        /match-history$/.test(url.pathname) ? answer() : null;
      const { deps } = createFetcher(mock, clock);
      const outcome = onlyOutcome(
        await refreshDeadlockAccounts(deps, {
          accountIds: [player.accountId],
          reason: "manual",
        }),
      );
      expect(outcome.result).toBe("failed");
      const snapshot = await snapshotOf(player.accountId);
      expect(snapshot?.status).toBe("ok");
      expect(digestOf(snapshot)).toEqual(good);
    }
  });

  it("reads a failed match's metadata again on the next fetch", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
    mock.state.override = (url) =>
      /metadata$/.test(url.pathname)
        ? new Response("busy", { status: 500 })
        : null;
    const { deps } = createFetcher(mock, clock);
    await refreshDeadlockAccounts(deps, { accountIds: [player.accountId] });
    expect(
      digestOf(await snapshotOf(player.accountId)).last_match?.level,
    ).toBeNull();

    mock.state.override = null;
    clock.now += 20 * MINUTE;
    await refreshDeadlockAccounts(deps, {
      accountIds: [player.accountId],
      reason: "manual",
    });
    expect(mock.count(/^\/v1\/matches\/\d+\/metadata$/)).toBe(2);
    expect(digestOf(await snapshotOf(player.accountId)).last_match?.level).toBe(
      28,
    );
  });

  it("skips a run the budget denies without charging anyone a failure", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
    const deps: FetcherDeps = {
      ...createFetcher(mock, clock).deps,
      api: createDeadlockApi({
        fetch: mock.fetch,
        now: () => clock.now,
        budget: async () => false,
      }),
    };
    const outcome = onlyOutcome(await refreshDeadlockAccounts(deps));
    expect(outcome).toMatchObject({ result: "skipped", error: "budget" });
    expect(mock.calls).toHaveLength(0);
    const state = await fetchStateOf(player.accountId);
    expect(state?.fail_count).toBe(0);
    expect(state?.retry_at).toBeNull();
  });

  it("pauses the whole API after a 429, for every later run and account", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const first = await addPlayer();
    mock.setPlayer(first.accountId, { newestMatchId: nextMatchId() });
    let pausedUntil: number | null = null;
    const pause = {
      until: async () => pausedUntil,
      hold: async (until: number) => {
        pausedUntil = until;
      },
    };
    const apiWith = () =>
      createDeadlockApi({ fetch: mock.fetch, now: () => clock.now, pause });
    mock.state.override = (url) =>
      url.pathname === "/v1/players/rank"
        ? new Response("slow down", {
            status: 429,
            headers: { "retry-after": "600" },
          })
        : null;
    const base = createFetcher(mock, clock).deps;
    await refreshDeadlockAccounts({ ...base, api: apiWith() });
    expect(pausedUntil).toBe(NOW + 600_000);
    expect((await fetchStateOf(first.accountId))?.fail_count).toBe(1);

    // Another account, another run, inside the pause: nothing is sent.
    mock.state.override = null;
    const second = await addPlayer();
    mock.setPlayer(second.accountId, { newestMatchId: nextMatchId() });
    const before = mock.calls.length;
    clock.now += 5 * MINUTE;
    const outcome = onlyOutcome(
      await refreshDeadlockAccounts({ ...base, api: apiWith() }),
    );
    expect(outcome).toMatchObject({
      account_id: second.accountId,
      result: "skipped",
      error: "paused",
    });
    expect(mock.calls.length).toBe(before);
    expect((await fetchStateOf(second.accountId))?.fail_count).toBe(0);
  });

  it("fails one account on an exception and still fetches the rest", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const bad = await addPlayer();
    const good = await addPlayer();
    mock.setPlayer(bad.accountId, { newestMatchId: nextMatchId() });
    mock.setPlayer(good.accountId, { newestMatchId: nextMatchId() });
    // Null rows in a history are skipped, not thrown on.
    mock.state.history.set(good.accountId, [
      null as unknown as Record<string, unknown>,
      ...(mock.state.history.get(good.accountId) ?? []),
    ]);
    const throwing = new Proxy(db, {
      get(target, key, receiver) {
        if (key !== "prepare") return Reflect.get(target, key, receiver);
        return (sql: string) => {
          const statement = target.prepare(sql);
          if (!sql.includes("INSERT INTO deadlock_snapshots")) return statement;
          return {
            bind: (...args: unknown[]) => {
              if (args[0] === bad.accountId) throw new Error("D1 hiccup");
              return statement.bind(...args);
            },
          };
        };
      },
    });
    const { deps } = createFetcher(mock, clock);
    const outcomes = await refreshDeadlockAccounts({ ...deps, db: throwing });
    const byId = new Map(outcomes.map((o) => [o.account_id, o]));
    expect(byId.get(bad.accountId)).toMatchObject({
      result: "failed",
      error: "exception",
    });
    expect(byId.get(good.accountId)?.result).toBe("updated");
    expect((await fetchStateOf(bad.accountId))?.fail_count).toBe(1);
    expect((await snapshotOf(good.accountId))?.status).toBe("ok");
  });

  it("queues post-game checks once per ten minutes, reconnects included", async () => {
    const player = await addPlayer({ tracked: false });
    await trackAccount(player.userId, player.accountId, NOW + 6 * HOUR);
    const sent: unknown[] = [];
    const liveEnv = {
      DB: db,
      BACKGROUND_TASKS: {
        async send(body: never) {
          sent.push(body);
        },
      },
    };
    const deadlock = (phase?: "post_match" | "in_match"): GameActivity => ({
      kind: "playing",
      name: "Deadlock",
      app_id: DEADLOCK_APP_ID,
      started_at: NOW,
      source: "detected",
      ...(phase ? { game: { kind: "deadlock" as const, phase } } : {}),
    });
    await noteDeadlockActivity(liveEnv, player.userId, deadlock(), null, NOW);
    expect(sent).toHaveLength(2);
    // A reconnect: the game "starts" and "ends" again a minute later.
    await noteDeadlockActivity(
      liveEnv,
      player.userId,
      null,
      deadlock(),
      NOW + MINUTE,
    );
    await noteDeadlockActivity(
      liveEnv,
      player.userId,
      deadlock(),
      null,
      NOW + 2 * MINUTE,
    );
    // A flapping phase.
    for (let i = 0; i < 5; i += 1) {
      await noteDeadlockActivity(
        liveEnv,
        player.userId,
        deadlock("in_match"),
        deadlock("post_match"),
        NOW + (3 + i) * MINUTE,
      );
    }
    expect(sent).toHaveLength(2);
    await noteDeadlockActivity(
      liveEnv,
      player.userId,
      deadlock("in_match"),
      deadlock("post_match"),
      NOW + 11 * MINUTE,
    );
    expect(sent).toHaveLength(4);
  });

  it("does not bring a snapshot back when Steam was unlinked mid-fetch", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addPlayer();
    mock.setPlayer(player.accountId, { newestMatchId: nextMatchId() });
    const { deps } = createFetcher(mock, clock);
    const racing = (async (input: RequestInfo | URL, init?: RequestInit) => {
      const href =
        typeof input === "string"
          ? input
          : input instanceof URL
            ? input.href
            : input.url;
      if (/match-history$/.test(new URL(href).pathname)) {
        await unlinkConnection(db, player.userId, "steam");
      }
      return mock.fetch(input, init);
    }) as typeof fetch;
    await refreshDeadlockAccounts({
      ...deps,
      api: createDeadlockApi({ fetch: racing, now: () => clock.now }),
    });
    expect(await snapshotOf(player.accountId)).toBeNull();
    expect(await fetchStateOf(player.accountId)).toBeNull();
  });

  it("drops the old account's stats when another Steam account is linked", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addFetchedPlayer(mock, clock);
    expect(await snapshotOf(player.accountId)).not.toBeNull();
    await linkSteam(player.userId, nextAccountId(), "kiwi_alt");
    expect(await snapshotOf(player.accountId)).toBeNull();
    expect(await fetchStateOf(player.accountId)).toBeNull();
  });

  it("names only party members who show Steam on their profile", async () => {
    const a = await addPlayer({ tracked: false });
    const hidden = await addPlayer({ tracked: false });
    await db
      .prepare(
        `UPDATE user_connections SET show_on_profile = 0 WHERE user_id = ?`,
      )
      .bind(hidden.userId)
      .run();
    const resolved = await resolveLiveState(db, a.userId, {
      kind: "playing",
      name: "Deadlock",
      app_id: DEADLOCK_APP_ID,
      started_at: NOW,
      source: "detected",
      game: {
        kind: "deadlock",
        phase: "in_match",
        account_id: a.accountId,
        party: [hidden.accountId],
      },
    });
    expect(resolved?.game?.party_user_ids).toBeUndefined();
  });

  it("re-copies art copied for an older version and refuses one over 2 MB", async () => {
    const mock = createMock();
    const key = "hero_haze_sm";
    const manifest: StoredManifest = {
      ...manifestFixture(),
      version: 7,
      sources: {
        [key]: `${ART}/heroes/haze_sm.png`,
        hero_big_sm: `${ART}/big.png`,
      },
    };
    await bucket.delete(assetObjectKey("hero_big_sm"));
    await bucket.put(assetObjectKey(key), new Uint8Array([1]), {
      customMetadata: { v: "6" },
    });
    mock.state.override = (url) =>
      url.pathname.endsWith("/big.png")
        ? new Response(new Uint8Array(8), {
            status: 200,
            headers: {
              "content-type": "image/png",
              "content-length": String(3 * 1024 * 1024),
            },
          })
        : null;
    const stored = await mirrorDeadlockAssets(
      { bucket, fetch: mock.fetch },
      manifest,
      [key, "hero_big_sm"],
    );
    expect(stored).toEqual([key]);
    const object = await bucket.head(assetObjectKey(key));
    expect(object?.customMetadata?.v).toBe("7");
    expect(object?.size).toBe(PNG_BYTES.byteLength);
    expect(await bucket.head(assetObjectKey("hero_big_sm"))).toBeNull();
    // Current now: not fetched again.
    const calls = mock.calls.length;
    await mirrorDeadlockAssets({ bucket, fetch: mock.fetch }, manifest, [key]);
    expect(mock.calls.length).toBe(calls);
  });

  it("omits the plate, not the board, when the Deadlock read fails", async () => {
    const mock = createMock();
    const clock = { now: NOW };
    const player = await addFetchedPlayer(mock, clock);
    const viewer = await addUser("viewer");
    await db
      .prepare(
        `ALTER TABLE deadlock_snapshots RENAME TO deadlock_snapshots_off`,
      )
      .run();
    try {
      const board = await viewBoard(player.userId, viewer);
      expect(widgetTypes(board.board)).not.toContain("deadlock");
      expect(board.data.deadlock).toBeUndefined();
    } finally {
      await db
        .prepare(
          `ALTER TABLE deadlock_snapshots_off RENAME TO deadlock_snapshots`,
        )
        .run();
    }
  });
});
