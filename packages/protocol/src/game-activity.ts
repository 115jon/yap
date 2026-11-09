// ── Game activity ("Playing X") ─────────────────────────────────────────────
// What a client reports with ActivityUpdate (op 43) and what PRESENCE_UPDATE
// and PRESENCE_LIST carry as `game_activity`. Phase 1 has the manual (dev)
// source; the desktop app detects games locally (phase 2) and Rich Presence
// (`source: "rpc"`, details/state/images) is a later opt-in phase.
//
// Images are always our own paths (the game icon route serves catalog icons
// from our storage), never an external URL: a client that renders an
// activity must not be made to fetch from a host a sender chose.

import { Schema } from "effect";

export const GAME_ACTIVITY_TEXT_MAX = 128;
export const GAME_ACTIVITY_URL_MAX = 256;

/** `/api/game-icons/{app id}/{icon hash}.png`: a catalog icon, from our storage. */
export const GAME_ICON_PATH_PATTERN =
  /^\/api\/game-icons\/\d{1,20}\/[a-f0-9]{32}\.png$/;

/**
 * Our own image paths an activity may carry: catalog icons, and (for Rich
 * Presence, later) per-app assets under the same prefix family.
 */
export const GAME_IMAGE_PATH_PATTERN =
  /^\/api\/game-(?:icons|assets)\/\d{1,20}\/[A-Za-z0-9_]{1,64}\.png$/;

/** The icon path for a catalog entry (Discord application id + icon hash). */
export function gameIconPath(appId: string, iconHash: string): string | null {
  const path = `/api/game-icons/${appId}/${iconHash}.png`;
  return GAME_ICON_PATH_PATTERN.test(path) ? path : null;
}

const ActivityText = Schema.String.check(
  Schema.isMaxLength(GAME_ACTIVITY_TEXT_MAX),
);
const ActivityName = Schema.String.check(
  Schema.isMinLength(1),
  Schema.isMaxLength(GAME_ACTIVITY_TEXT_MAX),
  Schema.makeFilter((name) => name.trim().length > 0),
);
const OwnImagePath = Schema.String.check(
  Schema.isMaxLength(GAME_ACTIVITY_URL_MAX),
  Schema.isPattern(GAME_IMAGE_PATH_PATTERN),
);
const IconPath = Schema.String.check(
  Schema.isMaxLength(GAME_ACTIVITY_URL_MAX),
  Schema.isPattern(GAME_ICON_PATH_PATTERN),
);

export const GameActivitySource = Schema.Literals([
  "detected",
  "rpc",
  "manual",
]);

export const GameActivity = Schema.Struct({
  kind: Schema.Literal("playing"),
  name: ActivityName,
  /** Catalog id (a Discord application id, as a decimal string). */
  app_id: Schema.optional(Schema.String.check(Schema.isPattern(/^\d{1,20}$/))),
  steam_app_id: Schema.optional(
    Schema.String.check(Schema.isPattern(/^\d{1,10}$/)),
  ),
  icon_url: Schema.optional(IconPath),
  /** Epoch ms the game started; the gateway clamps it to the present. */
  started_at: Schema.Finite.check(Schema.isGreaterThanOrEqualTo(0)),
  details: Schema.optional(ActivityText),
  state: Schema.optional(ActivityText),
  large_image: Schema.optional(OwnImagePath),
  small_image: Schema.optional(OwnImagePath),
  source: GameActivitySource,
});

export type GameActivity = typeof GameActivity.Type;
export type GameActivitySource = typeof GameActivitySource.Type;
/** A GameActivity being built field by field. */
export type MutableGameActivity = {
  -readonly [K in keyof GameActivity]: GameActivity[K];
};

const isGameActivity = Schema.is(GameActivity);

/**
 * A received `game_activity` value: the activity rebuilt from the declared
 * fields only (extra keys dropped), `null` for an explicit clear, or
 * `undefined` when the value is absent or invalid.
 */
export function parseGameActivity(
  value: unknown,
): GameActivity | null | undefined {
  if (value === null) return null;
  if (!isGameActivity(value)) return undefined;
  return pickGameActivity(value);
}

/** Copies the declared fields of a checked activity, nothing else. */
export function pickGameActivity(value: GameActivity): GameActivity {
  const activity: MutableGameActivity = {
    kind: "playing",
    name: value.name.trim(),
    started_at: Math.floor(value.started_at),
    source: value.source,
  };
  if (value.app_id !== undefined) activity.app_id = value.app_id;
  if (value.steam_app_id !== undefined)
    activity.steam_app_id = value.steam_app_id;
  if (value.icon_url !== undefined) activity.icon_url = value.icon_url;
  if (value.details !== undefined) activity.details = value.details;
  if (value.state !== undefined) activity.state = value.state;
  if (value.large_image !== undefined) activity.large_image = value.large_image;
  if (value.small_image !== undefined) activity.small_image = value.small_image;
  return activity;
}
