// ── PNG ─────────────────────────────────────────────────────────────────────
//
// eXIf, the text chunks (tEXt, zTXt, iTXt, which also carry XMP and
// ImageMagick's "Raw profile type exif") and tIME are renamed to a private
// ancillary chunk with zeroed data and a fresh CRC: every decoder skips an
// unknown ancillary chunk. An eXIf with an Orientation other than 1 stays
// eXIf, cut down to that one tag. Critical chunks, colour chunks (iCCP,
// sRGB, gAMA, cHRM, cICP) and APNG frames are never touched.

import { ascii, asciiBytes, type Reader, startsWith, u32be } from "./bytes";
import {
  ORIENTATION_TIFF_LENGTH,
  orientationTiff,
  summarizeTiff,
} from "./exif";
import { addKinds, type ScanState } from "./scan-state";
import { summarizeXmp } from "./xmp";

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
/** Ancillary, private, safe to copy: ignored by every PNG decoder. */
const PADDING_TYPE = "yaPd";
const TEXT_TYPES = new Set(["tEXt", "zTXt", "iTXt"]);
const MAX_CHUNKS = 1_000_000;
/** Text chunks bigger than this are zeroed without being read. */
const MAX_INSPECTED_TEXT = 1024 * 1024;

export function isPng(head: Uint8Array): boolean {
  return SIGNATURE.every((byte, k) => head[k] === byte);
}

export async function scanPng(reader: Reader, state: ScanState): Promise<void> {
  let pos = 8;
  for (let chunks = 0; chunks < MAX_CHUNKS; chunks += 1) {
    const head = await reader.bytes(pos, 8);
    if (head.byteLength < 8) return;
    const length = u32be(head, 0);
    const type = ascii(head, 4, 4);
    const dataStart = pos + 8;
    const next = dataStart + length + 4;
    if (next > reader.size) return;
    if (type === "IEND") return;

    if (type === "eXIf") {
      const data = await reader.exact(dataStart, length);
      const summary = summarizeTiff(data);
      addKinds(state, summary?.kinds ?? ["exif"]);
      const orientation = summary?.orientation ?? null;
      if (
        orientation !== null &&
        orientation !== 1 &&
        length >= ORIENTATION_TIFF_LENGTH
      ) {
        rewriteChunk(state, pos, "eXIf", length, orientationTiff(orientation));
      } else {
        rewriteChunk(state, pos, PADDING_TYPE, length);
      }
    } else if (TEXT_TYPES.has(type)) {
      const data =
        length <= MAX_INSPECTED_TEXT
          ? await reader.exact(dataStart, length)
          : await reader.exact(dataStart, 80);
      if (startsWith(data, 0, "XML:com.adobe.xmp\0")) {
        addKinds(state, summarizeXmp(data));
      } else if (startsWith(data, 0, "Raw profile type exif")) {
        state.kinds.add("exif");
      } else if (startsWith(data, 0, "Raw profile type iptc")) {
        state.kinds.add("iptc");
      } else {
        state.kinds.add("comment");
      }
      rewriteChunk(state, pos, PADDING_TYPE, length);
    } else if (type === "tIME") {
      state.kinds.add("timestamp");
      rewriteChunk(state, pos, PADDING_TYPE, length);
    }
    pos = next;
  }
}

/**
 * Overwrites the chunk at `pos` (length `length`) with type `type`, data
 * `data` zero-padded to `length`, and the matching CRC.
 */
function rewriteChunk(
  state: ScanState,
  pos: number,
  type: string,
  length: number,
  data?: Uint8Array,
) {
  const typeBytes = asciiBytes(type);
  let crc = crc32Update(0xffffffff, typeBytes);
  if (data) crc = crc32Update(crc, data);
  crc = crc32Zeros(crc, length - (data?.byteLength ?? 0));
  crc = (crc ^ 0xffffffff) >>> 0;
  state.patches.push({ offset: pos + 4, length: 4, bytes: typeBytes });
  state.patches.push({ offset: pos + 8, length, bytes: data });
  state.patches.push({
    offset: pos + 8 + length,
    length: 4,
    bytes: new Uint8Array(
      [crc >>> 24, crc >>> 16, crc >>> 8, crc].map((b) => b & 0xff),
    ),
  });
}

let crcTable: Uint32Array | null = null;

function table(): Uint32Array {
  if (crcTable) return crcTable;
  crcTable = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) {
      c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    }
    crcTable[n] = c >>> 0;
  }
  return crcTable;
}

function crc32Update(crc: number, bytes: Uint8Array): number {
  const t = table();
  let c = crc;
  for (let k = 0; k < bytes.byteLength; k += 1) {
    c = t[(c ^ bytes[k]) & 0xff] ^ (c >>> 8);
  }
  return c >>> 0;
}

function crc32Zeros(crc: number, count: number): number {
  const t = table();
  let c = crc;
  for (let k = 0; k < count; k += 1) c = t[c & 0xff] ^ (c >>> 8);
  return c >>> 0;
}

/** CRC-32 as PNG computes it (exported for the tests). */
export function crc32(bytes: Uint8Array): number {
  return (crc32Update(0xffffffff, bytes) ^ 0xffffffff) >>> 0;
}
