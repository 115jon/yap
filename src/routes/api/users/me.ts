import { createFileRoute } from "@tanstack/react-router";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { ensureUserProfileSchema } from "@/lib/ensure-user-profile-schema";
import { getCurrentUser } from "@/lib/kova-auth-server";
import { DEFAULT_MEDIA_CONTENT_FILTER } from "@/lib/media-content-filter";
import {
  applyProfileThemeDefaults,
  type DisplayNameStyle,
} from "@/lib/profile-customization";
import { normalizeUsernameForStorage } from "@/lib/validations";
import { ServiceError } from "@/lib/service-error";
import { lenientNumber } from "@/lib/lenient-schema";
import { looseStruct, safeParse, safeParseJson } from "@/lib/schema";
import { Schema } from "effect";
import { getMe } from "@/services/user.service";
import { clog } from "@/lib/console-logger";
import type { AvatarDisplay } from "@/lib/avatar-display";
import {
  normalizeSoundboardTriggerMap,
  normalizeSoundboardTriggerConfig,
  validatePersistedSoundboardTriggerMap,
  type SoundboardTriggerMap,
} from "@/lib/voice/soundboard-trigger";

const log = clog("users/me");

type UserProfileRow = {
  id: string;
  username: string;
  display_name: string | null;
  avatar_url: string | null;
  avatar_display: AvatarDisplay | null;
  banner_url: string | null;
  banner_content_type: string | null;
  nameplate_url: string | null;
  nameplate_content_type: string | null;
  profile_accent_color: string | null;
  profile_background_color: string | null;
  profile_banner_color: string | null;
  display_name_style: DisplayNameStyle | string | null;
  theme_preference: string | null;
  theme_sync_enabled: number;
  media_content_filter: string;
  share_activity?: boolean;
  updated_at: string | null;
  created_at: string | null;
  bio: string | null;
  pronouns: string | null;
  status: string;
  custom_status: string | null;
};

export const GET = async ({ request }: any) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;

  const db = getDB();
  await ensureUserProfileSchema(db);

  try {
    const user = await getMe(db, userId);
    await backfillMissingAvatar(db, user, request.headers);
    const settingsRow = await db
      .prepare("SELECT sound_settings FROM users WHERE id = ?")
      .bind(userId)
      .first<{ sound_settings: string | null }>();
    return apiSuccess({
      ...user,
      sound_settings: parseStoredSoundSettings(settingsRow?.sound_settings),
    });
  } catch (e) {
    if (e instanceof ServiceError && e.status === 404) {
      try {
        const synced = await syncUserFromRalphAuth(db, userId, request.headers);
        if (synced) return apiSuccess(synced);
      } catch (syncErr) {
        log.error("Auto-sync from Ralph Auth failed:", syncErr);
      }
      return apiError(
        "User profile not found. Please sign out and sign back in.",
        404,
      );
    }
    throw e;
  }
};

// Envelope only; each field is validated against the caller's servers below.
const soundSettingsPatchSchema = Schema.Struct({
  sound_settings: Schema.Record(Schema.String, Schema.Unknown),
});

const soundboardVolumeSchema = Schema.Finite.check(
  Schema.isBetween({ minimum: 0, maximum: 100 }),
);

