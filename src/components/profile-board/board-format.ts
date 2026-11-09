/**
 * What every widget on a board shares (who owns it, who is looking, the
 * clock) and the pure helpers for its readouts, so the widgets and the
 * tests format numbers one way. Media resolution is in board-context.tsx.
 */

import type { IconName } from "@/components/ui/icon";
import type {
  BoardResponse,
  BoardUserRef,
  QuoteSnapshot,
  WidgetType,
} from "@/lib/profile-board";

/** How a board is being looked at. */
export type BoardPreview = "owner" | "anyone";

export interface BoardCtx {
  ownerId: string;
  /** The owner's display name, for copy ("Leave Kiwi a note"). */
  ownerName: string;
  ownerAvatarUrl?: string | null;
  /** The viewer is the owner and is not previewing as someone else. */
  isOwner: boolean;
  /** The signed-in viewer (the guestbook's optimistic author). */
  viewer?: BoardUserRef | null;
  /** Widget ids only the owner sees ("Draft · only you"). */
  drafts: ReadonlySet<string>;
  /** A frozen clock (fixtures, tests). Live clocks tick when unset. */
  now?: number;
  /** When the board was read: `now`, or the time it mounted. Readouts
   * such as "2h ago" use it. */
  at: number;
  /** Jump to a pinned message; the Jump key shows only when set. */
  onOpenMessage?: (messageId: string, snapshot: QuoteSnapshot) => void;
}

/** The glyph on each widget's legend. */
export const WIDGET_ICON: Record<WidgetType, IconName> = {
  now_playing: "gamepad",
  top_games: "trophy",
  voice_hours: "mic",
  listening: "headphones",
  badges: "verified",
  servers: "server",
  quote: "pin",
  guestbook: "rules",
  mood: "emoji",
  photos: "image",
  links: "link",
  countdown: "timer",
  custom: "sparkle",
};

// ── Readout formatting ─────────────────────────────────────────────────────

/** "41.5h", "212h", "45m": a total in the fewest characters. */
export function formatHours(seconds: number): string {
  const s = Math.max(0, seconds);
  if (s < 3600) return `${Math.round(s / 60)}m`;
  const h = s / 3600;
  return h < 100 ? `${trimZero(h.toFixed(1))}h` : `${Math.round(h)}h`;
}

/** "11h 20m", "45m", "0m": a span to the minute. */
export function formatDuration(seconds: number): string {
  const total = Math.max(0, Math.round(seconds / 60));
  const h = Math.floor(total / 60);
  const m = total % 60;
  if (h === 0) return `${m}m`;
  return m === 0 ? `${h}h` : `${h}h ${String(m).padStart(2, "0")}m`;
}

function trimZero(value: string): string {
  return value.endsWith(".0") ? value.slice(0, -2) : value;
}

/** "now", "5m ago", "2h ago", "3d ago", "2w ago", "4mo ago". */
export function formatAgo(iso: string, now: number): string {
  const at = Date.parse(iso);
  if (!Number.isFinite(at)) return "";
  const minutes = Math.floor((now - at) / 60_000);
  if (minutes < 1) return "now";
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 14) return `${days}d ago`;
  if (days < 60) return `${Math.floor(days / 7)}w ago`;
  return `${Math.floor(days / 30)}mo ago`;
}

/** "2d", "5h", "12m": the compact age on a guestbook note. */
export function formatAge(iso: string, now: number): string {
  return formatAgo(iso, now).replace(/ ago$/, "");
}

const DATE_TIME = new Intl.DateTimeFormat(undefined, {
  weekday: "short",
  day: "numeric",
  month: "short",
  hour: "2-digit",
  minute: "2-digit",
});
const DAY_MONTH = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
});
const MONTH = new Intl.DateTimeFormat(undefined, { month: "long" });

/** "Fri 14 Nov, 21:00" in the viewer's locale. */
export function formatDateTime(iso: string): string {
  const at = Date.parse(iso);
  return Number.isFinite(at) ? DATE_TIME.format(at) : "";
}

/** "14 Nov". */
export function formatDayMonth(iso: string | number): string {
  const at = typeof iso === "number" ? iso : Date.parse(iso);
  return Number.isFinite(at) ? DAY_MONTH.format(at) : "";
}

/** "November". */
export function formatMonth(iso: string | number): string {
  const at = typeof iso === "number" ? iso : Date.parse(iso);
  return Number.isFinite(at) ? MONTH.format(at) : "";
}

/** Days, hours and minutes left until `at` (all 0 once it has passed). */
export function countdownParts(at: number, now: number) {
  const minutes = Math.max(0, Math.floor((at - now) / 60_000));
  return {
    passed: at <= now,
    days: Math.floor(minutes / 1440),
    hours: Math.floor((minutes % 1440) / 60),
    minutes: minutes % 60,
  };
}

/** A person's name as the board prints it. */
export function userName(user: BoardUserRef): string {
  return user.display_name?.trim() || user.username;
}

/** How many widgets a viewer sees on `response` (the Board tab's count). */
export function visibleWidgetCount(
  response: BoardResponse,
  previewAs?: BoardPreview,
) {
  if (response.is_owner && previewAs !== "anyone")
    return response.board.widgets.length;
  const drafts = new Set(response.drafts);
  return response.board.widgets.filter((widget) => !drafts.has(widget.id))
    .length;
}
