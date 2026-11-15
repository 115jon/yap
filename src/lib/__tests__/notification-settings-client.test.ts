import { describe, expect, it } from "vitest";

import { chatReducer, initialState } from "@/lib/chat-reducer";
import { getUnreadChannelState } from "@/lib/desktop-notifications";
import type { NotificationSetting } from "@/lib/notification-levels";
import {
  applyNotificationSettingsChange,
  createNotificationMuteLookup,
  effectiveSettingsFor,
  formatMutedUntil,
  mutedChannelPredicate,
  parseNotificationSetting,
  silencesChannelSounds,
  toNotificationSettingsMap,
} from "@/lib/notification-settings-client";

const NOW = Date.parse("2026-11-14T12:00:00.000Z");

function setting(
  partial: Partial<NotificationSetting> &
    Pick<NotificationSetting, "target_type" | "target_id">,
): NotificationSetting {
  return { level: null, muted: false, muted_until: null, ...partial };
}

describe("parseNotificationSetting", () => {
  it("reads an API row", () => {
    expect(
      parseNotificationSetting({
        target_type: "server",
        target_id: "s1",
        level: "all",
        muted: true,
        muted_until: "2026-11-14T13:00:00.000Z",
      }),
    ).toEqual({
      target_type: "server",
      target_id: "s1",
      level: "all",
      muted: true,
      muted_until: "2026-11-14T13:00:00.000Z",
    });
  });

  it("drops malformed payloads", () => {
    expect(parseNotificationSetting(null)).toBeNull();
    expect(
      parseNotificationSetting({ target_type: "user", target_id: "x" }),
    ).toBeNull();
    expect(
      parseNotificationSetting({
        target_type: "channel",
        target_id: "c1",
        level: "loud",
      }),
    ).toBeNull();
  });
});

describe("mute lookups (the unread rule)", () => {
  const settings = toNotificationSettingsMap([
    setting({ target_type: "server", target_id: "s-muted", muted: true }),
    setting({ target_type: "channel", target_id: "c-muted", muted: true }),
    setting({
      target_type: "channel",
      target_id: "c-expired",
      muted: true,
      muted_until: "2026-11-14T11:00:00.000Z",
    }),
    setting({
      target_type: "channel",
      target_id: "c-timed",
      muted: true,
      muted_until: "2026-11-14T13:00:00.000Z",
    }),
    setting({ target_type: "channel", target_id: "dm-muted", muted: true }),
  ]);
  const mutes = createNotificationMuteLookup(settings, NOW);

  it("dims a muted channel and every channel of a muted server", () => {
    expect(mutes.isChannelMuted("c-muted", "s1")).toBe(true);
    expect(mutes.isChannelMuted("c-any", "s-muted")).toBe(true);
    expect(mutes.isChannelMuted("c-any", "s1")).toBe(false);
    expect(mutes.isServerMuted("s-muted")).toBe(true);
    expect(mutes.isServerMuted("s1")).toBe(false);
  });

  it("treats a timed mute as over once its time passes", () => {
    expect(mutes.isChannelMuted("c-timed", "s1")).toBe(true);
    expect(mutes.isChannelMuted("c-expired", "s1")).toBe(false);
    const later = createNotificationMuteLookup(
      settings,
      Date.parse("2026-11-14T13:00:00.000Z"),
    );
    expect(later.isChannelMuted("c-timed", "s1")).toBe(false);
  });

  it("dims a muted DM, and DMs ignore server mutes", () => {
    expect(mutes.isDmMuted("dm-muted")).toBe(true);
    expect(mutes.isDmMuted("dm-open")).toBe(false);
  });

  it("keeps muted channels and DMs off the tray badge", () => {
    const isMuted = mutedChannelPredicate({
      notificationSettings: settings,
      notificationSettingsNow: NOW,
      channelsByServerId: { "s-muted": [{ id: "c-in-muted-server" }] },
    });
    const unread = getUnreadChannelState({
      lastMessageAt: {
        "c-muted": "2026-11-14T11:59:00.000Z",
        "c-in-muted-server": "2026-11-14T11:59:00.000Z",
        "c-open": "2026-11-14T11:59:00.000Z",
        "dm-muted": "2026-11-14T11:59:00.000Z",
        "dm-open": "2026-11-14T11:59:00.000Z",
      },
      readStates: {},
      dmChannelIds: ["dm-muted", "dm-open"],
      isMuted,
    });
    expect(unread.unreadServerChannelIds).toEqual(["c-open"]);
    expect(unread.unreadDmChannelIds).toEqual(["dm-open"]);
  });
});

