/**
 * Yappy's geometry on the 32-cell grid (branding/yappy-master.svg): the
 * parts YapBark prints and the "too big" poses (`ui/yappy-pose`) redraw.
 * Pure data, no React.
 */

export const YAP_CELLS = 32;

/** A cell predicate on the grid: true where the drawing is. */
export type Cell = (x: number, y: number) => boolean;

export const rect =
  (x0: number, y0: number, w: number, h: number): Cell =>
  (x, y) =>
    x >= x0 && x < x0 + w && y >= y0 && y < y0 + h;

// The master's polygons, sampled at cell centres.
const ear: Cell = (x, y) => x >= 2 && y < 10 && x < y + 2;
const innerEar: Cell = (x, y) => x >= 4 && y >= 5 && y < 9 && x < y - 1;
const skull = rect(2, 10, 19, 14);
const snout = rect(21, 12, 11, 7);
const neck = rect(4, 24, 14, 8);
const eye = rect(13, 13, 4, 4);
const nose = rect(29, 12, 3, 3);

/** Ear, skull, snout and neck; inner ear, eye and nose are the cut-outs. */
export const YAP_PARTS = {
  ear,
  innerEar,
  skull,
  snout,
  neck,
  eye,
  nose,
} as const;

/** The lower jaw per frame: it rises and lengthens as it shuts. */
export const JAW = {
  closed: rect(17, 19, 13, 4),
  half: rect(17, 20, 12, 4),
  open: rect(17, 22, 11, 4),
} as const;

/** Light across the mark: dense at the ear, thinning toward the jaw tip. */
export function yapLight(x: number, y: number): number {
  const d = (x + y) / (2 * (YAP_CELLS - 1));
  return 0.98 - d * 0.3;
}
