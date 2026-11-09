/**
 * YappyPose: Yappy in the three "too big" poses (toobig spec, 2026-11-09),
 * on YapBark's 32-cell grid and printed through the same accent dither.
 *
 *   stuffed   jaw wide open on a file that sticks out past the snout (the
 *             grid widens to 40 cells), three warning strain ticks overhead
 *   sad       ear drooped back and down, eye half shut, a one-cell tear in
 *             the live ink, jaw shut
 *   squashed  head pressed flat under a file page, with warning sweat ticks
 *
 * The geometry is the approved mockup's (toobig.html), cell for cell. Still:
 * nothing here moves. Same contract as YapBark: aria-hidden, no pointer
 * events, alpha masks over role colours, gone under forced colours
 * (`.rm-dither`), so the card's words always carry the meaning.
 */

import { bayerLit, cellMask } from "@/components/ui/dither";
import { CellLayer } from "@/components/ui/yap-bark";
import {
  JAW,
  rect,
  YAP_PARTS,
  yapLight,
  type Cell,
} from "@/components/ui/yap-geometry";
import type { YappyPose as Pose } from "@/lib/too-big-copy";
import { cn } from "@/lib/utils";

const { ear, innerEar, skull, snout, neck, eye, nose } = YAP_PARTS;

const any =
  (...cells: Cell[]): Cell =>
  (x, y) =>
    cells.some((cell) => cell(x, y));

const not =
  (region: Cell, cut: Cell): Cell =>
  (x, y) =>
    region(x, y) && !cut(x, y);

/** Shift a drawing by (`ox`, `oy`) on its canvas. */
const at =
  (ox: number, oy: number) =>
  (cell: Cell): Cell =>
  (x, y) =>
    cell(x - ox, y - oy);

type Ink = "accent" | "file" | "fold" | "line" | "warning" | "tear";

const INK: Record<Ink, string> = {
  accent: "var(--accent-ink)",
  // A page a shade under ink-1, so its ruled lines (ink-3) still read.
  file: "color-mix(in srgb, var(--ink-1) 82%, var(--ink-3))",
  fold: "var(--ink-2)",
  line: "var(--ink-3)",
  warning: "var(--warning-ink)",
  tear: "var(--live-ink)",
};

interface Layer {
  part: string;
  ink: Ink;
  cells: Cell;
}

interface PoseDrawing {
  cols: number;
  rows: number;
  layers: Layer[];
}

/**
 * The dog's cells, lit the way YapBark lights the mark. `shift` places it on
 * the canvas; the light and the dither run in the mark's own coordinates.
 */
function dog(shift: (cell: Cell) => Cell, body: Cell): Cell {
  return shift((x, y) => body(x, y) && bayerLit(x, y, yapLight(x, y)));
}

function stuffed(): PoseDrawing {
  const s = at(0, 2);
  const fold: Cell = (x, y) =>
    x >= 32 && x < 36 && y >= 17 && y < 21 && x - 32 <= y - 17;
  const lines = any(rect(26, 20, 7, 1), rect(26, 22, 8, 1));
  return {
    cols: 40,
    rows: 34,
    layers: [
      {
        part: "body",
        ink: "accent",
        cells: dog(
          s,
          not(any(ear, skull, snout, neck, JAW.open), any(innerEar, eye, nose)),
        ),
      },
      {
        part: "file",
        ink: "file",
        cells: s(not(rect(24, 17, 12, 9), any(fold, lines))),
      },
      { part: "fold", ink: "fold", cells: s(fold) },
      { part: "lines", ink: "line", cells: s(lines) },
      {
        part: "strain",
        ink: "warning",
        cells: s(any(rect(10, 1, 1, 3), rect(13, 0, 1, 3), rect(16, 1, 1, 3))),
      },
    ],
  };
}

function sad(): PoseDrawing {
  const s = at(1, 0);
  const droopEar: Cell = (x, y) =>
    y >= 10 && y < 16 && x >= 0 && x < 6 && y - 10 <= x + 1;
  // The lid is the top half of the eye, left filled; the lower half is cut.
  const halfEye = rect(13, 15, 4, 2);
  return {
    cols: 34,
    rows: 34,
    layers: [
      {
        part: "body",
        ink: "accent",
        cells: dog(
          s,
          not(
            any(droopEar, skull, snout, neck, JAW.closed),
            any(halfEye, nose),
          ),
        ),
      },
      {
        part: "tear",
        ink: "tear",
        cells: s(any(rect(14, 18, 1, 1), rect(14, 20, 1, 2))),
      },
    ],
  };
}

function squashed(): PoseDrawing {
  const s = at(1, 0);
  const flatSkull = rect(2, 18, 19, 9);
  const flatSnout = rect(21, 20, 11, 5);
  const flatJaw = rect(17, 25, 13, 2);
  const flatNeck = rect(4, 27, 14, 5);
  const flatEye = rect(13, 20, 4, 2);
  const flatNose = rect(29, 20, 3, 2);
  const page = rect(3, 3, 20, 14);
  const fold: Cell = (x, y) =>
    x >= 18 && x < 23 && y >= 3 && y < 8 && x - 18 <= y - 3;
  const lines = any(
    rect(6, 8, 10, 1),
    rect(6, 10, 13, 1),
    rect(6, 12, 13, 1),
    rect(6, 14, 9, 1),
  );
  return {
    cols: 34,
    rows: 34,
    layers: [
      {
        part: "body",
        ink: "accent",
        cells: dog(
          s,
          not(
            any(flatSkull, flatSnout, flatJaw, flatNeck),
            any(flatEye, flatNose),
          ),
        ),
      },
      { part: "file", ink: "file", cells: s(not(page, any(fold, lines))) },
      { part: "fold", ink: "fold", cells: s(fold) },
      { part: "lines", ink: "line", cells: s(lines) },
      {
        part: "sweat",
        ink: "warning",
        cells: s(
          any(
            rect(0, 14, 2, 1),
            rect(24, 14, 2, 1),
            rect(1, 12, 1, 1),
            rect(25, 12, 1, 1),
          ),
        ),
      },
    ],
  };
}

const DRAWINGS: Record<Pose, () => PoseDrawing> = { stuffed, sad, squashed };

/** Each pose's masks, built once per page. */
const built = new Map<
  Pose,
  {
    cols: number;
    rows: number;
    layers: { part: string; ink: Ink; mask: string }[];
  }
>();

function poseMasks(pose: Pose) {
  let entry = built.get(pose);
  if (!entry) {
    const { cols, rows, layers } = DRAWINGS[pose]();
    entry = {
      cols,
      rows,
      layers: layers.map(({ part, ink, cells }) => ({
        part,
        ink,
        mask: cellMask(cols, rows, cells),
      })),
    };
    built.set(pose, entry);
  }
  return entry;
}

export function YappyPose({
  pose,
  cell = 3,
  className,
}: {
  pose: Pose;
  /** CSS px per cell. */
  cell?: number;
  className?: string;
}) {
  const { cols, rows, layers } = poseMasks(pose);
  return (
    <span
      aria-hidden="true"
      data-dither=""
      data-slot="yappy-pose"
      data-pose={pose}
      className={cn("rm-dither relative block shrink-0", className)}
      style={{ width: cols * cell, height: rows * cell }}
    >
      {layers.map(({ part, ink, mask }) => (
        <CellLayer key={part} part={part} mask={mask} color={INK[ink]} />
      ))}
    </span>
  );
}
