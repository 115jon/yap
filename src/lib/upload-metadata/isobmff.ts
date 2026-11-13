// ── MP4 / MOV (ISO base media) ──────────────────────────────────────────────
//
// Phone videos record where they were shot: QuickTime and Android write a
// `©xyz` atom in moov/udta, 3GPP a `loci` box, iPhones a
// `com.apple.quicktime.location.ISO6709` key in moov/meta (ffmpeg also a
// plain `location` key). Location and make/model atoms, and XMP boxes, become
// `free` boxes of the same size with zeroed payloads (a zeroed `loci` would
// still read as 0°N 0°E); the matching meta values are zeroed, so they read
// as empty. Every box keeps its size, so nothing is remuxed and the sample
// tables still point at the same bytes. The moov box is read whole, up to
// MAX_MOOV bytes; mdat is only stepped over.
//
// A track whose samples are themselves metadata can carry a GPS fix per
// frame: GoPro's GPMF `gpmd`, the `camm` camera motion track, DJI's `djmd`,
// any timed-metadata (`meta` handler) track except Apple's `mebx` with no
// location key, and subtitle or text tracks (drones write their position
// as subtitles). Dashcams keep GPS of their own: a Novatek `gps ` atom, or
// data in top-level `free`/`skip` boxes. None of these is rewritten; the
// file is reported as unchecked.
//
// HEIF/HEIC and AVIF stills share the container but keep their EXIF in an
// item; they are reported as unchecked, not cleaned. So is any other brand
// that isn't a video one (Canon CR3 raw photos, for one): MP4, QuickTime,
// M4V/M4A and 3GPP files are walked as video, and so is a vendor's own
// major brand (Sony XAVC) when its compatible brands name MP4 and no still
// format.

import {
  ascii,
  asciiBytes,
  MalformedFile,
  type Reader,
  startsWith,
  u32be,
} from "./bytes";
import type { MetadataKind } from "./exif";
import { addKinds, type ScanState, write, zero } from "./scan-state";
import { summarizeXmp } from "./xmp";

export type IsoBrand = "video" | "heif" | "avif" | "isobmff";

const HEIF_BRANDS = new Set([
  "heic",
  "heix",
  "heim",
  "heis",
  "hevc",
  "hevx",
  "hevm",
  "hevs",
  "mif1",
  "mif2",
  "msf1",
]);
const AVIF_BRANDS = new Set(["avif", "avis"]);
const VIDEO_BRANDS = new Set([
  "isom",
  "iso2",
  "iso3",
  "iso4",
  "iso5",
  "iso6",
  "mp41",
  "mp42",
  "avc1",
  "qt  ",
  "M4A ",
]);

/** Compatible brands that make an unknown major brand an MP4. */
const MP4_COMPATIBLE_BRANDS = new Set([
  "isom",
  "iso2",
  "iso3",
  "iso4",
  "iso5",
  "iso6",
  "mp41",
  "mp42",
  "avc1",
]);
/** Compatible brands of a still image or raw photo. */
const STILL_BRANDS = new Set([...HEIF_BRANDS, ...AVIF_BRANDS, "heif", "crx "]);

function isVideoBrand(brand: string): boolean {
  return (
    VIDEO_BRANDS.has(brand) ||
    brand.startsWith("M4V") ||
    brand.startsWith("3gp") ||
    brand.startsWith("3g2")
  );
}
/** First boxes of a QuickTime file written without `ftyp`. */
const BARE_QUICKTIME = new Set(["moov", "mdat", "wide", "free", "skip"]);
const XMP_UUID = [
  0xbe, 0x7a, 0xcf, 0xcb, 0x97, 0xa9, 0x42, 0xe8, 0x9c, 0x71, 0x99, 0x94, 0x91,
  0xe3, 0xaf, 0xac,
];

const MAX_MOOV = 32 * 1024 * 1024;
const MAX_TOP_LEVEL_BOXES = 100_000;
const FREE_TYPE = asciiBytes("free");
const MAX_DEPTH = 8;

const LOCATION_ATOMS = new Set(["\xa9xyz", "loci"]);
const CAMERA_ATOMS = new Set(["\xa9mak", "\xa9mod"]);
const CONTAINERS = new Set([
  "moov",
  "trak",
  "udta",
  "meta",
  "mdia",
  "minf",
  "stbl",
]);
/** Sample entries of timed metadata or text that may hold location. */
const TIMED_METADATA_ENTRIES = new Set([
  "gpmd",
  "camm",
  "djmd",
  "tx3g",
  "text",
  "sbtl",
  "wvtt",
  "stpp",
]);
/** Track handlers whose samples are text (subtitles, captions). */
const TEXT_HANDLERS = new Set(["text", "sbtl", "subt"]);
const METADATA_HANDLER = "meta";
/** Novatek dashcams' GPS index (QuickTime `gps `, sometimes upper case). */
const VENDOR_GPS_ATOMS = new Set(["gps ", "GPS "]);
/** Top-level padding boxes; a dashcam may keep its GPS log in one. */
const PADDING_BOXES = new Set(["free", "skip"]);
const PADDING_READ = 64 * 1024;

