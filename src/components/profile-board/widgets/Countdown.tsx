/**
 * Countdown: days, hours and minutes to a date the owner set. The readout
 * ticks once a minute in its own leaf (aligned to the minute, paused while
 * the app is inactive), so nothing else on the board re-renders.
 */

import { useEffect, useState } from "react";

import { useAppActive } from "@/hooks/useAppActive";
import { cn } from "@/lib/utils";

import { countdownParts, formatDateTime } from "../board-format";
import { Plate, WidgetEmpty, WidgetTitle, type WidgetProps } from "./shared";

export function CountdownWidget({ widget, ctx }: WidgetProps<"countdown">) {
  const { title, at } = widget.cfg;
  const target = at ? Date.parse(at) : NaN;

  if (!Number.isFinite(target)) {
    return (
      <Plate widget={widget} ctx={ctx}>
        <WidgetEmpty
          icon="timer"
          title={ctx.isOwner ? "Pick a date" : "No date set"}
          hint={
            ctx.isOwner
              ? "Set what you're counting down to in Edit board."
              : undefined
          }
        />
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx}>
      {title ? (
        <WidgetTitle
          className={cn(
            "line-clamp-2 text-balance",
            widget.size === "M" && "text-[20px] leading-6",
          )}
        >
          {title}
        </WidgetTitle>
      ) : null}
      <CountdownReadout
        target={target}
        now={ctx.now}
        big={widget.size === "M"}
      />
      <p className="mt-auto truncate text-ui-xs text-ink-3">
        <time dateTime={at ?? undefined}>{formatDateTime(at!)}</time>
      </p>
    </Plate>
  );
}

/** The ticking leaf. Pass `now` to freeze it (fixtures, tests). */
export function CountdownReadout({
  target,
  now: frozen,
  big = false,
}: {
  target: number;
  now?: number;
  big?: boolean;
}) {
  const active = useAppActive();
  const [tick, setTick] = useState(() => frozen ?? Date.now());

  useEffect(() => {
    if (frozen !== undefined || !active) return;
    let id: ReturnType<typeof setTimeout>;
    const schedule = () => {
      const current = Date.now();
      setTick(current);
      if (current >= target) return;
      id = setTimeout(schedule, 60_000 - (current % 60_000) + 50);
    };
    schedule();
    return () => clearTimeout(id);
  }, [active, frozen, target]);

  const parts = countdownParts(target, frozen ?? tick);
  const numeral = cn(
    "font-silk font-semibold tracking-[-0.02em] text-ink-1 tabular-nums [font-stretch:90%]",
    big
      ? "text-[44px] leading-[44px]"
      : "text-[36px] leading-9 @max-[200px]/cell:text-[28px] @max-[200px]/cell:leading-[30px]",
  );
  const unit = "ml-0.5 text-[13px] font-medium tracking-normal text-ink-3";

  if (parts.passed) {
    return (
      <p
        role="timer"
        className="font-display text-[22px] leading-7 font-bold tracking-[-0.01em] text-ink-2"
      >
        It's here
      </p>
    );
  }

  const label = `${parts.days} days, ${parts.hours} hours and ${parts.minutes} minutes to go`;
  if (big) {
    return (
      <dl role="timer" aria-label={label} className="grid grid-cols-3 gap-2">
        {(
          [
            ["Days", parts.days],
            ["Hrs", parts.hours],
            ["Min", parts.minutes],
          ] as const
        ).map(([name, value]) => (
          <div key={name} className="flex min-w-0 flex-col gap-1">
            <dd className={numeral}>{String(value).padStart(2, "0")}</dd>
            <dt className="silkscreen order-first text-ink-3">{name}</dt>
          </div>
        ))}
      </dl>
    );
  }

  return (
    <p
      role="timer"
      aria-label={label}
      className={cn(numeral, "whitespace-nowrap")}
    >
      {parts.days > 0 ? (
        <>
          {parts.days}
          <span className={unit}>d</span>{" "}
        </>
      ) : null}
      {String(parts.hours).padStart(2, "0")}
      <span className={unit}>h</span>
      {parts.days === 0 ? (
        <>
          {" "}
          {String(parts.minutes).padStart(2, "0")}
          <span className={unit}>m</span>
        </>
      ) : (
        <span className="ml-1.5 text-[13px] font-medium tracking-normal text-ink-3 @max-[200px]/cell:hidden">
          {String(parts.minutes).padStart(2, "0")}m
        </span>
      )}
    </p>
  );
}
