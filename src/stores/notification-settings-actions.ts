// ── Notification settings (store side) ──────────────────────────────────────
// Loads the user's levels and mutes, saves a change optimistically (rolled
// back if the server refuses it), takes NOTIFICATION_SETTINGS_UPDATE from the
// gateway, and moves the store's mute clock when a timed mute runs out so
// the sidebars un-dim on their own.

import { apiGet, apiPut } from "@/lib/api-client";
import type { ChatAction, ChatState } from "@/lib/chat-reducer";
import {
  nextMuteExpiry,
  notificationSettingKey,
  type NotificationSetting,
} from "@/lib/notification-levels";
import {
  applyNotificationSettingsChange,
  parseNotificationSetting,
  toNotificationSettingsMap,
  type NotificationSettingsChange,
} from "@/lib/notification-settings-client";

export interface NotificationSettingsActions {
  loadNotificationSettings: () => Promise<void>;
  /** Saves a level and/or mute; resolves false when it was rolled back. */
  updateNotificationSettings: (
    change: NotificationSettingsChange,
  ) => Promise<boolean>;
  /** A setting pushed by the gateway (another device, or this one's echo). */
  applyNotificationSetting: (setting: unknown) => void;
}

/** setTimeout's ceiling (about 24.8 days). */
const MAX_TIMER_MS = 2 ** 31 - 1;

export function createNotificationSettingsActions(
  get: () => ChatState,
  dispatch: (action: ChatAction) => void,
  options: {
    /** Reports a save that failed (a toast in the app). */
    onSaveError?: () => void;
  } = {},
): NotificationSettingsActions {
  /** Server clock minus ours, from GET's server_time. */
  let clockSkewMs = 0;
  let expiryTimer: ReturnType<typeof setTimeout> | null = null;
  let loadGeneration = 0;
  /** Keys changed (locally or by the gateway) while a load is in flight. */
  const changedDuringLoad = new Set<string>();
  /** The latest local save per key; an older save's answer is ignored. */
  const saveSeqByKey = new Map<string, number>();
  let saveSeq = 0;

  const now = () => Date.now() + clockSkewMs;

  const scheduleExpiry = () => {
    if (expiryTimer) clearTimeout(expiryTimer);
    expiryTimer = null;
    const state = get();
    const next = nextMuteExpiry(
      Object.values(state.notificationSettings),
      Math.max(now(), state.notificationSettingsNow),
    );
    if (next === null) return;
    const delay = Math.min(Math.max(0, next - now()), MAX_TIMER_MS);
    expiryTimer = setTimeout(() => {
      expiryTimer = null;
      dispatch({
        type: "TICK_NOTIFICATION_SETTINGS",
        nowMs: Math.max(now(), next),
      });
      scheduleExpiry();
    }, delay);
  };

  const upsert = (setting: NotificationSetting) => {
    changedDuringLoad.add(
      notificationSettingKey(setting.target_type, setting.target_id),
    );
    dispatch({ type: "UPSERT_NOTIFICATION_SETTING", setting, nowMs: now() });
    scheduleExpiry();
  };

  const loadNotificationSettings = async () => {
    const generation = ++loadGeneration;
    changedDuringLoad.clear();
    try {
      const data = await apiGet<{
        settings?: unknown[];
        server_time?: string;
      }>("/api/notification-settings");
      if (generation !== loadGeneration) return;
      const serverTime = Date.parse(data.server_time ?? "");
      if (!Number.isNaN(serverTime)) clockSkewMs = serverTime - Date.now();
      const loaded = toNotificationSettingsMap(
        (data.settings ?? [])
          .map(parseNotificationSetting)
          .filter((setting): setting is NotificationSetting => !!setting),
      );
      // What changed while the request was out is newer than its answer.
      const current = get().notificationSettings;
      for (const key of changedDuringLoad) {
        const value = current[key];
        if (value) loaded[key] = value;
        else delete loaded[key];
      }
      dispatch({
        type: "SET_NOTIFICATION_SETTINGS",
        settings: loaded,
        nowMs: now(),
      });
      scheduleExpiry();
    } catch {
      /* keep what we have; the next Ready loads again */
    }
  };

  const updateNotificationSettings = async (
    change: NotificationSettingsChange,
  ) => {
    const key = notificationSettingKey(change.target_type, change.target_id);
    const previous = get().notificationSettings[key];
    const optimistic = applyNotificationSettingsChange(previous, change, now());
    const seq = ++saveSeq;
    saveSeqByKey.set(key, seq);
    upsert(optimistic);
    try {
      const data = await apiPut<{ setting?: unknown }>(
        "/api/notification-settings",
        change,
      );
      const saved = parseNotificationSetting(data.setting);
      if (saved && saveSeqByKey.get(key) === seq) upsert(saved);
      return true;
    } catch {
      // Roll back only if nothing newer (a later save, the gateway) landed.
      if (
        saveSeqByKey.get(key) === seq &&
        get().notificationSettings[key] === optimistic
      ) {
        if (previous) {
          upsert(previous);
        } else {
          dispatch({
            type: "REMOVE_NOTIFICATION_SETTING",
            targetType: change.target_type,
            targetId: change.target_id,
            nowMs: now(),
          });
          scheduleExpiry();
        }
      }
      options.onSaveError?.();
      return false;
    } finally {
      if (saveSeqByKey.get(key) === seq) saveSeqByKey.delete(key);
    }
  };

  const applyNotificationSetting = (value: unknown) => {
    const setting = parseNotificationSetting(value);
    if (!setting) return;
    // The server broadcasts after each write, in order, so the gateway is
    // the latest word: a save of ours still out no longer settles the row.
    saveSeqByKey.delete(
      notificationSettingKey(setting.target_type, setting.target_id),
    );
    upsert(setting);
  };

  return {
    loadNotificationSettings,
    updateNotificationSettings,
    applyNotificationSetting,
  };
}
