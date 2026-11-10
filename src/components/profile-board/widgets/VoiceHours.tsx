/**
 * Voice hours: time in calls this month (or the last 30 days), day by day.
 *
 * The chart is the board's sanctioned dither (spec §2): one still Bayer
 * mask of square 3px cells over the accent ink, built once per box size and
 * data, so a theme change repaints nothing and nothing runs at rest. Each
 * day is a bar whose light thins toward its top under a solid cap; a day
 * with no voice is a dotted baseline, a day still to come is blank. Hover
 * names a day in the axis row; a hidden table carries every value for
 * screen readers.
 */

import { useMemo, useRef, useState, type PointerEvent } from "react";

import { bayerLit, cellMask } from "@/components/ui/dither";
import { Num } from "@/components/ui/num";
import { useBoxSize } from "@/components/ui/use-dither-geometry";
import { cn } from "@/lib/utils";

import { formatDuration, formatHours } from "../board-format";
import { Plate, WidgetEmpty, type WidgetProps } from "./shared";

const DAY_MS = 86_400_000;

// Days are UTC days (daily_activity.day), so their labels are too.
const UTC_DAY = new Intl.DateTimeFormat(undefined, {
  day: "numeric",
  month: "short",
  timeZone: "UTC",
});
const UTC_MONTH = new Intl.DateTimeFormat(undefined, {
  month: "long",
  timeZone: "UTC",
});
const formatDayMonth = (at: number) => UTC_DAY.format(at);
const formatMonth = (at: number) => UTC_MONTH.format(at);
const CELL = 3;

export function VoiceHoursWidget({
  widget,
  data,
  ctx,
}: WidgetProps<"voice_hours">) {
  const voice = data.voice_hours;
  const period = voice?.period ?? widget.cfg.period;
  const start = voice ? Date.parse(`${voice.start_day}T00:00:00Z`) : NaN;
  const readout = (
    <span className="silkscreen">
      {period === "month" && Number.isFinite(start)
        ? formatMonth(start)
        : "30 days"}
    </span>
  );

  if (!voice) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <WidgetEmpty
          icon="mic"
          title="No voice hours yet"
          hint="Time in calls adds up here, day by day."
        />
      </Plate>
    );
  }

  const slots =
    period === "month" && Number.isFinite(start)
      ? daysInMonth(start)
      : Math.max(voice.days.length, 30);
  const previousLabel =
    period === "month" && Number.isFinite(start)
      ? formatMonth(start - DAY_MS)
      : "Previous 30 days";
  const empty = voice.total_seconds <= 0;

  const total = (
    <p
      className={cn(
        "font-silk leading-none font-semibold tracking-[-0.02em] text-ink-1 tabular-nums [font-stretch:90%]",
        widget.size === "S" ? "text-[28px]" : "text-[36px]",
      )}
    >
      {hoursNumber(voice.total_seconds)}
      <span className="ml-1 text-[13px] font-medium tracking-normal text-ink-3">
        h
      </span>
    </p>
  );
  const previous =
    voice.previous_seconds > 0 ? (
      <p className="text-right text-ui-xs text-ink-3 @max-[240px]/cell:hidden">
        {previousLabel}
        <br />
        <Num className="text-ink-2">{formatHours(voice.previous_seconds)}</Num>
      </p>
    ) : null;

  const chart = (
    <VoiceChart
      days={voice.days}
      slots={slots}
      start={start}
      height={widget.size === "S" ? 30 : widget.size === "M" ? 108 : 66}
    />
  );

  const rows = (
    <dl className="flex flex-col border-t border-line">
      <StatRow label="Streamed" value={formatDuration(voice.stream_seconds)} />
      {voice.longest_seconds > 0 ? (
        <StatRow
          label="Longest call"
          value={formatDuration(voice.longest_seconds)}
        />
      ) : null}
      <StatRow
        label="Busiest day"
        value={formatDuration(Math.max(0, ...voice.days))}
      />
    </dl>
  );

  if (empty) {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <div className="flex items-end justify-between gap-2">
          {total}
          {widget.size !== "S" ? previous : null}
        </div>
        {chart}
        <p
          className={cn(
            "text-ui-xs text-ink-3",
            widget.size === "S" && "sr-only",
          )}
        >
          {period === "month"
            ? "No calls yet this month."
            : "No calls in the last 30 days."}
        </p>
      </Plate>
    );
  }

  if (widget.size === "S") {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        {total}
        <div className="mt-auto">{chart}</div>
      </Plate>
    );
  }

  if (widget.size === "M") {
    return (
      <Plate widget={widget} ctx={ctx} readout={readout}>
        <div className="flex items-end justify-between gap-2">
          {total}
          {previous}
        </div>
        {chart}
        <div className="mt-auto">{rows}</div>
      </Plate>
    );
  }

  return (
    <Plate widget={widget} ctx={ctx} readout={readout}>
      <div className="flex min-h-0 flex-1 gap-5">
        <div className="flex w-[30%] shrink-0 flex-col justify-between">
          {total}
          <div className="text-left [&_p]:text-left">{previous}</div>
        </div>
        <div className="flex min-w-0 flex-1 flex-col justify-end">{chart}</div>
      </div>
    </Plate>
  );
}

function StatRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex h-8 items-center justify-between gap-2 border-b border-line">
      <dt className="truncate text-ui-xs text-ink-3">{label}</dt>
      <dd>
        <Num size="sm" className="text-ink-2">
          {value}
        </Num>
      </dd>
    </div>
  );
}

/** "38.4" for 38.4 hours; whole hours from 100. */
function hoursNumber(seconds: number): string {
  const h = Math.max(0, seconds) / 3600;
  return h >= 100 ? String(Math.round(h)) : h.toFixed(1);
}

function daysInMonth(startUtc: number): number {
  const date = new Date(startUtc);
  return new Date(
    Date.UTC(date.getUTCFullYear(), date.getUTCMonth() + 1, 0),
  ).getUTCDate();
}

/** The still bar chart. Exported for the tests. */
export function VoiceChart({
  days,
  slots,
  start,
  height,
}: {
  /** Seconds per day, oldest first; days past the end are still to come. */
  days: number[];
  slots: number;
  /** UTC ms of the first day (NaN when unknown). */
  start: number;
  height: number;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const box = useBoxSize(hostRef);
  const [hover, setHover] = useState<number | null>(null);
  const max = Math.max(...days, 1);
  const cols = box ? Math.floor(box.width / CELL) : 0;
  const rows = Math.max(4, Math.floor(height / CELL));
  const slotCells = cols / Math.max(1, slots);

  const mask = useMemo(() => {
    if (cols <= 0) return null;
    const bars = days.map((seconds) =>
      seconds > 0 ? Math.max(2, Math.round((seconds / max) * rows)) : 0,
    );
    return cellMask(cols, rows, (x, y) => {
      const slot = Math.floor(x / slotCells);
      if (slot >= days.length) return false;
      const from = Math.floor(slot * slotCells);
      const to = Math.floor((slot + 1) * slotCells) - 1;
      // One cell of ground between bars once they are wider than one cell.
      if (to - from >= 1 && x === to) return false;
      const bar = bars[slot] ?? 0;
      const fromBottom = rows - 1 - y;
      if (bar === 0) return fromBottom === 0 && (x - from) % 2 === 0;
      if (fromBottom >= bar) return false;
      if (fromBottom === bar - 1) return true;
      return bayerLit(x, y, 1 - (fromBottom / bar) * 0.55);
    });
  }, [cols, rows, days, max, slotCells]);

  const onMove = (event: PointerEvent<HTMLDivElement>) => {
    if (!box || event.pointerType === "touch") return;
    const x = event.clientX - event.currentTarget.getBoundingClientRect().left;
    const slot = Math.floor(x / CELL / slotCells);
    setHover(slot >= 0 && slot < days.length ? slot : null);
  };

  const dayLabel = (index: number) =>
    Number.isFinite(start)
      ? formatDayMonth(start + index * DAY_MS)
      : `Day ${index + 1}`;
  const last = days.length - 1;
  const endLabel =
    slots > days.length && Number.isFinite(start)
      ? formatDayMonth(start + (slots - 1) * DAY_MS)
      : "Today";

  return (
    <div className="flex flex-col gap-1.5">
      <div
        ref={hostRef}
        className="relative w-full"
        style={{ height: rows * CELL }}
        onPointerMove={onMove}
        onPointerLeave={() => setHover(null)}
      >
        {hover !== null && box ? (
          <span
            aria-hidden="true"
            className="absolute inset-y-0 bg-tint-2"
            style={{
              left: Math.floor(hover * slotCells) * CELL - 1,
              width: Math.max(1, Math.floor(slotCells)) * CELL + 1,
            }}
          />
        ) : null}
        {mask ? (
          <span
            aria-hidden="true"
            data-dither=""
            data-slot="voice-chart"
            className="rm-dither absolute top-0 left-0"
            style={{
              width: cols * CELL,
              height: rows * CELL,
              backgroundColor: "var(--accent-ink)",
              maskImage: mask,
              WebkitMaskImage: mask,
              maskSize: "100% 100%",
              WebkitMaskSize: "100% 100%",
              maskRepeat: "no-repeat",
              WebkitMaskRepeat: "no-repeat",
            }}
          />
        ) : null}
        <table className="sr-only">
          <caption>Voice time per day</caption>
          <tbody>
            {days.map((seconds, index) => (
              <tr key={index}>
                <th scope="row">{dayLabel(index)}</th>
                <td>{formatDuration(seconds)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
      <div
        aria-hidden="true"
        className="flex items-center justify-between gap-2"
      >
        {hover !== null ? (
          <span className="silkscreen text-ink-1">
            {dayLabel(hover)} · {formatDuration(days[hover] ?? 0)}
          </span>
        ) : (
          <>
            <span className="silkscreen text-ink-3">{dayLabel(0)}</span>
            <span className="silkscreen text-ink-3">
              {last >= 0 && slots === days.length ? "Today" : endLabel}
            </span>
          </>
        )}
      </div>
    </div>
  );
}
