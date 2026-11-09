/**
 * The console's charts, in the voice-stats language (voice/DitherChart):
 * bars filled with a still 4x4 Bayer dither in accent ink, a 2px solid cap
 * carrying each value, the latest bar solid, recessive guides, every label
 * in silkscreen or Martian.
 *
 * One series per chart (so no legend: the title names it); two measures of
 * different scale are two charts, never two axes. Hover or focus a day for
 * its exact value; every chart has its numbers as a table behind a
 * disclosure, and a written summary for screen readers. Under forced
 * colours the fills go solid CanvasText.
 */

import {
  useCallback,
  useId,
  useMemo,
  useState,
  type CSSProperties,
} from "react";

import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import type { AdminDailyPoint } from "../shared/admin-api";
import { cn } from "@/lib/utils";

import { formatDay } from "./admin-format";

const CELL = 2;
const snap = (v: number) => Math.round(v / CELL) * CELL;

const BAYER4 = [
  [0, 8, 2, 10],
  [12, 4, 14, 6],
  [3, 11, 1, 9],
  [15, 7, 13, 5],
];

function DitherPattern({ id, level }: { id: string; level: number }) {
  return (
    <pattern
      id={id}
      width={CELL * 4}
      height={CELL * 4}
      patternUnits="userSpaceOnUse"
    >
      <g className="fill-(--accent-ink)">
        {BAYER4.flatMap((row, y) =>
          row.map((m, x) =>
            m < level ? (
              <rect
                key={`${x}-${y}`}
                x={x * CELL}
                y={y * CELL}
                width={CELL}
                height={CELL}
              />
            ) : null,
          ),
        )}
      </g>
    </pattern>
  );
}

const INK = "fill-(--accent-ink) forced-colors:fill-[CanvasText]";
const DITHER = "fill-(--dither) forced-colors:fill-[CanvasText]";
const GUIDE = "stroke-(--line) forced-colors:stroke-[GrayText]";
const BASELINE = "stroke-(--line-strong) forced-colors:stroke-[CanvasText]";

const rect = (x: number, y: number, w: number, h: number) =>
  w > 0 && h > 0 ? `M${x} ${y}h${w}v${h}h${-w}Z` : "";

function useWidth(): [(el: HTMLElement | null) => void, number] {
  const [width, setWidth] = useState(0);
  const ref = useCallback((el: HTMLElement | null) => {
    if (!el) return;
    setWidth(Math.floor(el.getBoundingClientRect().width));
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(([entry]) => {
      if (entry) setWidth(Math.floor(entry.contentRect.width));
    });
    observer.observe(el);
    return () => observer.disconnect();
  }, []);
  return [ref, width];
}

/** A round ceiling with three guides: 0, half, top. */
function niceCeiling(max: number): number {
  if (max <= 0) return 1;
  const pow = 10 ** Math.floor(Math.log10(max));
  for (const step of [1, 2, 2.5, 5, 10]) {
    if (step * pow >= max) return step * pow;
  }
  return 10 * pow;
}

export interface DailyBarsProps {
  title: string;
  points: AdminDailyPoint[];
  /** Formats a value for the axis, the tooltip and the table. */
  format: (value: number) => string;
  /** The headline beside the title: a total, or the latest value. */
  headline: string;
  headlineLabel: string;
  /** A line under the chart (a data caveat). */
  note?: string;
  height?: number;
  className?: string;
}

