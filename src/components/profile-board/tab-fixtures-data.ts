/**
 * Fixture data for the Activity and Wishlist tabs (dev `/ui-fixtures`
 * surfaces `pb-activity*` and `pb-wishlist*`, and the DOM tests).
 *
 * Fixed clock: every event sits at a fixed local time on a fixed number of
 * days before `now`'s day, so a capture always shows the same groups
 * (Today, Yesterday, Earlier this week, then dated days) and the same times.
 * Shop art is the dev covers under `components/dev/assets`, not third-party
 * catalog art.
 */

import coverCopperline from "@/components/dev/assets/cover-copperline.webp";
import coverGlasshouse from "@/components/dev/assets/cover-glasshouse.webp";
import coverNightdrive from "@/components/dev/assets/cover-nightdrive.webp";
import coverPaperboats from "@/components/dev/assets/cover-paperboats.webp";
import coverSlowfires from "@/components/dev/assets/cover-slowfires.webp";
import coverSmallhours from "@/components/dev/assets/cover-smallhours.webp";
import coverTidelines from "@/components/dev/assets/cover-tidelines.webp";
import type {
  CollectibleCatalogItem,
  CollectiblesCatalog,
} from "@/lib/collectibles-catalog";
import type {
  ActivityEvent,
  ActivityEventData,
  ActivityKind,
  ActivityPage,
  WishlistItem,
} from "@/lib/profile-board";

export const FIXTURE_OWNER = { id: "u-kiwi", name: "Kiwi" } as const;

// ── Activity ─────────────────────────────────────────────────────────────

/** Local time `daysAgo` days before `now`'s day, at hh:mm. */
function at(now: number, daysAgo: number, hh: number, mm: number): number {
  const date = new Date(now);
  date.setHours(0, 0, 0, 0);
  date.setDate(date.getDate() - daysAgo);
  date.setHours(hh, mm, 0, 0);
  return date.getTime();
}

interface Spec {
  id: string;
  kind: ActivityKind;
  day: number;
  time: [number, number];
  minutes?: number;
  ref?: string;
  data?: ActivityEventData;
  hidden?: boolean;
}

function event(now: number, spec: Spec): ActivityEvent {
  const start = at(now, spec.day, spec.time[0], spec.time[1]);
  const seconds = spec.minutes ? spec.minutes * 60 : null;
  return {
    id: spec.id,
    kind: spec.kind,
    ref: spec.ref ?? null,
    data: spec.data ?? {},
    started_at: new Date(start).toISOString(),
    ended_at: seconds ? new Date(start + seconds * 1000).toISOString() : null,
    seconds,
    hidden: spec.hidden ?? false,
  };
}

const RAID_HQ = { server_id: "s-raid", server_name: "Raid HQ" };

/** The timeline both views share; `own` adds what only the owner sees. */
function activitySpecs(own: boolean): Spec[] {
  return [
    {
      id: "a1",
      kind: "game",
      day: 0,
      time: [9, 12],
      minutes: 102,
      data: { name: "Deadlock", icon_url: coverSlowfires },
    },
    {
      id: "a2",
      kind: "voice",
      day: 0,
      time: [3, 18],
      minutes: 220,
      data: { channel_id: "c-lounge", channel_name: "Lounge", ...RAID_HQ },
    },
    { id: "a3", kind: "badge", day: 0, time: [3, 2], ref: "night-owl" },
    {
      id: "a4",
      kind: "stream",
      day: 1,
      time: [21, 10],
      minutes: 48,
      data: {
        name: "Balatro",
        channel_id: "c-stage",
        channel_name: "Stage",
        ...RAID_HQ,
      },
    },
    {
      id: "a5",
      kind: "listen",
      day: 1,
      time: [19, 30],
      minutes: 64,
      data: {
        tracks: 12,
        top_track: "Night Drive (Slowed)",
        channel_id: "c-lounge",
        channel_name: "Lounge",
        ...RAID_HQ,
      },
    },
    // The owner sees a DM call; everyone else gets no names for it.
    {
      id: "a6",
      kind: "voice",
      day: 2,
      time: [22, 40],
      minutes: 75,
      data: own ? { channel_id: "c-dm-mo" } : {},
    },
    {
      id: "a7",
      kind: "server_join",
      day: 3,
      time: [18, 5],
      ref: "s-cozy",
      data: { server_id: "s-cozy", server_name: "Cozy Corner" },
    },
    {
      id: "a8",
      kind: "game",
      day: 4,
      time: [20, 15],
      minutes: 125,
      data: { name: "Stardew Valley", icon_url: coverPaperboats },
      hidden: own,
    },
    { id: "a9", kind: "badge", day: 5, time: [12, 0], ref: "voice-regular" },
    {
      id: "a10",
      kind: "game",
      day: 9,
      time: [23, 1],
      minutes: 88,
      data: { name: "Hades II" },
    },
    {
      id: "a11",
      kind: "voice",
      day: 9,
      time: [20, 30],
      minutes: 155,
      data: { channel_id: "c-raid", channel_name: "Raid night", ...RAID_HQ },
    },
  ];
}

