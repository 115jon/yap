/**
 * One widget's plate on the Rack (DESIGN.md, C1 legend plate): ply-2, a
 * line-strong edge, 2px corners, the legend printed across the top edge in
 * silkscreen beside the widget's glyph, and an optional readout at the
 * right end of the same line ("● PLAYING", "30 DAYS"). The plate fills its
 * Rack cell; the body is a size container (`@container/cell`) so each widget
 * can pick its narrow layout from the space it really has.
 *
 * The legend and readout sit on `--plate-ground` (the ground the board is
 * on, set by BoardRack; ply-3 in the profile modal).
 */

import { useId, type ComponentProps, type ReactNode } from "react";

import { Icon } from "@/components/ui/icon";
import {
  WIDGET_SPECS,
  type WidgetSize,
  type WidgetType,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { WIDGET_ICON } from "./board-format";

export type WidgetPlateProps = Omit<ComponentProps<"section">, "title"> & {
  type: WidgetType;
  size: WidgetSize;
  /** The printed legend; defaults to the widget's name. */
  legend?: string;
  /** Right end of the legend line: an LED label, a period, a count. */
  readout?: ReactNode;
  /**
   * Keep the readout on a narrow plate (a live LED label). Otherwise it
   * gives way below 200px so the legend reads in full.
   */
  readoutLive?: boolean;
  /** Owner only: shown to nobody else (a paid widget on a free plan). */
  draft?: boolean;
  /** Classes for the body under the legend. */
  bodyClassName?: string;
};

export function WidgetPlate({
  type,
  size,
  legend,
  readout,
  readoutLive = false,
  draft = false,
  className,
  bodyClassName,
  children,
  ...props
}: WidgetPlateProps) {
  const legendId = useId();
  const name = legend?.trim() || WIDGET_SPECS[type].name;
  return (
    <section
      data-slot="widget-plate"
      data-type={type}
      data-size={size}
      data-draft={draft || undefined}
      aria-labelledby={legendId}
      className={cn(
        "@container/plate relative flex h-full min-w-0 flex-col rounded-[2px] border border-line-strong bg-ply-2 px-3.5 pt-5 pb-3.5",
        "[--legend-ground:var(--plate-ground,var(--ply-3))]",
        className,
      )}
      {...props}
    >
      <div className="pointer-events-none absolute -top-[7px] right-2.5 left-2.5 flex min-w-0 items-center gap-2">
        <h3
          id={legendId}
          className="silkscreen inline-flex min-w-0 items-center gap-1.5 bg-(--legend-ground) px-1 text-ink-2"
        >
          <Icon name={WIDGET_ICON[type]} size="xs" className="shrink-0" />
          <span className="truncate">{name}</span>
        </h3>
        {draft ? (
          <span
            data-slot="widget-draft"
            className="silkscreen ml-auto inline-flex shrink-0 items-center gap-1.5 bg-(--legend-ground) px-1 text-accent-ink"
          >
            <Icon name="lock" size="xs" />
            Draft · only you
          </span>
        ) : readout ? (
          <span
            data-slot="widget-readout"
            className={cn(
              "ml-auto inline-flex min-w-0 shrink-0 items-center gap-1.5 bg-(--legend-ground) px-1 text-ink-3",
              !readoutLive && "@max-[200px]/plate:hidden",
            )}
          >
            {readout}
          </span>
        ) : null}
      </div>
      <div
        data-slot="widget-body"
        className={cn(
          "@container/cell flex min-h-0 flex-1 flex-col gap-2.5 overflow-clip [overflow-clip-margin:6px]",
          bodyClassName,
        )}
      >
        {children}
      </div>
    </section>
  );
}