export function DailyBars({
  title,
  points,
  format,
  headline,
  headlineLabel,
  note,
  height = 132,
  className,
}: DailyBarsProps) {
  const uid = useId().replace(/:/g, "");
  const [ref, width] = useWidth();
  const [hover, setHover] = useState<number | null>(null);
  const [focusIndex, setFocusIndex] = useState<number | null>(null);
  const active = hover ?? focusIndex;

  const max = useMemo(
    () => Math.max(0, ...points.map((point) => point.value)),
    [points],
  );
  const ceiling = niceCeiling(max);
  const peak = points.reduce<AdminDailyPoint | null>(
    (best, point) => (!best || point.value > best.value ? point : best),
    null,
  );
  const total = points.reduce((sum, point) => sum + point.value, 0);
  const summary = points.length
    ? `${title}, ${formatDay(points[0]!.day)} to ${formatDay(points[points.length - 1]!.day)}. Total ${format(total)}; peak ${peak ? `${format(peak.value)} on ${formatDay(peak.day)}` : "none"}.`
    : `${title}: no data.`;

  const AXIS = 36;
  const plotW = Math.max(0, width - AXIS);
  const n = points.length;
  const slot = n ? plotW / n : 0;
  const barW =
    slot >= 4
      ? Math.max(CELL, Math.floor((slot - 2) / CELL) * CELL)
      : Math.max(1, slot - 1);
  const y = (v: number) =>
    height - snap((Math.min(v, ceiling) / ceiling) * height);

  const paths = useMemo(() => {
    let body = "";
    let caps = "";
    let latest = "";
    points.forEach((point, i) => {
      const x0 = AXIS + Math.round(i * slot + (slot - barW) / 2);
      if (point.value <= 0) return;
      const top = Math.min(y(point.value), height - 2);
      if (i === n - 1) latest = rect(x0, top, barW, height - top);
      else body += rect(x0, top, barW, height - top);
      caps += rect(x0, top, barW, 2);
    });
    return { body, caps, latest };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [points, slot, barW, height, ceiling]);

  const activePoint = active !== null ? points[active] : null;
  const tipLeft =
    active !== null && width > 0
      ? Math.min(Math.max(AXIS + (active + 0.5) * slot, 60), width - 60)
      : 0;
  const labelDays = n > 0 ? [0, Math.floor((n - 1) / 2), n - 1] : [];

  return (
    <figure
      className={cn("flex min-w-0 flex-col gap-3", className)}
      aria-label={title}
    >
      <figcaption className="flex items-baseline justify-between gap-3">
        <span className="font-display text-[15px] leading-5 font-bold text-ink-1">
          {title}
        </span>
        <span className="flex items-baseline gap-2">
          <span className="silkscreen text-ink-3">{headlineLabel}</span>
          <Num size="sm" className="text-ink-1">
            {headline}
          </Num>
        </span>
      </figcaption>
      <p className="sr-only">{summary}</p>
      <div className="relative">
        <div ref={ref} className="w-full min-w-0" style={{ height }}>
          {width > 0 ? (
            <svg
              width={width}
              height={height}
              viewBox={`0 0 ${width} ${height}`}
              className="block overflow-visible"
              shapeRendering="crispEdges"
              aria-hidden="true"
              focusable="false"
            >
              <defs>
                <DitherPattern id={`${uid}-fill`} level={7} />
              </defs>
              {[0.5, 1].map((share) => (
                <g key={share}>
                  <line
                    x1={AXIS}
                    x2={width}
                    y1={y(ceiling * share) + 0.5}
                    y2={y(ceiling * share) + 0.5}
                    strokeDasharray="2 4"
                    className={GUIDE}
                  />
                  <text
                    x={AXIS - 6}
                    y={y(ceiling * share) + 4}
                    textAnchor="end"
                    className="fill-(--ink-3) text-num-xs"
                  >
                    {format(ceiling * share)}
                  </text>
                </g>
              ))}
              <text
                x={AXIS - 6}
                y={height}
                textAnchor="end"
                className="fill-(--ink-3) text-num-xs"
              >
                0
              </text>
              {active !== null ? (
                <rect
                  x={AXIS + Math.round(active * slot)}
                  y={0}
                  width={Math.max(1, Math.round(slot))}
                  height={height}
                  className="fill-(--tint-1)"
                />
              ) : null}
              <path
                d={paths.body}
                className={DITHER}
                style={{ "--dither": `url(#${uid}-fill)` } as CSSProperties}
              />
              <path d={paths.latest} className={INK} />
              <path d={paths.caps} className={INK} />
              <line
                x1={AXIS}
                x2={width}
                y1={height - 0.5}
                y2={height - 0.5}
                className={BASELINE}
              />
            </svg>
          ) : null}
        </div>
        {/* Hit targets: a column per day, wider than its bar. */}
        <div
          className="absolute inset-y-0 right-0 flex"
          style={{ left: AXIS }}
          onPointerLeave={() => setHover(null)}
        >
          {points.map((point, i) => (
            <button
              key={point.day}
              type="button"
              tabIndex={i === (focusIndex ?? n - 1) ? 0 : -1}
              aria-label={`${formatDay(point.day)}: ${format(point.value)}`}
              onPointerEnter={() => setHover(i)}
              onFocus={() => setFocusIndex(i)}
              onBlur={() => setFocusIndex(null)}
              onKeyDown={(event) => {
                const step =
                  event.key === "ArrowRight"
                    ? 1
                    : event.key === "ArrowLeft"
                      ? -1
                      : 0;
                if (!step) return;
                event.preventDefault();
                const next = Math.max(0, Math.min(n - 1, i + step));
                const target = event.currentTarget.parentElement?.children[
                  next
                ] as HTMLElement | undefined;
                target?.focus();
              }}
              className="h-full min-w-0 flex-1 outline-offset-[-2px]"
            />
          ))}
        </div>
        {activePoint ? (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute -top-2 z-10 -translate-x-1/2 -translate-y-full rounded-[2px] bg-ink-1 px-2 py-1 whitespace-nowrap text-ply-0 shadow-float"
            style={{ left: tipLeft }}
          >
            <span className="text-ui-xs font-medium">
              {formatDay(activePoint.day)}
            </span>{" "}
            <span className="text-num-xs">{format(activePoint.value)}</span>
          </div>
        ) : null}
      </div>
      <div
        className="flex justify-between pl-9 text-num-xs text-ink-3"
        aria-hidden="true"
      >
        {labelDays.map((index) => (
          <span key={index}>
            {points[index] ? formatDay(points[index]!.day) : ""}
          </span>
        ))}
      </div>
      {note ? <p className="text-ui-xs text-ink-3">{note}</p> : null}
      <ChartTable title={title} points={points} format={format} />
    </figure>
  );
}

function ChartTable({
  title,
  points,
  format,
}: {
  title: string;
  points: AdminDailyPoint[];
  format: (value: number) => string;
}) {
  const [open, setOpen] = useState(false);
  return (
    <details
      className="group"
      onToggle={(event) => setOpen(event.currentTarget.open)}
    >
      <summary className="silkscreen inline-flex h-7 cursor-pointer items-center gap-1.5 rounded-[2px] text-ink-3 select-none pointer-fine:hover:text-ink-1">
        <Icon
          name="chevronRight"
          size="xs"
          className="transition-transform duration-(--dur-1) group-open:rotate-90"
        />
        As a table
      </summary>
      {open ? (
        <div className="mt-2 max-h-56 overflow-auto rounded-[2px] border border-line">
          <table className="w-full text-left">
            <caption className="sr-only">{title} by day</caption>
            <thead className="sticky top-0 bg-ply-2">
              <tr>
                <th
                  scope="col"
                  className="silkscreen px-3 py-1.5 font-normal text-ink-3"
                >
                  Day
                </th>
                <th
                  scope="col"
                  className="silkscreen px-3 py-1.5 text-right font-normal text-ink-3"
                >
                  {title}
                </th>
              </tr>
            </thead>
            <tbody className="text-num-xs text-ink-2">
              {[...points].reverse().map((point) => (
                <tr key={point.day} className="border-t border-line">
                  <th scope="row" className="px-3 py-1 font-medium">
                    {formatDay(point.day)}
                  </th>
                  <td className="px-3 py-1 text-right">
                    {format(point.value)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}
    </details>
  );
}

/** A ranked list with one dithered bar per row (top servers). */
export function RankedBars({
  rows,
  format,
}: {
  rows: Array<{ id: string; label: string; value: number; detail?: string }>;
  format: (value: number) => string;
}) {
  const uid = useId().replace(/:/g, "");
  const max = Math.max(1, ...rows.map((row) => row.value));
  return (
    <div className="relative">
      <svg
        width="0"
        height="0"
        className="absolute"
        aria-hidden="true"
        focusable="false"
      >
        <defs>
          <DitherPattern id={`${uid}-rank`} level={7} />
        </defs>
      </svg>
      <ol className="flex flex-col gap-3">
        {rows.map((row, index) => {
          const share = (row.value / max) * 100;
          return (
            <li
              key={row.id}
              className="grid grid-cols-[20px_minmax(0,1fr)] gap-x-3 gap-y-1.5"
            >
              <Num className="row-span-2 pt-0.5 text-ink-3">
                {String(index + 1).padStart(2, "0")}
              </Num>
              <span className="flex min-w-0 items-baseline gap-3">
                <span className="min-w-0 flex-1 truncate text-ui-row font-medium text-ink-1">
                  {row.label}
                </span>
                {row.detail ? (
                  <span className="hidden shrink-0 text-ui-xs text-ink-3 sm:inline">
                    {row.detail}
                  </span>
                ) : null}
                <Num className="w-14 shrink-0 text-right text-ink-1">
                  {format(row.value)}
                </Num>
              </span>
              <svg
                width="100%"
                height="8"
                aria-hidden="true"
                focusable="false"
                shapeRendering="crispEdges"
                className="block"
              >
                <rect width="100%" height="8" className="fill-(--tint-1)" />
                <rect
                  width={`${share}%`}
                  height="8"
                  className={DITHER}
                  style={{ "--dither": `url(#${uid}-rank)` } as CSSProperties}
                />
                <rect
                  x={`${share}%`}
                  width="2"
                  height="8"
                  transform="translate(-2 0)"
                  className={INK}
                />
              </svg>
            </li>
          );
        })}
      </ol>
    </div>
  );
}
