/**
 * Fixed-clock boards for /ui-fixtures and tests (no network, no images:
 * asset names and icon paths are plain strings that the dev fixtures map
 * to bundled pictures through `BoardMediaProvider`).
 *
 *   FIXTURE_BOARD        Kiwi's full board seen by someone else: all twelve
 *                        presets and a Custom (13 plates; one over the real
 *                        12-widget cap, on purpose, to show every type)
 *   FIXTURE_BOARD_OWN    the same board as its owner on a free plan: the
 *                        Custom is a draft ("Draft · only you")
 *   FIXTURE_BOARD_EMPTY  an owner with nothing on the board yet
 *   FIXTURE_BOARD_STATES every widget with nothing to show, as a stranger
 *                        sees it (cleared mood, past countdown, pinned
 *                        message from a server they're not in, closed
 *                        guestbook)
 */

import type { GameActivity } from "@/lib/gateway-protocol";
import type {
  AnyProfileWidget,
  BoardData,
  BoardResponse,
  GuestbookNote,
} from "@/lib/profile-board";

/** The fixtures' clock: Thursday 27 November 2026, 20:30 UTC. */
export const FIXTURE_NOW = Date.parse("2026-11-27T20:30:00Z");

export const FIXTURE_OWNER = {
  id: "u-kiwi",
  username: "kiwi",
  name: "Kiwi",
} as const;

/** The signed-in user of the fixtures (the store's `users.me`). */
export const FIXTURE_VIEWER = {
  id: "u-me",
  username: "jon",
  display_name: "Jon",
  avatar_url: null,
} as const;

const iso = (minutesAgo: number) =>
  new Date(FIXTURE_NOW - minutesAgo * 60_000).toISOString();

/** Icon paths the dev fixtures resolve to bundled covers. */
export const FIXTURE_ICONS = {
  deadlock: "fixture/game/deadlock",
  balatro: "fixture/game/balatro",
  rivals: "fixture/game/rivals",
  stardew: "fixture/game/stardew",
  helldivers: "fixture/game/helldivers",
} as const;

/** Widget asset names the dev fixtures resolve to bundled pictures. */
export const FIXTURE_ASSETS = {
  beach: "kiwiphoto0001beach.webp",
  cat: "kiwiphoto0002mochi.gif",
  desk: "kiwiphoto0003desk.webp",
  commission: "kiwicustom0001chibi.webp",
} as const;

/** What Kiwi is playing right now (seed `gameActivityByUserId`). */
export const FIXTURE_LIVE_GAME: GameActivity = {
  kind: "playing",
  name: "Deadlock",
  icon_url: FIXTURE_ICONS.deadlock,
  details: "Ranked · as Haze",
  state: "Archon II",
  started_at: FIXTURE_NOW - (1 * 3600 + 42 * 60 + 8) * 1000,
  source: "detected",
};

const VOICE_DAYS_H = [
  0.4, 2.1, 0, 1.2, 3.4, 0.8, 0, 0, 2.6, 1.9, 0.3, 4.1, 1.1, 0, 2.2, 3.0, 0.6,
  1.5, 0, 0.9, 2.8, 1.3, 0.2, 0, 1.7, 2.4, 1.0,
];

const NOTES: GuestbookNote[] = [
  {
    id: "note-juno",
    author: {
      id: "u-juno",
      username: "juno",
      display_name: "Juno",
      avatar_url: null,
    },
    body: "best duo partner, terrible taste in music",
    created_at: iso(2 * 24 * 60),
  },
  {
    id: "note-mo",
    author: {
      id: "u-mo",
      username: "mo",
      display_name: "Mo",
      avatar_url: null,
    },
    body: "carried me through plat. I owe you one",
    created_at: iso(5 * 24 * 60),
  },
  {
    id: "note-pip",
    author: {
      id: "u-pip",
      username: "pip",
      display_name: "Pip",
      avatar_url: null,
    },
    body: "the cat avatar >>>",
    created_at: iso(8 * 24 * 60),
  },
];

