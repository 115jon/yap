import { MalformedFile, type Patch } from "./bytes";
import type { MetadataKind } from "./exif";

/**
 * Most patches one scan may produce. A real file needs a handful; a file
 * built from millions of tiny metadata blocks would otherwise cost a Patch
 * object each, enough to run a Worker out of memory. Past this the scan
 * stops as malformed and the upload is flagged.
 */
export const MAX_PATCHES = 4096;

/** What a format scanner collects as it walks a file. */
export interface ScanState {
  kinds: Set<MetadataKind>;
  patches: Patch[];
}

export function addKinds(state: ScanState, kinds: Iterable<MetadataKind>) {
  for (const kind of kinds) state.kinds.add(kind);
}

function push(state: ScanState, patch: Patch) {
  if (state.patches.length >= MAX_PATCHES) {
    throw new MalformedFile("too many metadata blocks");
  }
  state.patches.push(patch);
}

/**
 * Zeroes `length` bytes at `offset`. A range that starts where the last
 * patch's zeros end (or inside them) extends that patch instead of adding
 * one.
 */
export function zero(state: ScanState, offset: number, length: number) {
  if (length <= 0) return;
  const last = state.patches[state.patches.length - 1];
  if (last) {
    const zerosFrom = last.offset + (last.bytes?.byteLength ?? 0);
    const end = last.offset + last.length;
    if (offset >= zerosFrom && offset <= end) {
      last.length = Math.max(end, offset + length) - last.offset;
      return;
    }
  }
  push(state, { offset, length });
}

/**
 * Overwrites `length` bytes at `offset` with `bytes`, then zeros. `bytes`
 * is held, not copied: pass a shared constant where one fits.
 */
export function write(
  state: ScanState,
  offset: number,
  length: number,
  bytes: Uint8Array,
) {
  if (length <= 0) return;
  push(state, { offset, length, bytes });
}