export const PATCH = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;
  const body = safeParse(
    soundSettingsPatchSchema,
    await request.json().catch(() => null),
  );
  if (!body.success) {
    return apiError("Invalid sound settings", 400, "INVALID_SOUND_SETTINGS");
  }
  const raw = body.data.sound_settings;

  const db = getDB();
  await ensureUserProfileSchema(db);
  const currentRow = await db
    .prepare("SELECT sound_settings FROM users WHERE id = ?")
    .bind(userId)
    .first<{ sound_settings: string | null }>();
  const current = parseStoredSoundSettingsRecord(currentRow?.sound_settings);
  const requestedFields = [
    "voiceJoinSoundboard",
    "voiceLeaveSoundboard",
    "soundboardVolume",
  ] as const;
  const changedFields = requestedFields.filter((field) => field in raw);
  if (changedFields.length === 0) {
    return apiError(
      "No trigger settings supplied",
      400,
      "INVALID_SOUND_SETTINGS",
    );
  }

  const resolveServerSound = async (serverId: string, soundId: string) => {
    const row = await db
      .prepare(
        `SELECT a.id, a.soundboard_server_id AS server_id,
                a.sound_name, a.sound_emoji,
                COALESCE(a.sound_volume, 1.0) AS sound_volume
         FROM attachments a
         JOIN server_members sm
           ON sm.server_id = a.soundboard_server_id AND sm.user_id = ?
         WHERE a.soundboard_server_id = ? AND a.id = ?
         LIMIT 1`,
      )
      .bind(userId, serverId, soundId)
      .first<{
        id: string;
        server_id: string;
        sound_name: string | null;
        sound_emoji: string | null;
        sound_volume: number;
      }>();
    if (!row) return null;
    return {
      id: row.id,
      serverId: row.server_id,
      name: row.sound_name || "Sound",
      ...(row.sound_emoji ? { emoji: row.sound_emoji } : {}),
      volume: Number(row.sound_volume),
    };
  };

  const validated: Partial<
    Record<(typeof requestedFields)[number], SoundboardTriggerMap | number>
  > = {};
  for (const field of changedFields) {
    if (field === "soundboardVolume") {
      const volume = safeParse(soundboardVolumeSchema, raw[field]);
      if (!volume.success) {
        return apiError(
          "Soundboard volume is invalid",
          400,
          "INVALID_SOUND_SETTINGS",
        );
      }
      validated[field] = volume.data;
      continue;
    }
    const value = await validatePersistedSoundboardTriggerMap(
      raw[field],
      resolveServerSound,
    );
    if (!value) {
      return apiError(
        "Soundboard trigger selection is invalid or unavailable",
        400,
        "INVALID_SOUND_SELECTION",
      );
    }
    validated[field] = value;
  }

  const soundSettings = JSON.stringify({ ...current, ...validated });
  await db
    .prepare("UPDATE users SET sound_settings = ?, updated_at = ? WHERE id = ?")
    .bind(soundSettings, new Date().toISOString(), userId)
    .run();

  return apiSuccess({ sound_settings: validated });
};

/**
 * What `users.sound_settings` holds. Tolerant per field: a corrupt field decodes
 * to `undefined` (and so to its default), never to a throw. Other keys are kept
 * so a PATCH merges into whatever is stored.
 */
const StoredSoundSettings = looseStruct({
  voiceJoinSoundboard: Schema.optional(Schema.Unknown),
  voiceLeaveSoundboard: Schema.optional(Schema.Unknown),
  soundboardVolume: lenientNumber,
});

function decodeStoredSoundSettings(value: string | null | undefined) {
  if (!value) return undefined;
  const parsed = safeParseJson(StoredSoundSettings, value);
  return parsed.success ? parsed.data : undefined;
}

function parseStoredSoundSettingsRecord(
  value: string | null | undefined,
): Record<string, unknown> {
  const stored = decodeStoredSoundSettings(value);
  if (!stored) return {};

  const sanitized = { ...stored };
  if ("voiceJoinSoundboard" in stored) {
    sanitized.voiceJoinSoundboard = parseStoredTriggerMap(
      stored.voiceJoinSoundboard,
    );
  }
  if ("voiceLeaveSoundboard" in stored) {
    sanitized.voiceLeaveSoundboard = parseStoredTriggerMap(
      stored.voiceLeaveSoundboard,
    );
  }
  return sanitized;
}

function parseStoredSoundSettings(value: string | null | undefined): {
  voiceJoinSoundboard: SoundboardTriggerMap;
  voiceLeaveSoundboard: SoundboardTriggerMap;
  soundboardVolume: number;
} {
  const stored = decodeStoredSoundSettings(value);
  return {
    voiceJoinSoundboard: parseStoredTriggerMap(stored?.voiceJoinSoundboard),
    voiceLeaveSoundboard: parseStoredTriggerMap(stored?.voiceLeaveSoundboard),
    soundboardVolume:
      stored?.soundboardVolume === undefined
        ? 100
        : Math.max(0, Math.min(100, stored.soundboardVolume)),
  };
}

function parseStoredTriggerMap(value: unknown): SoundboardTriggerMap {
  const normalized = normalizeSoundboardTriggerMap(value);
  return Object.fromEntries(
    Object.entries(normalized).map(([scope, config]) => {
      const normalizedConfig = normalizeSoundboardTriggerConfig(config);
      if (!normalizedConfig.sound) return [scope, normalizedConfig];
      const { mediaUrl: _mediaUrl, ...sound } = normalizedConfig.sound;
      return [scope, { enabled: normalizedConfig.enabled, sound }];
    }),
  );
}

