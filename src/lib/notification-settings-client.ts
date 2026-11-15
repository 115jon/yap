// ── notification-settings-client ─────────────────────────────────────────────
// The web client's side of notification levels and mutes: reading the API and
// gateway payloads, the optimistic value of a change, and the lookups the
// sidebars and the tray badge make. The rules themselves live in
// notification-levels.ts; this file only feeds them.

import {
  isMuteActive,
  isNotificationLevel,
  MUTE_DURATION_MS,
  notificationSettingKey,
  resolveNotificationSettings,
  showsUnreadBadge,
  type EffectiveNotificationSettings,
  type MuteDuration,
  type NotificationLevel,
  type NotificationSetting,
  type NotificationTargetType,
} from "@/lib/notification-levels";

/** Settings by `notificationSettingKey` (`server:<id>`, `channel:<id>`). */
export type NotificationSettingsMap = Record<string, NotificationSetting>;

/** What a menu acts on: a server, a server channel, or a DM. */
export type NotificationTarget =
  | { kind: "server"; serverId: string }
  | { kind: "channel"; channelId: string; serverId: string | null }
  | { kind: "dm"; channelId: string };

/** The body of PUT /api/notification-settings. */
export interface NotificationSettingsChange {
  target_type: NotificationTargetType;
  target_id: string;
  /** null = use the server's level (server channels only). */
  level?: NotificationLevel | null;
  mute?: MuteDuration | "off";
}

/** A stored setting from the API or the gateway, or null when malformed. */
export function parseNotificationSetting(
  value: unknown,
): NotificationSetting | null {
  if (typeof value !== "object" || value === null) return null;
  const raw = value as Record<string, unknown>;
  const targetType = raw.target_type;
  if (targetType !== "server" && targetType !== "channel") return null;
  if (typeof raw.target_id !== "string" || raw.target_id.length === 0) {
    return null;
  }
  const level = raw.level ?? null;
  if (level !== null && !isNotificationLevel(level)) return null;
  const mutedUntil = raw.muted_until ?? null;
  if (mutedUntil !== null && typeof mutedUntil !== "string") return null;
  return {
    target_type: targetType,
    target_id: raw.target_id,
    level,
    muted: raw.muted === true || raw.muted === 1,
    muted_until: mutedUntil,
  };
}

export function toNotificationSettingsMap(
  settings: Iterable<NotificationSetting>,
): NotificationSettingsMap {
  const map: NotificationSettingsMap = {};
  for (const setting of settings) {
    map[notificationSettingKey(setting.target_type, setting.target_id)] =
      setting;
  }
  return map;
}

export function targetTypeAndId(target: NotificationTarget): {
  targetType: NotificationTargetType;
  targetId: string;
} {
  return target.kind === "server"
    ? { targetType: "server", targetId: target.serverId }
    : { targetType: "channel", targetId: target.channelId };
}

export function settingFor(
  settings: NotificationSettingsMap,
  target: NotificationTarget,
): NotificationSetting | undefined {
  const { targetType, targetId } = targetTypeAndId(target);
  return settings[notificationSettingKey(targetType, targetId)];
}

/** The level and mute a channel, DM or server resolves to at `nowMs`. */
export function effectiveSettingsFor(
  settings: NotificationSettingsMap,
  target: NotificationTarget,
  nowMs: number,
): EffectiveNotificationSettings {
  if (target.kind === "server") {
    return resolveNotificationSettings({
      isDm: false,
      channel: null,
      server: settings[notificationSettingKey("server", target.serverId)],
      nowMs,
    });
  }
  const isDm = target.kind === "dm";
  return resolveNotificationSettings({
    isDm,
    channel: settings[notificationSettingKey("channel", target.channelId)],
    server:
      target.kind === "channel" && target.serverId
        ? settings[notificationSettingKey("server", target.serverId)]
        : null,
    nowMs,
  });
}

/**
 * Whether the open channel's own "new message" sound stays quiet: it is
 * muted (here or through its server) or set to Nothing.
 */
export function silencesChannelSounds(
  effective: EffectiveNotificationSettings,
): boolean {
  return effective.muted || effective.level === "none";
}

