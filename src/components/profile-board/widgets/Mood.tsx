/**
 * Mood: Yappy in a pose (the still bark frame or a YappyPose) beside one
 * line about how it's going, and when that line clears. A cleared mood
 * keeps Yappy, quietly.
 */

import { YapBark } from "@/components/ui/yap-bark";
import { YappyPose } from "@/components/ui/yappy-pose";
import { cn } from "@/lib/utils";

import { formatDateTime } from "../board-format";
import { Plate, type WidgetProps } from "./shared";

const CLOCK = new Intl.DateTimeFormat(undefined, {
  hour: "2-digit",
  minute: "2-digit",
});

export function MoodWidget({ widget, ctx }: WidgetProps<"mood">) {
  const { pose, text, clears_at } = widget.cfg;
  const now = ctx.at;
  const clearsAt = clears_at ? Date.parse(clears_at) : NaN;
  const cleared = !text || (Number.isFinite(clearsAt) && clearsAt <= now);
  const big = widget.size === "M";
  // YappyPose's grid is 32 cells (40 for "stuffed"); match the bark's width.
  const cell = big ? 3 : 2;

  const yappy =
    pose === "bark" ? (
      <YapBark
        cell={cell}
        className={cn(!big && "@max-[200px]/cell:size-12!")}
      />
    ) : (
      <YappyPose
        pose={pose}
        cell={pose === "stuffed" ? (cell * 32) / 40 : cell}
        className={cn(
          !big &&
            "@max-[200px]/cell:origin-top-left @max-[200px]/cell:scale-75",
        )}
      />
    );

  return (
    <Plate widget={widget} ctx={ctx} bodyClassName="justify-center">
      <div
        className={cn(
          "flex min-w-0 items-center gap-3.5",
          big
            ? "flex-col items-start gap-4"
            : "@max-[200px]/cell:flex-col @max-[200px]/cell:items-start @max-[200px]/cell:gap-2",
          cleared && "opacity-60",
        )}
      >
        <span className="shrink-0">{yappy}</span>
        <div className="flex min-w-0 flex-col gap-1">
          {cleared ? (
            <p className="text-ui-sm text-ink-2">
              {ctx.isOwner ? "No mood set" : "No mood right now"}
            </p>
          ) : (
            <>
              <p
                className={cn(
                  "font-display font-bold tracking-[-0.01em] text-balance break-words text-ink-1",
                  big
                    ? "text-[22px] leading-[26px]"
                    : "text-[17px] leading-[21px] @max-[200px]/cell:text-[15px] @max-[200px]/cell:leading-[19px]",
                )}
              >
                {text}
              </p>
              {Number.isFinite(clearsAt) ? (
                <p className="text-ui-xs text-ink-3">
                  Clears {clearsLabel(clearsAt, now)}
                </p>
              ) : null}
            </>
          )}
        </div>
      </div>
    </Plate>
  );
}

function clearsLabel(at: number, now: number): string {
  const date = new Date(at);
  if (
    at - now <= 86_400_000 &&
    date.getHours() === 0 &&
    date.getMinutes() === 0
  ) {
    return "at midnight";
  }
  if (date.toDateString() === new Date(now).toDateString()) {
    return `at ${CLOCK.format(at)}`;
  }
  return formatDateTime(new Date(at).toISOString());
}
