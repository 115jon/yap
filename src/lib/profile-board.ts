/**
 * Profile boards: the widgets on a profile's right column, the Activity
 * timeline's kinds and settings, and the Wishlist. Pure (no server-only
 * imports, no DOM), like `entitlements.ts`, so the server routes, the web
 * client and the native port share one model.
 *
 * The board is one JSON document per user (`profile_boards.board`), always
 * read and written whole, with an integer `version` for optimistic saves.
 * The server is authoritative: every write goes through `parseBoard`, which
 * normalises text, URLs and sizes and drops what it cannot keep, so a stale
 * or hostile client never stores something a viewer could not render.
 *
 * Everyone who can open a profile sees its board: there is no audience
 * setting. The one exception is the paid Custom widget: without the
 * `customWidgets` perk it is kept in storage but stripped from every viewer
 * read (`boardForViewer`), and the owner sees it as a draft.
 */

import type { DeadlockBoardData, DeadlockRange } from "@/lib/deadlock";
import { hasPerk, type Entitlement, type Perk } from "@/lib/entitlements";

// ── Widgets ────────────────────────────────────────────────────────────────

export const WIDGET_TYPES = [
  "now_playing",
  "top_games",
  "deadlock",
  "voice_hours",
  "listening",
  "badges",
  "servers",
  "quote",
  "guestbook",
  "mood",
  "photos",
  "links",
  "countdown",
  "custom",
] as const;
export type WidgetType = (typeof WIDGET_TYPES)[number];

export const WIDGET_SIZES = ["S", "M", "L"] as const;
export type WidgetSize = (typeof WIDGET_SIZES)[number];

/** The picker's and the Studio library's groups, in order. */
export const WIDGET_CATEGORIES = [
  "games",
  "voice",
  "show",
  "social",
  "yours",
] as const;
export type WidgetCategory = (typeof WIDGET_CATEGORIES)[number];

export const WIDGET_CATEGORY_LABELS: Record<WidgetCategory, string> = {
  games: "Games",
  voice: "Voice & music",
  show: "Show off",
  social: "Social",
  yours: "Yours",
};

export interface WidgetSpec {
  type: WidgetType;
  /** The legend printed on the plate and the picker's name. */
  name: string;
  /** One sentence for the picker. */
  blurb: string;
  category: WidgetCategory;
  sizes: readonly WidgetSize[];
  defaultSize: WidgetSize;
  /** How many of this type one board may hold. */
  max: number;
  /** A plan perk the widget needs to be shown to others. */
  perk?: Perk;
}

