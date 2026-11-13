// ── JPEG ────────────────────────────────────────────────────────────────────
//
// Walks the marker segments up to the first scan (SOS). Kept as they are:
// APP0 (JFIF), APP2 ICC_PROFILE, APP2 MPF, APP2 ISO 21496-1 gain-map
// parameters, APP14 (Adobe colour transform) and render-only XMP (Ultra HDR). Every other APPn and COM keeps its marker
// and length and has its payload zeroed: no identifier is left, so readers
// skip it. An EXIF block with an Orientation other than 1 becomes a minimal
// EXIF with only that tag (zero-padded to the old length), so browsers still
// turn the picture the right way up.
//
// Images listed in an MPF index (an Ultra HDR gain map, a camera's preview)
// are walked the same way.

import {
  asciiBytes,
  MalformedFile,
  type Reader,
  startsWith,
  u16be,
} from "./bytes";
import {
  ORIENTATION_TIFF_LENGTH,
  orientationTiff,
  readIfd0Tag,
  summarizeTiff,
} from "./exif";
import { addKinds, type ScanState, write, zero } from "./scan-state";
import { isRenderOnlyXmp, summarizeXmp } from "./xmp";

const MARKER_SOS = 0xda;
const MARKER_EOI = 0xd9;
const MARKER_COM = 0xfe;
const APP0 = 0xe0;
const APP1 = 0xe1;
const APP2 = 0xe2;
const APP13 = 0xed;
const APP14 = 0xee;
const APP15 = 0xef;

const EXIF_ID = "Exif\0\0";
const XMP_ID = "http://ns.adobe.com/xap/1.0/\0";
const ICC_ID = "ICC_PROFILE\0";
const MPF_ID = "MPF\0";
const XMP_EXTENSION_ID = "http://ns.adobe.com/xmp/extension/\0";
const GAIN_MAP_ID = "urn:iso:std:iso:ts:21496:-1\0";
const MPF_ENTRY_TAG = 0xb002;
/** Marker segments before the first scan (fill and stray bytes don't count). */
const MAX_SEGMENTS = 4096;
const MAX_MPF_IMAGES = 16;
/** Bytes read at a time while looking for the next marker. */
const MARKER_WINDOW = 4096;

/** SOI is all a decoder needs; what follows may start with stray bytes. */
export function isJpeg(head: Uint8Array): boolean {
  return head[0] === 0xff && head[1] === 0xd8;
}

