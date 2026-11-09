/**
 * Badges: the definitions (in code, not the database) and the rules that
 * award the automatic ones. Pure, so the server, the realtime gateway and
 * the client share it. The pixel art lives in `badge-art.ts` (client only).
 *
 * Awards are rows in `user_badges` (user_id, badge_id, awarded_at, source).
 * `loud` is never stored: a profile shows it while the tier is Loud.
 */

import { effectiveTier, type Entitlement } from "@/lib/entitlements";
import { formatProfileReferenceDate } from "@/lib/profile-dates";

export const BADGE_IDS = [
  "founder",
  "loud",
  "yap-team",
  "bug-hunter",
  "early-yapper",
  "server-owner",
  "voice-regular",
  "night-owl",
] as const;
export type BadgeId = (typeof BADGE_IDS)[number];

export type BadgeAwardSource = "auto" | "admin" | "grant";

/** How a badge is earned. */
export type BadgeRule =
  /** Given once, by the founder grant. */
  | { kind: "grant" }
  /** Shown while the tier is Loud; never stored. */
  | { kind: "tier" }
  /** Awarded and revoked by an admin. */
  | { kind: "manual" }
  /** Earned from account stats; checked on change and by the daily sweep. */
  | { kind: "auto"; earned: (stats: BadgeStats) => boolean }
  /** Earned by something happening (a message at 3 a.m.), when it happens. */
  | { kind: "event" };

export interface BadgeDefinition {
  id: BadgeId;
  name: string;
  description: string;
  rule: BadgeRule;
  /** Left out of any list of badges until someone has earned it. */
  hidden?: boolean;
}

/** What the automatic rules read. Missing numbers count as zero. */
export interface BadgeStats {
  voice_seconds?: number | null;
  /** 1 for the first account created, 2 for the next... */
  signup_rank?: number | null;
  /** Members in the biggest server this user owns. */
  owned_server_members?: number | null;
}

export const VOICE_REGULAR_SECONDS = 100 * 60 * 60;
export const EARLY_YAPPER_RANK = 100;
export const SERVER_OWNER_MEMBERS = 10;
/** Local hour (0-23) a message earns Night owl in. */
export const NIGHT_OWL_HOUR = 3;

const n = (value: number | null | undefined) =>
  typeof value === "number" && Number.isFinite(value) ? value : 0;

/** Definitions, in display order (founder and loud first). */
export const BADGES: readonly BadgeDefinition[] = [
  {
    id: "founder",
    name: "Founder",
    description: "Here before launch. Lifetime Loud.",
    rule: { kind: "grant" },
  },
  {
    id: "loud",
    name: "Loud",
    description: "Subscribed to Yap Loud.",
    rule: { kind: "tier" },
  },
  {
    id: "yap-team",
    name: "Yap team",
    description: "Builds Yap.",
    rule: { kind: "manual" },
  },
  {
    id: "bug-hunter",
    name: "Bug hunter",
    description: "Reported a confirmed bug.",
    rule: { kind: "manual" },
  },
  {
    id: "early-yapper",
    name: "Early yapper",
    description: "Joined in the first 100 users.",
    rule: {
      kind: "auto",
      earned: (stats) =>
        n(stats.signup_rank) > 0 && n(stats.signup_rank) <= EARLY_YAPPER_RANK,
    },
  },
  {
    id: "server-owner",
    name: "Server owner",
    description: "Runs a server with 10+ members.",
    rule: {
      kind: "auto",
      earned: (stats) => n(stats.owned_server_members) >= SERVER_OWNER_MEMBERS,
    },
  },
  {
    id: "voice-regular",
    name: "Voice regular",
    description: "100 hours in voice.",
    rule: {
      kind: "auto",
      earned: (stats) => n(stats.voice_seconds) >= VOICE_REGULAR_SECONDS,
    },
  },
  {
    id: "night-owl",
    name: "Night owl",
    description: "Yapped at 3 a.m.",
    rule: { kind: "event" },
    hidden: true,
  },
];

const BY_ID = new Map<string, BadgeDefinition>(
  BADGES.map((badge) => [badge.id, badge]),
);
const RANK = new Map<string, number>(
  BADGES.map((badge, index) => [badge.id, index]),
);

export function isBadgeId(value: unknown): value is BadgeId {
  return typeof value === "string" && BY_ID.has(value);
}

