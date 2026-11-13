// Builders for the upload-metadata tests: tiny real images (made with
// ImageMagick, 4–8 px of flat colour) with metadata spliced in. The GPS
// values are made up (12°34'56"N 65°43'21"W, in the sea) and only there so
// the tests can check they are gone.

import { deflateSync } from "node:zlib";

export const FAKE_LATITUDE = [12, 34, 56];
export const FAKE_LONGITUDE = [65, 43, 21];
export const FAKE_ISO6709 = "+12.5822-065.7225/";
export const FAKE_MAKE = "FixtureCam";
export const FAKE_SERIAL = "SN-0000-FIXTURE";

/** 8x8 red baseline JPEG (JFIF, no metadata). */
export const BASE_JPEG = Uint8Array.from(
  atob(
    "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAMCAgMCAgMDAwMEAwMEBQgFBQQEBQoHBwYIDAoMDAsKCwsNDhIQDQ4RDgsLEBYQERMUFRUVDA8XGBYUGBIUFRT/2wBDAQMEBAUEBQkFBQkUDQsNFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBQUFBT/wAARCAAIAAgDAREAAhEBAxEB/8QAFAABAAAAAAAAAAAAAAAAAAAACP/EABQQAQAAAAAAAAAAAAAAAAAAAAD/xAAVAQEBAAAAAAAAAAAAAAAAAAAHCf/EABQRAQAAAAAAAAAAAAAAAAAAAAD/2gAMAwEAAhEDEQA/ADoDFU3/2Q==",
  ),
  (c) => c.charCodeAt(0),
);

/** 4x4 lossless WebP (simple format: one VP8L chunk). */
const BASE_WEBP = Uint8Array.from(
  atob("UklGRhwAAABXRUJQVlA4TA8AAAAvA8AAAAdQwIj+ByKi/wEA"),
  (c) => c.charCodeAt(0),
);

/** 4x4 GIF89a with a two-colour table and a graphic control block. */
const BASE_GIF = Uint8Array.from([
  0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x04, 0x00, 0x04, 0x00, 0xf0, 0x00, 0x00,
  0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x21, 0xf9, 0x04, 0x00, 0x00, 0x00, 0x00,
  0x00, 0x2c, 0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x04, 0x00, 0x00, 0x02, 0x04,
  0x84, 0x8f, 0x09, 0x05, 0x00, 0x3b,
]);

export function concat(...parts: (Uint8Array | number[])[]): Uint8Array {
  const total = parts.reduce((sum, part) => sum + part.length, 0);
  const out = new Uint8Array(total);
  let at = 0;
  for (const part of parts) {
    out.set(part, at);
    at += part.length;
  }
  return out;
}

export function text(value: string): Uint8Array {
  return new TextEncoder().encode(value);
}

function u16be(n: number) {
  return [(n >> 8) & 0xff, n & 0xff];
}