/** Which kind a QuickTime metadata key holds, or null to leave it. */
function keyKind(name: string): MetadataKind | null {
  if (/(^|\.)location([.-]|$)|ISO6709/i.test(name)) return "gps";
  if (
    name === "com.apple.quicktime.make" ||
    name === "com.apple.quicktime.model" ||
    name.startsWith("com.apple.quicktime.camera.") ||
    name === "com.android.manufacturer" ||
    name === "com.android.model"
  ) {
    return "camera";
  }
  return null;
}

/**
 * The container a file's `ftyp` (or first box) names, or null when it
 * isn't ISO BMFF. "isobmff" is a brand this scanner doesn't know.
 */
export function isoBrand(head: Uint8Array): IsoBrand | null {
  if (head.byteLength < 12) return null;
  const type = ascii(head, 4, 4);
  if (type !== "ftyp") return BARE_QUICKTIME.has(type) ? "video" : null;
  const size = u32be(head, 0);
  const major = ascii(head, 8, 4);
  if (AVIF_BRANDS.has(major)) return "avif";
  if (HEIF_BRANDS.has(major)) return "heif";
  // A few writers put a generic major brand first; look at the rest.
  const compatible: string[] = [];
  for (let i = 16; i + 4 <= Math.min(size, head.byteLength); i += 4) {
    const brand = ascii(head, i, 4);
    if (AVIF_BRANDS.has(brand)) return "avif";
    if (brand === "heic" || brand === "heix") return "heif";
    compatible.push(brand);
  }
  if (isVideoBrand(major)) return "video";
  // A camera's own major brand (Sony "XAVC") over MP4 compatible brands.
  return compatible.some((brand) => MP4_COMPATIBLE_BRANDS.has(brand)) &&
    !compatible.some((brand) => STILL_BRANDS.has(brand))
    ? "video"
    : "isobmff";
}

interface BoxHeader {
  type: string;
  headerSize: number;
  size: number;
}

function boxAt(b: Uint8Array, i: number, end: number): BoxHeader | null {
  if (i + 8 > end) return null;
  let size = u32be(b, i);
  let headerSize = 8;
  if (size === 1) {
    if (i + 16 > end) return null;
    size = u32be(b, i + 8) * 2 ** 32 + u32be(b, i + 12);
    headerSize = 16;
  } else if (size === 0) {
    size = end - i;
  }
  if (size < headerSize || i + size > end) return null;
  return { type: ascii(b, i + 4, 4), headerSize, size };
}

export async function scanIsoVideo(
  reader: Reader,
  state: ScanState,
): Promise<void> {
  let pos = 0;
  for (let k = 0; pos < reader.size; k += 1) {
    if (k >= MAX_TOP_LEVEL_BOXES) throw new MalformedFile("too many boxes");
    const head = await reader.bytes(pos, 32);
    const box = topLevelBox(head, reader.size - pos);
    if (!box) throw new MalformedFile(`bad box at ${pos}`);
    if (box.type === "moov" || box.type === "udta" || box.type === "meta") {
      if (box.size > MAX_MOOV) throw new MoovTooLarge();
      // `walk` over a range holding just this box handles it as a child.
      walk(await reader.exact(pos, box.size), 0, box.size, pos, state, 0);
    } else if (
      box.type === "uuid" &&
      box.size >= box.headerSize + 16 &&
      isXmpUuid(head, box.headerSize)
    ) {
      state.kinds.add("xmp");
      toFree(state, pos, box);
    } else if (VENDOR_GPS_ATOMS.has(box.type)) {
      state.unchecked ??= "vendor-data";
    } else if (
      PADDING_BOXES.has(box.type) &&
      (await holdsData(reader, pos + box.headerSize, pos + box.size))
    ) {
      state.unchecked ??= "vendor-data";
    }
    pos += box.size;
  }
}

/** Whether any byte of [start, end) is not zero (padding normally is). */
async function holdsData(
  reader: Reader,
  start: number,
  end: number,
): Promise<boolean> {
  for (let at = start; at < end; at += PADDING_READ) {
    const bytes = await reader.exact(at, Math.min(PADDING_READ, end - at));
    if (bytes.some((byte) => byte !== 0)) return true;
  }
  return false;
}