export function getBadge(id: string): BadgeDefinition | undefined {
  return BY_ID.get(id);
}

/** Badges an admin may award or revoke by hand. */
export function isManualBadge(id: string): boolean {
  return BY_ID.get(id)?.rule.kind === "manual";
}

/** The automatic badges `stats` has earned (stat rules only). */
export function earnedAutoBadges(stats: BadgeStats): BadgeId[] {
  return BADGES.flatMap((badge) =>
    badge.rule.kind === "auto" && badge.rule.earned(stats) ? [badge.id] : [],
  );
}

/**
 * The local hour of `at` for a client `utcOffsetMinutes` east of UTC (as
 * `-new Date().getTimezoneOffset()` gives it). Without a usable offset, the
 * hour in UTC.
 */
export function localHour(at: Date, utcOffsetMinutes?: number | null): number {
  const offset =
    typeof utcOffsetMinutes === "number" &&
    Number.isFinite(utcOffsetMinutes) &&
    Math.abs(utcOffsetMinutes) <= 14 * 60
      ? Math.round(utcOffsetMinutes)
      : 0;
  return new Date(at.getTime() + offset * 60_000).getUTCHours();
}

/** Whether a message sent at `at` earns Night owl. */
export function isNightOwlMoment(
  at: Date,
  utcOffsetMinutes?: number | null,
): boolean {
  return localHour(at, utcOffsetMinutes) === NIGHT_OWL_HOUR;
}

// ── Awards, as the API sends them ──────────────────────────────────────────

export interface UserBadge {
  id: BadgeId;
  awarded_at: string;
}

/**
 * Reads awards from untrusted JSON: unknown ids dropped, one per id, sorted
 * by the fixed rank (founder and loud first).
 */
export function parseUserBadges(value: unknown): UserBadge[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const badges: UserBadge[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const { id, awarded_at } = item as Record<string, unknown>;
    if (!isBadgeId(id) || seen.has(id)) continue;
    seen.add(id);
    badges.push({
      id,
      awarded_at: typeof awarded_at === "string" ? awarded_at : "",
    });
  }
  return sortBadges(badges);
}

export function sortBadges<T extends { id: string }>(badges: T[]): T[] {
  return [...badges].sort(
    (a, b) => (RANK.get(a.id) ?? 99) - (RANK.get(b.id) ?? 99),
  );
}

/**
 * A profile's badges: the stored awards plus `loud` while the tier is Loud
 * (dated from when the subscription started), in display order.
 */
export function profileBadges(
  stored: readonly UserBadge[],
  entitlement: Entitlement | null | undefined,
  now: number = Date.now(),
): UserBadge[] {
  const badges = stored.filter((badge) => badge.id !== "loud");
  if (effectiveTier(entitlement, now) === "loud") {
    badges.push({ id: "loud", awarded_at: entitlement?.started_at ?? "" });
  }
  return sortBadges(badges);
}

/** Whole months since `startedAt` (the Loud badge's hover), at least 1. */
export function monthsSince(startedAt: string, now: number = Date.now()) {
  const start = new Date(startedAt);
  if (Number.isNaN(start.getTime())) return 1;
  const end = new Date(now);
  const months =
    (end.getUTCFullYear() - start.getUTCFullYear()) * 12 +
    (end.getUTCMonth() - start.getUTCMonth()) -
    (end.getUTCDate() < start.getUTCDate() ? 1 : 0);
  return Math.max(1, months);
}

/** The tooltip's lines for one award. */
export interface BadgeTooltipLines {
  name: string;
  description: string;
  earned: string | null;
}

export function badgeTooltipLines(
  badge: UserBadge,
  now: number = Date.now(),
): BadgeTooltipLines {
  const def = getBadge(badge.id);
  const name = def?.name ?? badge.id;
  let description = def?.description ?? "";
  if (badge.id === "loud" && badge.awarded_at) {
    const months = monthsSince(badge.awarded_at, now);
    description = `Subscribed for ${months} ${months === 1 ? "month" : "months"}.`;
  }
  const date = formatProfileReferenceDate(badge.awarded_at || null);
  return {
    name,
    description,
    earned: date ? `${badge.id === "loud" ? "Since" : "Earned"} ${date}` : null,
  };
}