export async function scanJpeg(
  reader: Reader,
  state: ScanState,
  start = 0,
  nested = false,
): Promise<void> {
  const soi = await reader.exact(start, 2);
  if (soi[0] !== 0xff || soi[1] !== 0xd8) return;

  let pos = start + 2;
  let keptOrientation = false;
  const secondaryImages: number[] = [];

  // Decoders look for the next marker past stray bytes and fill bytes, so
  // this walk does too; and every way out of it but SOS or EOI throws, since
  // a scan that stopped early can't say the rest of the file is clean.
  for (let segments = 0; ; segments += 1) {
    if (segments >= MAX_SEGMENTS) throw new MalformedFile("too many segments");
    const found = await nextMarker(reader, pos);
    if (!found) throw new MalformedFile("no scan");
    const { at, marker } = found;
    if (marker === 0x01 || (marker >= 0xd0 && marker <= 0xd7)) {
      pos = at + 2;
      continue;
    }
    if (marker === MARKER_SOS || marker === MARKER_EOI) break;
    const length = u16be(await reader.exact(at + 2, 2), 0);
    if (length < 2) throw new MalformedFile(`bad segment length at ${at}`);
    const payloadStart = at + 4;
    const payloadLength = length - 2;
    const next = at + 2 + length;
    if (next > reader.size)
      throw new MalformedFile(`segment past end at ${at}`);

    if ((marker >= APP0 && marker <= APP15) || marker === MARKER_COM) {
      const id = await reader.bytes(payloadStart, Math.min(payloadLength, 48));
      const keep =
        marker === APP0 ||
        marker === APP14 ||
        (marker === APP2 &&
          (startsWith(id, 0, ICC_ID) ||
            startsWith(id, 0, MPF_ID) ||
            startsWith(id, 0, GAIN_MAP_ID)));

      if (marker === APP2 && startsWith(id, 0, MPF_ID) && !nested) {
        const payload = await reader.exact(payloadStart, payloadLength);
        for (const offset of mpfImageOffsets(payload.subarray(4))) {
          secondaryImages.push(payloadStart + 4 + offset);
        }
      }

      if (!keep) {
        if (marker === APP1 && startsWith(id, 0, EXIF_ID)) {
          const payload = await reader.exact(payloadStart, payloadLength);
          const summary = summarizeTiff(payload.subarray(EXIF_ID.length));
          addKinds(state, summary?.kinds ?? ["exif"]);
          const orientation = summary?.orientation ?? null;
          if (
            !keptOrientation &&
            orientation !== null &&
            orientation !== 1 &&
            payloadLength >= EXIF_ID.length + ORIENTATION_TIFF_LENGTH
          ) {
            keptOrientation = true;
            const bytes = new Uint8Array(
              EXIF_ID.length + ORIENTATION_TIFF_LENGTH,
            );
            bytes.set(asciiBytes(EXIF_ID));
            bytes.set(orientationTiff(orientation), EXIF_ID.length);
            write(state, payloadStart, payloadLength, bytes);
          } else {
            zero(state, payloadStart, payloadLength);
          }
        } else if (marker === APP1 && startsWith(id, 0, XMP_ID)) {
          const payload = await reader.exact(payloadStart, payloadLength);
          const packet = payload.subarray(XMP_ID.length);
          if (isRenderOnlyXmp(packet)) {
            pos = next;
            continue;
          }
          addKinds(state, summarizeXmp(packet));
          zero(state, payloadStart, payloadLength);
        } else {
          state.kinds.add(
            marker === MARKER_COM
              ? "comment"
              : marker === APP13
                ? "iptc"
                : marker === APP1 && startsWith(id, 0, XMP_EXTENSION_ID)
                  ? "xmp"
                  : "other",
          );
          zero(state, payloadStart, payloadLength);
        }
      }
    }
    pos = next;
  }

  for (const offset of secondaryImages.slice(0, MAX_MPF_IMAGES)) {
    if (offset > start && offset + 3 <= reader.size) {
      await scanJpeg(reader, state, offset, true);
    }
  }
}

/**
 * The next marker at or after `from`, found the way libjpeg's next_marker
 * finds it: bytes other than 0xFF are skipped, a run of 0xFF fill bytes
 * counts as one, and 0xFF 0x00 is skipped as garbage. `at` is the 0xFF
 * just before the marker code. Null at the end of the file.
 */
async function nextMarker(
  reader: Reader,
  from: number,
): Promise<{ at: number; marker: number } | null> {
  let pos = from;
  while (pos < reader.size) {
    const window = await reader.bytes(pos, MARKER_WINDOW);
    let k = 0;
    while (k < window.byteLength) {
      k = window.indexOf(0xff, k);
      if (k < 0) break;
      let code = k + 1;
      while (code < window.byteLength && window[code] === 0xff) code += 1;
      if (code === window.byteLength) break; // the run goes on past the window
      if (window[code] !== 0x00) {
        return { at: pos + code - 1, marker: window[code] };
      }
      k = code + 1;
    }
    if (k < 0) {
      pos += window.byteLength;
    } else {
      // Read again from the window's last byte, which may start a marker.
      if (pos + window.byteLength >= reader.size) return null;
      pos += window.byteLength - 1;
    }
  }
  return null;
}

/**
 * Offsets (from the MPF TIFF header) of the images an MPF index lists after
 * the first. `tiff` starts at the MPF block's byte-order mark.
 */
function mpfImageOffsets(tiff: Uint8Array): number[] {
  const entry = readIfd0Tag(tiff, MPF_ENTRY_TAG);
  if (!entry) return [];
  const offsets: number[] = [];
  const images = Math.floor(entry.count / 16);
  for (let k = 0; k < images; k += 1) {
    const at = entry.offset + k * 16;
    if (at + 16 > tiff.byteLength) break;
    const offset = entry.endian.u32(at + 8);
    if (offset > 0) offsets.push(offset);
  }
  return offsets;
}