/**
 * The target for a channel id from the store's lists: a DM when it is one
 * of the DM channels, else a server channel (its server if known).
 */
export function targetForChannel(
  channelId: string,
  lists: {
    dmChannels: ReadonlyArray<{ id: string }>;
    channels: ReadonlyArray<{ id: string; server_id?: string | null }>;
  },
): NotificationTarget {
  if (lists.dmChannels.some((dm) => dm.id === channelId)) {
    return { kind: "dm", channelId };
  }
  const channel = lists.channels.find(
    (candidate) => candidate.id === channelId,
  );
  return { kind: "channel", channelId, serverId: channel?.server_id ?? null };
}

/** Whether the channel or DM has messages past its read marker. */
export function hasUnreadMessages(
  channelId: string,
  readStates: Record<string, string>,
  lastMessageAt: Record<string, string>,
): boolean {
  const lastMessage = lastMessageAt[channelId];
  if (!lastMessage) return false;
  const lastRead = readStates[channelId];
  if (!lastRead) return true;
  return lastMessage > lastRead;
}

/**
 * The lookups a list makes for its rows, bound to one settings snapshot and
 * one clock reading.
 */
export interface NotificationMuteLookup {
  /** A server channel (muted itself or through its server). */
  isChannelMuted: (channelId: string, serverId: string | null) => boolean;
  isDmMuted: (channelId: string) => boolean;
  isServerMuted: (serverId: string) => boolean;
}

export function createNotificationMuteLookup(
  settings: NotificationSettingsMap,
  nowMs: number,
): NotificationMuteLookup {
  return {
    isChannelMuted: (channelId, serverId) =>
      !showsUnreadBadge(
        effectiveSettingsFor(
          settings,
          { kind: "channel", channelId, serverId },
          nowMs,
        ),
      ),
    isDmMuted: (channelId) =>
      !showsUnreadBadge(
        effectiveSettingsFor(settings, { kind: "dm", channelId }, nowMs),
      ),
    isServerMuted: (serverId) =>
      isMuteActive(settings[notificationSettingKey("server", serverId)], nowMs),
  };
}

/** No settings: nothing is muted. */
export const NO_MUTES: NotificationMuteLookup = createNotificationMuteLookup(
  {},
  0,
);

/**
 * Channel id → server id for every cached server channel, so a channel id
 * alone (lastMessageAt keys) can find its server's mute.
 */
export function channelServerIndex(
  channelsByServerId: Record<string, ReadonlyArray<{ id: string }>>,
): Map<string, string> {
  const index = new Map<string, string>();
  for (const [serverId, channels] of Object.entries(channelsByServerId)) {
    for (const channel of channels) index.set(channel.id, serverId);
  }
  return index;
}

/**
 * A predicate for getUnreadChannelState: is this channel or DM muted (so its
 * unread activity should not light the tray badge)? Built from the store.
 */
export function mutedChannelPredicate(input: {
  notificationSettings: NotificationSettingsMap;
  notificationSettingsNow: number;
  channelsByServerId: Record<string, ReadonlyArray<{ id: string }>>;
  channels?: ReadonlyArray<{ id: string; server_id?: string | null }>;
}): (channelId: string, isDm: boolean) => boolean {
  const lookup = createNotificationMuteLookup(
    input.notificationSettings,
    input.notificationSettingsNow,
  );
  const serverOf = channelServerIndex(input.channelsByServerId);
  for (const channel of input.channels ?? []) {
    if (channel.server_id && !serverOf.has(channel.id)) {
      serverOf.set(channel.id, channel.server_id);
    }
  }
  return (channelId, isDm) =>
    isDm
      ? lookup.isDmMuted(channelId)
      : lookup.isChannelMuted(channelId, serverOf.get(channelId) ?? null);
}

/**
 * What the settings will be once `change` is saved, so the menu can show it
 * before the server answers. `nowMs` is the server-adjusted clock.
 */