describe("effectiveSettingsFor", () => {
  it("lets a channel inherit its server's level, then the default", () => {
    const settings = toNotificationSettingsMap([
      setting({ target_type: "server", target_id: "s1", level: "all" }),
      setting({ target_type: "channel", target_id: "c2", level: "none" }),
    ]);
    const channel = (channelId: string, serverId: string) =>
      effectiveSettingsFor(
        settings,
        { kind: "channel", channelId, serverId },
        NOW,
      ).level;
    expect(channel("c1", "s1")).toBe("all");
    expect(channel("c2", "s1")).toBe("none");
    expect(channel("c1", "s2")).toBe("mentions");
    expect(
      effectiveSettingsFor(settings, { kind: "dm", channelId: "d1" }, NOW)
        .level,
    ).toBe("all");
  });

  it("silences the open channel's sound when muted or set to Nothing", () => {
    expect(
      silencesChannelSounds({ level: "mentions", muted: true, isDm: false }),
    ).toBe(true);
    expect(
      silencesChannelSounds({ level: "none", muted: false, isDm: false }),
    ).toBe(true);
    expect(
      silencesChannelSounds({ level: "mentions", muted: false, isDm: false }),
    ).toBe(false);
  });
});

describe("applyNotificationSettingsChange", () => {
  const base = setting({
    target_type: "channel",
    target_id: "c1",
    level: "all",
  });

  it("sets a timed mute from the clock and keeps the level", () => {
    expect(
      applyNotificationSettingsChange(
        base,
        { target_type: "channel", target_id: "c1", mute: "8h" },
        NOW,
      ),
    ).toEqual({
      ...base,
      muted: true,
      muted_until: "2026-11-14T20:00:00.000Z",
    });
  });

  it("mutes until turned off, unmutes, and clears the level", () => {
    const forever = applyNotificationSettingsChange(
      base,
      { target_type: "channel", target_id: "c1", mute: "forever" },
      NOW,
    );
    expect(forever).toMatchObject({ muted: true, muted_until: null });
    expect(
      applyNotificationSettingsChange(
        forever,
        { target_type: "channel", target_id: "c1", mute: "off", level: null },
        NOW,
      ),
    ).toEqual({ ...base, level: null });
  });

  it("starts from the inherited default when nothing is stored", () => {
    expect(
      applyNotificationSettingsChange(
        undefined,
        { target_type: "server", target_id: "s1", level: "none" },
        NOW,
      ),
    ).toEqual(
      setting({ target_type: "server", target_id: "s1", level: "none" }),
    );
  });
});

describe("formatMutedUntil", () => {
  it("says nothing when not muted and Muted for an open-ended mute", () => {
    expect(formatMutedUntil(undefined, NOW)).toBeNull();
    expect(formatMutedUntil({ muted: true, muted_until: null }, NOW)).toBe(
      "Muted",
    );
  });

  it("prints the end time", () => {
    const label = formatMutedUntil(
      { muted: true, muted_until: new Date(NOW + 60_000).toISOString() },
      NOW,
      "en-US",
    );
    expect(label).toMatch(/^Muted until /);
  });
});

describe("chat reducer notification settings", () => {
  const muted = setting({
    target_type: "channel",
    target_id: "c1",
    muted: true,
  });

  it("replaces the map on load and moves the clock forward only", () => {
    let state = chatReducer(initialState, {
      type: "SET_NOTIFICATION_SETTINGS",
      settings: toNotificationSettingsMap([muted]),
      nowMs: NOW,
    });
    expect(state.notificationSettings).toEqual({ "channel:c1": muted });
    expect(state.notificationSettingsNow).toBe(NOW);
    state = chatReducer(state, {
      type: "TICK_NOTIFICATION_SETTINGS",
      nowMs: NOW - 1,
    });
    expect(state.notificationSettingsNow).toBe(NOW);
  });

  it("upserts a gateway setting and removes a rolled-back one", () => {
    let state = chatReducer(initialState, {
      type: "UPSERT_NOTIFICATION_SETTING",
      setting: muted,
      nowMs: NOW,
    });
    const unmuted = { ...muted, muted: false };
    state = chatReducer(state, {
      type: "UPSERT_NOTIFICATION_SETTING",
      setting: unmuted,
      nowMs: NOW + 5,
    });
    expect(state.notificationSettings["channel:c1"]).toEqual(unmuted);
    expect(state.notificationSettingsNow).toBe(NOW + 5);
    state = chatReducer(state, {
      type: "REMOVE_NOTIFICATION_SETTING",
      targetType: "channel",
      targetId: "c1",
      nowMs: NOW + 6,
    });
    expect(state.notificationSettings).toEqual({});
  });
});
