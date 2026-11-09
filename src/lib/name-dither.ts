/**
 * Dither fade's fill: two inks printed through the brand's 8x8 Bayer
 * matrix, the second ink's coverage ramping from 0 to 1 left to right.
 *
 * The tile is an SVG (no canvas, so it works on the server and in tests)
 * with `cols` x 8 square cells; ProfileDisplayName sizes it to the name's
 * width (`100%`, or `n * 1ch` per letter while the name is split), so a
 * name gets about six cells per character. Cached per (inks, columns).
 */

const BAYER8 = [
  [0, 32, 8, 40, 2, 34, 10, 42],
  [48, 16, 56, 24, 50, 18, 58, 26],
  [12, 44, 4, 36, 14, 46, 6, 38],
  [60, 28, 52, 20, 62, 30, 54, 22],
  [3, 35, 11, 43, 1, 33, 9, 41],
  [51, 19, 59, 27, 49, 17, 57, 25],
  [15, 47, 7, 39, 13, 45, 5, 37],
  [63, 31, 55, 23, 61, 29, 53, 21],
] as const;

/** Rows in the tile: one Bayer period. */
export const DITHER_FADE_ROWS = 8;
/** Cells per character of the name. */
export const DITHER_FADE_CELLS_PER_CHAR = 6;
const MIN_COLS = 24;
const MAX_COLS = 256;
/** The ramp holds pure first ink for this much at the start, and pure second ink at the end. */
const RAMP_MARGIN = 0.08;

const cache = new Map<string, string>();
const CACHE_MAX = 64;

export function ditherFadeColumns(length: number): number {
  return Math.min(
    MAX_COLS,
    Math.max(MIN_COLS, Math.round(length * DITHER_FADE_CELLS_PER_CHAR)),
  );
}

/** Second-ink coverage (0-1) at column `x` of `cols`. */
export function ditherFadeCoverage(x: number, cols: number): number {
  const u = (x + 0.5) / cols;
  return Math.min(1, Math.max(0, (u - RAMP_MARGIN) / (1 - 2 * RAMP_MARGIN)));
}

/** True when cell (`x`, `y`) prints the second ink. */
export function ditherFadeLit(x: number, y: number, cols: number): boolean {
  return ditherFadeCoverage(x, cols) * 64 > BAYER8[y % 8]![x % 8]! + 0.5;
}

/**
 * The fill as a CSS `url()`: `first` everywhere, `second` on lit cells.
 * `first`/`second` are #RRGGBB.
 */
export function getDitherFadeImage(
  first: string,
  second: string,
  length: number,
): string {
  const cols = ditherFadeColumns(length);
  const key = `${first}:${second}:${cols}`;
  const cached = cache.get(key);
  if (cached) return cached;
  let d = "";
  for (let y = 0; y < DITHER_FADE_ROWS; y++)
    for (let x = 0; x < cols; x++)
      if (ditherFadeLit(x, y, cols)) d += `M${x} ${y}h1v1h-1z`;
  const svg = `<svg xmlns='http://www.w3.org/2000/svg' width='${cols}' height='${DITHER_FADE_ROWS}' viewBox='0 0 ${cols} ${DITHER_FADE_ROWS}' preserveAspectRatio='none' shape-rendering='crispEdges'><rect width='${cols}' height='${DITHER_FADE_ROWS}' fill='${first}'/><path fill='${second}' d='${d || "M0 0h0z"}'/></svg>`;
  const url = `url("data:image/svg+xml,${encodeURIComponent(svg)}")`;
  if (cache.size >= CACHE_MAX) cache.delete(cache.keys().next().value!);
  cache.set(key, url);
  return url;
}