export function applyNotificationSettingsChange(
  previous: NotificationSetting | undefined,
  change: NotificationSettingsChange,
  nowMs: number,
): NotificationSetting {
  const next: NotificationSetting = previous
    ? { ...previous }
    : {
        target_type: change.target_type,
        target_id: change.target_id,
        level: null,
        muted: false,
        muted_until: null,
      };
  if (change.level !== undefined) next.level = change.level;
  if (change.mute === "off") {
    next.muted = false;
    next.muted_until = null;
  } else if (change.mute === "forever") {
    next.muted = true;
    next.muted_until = null;
  } else if (change.mute) {
    next.muted = true;
    next.muted_until = new Date(
      nowMs + MUTE_DURATION_MS[change.mute],
    ).toISOString();
  }
  return next;
}

export const NOTIFICATION_LEVEL_LABELS: Record<NotificationLevel, string> = {
  all: "All messages",
  mentions: "Only @mentions",
  none: "Nothing",
};

export const MUTE_DURATION_LABELS: Record<MuteDuration, string> = {
  "1h": "For 1 hour",
  "8h": "For 8 hours",
  "24h": "For 24 hours",
  forever: "Until I turn it back on",
};

export const MUTE_DURATIONS: readonly MuteDuration[] = [
  "1h",
  "8h",
  "24h",
  "forever",
];

/**
 * "Muted until 3:00 PM", "Muted until Tue 3:00 PM" for a later day, or
 * "Muted" for a mute with no end. Null when not muted at `nowMs`.
 */
export function formatMutedUntil(
  setting: Pick<NotificationSetting, "muted" | "muted_until"> | undefined,
  nowMs: number,
  locale?: string,
): string | null {
  if (!isMuteActive(setting, nowMs)) return null;
  if (!setting?.muted_until) return "Muted";
  const until = new Date(setting.muted_until);
  const now = new Date(nowMs);
  const sameDay = until.toDateString() === now.toDateString();
  const time = until.toLocaleTimeString(locale, {
    hour: "numeric",
    minute: "2-digit",
  });
  if (sameDay) return `Muted until ${time}`;
  const day = until.toLocaleDateString(locale, { weekday: "short" });
  return `Muted until ${day} ${time}`;
}

/** The radio value for "Use server default" on a server channel. */
export const INHERIT_LEVEL = "inherit";
export type LevelChoice = NotificationLevel | typeof INHERIT_LEVEL;

/** What the menu shows for one target at `nowMs`. */
export interface NotificationMenuView {
  /** The checked level radio. */
  level: LevelChoice;
  /** The level a server channel inherits (its server's, or the default). */
  serverLevel: NotificationLevel;
  /** The target's own mute is on. */
  muted: boolean;
  /** "Muted until 3:00 PM" for the target's own mute, else null. */
  mutedLabel: string | null;
  /** A server channel whose server is muted: "Server muted until …". */
  serverMutedLabel: string | null;
  /** One line for the menu row: the mute, else the level. */
  summary: string;
  /** Muted here or through the server (the row's glyph). */
  effectivelyMuted: boolean;
}

export function notificationMenuView(
  settings: NotificationSettingsMap,
  target: NotificationTarget,
  nowMs: number,
): NotificationMenuView {
  const own = settingFor(settings, target);
  const server =
    target.kind === "channel" && target.serverId
      ? settings[notificationSettingKey("server", target.serverId)]
      : undefined;
  const effective = effectiveSettingsFor(settings, target, nowMs);
  const serverLevel = server?.level ?? "mentions";
  const level: LevelChoice =
    target.kind === "channel" ? (own?.level ?? INHERIT_LEVEL) : effective.level;
  const muted = isMuteActive(own, nowMs);
  const mutedLabel = formatMutedUntil(own, nowMs);
  const serverMuted = formatMutedUntil(server, nowMs);
  const serverMutedLabel = serverMuted
    ? `Server ${serverMuted.charAt(0).toLowerCase()}${serverMuted.slice(1)}`
    : null;
  return {
    level,
    serverLevel,
    muted,
    mutedLabel,
    serverMutedLabel,
    summary:
      mutedLabel ??
      serverMutedLabel ??
      NOTIFICATION_LEVEL_LABELS[effective.level],
    effectivelyMuted: effective.muted,
  };
}
