/**
 * Dev-only: the site announcement strip (chat/SiteAnnouncementBanner) in
 * each tone, as `?surface=site-banner` (mounted by UiFixtures).
 */

import { SiteAnnouncementBar } from "@/components/chat/SiteAnnouncementBar";
import type { AnnouncementTone } from "@/lib/site-flags";

const TEXT: Record<AnnouncementTone, string> = {
  info: "Yap 3.14 is out: chunked uploads up to 500 MB for Loud.",
  warning:
    "Voice moves to the new SFU tonight at 22:00 UTC. Expect a 30-second blip.",
  critical:
    "Sign-in is down while Kova restarts. Calls already running keep going.",
};

export function SiteBannerFixture() {
  return (
    <div className="flex h-full flex-col gap-6 bg-ply-0 p-6 max-md:p-3">
      {(["info", "warning", "critical"] as const).map((tone) => (
        <div
          key={tone}
          data-testid={`banner-${tone}`}
          className="overflow-hidden rounded-[2px] border border-line"
        >
          <SiteAnnouncementBar
            announcement={{
              id: tone,
              tone,
              text: TEXT[tone],
              link_url:
                tone === "critical" ? null : "https://yap.115jon.com/status",
              link_label: tone === "info" ? "What's new" : "Status",
              expires_at: null,
              updated_at: "2026-11-09T12:00:00.000Z",
            }}
            onDismiss={() => {}}
          />
        </div>
      ))}
    </div>
  );
}
