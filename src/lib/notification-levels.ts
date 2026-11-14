// ── notification-levels ──────────────────────────────────────────────────────
// Per-server, per-channel and per-DM notification levels and mutes, the rules
// that turn them into "does this message notify me", and the rule for unread
// badges. The server (push and inbox), the web client and the Qt apps
// (native/rust/crates/yap-core/src/notification_levels.rs mirrors this file)
// all decide the same way.
//
// Levels, as in Discord:
//   all      every message notifies
//   mentions only @mentions and replies to you notify
//   none     nothing notifies, not even mentions
//
// A channel without its own level takes its server's, and a server without
// one takes the default: mentions for server channels, all for DMs (what Yap
// did before levels existed).
//
// Mute (for 1h, 8h, 24h, or until turned off) silences a server, a channel or
// a DM. In a server, a muted channel (or any channel of a muted server) still
// notifies for mentions and replies unless its level is none, as Discord's
// mute does; plain messages stay silent whatever the level. A muted DM is
// silent altogether. Muted channels show no unread badge and render dimmed;
// mention badges still show, because those notifications still arrive.

export type NotificationLevel = "all" | "mentions" | "none";
export type NotificationTargetType = "server" | "channel";

export const NOTIFICATION_LEVELS: readonly NotificationLevel[] = [
  "all",
  "mentions",
  "none",
];

/** The mute choices the UI offers; "forever" is "until I turn it back on". */
export type MuteDuration = "1h" | "8h" | "24h" | "forever";

export const MUTE_DURATION_MS: Record<
  Exclude<MuteDuration, "forever">,
  number
> = {
  "1h": 60 * 60 * 1000,
  "8h": 8 * 60 * 60 * 1000,
  "24h": 24 * 60 * 60 * 1000,
};

/** One stored setting, as the API returns it and the gateway broadcasts it. */
export interface NotificationSetting {
  target_type: NotificationTargetType;
  target_id: string;
  /** null = inherit (the server's level, then the default). */
  level: NotificationLevel | null;
  muted: boolean;
  /** ISO time the mute ends; null while muted = until turned off. */
  muted_until: string | null;
}

export function isNotificationLevel(
  value: unknown,
): value is NotificationLevel {
  return value === "all" || value === "mentions" || value === "none";
}

/** Whether `setting` is muted at `nowMs` (a timed mute that ran out is not). */
export function isMuteActive(
  setting:
    | Pick<NotificationSetting, "muted" | "muted_until">
    | null
    | undefined,
  nowMs: number,
): boolean {
  if (!setting?.muted) return false;
  if (setting.muted_until === null) return true;
  const until = Date.parse(setting.muted_until);
  return Number.isNaN(until) || until > nowMs;
}

/** The setting a target resolves to after inheritance. */
export interface EffectiveNotificationSettings {
  level: NotificationLevel;
  muted: boolean;
  isDm: boolean;
}

export function resolveNotificationSettings(args: {
  isDm: boolean;
  channel: NotificationSetting | null | undefined;
  server: NotificationSetting | null | undefined;
  nowMs: number;
}): EffectiveNotificationSettings {
  const { isDm, channel, nowMs } = args;
  const server = isDm ? null : args.server;
  const level = channel?.level ?? server?.level ?? (isDm ? "all" : "mentions");
  const muted = isMuteActive(channel, nowMs) || isMuteActive(server, nowMs);
  return { level, muted, isDm };
}

/**
 * Whether a message notifies (inbox entry and push): `addressed` is true for
 * an @mention of the user or a reply to their message.
 */
export function shouldNotify(
  effective: EffectiveNotificationSettings,
  addressed: boolean,
): boolean {
  if (effective.level === "none") return false;
  if (effective.isDm) {
    if (effective.muted) return false;
    return effective.level === "all" || addressed;
  }
  if (addressed) return true;
  return effective.level === "all" && !effective.muted;
}

/** Whether the channel's unread badge shows (muted channels show none). */
export function showsUnreadBadge(
  effective: Pick<EffectiveNotificationSettings, "muted">,
): boolean {
  return !effective.muted;
}

/** Map key for a setting: `server:<id>` or `channel:<id>`. */
export function notificationSettingKey(
  targetType: NotificationTargetType,
  targetId: string,
): string {
  return `${targetType}:${targetId}`;
}

/** The earliest time any timed mute in `settings` ends, or null. */
export function nextMuteExpiry(
  settings: Iterable<NotificationSetting>,
  nowMs: number,
): number | null {
  let next: number | null = null;
  for (const setting of settings) {
    if (!setting.muted || setting.muted_until === null) continue;
    const until = Date.parse(setting.muted_until);
    if (Number.isNaN(until) || until <= nowMs) continue;
    if (next === null || until < next) next = until;
  }
  return next;
}
