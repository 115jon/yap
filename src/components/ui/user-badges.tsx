/**
 * Badges: the pixel-art marks a profile shows (src/lib/badges.ts for what
 * they are, src/lib/badge-art.ts for the cells).
 *
 * Each badge is a 16-cell drawing printed like Yappy (ui/yap-bark): the body
 * through the accent Bayer dither, the edge solid accent, details in ink-1,
 * as alpha masks over role colours, so every theme recolours them with no
 * JS. Unlike the decorative dither they carry meaning, so they stay under
 * forced colours (drawn in CanvasText) and each has an accessible name.
 *
 * `UserBadges` never cuts a badge off: it wraps onto a second line, or with
 * `maxVisible` shows that many and a "+N" key whose tooltip names the rest.
 * Every badge is a button: hover or focus shows its tooltip (name,
 * description, "Earned {date}"), and a tap toggles it, for touch.
 */

import * as React from "react";

import { bayerLit, cellMask } from "@/components/ui/dither";
import { CellLayer } from "@/components/ui/yap-bark";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { BADGE_CELLS, badgeCellInk, badgeLight } from "@/lib/badge-art";
import {
  badgeTooltipLines,
  getBadge,
  sortBadges,
  type BadgeId,
  type UserBadge,
} from "@/lib/badges";
import { cn } from "@/lib/utils";

export type BadgeSize = "sm" | "md" | "lg";

/** CSS px per cell: 16, 24 and 32 px badges. */
const CELL_PX: Record<BadgeSize, number> = { sm: 1, md: 1.5, lg: 2 };

const GAP: Record<BadgeSize, string> = {
  sm: "gap-1",
  md: "gap-1",
  lg: "gap-1.5",
};

const masks = new Map<BadgeId, { lit: string; detail: string }>();

function badgeMasks(id: BadgeId) {
  let entry = masks.get(id);
  if (!entry) {
    entry = {
      lit: cellMask(BADGE_CELLS, BADGE_CELLS, (x, y) => {
        const ink = badgeCellInk(id, x, y);
        if (ink === "edge") return true;
        return ink === "body" && bayerLit(x, y, badgeLight(x, y));
      }),
      detail: cellMask(
        BADGE_CELLS,
        BADGE_CELLS,
        (x, y) => badgeCellInk(id, x, y) === "detail",
      ),
    };
    masks.set(id, entry);
  }
  return entry;
}

/** One badge's drawing. Decorative: the caller names it. */
export function BadgeArt({
  id,
  size = "md",
  className,
}: {
  id: BadgeId;
  size?: BadgeSize;
  className?: string;
}) {
  const { lit, detail } = badgeMasks(id);
  const px = BADGE_CELLS * CELL_PX[size];
  return (
    <span
      aria-hidden="true"
      data-slot="badge-art"
      data-badge={id}
      className={cn(
        "pointer-events-none relative block shrink-0 [print-color-adjust:exact]",
        "[--badge-detail:var(--ink-1)] [--badge-lit:var(--accent-ink)]",
        "forced-colors:[--badge-detail:CanvasText] forced-colors:[--badge-lit:CanvasText] forced-colors:[forced-color-adjust:none]",
        className,
      )}
      style={{ width: px, height: px }}
    >
      <CellLayer part="lit" mask={lit} color="var(--badge-lit)" />
      <CellLayer part="detail" mask={detail} color="var(--badge-detail)" />
    </span>
  );
}

const KEY = cn(
  "relative inline-flex shrink-0 items-center justify-center rounded-[2px] outline-none [-webkit-tap-highlight-color:transparent]",
  "transition-colors duration-(--dur-1) ease-out pointer-fine:hover:bg-tint-1",
  "focus-visible:ring-2 focus-visible:ring-accent-ink",
  // A 44px target on coarse pointers without growing the row.
  "after:absolute after:-inset-2.5 after:content-[''] pointer-fine:after:hidden",
);

function BadgeKey({
  badge,
  size,
  now,
}: {
  badge: UserBadge;
  size: BadgeSize;
  now?: number;
}) {
  const [open, setOpen] = React.useState(false);
  const lines = badgeTooltipLines(badge, now);
  return (
    <Tooltip open={open} onOpenChange={setOpen}>
      <TooltipTrigger asChild>
        <button
          type="button"
          role="listitem"
          data-slot="user-badge"
          data-badge={badge.id}
          aria-label={`${lines.name}: ${lines.description}`}
          aria-description={lines.earned ?? undefined}
          className={cn(KEY, "p-0.5")}
          onClick={() => setOpen((value) => !value)}
        >
          <BadgeArt id={badge.id} size={size} />
        </button>
      </TooltipTrigger>
      <TooltipContent>
        <span className="flex flex-col gap-0.5 py-0.5">
          <span className="font-semibold">{lines.name}</span>
          <span>{lines.description}</span>
          {lines.earned ? (
            <span className="opacity-70">{lines.earned}</span>
          ) : null}
        </span>
      </TooltipContent>
    </Tooltip>
  );
}

function MoreKey({ rest, size }: { rest: UserBadge[]; size: BadgeSize }) {
  const [open, setOpen] = React.useState(false);
  const names = rest.map((badge) => getBadge(badge.id)?.name ?? badge.id);
  const px = BADGE_CELLS * CELL_PX[size];
  return (
    <Tooltip open={open} onOpenChange={setOpen}>
      <TooltipTrigger asChild>
        <button
          type="button"
          role="listitem"
          data-slot="user-badge-more"
          aria-label={`${rest.length} more: ${names.join(", ")}`}
          className={cn(KEY, "p-0.5 text-num-xs text-ink-2")}
          onClick={() => setOpen((value) => !value)}
        >
          <span
            className="flex items-center justify-center"
            style={{ minWidth: px, height: px }}
          >
            +{rest.length}
          </span>
        </button>
      </TooltipTrigger>
      <TooltipContent>{names.join(", ")}</TooltipContent>
    </Tooltip>
  );
}

/**
 * A profile's badges, in their fixed order. Nothing renders without any.
 * Without `maxVisible` they wrap; with it, the rest fold into "+N".
 */
export function UserBadges({
  badges,
  size = "md",
  maxVisible,
  now,
  className,
}: {
  badges: readonly UserBadge[] | null | undefined;
  size?: BadgeSize;
  /** Show at most this many, then "+N" (one line in a tight row). */
  maxVisible?: number;
  /** For tests and fixtures: the clock the Loud months read. */
  now?: number;
  className?: string;
}) {
  const known = React.useMemo(
    () => sortBadges((badges ?? []).filter((badge) => getBadge(badge.id))),
    [badges],
  );
  if (known.length === 0) return null;
  const folds =
    maxVisible !== undefined && maxVisible > 0 && known.length > maxVisible;
  // "+N" takes a slot, so one fewer badge shows beside it.
  const shown = folds ? known.slice(0, Math.max(1, maxVisible - 1)) : known;
  const rest = folds ? known.slice(shown.length) : [];
  return (
    <div
      role="list"
      aria-label="Badges"
      data-slot="user-badges"
      className={cn(
        "flex min-w-0 items-center",
        folds ? "flex-nowrap" : "flex-wrap",
        GAP[size],
        className,
      )}
    >
      {shown.map((badge) => (
        <BadgeKey key={badge.id} badge={badge} size={size} now={now} />
      ))}
      {rest.length > 0 ? <MoreKey rest={rest} size={size} /> : null}
    </div>
  );
}
