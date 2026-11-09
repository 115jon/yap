/**
 * Now playing: the owner's live game from presence (only this leaf
 * re-renders on a presence flip), else the last finished game when the
 * owner chose "last played". Play time this week and all time come from
 * `data.game_totals`, keyed by the lower-cased game name.
 */

import type { ReactNode } from "react";

import { LedLabel } from "@/components/ui/led";
import { Num, NumClock } from "@/components/ui/num";
import { useGameActivity } from "@/hooks/useGameActivity";
import { cn } from "@/lib/utils";

import { formatAgo, formatDuration, formatHours } from "../board-format";
import { useBoardMedia } from "../board-context";
import {
  Plate,
  WidgetEmpty,
  WidgetImage,
  WidgetTitle,
  type WidgetProps,
} from "./shared";

interface Shown {
  live: boolean;
  name: string;
  /** A server media path. */
  art: string | null;
  line: string | null;
  startedAt: number | null;
  endedAt: string | null;
  sessionSeconds: number | null;
}

export function NowPlayingWidget({
  widget,
  data,
  ctx,
}: WidgetProps<"now_playing">) {
  const activity = useGameActivity(ctx.ownerId);
  const media = useBoardMedia();
  const now = ctx.at;

  let shown: Shown | null = null;
  if (activity) {
    shown = {
      live: true,
      name: activity.name,
      art: activity.large_image ?? activity.icon_url ?? null,
      line:
        [activity.details, activity.state].filter(Boolean).join(" · ") || null,
      startedAt: activity.started_at,
      endedAt: null,
      sessionSeconds: null,
    };
  } else if (widget.cfg.when_idle === "last_played" && data.last_game) {
    shown = {
      live: false,
      name: data.last_game.name,
      art: data.last_game.icon_url,
      line: null,
      startedAt: null,
      endedAt: data.last_game.ended_at || null,
      sessionSeconds: data.last_game.seconds,
    };
  }

  const readout = shown?.live ? (
    <LedLabel state="talk">Playing</LedLabel>
  ) : shown?.endedAt ? (
    <span className="silkscreen">Last played</span>
  ) : null;

  if (!shown) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="gamepad"
          title={ctx.isOwner ? "Nothing played yet" : "Not in a game right now"}
          hint={
            ctx.isOwner
              ? "Games show up here while you play, if you share your activity."
              : undefined
          }
        />
      </Plate>
    );
  }

  const totals = data.game_totals?.[shown.name.toLowerCase()];
  const art = shown.art ? (
    <WidgetImage src={media.url(shown.art)} alt="" className="size-full" />
  ) : (
    <span className="flex size-full items-center justify-center rounded-[2px] bg-ply-4 font-display text-[22px] font-extrabold text-ink-3">
      {shown.name.slice(0, 1).toUpperCase()}
    </span>
  );

  const session = shown.live ? (
    <NumClock
      since={shown.startedAt ?? now}
      now={ctx.now}
      size="sm"
      className="text-ink-1"
    />
  ) : shown.sessionSeconds ? (
    <Num size="sm" className="text-ink-1">
      {formatDuration(shown.sessionSeconds)}
    </Num>
  ) : null;
  const sessionLabel = shown.live
    ? "this session"
    : shown.endedAt
      ? formatAgo(shown.endedAt, now)
      : "";

  const stats = (
    <dl className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1">
      {session ? <Stat value={session} label={sessionLabel} /> : null}
      {totals ? (
        <Stat
          value={
            <Num size="sm" className="text-ink-1">
              {formatDuration(totals.week)}
            </Num>
          }
          label="this week"
          className="@max-[240px]/cell:hidden"
        />
      ) : null}
      {totals && widget.size === "L" ? (
        <Stat
          value={
            <Num size="sm" className="text-ink-1">
              {formatHours(totals.all)}
            </Num>
          }
          label="all time"
        />
      ) : null}
    </dl>
  );

  const title = (
    <WidgetTitle
      title={shown.name}
      className={cn("truncate", widget.size === "L" && "text-[20px] leading-6")}
    >
      {shown.name}
    </WidgetTitle>
  );
  const line = shown.line ? (
    <p className="truncate text-ui-xs text-ink-3">{shown.line}</p>
  ) : null;

  if (widget.size === "S") {
    return (
      <Plate
        widget={widget}
        ctx={ctx}
        readout={readout}
        readoutLive={shown.live}
      >
        <div className="flex min-w-0 items-center gap-3 @max-[200px]/cell:flex-col @max-[200px]/cell:items-start @max-[200px]/cell:gap-2">
          <span className="size-14 shrink-0 @max-[200px]/cell:size-12">
            {art}
          </span>
          <div className="flex min-w-0 flex-col gap-0.5 self-stretch">
            {title}
            {line}
          </div>
        </div>
        <div className="mt-auto">{stats}</div>
      </Plate>
    );
  }

  if (widget.size === "M") {
    return (
      <Plate
        widget={widget}
        ctx={ctx}
        readout={readout}
        readoutLive={shown.live}
      >
        <span className="block aspect-[16/9] w-full shrink-0">{art}</span>
        <div className="flex min-w-0 flex-col gap-0.5">
          {title}
          {line}
        </div>
        <div className="mt-auto">{stats}</div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} readout={readout} readoutLive={shown.live}>
      <div className="flex min-h-0 flex-1 gap-4">
        <span className="aspect-[16/9] h-full shrink-0 @max-[420px]/cell:aspect-square">
          {art}
        </span>
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          {title}
          {line}
          <div className="mt-auto">{stats}</div>
        </div>
      </div>
    </Plate>
  );
}

function Stat({
  value,
  label,
  className,
}: {
  value: ReactNode;
  label: string;
  className?: string;
}) {
  return (
    <div className={cn("flex min-w-0 items-baseline gap-1.5", className)}>
      <dd className="order-1">{value}</dd>
      <dt className="order-2 truncate text-ui-xs text-ink-3">{label}</dt>
    </div>
  );
}
