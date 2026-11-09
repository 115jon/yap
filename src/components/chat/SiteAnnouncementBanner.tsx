/**
 * The site-wide announcement a site admin publishes from the admin console,
 * read from GET /api/site-config and drawn by SiteAnnouncementBar across the
 * top of the app.
 *
 * Dismissal is per announcement (its id) and per browser. A critical one
 * cannot be dismissed. The config is fetched on mount, every 5 minutes while
 * the page is visible, and when the tab comes back.
 */

import { useCallback, useEffect, useState } from "react";

import { SiteAnnouncementBar } from "@/components/chat/SiteAnnouncementBar";
import { apiGet } from "@/lib/api-client";
import {
  activeAnnouncement,
  parseAnnouncement,
  type SiteAnnouncement,
} from "@/lib/site-flags";

const REFRESH_MS = 5 * 60_000;
const DISMISSED_KEY = "yap:announcement-dismissed";

function readDismissed(): string | null {
  try {
    return window.localStorage.getItem(DISMISSED_KEY);
  } catch {
    return null;
  }
}

function writeDismissed(id: string) {
  try {
    window.localStorage.setItem(DISMISSED_KEY, id);
  } catch {
    // Private mode or blocked storage: it comes back next load.
  }
}

/** The live announcement for this client, and a way to dismiss it. */
export function useSiteAnnouncement(): {
  announcement: SiteAnnouncement | null;
  dismiss: () => void;
} {
  const [announcement, setAnnouncement] = useState<SiteAnnouncement | null>(
    null,
  );
  const [dismissed, setDismissed] = useState<string | null>(() =>
    typeof window === "undefined" ? null : readDismissed(),
  );

  const load = useCallback(async () => {
    try {
      const config = await apiGet<{ announcement?: unknown }>(
        "/api/site-config",
      );
      setAnnouncement(parseAnnouncement(config.announcement));
    } catch {
      // Offline, or an older server without the route: show nothing.
    }
  }, []);

  useEffect(() => {
    // The fetch sets state when it answers, not synchronously.
    // eslint-disable-next-line react-hooks-js/set-state-in-effect
    void load();
    let id: number | undefined;
    const schedule = () => {
      window.clearInterval(id);
      if (document.visibilityState === "visible") {
        id = window.setInterval(() => void load(), REFRESH_MS);
      }
    };
    const onVisibility = () => {
      if (document.visibilityState === "visible") void load();
      schedule();
    };
    schedule();
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.clearInterval(id);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [load]);

  const live = activeAnnouncement(announcement);
  const shown =
    live && (live.tone === "critical" || live.id !== dismissed) ? live : null;
  return {
    announcement: shown,
    dismiss: () => {
      if (!live) return;
      writeDismissed(live.id);
      setDismissed(live.id);
    },
  };
}

/** The bar wired to /api/site-config, for the app shell. */
export function SiteAnnouncementBanner() {
  const { announcement, dismiss } = useSiteAnnouncement();
  if (!announcement) return null;
  return (
    <SiteAnnouncementBar announcement={announcement} onDismiss={dismiss} />
  );
}
