/**
 * Site-wide switches and the announcement banner: the shapes, the defaults
 * and the copy. Pure (no server imports), so the server, the admin console
 * and every client read one definition.
 *
 * Stored in D1 `site_settings` (services/site-flags.service.ts): key `flags`
 * holds only the switches an admin turned off, key `announcement` the banner.
 * A missing key is the default: every feature on, no banner.
 */

export const SITE_FLAGS = [
  "signups",
  "chunkedUploads",
  "soundboardUploads",
  "linkEmbeds",
  "listenTogether",
] as const;
export type SiteFlag = (typeof SITE_FLAGS)[number];

export type SiteFlags = Record<SiteFlag, boolean>;

export const DEFAULT_SITE_FLAGS: SiteFlags = Object.freeze({
  signups: true,
  chunkedUploads: true,
  soundboardUploads: true,
  linkEmbeds: true,
  listenTogether: true,
});

export interface SiteFlagInfo {
  /** The console's row label. */
  label: string;
  /** What turning it off stops, for the console. */
  description: string;
  /** What a client is told when it reaches the switched-off path. */
  pausedMessage: string;
}

export const SITE_FLAG_INFO: Record<SiteFlag, SiteFlagInfo> = {
  signups: {
    label: "New sign-ups",
    description:
      "New accounts can join Yap. Off, a first sign-in is refused before a profile is made; existing accounts are unaffected.",
    pausedMessage: "New sign-ups are paused right now. Try again later.",
  },
  chunkedUploads: {
    label: "Chunked uploads",
    description:
      "Files of 16 MB and up go to R2 in parts. Off, new big uploads are refused; uploads already in flight can finish.",
    pausedMessage:
      "Big file uploads are paused right now. Files under 16 MB still send.",
  },
  soundboardUploads: {
    label: "Soundboard uploads",
    description:
      "Members can add sounds to a server's soundboard. Off, new sounds are refused; existing sounds still play.",
    pausedMessage: "Soundboard uploads are paused right now.",
  },
  linkEmbeds: {
    label: "Link embeds",
    description:
      "Links in messages get previews. Off, nothing is fetched from other sites; messages still send, without previews.",
    pausedMessage: "Link previews are paused right now.",
  },
  listenTogether: {
    label: "Listen Together",
    description:
      "Calls can play music together. Off, adding, searching and streaming tracks are refused.",
    pausedMessage: "Listen Together is paused right now.",
  },
};

/** The code every switched-off path answers with (503, see featurePaused). */
export const FEATURE_PAUSED = "FEATURE_PAUSED";

export const ANNOUNCEMENT_TONES = ["info", "warning", "critical"] as const;
export type AnnouncementTone = (typeof ANNOUNCEMENT_TONES)[number];

export const ANNOUNCEMENT_MAX_LENGTH = 280;

export interface SiteAnnouncement {
  /** Changes on every save, so a dismissal is per announcement. */
  id: string;
  text: string;
  tone: AnnouncementTone;
  link_url: string | null;
  link_label: string | null;
  /** ISO time; null runs until it is cleared. */
  expires_at: string | null;
  updated_at: string;
}

/** What GET /api/site-config sends every client. */
export interface PublicSiteConfig {
  flags: SiteFlags;
  announcement: SiteAnnouncement | null;
}

/** Reads stored or sent flags; anything unreadable is the default (on). */
export function parseSiteFlags(value: unknown): SiteFlags {
  const flags: SiteFlags = { ...DEFAULT_SITE_FLAGS };
  if (!value || typeof value !== "object") return flags;
  const raw = value as Record<string, unknown>;
  for (const flag of SITE_FLAGS) {
    if (typeof raw[flag] === "boolean") flags[flag] = raw[flag];
  }
  return flags;
}

export function isAnnouncementTone(value: unknown): value is AnnouncementTone {
  return (
    typeof value === "string" &&
    (ANNOUNCEMENT_TONES as readonly string[]).includes(value)
  );
}

/** Only http(s) links; anything else is dropped. */
export function safeAnnouncementUrl(value: unknown): string | null {
  if (typeof value !== "string" || !value.trim()) return null;
  try {
    const url = new URL(value.trim());
    return url.protocol === "https:" || url.protocol === "http:"
      ? url.toString()
      : null;
  } catch {
    return null;
  }
}

/** Reads a stored or sent announcement; anything unreadable is none. */
export function parseAnnouncement(value: unknown): SiteAnnouncement | null {
  if (!value || typeof value !== "object") return null;
  const raw = value as Record<string, unknown>;
  const text =
    typeof raw.text === "string"
      ? raw.text.trim().slice(0, ANNOUNCEMENT_MAX_LENGTH)
      : "";
  if (!text) return null;
  const linkUrl = safeAnnouncementUrl(raw.link_url);
  const expires =
    typeof raw.expires_at === "string" &&
    Number.isFinite(Date.parse(raw.expires_at))
      ? new Date(raw.expires_at).toISOString()
      : null;
  return {
    id: typeof raw.id === "string" && raw.id ? raw.id : "announcement",
    text,
    tone: isAnnouncementTone(raw.tone) ? raw.tone : "info",
    link_url: linkUrl,
    link_label:
      linkUrl && typeof raw.link_label === "string" && raw.link_label.trim()
        ? raw.link_label.trim().slice(0, 40)
        : null,
    expires_at: expires,
    updated_at:
      typeof raw.updated_at === "string"
        ? raw.updated_at
        : new Date(0).toISOString(),
  };
}

/** The announcement still showing at `now`, or null once it has expired. */
export function activeAnnouncement(
  announcement: SiteAnnouncement | null,
  now: number = Date.now(),
): SiteAnnouncement | null {
  if (!announcement) return null;
  if (announcement.expires_at && Date.parse(announcement.expires_at) <= now) {
    return null;
  }
  return announcement;
}