export const WIDGET_SPECS: Record<WidgetType, WidgetSpec> = {
  now_playing: {
    type: "now_playing",
    name: "Now playing",
    blurb: "What you're in right now, or the last thing you played.",
    category: "games",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  top_games: {
    type: "top_games",
    name: "Top games",
    blurb: "Your most played over 7 days, 30 days or all time.",
    category: "games",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  deadlock: {
    type: "deadlock",
    name: "Deadlock",
    blurb:
      "Your rank, heroes and recent matches, and what you're playing right now.",
    category: "games",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  voice_hours: {
    type: "voice_hours",
    name: "Voice hours",
    blurb: "Hours in calls this month, day by day.",
    category: "voice",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  listening: {
    type: "listening",
    name: "Listening",
    blurb: "The track you're sharing in Listen Together, or the last one.",
    category: "voice",
    sizes: ["S", "M"],
    defaultSize: "S",
    max: 1,
  },
  badges: {
    type: "badges",
    name: "Badge case",
    blurb: "Pick up to 8 badges and their order.",
    category: "show",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  servers: {
    type: "servers",
    name: "Favourite servers",
    blurb: "Up to 4 of your servers.",
    category: "social",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 1,
  },
  quote: {
    type: "quote",
    name: "Pinned message",
    blurb: "Pin a message you sent.",
    category: "social",
    sizes: ["S", "M", "L"],
    defaultSize: "L",
    max: 1,
  },
  guestbook: {
    type: "guestbook",
    name: "Guestbook",
    blurb: "People you share a server or DM with leave short notes.",
    category: "social",
    sizes: ["M", "L"],
    defaultSize: "M",
    max: 1,
  },
  mood: {
    type: "mood",
    name: "Mood",
    blurb: "Yappy in a pose, with a line about how it's going.",
    category: "show",
    sizes: ["S", "M"],
    defaultSize: "S",
    max: 1,
  },
  photos: {
    type: "photos",
    name: "Photo strip",
    blurb: "Up to 4 pictures with a caption.",
    category: "show",
    sizes: ["S", "M", "L"],
    defaultSize: "M",
    max: 2,
  },
  links: {
    type: "links",
    name: "Links",
    blurb: "Up to 5 links, each with a label.",
    category: "show",
    sizes: ["S", "M"],
    defaultSize: "S",
    max: 2,
  },
  countdown: {
    type: "countdown",
    name: "Countdown",
    blurb: "Days and hours to something you're looking forward to.",
    category: "show",
    sizes: ["S", "M"],
    defaultSize: "S",
    max: 1,
  },
  custom: {
    type: "custom",
    name: "Custom",
    blurb: "Your own plate: a title, a picture and four short lines.",
    category: "yours",
    sizes: ["S", "M", "L"],
    defaultSize: "L",
    max: 2,
    perk: "customWidgets",
  },
};

/** Most widgets one board holds. */
export const MAX_WIDGETS = 12;
/** The stored document's ceiling, as UTF-8 JSON after normalisation. */
export const MAX_BOARD_BYTES = 8 * 1024;

/** Text limits, in grapheme clusters (what a person counts as characters). */
export const TEXT_LIMITS = {
  legend: 24,
  title: 40,
  line: 48,
  mood: 60,
  caption: 80,
  alt: 80,
  linkLabel: 24,
  url: 200,
  countdownTitle: 40,
  quote: 280,
  guestbook: 140,
  wishlistHint: 80,
  wishlistName: 40,
} as const;

export const MAX_BADGES_SHOWN = 8;
export const MAX_SERVERS_SHOWN = 4;
export const MAX_PHOTOS = 4;
export const MAX_LINKS = 5;
export const MAX_CUSTOM_LINES = 4;
export const MAX_WISHLIST_ITEMS = 24;

/** Yappy poses with existing art: YapBark (bark) and YappyPose (the rest). */
export const MOOD_POSES = ["bark", "stuffed", "sad", "squashed"] as const;
export type MoodPose = (typeof MOOD_POSES)[number];

export const TOP_GAMES_RANGES = ["7d", "30d", "all"] as const;
export type TopGamesRange = (typeof TOP_GAMES_RANGES)[number];

export const VOICE_HOURS_PERIODS = ["month", "30d"] as const;

/** Deadlock: what the plate shows when the owner is not in the game. */
export const DEADLOCK_AT_REST = ["career", "last_match"] as const;
export type DeadlockAtRest = (typeof DEADLOCK_AT_REST)[number];
export const DEADLOCK_WIDGET_RANGES = ["7d", "30d", "season"] as const;
export type VoiceHoursPeriod = (typeof VOICE_HOURS_PERIODS)[number];

export interface PhotoItem {
  /** An uploaded asset name (`isWidgetAssetName`), resolved per owner. */
  asset: string;
  alt: string;
}

export interface LinkItem {
  label: string;
  url: string;
}

/** A pinned message as it was when pinned; never a live join. */
export interface QuoteSnapshot {
  text: string;
  /** ISO time the message was sent. */
  at: string;
  channel_id: string;
  channel_name: string | null;
  server_id: string | null;
  server_name: string | null;
}

export interface WidgetConfigMap {
  now_playing: { when_idle: "last_played" | "hide" };
  top_games: { range: TopGamesRange };
  /**
   * Every game mode counts. `show_party`: name Yap friends in the party
   * (only people who linked Steam too) on the live plate.
   */
  deadlock: {
    at_rest: DeadlockAtRest;
    range: DeadlockRange;
    show_party: boolean;
  };
  voice_hours: { period: VoiceHoursPeriod };
  listening: Record<string, never>;
  /** Empty means every badge the owner has, newest first. */
  badges: { badge_ids: string[] };
  servers: { server_ids: string[] };
  /** The server fills `snapshot` on save from `message_id`. */
  quote: { message_id: string | null; snapshot: QuoteSnapshot | null };
  guestbook: Record<string, never>;
  /** `clears_at`: ISO; once past, the widget shows nothing to others. */
  mood: { pose: MoodPose; text: string; clears_at: string | null };
  photos: { photos: PhotoItem[]; caption: string };
  links: { links: LinkItem[] };
  /** `at`: ISO; null until the owner sets a date. */
  countdown: { title: string; at: string | null };
  custom: {
    legend: string;
    title: string;
    asset: string | null;
    lines: string[];
  };
}

export type WidgetConfig<T extends WidgetType = WidgetType> =
  WidgetConfigMap[T];

export interface ProfileWidget<T extends WidgetType = WidgetType> {
  /** Client-generated, stable for undo and React keys. */
  id: string;
  type: T;
  size: WidgetSize;
  cfg: WidgetConfigMap[T];
}

export type AnyProfileWidget = {
  [T in WidgetType]: ProfileWidget<T>;
}[WidgetType];

export interface ProfileBoard {
  v: 1;
  widgets: AnyProfileWidget[];
}

export const EMPTY_BOARD: ProfileBoard = Object.freeze({
  v: 1,
  widgets: [],
}) as ProfileBoard;

export function isWidgetType(value: unknown): value is WidgetType {
  return (
    typeof value === "string" &&
    (WIDGET_TYPES as readonly string[]).includes(value)
  );
}

export function isWidgetSize(value: unknown): value is WidgetSize {
  return value === "S" || value === "M" || value === "L";
}

/** The config a widget starts with when added. */
export function defaultWidgetConfig<T extends WidgetType>(
  type: T,
): WidgetConfigMap[T] {
  const configs: { [K in WidgetType]: () => WidgetConfigMap[K] } = {
    now_playing: () => ({ when_idle: "last_played" }),
    top_games: () => ({ range: "30d" }),
    deadlock: () => ({ at_rest: "career", range: "30d", show_party: true }),
    voice_hours: () => ({ period: "month" }),
    listening: () => ({}),
    badges: () => ({ badge_ids: [] }),
    servers: () => ({ server_ids: [] }),
    quote: () => ({ message_id: null, snapshot: null }),
    guestbook: () => ({}),
    mood: () => ({ pose: "bark", text: "", clears_at: null }),
    photos: () => ({ photos: [], caption: "" }),
    links: () => ({ links: [] }),
    countdown: () => ({ title: "", at: null }),
    custom: () => ({ legend: "", title: "", asset: null, lines: [] }),
  };
  return configs[type]() as WidgetConfigMap[T];
}

/** A random widget id: 10 characters of [a-z0-9]. */
export function newWidgetId(): string {
  const bytes = new Uint8Array(10);
  crypto.getRandomValues(bytes);
  return [...bytes].map((byte) => ALPHABET[byte % ALPHABET.length]).join("");
}
const ALPHABET = "abcdefghijklmnopqrstuvwxyz0123456789";
const WIDGET_ID_PATTERN = /^[a-zA-Z0-9_-]{4,24}$/;

export function createWidget<T extends WidgetType>(
  type: T,
  size: WidgetSize = WIDGET_SPECS[type].defaultSize,
): ProfileWidget<T> {
  return { id: newWidgetId(), type, size, cfg: defaultWidgetConfig(type) };
}

/** How many more of `type` fit on `board` (0 when full or at the type cap). */
export function remainingOfType(board: ProfileBoard, type: WidgetType) {
  if (board.widgets.length >= MAX_WIDGETS) return 0;
  const used = board.widgets.filter((widget) => widget.type === type).length;
  return Math.max(0, WIDGET_SPECS[type].max - used);
}

/** Whether `entitlement` may show a widget of `type` to others. */
export function canShowWidget(
  type: WidgetType,
  entitlement: Entitlement | null | undefined,
  now: number = Date.now(),
): boolean {
  const perk = WIDGET_SPECS[type].perk;
  return !perk || hasPerk(entitlement, perk, now);
}

// ── Text and URL hygiene ───────────────────────────────────────────────────

// C0/C1 controls, bidi embeddings/overrides/isolates and marks, zero-width
// space and the BOM. ZWJ (U+200D) and variation selectors stay: emoji use
// them.
const STRIP_PATTERN =
  // eslint-disable-next-line no-control-regex
  /[\u0000-\u001f\u007f-\u009f؜​‎‏‪-‮⁦-⁩﻿￹-￻]/g;

let segmenter: Intl.Segmenter | null = null;
function graphemes(value: string): string[] {
  if (typeof Intl !== "undefined" && "Segmenter" in Intl) {
    segmenter ??= new Intl.Segmenter(undefined, { granularity: "grapheme" });
    return Array.from(segmenter.segment(value), (part) => part.segment);
  }
  return Array.from(value);
}

/** Grapheme clusters in `value`: what the editor's counters show. */
export function textLength(value: string): number {
  return graphemes(value).length;
}

/**
 * One line of plain text: NFC, controls and bidi overrides stripped,
 * whitespace collapsed and trimmed, cut to `max` grapheme clusters.
 * Anything that is not a string is "".
 */
export function cleanText(value: unknown, max: number): string {
  if (typeof value !== "string") return "";
  const text = value
    .normalize("NFC")
    .replace(/[\r\n\t]+/g, " ")
    .replace(STRIP_PATTERN, "")
    .replace(/\s+/g, " ")
    .trim();
  const parts = graphemes(text);
  return parts.length > max ? parts.slice(0, max).join("").trimEnd() : text;
}

const IPV4_PATTERN = /^\d{1,3}(\.\d{1,3}){3}$/;

/**
 * A link a profile may show: https only, no credentials, no IP literal or
 * bare host, at most `TEXT_LIMITS.url` characters. Returns the normalised
 * href, or null.
 */
export function cleanUrl(value: unknown): string | null {
  if (typeof value !== "string") return null;
  let raw = value.trim().replace(STRIP_PATTERN, "");
  if (!raw) return null;
  if (!/^[a-z][a-z0-9+.-]*:/i.test(raw)) raw = `https://${raw}`;
  let url: URL;
  try {
    url = new URL(raw);
  } catch {
    return null;
  }
  if (url.protocol !== "https:") return null;
  if (url.username || url.password) return null;
  const host = url.hostname.toLowerCase();
  if (!host.includes(".") || host.endsWith(".")) return null;
  if (IPV4_PATTERN.test(host) || host.startsWith("[")) return null;
  if (url.port) return null;
  const href = url.href;
  return href.length <= TEXT_LIMITS.url ? href : null;
}

/** "kiwi.page" for "https://www.kiwi.page/art": shown beside a link. */
export function linkHost(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "");
  } catch {
    return "";
  }
}

/** An ISO time `Date.parse` accepts, re-serialised; otherwise null. */
export function cleanIsoTime(value: unknown): string | null {
  if (typeof value !== "string" || value.length > 40) return null;
  const at = Date.parse(value);
  return Number.isFinite(at) ? new Date(at).toISOString() : null;
}

// ── Widget assets ──────────────────────────────────────────────────────────

export const WIDGET_ASSET_EXTENSIONS = ["png", "jpg", "gif", "webp"] as const;
const WIDGET_ASSET_PATTERN = /^[a-z0-9]{12,40}\.(png|jpg|gif|webp)$/;

/** An uploaded widget image's stored name: `<random>.<ext>`. */
export function isWidgetAssetName(value: unknown): value is string {
  return typeof value === "string" && WIDGET_ASSET_PATTERN.test(value);
}

/** The R2 key prefix for one owner's widget images. */
export function widgetAssetPrefix(ownerId: string): string {
  return `profile-assets/widget/${ownerId}/`;
}

/** Where a widget image is served (the profile-assets route). */
export function widgetAssetUrl(ownerId: string, asset: string): string {
  return `/api/profile-assets/widget/${encodeURIComponent(ownerId)}/${asset}`;
}

/** Every asset a board references, for orphan cleanup. */
export function boardAssets(board: ProfileBoard): Set<string> {
  const assets = new Set<string>();
  for (const widget of board.widgets) {
    if (widget.type === "photos") {
      for (const photo of widget.cfg.photos) assets.add(photo.asset);
    } else if (widget.type === "custom" && widget.cfg.asset) {
      assets.add(widget.cfg.asset);
    }
  }
  return assets;
}

// ── Parsing ────────────────────────────────────────────────────────────────

const ID_PATTERN = /^[A-Za-z0-9_-]{1,64}$/;

function isId(value: unknown): value is string {
  return typeof value === "string" && ID_PATTERN.test(value);
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function list(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}

function oneOf<T extends string>(
  value: unknown,
  options: readonly T[],
  fallback: T,
): T {
  return typeof value === "string" &&
    (options as readonly string[]).includes(value)
    ? (value as T)
    : fallback;
}

function uniqueIds(value: unknown, max: number): string[] {
  const ids: string[] = [];
  for (const item of list(value)) {
    if (isId(item) && !ids.includes(item)) ids.push(item);
    if (ids.length >= max) break;
  }
  return ids;
}

function parseQuoteSnapshot(value: unknown): QuoteSnapshot | null {
  const raw = record(value);
  const text = cleanText(raw.text, TEXT_LIMITS.quote);
  const at = cleanIsoTime(raw.at);
  if (!text || !at || !isId(raw.channel_id)) return null;
  return {
    text,
    at,
    channel_id: raw.channel_id,
    channel_name: cleanText(raw.channel_name, 100) || null,
    server_id: isId(raw.server_id) ? raw.server_id : null,
    server_name: cleanText(raw.server_name, 100) || null,
  };
}

/** One widget's config, normalised. Unknown keys are dropped. */
export function parseWidgetConfig<T extends WidgetType>(
  type: T,
  value: unknown,
): WidgetConfigMap[T] {
  const raw = record(value);
  const parsers: { [K in WidgetType]: () => WidgetConfigMap[K] } = {
    now_playing: () => ({
      when_idle: oneOf(raw.when_idle, ["last_played", "hide"], "last_played"),
    }),
    top_games: () => ({ range: oneOf(raw.range, TOP_GAMES_RANGES, "30d") }),
    deadlock: () => ({
      at_rest: oneOf(raw.at_rest, DEADLOCK_AT_REST, "career"),
      range: oneOf(raw.range, DEADLOCK_WIDGET_RANGES, "30d"),
      show_party: raw.show_party !== false,
    }),
    voice_hours: () => ({
      period: oneOf(raw.period, VOICE_HOURS_PERIODS, "month"),
    }),
    listening: () => ({}),
    badges: () => ({ badge_ids: uniqueIds(raw.badge_ids, MAX_BADGES_SHOWN) }),
    servers: () => ({
      server_ids: uniqueIds(raw.server_ids, MAX_SERVERS_SHOWN),
    }),
    quote: () => {
      const messageId = isId(raw.message_id) ? raw.message_id : null;
      return {
        message_id: messageId,
        snapshot: messageId ? parseQuoteSnapshot(raw.snapshot) : null,
      };
    },
    guestbook: () => ({}),
    mood: () => ({
      pose: oneOf(raw.pose, MOOD_POSES, "bark"),
      text: cleanText(raw.text, TEXT_LIMITS.mood),
      clears_at: cleanIsoTime(raw.clears_at),
    }),
    photos: () => {
      const photos: PhotoItem[] = [];
      for (const item of list(raw.photos)) {
        const photo = record(item);
        if (!isWidgetAssetName(photo.asset)) continue;
        if (photos.some((existing) => existing.asset === photo.asset)) continue;
        photos.push({
          asset: photo.asset,
          alt: cleanText(photo.alt, TEXT_LIMITS.alt),
        });
        if (photos.length >= MAX_PHOTOS) break;
      }
      return { photos, caption: cleanText(raw.caption, TEXT_LIMITS.caption) };
    },
    links: () => {
      const links: LinkItem[] = [];
      for (const item of list(raw.links)) {
        const link = record(item);
        const url = cleanUrl(link.url);
        if (!url) continue;
        links.push({
          label: cleanText(link.label, TEXT_LIMITS.linkLabel) || linkHost(url),
          url,
        });
        if (links.length >= MAX_LINKS) break;
      }
      return { links };
    },
    countdown: () => ({
      title: cleanText(raw.title, TEXT_LIMITS.countdownTitle),
      at: cleanIsoTime(raw.at),
    }),
    custom: () => ({
      legend: cleanText(raw.legend, TEXT_LIMITS.legend),
      title: cleanText(raw.title, TEXT_LIMITS.title),
      asset: isWidgetAssetName(raw.asset) ? raw.asset : null,
      lines: list(raw.lines)
        .slice(0, MAX_CUSTOM_LINES)
        .map((line) => cleanText(line, TEXT_LIMITS.line)),
    }),
  };
  return parsers[type]() as WidgetConfigMap[T];
}

export interface DroppedWidget {
  /** The widget's id when it had a usable one. */
  id: string | null;
  type: string | null;
  reason: "unknown_type" | "type_limit" | "board_limit" | "duplicate_id";
}

export interface ParsedBoard {
  board: ProfileBoard;
  dropped: DroppedWidget[];
}

/**
 * Reads a board from untrusted JSON (a PUT body, a stored row). Never
 * throws and never rejects wholesale: unknown types, duplicates and
 * widgets past a limit are dropped and reported, a size a type cannot take
 * becomes its default, and every field is normalised. Perk widgets are
 * kept (a lapsed plan must not lose them); `boardForViewer` hides them.
 */
export function parseBoard(value: unknown): ParsedBoard {
  const raw = record(typeof value === "string" ? safeJson(value) : value);
  const widgets: AnyProfileWidget[] = [];
  const dropped: DroppedWidget[] = [];
  const perType = new Map<WidgetType, number>();
  const ids = new Set<string>();

  for (const item of list(raw.widgets)) {
    const entry = record(item);
    const id = WIDGET_ID_PATTERN.test(String(entry.id ?? ""))
      ? String(entry.id)
      : null;
    if (!isWidgetType(entry.type)) {
      dropped.push({
        id,
        type: typeof entry.type === "string" ? entry.type.slice(0, 32) : null,
        reason: "unknown_type",
      });
      continue;
    }
    const type = entry.type;
    if (widgets.length >= MAX_WIDGETS) {
      dropped.push({ id, type, reason: "board_limit" });
      continue;
    }
    const count = perType.get(type) ?? 0;
    if (count >= WIDGET_SPECS[type].max) {
      dropped.push({ id, type, reason: "type_limit" });
      continue;
    }
    const widgetId = id ?? newWidgetId();
    if (ids.has(widgetId)) {
      dropped.push({ id, type, reason: "duplicate_id" });
      continue;
    }
    const spec = WIDGET_SPECS[type];
    const size =
      isWidgetSize(entry.size) && spec.sizes.includes(entry.size)
        ? entry.size
        : spec.defaultSize;
    ids.add(widgetId);
    perType.set(type, count + 1);
    widgets.push({
      id: widgetId,
      type,
      size,
      cfg: parseWidgetConfig(type, entry.cfg),
    } as AnyProfileWidget);
  }

  return { board: { v: 1, widgets }, dropped };
}

function safeJson(value: string): unknown {
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

/** The stored size of `board` in bytes (UTF-8 JSON). */
export function boardBytes(board: ProfileBoard): number {
  return new TextEncoder().encode(JSON.stringify(board)).length;
}

export interface ViewerBoard {
  board: ProfileBoard;
  /**
   * Widgets only the owner sees: perk widgets without the perk ("Draft ·
   * only you"). Empty for everyone else, who never receives them.
   */
  drafts: string[];
}

/**
 * The board as one viewer may see it. Others never receive a perk widget
 * the owner's plan does not cover; the owner gets everything, with those
 * listed as drafts. A mood past its `clears_at` is empty for others.
 */
export function boardForViewer(
  board: ProfileBoard,
  {
    isOwner,
    ownerEntitlement,
    now = Date.now(),
  }: {
    isOwner: boolean;
    ownerEntitlement: Entitlement | null | undefined;
    now?: number;
  },
): ViewerBoard {
  const drafts: string[] = [];
  const widgets: AnyProfileWidget[] = [];
  for (const widget of board.widgets) {
    if (!canShowWidget(widget.type, ownerEntitlement, now)) {
      if (!isOwner) continue;
      drafts.push(widget.id);
    }
    if (
      !isOwner &&
      widget.type === "mood" &&
      widget.cfg.clears_at &&
      Date.parse(widget.cfg.clears_at) <= now
    ) {
      widgets.push({ ...widget, cfg: { ...widget.cfg, text: "" } });
      continue;
    }
    widgets.push(widget);
  }
  return { board: { v: 1, widgets }, drafts };
}

// ── The Rack layout ────────────────────────────────────────────────────────

/** Columns in the Rack. */
export const RACK_COLUMNS = 2;
/** One row unit, in CSS pixels (the native port uses the same number). */
export const RACK_ROW_UNIT = 168;

export interface RackSpan {
  cols: 1 | 2;
  rows: 1 | 2;
}

/** S: half width × 1 row; M: half width × 2 rows; L: full width × 1 row. */
export const RACK_SPANS: Record<WidgetSize, RackSpan> = {
  S: { cols: 1, rows: 1 },
  M: { cols: 1, rows: 2 },
  L: { cols: 2, rows: 1 },
};

export interface RackCell {
  /** 0-based column. */
  col: number;
  /** 0-based row. */
  row: number;
  cols: 1 | 2;
  rows: 1 | 2;
}

/**
 * Places widgets in board order on the two-column Rack: each takes the
 * first free slot, scanning rows top to bottom and columns left to right
 * (CSS `grid-auto-flow: dense`), so a later half-width widget fills a hole
 * an earlier one left. Web and native both render these cells, so the two
 * never disagree about where a widget sits.
 */
export function packRack(sizes: readonly WidgetSize[]): {
  cells: RackCell[];
  rows: number;
} {
  const taken: boolean[][] = [];
  const isFree = (row: number, col: number) => !taken[row]?.[col];
  const take = (row: number, col: number) => {
    taken[row] ??= [];
    taken[row][col] = true;
  };
  const cells: RackCell[] = [];
  let rows = 0;
  for (const size of sizes) {
    const span = RACK_SPANS[size];
    let placed: RackCell | null = null;
    for (let row = 0; !placed; row += 1) {
      for (let col = 0; col + span.cols <= RACK_COLUMNS; col += 1) {
        let fits = true;
        for (let r = row; r < row + span.rows && fits; r += 1) {
          for (let c = col; c < col + span.cols && fits; c += 1) {
            if (!isFree(r, c)) fits = false;
          }
        }
        if (fits) {
          placed = { col, row, cols: span.cols, rows: span.rows };
          break;
        }
      }
    }
    for (let r = placed.row; r < placed.row + placed.rows; r += 1) {
      for (let c = placed.col; c < placed.col + placed.cols; c += 1) {
        take(r, c);
      }
    }
    rows = Math.max(rows, placed.row + placed.rows);
    cells.push(placed);
  }
  return { cells, rows };
}

// ── Board data (what the server derives for the widgets) ──────────────────

export interface BoardGameTotal {
  name: string;
  icon_url: string | null;
  seconds: number;
}

export interface BoardUserRef {
  id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
}

export interface GuestbookNote {
  id: string;
  author: BoardUserRef;
  body: string;
  created_at: string;
}

export interface BoardServer {
  id: string;
  name: string;
  icon_url: string | null;
  member_count: number;
  /** Whether the viewer is in it too. */
  viewer_is_member: boolean;
}

/**
 * What the widgets render besides their config, filled by the server only
 * for the widget types on the board.
 */
export interface BoardData {
  /** now_playing: the last finished game, and play time for it. */
  last_game?: {
    name: string;
    icon_url: string | null;
    ended_at: string;
    seconds: number;
  } | null;
  /** Seconds per game name over the week and all time (now_playing). */
  game_totals?: Record<string, { week: number; all: number }>;
  /** top_games: up to 5, most played first, for the widget's range. */
  top_games?: {
    range: TopGamesRange;
    games: BoardGameTotal[];
    total_seconds: number;
    game_count: number;
  };
  /**
   * deadlock: the owner's career digest and its state. Others receive it
   * only when there is something to show (see boardForViewerData).
   */
  deadlock?: DeadlockBoardData;
  /** voice_hours: one entry per day of the period, oldest first. */
  voice_hours?: {
    period: VoiceHoursPeriod;
    /** ISO day of the first entry. */
    start_day: string;
    days: number[];
    total_seconds: number;
    previous_seconds: number;
    stream_seconds: number;
    longest_seconds: number;
  };
  /** listening: the last Listen Together session. */
  last_listen?: {
    tracks: number;
    top_track: string | null;
    ended_at: string;
  } | null;
  /** badges: awarded badge ids with dates, newest first. */
  badges?: Array<{ id: string; awarded_at: string }>;
  servers?: BoardServer[];
  /** quote: false when the viewer cannot read the pinned message's server. */
  quote_visible?: boolean;
  guestbook?: {
    enabled: boolean;
    notes: GuestbookNote[];
    total: number;
    /** Whether the viewer may post (not the owner, shares a server or DM,
     * not blocked from it, guestbook on). */
    can_post: boolean;
    /** Why the viewer may not post (null when they may), so the plate can
     * say so. Absent from older servers. */
    denial?: GuestbookDenial | null;
  };
}

export interface BoardResponse {
  user_id: string;
  board: ProfileBoard;
  version: number;
  /** Owner only: widget ids shown to nobody else. */
  drafts: string[];
  is_owner: boolean;
  data: BoardData;
  updated_at: string | null;
}

export interface BoardSaveResponse {
  board: ProfileBoard;
  version: number;
  drafts: string[];
  dropped: DroppedWidget[];
  updated_at: string;
}

/** The 409 body of a save whose version is behind the server's. */
export interface BoardConflictBody {
  error: string;
  code: "BOARD_VERSION_CONFLICT";
  board: ProfileBoard;
  version: number;
}

// ── Activity ───────────────────────────────────────────────────────────────

export const ACTIVITY_KINDS = [
  "game",
  "voice",
  "stream",
  "listen",
  "badge",
  "server_join",
] as const;
export type ActivityKind = (typeof ACTIVITY_KINDS)[number];

export const ACTIVITY_KIND_LABELS: Record<ActivityKind, string> = {
  game: "Games",
  voice: "Voice",
  stream: "Streams",
  listen: "Listen Together",
  badge: "Badges",
  server_join: "Servers joined",
};

export const ACTIVITY_RETENTION_DAYS = [7, 30, 90] as const;
export type ActivityRetentionDays = (typeof ACTIVITY_RETENTION_DAYS)[number];
export const DEFAULT_ACTIVITY_RETENTION: ActivityRetentionDays = 30;

/** Shortest sessions worth an event, in seconds. */
export const ACTIVITY_MIN_SECONDS = {
  game: 5 * 60,
  voice: 15 * 60,
  stream: 5 * 60,
} as const;
/** Fewest tracks for a Listen Together event. */
export const ACTIVITY_MIN_LISTEN_TRACKS = 3;

export function isActivityKind(value: unknown): value is ActivityKind {
  return (
    typeof value === "string" &&
    (ACTIVITY_KINDS as readonly string[]).includes(value)
  );
}

export interface ActivityEventData {
  /** game: name and icon; stream: game name when one was playing. */
  name?: string;
  icon_url?: string | null;
  /** voice/stream: where. Dropped for viewers outside that server. */
  channel_id?: string;
  channel_name?: string;
  server_id?: string;
  server_name?: string;
  /** listen: tracks played and the most played one. */
  tracks?: number;
  top_track?: string;
  /** badge: the badge id is `ref`. */
}

export interface ActivityEvent {
  id: string;
  kind: ActivityKind;
  ref: string | null;
  data: ActivityEventData;
  started_at: string;
  ended_at: string | null;
  /** Seconds, for sessions. */
  seconds: number | null;
  /** Owner only: hidden from everyone else. */
  hidden: boolean;
}

export interface ActivitySettings {
  retention_days: ActivityRetentionDays;
  /** Kinds shown only to the owner. */
  hidden_kinds: ActivityKind[];
}

export const DEFAULT_ACTIVITY_SETTINGS: ActivitySettings = Object.freeze({
  retention_days: DEFAULT_ACTIVITY_RETENTION,
  hidden_kinds: [],
}) as ActivitySettings;

export function parseActivitySettings(value: unknown): ActivitySettings {
  const raw = record(typeof value === "string" ? safeJson(value) : value);
  const retention = Number(raw.retention_days);
  const hidden: ActivityKind[] = [];
  for (const kind of list(raw.hidden_kinds)) {
    if (isActivityKind(kind) && !hidden.includes(kind)) hidden.push(kind);
  }
  return {
    retention_days: (ACTIVITY_RETENTION_DAYS as readonly number[]).includes(
      retention,
    )
      ? (retention as ActivityRetentionDays)
      : DEFAULT_ACTIVITY_RETENTION,
    hidden_kinds: hidden,
  };
}

export interface ActivityPage {
  events: ActivityEvent[];
  next_cursor: string | null;
  /** Owner only. */
  settings?: ActivitySettings;
  is_owner: boolean;
}

// ── Profile preferences ────────────────────────────────────────────────────

export interface ProfilePrefs {
  activity: ActivitySettings;
  guestbook_enabled: boolean;
}

// ── Wishlist ───────────────────────────────────────────────────────────────

export type WishlistItem =
  | {
      id: string;
      kind: "shop";
      /** The collectible's sku in the shop catalog. */
      sku: string;
      /** The name when it was added, for when the catalog has dropped it. */
      name: string;
    }
  | { id: string; kind: "hint"; text: string };

const SKU_PATTERN = /^[A-Za-z0-9_-]{1,40}$/;

/** A wishlist from untrusted JSON: hints cleaned, duplicates dropped. */
export function parseWishlist(value: unknown): WishlistItem[] {
  const items: WishlistItem[] = [];
  const skus = new Set<string>();
  const ids = new Set<string>();
  for (const item of list(
    typeof value === "string" ? safeJson(value) : value,
  )) {
    const raw = record(item);
    let id = WIDGET_ID_PATTERN.test(String(raw.id ?? ""))
      ? String(raw.id)
      : newWidgetId();
    if (ids.has(id)) id = newWidgetId();
    if (raw.kind === "shop") {
      if (typeof raw.sku !== "string" || !SKU_PATTERN.test(raw.sku)) continue;
      if (skus.has(raw.sku)) continue;
      skus.add(raw.sku);
      ids.add(id);
      items.push({
        id,
        kind: "shop",
        sku: raw.sku,
        name: cleanText(raw.name, TEXT_LIMITS.wishlistName),
      });
    } else if (raw.kind === "hint") {
      const text = cleanText(raw.text, TEXT_LIMITS.wishlistHint);
      if (!text) continue;
      ids.add(id);
      items.push({ id, kind: "hint", text });
    }
    if (items.length >= MAX_WISHLIST_ITEMS) break;
  }
  return items;
}

// ── Guestbook ──────────────────────────────────────────────────────────────

/**
 * Why someone may not sign a guestbook: it is theirs, the owner closed it,
 * either side blocked the other (or the owner blocked them from it), or they
 * share no server or DM with the owner.
 */
export type GuestbookDenial =
  | "GUESTBOOK_OWN"
  | "GUESTBOOK_OFF"
  | "GUESTBOOK_BLOCKED"
  | "GUESTBOOK_NO_SHARED_SPACE";

/** Notes one author may leave on one owner's guestbook per day. */
export const GUESTBOOK_DAILY_LIMIT = 5;
/** Notes the board's widget shows. */
export const GUESTBOOK_PREVIEW = 3;

/** A note's body, or null when nothing is left after cleaning. */
export function cleanGuestbookBody(value: unknown): string | null {
  const body = cleanText(value, TEXT_LIMITS.guestbook);
  return body ? body : null;
}
