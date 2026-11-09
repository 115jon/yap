/**
 * Entitlements and badges, as the profile API sends them. Pure and
 * framework-free (no React, no Effect), ported from the web's
 * src/lib/entitlements.ts, badges.ts and badge-art.ts: the phone cannot
 * import the web's `@/` alias, so the logic is copied and kept in step by
 * hand.
 *
 * A badge drawing is 16 rows of 16 characters:
 *   "#"  body, lit through the Bayer dither (dense top left, thinning toward
 *        the bottom right)
 *   "@"  edge, always lit
 *   "+"  detail in the second ink
 *   "."  empty
 */

// ── Entitlement ────────────────────────────────────────────────────────────

export const TIERS = ["free", "loud", "founder"] as const;
export type Tier = (typeof TIERS)[number];

export const ENTITLEMENT_SOURCES = ["grant", "stripe", "admin"] as const;
export type EntitlementSource = (typeof ENTITLEMENT_SOURCES)[number];

export interface Entitlement {
  tier: Tier;
  source: EntitlementSource | null;
  started_at: string | null;
  /** Null for founders and admin grants: they do not run out. */
  expires_at: string | null;
}

export const FREE_ENTITLEMENT: Entitlement = Object.freeze({
  tier: "free",
  source: null,
  started_at: null,
  expires_at: null,
});

export const TIER_NAMES: Record<Tier, string> = {
  free: "Yap",
  loud: "Yap Loud",
  founder: "Founder",
};

function isTier(value: unknown): value is Tier {
  return (
    typeof value === "string" && (TIERS as readonly string[]).includes(value)
  );
}

/** Reads an entitlement from untrusted JSON. Anything unreadable is free. */
export function parseEntitlement(value: unknown): Entitlement {
  if (!value || typeof value !== "object") return FREE_ENTITLEMENT;
  const raw = value as Record<string, unknown>;
  if (!isTier(raw.tier)) return FREE_ENTITLEMENT;
  const text = (field: unknown) =>
    typeof field === "string" && field ? field : null;
  const source =
    typeof raw.source === "string" &&
    (ENTITLEMENT_SOURCES as readonly string[]).includes(raw.source)
      ? (raw.source as EntitlementSource)
      : null;
  return {
    tier: raw.tier,
    source,
    started_at: text(raw.started_at),
    expires_at: text(raw.expires_at),
  };
}

/** The tier in force at `now`: an expired plan counts as free. */
export function effectiveTier(
  entitlement: Entitlement | null | undefined,
  now: number = Date.now(),
): Tier {
  if (!entitlement || !isTier(entitlement.tier)) return "free";
  if (entitlement.expires_at) {
    const expiresAt = Date.parse(entitlement.expires_at);
    if (Number.isFinite(expiresAt) && expiresAt <= now) return "free";
  }
  return entitlement.tier;
}

// ── Badges ─────────────────────────────────────────────────────────────────

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

export interface BadgeInfo {
  name: string;
  description: string;
}

export const BADGES: Record<BadgeId, BadgeInfo> = {
  founder: {
    name: "Founder",
    description: "Here before launch. Lifetime Loud.",
  },
  loud: { name: "Loud", description: "Subscribed to Yap Loud." },
  "yap-team": { name: "Yap team", description: "Builds Yap." },
  "bug-hunter": {
    name: "Bug hunter",
    description: "Reported a confirmed bug.",
  },
  "early-yapper": {
    name: "Early yapper",
    description: "Joined in the first 100 users.",
  },
  "server-owner": {
    name: "Server owner",
    description: "Runs a server with 10+ members.",
  },
  "voice-regular": {
    name: "Voice regular",
    description: "100 hours in voice.",
  },
  "night-owl": { name: "Night owl", description: "Yapped at 3 a.m." },
};

export interface UserBadge {
  id: BadgeId;
  awarded_at: string;
}

export function isBadgeId(value: string): value is BadgeId {
  return (BADGE_IDS as readonly string[]).includes(value);
}

/**
 * Reads awards from untrusted JSON: unknown ids dropped (an old app meeting
 * a new badge), one per id, in the fixed rank order.
 */
export function parseUserBadges(value: unknown): UserBadge[] {
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const badges: UserBadge[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const { id, awarded_at } = item as Record<string, unknown>;
    if (typeof id !== "string" || !isBadgeId(id) || seen.has(id)) continue;
    seen.add(id);
    badges.push({
      id,
      awarded_at: typeof awarded_at === "string" ? awarded_at : "",
    });
  }
  return badges.sort(
    (a, b) => BADGE_IDS.indexOf(a.id) - BADGE_IDS.indexOf(b.id),
  );
}

// ── Pixel art ──────────────────────────────────────────────────────────────

export const BADGE_CELLS = 16;

export type BadgeInk = "body" | "edge" | "detail";

const FOUNDER = [
  "................",
  "................",
  ".......++.......",
  ".@.....++.....@.",
  ".@@....@@....@@.",
  ".@#@..@##@..@#@.",
  ".@##@@####@@##@.",
  ".@############@.",
  ".@############@.",
  ".@############@.",
  ".@############@.",
  ".@@@@@@@@@@@@@@.",
  ".@@+@@@++@@@+@@.",
  ".@@@@@@@@@@@@@@.",
  "................",
  "................",
];

const LOUD = [
  "................",
  "...........+....",
  "......@@....+...",
  ".....@#@.+...+..",
  "....@##@..+...+.",
  "@@@@###@..+...+.",
  "@######@...+...+",
  "@######@...+...+",
  "@######@...+...+",
  "@######@...+...+",
  "@@@@###@..+...+.",
  "....@##@..+...+.",
  ".....@#@.+...+..",
  "......@@....+...",
  "...........+....",
  "................",
];

