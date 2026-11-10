/**
 * Site Flags Service: the main Worker's read side of the switches and the
 * announcement in D1 `site_settings` (rows: services/site-settings-store,
 * written by the admin Worker in apps/admin).
 *
 * Every switched path asks `featurePaused(flag)`, so reads are cached per
 * isolate for SITE_SETTINGS_TTL_MS: a switch flipped in the console applies
 * everywhere within the TTL. A read that
 * fails keeps the last copy, or the defaults (everything on): a D1 hiccup
 * must not switch features off.
 */

import { env } from "cloudflare:workers";

import { getCorsHeaders } from "@/lib/api-helpers";
import {
  activeAnnouncement,
  FEATURE_PAUSED,
  SITE_FLAG_INFO,
  type PublicSiteConfig,
  type SiteFlag,
} from "@/lib/site-flags";
import {
  DEFAULT_NATIVE_HANDOFF,
  type NativeHandoffConfig,
} from "@/lib/native-handoff";
import {
  defaultSiteSettings,
  readSiteSettings,
  type SiteSettings,
} from "@/services/site-settings-store";

export type { SiteSettings } from "@/services/site-settings-store";

export const SITE_SETTINGS_TTL_MS = 10_000;

type SettingsDb = Pick<D1Database, "prepare">;

let cached: { at: number; settings: SiteSettings } | null = null;

function defaultDb(): SettingsDb | null {
  const db = (env as { DB?: SettingsDb } | undefined)?.DB;
  return db && typeof db.prepare === "function" ? db : null;
}

/** The settings, from this isolate's copy while it is fresh. */
export async function getSiteSettings(
  db: SettingsDb | null = defaultDb(),
  now: number = Date.now(),
): Promise<SiteSettings> {
  if (cached && now - cached.at < SITE_SETTINGS_TTL_MS) return cached.settings;
  if (!db) return cached?.settings ?? defaultSiteSettings();
  try {
    const settings = await readSiteSettings(db);
    cached = { at: now, settings };
    return settings;
  } catch (error) {
    console.warn(
      `site settings unavailable: ${error instanceof Error ? error.message : String(error)}`,
    );
    return cached?.settings ?? defaultSiteSettings();
  }
}

export function invalidateSiteSettingsCache(): void {
  cached = null;
}

export async function isFeatureEnabled(
  flag: SiteFlag,
  db?: SettingsDb | null,
): Promise<boolean> {
  const settings = await getSiteSettings(db === undefined ? defaultDb() : db);
  return settings.flags[flag];
}

/** The 503 a switched-off path answers with. Clients show `error`. */
export function featurePausedResponse(flag: SiteFlag, req?: Request): Response {
  return Response.json(
    {
      error: SITE_FLAG_INFO[flag].pausedMessage,
      code: FEATURE_PAUSED,
      feature: flag,
    },
    { status: 503, headers: getCorsHeaders(req) },
  );
}

/**
 * Null while `flag` is on; the 503 FEATURE_PAUSED to return while it is off.
 * Ask it before auth: a paused feature is paused for everyone, and the
 * answer says nothing about the caller.
 */
export async function featurePaused(
  flag: SiteFlag,
  req?: Request,
  db?: SettingsDb | null,
): Promise<Response | null> {
  return (await isFeatureEnabled(flag, db))
    ? null
    : featurePausedResponse(flag, req);
}

/** What every client is sent: the switches and the live announcement. */
export async function publicSiteConfig(
  db?: SettingsDb | null,
  now: number = Date.now(),
): Promise<PublicSiteConfig> {
  const settings = await getSiteSettings(db === undefined ? defaultDb() : db);
  return {
    flags: settings.flags,
    announcement: activeAnnouncement(settings.announcement, now),
  };
}

/**
 * The desktop native handoff switch (GET /api/desktop/native-handoff). Fails
 * closed, unlike the kill switches: only a read that worked within
 * SITE_SETTINGS_TTL_MS counts. A failed read, no D1, or only an older copy
 * gives the default (off), so the Tauri app stays put.
 */
export async function nativeHandoffConfig(
  db?: SettingsDb | null,
  now: number = Date.now(),
): Promise<NativeHandoffConfig> {
  if (cached && now - cached.at < SITE_SETTINGS_TTL_MS) {
    return cached.settings.nativeHandoff;
  }
  const source = db === undefined ? defaultDb() : db;
  if (!source) return { ...DEFAULT_NATIVE_HANDOFF };
  try {
    const settings = await readSiteSettings(source);
    cached = { at: now, settings };
    return settings.nativeHandoff;
  } catch (error) {
    console.warn(
      `native handoff switch unavailable, answering off: ${error instanceof Error ? error.message : String(error)}`,
    );
    return { ...DEFAULT_NATIVE_HANDOFF };
  }
}
