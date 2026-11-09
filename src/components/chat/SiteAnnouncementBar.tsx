/**
 * The site announcement strip (shapes in lib/site-flags.ts): an LED for the
 * tone (unlit for info, idle amber for a warning, muted red when critical),
 * the text, an optional link, and a dismiss key a critical one does not
 * get. Presentational: the app wires it in SiteAnnouncementBanner, the
 * admin console previews it.
 */

import { IconButton } from "@/components/ui/IconButton";
import { Led, type LedState } from "@/components/ui/led";
import type { AnnouncementTone, SiteAnnouncement } from "@/lib/site-flags";
import { cn } from "@/lib/utils";

const TONE_LED: Record<AnnouncementTone, LedState> = {
  info: "off",
  warning: "idle",
  critical: "muted",
};

const TONE_LABEL: Record<AnnouncementTone, string> = {
  info: "Notice",
  warning: "Heads up",
  critical: "Important",
};

export function SiteAnnouncementBar({
  announcement,
  onDismiss,
  className,
}: {
  announcement: SiteAnnouncement;
  onDismiss?: () => void;
  className?: string;
}) {
  const critical = announcement.tone === "critical";
  return (
    <div
      role={critical ? "alert" : "status"}
      data-tone={announcement.tone}
      className={cn(
        "relative flex min-h-9 shrink-0 items-center gap-3 border-b border-line bg-ply-2 px-3 py-1.5 text-ui-sm text-ink-1",
        critical && "bg-danger-tint",
        announcement.tone === "warning" && "bg-warning-tint",
        className,
      )}
    >
      <span className="silkscreen inline-flex shrink-0 items-center gap-2 text-ink-2 max-sm:sr-only">
        <Led state={TONE_LED[announcement.tone]} />
        {TONE_LABEL[announcement.tone]}
      </span>
      <span className="flex shrink-0 sm:hidden">
        <Led state={TONE_LED[announcement.tone]} />
      </span>
      <p className="min-w-0 flex-1 text-pretty">
        {announcement.text}
        {announcement.link_url ? (
          <>
            {" "}
            <a
              href={announcement.link_url}
              target="_blank"
              rel="noreferrer"
              className="font-medium whitespace-nowrap text-accent-ink underline-offset-[3px] pointer-fine:hover:underline"
            >
              {announcement.link_label ?? "Read more"}
            </a>
          </>
        ) : null}
      </p>
      {!critical && onDismiss ? (
        <IconButton
          icon="close"
          size={24}
          tooltip="Dismiss"
          onClick={onDismiss}
          className="-mr-1 shrink-0"
        />
      ) : null}
    </div>
  );
}
