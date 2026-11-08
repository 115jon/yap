/**
 * Yap links in messages, handed to Yap Native (the Windows app) when this
 * browser's "Open links in Yap Native" setting is on.
 *
 * A click on a message link (`a[data-message-link]`, MarkdownRenderer) to a
 * Yap invite or channel/message URL opens its `yap-native://` link instead
 * (src/lib/native-app-link.ts). The browser asks before it opens the app,
 * and nothing here can tell whether the app is installed, so a toast keeps
 * "Open here" for the web page. Modified clicks (new tab, new window) and
 * every other link behave as before.
 *
 * Off in the Tauri desktop app, the mobile app and browsers off Windows
 * (canOfferNativeApp), whatever the setting says.
 */

import { toast } from "@/components/ui/toast";
import {
  canOfferNativeApp,
  launchNativeApp,
  nativeLinkForWebUrl,
  readOpenLinksInNative,
  subscribeOpenLinksInNative,
} from "@/lib/native-app-link";
import { useEffect, useSyncExternalStore } from "react";

const noSubscribe = () => () => {};

/** canOfferNativeApp, false while server rendering and hydrating. */
export function useCanOfferNativeApp(): boolean {
  return useSyncExternalStore(noSubscribe, canOfferNativeApp, () => false);
}

/** This browser's "Open links in Yap Native" setting. */
export function useOpenLinksInNative(): boolean {
  return useSyncExternalStore(
    subscribeOpenLinksInNative,
    readOpenLinksInNative,
    () => false,
  );
}

/** The selector for links the handler may take over. */
export const MESSAGE_LINK_SELECTOR = "a[data-message-link]";

/**
 * Handles one click: true when it went to Yap Native. Exported for tests.
 */
export function handleNativeLinkClick(event: MouseEvent): boolean {
  if (
    event.defaultPrevented ||
    event.button !== 0 ||
    event.metaKey ||
    event.ctrlKey ||
    event.shiftKey ||
    event.altKey
  ) {
    return false;
  }
  const target = event.target as Element | null;
  const anchor = target?.closest?.<HTMLAnchorElement>(MESSAGE_LINK_SELECTOR);
  if (!anchor?.href) return false;
  const appLink = nativeLinkForWebUrl(anchor.href);
  if (!appLink) return false;

  event.preventDefault();
  event.stopPropagation();
  const webUrl = anchor.href;
  launchNativeApp(appLink);
  toast.info("Opening in Yap Native", {
    id: "native-app-link",
    description: "If the app doesn't open, open the link here.",
    action: {
      label: "Open here",
      onClick: () => {
        window.open(webUrl, "_blank", "noopener,noreferrer");
      },
    },
  });
  return true;
}

export function useNativeAppLinkHandler() {
  const available = useCanOfferNativeApp();
  const enabled = useOpenLinksInNative();
  useEffect(() => {
    if (!available || !enabled) return;
    const onClick = (event: MouseEvent) => {
      handleNativeLinkClick(event);
    };
    // Capture, so the link's own target="_blank" never opens a tab first.
    document.addEventListener("click", onClick, true);
    return () => document.removeEventListener("click", onClick, true);
  }, [available, enabled]);
}