const WIDGETS: AnyProfileWidget[] = [
  {
    id: "w-nowplay",
    type: "now_playing",
    size: "L",
    cfg: { when_idle: "last_played" },
  },
  {
    id: "w-mood",
    type: "mood",
    size: "S",
    cfg: {
      pose: "bark",
      text: "ranked grind. send snacks",
      clears_at: "2026-11-28T00:00:00Z",
    },
  },
  {
    id: "w-countdown",
    type: "countdown",
    size: "S",
    cfg: { title: "Raid night: Vault of Glass", at: "2026-11-29T21:00:00Z" },
  },
  { id: "w-voice", type: "voice_hours", size: "M", cfg: { period: "month" } },
  { id: "w-topgames", type: "top_games", size: "M", cfg: { range: "30d" } },
  { id: "w-badges", type: "badges", size: "S", cfg: { badge_ids: [] } },
  { id: "w-listen", type: "listening", size: "S", cfg: {} },
  {
    id: "w-quote",
    type: "quote",
    size: "L",
    cfg: {
      message_id: "m-uninstall",
      snapshot: {
        text: "if we lose this one I'm uninstalling. (we lost. I did not uninstall.) @Mo you owe me a rematch",
        at: "2026-10-12T01:14:00Z",
        channel_id: "c-general",
        channel_name: "general",
        server_id: "s-raidhq",
        server_name: "Raid HQ",
      },
    },
  },
  {
    id: "w-photos",
    type: "photos",
    size: "M",
    cfg: {
      photos: [
        { asset: FIXTURE_ASSETS.beach, alt: "Sunset over the beach in Lisbon" },
        { asset: FIXTURE_ASSETS.cat, alt: "Mochi the cat supervising" },
      ],
      caption: "Lisbon trip, and Mochi supervising the build",
    },
  },
  { id: "w-guestbook", type: "guestbook", size: "M", cfg: {} },
  {
    id: "w-links",
    type: "links",
    size: "S",
    cfg: {
      links: [
        { label: "Art", url: "https://kiwi.page/art" },
        { label: "Twitch", url: "https://www.twitch.tv/kiwiyaps" },
        { label: "Ko-fi", url: "https://ko-fi.com/kiwiyaps" },
      ],
    },
  },
  {
    id: "w-servers",
    type: "servers",
    size: "S",
    cfg: { server_ids: ["s-raidhq", "s-cozy", "s-ranked", "s-books"] },
  },
  {
    id: "w-custom",
    type: "custom",
    size: "L",
    cfg: {
      legend: "Commissions",
      title: "Commissions open",
      asset: FIXTURE_ASSETS.commission,
      lines: [
        "Chibi icons, from 15 €",
        "Emote packs, 5 for 30 €",
        "DM me here on Yap",
        "Queue: 3 of 5 slots open",
      ],
    },
  },
];

const DATA: BoardData = {
  last_game: {
    name: "Balatro",
    icon_url: FIXTURE_ICONS.balatro,
    ended_at: iso(26 * 60),
    seconds: 2 * 3600 + 14 * 60,
  },
  game_totals: {
    deadlock: { week: 11 * 3600 + 20 * 60, all: 212 * 3600 },
    balatro: { week: 3 * 3600 + 5 * 60, all: 64 * 3600 },
  },
  top_games: {
    range: "30d",
    games: [
      {
        name: "Deadlock",
        icon_url: FIXTURE_ICONS.deadlock,
        seconds: 41.5 * 3600,
      },
      {
        name: "Balatro",
        icon_url: FIXTURE_ICONS.balatro,
        seconds: 18.2 * 3600,
      },
      {
        name: "Marvel Rivals",
        icon_url: FIXTURE_ICONS.rivals,
        seconds: 12 * 3600,
      },
      {
        name: "Stardew Valley",
        icon_url: FIXTURE_ICONS.stardew,
        seconds: 6.4 * 3600,
      },
      {
        name: "Helldivers 2",
        icon_url: FIXTURE_ICONS.helldivers,
        seconds: 3.1 * 3600,
      },
    ],
    total_seconds: 81 * 3600,
    game_count: 9,
  },
  voice_hours: {
    period: "month",
    start_day: "2026-11-01",
    days: VOICE_DAYS_H.map((hours) => Math.round(hours * 3600)),
    total_seconds: Math.round(
      VOICE_DAYS_H.reduce((sum, h) => sum + h, 0) * 3600,
    ),
    previous_seconds: Math.round(32.3 * 3600),
    stream_seconds: 6 * 3600 + 12 * 60,
    longest_seconds: 5 * 3600 + 41 * 60,
  },
  last_listen: {
    tracks: 12,
    top_track: "Night Drive (Slowed)",
    ended_at: iso(27 * 60),
  },
  badges: [
    { id: "night-owl", awarded_at: "2026-11-03T03:12:00Z" },
    { id: "voice-regular", awarded_at: "2026-10-20T22:00:00Z" },
    { id: "server-owner", awarded_at: "2026-08-14T18:00:00Z" },
    { id: "early-yapper", awarded_at: "2025-06-26T12:00:00Z" },
    { id: "founder", awarded_at: "2025-06-26T12:00:00Z" },
  ],
  servers: [
    {
      id: "s-raidhq",
      name: "Raid HQ",
      icon_url: null,
      member_count: 14,
      viewer_is_member: true,
    },
    {
      id: "s-cozy",
      name: "Cozy Corner",
      icon_url: null,
      member_count: 9,
      viewer_is_member: true,
    },
    {
      id: "s-ranked",
      name: "Ranked Pain",
      icon_url: null,
      member_count: 6,
      viewer_is_member: false,
    },
    {
      id: "s-books",
      name: "Book Club",
      icon_url: null,
      member_count: 5,
      viewer_is_member: false,
    },
  ],
  quote_visible: true,
  guestbook: { enabled: true, notes: NOTES, total: 12, can_post: true },
};