/** Someone else's timeline, as a viewer gets it (no settings). */
export function activityFixture(now: number = Date.now()): ActivityPage {
  return {
    events: activitySpecs(false).map((spec) => event(now, spec)),
    next_cursor: null,
    is_owner: false,
  };
}

/** The owner's own: everything, one hidden event, and their settings. */
export function ownActivityFixture(now: number = Date.now()): ActivityPage {
  return {
    events: activitySpecs(true).map((spec) => event(now, spec)),
    next_cursor: null,
    is_owner: true,
    settings: { retention_days: 30, hidden_kinds: ["server_join", "listen"] },
  };
}

export function emptyActivityFixture(owner: boolean): ActivityPage {
  const page: ActivityPage = { events: [], next_cursor: null, is_owner: owner };
  if (owner) page.settings = { retention_days: 30, hidden_kinds: [] };
  return page;
}

// ── Wishlist ─────────────────────────────────────────────────────────────

function item(
  partial: Pick<CollectibleCatalogItem, "skuId" | "name" | "kind"> &
    Partial<CollectibleCatalogItem>,
): CollectibleCatalogItem {
  return {
    id: `${partial.kind}:${partial.skuId}`,
    summary: "",
    source: "yapper",
    categoryId: "cat-covers",
    categoryName: "Cover Stories",
    productType: 0,
    itemType: 0,
    ...partial,
  };
}

/** A small catalog: nameplates and effects drawn from the dev covers. */
export const FIXTURE_CATALOG_ITEMS: CollectibleCatalogItem[] = [
  item({
    skuId: "np-nightdrive",
    name: "Night Drive",
    kind: "nameplate",
    staticUrl: coverNightdrive,
  }),
  item({
    skuId: "fx-glasshouse",
    name: "Glasshouse",
    kind: "profile_effect",
    profileEffect: { thumbnailPreviewSrc: coverGlasshouse, effects: [] },
  }),
  item({
    skuId: "np-tidelines",
    name: "Tidelines",
    kind: "nameplate",
    staticUrl: coverTidelines,
    categoryId: "cat-shore",
    categoryName: "Shoreline",
  }),
  item({
    skuId: "fx-copperline",
    name: "Copperline",
    kind: "profile_effect",
    profileEffect: { thumbnailPreviewSrc: coverCopperline, effects: [] },
  }),
  item({
    skuId: "np-smallhours",
    name: "Small Hours",
    kind: "nameplate",
    staticUrl: coverSmallhours,
  }),
  item({
    skuId: "fx-paperboats",
    name: "Paper Boats",
    kind: "profile_effect",
    profileEffect: { thumbnailPreviewSrc: coverPaperboats, effects: [] },
    categoryId: "cat-shore",
    categoryName: "Shoreline",
  }),
];

export function fixtureCatalog(): CollectiblesCatalog {
  return {
    version: 1,
    source: "yapper",
    syncedAt: "2026-11-09T12:00:00.000Z",
    stale: false,
    categories: [
      { id: "cat-covers", name: "Cover Stories", summary: "", itemIds: [] },
      { id: "cat-shore", name: "Shoreline", summary: "", itemIds: [] },
    ],
    items: FIXTURE_CATALOG_ITEMS,
    counts: {
      avatar_decoration: 0,
      profile_effect: 3,
      nameplate: 3,
      profile_frame: 0,
    },
  };
}

/** Six entries: four shop items (the shop has dropped one) and two hints. */
export const WISHLIST_FIXTURE: WishlistItem[] = [
  { id: "w1", kind: "shop", sku: "np-nightdrive", name: "Night Drive" },
  { id: "w2", kind: "shop", sku: "fx-glasshouse", name: "Glasshouse" },
  {
    id: "w3",
    kind: "hint",
    text: "Anything with cats. Or a cat. Mostly cats.",
  },
  { id: "w4", kind: "shop", sku: "np-tidelines", name: "Tidelines" },
  { id: "w5", kind: "shop", sku: "retired-ember", name: "Ember Ring" },
  { id: "w6", kind: "hint", text: "A Balatro deck with the gold stake art" },
];
