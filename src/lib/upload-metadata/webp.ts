// ── WebP ────────────────────────────────────────────────────────────────────
//
// The EXIF and XMP chunks of an extended (VP8X) WebP are renamed to an
// unknown chunk ("JUNK", skipped by libwebp's demuxer and every other
// reader) with zeroed data, and their VP8X flags are cleared. An EXIF with
// an Orientation other than 1 stays EXIF, cut down to that tag. ICCP,
// ALPH, ANIM/ANMF frames and the bitstream are never touched.

import { ascii, asciiBytes, type Reader, startsWith, u32le } from "./bytes";
import {
  ORIENTATION_TIFF_LENGTH,
  orientationTiff,
  summarizeTiff,
} from "./exif";
import { addKinds, type ScanState } from "./scan-state";
import { isRenderOnlyXmp, summarizeXmp } from "./xmp";

const FLAG_EXIF = 0x08;
const FLAG_XMP = 0x04;
const PADDING_FOURCC = "JUNK";
const MAX_CHUNKS = 1_000_000;
/** Metadata chunks bigger than this are zeroed without being read. */
const MAX_INSPECTED = 4 * 1024 * 1024;

export function isWebp(head: Uint8Array): boolean {
  return startsWith(head, 0, "RIFF") && startsWith(head, 8, "WEBP");
}

export async function scanWebp(
  reader: Reader,
  state: ScanState,
): Promise<void> {
  const header = await reader.exact(0, 12);
  const riffEnd = Math.min(reader.size, 8 + u32le(header, 4));
  let pos = 12;
  let flagsAt = -1;
  let clearFlags = 0;

  for (let chunks = 0; chunks < MAX_CHUNKS && pos + 8 <= riffEnd; chunks += 1) {
    const head = await reader.exact(pos, 8);
    const fourcc = ascii(head, 0, 4);
    const length = u32le(head, 4);
    const dataStart = pos + 8;
    const next = dataStart + length + (length & 1);
    if (dataStart + length > riffEnd) break;

    if (fourcc === "VP8X" && pos === 12 && length >= 1) {
      flagsAt = dataStart;
    } else if (fourcc === "EXIF") {
      const data =
        length <= MAX_INSPECTED ? await reader.exact(dataStart, length) : null;
      // Some writers put JPEG's "Exif\0\0" in front of the TIFF block.
      const prefix = data && startsWith(data, 0, "Exif\0\0") ? 6 : 0;
      const summary = data ? summarizeTiff(data.subarray(prefix)) : null;
      addKinds(state, summary?.kinds ?? ["exif"]);
      const orientation = summary?.orientation ?? null;
      if (
        orientation !== null &&
        orientation !== 1 &&
        length >= prefix + ORIENTATION_TIFF_LENGTH
      ) {
        const bytes = new Uint8Array(prefix + ORIENTATION_TIFF_LENGTH);
        if (prefix) bytes.set(asciiBytes("Exif\0\0"));
        bytes.set(orientationTiff(orientation), prefix);
        state.patches.push({ offset: dataStart, length, bytes });
      } else {
        rename(state, pos, length);
        clearFlags |= FLAG_EXIF;
      }
    } else if (fourcc === "XMP ") {
      const data =
        length <= MAX_INSPECTED ? await reader.exact(dataStart, length) : null;
      if (!data || !isRenderOnlyXmp(data)) {
        addKinds(state, data ? summarizeXmp(data) : ["xmp"]);
        rename(state, pos, length);
        clearFlags |= FLAG_XMP;
      }
    }
    pos = next;
  }

  if (clearFlags && flagsAt >= 0) {
    const flags = await reader.u8(flagsAt);
    if (flags & clearFlags) {
      state.patches.push({
        offset: flagsAt,
        length: 1,
        bytes: new Uint8Array([flags & ~clearFlags]),
      });
    }
  }
}

function rename(state: ScanState, pos: number, length: number) {
  state.patches.push({
    offset: pos,
    length: 4,
    bytes: asciiBytes(PADDING_FOURCC),
  });
  if (length > 0) state.patches.push({ offset: pos + 8, length });
}
