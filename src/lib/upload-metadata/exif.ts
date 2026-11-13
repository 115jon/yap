// ── EXIF (TIFF) reading and the one tag we keep ─────────────────────────────
//
// Only enough of TIFF to say what an EXIF block held (location, camera,
// times, names) and to read its Orientation. Nothing here reads a value
// beyond Orientation: the GPS tags are counted, never decoded.

import { u16be, u32be } from "./bytes";

/** What a cleaned upload had in it, by kind, never by value. */
export type MetadataKind =
  | "gps"
  | "camera"
  | "timestamp"
  | "author"
  | "exif"
  | "xmp"
  | "iptc"
  | "comment"
  | "other";

export const METADATA_KIND_ORDER: readonly MetadataKind[] = [
  "gps",
  "camera",
  "timestamp",
  "author",
  "exif",
  "xmp",
  "iptc",
  "comment",
  "other",
];

const TAG_ORIENTATION = 0x0112;
const TAG_EXIF_IFD = 0x8769;
const TAG_GPS_IFD = 0x8825;
const TAG_GPS_VERSION = 0x0000;

const TAG_KINDS: ReadonlyMap<number, MetadataKind> = new Map([
  [0x010f, "camera"], // Make
  [0x0110, "camera"], // Model
  [0x0132, "timestamp"], // DateTime
  [0x013b, "author"], // Artist
  [0x8298, "author"], // Copyright
  [0x9003, "timestamp"], // DateTimeOriginal
  [0x9004, "timestamp"], // DateTimeDigitized
  [0x927c, "camera"], // MakerNote
  [0x9286, "comment"], // UserComment
  [0xa430, "author"], // CameraOwnerName
  [0xa431, "camera"], // BodySerialNumber
  [0xa433, "camera"], // LensMake
  [0xa434, "camera"], // LensModel
  [0xa435, "camera"], // LensSerialNumber
]);

const MAX_IFD_ENTRIES = 1024;

export interface TiffSummary {
  kinds: Set<MetadataKind>;
  /** 1–8 when the IFD0 Orientation tag is there and valid. */
  orientation: number | null;
}

interface Endian {
  u16(i: number): number;
  u32(i: number): number;
}

function endian(t: Uint8Array): Endian | null {
  if (t.byteLength < 8) return null;
  if (t[0] === 0x4d && t[1] === 0x4d) {
    return { u16: (i) => u16be(t, i), u32: (i) => u32be(t, i) };
  }
  if (t[0] === 0x49 && t[1] === 0x49) {
    return {
      u16: (i) => t[i] | (t[i + 1] << 8),
      u32: (i) =>
        (t[i] | (t[i + 1] << 8) | (t[i + 2] << 16) | (t[i + 3] << 24)) >>> 0,
    };
  }
  return null;
}

/** One IFD's entries as [tag, type, count, value-or-offset field index]. */
function* ifdEntries(
  t: Uint8Array,
  e: Endian,
  offset: number,
): Generator<{ tag: number; type: number; count: number; field: number }> {
  if (offset < 8 || offset + 2 > t.byteLength) return;
  const count = Math.min(e.u16(offset), MAX_IFD_ENTRIES);
  for (let k = 0; k < count; k += 1) {
    const at = offset + 2 + k * 12;
    if (at + 12 > t.byteLength) return;
    yield {
      tag: e.u16(at),
      type: e.u16(at + 2),
      count: e.u32(at + 4),
      field: at + 8,
    };
  }
}

/**
 * The kinds of data a TIFF/EXIF block holds and its Orientation. Always
 * includes "exif". Null when `t` isn't TIFF at all.
 */
export function summarizeTiff(t: Uint8Array): TiffSummary | null {
  const e = endian(t);
  if (!e || e.u16(2) !== 42) return null;
  const kinds = new Set<MetadataKind>(["exif"]);
  let orientation: number | null = null;
  let exifIfd = 0;
  let gpsIfd = 0;
  for (const entry of ifdEntries(t, e, e.u32(4))) {
    if (entry.tag === TAG_ORIENTATION && entry.type === 3) {
      const value = e.u16(entry.field);
      if (value >= 1 && value <= 8) orientation = value;
    } else if (entry.tag === TAG_EXIF_IFD) {
      exifIfd = e.u32(entry.field);
    } else if (entry.tag === TAG_GPS_IFD) {
      gpsIfd = e.u32(entry.field);
    } else {
      const kind = TAG_KINDS.get(entry.tag);
      if (kind) kinds.add(kind);
    }
  }
  if (exifIfd) {
    for (const entry of ifdEntries(t, e, exifIfd)) {
      const kind = TAG_KINDS.get(entry.tag);
      if (kind) kinds.add(kind);
    }
  }
  if (gpsIfd) {
    for (const entry of ifdEntries(t, e, gpsIfd)) {
      if (entry.tag !== TAG_GPS_VERSION) {
        kinds.add("gps");
        break;
      }
    }
  }
  return { kinds, orientation };
}

/** The TIFF block that keeps only Orientation: 26 bytes, big-endian. */
export const ORIENTATION_TIFF_LENGTH = 26;

export function orientationTiff(orientation: number): Uint8Array {
  const t = new Uint8Array(ORIENTATION_TIFF_LENGTH);
  t.set([0x4d, 0x4d, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x08]);
  t.set([0x00, 0x01], 8); // one entry
  // Orientation, SHORT, count 1, value (left-aligned in the 4-byte field)
  t.set([0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01], 10);
  t.set([0x00, orientation & 0xff, 0x00, 0x00], 18);
  // next IFD offset 0 (bytes 22..25 are already zero)
  return t;
}

/**
 * The TIFF IFD tag `tag` of the first IFD as [field index, count], for the
 * MPF index (same layout as EXIF). Null when it isn't there.
 */
export function readIfd0Tag(
  t: Uint8Array,
  tag: number,
): { offset: number; count: number; endian: Endian } | null {
  const e = endian(t);
  if (!e || e.u16(2) !== 42) return null;
  for (const entry of ifdEntries(t, e, e.u32(4))) {
    if (entry.tag === tag) {
      return { offset: e.u32(entry.field), count: entry.count, endian: e };
    }
  }
  return null;
}
