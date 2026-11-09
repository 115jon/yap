/**
 * Profile prefs: the owner's Activity settings (retention, kinds hidden
 * from others), the guestbook switch and the wishlist. One row per user in
 * `profile_prefs`; no row is the defaults.
 */

import type { D1Database } from "@cloudflare/workers-types";

import {
  DEFAULT_ACTIVITY_SETTINGS,
  parseActivitySettings,
  parseWishlist,
  type ActivitySettings,
  type ProfilePrefs,
  type WishlistItem,
} from "@/lib/profile-board";

interface PrefsRow {
  activity_retention_days: number;
  activity_hidden_kinds: string;
  guestbook_enabled: number;
  wishlist: string;
}

const DEFAULT_PREFS: ProfilePrefs = {
  activity: DEFAULT_ACTIVITY_SETTINGS,
  guestbook_enabled: true,
};

async function readRow(
  db: D1Database,
  userId: string,
): Promise<PrefsRow | null> {
  return db
    .prepare(
      `SELECT activity_retention_days, activity_hidden_kinds, guestbook_enabled, wishlist
       FROM profile_prefs WHERE user_id = ?`,
    )
    .bind(userId)
    .first<PrefsRow>();
}

function prefsFromRow(row: PrefsRow | null): ProfilePrefs {
  if (!row) return DEFAULT_PREFS;
  return {
    activity: parseActivitySettings({
      retention_days: row.activity_retention_days,
      hidden_kinds: safeParse(row.activity_hidden_kinds),
    }),
    guestbook_enabled: row.guestbook_enabled !== 0,
  };
}

function safeParse(value: string | null): unknown {
  if (!value) return null;
  try {
    return JSON.parse(value);
  } catch {
    return null;
  }
}

export async function getProfilePrefs(
  db: D1Database,
  userId: string,
): Promise<ProfilePrefs> {
  return prefsFromRow(await readRow(db, userId));
}

export interface ProfilePrefsPatch {
  activity?: Partial<ActivitySettings>;
  guestbook_enabled?: boolean;
}

/** Merges `patch` into the owner's prefs and returns the result. */
export async function updateProfilePrefs(
  db: D1Database,
  userId: string,
  patch: ProfilePrefsPatch,
  now: string = new Date().toISOString(),
): Promise<ProfilePrefs> {
  const current = await getProfilePrefs(db, userId);
  const activity = parseActivitySettings({
    ...current.activity,
    ...(patch.activity ?? {}),
  });
  const guestbookEnabled =
    typeof patch.guestbook_enabled === "boolean"
      ? patch.guestbook_enabled
      : current.guestbook_enabled;
  await db
    .prepare(
      `INSERT INTO profile_prefs
         (user_id, activity_retention_days, activity_hidden_kinds, guestbook_enabled, updated_at)
       VALUES (?, ?, ?, ?, ?)
       ON CONFLICT(user_id) DO UPDATE SET
         activity_retention_days = excluded.activity_retention_days,
         activity_hidden_kinds = excluded.activity_hidden_kinds,
         guestbook_enabled = excluded.guestbook_enabled,
         updated_at = excluded.updated_at`,
    )
    .bind(
      userId,
      activity.retention_days,
      JSON.stringify(activity.hidden_kinds),
      guestbookEnabled ? 1 : 0,
      now,
    )
    .run();
  return { activity, guestbook_enabled: guestbookEnabled };
}

export async function getWishlist(
  db: D1Database,
  userId: string,
): Promise<WishlistItem[]> {
  const row = await readRow(db, userId);
  return row ? parseWishlist(row.wishlist) : [];
}

/** Replaces the wishlist with `items`, cleaned. Returns what was stored. */
export async function setWishlist(
  db: D1Database,
  userId: string,
  items: unknown,
  now: string = new Date().toISOString(),
): Promise<WishlistItem[]> {
  const wishlist = parseWishlist(items);
  await db
    .prepare(
      `INSERT INTO profile_prefs (user_id, wishlist, updated_at)
       VALUES (?, ?, ?)
       ON CONFLICT(user_id) DO UPDATE SET
         wishlist = excluded.wishlist,
         updated_at = excluded.updated_at`,
    )
    .bind(userId, JSON.stringify(wishlist), now)
    .run();
  return wishlist;
}