/** Kiwi's board as a friend (the fixtures' viewer) sees it. */
export const FIXTURE_BOARD: BoardResponse = {
  user_id: FIXTURE_OWNER.id,
  board: { v: 1, widgets: WIDGETS },
  version: 7,
  drafts: [],
  is_owner: false,
  data: DATA,
  updated_at: iso(3 * 60),
};

/** The same board as its owner on a free plan: the Custom is a draft. */
export const FIXTURE_BOARD_OWN: BoardResponse = {
  ...FIXTURE_BOARD,
  user_id: FIXTURE_VIEWER.id,
  drafts: ["w-custom"],
  is_owner: true,
  data: {
    ...DATA,
    guestbook: { ...DATA.guestbook!, can_post: false },
  },
};

/** An owner with nothing on the board yet. */
export const FIXTURE_BOARD_EMPTY: BoardResponse = {
  user_id: FIXTURE_VIEWER.id,
  board: { v: 1, widgets: [] },
  version: 0,
  drafts: [],
  is_owner: true,
  data: {},
  updated_at: null,
};

/** Every widget with nothing to show, as a stranger sees it. */
export const FIXTURE_BOARD_STATES: BoardResponse = {
  user_id: FIXTURE_OWNER.id,
  board: {
    v: 1,
    widgets: [
      {
        id: "e-nowplay",
        type: "now_playing",
        size: "M",
        cfg: { when_idle: "hide" },
      },
      { id: "e-topgames", type: "top_games", size: "M", cfg: { range: "7d" } },
      {
        id: "e-mood",
        type: "mood",
        size: "S",
        cfg: { pose: "sad", text: "", clears_at: null },
      },
      {
        id: "e-countdown",
        type: "countdown",
        size: "S",
        cfg: { title: "Season 4 launch", at: "2026-11-20T18:00:00Z" },
      },
      {
        id: "e-quote",
        type: "quote",
        size: "L",
        cfg: {
          message_id: "m-secret",
          snapshot: {
            text: "secret plans",
            at: "2026-11-01T12:00:00Z",
            channel_id: "c-x",
            channel_name: "plans",
            server_id: "s-secret",
            server_name: "Secret Base",
          },
        },
      },
      { id: "e-voice", type: "voice_hours", size: "S", cfg: { period: "30d" } },
      { id: "e-listen", type: "listening", size: "S", cfg: {} },
      { id: "e-guestbook", type: "guestbook", size: "M", cfg: {} },
      { id: "e-badges", type: "badges", size: "S", cfg: { badge_ids: [] } },
      { id: "e-links", type: "links", size: "S", cfg: { links: [] } },
    ],
  },
  version: 2,
  drafts: [],
  is_owner: false,
  data: {
    last_game: null,
    game_totals: {},
    top_games: { range: "7d", games: [], total_seconds: 0, game_count: 0 },
    voice_hours: {
      period: "30d",
      start_day: "2026-10-29",
      days: Array.from({ length: 30 }, () => 0),
      total_seconds: 0,
      previous_seconds: 0,
      stream_seconds: 0,
      longest_seconds: 0,
    },
    last_listen: null,
    badges: [],
    quote_visible: false,
    guestbook: { enabled: false, notes: [], total: 0, can_post: false },
  },
  updated_at: iso(60),
};
