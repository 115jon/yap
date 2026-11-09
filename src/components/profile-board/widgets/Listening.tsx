/**
 * Listening: the owner's last Listen Together session. Another user's live
 * room state is not on this client, so the widget shows the last session
 * the server recorded (no LED: nothing here is live).
 */

import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import { cn } from "@/lib/utils";

import { formatAgo } from "../board-format";
import { Plate, WidgetEmpty, WidgetTitle, type WidgetProps } from "./shared";

export function ListeningWidget({
  widget,
  data,
  ctx,
}: WidgetProps<"listening">) {
  const last = data.last_listen;
  const now = ctx.at;

  if (!last || last.tracks <= 0) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="headphones"
          title="No Listen Together yet"
          hint={
            widget.size === "M"
              ? "Sessions of 3 tracks or more show up here."
              : undefined
          }
        />
      </Plate>
    );
  }

  const readout = last.ended_at ? (
    <Num className="text-ink-3">{formatAgo(last.ended_at, now)}</Num>
  ) : null;
  const tile = (
    <span
      aria-hidden="true"
      className="flex size-12 shrink-0 items-center justify-center rounded-[2px] bg-ply-4 text-ink-2 @min-[240px]/cell:size-14"
    >
      <Icon name="music" size="xl" />
    </span>
  );

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <div className="flex min-w-0 items-center gap-3 @max-[200px]/cell:flex-col @max-[200px]/cell:items-start @max-[200px]/cell:gap-2">
        {tile}
        <div className="flex min-w-0 flex-col gap-0.5">
          <p className="text-ui-xs text-ink-3">Last listened together</p>
          <WidgetTitle>
            <Num size="sm" className="text-[16px] leading-5 text-ink-1">
              {last.tracks}
            </Num>{" "}
            {last.tracks === 1 ? "track" : "tracks"}
          </WidgetTitle>
        </div>
      </div>
      {last.top_track ? (
        <div
          className={cn(
            "mt-auto flex min-w-0 flex-col gap-0.5 border-t border-line pt-2.5",
            widget.size === "S" && "@max-[200px]/cell:hidden",
          )}
        >
          <span className="silkscreen text-ink-3">Most played</span>
          <p
            className="truncate text-ui-sm font-medium text-ink-1"
            title={last.top_track}
          >
            {last.top_track}
          </p>
        </div>
      ) : null}
    </Plate>
  );
}
