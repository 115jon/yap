/**
 * Pure helpers for the Activity tab: durations ("3h 40m"), day groups
 * (Today, Yesterday, Earlier this week, then one group per date) and the
 * sentence each event prints. No React and no DOM, so they are unit-tested.
 *
 * Days are the viewer's local days: an event at 23:50 last night is
 * "Yesterday" wherever the owner was.
 */

import { getBadge } from "@/lib/badges";
import {
  ACTIVITY_KIND_LABELS,
  type ActivityEvent,
  type ActivityEventData,
  type ActivityKind,
} from "@/lib/profile-board";

const DAY_MS = 86_400_000;

// ── Durations and times ─────────────────────────────────────────────────

/** "48m", "2h", "3h 40m", "2h 05m". Under a minute is "<1m". */
export function formatActivityDuration(seconds: number | null): string | null {
  if (seconds === null || !Number.isFinite(seconds) || seconds < 0) {
    return null;
  }
  const minutes = Math.floor(seconds / 60);
  if (minutes < 1) return "<1m";
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest === 0
    ? `${hours}h`
    : `${hours}h ${String(rest).padStart(2, "0")}m`;
}

/** Local midnight of the day `ms` falls in. */
export function startOfLocalDay(ms: number): number {
  const date = new Date(ms);
  date.setHours(0, 0, 0, 0);
  return date.getTime();
}

/** Whole local days from `ms`'s day back to `now`'s day (0 = today). */
export function localDaysAgo(ms: number, now: number): number {
  // Round, not floor: a day across a DST change is 23 or 25 hours.
  return Math.round((startOfLocalDay(now) - startOfLocalDay(ms)) / DAY_MS);
}

const clockFormat = new Intl.DateTimeFormat(undefined, {
  hour: "numeric",
  minute: "2-digit",
});
const weekdayFormat = new Intl.DateTimeFormat("en-GB", { weekday: "short" });
const dateFormat = new Intl.DateTimeFormat("en-GB", {
  weekday: "short",
  day: "numeric",
  month: "short",
});
const dateYearFormat = new Intl.DateTimeFormat("en-GB", {
  day: "numeric",
  month: "short",
  year: "numeric",
});

export function formatClock(ms: number): string {
  return clockFormat.format(ms);
}

// ── Day groups ──────────────────────────────────────────────────────────

export type ActivityGroupKind = "today" | "yesterday" | "week" | "date";

export interface ActivityGroup {
  /** Stable key: "today", "yesterday", "week", or "date:YYYY-MM-DD". */
  key: string;
  kind: ActivityGroupKind;
  label: string;
  events: ActivityEvent[];
}

