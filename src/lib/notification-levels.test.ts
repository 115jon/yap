import { describe, expect, it } from "vitest";

import {
  isMuteActive,
  nextMuteExpiry,
  type NotificationSetting,
  resolveNotificationSettings,
  shouldNotify,
  showsUnreadBadge,
} from "./notification-levels";

const NOW = Date.parse("2026-11-14T12:00:00.000Z");

function row(change: Partial<NotificationSetting> = {}): NotificationSetting {
  return {
    target_type: "channel",
    target_id: "c1",
    level: null,
    muted: false,
    muted_until: null,
    ...change,
  };
}

describe("isMuteActive", () => {
  it("reads a timed mute against now, and null as until turned off", () => {
    expect(isMuteActive(row({ muted: true }), NOW)).toBe(true);
    expect(
      isMuteActive(
        row({ muted: true, muted_until: "2026-11-14T12:00:01Z" }),
        NOW,
      ),
    ).toBe(true);
    expect(
      isMuteActive(
        row({ muted: true, muted_until: "2026-11-14T12:00:00Z" }),
        NOW,
      ),
    ).toBe(false);
    expect(isMuteActive(row(), NOW)).toBe(false);
    expect(isMuteActive(null, NOW)).toBe(false);
  });
});

describe("resolveNotificationSettings", () => {
  it("defaults to mentions in servers and all in DMs", () => {
    const args = { channel: null, server: null, nowMs: NOW };
    expect(resolveNotificationSettings({ ...args, isDm: false }).level).toBe(
      "mentions",
    );
    expect(resolveNotificationSettings({ ...args, isDm: true }).level).toBe(
      "all",
    );
  });

  it("prefers the channel's level, then the server's; either mute counts", () => {
    const server = row({ target_type: "server", level: "none", muted: true });
    const effective = resolveNotificationSettings({
      isDm: false,
      channel: row({ level: "all" }),
      server,
      nowMs: NOW,
    });
    expect(effective).toEqual({ level: "all", muted: true, isDm: false });
    expect(
      resolveNotificationSettings({
        isDm: false,
        channel: null,
        server,
        nowMs: NOW,
      }).level,
    ).toBe("none");
  });

  it("ignores a server row for a DM", () => {
    const effective = resolveNotificationSettings({
      isDm: true,
      channel: null,
      server: row({ target_type: "server", level: "none", muted: true }),
      nowMs: NOW,
    });
    expect(effective).toEqual({ level: "all", muted: false, isDm: true });
  });
});

describe("shouldNotify", () => {
  const server = (level: "all" | "mentions" | "none", muted = false) => ({
    level,
    muted,
    isDm: false,
  });
  const dm = (level: "all" | "mentions" | "none", muted = false) => ({
    level,
    muted,
    isDm: true,
  });

  it("follows the level in a server channel; mute silences plain messages only", () => {
    expect(shouldNotify(server("all"), false)).toBe(true);
    expect(shouldNotify(server("all", true), false)).toBe(false);
    expect(shouldNotify(server("all", true), true)).toBe(true);
    expect(shouldNotify(server("mentions"), false)).toBe(false);
    expect(shouldNotify(server("mentions", true), true)).toBe(true);
    expect(shouldNotify(server("none"), true)).toBe(false);
  });

  it("silences a muted DM altogether", () => {
    expect(shouldNotify(dm("all"), false)).toBe(true);
    expect(shouldNotify(dm("all", true), true)).toBe(false);
    expect(shouldNotify(dm("mentions"), false)).toBe(false);
    expect(shouldNotify(dm("mentions"), true)).toBe(true);
    expect(shouldNotify(dm("none"), true)).toBe(false);
  });

  it("hides the unread badge only while muted", () => {
    expect(showsUnreadBadge({ muted: true })).toBe(false);
    expect(showsUnreadBadge({ muted: false })).toBe(true);
  });
});

describe("nextMuteExpiry", () => {
  it("is the earliest timed mute still to end", () => {
    expect(
      nextMuteExpiry(
        [
          row({ muted: true }),
          row({ muted: true, muted_until: "2026-11-14T11:00:00Z" }),
          row({ muted: true, muted_until: "2026-11-14T15:00:00Z" }),
          row({ muted: true, muted_until: "2026-11-14T13:00:00Z" }),
          row({ muted: false, muted_until: "2026-11-14T12:30:00Z" }),
        ],
        NOW,
      ),
    ).toBe(Date.parse("2026-11-14T13:00:00Z"));
    expect(nextMuteExpiry([row({ muted: true })], NOW)).toBeNull();
  });
});
