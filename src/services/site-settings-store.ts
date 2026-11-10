/**
 * The site settings row store: D1 `site_settings` read and written as plain
 * statements (shapes in lib/site-flags.ts and lib/native-handoff.ts). No caching and no request
 * helpers, so the main Worker (services/site-flags.service.ts, which caches
 * and enforces) and the admin Worker (apps/admin, which writes) share it.
 */

import {
  parseNativeHandoff,
  type NativeHandoffConfig,
} from "@/lib/native-handoff";
import {
  parseAnnouncement,
  parseSiteFlags,
  type SiteAnnouncement,
  type SiteFlags,
} from "@/lib/site-flags";

export interface SiteSettings {
  flags: SiteFlags;
  announcement: SiteAnnouncement | null;
  /** The Tauri app's handoff to the native app (off by default). */
  nativeHandoff: NativeHandoffConfig;
}

export const SITE_SETTING_KEYS = [
  "flags",
  "announcement",
  "nativeHandoff",
] as const;
export type SiteSettingKey = (typeof SITE_SETTING_KEYS)[number];

/** The settings when nothing is stored (or D1 cannot be read). */
export function defaultSiteSettings(): SiteSettings {
  return {
    flags: parseSiteFlags(null),
    announcement: null,
    nativeHandoff: parseNativeHandoff(null),
  };
}

function parseJson(value: string | null | undefined): unknown {
  if (!value) return null;
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

/** Reads the settings straight from D1 (no cache). */
export async function readSiteSettings(
  db: Pick<D1Database, "prepare">,
): Promise<SiteSettings> {
  const { results } = await db
    .prepare(
      `SELECT key, value FROM site_settings WHERE key IN ('flags', 'announcement', 'nativeHandoff')`,
    )
    .all<{ key: string; value: string }>();
  const byKey = new Map((results ?? []).map((row) => [row.key, row.value]));
  return {
    flags: parseSiteFlags(parseJson(byKey.get("flags"))),
    announcement: parseAnnouncement(parseJson(byKey.get("announcement"))),
    nativeHandoff: parseNativeHandoff(parseJson(byKey.get("nativeHandoff"))),
  };
}

/** The upsert for one settings key (batched with its audit entry). */
export function siteSettingStatement(
  db: D1Database,
  key: SiteSettingKey,
  value: unknown,
  actorId: string,
  now: string,
): D1PreparedStatement {
  return db
    .prepare(
      `INSERT INTO site_settings (key, value, updated_at, updated_by)
       VALUES (?, ?, ?, ?)
       ON CONFLICT(key) DO UPDATE SET
         value = excluded.value,
         updated_at = excluded.updated_at,
         updated_by = excluded.updated_by`,
    )
    .bind(key, JSON.stringify(value), now, actorId);
}

export function deleteSiteSettingStatement(
  db: D1Database,
  key: SiteSettingKey,
): D1PreparedStatement {
  return db.prepare(`DELETE FROM site_settings WHERE key = ?`).bind(key);
}