function dayKey(ms: number): string {
  const date = new Date(ms);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/** Which group an event starting at `ms` sits in, seen at `now`. */
export function activityGroupFor(
  ms: number,
  now: number,
): Pick<ActivityGroup, "key" | "kind" | "label"> {
  const ago = localDaysAgo(ms, now);
  // A clock a little ahead of the server's still reads as today.
  if (ago <= 0) return { key: "today", kind: "today", label: "Today" };
  if (ago === 1) {
    return { key: "yesterday", kind: "yesterday", label: "Yesterday" };
  }
  if (ago < 7) {
    return { key: "week", kind: "week", label: "Earlier this week" };
  }
  const sameYear = new Date(ms).getFullYear() === new Date(now).getFullYear();
  return {
    key: `date:${dayKey(ms)}`,
    kind: "date",
    label: (sameYear ? dateFormat : dateYearFormat).format(ms),
  };
}

/**
 * Events (newest first, as the server pages them) in day groups, in order.
 * Unparseable dates fall into the oldest group rather than vanish.
 */
export function groupActivity(
  events: readonly ActivityEvent[],
  now: number,
): ActivityGroup[] {
  const groups: ActivityGroup[] = [];
  const byKey = new Map<string, ActivityGroup>();
  for (const event of events) {
    const ms = Date.parse(event.started_at);
    const head = Number.isFinite(ms)
      ? activityGroupFor(ms, now)
      : { key: "unknown", kind: "date" as const, label: "Earlier" };
    let group = byKey.get(head.key);
    if (!group) {
      group = { ...head, events: [] };
      byKey.set(head.key, group);
      groups.push(group);
    }
    group.events.push(event);
  }
  return groups;
}

/**
 * When an event happened, in the words its group leaves out: "21:04" under
 * Today, Yesterday or a dated group; "Mon · 21:04" under Earlier this week.
 * Sessions print their span: "19:58–23:38".
 */
export function activityWhen(
  event: Pick<ActivityEvent, "started_at" | "ended_at" | "kind">,
  groupKind: ActivityGroupKind,
): string {
  const start = Date.parse(event.started_at);
  if (!Number.isFinite(start)) return "";
  const end = event.ended_at ? Date.parse(event.ended_at) : Number.NaN;
  const span =
    Number.isFinite(end) && end > start && SESSION_KINDS.has(event.kind)
      ? `${formatClock(start)}–${formatClock(end)}`
      : formatClock(start);
  return groupKind === "week"
    ? `${weekdayFormat.format(start)} · ${span}`
    : span;
}

// ── What an event says ──────────────────────────────────────────────────

const SESSION_KINDS = new Set<ActivityKind>([
  "game",
  "voice",
  "stream",
  "listen",
]);

/** A run of the event's sentence; `strong` runs are names and amounts. */
export interface ActivityRun {
  text: string;
  strong?: boolean;
}

export interface ActivityDescription {
  runs: ActivityRun[];
  /** Secondary line: a detail before the time ("Most played: …"). */
  detail: string | null;
  /** Session length for the trailing readout ("1h 42m"). */
  readout: string | null;
}

/** " › " between server and channel, with no-break spaces so the
 * channel never starts a line alone. */
export const PLACE_JOIN = "\u00a0\u203a\u00a0";

/**
 * Where a call was: "Raid HQ › Lounge" when the viewer can see it, "a call"
 * for the owner's own DM call, "a private server" when the server dropped
 * the names because the viewer is not in that server.
 */
export function activityPlace(
  data: ActivityEventData,
): { name: string; known: true } | { name: string; known: false } {
  if (data.server_name) {
    return {
      name: data.channel_name
        ? `${data.server_name}${PLACE_JOIN}${data.channel_name}`
        : data.server_name,
      known: true,
    };
  }
  if (data.channel_id && !data.server_id)
    return { name: "a call", known: false };
  return { name: "a private server", known: false };
}

function placeRuns(data: ActivityEventData): ActivityRun[] {
  const place = activityPlace(data);
  return place.known
    ? [{ text: " in " }, { text: place.name, strong: true }]
    : [{ text: ` in ${place.name}` }];
}

export function describeActivity(event: ActivityEvent): ActivityDescription {
  const { data } = event;
  const readout = SESSION_KINDS.has(event.kind)
    ? formatActivityDuration(event.seconds)
    : null;
  switch (event.kind) {
    case "game":
      return {
        runs: data.name
          ? [{ text: "Played " }, { text: data.name, strong: true }]
          : [{ text: "Played a game" }],
        detail: null,
        readout,
      };
    case "voice":
      return {
        runs: [{ text: "Hung out" }, ...placeRuns(data)],
        detail: null,
        readout,
      };
    case "stream":
      return {
        runs: [
          { text: "Streamed" },
          ...(data.name
            ? [{ text: " " }, { text: data.name, strong: true }]
            : []),
          ...placeRuns(data),
        ],
        detail: null,
        readout,
      };
    case "listen": {
      const tracks = data.tracks ?? 0;
      return {
        runs: [
          { text: "Listened together: " },
          {
            text: `${tracks} ${tracks === 1 ? "track" : "tracks"}`,
            strong: true,
          },
          ...placeRuns(data),
        ],
        detail: data.top_track ? `Most played: ${data.top_track}` : null,
        readout,
      };
    }
    case "badge": {
      const badge = event.ref ? getBadge(event.ref) : undefined;
      return {
        runs: badge
          ? [
              { text: "Earned the " },
              { text: badge.name, strong: true },
              { text: " badge" },
            ]
          : [{ text: "Earned a badge" }],
        detail: badge?.description ?? null,
        readout: null,
      };
    }
    case "server_join":
      return {
        runs: data.server_name
          ? [{ text: "Joined " }, { text: data.server_name, strong: true }]
          : [{ text: "Joined a server" }],
        detail: null,
        readout: null,
      };
  }
}

/** The sentence as plain text, for accessible names. */
export function activityText(description: ActivityDescription): string {
  return description.runs.map((run) => run.text).join("");
}

// ── Filter and privacy copy ─────────────────────────────────────────────

/** The filter rail: two to four choices (DESIGN.md, Segmented rail). */
export const ACTIVITY_FILTERS = ["all", "game", "voice", "badge"] as const;
export type ActivityFilter = (typeof ACTIVITY_FILTERS)[number];

export const ACTIVITY_FILTER_LABELS: Record<ActivityFilter, string> = {
  all: "All",
  game: ACTIVITY_KIND_LABELS.game,
  voice: ACTIVITY_KIND_LABELS.voice,
  badge: ACTIVITY_KIND_LABELS.badge,
};

/** The owner's privacy rows: a name and, where it helps, what it covers. */
export const ACTIVITY_KIND_PRIVACY_COPY: Record<
  ActivityKind,
  { label: string; detail: string | null }
> = {
  game: { label: "Games you play", detail: "Sessions of 5 minutes or more" },
  voice: {
    label: "Voice calls",
    detail: "Channel names stay hidden from people outside that server",
  },
  stream: {
    label: "Streams",
    detail: "Channel names stay hidden from people outside that server",
  },
  listen: { label: "Listen Together", detail: "Sessions of 3 tracks or more" },
  badge: { label: "Badges you earn", detail: null },
  server_join: { label: "Servers you join", detail: null },
};

/** Drops events older than `days` of retention at `now`. */
export function withinRetention(
  events: readonly ActivityEvent[],
  days: number,
  now: number,
): ActivityEvent[] {
  const cutoff = now - days * DAY_MS;
  return events.filter((event) => {
    const ms = Date.parse(event.started_at);
    return !Number.isFinite(ms) || ms >= cutoff;
  });
}
