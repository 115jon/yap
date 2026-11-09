import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import { TooBigCard } from "@/components/ui/too-big-card";
import type { TooBigSituation } from "@/lib/too-big-copy";
import { useMyEntitlement } from "@/hooks/useEntitlement";
import { formatUploadLimit, clientUploadLimitFor } from "@/lib/upload-limits";

/**
 * Shown over the chat column while files are dragged in. Presence is opacity
 * only (it mounts transparent and fades in); nothing moves or loops.
 *
 * With `oversize` (a dragged file is known to be over the limit before the
 * drop) the zone turns into the "too big" card: Yappy with the file in his
 * mouth, the numbers and the meter. Browsers that hide sizes until the drop
 * get the normal zone, and the tray says it after the drop.
 */
export function DragDropOverlay({
  isDragging,
  oversize = null,
}: {
  isDragging: boolean;
  oversize?: TooBigSituation | null;
}) {
  const limit = clientUploadLimitFor(useMyEntitlement(), "attachment");
  if (!isDragging) return null;
  if (oversize) {
    return (
      <div
        data-testid="drop-overlay-oversize"
        className="pointer-events-none absolute inset-0 z-30 m-4 flex items-center justify-center rounded-xl border-2 border-dashed border-accent-ink bg-ply-1/92 px-6 transition-opacity duration-(--dur-2) ease-out starting:opacity-0"
      >
        <TooBigCard variant="overlay" situation={oversize} />
      </div>
    );
  }
  return (
    <div
      aria-hidden="true"
      className="pointer-events-none absolute inset-0 z-30 m-4 flex items-center justify-center rounded-xl border-2 border-dashed border-line-strong bg-scrim transition-opacity duration-(--dur-2) ease-out starting:opacity-0"
    >
      <div className="flex flex-col items-center gap-2 rounded-lg border border-line bg-ply-3 px-8 py-6 text-center shadow-float">
        <Icon name="upload" size="2xl" className="text-ink-2" />
        <p className="text-ui-md font-semibold text-ink-1">Drop to upload</p>
        <p className="text-ui-sm text-ink-3">
          Files can be up to <Num size="sm">{formatUploadLimit(limit)}</Num>.
        </p>
      </div>
    </div>
  );
}
