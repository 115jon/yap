import type { Patch } from "./bytes";
import type { MetadataKind } from "./exif";

/** What a format scanner collects as it walks a file. */
export interface ScanState {
  kinds: Set<MetadataKind>;
  patches: Patch[];
}

export function addKinds(state: ScanState, kinds: Iterable<MetadataKind>) {
  for (const kind of kinds) state.kinds.add(kind);
}

export function zero(state: ScanState, offset: number, length: number) {
  if (length > 0) state.patches.push({ offset, length });
}
