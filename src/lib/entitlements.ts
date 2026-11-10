/**
 * Tiers and perks: what a person's plan lets them do. Pure (no server-only
 * imports), so the server routes and the client share one answer.
 *
 * Feature code asks `hasPerk(entitlement, perk)` and never compares tier
 * strings: a new tier, a trial or an expiry then changes one table here, not
 * every call site.
 *
 * Where each perk is enforced today:
 *   bigUploads  the server, from the entitlement it reads itself
 *               (upload-limits.ts uploadLimitFor)
 *   hdStreams   the client only: 1440p and 4K are hidden from people
 *               without it, but nothing on the server stops a client that
 *               asks for them. Server enforcement (the SFU path) comes later.
 *   customWidgets  the server: a Custom profile widget is stored for anyone
 *               but only shown to others while the owner has the perk
 *               (profile-board.ts boardForViewer); a lapsed plan hides it
 *               without deleting it.
 *   the rest    the client only, for now (profile styling)
 */

export const TIERS = ["free", "loud", "founder"] as const;
export type Tier = (typeof TIERS)[number];

export const ENTITLEMENT_SOURCES = ["grant", "stripe", "admin"] as const;
export type EntitlementSource = (typeof ENTITLEMENT_SOURCES)[number];

export const PERKS = [
  "bigUploads",
  "hdStreams",
  "allNameStyles",
  "animatedProfile",
  "profileThemes",
  "founderEffect",
  "customWidgets",
] as const;
export type Perk = (typeof PERKS)[number];

/**
 * A user's plan as the API sends it (users/me and profiles). No row in
 * `user_entitlements` means free. Billing ids never leave the server.
 */
export interface Entitlement {
  tier: Tier;
  /** Absent for free (no row). */
  source?: EntitlementSource | null;
  started_at?: string | null;
  /** Null for founders and admin grants: they do not run out. */
  expires_at?: string | null;
}

export const FREE_ENTITLEMENT: Entitlement = Object.freeze({
  tier: "free",
  source: null,
  started_at: null,
  expires_at: null,
});

const TIER_PERKS: Record<Tier, ReadonlySet<Perk>> = {
  free: new Set(),
  loud: new Set([
    "bigUploads",
    "hdStreams",
    "allNameStyles",
    "animatedProfile",
    "profileThemes",
    "customWidgets",
  ]),
  // Everything in Loud, forever, plus the Founder-only effect.
  founder: new Set(PERKS),
};

export function isTier(value: unknown): value is Tier {
  return (
    typeof value === "string" && (TIERS as readonly string[]).includes(value)
  );
}

/**
 * The tier in force at `now`: an expired plan (a lapsed subscription the
 * webhook has not cleared yet) counts as free.
 */
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

/** Whether `entitlement` includes `perk` at `now`. The one perk check. */
export function hasPerk(
  entitlement: Entitlement | null | undefined,
  perk: Perk,
  now: number = Date.now(),
): boolean {
  return TIER_PERKS[effectiveTier(entitlement, now)].has(perk);
}

/**
 * Reads an entitlement from untrusted JSON (an API response, a stored row).
 * Anything unreadable is free.
 */
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

// ── Plans, for the tier cards ───────────────────────────────────────────────

export interface TierPlan {
  tier: Tier;
  name: string;
  /** Under the name: the price, or how it is had. */
  price: string;
  /** One line under the price. */
  blurb: string;
  /**
   * What the plan includes, in the order the card lists it.
   * `UPLOADS_FEATURE` stands for the upload size the plan gets today; the
   * card fills it in from the enforced limit (clientUploadLimitFor).
   */
  features: string[];
}

/** Placeholder for "{size} uploads" in `TierPlan.features`. */
export const UPLOADS_FEATURE = "{uploads}";

/**
 * The three plans as the Yap Loud page shows them (spec 2026-11-09). The
 * Loud price is a placeholder until billing ships.
 */
export const TIER_PLANS: readonly TierPlan[] = [
  {
    tier: "free",
    name: "Yap",
    price: "Free",
    blurb: "Everything you need to hang out.",
    features: [
      UPLOADS_FEATURE,
      "1080p60 streams",
      "Classic name styles",
      "400% volume, soundboard and Listen Together",
    ],
  },
  {
    tier: "loud",
    name: "Yap Loud",
    price: "$4.99 a month",
    blurb: "Turn it up.",
    features: [
      UPLOADS_FEATURE,
      "1440p and 4K streams",
      "Every name style and animated effect",
      "Animated avatar, banner and nameplate",
      "Custom profile themes",
      "Custom profile widgets",
      "The Loud badge",
    ],
  },
  {
    tier: "founder",
    name: "Founder",
    price: "Lifetime",
    blurb: "For everyone who was here before launch.",
    features: [
      "Everything in Loud, forever",
      "The Founder badge",
      "A Founder-only name effect and frame",
      "Early access",
    ],
  },
];

export const TIER_NAMES: Record<Tier, string> = {
  free: "Yap",
  loud: "Yap Loud",
  founder: "Founder",
};