const BUG_HUNTER = [
  "................",
  "....+......+....",
  ".....+....+.....",
  "......@@@@......",
  ".....@@@@@@.....",
  "+...@##@@##@...+",
  ".+.@###@@###@.+.",
  "..+@###@@###@+..",
  "...@###@@###@...",
  "+++@###@@###@+++",
  "...@###@@###@...",
  "..+@###@@###@+..",
  ".+..@##@@##@..+.",
  "+....@@@@@@....+",
  "................",
  "................",
];

const EARLY_YAPPER = [
  "................",
  "................",
  ".......++.......",
  "..+....++....+..",
  "...+........+...",
  "......@@@@......",
  "....@@####@@....",
  "...@########@...",
  "..@##########@..",
  "..@##########@..",
  ".@############@.",
  "++++++++++++++++",
  "................",
  "...++++++++++...",
  "................",
  "......++++......",
];

const SERVER_OWNER = [
  "................",
  "................",
  ".@@..@@..@@..@@.",
  ".@#@@#@@@@#@@#@.",
  ".@############@.",
  ".@############@.",
  ".@###++##++###@.",
  ".@###++##++###@.",
  ".@############@.",
  ".@#####++#####@.",
  ".@####++++####@.",
  ".@####++++####@.",
  ".@####++++####@.",
  ".@@@@@@@@@@@@@@.",
  "................",
  "................",
];

const VOICE_REGULAR = [
  "................",
  "......@@@@......",
  ".....@####@.....",
  ".....@#++#@.....",
  ".....@####@.....",
  ".....@#++#@.....",
  ".....@####@.....",
  "...+.@####@.+...",
  "...+.@####@.+...",
  "...+..@@@@..+...",
  "....+......+....",
  ".....++++++.....",
  ".......@@.......",
  ".......@@.......",
  ".....@@@@@@.....",
  "................",
];

const NIGHT_OWL = [
  "................",
  "...........+....",
  ".....@@...+++...",
  "...@@@.....+....",
  "..@@#@..........",
  "..@#@.......+...",
  ".@##@......+++..",
  ".@###@......+...",
  ".@###@..........",
  ".@###@@.........",
  ".@####@@........",
  "..@####@@@..@...",
  "..@@######@@@...",
  "...@@#####@@....",
  ".....@@@@@......",
  "................",
];

/** Yappy on 16 cells (see the header). */
const YAP_TEAM = [
  "................",
  ".@..............",
  ".@@.............",
  ".@.@............",
  ".@@#@...........",
  ".@###@@@@@@.....",
  ".@#####@###@@@@.",
  ".@####@.@######@",
  ".@#####@#######@",
  ".@#########@@@@@",
  ".@########@.....",
  ".@#########@@@..",
  "..@######@@@@@..",
  "..@#####@.......",
  "..@#####@.......",
  "..@@@@@@@.......",
];

export const BADGE_ART: Record<BadgeId, readonly string[]> = {
  founder: FOUNDER,
  loud: LOUD,
  "yap-team": YAP_TEAM,
  "bug-hunter": BUG_HUNTER,
  "early-yapper": EARLY_YAPPER,
  "server-owner": SERVER_OWNER,
  "voice-regular": VOICE_REGULAR,
  "night-owl": NIGHT_OWL,
};

const BAYER8: readonly (readonly number[])[] = [
  [0, 32, 8, 40, 2, 34, 10, 42],
  [48, 16, 56, 24, 50, 18, 58, 26],
  [12, 44, 4, 36, 14, 46, 6, 38],
  [60, 28, 52, 20, 62, 30, 54, 22],
  [3, 35, 11, 43, 1, 33, 9, 41],
  [51, 19, 59, 27, 49, 17, 57, 25],
  [15, 47, 7, 39, 13, 45, 5, 37],
  [63, 31, 55, 23, 61, 29, 53, 21],
];

/** Whether cell (`x`, `y`) is lit at `coverage` 0..1 through the 8x8 Bayer. */
export function bayerLit(x: number, y: number, coverage: number): boolean {
  return coverage * 64 > (BAYER8[y % 8]?.[x % 8] ?? 0) + 0.5;
}

/** Light across a badge: dense at the top left, thinning to the bottom right. */
export function badgeLight(x: number, y: number): number {
  const d = (x + y) / (2 * (BADGE_CELLS - 1));
  return 0.98 - d * 0.34;
}

export interface BadgeRun {
  ink: BadgeInk;
  x: number;
  y: number;
  width: number;
}

const INK_OF = new Map<string, BadgeInk>([
  ["#", "body"],
  ["@", "edge"],
  ["+", "detail"],
]);

/**
 * The cells to paint for `badge`, as horizontal runs of one ink per row:
 * body cells only where the dither lights them, edge and detail always.
 */
export function badgeRuns(badge: BadgeId): BadgeRun[] {
  const runs: BadgeRun[] = [];
  BADGE_ART[badge].forEach((row, y) => {
    let current: BadgeRun | null = null;
    for (let x = 0; x < BADGE_CELLS; x++) {
      const ink = INK_OF.get(row[x] ?? ".");
      const lit =
        ink !== undefined &&
        (ink !== "body" || bayerLit(x, y, badgeLight(x, y)));
      if (!lit) {
        current = null;
      } else if (current && current.ink === ink) {
        current.width += 1;
      } else {
        current = { ink, x, y, width: 1 };
        runs.push(current);
      }
    }
  });
  return runs;
}
