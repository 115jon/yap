/**
 * TooBigCard: what Yap says when a file is over its upload limit (toobig
 * spec, 2026-11-09). Yappy's pose, a headline, one line, the size meter
 * where there is room, and the fix (Remove / Choose another / Pick another).
 * Nothing is shrunk or transcoded; nothing is uploaded.
 *
 *   overlay  the composer's drop zone while a too-big file is dragged over
 *   tray     a card in the composer's attachment tray (after a drop, paste or
 *            the file picker)
 *   inline   inside a picker or settings row: avatar, banner, soundboard,
 *            GIF, vibe, camera background
 *
 * The card is `role="alert"`, named by the whole sentence; the pose is
 * decorative and gone under forced colours. Words: `lib/too-big-copy`.
 */

import { Button } from "@/components/ui/button";
import { DitherDots } from "@/components/ui/dither";
import { Num } from "@/components/ui/num";
import { YappyPose } from "@/components/ui/yappy-pose";
import {
  TOO_BIG_ACTION_LABELS,
  tooBigCopy,
  type TooBigAction,
  type TooBigSituation,
} from "@/lib/too-big-copy";
import { formatUploadLimit, formatUploadSize } from "@/lib/upload-limits";
import { cn } from "@/lib/utils";

export type TooBigCardVariant = "overlay" | "tray" | "inline";

/**
 * The size meter: a 6px track, the in-limit part in accent ink, a 2px ink-1
 * marker at the limit and the overflow as a still warning dither. The bar
 * spans max(size, limit). Labels are Martian numbers; the size is in the
 * warning ink.
 */
export function SizeMeter({
  size,
  limit,
  className,
}: {
  size: number;
  limit: number;
  className?: string;
}) {
  const span = Math.max(size, limit, 1);
  const limitPct = (Math.min(limit, span) / span) * 100;
  const fillPct = (Math.min(size, limit) / span) * 100;
  const over = size > limit;
  return (
    <div data-slot="size-meter" className={cn("w-full", className)}>
      <div aria-hidden="true" className="relative h-1.5 rounded-xs bg-tint-3">
        <div
          className="absolute inset-y-0 left-0 rounded-xs bg-accent-ink"
          style={{ width: `${fillPct}%` }}
        />
        {over ? (
          <DitherDots
            ink="warning"
            coverage={0.5}
            cell={1}
            className="absolute inset-y-0"
            style={{ left: `${limitPct}%`, right: 0 }}
          />
        ) : null}
        <div
          className="absolute -inset-y-1 w-0.5 -translate-x-1/2 bg-ink-1"
          style={{ left: `${limitPct}%` }}
        />
      </div>
      <div className="mt-1.5 flex items-baseline justify-between gap-3">
        <Num className="text-ink-3">{formatUploadLimit(limit)} limit</Num>
        <Num className="font-semibold text-warning-ink">
          {formatUploadSize(size)}
        </Num>
      </div>
    </div>
  );
}

const SHELL: Record<TooBigCardVariant, string> = {
  overlay: "flex items-center gap-6 max-sm:flex-col max-sm:text-center",
  tray: "flex w-[320px] max-w-full items-center gap-3 rounded-sm border border-warning-ink/40 bg-ply-3 p-2.5",
  inline:
    "flex items-center gap-4 rounded-sm border border-line-strong bg-ply-2 p-3.5",
};

/** CSS px per Yappy cell, per variant. */
const POSE_CELL: Record<TooBigCardVariant, number> = {
  overlay: 4,
  tray: 2,
  inline: 3,
};

const HEADLINE: Record<TooBigCardVariant, string> = {
  overlay:
    "font-display text-[20px] leading-[23px] font-extrabold text-balance text-ink-1",
  tray: "truncate text-ui-sm font-semibold text-ink-1",
  inline:
    "font-display text-[15px] leading-[18px] font-bold text-balance text-ink-1",
};

const LINE: Record<TooBigCardVariant, string> = {
  overlay: "mt-1.5 text-ui-sm text-pretty text-ink-2",
  tray: "mt-0.5 text-ui-xs text-ink-2",
  inline: "mt-1 text-ui-sm text-pretty text-ink-2",
};

export function TooBigCard({
  situation,
  variant,
  onAction,
  className,
}: {
  situation: TooBigSituation;
  variant: TooBigCardVariant;
  /** Called with the action a key names. Without it no keys are shown. */
  onAction?: (action: TooBigAction) => void;
  className?: string;
}) {
  const copy = tooBigCopy(situation);
  // The tray's small tile has no room for the meter.
  const meter = variant === "tray" ? undefined : copy.meter;
  const actions = onAction ? copy.actions : [];

  return (
    <div
      role="alert"
      aria-label={copy.announcement}
      data-slot="too-big-card"
      data-variant={variant}
      data-situation={situation.kind}
      className={cn(SHELL[variant], className)}
    >
      <YappyPose pose={copy.pose} cell={POSE_CELL[variant]} />
      <div
        className={cn(
          "min-w-0",
          variant === "overlay" ? "max-w-[34ch]" : "flex-1",
        )}
      >
        <p
          className={HEADLINE[variant]}
          title={variant === "tray" ? copy.headline : undefined}
        >
          {copy.headline}
        </p>
        <p className={LINE[variant]}>
          {copy.lead ? (
            <span className="font-semibold break-all text-ink-1">
              {copy.lead}
            </span>
          ) : null}
          {copy.line}
        </p>
        {meter ? (
          <SizeMeter
            size={meter.size}
            limit={meter.limit}
            className={cn(
              "mt-2.5",
              variant === "overlay"
                ? "max-w-[260px] max-sm:mx-auto"
                : "max-w-[220px]",
            )}
          />
        ) : null}
        {copy.hint ? (
          <p className="mt-2 text-ui-xs text-ink-3">{copy.hint}</p>
        ) : null}
        {actions.length > 0 ? (
          <div
            className={cn(
              "flex flex-wrap gap-1.5",
              variant === "tray" ? "mt-2" : "mt-2.5",
            )}
          >
            {actions.map((action) => (
              <Button
                key={action}
                type="button"
                size={variant === "tray" ? "xs" : "sm"}
                variant="outline"
                onClick={() => onAction?.(action)}
              >
                {TOO_BIG_ACTION_LABELS[action]}
              </Button>
            ))}
          </div>
        ) : null}
      </div>
    </div>
  );
}