async function backfillMissingAvatar(
  db: any,
  user: UserProfileRow,
  headers: Headers,
) {
  if (user.avatar_url) return;

  try {
    const authUser = await getCurrentUser(headers);
    const avatarUrl = authUser?.imageUrl ?? authUser?.image ?? null;
    if (!avatarUrl) return;

    await db
      .prepare(
        `UPDATE users SET avatar_url = ? WHERE id = ? AND (avatar_url IS NULL OR avatar_url = '')`,
      )
      .bind(avatarUrl, user.id)
      .run();
    user.avatar_url = avatarUrl;
  } catch {
    // Avatar backfill is best-effort; a missing avatar should not block /me.
  }
}

async function syncUserFromRalphAuth(
  db: any,
  userId: string,
  headers: Headers,
): Promise<UserProfileRow | null> {
  const authUser = await getCurrentUser(headers);
  if (!authUser) return null;

  const email =
    authUser.email ?? authUser.primaryEmailAddress?.emailAddress ?? null;
  const preferredUsername = normalizeUsernameForStorage(
    authUser.username ??
      (email ? email.split("@")[0] : null) ??
      `user_${userId.slice(-6)}`,
    `user_${userId.slice(-6)}`,
  );
  // Usernames are unique, case-insensitively. A name another account already
  // holds never transfers; this sign-in gets its own, suffixed with its id.
  const nameTaken = await db
    .prepare(
      "SELECT 1 AS taken FROM users WHERE lower(username) = ? AND id != ? LIMIT 1",
    )
    .bind(preferredUsername, userId)
    .first();
  const username = nameTaken
    ? normalizeUsernameForStorage(
        `${preferredUsername.slice(0, 25)}_${userId.slice(-6)}`,
        `user_${userId.slice(-6)}`,
      )
    : preferredUsername;
  const fullName = [authUser.firstName, authUser.lastName]
    .filter(Boolean)
    .join(" ");
  const displayName = authUser.name ?? (fullName || username);
  const avatarUrl = authUser.imageUrl ?? authUser.image ?? null;
  const bio = authUser.bio ?? null;
  const now = new Date().toISOString();
  const defaultProfileTheme = applyProfileThemeDefaults({});

  await db
    .prepare(
      `INSERT INTO users (id, username, display_name, avatar_url, bio, status, profile_accent_color, profile_background_color, created_at)
       VALUES (?, ?, ?, ?, ?, 'online', ?, ?, ?)
       ON CONFLICT(id) DO UPDATE SET
         username = excluded.username,
         display_name = CASE
           WHEN users.display_name IS NOT NULL AND users.display_name != '' THEN users.display_name
           ELSE excluded.display_name
         END,
         avatar_url = CASE
           WHEN users.avatar_url LIKE '/api/avatars/%' THEN users.avatar_url
           ELSE excluded.avatar_url
         END,
         bio = COALESCE(users.bio, excluded.bio),
         profile_accent_color = COALESCE(users.profile_accent_color, excluded.profile_accent_color),
         profile_background_color = COALESCE(users.profile_background_color, excluded.profile_background_color)`,
    )
    .bind(
      userId,
      username,
      displayName,
      avatarUrl,
      bio,
      defaultProfileTheme.profile_accent_color,
      defaultProfileTheme.profile_background_color,
      now,
    )
    .run();

  return {
    id: userId,
    username,
    display_name: displayName,
    avatar_url: avatarUrl,
    avatar_display: null,
    banner_url: null,
    banner_content_type: null,
    nameplate_url: null,
    nameplate_content_type: null,
    profile_accent_color: defaultProfileTheme.profile_accent_color,
    profile_background_color: defaultProfileTheme.profile_background_color,
    profile_banner_color: null,
    display_name_style: null,
    theme_preference: null,
    theme_sync_enabled: 0,
    media_content_filter: DEFAULT_MEDIA_CONTENT_FILTER,
    share_activity: true,
    updated_at: now,
    created_at: now,
    bio,
    pronouns: null,
    status: "online",
    custom_status: null,
  };
}

export const Route = createFileRoute("/api/users/me")({
  server: {
    handlers: {
      GET,
      PATCH,
    },
  },
});
