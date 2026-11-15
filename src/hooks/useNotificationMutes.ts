import { useMemo } from "react";

import {
  createNotificationMuteLookup,
  type NotificationMuteLookup,
} from "@/lib/notification-settings-client";
import { useChatStore } from "@/stores/chat-store";

/**
 * Mute lookups for list rows (sidebars, the rail, the tab bar). Re-renders
 * when a setting changes or a timed mute runs out (the store's mute clock).
 */
export function useNotificationMutes(): NotificationMuteLookup {
  const settings = useChatStore((s) => s.notificationSettings);
  const nowMs = useChatStore((s) => s.notificationSettingsNow);
  return useMemo(
    () => createNotificationMuteLookup(settings, nowMs),
    [settings, nowMs],
  );
}
