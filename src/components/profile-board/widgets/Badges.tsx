/**
 * Badge case: the owner's badges in the order they picked (or newest
 * first), drawn with BadgeArt. S is a row of marks; M and L name each one.
 * Every badge is a key whose tooltip says what it is and when it was earned.
 */

import { useState, type ReactNode } from "react";

import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { Num } from "@/components/ui/num";
import { BadgeArt } from "@/components/ui/user-badges";
import {
  BADGE_IDS,
  badgeTooltipLines,
  getBadge,
  type UserBadge,
} from "@/lib/badges";
import { cn } from "@/lib/utils";

import { formatDayMonth } from "../board-format";
import { Plate, WidgetEmpty, type WidgetProps } from "./shared";

export function BadgesWidget({ widget, data, ctx }: WidgetProps<"badges">) {
  const earned = (data.badges ?? []).filter((badge) =>
    getBadge(badge.id),
  ) as UserBadge[];
  const picked = widget.cfg.badge_ids;
  const shown =
    picked.length > 0
      ? picked
          .map((id) => earned.find((badge) => badge.id === id))
          .filter((badge): badge is UserBadge => !!badge)
      : earned;
  const now = ctx.at;
  const readout = (
    <Num className="text-ink-3">
      {earned.length} / {BADGE_IDS.length}
    </Num>
  );

  if (shown.length === 0) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <WidgetEmpty
          icon="verified"
          title="No badges yet"
          hint={
            widget.size === "S"
              ? undefined
              : "Badges come from voice time, servers and a few surprises."
          }
        />
      </Plate>
    );
  }

  const latest = [...earned].sort((a, b) =>
    b.awarded_at.localeCompare(a.awarded_at),
  )[0];

  if (widget.size === "S") {
    const visible = shown.slice(0, 6);
    const more = shown.length - visible.length;
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <ul aria-label="Badges" className="flex flex-wrap items-center gap-1">
          {visible.map((badge) => (
            <li key={badge.id}>
              <BadgeKey badge={badge} now={now}>
                <BadgeArt id={badge.id} size="lg" />
              </BadgeKey>
            </li>
          ))}
          {more > 0 ? (
            <li>
              <Num className="px-1 text-ink-2">+{more}</Num>
            </li>
          ) : null}
        </ul>
        {latest ? (
          <p className="mt-auto truncate text-ui-xs text-ink-3">
            Latest: {getBadge(latest.id)?.name},{" "}
            {formatDayMonth(latest.awarded_at)}
          </p>
        ) : null}
      </Plate>
    );
  }

  if (widget.size === "M") {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <ul
          aria-label="Badges"
          className="grid grid-cols-2 gap-x-2 gap-y-2 @max-[240px]/cell:grid-cols-1 @max-[240px]/cell:[&>li:nth-child(n+7)]:hidden"
        >
          {shown.slice(0, 8).map((badge) => (
            <li key={badge.id} className="min-w-0">
              <BadgeKey
                badge={badge}
                now={now}
                className="w-full justify-start gap-2.5 p-1"
              >
                <BadgeArt id={badge.id} size="lg" />
                <span className="flex min-w-0 flex-col text-left">
                  <span className="truncate text-ui-sm font-medium text-ink-1">
                    {getBadge(badge.id)?.name}
                  </span>
                  <span className="truncate text-ui-xs text-ink-3">
                    {formatDayMonth(badge.awarded_at)}
                  </span>
                </span>
              </BadgeKey>
            </li>
          ))}
        </ul>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <ul
        aria-label="Badges"
        className="grid flex-1 grid-cols-8 gap-2 @max-[480px]/cell:grid-cols-4 @max-[480px]/cell:[&>li:nth-child(n+5)]:hidden"
      >
        {shown.slice(0, 8).map((badge) => (
          <li key={badge.id} className="min-w-0">
            <BadgeKey
              badge={badge}
              now={now}
              className="w-full flex-col gap-2 px-1 py-2"
            >
              <BadgeArt id={badge.id} size="lg" />
              <span className="w-full truncate text-center text-ui-xs text-ink-2">
                {getBadge(badge.id)?.name}
              </span>
            </BadgeKey>
          </li>
        ))}
      </ul>
    </Plate>
  );
}

function BadgeKey({
  badge,
  now,
  className,
  children,
}: {
  badge: UserBadge;
  now: number;
  className?: string;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(false);
  const lines = badgeTooltipLines(badge, now);
  return (
    <Tooltip open={open} onOpenChange={setOpen}>
      <TooltipTrigger asChild>
        <button
          type="button"
          data-badge={badge.id}
          aria-label={`${lines.name}: ${lines.description}`}
          className={cn(
            "relative inline-flex items-center justify-center rounded-[2px] p-0.5 outline-none [-webkit-tap-highlight-color:transparent]",
            "transition-colors duration-(--dur-1) ease-out pointer-fine:hover:bg-tint-1",
            "focus-visible:ring-2 focus-visible:ring-accent-ink",
            className,
          )}
          onClick={() => setOpen((value) => !value)}
        >
          {children}
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