function u32be(n: number) {
  return [(n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];
}

function u32le(n: number) {
  return [n & 0xff, (n >>> 8) & 0xff, (n >>> 16) & 0xff, (n >>> 24) & 0xff];
}

/** Does `haystack` contain `needle` anywhere? */
export function contains(haystack: Uint8Array, needle: Uint8Array): boolean {
  outer: for (let i = 0; i + needle.length <= haystack.length; i += 1) {
    for (let k = 0; k < needle.length; k += 1) {
      if (haystack[i + k] !== needle[k]) continue outer;
    }
    return true;
  }
  return false;
}

/** The latitude rationals as they sit in the EXIF block (big-endian). */
export function latitudeBytes(): Uint8Array {
  return Uint8Array.from(
    FAKE_LATITUDE.flatMap((v) => [...u32be(v), 0, 0, 0, 1]),
  );
}

// ── EXIF ────────────────────────────────────────────────────────────────────

interface TiffOptions {
  orientation?: number;
  gps?: boolean;
  camera?: boolean;
}

/**
 * A big-endian TIFF block: IFD0 (Make, Model, Orientation, Exif and GPS
 * pointers), an Exif IFD (DateTimeOriginal, BodySerialNumber) and a GPS IFD
 * (latitude and longitude).
 */
export function tiff({
  orientation,
  gps = true,
  camera = true,
}: TiffOptions = {}): Uint8Array {
  type Entry = { tag: number; type: number; count: number; data: number[] };
  const ascii = (s: string) => [...text(s), 0];
  const rationals = (values: number[]) =>
    values.flatMap((v) => [...u32be(v), ...u32be(1)]);

  const ifd0: Entry[] = [];
  if (camera) {
    ifd0.push({
      tag: 0x010f,
      type: 2,
      count: FAKE_MAKE.length + 1,
      data: ascii(FAKE_MAKE),
    });
    ifd0.push({ tag: 0x0110, type: 2, count: 8, data: ascii("Model 1") });
  }
  if (orientation !== undefined) {
    ifd0.push({
      tag: 0x0112,
      type: 3,
      count: 1,
      data: [...u16be(orientation), 0, 0],
    });
  }
  const exifIfd: Entry[] = [
    { tag: 0x9003, type: 2, count: 20, data: ascii("2020:01:02 03:04:05") },
    {
      tag: 0xa431,
      type: 2,
      count: FAKE_SERIAL.length + 1,
      data: ascii(FAKE_SERIAL),
    },
  ];
  const gpsIfd: Entry[] = [
    { tag: 0x0000, type: 1, count: 4, data: [2, 3, 0, 0] },
    { tag: 0x0001, type: 2, count: 2, data: [0x4e, 0, 0, 0] },
    { tag: 0x0002, type: 5, count: 3, data: rationals(FAKE_LATITUDE) },
    { tag: 0x0003, type: 2, count: 2, data: [0x57, 0, 0, 0] },
    { tag: 0x0004, type: 5, count: 3, data: rationals(FAKE_LONGITUDE) },
  ];

  // Layout: header (8) | IFD0 | Exif IFD | GPS IFD | out-of-line values.
  const ifdSize = (n: number) => 2 + n * 12 + 4;
  const ifd0Count = ifd0.length + 1 + (gps ? 1 : 0);
  const exifAt = 8 + ifdSize(ifd0Count);
  const gpsAt = exifAt + ifdSize(exifIfd.length);
  let dataAt = gpsAt + (gps ? ifdSize(gpsIfd.length) : 0);
  const out: number[] = [0x4d, 0x4d, 0, 42, ...u32be(8)];
  const extra: number[] = [];

  const writeIfd = (entries: Entry[]) => {
    out.push(...u16be(entries.length));
    for (const entry of entries) {
      out.push(
        ...u16be(entry.tag),
        ...u16be(entry.type),
        ...u32be(entry.count),
      );
      if (entry.data.length <= 4) {
        out.push(...entry.data, ...Array(4 - entry.data.length).fill(0));
      } else {
        out.push(...u32be(dataAt));
        extra.push(...entry.data);
        dataAt += entry.data.length;
      }
    }
    out.push(...u32be(0));
  };

  writeIfd(
    [
      ...ifd0,
      { tag: 0x8769, type: 4, count: 1, data: u32be(exifAt) },
      ...(gps ? [{ tag: 0x8825, type: 4, count: 1, data: u32be(gpsAt) }] : []),
    ].sort((a, b) => a.tag - b.tag),
  );
  writeIfd(exifIfd);
  if (gps) writeIfd(gpsIfd);
  return Uint8Array.from([...out, ...extra]);
}

export function xmpPacket(withGps = true): string {
  return (
    `<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">` +
    `<rdf:Description xmlns:exif="http://ns.adobe.com/exif/1.0/" xmlns:xmp="http://ns.adobe.com/xap/1.0/"` +
    ` xmp:CreateDate="2020-01-02T03:04:05"` +
    (withGps
      ? ` exif:GPSLatitude="12,34.93N" exif:GPSLongitude="65,43.35W"`
      : "") +
    `/></rdf:RDF></x:xmpmeta>`
  );
}

/** An Ultra HDR-style packet: only the gain-map namespaces. */
export const GAIN_MAP_XMP =
  `<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">` +
  `<rdf:Description xmlns:hdrgm="http://ns.adobe.com/hdr-gain-map/1.0/" hdrgm:Version="1.0"/>` +
  `</rdf:RDF></x:xmpmeta>`;

// ── JPEG ────────────────────────────────────────────────────────────────────

export function jpegSegment(marker: number, payload: Uint8Array): Uint8Array {
  return concat([0xff, marker, ...u16be(payload.length + 2)], payload);
}

/** BASE_JPEG with `segments` inserted right after SOI. */
export function jpegWith(...segments: Uint8Array[]): Uint8Array {
  return concat(BASE_JPEG.subarray(0, 2), ...segments, BASE_JPEG.subarray(2));
}

export function exifSegment(options: TiffOptions = {}): Uint8Array {
  return jpegSegment(0xe1, concat(text("Exif\0\0"), tiff(options)));
}

// ── PNG ─────────────────────────────────────────────────────────────────────

const CRC_TABLE = (() => {
  const table = new Uint32Array(256);
  for (let n = 0; n < 256; n += 1) {
    let c = n;
    for (let k = 0; k < 8; k += 1) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  return table;
})();

export function pngCrc(bytes: Uint8Array): number {
  let c = 0xffffffff;
  for (const byte of bytes) c = CRC_TABLE[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export function pngChunk(type: string, data: Uint8Array): Uint8Array {
  const body = concat(text(type), data);
  return concat(u32be(data.length), body, u32be(pngCrc(body)));
}

/** A 2x2 RGB PNG with `chunks` between IHDR and IDAT. */
export function pngWith(...chunks: Uint8Array[]): Uint8Array {
  const ihdr = pngChunk(
    "IHDR",
    Uint8Array.from([...u32be(2), ...u32be(2), 8, 2, 0, 0, 0]),
  );
  const rows = Uint8Array.from([
    0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 255, 255, 255,
  ]);
  const idat = pngChunk("IDAT", new Uint8Array(deflateSync(rows)));
  return concat(
    [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a],
    ihdr,
    ...chunks,
    idat,
    pngChunk("IEND", new Uint8Array(0)),
  );
}

// ── WebP ────────────────────────────────────────────────────────────────────

export function riffChunk(fourcc: string, data: Uint8Array): Uint8Array {
  return concat(
    text(fourcc),
    u32le(data.length),
    data,
    data.length & 1 ? [0] : [],
  );
}

/** An extended (VP8X) WebP: the base bitstream plus EXIF and/or XMP. */
export function webpWith({
  exif,
  xmp,
}: {
  exif?: Uint8Array;
  xmp?: string;
}): Uint8Array {
  const flags = (exif ? 0x08 : 0) | (xmp ? 0x04 : 0);
  const vp8x = riffChunk(
    "VP8X",
    Uint8Array.from([flags, 0, 0, 0, 3, 0, 0, 3, 0, 0]),
  );
  const body = concat(
    text("WEBP"),
    vp8x,
    BASE_WEBP.subarray(12),
    exif ? riffChunk("EXIF", exif) : [],
    xmp ? riffChunk("XMP ", text(xmp)) : [],
  );
  return concat(text("RIFF"), u32le(body.length), body);
}

// ── GIF ─────────────────────────────────────────────────────────────────────

function subBlocks(data: Uint8Array): number[] {
  const out: number[] = [];
  for (let i = 0; i < data.length; i += 255) {
    const block = data.subarray(i, i + 255);
    out.push(block.length, ...block);
  }
  return [...out, 0];
}

/** BASE_GIF with a comment and an XMP application block before the frame. */
export function gifWith({ comment, xmp }: { comment?: string; xmp?: string }) {
  const at = BASE_GIF.indexOf(0x21, 13); // the graphic control block
  return concat(
    BASE_GIF.subarray(0, at),
    comment ? [0x21, 0xfe, ...subBlocks(text(comment))] : [],
    xmp
      ? [0x21, 0xff, 11, ...text("XMP DataXMP"), ...subBlocks(text(xmp))]
      : [],
    BASE_GIF.subarray(at),
  );
}

// ── MP4 ─────────────────────────────────────────────────────────────────────

export function box(
  type: string | number[],
  ...children: (Uint8Array | number[])[]
) {
  const payload = concat(...children);
  const typeBytes =
    typeof type === "string" ? Array.from(type, (c) => c.charCodeAt(0)) : type;
  return concat(u32be(payload.length + 8), typeBytes, payload);
}

/**
 * An MP4 shaped like a phone video: ftyp, mdat, then moov with a udta ©xyz
 * and a QuickTime meta (keys + ilst) holding location and make.
 */
export function mp4WithLocation(): Uint8Array {
  const qtString = (s: string) =>
    concat(u16be(s.length), [0x15, 0xc7], text(s));
  const key = (name: string) =>
    concat(u32be(name.length + 8), text("mdta"), text(name));
  const dataBox = (value: string) =>
    box("data", u32be(1), u32be(0), text(value));
  const moov = box(
    "moov",
    box("mvhd", new Uint8Array(100)),
    box("udta", box([0xa9, 0x78, 0x79, 0x7a], qtString(FAKE_ISO6709))),
    box(
      "meta",
      box("hdlr", new Uint8Array(8), text("mdta"), new Uint8Array(13)),
      box(
        "keys",
        u32be(0),
        u32be(4),
        key("com.apple.quicktime.location.ISO6709"),
        key("com.apple.quicktime.make"),
        key("com.apple.quicktime.software"),
        key("location"),
      ),
      box(
        "ilst",
        box(u32be(1), dataBox(FAKE_ISO6709)),
        box(u32be(2), dataBox(FAKE_MAKE)),
        box(u32be(3), dataBox("17.1")),
        box(u32be(4), dataBox(FAKE_ISO6709)),
      ),
    ),
  );
  return concat(
    box("ftyp", text("qt  "), u32be(0), text("qt  ")),
    box("mdat", new Uint8Array(64).fill(0x5a)),
    moov,
  );
}