/** A moov bigger than the scanner will hold. */
export class MoovTooLarge extends Error {
  constructor() {
    super("moov too large to scan");
  }
}

/** A top-level box from its first bytes (`boxAt` needs the whole box). */
function topLevelBox(head: Uint8Array, remaining: number): BoxHeader | null {
  if (head.byteLength < 8) return null;
  let size = u32be(head, 0);
  let header = 8;
  if (size === 1) {
    if (head.byteLength < 16) return null;
    size = u32be(head, 8) * 2 ** 32 + u32be(head, 12);
    header = 16;
  } else if (size === 0) {
    size = remaining;
  }
  if (size < header || size > remaining) return null;
  return { type: ascii(head, 4, 4), headerSize: header, size };
}

/** Turns the box at file offset `at` into a zero-filled `free` box. */
function toFree(state: ScanState, at: number, box: BoxHeader) {
  if (box.headerSize === 8) {
    // The type and the payload are contiguous: one patch.
    write(state, at + 4, box.size - 4, FREE_TYPE);
  } else {
    // A 64-bit size sits between them and stays.
    write(state, at + 4, 4, FREE_TYPE);
    zero(state, at + box.headerSize, box.size - box.headerSize);
  }
}

function isXmpUuid(b: Uint8Array, at: number): boolean {
  return XMP_UUID.every((byte, k) => b[at + k] === byte);
}

/**
 * Walks the children of the box whose payload is b[start, end) (b is the
 * whole moov, read from file offset `base`).
 */
function walk(
  b: Uint8Array,
  start: number,
  end: number,
  base: number,
  state: ScanState,
  depth: number,
) {
  if (depth > MAX_DEPTH) return;
  let i = start;
  while (i + 8 <= end) {
    const box = boxAt(b, i, end);
    if (!box) return;
    const payload = i + box.headerSize;
    const boxEnd = i + box.size;
    if (LOCATION_ATOMS.has(box.type)) {
      state.kinds.add("gps");
      toFree(state, base + i, box);
    } else if (CAMERA_ATOMS.has(box.type)) {
      state.kinds.add("camera");
      toFree(state, base + i, box);
    } else if (box.type === "XMP_") {
      addKinds(state, summarizeXmp(b.subarray(payload, boxEnd)));
      toFree(state, base + i, box);
    } else if (box.type === "uuid" && isXmpUuid(b, payload)) {
      addKinds(state, summarizeXmp(b.subarray(payload + 16, boxEnd)));
      toFree(state, base + i, box);
    } else if (box.type === "meta") {
      // ISO meta is a FullBox (4 bytes of version/flags); QuickTime's isn't.
      const children = startsWith(b, payload + 4, "hdlr")
        ? payload
        : payload + 4;
      walkMeta(b, children, boxEnd, base, state, depth + 1);
    } else if (VENDOR_GPS_ATOMS.has(box.type)) {
      state.unchecked ??= "vendor-data";
    } else if (box.type === "trak") {
      if (trackMayHoldLocation(b, payload, boxEnd)) {
        state.unchecked ??= "timed-metadata";
      }
      walk(b, payload, boxEnd, base, state, depth + 1);
    } else if (CONTAINERS.has(box.type)) {
      walk(b, payload, boxEnd, base, state, depth + 1);
    }
    i = boxEnd;
  }
}

/** The first child of b[start, end) of this type, as [payload, end). */
function child(
  b: Uint8Array,
  start: number,
  end: number,
  type: string,
): [number, number] | null {
  let i = start;
  while (i + 8 <= end) {
    const box = boxAt(b, i, end);
    if (!box) return null;
    if (box.type === type) return [i + box.headerSize, i + box.size];
    i += box.size;
  }
  return null;
}

/**
 * Whether a track (trak payload b[start, end)) may carry location in its
 * samples: a GPS, sensor or text sample entry, a subtitle handler, or a
 * timed-metadata handler other than an Apple `mebx` whose keys name no
 * location.
 */
function trackMayHoldLocation(
  b: Uint8Array,
  start: number,
  end: number,
): boolean {
  const mdia = child(b, start, end, "mdia");
  if (!mdia) return false;
  // hdlr: version and flags (4), pre_defined or component type (4), then
  // the handler (component subtype).
  const hdlr = child(b, mdia[0], mdia[1], "hdlr");
  const handler =
    hdlr && hdlr[0] + 12 <= hdlr[1] ? ascii(b, hdlr[0] + 8, 4) : "";
  if (TEXT_HANDLERS.has(handler)) return true;

  const entries = sampleEntries(b, mdia[0], mdia[1]);
  if (entries.some((entry) => TIMED_METADATA_ENTRIES.has(entry.type))) {
    return true;
  }
  const mebx = entries.filter((entry) => entry.type === "mebx");
  if (mebx.some((entry) => mebxMayHoldLocation(b, entry.start, entry.end))) {
    return true;
  }
  // Timed metadata of any other kind (mett, metx, urim, a vendor's own).
  return handler === METADATA_HANDLER && mebx.length < entries.length;
}

