import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { chatReducer, initialState, type ChatState } from "@/lib/chat-reducer";
import type { NotificationSetting } from "@/lib/notification-levels";

const mocks = vi.hoisted(() => ({
  apiGet: vi.fn(),
  apiPut: vi.fn(),
}));

vi.mock("@/lib/api-client", () => ({
  apiGet: mocks.apiGet,
  apiPut: mocks.apiPut,
}));

import { createNotificationSettingsActions } from "./notification-settings-actions";

const NOW = Date.parse("2026-11-14T12:00:00.000Z");

function createFixture() {
  let state: ChatState = { ...initialState };
  const onChange = vi.fn();
  const onSaveError = vi.fn();
  const actions = createNotificationSettingsActions(
    () => state,
    (action) => {
      state = chatReducer(state, action);
    },
    { onChange, onSaveError },
  );
  return { actions, getState: () => state, onChange, onSaveError };
}

const channelMute: NotificationSetting = {
  target_type: "channel",
  target_id: "c1",
  level: null,
  muted: true,
  muted_until: null,
};

describe("notification settings actions", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
    mocks.apiGet.mockReset();
    mocks.apiPut.mockReset();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("loads the settings into the store", async () => {
    mocks.apiGet.mockResolvedValue({
      settings: [channelMute, { target_type: "bogus" }],
      server_time: new Date(NOW).toISOString(),
    });
    const { actions, getState, onChange } = createFixture();
    await actions.loadNotificationSettings();
    expect(mocks.apiGet).toHaveBeenCalledWith("/api/notification-settings");
    expect(getState().notificationSettings).toEqual({
      "channel:c1": channelMute,
    });
    expect(onChange).toHaveBeenCalled();
  });

  it("keeps a change that landed while the load was out", async () => {
    let resolveLoad: (value: unknown) => void = () => {};
    mocks.apiGet.mockReturnValue(
      new Promise((resolve) => {
        resolveLoad = resolve;
      }),
    );
    const { actions, getState } = createFixture();
    const load = actions.loadNotificationSettings();
    const unmuted = { ...channelMute, muted: false };
    actions.applyNotificationSetting(unmuted);
    resolveLoad({ settings: [channelMute], server_time: null });
    await load;
    expect(getState().notificationSettings["channel:c1"]).toEqual(unmuted);
  });

  it("shows a mute at once and keeps the server's answer", async () => {
    let resolvePut: (value: unknown) => void = () => {};
    mocks.apiPut.mockReturnValue(
      new Promise((resolve) => {
        resolvePut = resolve;
      }),
    );
    const { actions, getState } = createFixture();
    const save = actions.updateNotificationSettings({
      target_type: "server",
      target_id: "s1",
      mute: "1h",
    });
    expect(getState().notificationSettings["server:s1"]).toMatchObject({
      muted: true,
      muted_until: "2026-11-14T13:00:00.000Z",
    });
    const saved: NotificationSetting = {
      target_type: "server",
      target_id: "s1",
      level: null,
      muted: true,
      muted_until: "2026-11-14T13:00:01.000Z",
    };
    resolvePut({ setting: saved });
    await expect(save).resolves.toBe(true);
    expect(mocks.apiPut).toHaveBeenCalledWith("/api/notification-settings", {
      target_type: "server",
      target_id: "s1",
      mute: "1h",
    });
    expect(getState().notificationSettings["server:s1"]).toEqual(saved);
  });

  it("rolls back a refused change and reports it", async () => {
    mocks.apiGet.mockResolvedValue({ settings: [channelMute] });
    mocks.apiPut.mockRejectedValue(new Error("nope"));
    const { actions, getState, onSaveError } = createFixture();
    await actions.loadNotificationSettings();

    await expect(
      actions.updateNotificationSettings({
        target_type: "channel",
        target_id: "c1",
        mute: "off",
        level: "all",
      }),
    ).resolves.toBe(false);
    expect(getState().notificationSettings["channel:c1"]).toEqual(channelMute);

    await actions.updateNotificationSettings({
      target_type: "channel",
      target_id: "c-new",
      level: "none",
    });
    expect(getState().notificationSettings["channel:c-new"]).toBeUndefined();
    expect(onSaveError).toHaveBeenCalledTimes(2);
  });

  it("does not roll back over a newer gateway setting", async () => {
    let rejectPut: (error: unknown) => void = () => {};
    mocks.apiPut.mockReturnValue(
      new Promise((_, reject) => {
        rejectPut = reject;
      }),
    );
    const { actions, getState } = createFixture();
    const save = actions.updateNotificationSettings({
      target_type: "channel",
      target_id: "c1",
      mute: "forever",
    });
    const fromOtherDevice = { ...channelMute, level: "all" as const };
    actions.applyNotificationSetting(fromOtherDevice);
    rejectPut(new Error("offline"));
    await save;
    expect(getState().notificationSettings["channel:c1"]).toEqual(
      fromOtherDevice,
    );
  });

  it("moves the mute clock when a timed mute runs out", async () => {
    const { actions, getState, onChange } = createFixture();
    actions.applyNotificationSetting({
      ...channelMute,
      muted_until: "2026-11-14T13:00:00.000Z",
    });
    expect(getState().notificationSettingsNow).toBe(NOW);
    onChange.mockClear();

    vi.advanceTimersByTime(60 * 60 * 1000 - 1);
    expect(getState().notificationSettingsNow).toBe(NOW);
    vi.advanceTimersByTime(1);
    expect(getState().notificationSettingsNow).toBe(
      Date.parse("2026-11-14T13:00:00.000Z"),
    );
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});
