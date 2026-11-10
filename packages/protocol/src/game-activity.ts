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

/** Deadlock's phases as the desktop app reads them from the game's log. */
export const DEADLOCK_LIVE_PHASES = [
  "menu",
  "hideout",
  "queue",
  "match_intro",
  "in_match",
  "post_match",
  "spectating",
] as const;

export const DEADLOCK_LIVE_MODES = [
  "ranked",
  "unranked",
  "brawl",
  "bots",
  "private",
  "other",
] as const;

/** Most party members a live state may name (Deadlock parties are 6). */
export const DEADLOCK_PARTY_MAX = 5;

const AccountId = Schema.Finite.check(
  Schema.isInt(),
  Schema.isGreaterThan(0),
  Schema.isLessThanOrEqualTo(0xffffffff),
);
const UserIdText = Schema.String.check(
  Schema.isPattern(/^[A-Za-z0-9_-]{1,64}$/),
);

/**
 * A structured live state for a game Yap understands, so the profile widget
 * renders it without parsing `details`/`state` text. Deadlock only for now;
 * the desktop app fills it from Deadlock's own console.log (`-condebug`).
 *
 * Two fields only go one way. A client sends `account_id` (the Steam
 * account the log says is playing) and `party` (the party's account ids);
 * the gateway checks `account_id` against the user's linked Steam account,
 * maps `party` to Yap users who linked theirs, and replaces both with
 * `party_user_ids` before anything is stored or sent. Viewers never see an
 * account id. A `party_user_ids` sent by a client is discarded.
 */
export const DeadlockLiveState = Schema.Struct({
  kind: Schema.Literal("deadlock"),
  phase: Schema.Literals(DEADLOCK_LIVE_PHASES),
  /** The hero's class name without its prefix: `haze` for `hero_haze`. */
  hero: Schema.optional(
    Schema.String.check(Schema.isPattern(/^[a-z0-9_]{1,40}$/)),
  ),
  mode: Schema.optional(Schema.Literals(DEADLOCK_LIVE_MODES)),
  /** Epoch ms the current phase began (the match clock in a match). */
  phase_started_at: Schema.optional(
    Schema.Finite.check(Schema.isGreaterThanOrEqualTo(0)),
  ),
  account_id: Schema.optional(AccountId),
  party: Schema.optional(
    Schema.Array(AccountId).check(Schema.isMaxLength(DEADLOCK_PARTY_MAX)),
  ),
  party_user_ids: Schema.optional(
    Schema.Array(UserIdText).check(Schema.isMaxLength(DEADLOCK_PARTY_MAX)),
  ),
});

export type DeadlockLiveState = typeof DeadlockLiveState.Type;

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
  /** Structured live state (see DeadlockLiveState). */
  game: Schema.optional(DeadlockLiveState),
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
  if (value.game !== undefined) activity.game = pickLiveState(value.game);
  return activity;
}

/** Copies the declared fields of a checked live state, nothing else. */
export function pickLiveState(value: DeadlockLiveState): DeadlockLiveState {
  const live: {
    -readonly [K in keyof DeadlockLiveState]: DeadlockLiveState[K];
  } = { kind: "deadlock", phase: value.phase };
  if (value.hero !== undefined) live.hero = value.hero;
  if (value.mode !== undefined) live.mode = value.mode;
  if (value.phase_started_at !== undefined)
    live.phase_started_at = Math.floor(value.phase_started_at);
  if (value.account_id !== undefined) live.account_id = value.account_id;
  if (value.party !== undefined) live.party = [...value.party];
  if (value.party_user_ids !== undefined)
    live.party_user_ids = [...value.party_user_ids];
  return live;
}