interface SampleEntry {
  type: string;
  /** Payload, after the entry's box header. */
  start: number;
  end: number;
}

/** The sample entries in an mdia (payload b[start, end))'s stsd. */
function sampleEntries(
  b: Uint8Array,
  start: number,
  end: number,
): SampleEntry[] {
  const minf = child(b, start, end, "minf");
  const stbl = minf && child(b, minf[0], minf[1], "stbl");
  const stsd = stbl && child(b, stbl[0], stbl[1], "stsd");
  if (!stsd) return [];
  const entries: SampleEntry[] = [];
  // version and flags (4), entry count (4), then the sample entries
  let i = stsd[0] + 8;
  while (i + 8 <= stsd[1]) {
    const entry = boxAt(b, i, stsd[1]);
    if (!entry) break;
    entries.push({
      type: entry.type,
      start: i + entry.headerSize,
      end: i + entry.size,
    });
    i += entry.size;
  }
  return entries;
}

/**
 * Whether an Apple `mebx` sample entry may carry location: one of its
 * `keyd` keys names location, or it has no key this can read.
 */
function mebxMayHoldLocation(
  b: Uint8Array,
  start: number,
  end: number,
): boolean {
  // Keys sit in keys/<local id>/keyd as namespace (4) + name. Writers
  // differ on whether `keys` has a version field, so every `keyd` in the
  // entry is read wherever it sits.
  let keys = 0;
  for (let i = start + 4; i + 8 <= end; i += 1) {
    if (!startsWith(b, i, "keyd")) continue;
    const size = u32be(b, i - 4);
    if (size < 12 || i - 4 + size > end) continue;
    keys += 1;
    if (keyKind(ascii(b, i + 8, size - 12)) === "gps") return true;
    i += size - 5;
  }
  return keys === 0;
}

/** A meta box's children: `keys` names what `ilst` items hold. */
function walkMeta(
  b: Uint8Array,
  start: number,
  end: number,
  base: number,
  state: ScanState,
  depth: number,
) {
  const keyKinds = new Map<number, MetadataKind>();
  let i = start;
  while (i + 8 <= end) {
    const box = boxAt(b, i, end);
    if (!box) break;
    if (box.type === "keys")
      readKeys(b, i + box.headerSize, i + box.size, keyKinds);
    i += box.size;
  }
  i = start;
  while (i + 8 <= end) {
    const box = boxAt(b, i, end);
    if (!box) return;
    const payload = i + box.headerSize;
    const boxEnd = i + box.size;
    if (box.type === "ilst") {
      let j = payload;
      while (j + 8 <= boxEnd) {
        const item = boxAt(b, j, boxEnd);
        if (!item) break;
        const kind = LOCATION_ATOMS.has(item.type)
          ? "gps"
          : CAMERA_ATOMS.has(item.type)
            ? "camera"
            : (keyKinds.get(u32be(b, j + 4)) ?? null);
        if (kind) {
          state.kinds.add(kind);
          zeroDataBoxes(b, j + item.headerSize, j + item.size, base, state);
        }
        j += item.size;
      }
    } else if (box.type === "XMP_" || CONTAINERS.has(box.type)) {
      // `walk` over just this box handles it as it would anywhere else.
      walk(b, i, boxEnd, base, state, depth);
    }
    i = boxEnd;
  }
}

function readKeys(
  b: Uint8Array,
  start: number,
  end: number,
  out: Map<number, MetadataKind>,
) {
  if (start + 8 > end) return;
  const count = u32be(b, start + 4);
  let i = start + 8;
  for (let index = 1; index <= count && i + 8 <= end; index += 1) {
    const size = u32be(b, i);
    if (size < 8 || i + size > end) return;
    const kind = keyKind(ascii(b, i + 8, size - 8));
    if (kind) out.set(index, kind);
    i += size;
  }
}

/** Zeroes the value of every `data` box in an ilst item. */
function zeroDataBoxes(
  b: Uint8Array,
  start: number,
  end: number,
  base: number,
  state: ScanState,
) {
  let i = start;
  while (i + 8 <= end) {
    const box = boxAt(b, i, end);
    if (!box) return;
    if (box.type === "data") {
      // type indicator (4) + locale (4), then the value
      const value = i + box.headerSize + 8;
      zero(state, base + value, i + box.size - value);
    }
    i += box.size;
  }
}
