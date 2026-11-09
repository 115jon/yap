/**
 * YapBark: Yappy, the Yap mark, printed on a 32-cell grid through the
 * accent dither, barking while something loads.
 *
 * The drawing is the master icon's geometry (branding/yappy-master.svg,
 * drawn on the same 32-unit grid): ear, skull, snout, neck and a lower jaw,
 * with the eye, nose and inner ear cut out. The jaw has three frames,
 * closed, half and open (the master pose), and two short "yap" ticks
 * show beside the open mouth.
 *
 * At rest it is one still frame, the open jaw of the icon. With `barking`
 * the jaw steps closed, half, open, half twice ("yap yap"), then rests
 * shut, on a stepped timeline (no easing; motion.css `rm-bark`). It barks
 * only while its caller is loading, and stops for reduced motion (the
 * still frame stays). Same contract as `ui/dither`: aria-hidden, no
 * pointer events, alpha masks over the accent role colour, and gone under
 * forced colours (`.rm-dither`), where the words beside it carry the state.
 */

import * as React from "react";

import { bayerLit, cellMask } from "@/components/ui/dither";
import {
  JAW,
  YAP_CELLS,
  YAP_PARTS,
  yapLight,
  type Cell,
} from "@/components/ui/yap-geometry";
import { cn } from "@/lib/utils";

export { YAP_CELLS } from "@/components/ui/yap-geometry";

const { ear, innerEar, skull, snout, neck, eye, nose } = YAP_PARTS;

/** Two 45-degree ticks off the snout and the jaw tip. */
const TICK_CELLS = [
  [29, 10],
  [30, 9],
  [31, 8],
  [29, 28],
  [30, 29],
  [31, 30],
] as const;
const ticks: Cell = (x, y) =>
  TICK_CELLS.some(([tx, ty]) => tx === x && ty === y);

const cutout: Cell = (x, y) => innerEar(x, y) || eye(x, y) || nose(x, y);

/** The head and neck without the jaw, features cut out. */
const yapBody: Cell = (x, y) =>
  (ear(x, y) || skull(x, y) || snout(x, y) || neck(x, y)) && !cutout(x, y);

const lit =
  (region: Cell): Cell =>
  (x, y) =>
    region(x, y) && !cutout(x, y) && bayerLit(x, y, yapLight(x, y));

const masks = new Map<string, string>();
function mask(key: string, build: () => Cell): string {
  let m = masks.get(key);
  if (!m) {
    m = cellMask(YAP_CELLS, YAP_CELLS, build());
    masks.set(key, m);
  }
  return m;
}

const LAYERS = [
  ["body", () => lit(yapBody)],
  ["closed", () => lit(JAW.closed)],
  ["half", () => lit(JAW.half)],
  ["open", () => lit(JAW.open)],
  ["ticks", () => ticks],
] as const;

type YapBarkProps = {
  /** Bark (a stepped loop) while true; a still frame otherwise. */
  barking?: boolean;
  /** Cell size in px. Without it the mark fills its box. */
  cell?: number;
  className?: string;
};

function YapBark({ barking = false, cell, className }: YapBarkProps) {
  const px = cell ? YAP_CELLS * cell : undefined;
  return (
    <span
      aria-hidden="true"
      data-dither=""
      data-slot="yap-bark"
      data-barking={barking ? "" : undefined}
      className={cn(
        "rm-dither rm-bark relative block shrink-0",
        !px && "aspect-square w-full",
        className,
      )}
      style={px ? { width: px, height: px } : undefined}
    >
      {LAYERS.map(([part, cells]) => (
        <CellLayer key={part} part={part} mask={mask(part, cells)} />
      ))}
    </span>
  );
}

/**
 * One layer of a cell drawing: `mask` (a `cellMask`) over a role colour,
 * stretched to the box. Shared by YapBark and the Yappy poses.
 */
export function CellLayer({
  part,
  mask,
  color = "var(--accent-ink)",
}: {
  part: string;
  mask: string;
  color?: string;
}) {
  return (
    <span
      data-part={part}
      className="absolute inset-0"
      style={
        {
          backgroundColor: color,
          maskImage: mask,
          WebkitMaskImage: mask,
          maskSize: "100% 100%",
          WebkitMaskSize: "100% 100%",
          maskRepeat: "no-repeat",
          WebkitMaskRepeat: "no-repeat",
        } as React.CSSProperties
      }
    />
  );
}

export { YapBark };
