import { describe, expect, it } from "vitest";

import {
  applyPatches,
  blobSource,
  bufferSource,
  type ByteSource,
  type MetadataScan,
  patchingStream,
  scanUploadMetadata,
} from "@/lib/upload-metadata";
import { summarizeTiff } from "@/lib/upload-metadata/exif";
import {
  MAX_PATCHES,
  type ScanState,
  write,
  zero,
} from "@/lib/upload-metadata/scan-state";
import {
  LOCATION_TRACK_MESSAGE,
  UNCHECKED_IMAGE_MESSAGE,
  UNCHECKED_VIDEO_MESSAGE,
  uncheckedUploadError,
  UNSUPPORTED_FILE_MESSAGE,
  UNSUPPORTED_IMAGE_MESSAGE,
} from "@/lib/upload-metadata/strip-upload";

import {
  BASE_JPEG,
  box,
  concat,
  contains,
  exifSegment,
  FAKE_ISO6709,
  FAKE_MAKE,
  FAKE_SERIAL,
  GAIN_MAP_XMP,
  gifWith,
  jpegSegment,
  jpegWith,
  latitudeBytes,
  mp4WithLocation,
  pngChunk,
  pngCrc,
  pngWith,
  text,
  tiff,
  webpWith,
  xmpPacket,
} from "./fixtures/metadata-files";

function u32(value: number): number[] {
  return [value >>> 24, (value >>> 16) & 255, (value >>> 8) & 255, value & 255];
}

async function strip(
  file: Uint8Array,
): Promise<{ out: Uint8Array; scan: MetadataScan }> {
  const out = file.slice();
  const scan = await scanUploadMetadata(bufferSource(out));
  applyPatches(out, scan.patches);
  return { out, scan };
}

/** The JPEG segments up to SOS, as [marker, payload]. */
function jpegSegments(file: Uint8Array): [number, Uint8Array][] {
  const out: [number, Uint8Array][] = [];
  let pos = 2;
  while (pos + 4 <= file.length && file[pos + 1] !== 0xda) {
    const length = (file[pos + 2] << 8) | file[pos + 3];
    out.push([file[pos + 1], file.subarray(pos + 4, pos + 2 + length)]);
    pos += 2 + length;
  }
  return out;
}

function pngChunks(
  file: Uint8Array,
): { type: string; data: Uint8Array; crcOk: boolean }[] {
  const out = [];
  let pos = 8;
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  while (pos + 12 <= file.length) {
    const length = view.getUint32(pos);
    const type = String.fromCharCode(...file.subarray(pos + 4, pos + 8));
    const data = file.subarray(pos + 8, pos + 8 + length);
    const crc = view.getUint32(pos + 8 + length);
    out.push({
      type,
      data,
      crcOk: crc === pngCrc(file.subarray(pos + 4, pos + 8 + length)),
    });
    pos += 12 + length;
  }
  return out;
}

function riffChunks(file: Uint8Array): { fourcc: string; data: Uint8Array }[] {
  const out = [];
  let pos = 12;
  const view = new DataView(file.buffer, file.byteOffset, file.byteLength);
  while (pos + 8 <= file.length) {
    const length = view.getUint32(pos + 4, true);
    out.push({
      fourcc: String.fromCharCode(...file.subarray(pos, pos + 4)),
      data: file.subarray(pos + 8, pos + 8 + length),
    });
    pos += 8 + length + (length & 1);
  }
  return out;
}

function noSecrets(out: Uint8Array) {
  expect(contains(out, latitudeBytes())).toBe(false);
  expect(contains(out, text(FAKE_MAKE))).toBe(false);
  expect(contains(out, text(FAKE_SERIAL))).toBe(false);
  expect(contains(out, text("GPSLatitude"))).toBe(false);
}

describe("JPEG", () => {
  it("removes GPS EXIF, XMP, IPTC and comments, keeps ICC and JFIF", async () => {
    const icc = jpegSegment(
      0xe2,
      concat(text("ICC_PROFILE\0"), [1, 1], new Uint8Array(40).fill(7)),
    );
    const gainMap = jpegSegment(
      0xe2,
      concat(text("urn:iso:std:iso:ts:21496:-1\0"), [0, 0, 0, 0]),
    );
    const file = jpegWith(
      exifSegment(),
      jpegSegment(0xe1, text(`http://ns.adobe.com/xap/1.0/\0${xmpPacket()}`)),
      icc,
      gainMap,
      jpegSegment(0xed, text("Photoshop 3.0\u00008BIM caption")),
      jpegSegment(0xfe, text("shot at home")),
    );
    const { out, scan } = await strip(file);

    expect(scan.format).toBe("jpeg");
    expect(scan.unchecked).toBeNull();
    expect(scan.stripped).toEqual([
      "gps",
      "camera",
      "timestamp",
      "exif",
      "xmp",
      "iptc",
      "comment",
    ]);
    expect(out.length).toBe(file.length);
    noSecrets(out);
    expect(contains(out, text("shot at home"))).toBe(false);
    expect(contains(out, text("caption"))).toBe(false);
    // JFIF, the ICC profile, the gain-map parameters and the image data are
    // byte for byte the same.
    expect(contains(out, icc)).toBe(true);
    expect(contains(out, gainMap)).toBe(true);
    expect(contains(out, BASE_JPEG.subarray(2, 20))).toBe(true);
    expect(contains(out, BASE_JPEG.subarray(-64))).toBe(true);
    // Every stripped segment kept its marker and length; no identifier left.
    const markers = jpegSegments(out).map(([marker]) => marker);
    expect(markers).toEqual(jpegSegments(file).map(([marker]) => marker));
    for (const [marker, payload] of jpegSegments(out)) {
      if (marker === 0xe1 || marker === 0xed || marker === 0xfe) {
        expect(payload.every((b) => b === 0)).toBe(true);
      }
    }
  });

  it("keeps a minimal EXIF holding only the Orientation", async () => {
    const { out, scan } = await strip(
      jpegWith(exifSegment({ orientation: 6 })),
    );
    expect(scan.stripped).toContain("gps");
    noSecrets(out);
    const [, payload] = jpegSegments(out).find(([m]) => m === 0xe1)!;
    expect(String.fromCharCode(...payload.subarray(0, 6))).toBe("Exif\0\0");
    const summary = summarizeTiff(payload.subarray(6));
    expect(summary?.orientation).toBe(6);
    expect([...summary!.kinds]).toEqual(["exif"]);
  });

  it("drops an EXIF whose Orientation is the default", async () => {
    const { out } = await strip(jpegWith(exifSegment({ orientation: 1 })));
    const [, payload] = jpegSegments(out).find(([m]) => m === 0xe1)!;
    expect(payload.every((b) => b === 0)).toBe(true);
  });

  it("keeps a render-only (Ultra HDR gain map) XMP packet", async () => {
    const xmp = jpegSegment(
      0xe1,
      text(`http://ns.adobe.com/xap/1.0/\0${GAIN_MAP_XMP}`),
    );
    const { out, scan } = await strip(jpegWith(xmp));
    expect(scan.stripped).toEqual([]);
    expect(scan.patches).toEqual([]);
    expect(contains(out, xmp)).toBe(true);
  });

  it("cleans the images an MPF index points at", async () => {
    const secondary = jpegWith(exifSegment({ orientation: 3 }));
    // MPF: header, one IFD with MPEntry (two 16-byte entries).
    const mpfTiff = (secondaryOffset: number) =>
      Uint8Array.from([
        0x4d,
        0x4d,
        0,
        42,
        0,
        0,
        0,
        8,
        0,
        1,
        0xb0,
        0x02,
        0,
        7,
        0,
        0,
        0,
        32,
        0,
        0,
        0,
        26,
        0,
        0,
        0,
        0,
        // entry 1: the primary (offset 0); entry 2: the secondary
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        (secondaryOffset >>> 24) & 0xff,
        (secondaryOffset >>> 16) & 0xff,
        (secondaryOffset >>> 8) & 0xff,
        secondaryOffset & 0xff,
        0,
        0,
        0,
        0,
      ]);
    const draft = jpegWith(
      jpegSegment(0xe2, concat(text("MPF\0"), mpfTiff(0))),
    );
    // The MPF TIFF header starts 2 (SOI) + 4 (marker, length) + 4 ("MPF\0") in.
    const offset = draft.length - 10;
    const primary = jpegWith(
      jpegSegment(0xe2, concat(text("MPF\0"), mpfTiff(offset))),
    );
    const file = concat(primary, secondary);

    const { out, scan } = await strip(file);
    expect(scan.stripped).toContain("gps");
    noSecrets(out);
    const tail = out.subarray(primary.length);
    const [, payload] = jpegSegments(tail).find(([m]) => m === 0xe1)!;
    expect(summarizeTiff(payload.subarray(6))?.orientation).toBe(3);
  });
});

describe("PNG", () => {
  it("removes eXIf, text chunks, XMP and tIME with valid CRCs", async () => {
    const file = pngWith(
      pngChunk("iCCP", concat(text("icc\0\0"), [0x78, 0x9c, 1, 2, 3])),
      pngChunk("eXIf", tiff()),
      pngChunk("tEXt", text("Comment\0shot at home")),
      pngChunk("iTXt", text(`XML:com.adobe.xmp\0\0\0\0\0${xmpPacket()}`)),
      pngChunk("tIME", Uint8Array.from([7, 228, 1, 2, 3, 4, 5])),
    );
    const { out, scan } = await strip(file);
    expect(scan.format).toBe("png");
    expect(scan.stripped).toEqual([
      "gps",
      "camera",
      "timestamp",
      "exif",
      "xmp",
      "comment",
    ]);
    expect(out.length).toBe(file.length);
    noSecrets(out);
    expect(contains(out, text("shot at home"))).toBe(false);
    const chunks = pngChunks(out);
    expect(chunks.map((c) => c.type)).toEqual([
      "IHDR",
      "iCCP",
      "yaPd",
      "yaPd",
      "yaPd",
      "yaPd",
      "IDAT",
      "IEND",
    ]);
    expect(chunks.every((c) => c.crcOk)).toBe(true);
  });

  it("keeps eXIf with only the Orientation", async () => {
    const { out } = await strip(
      pngWith(pngChunk("eXIf", tiff({ orientation: 8 }))),
    );
    noSecrets(out);
    const exif = pngChunks(out).find((c) => c.type === "eXIf")!;
    expect(exif.crcOk).toBe(true);
    expect(summarizeTiff(exif.data)?.orientation).toBe(8);
  });

  it("leaves a PNG without metadata untouched", async () => {
    const { scan } = await strip(pngWith());
    expect(scan.patches).toEqual([]);
    expect(scan.stripped).toEqual([]);
  });
});

describe("WebP", () => {
  it("renames EXIF and XMP chunks and clears their VP8X flags", async () => {
    const file = webpWith({ exif: tiff(), xmp: xmpPacket() });
    const { out, scan } = await strip(file);
    expect(scan.format).toBe("webp");
    expect(scan.stripped).toEqual([
      "gps",
      "camera",
      "timestamp",
      "exif",
      "xmp",
    ]);
    noSecrets(out);
    const chunks = riffChunks(out);
    expect(chunks.map((c) => c.fourcc)).toEqual([
      "VP8X",
      "VP8L",
      "JUNK",
      "JUNK",
    ]);
    expect(chunks[0].data[0] & 0x0c).toBe(0);
    // The bitstream is unchanged.
    expect(chunks[1].data).toEqual(riffChunks(file)[1].data);
  });

  it("keeps an Orientation-only EXIF (also behind an Exif\\0\\0 prefix)", async () => {
    const exif = concat(text("Exif\0\0"), tiff({ orientation: 5 }));
    const { out } = await strip(webpWith({ exif }));
    noSecrets(out);
    const chunks = riffChunks(out);
    const kept = chunks.find((c) => c.fourcc === "EXIF")!;
    expect(summarizeTiff(kept.data.subarray(6))?.orientation).toBe(5);
    expect(chunks[0].data[0] & 0x08).toBe(0x08);
  });
});

describe("GIF", () => {
  it("zeroes comment and XMP blocks without changing their structure", async () => {
    const file = gifWith({ comment: "shot at home", xmp: xmpPacket() });
    const { out, scan } = await strip(file);
    expect(scan.format).toBe("gif");
    expect(scan.stripped).toEqual(["gps", "timestamp", "xmp", "comment"]);
    expect(out.length).toBe(file.length);
    expect(contains(out, text("shot at home"))).toBe(false);
    expect(contains(out, text("GPSLatitude"))).toBe(false);
    // Still a GIF that ends in its trailer, with the frame intact.
    expect(out[out.length - 1]).toBe(0x3b);
    expect(contains(out, Uint8Array.from([0x2c, 0, 0, 0, 0, 4, 0, 4, 0]))).toBe(
      true,
    );
  });
});

describe("GIF XMP in Adobe's layout", () => {
  /** BASE_GIF with a raw XMP packet and Adobe's magic trailer. */
  function gifWithAdobeXmp(packet: string): Uint8Array {
    const base = gifWith({});
    const at = base.indexOf(0x21, 13);
    const trailer = [1];
    for (let b = 0xff; b >= 0; b -= 1) trailer.push(b);
    trailer.push(0);
    return concat(
      base.subarray(0, at),
      [0x21, 0xff, 11],
      text("XMP DataXMP"),
      text(packet),
      trailer,
      base.subarray(at),
    );
  }

  /** Walks `file` as a GIF decoder does; the introducers it meets. */
  function decoderWalk(file: Uint8Array): number[] {
    const seen: number[] = [];
    const flags = file[10];
    let pos = 13 + (flags & 0x80 ? 3 * (1 << ((flags & 7) + 1)) : 0);
    const blocks = () => {
      while (file[pos] !== 0) pos += 1 + file[pos];
      pos += 1;
    };
    while (pos < file.length) {
      const introducer = file[pos];
      seen.push(introducer);
      if (introducer === 0x3b) break;
      if (introducer === 0x21) {
        pos += 2;
        blocks();
      } else if (introducer === 0x2c) {
        const local = file[pos + 9];
        pos += 10 + (local & 0x80 ? 3 * (1 << ((local & 7) + 1)) : 0) + 1;
        blocks();
      } else {
        throw new Error(`decoder hit ${introducer} at ${pos}`);
      }
    }
    return seen;
  }

  it("blanks the raw packet and keeps the file decodable", async () => {
    for (const padding of [0, 1, 200, 255, 256, 600]) {
      const packet = xmpPacket() + " ".repeat(padding);
      const file = gifWithAdobeXmp(packet);
      const { out, scan } = await strip(file);
      expect(scan.format).toBe("gif");
      expect(scan.unchecked).toBeNull();
      expect(scan.stripped).toContain("gps");
      expect(scan.stripped).toContain("xmp");
      expect(out.length).toBe(file.length);
      expect(contains(out, text("GPSLatitude"))).toBe(false);
      expect(contains(out, text("XMP DataXMP"))).toBe(false);
      expect(decoderWalk(out)).toEqual(decoderWalk(file));
      const again = await scanUploadMetadata(bufferSource(out));
      expect(again.stripped).toEqual([]);
      expect(again.unchecked).toBeNull();
    }
  });
});

describe("MP4 / MOV", () => {
  it("zeroes the ©xyz atom and location/make keys in place", async () => {
    const file = mp4WithLocation();
    const { out, scan } = await strip(file);
    expect(scan.format).toBe("video");
    expect(scan.stripped).toEqual(["gps", "camera"]);
    expect(out.length).toBe(file.length);
    expect(contains(out, text(FAKE_ISO6709))).toBe(false);
    expect(contains(out, text(FAKE_MAKE))).toBe(false);
    // ©xyz is now a free box of the same size; key names stay, values went.
    expect(contains(out, Uint8Array.from([0xa9, 0x78, 0x79, 0x7a]))).toBe(
      false,
    );
    expect(contains(out, text("free"))).toBe(true);
    expect(contains(out, text("com.apple.quicktime.location.ISO6709"))).toBe(
      true,
    );
    expect(contains(out, text("17.1"))).toBe(true);
    expect(contains(out, new Uint8Array(64).fill(0x5a))).toBe(true);
  });

  it("finds moov after a 64-bit mdat", async () => {
    const big = concat(
      [0, 0, 0, 1],
      text("mdat"),
      [0, 0, 0, 0, 0, 0, 0, 24],
      new Uint8Array(8),
    );
    const file = concat(
      box("ftyp", text("isom"), [0, 0, 0, 0], text("isom")),
      big,
      box(
        "moov",
        box("udta", box([0xa9, 0x78, 0x79, 0x7a], text(FAKE_ISO6709))),
      ),
    );
    const { out, scan } = await strip(file);
    expect(scan.stripped).toEqual(["gps"]);
    expect(contains(out, text(FAKE_ISO6709))).toBe(false);
  });
});

describe("pass-through", () => {
  it("flags HEIC and AVIF as unchecked and leaves them alone", async () => {
    for (const [brand, format] of [
      ["heic", "heif"],
      ["avif", "avif"],
    ] as const) {
      const file = concat(
        box("ftyp", text(brand), [0, 0, 0, 0], text("mif1")),
        box("meta", new Uint8Array(32)),
      );
      const { out, scan } = await strip(file);
      expect(scan.format).toBe(format);
      expect(scan.unchecked).toBe("unsupported-format");
      expect(scan.patches).toEqual([]);
      expect(out).toEqual(file);
    }
  });

  it("does not touch files it doesn't know", async () => {
    const zip = concat(text("PK\x03\x04"), new Uint8Array(100).fill(9));
    const { scan } = await strip(zip);
    expect(scan).toEqual({
      format: "other",
      stripped: [],
      unchecked: null,
      patches: [],
    });
  });

  it("reads only the head of a huge non-image", async () => {
    let read = 0;
    const source: ByteSource = {
      size: 500 * 1024 * 1024,
      read: async (_offset, length) => {
        read += length;
        return new Uint8Array(length);
      },
    };
    const scan = await scanUploadMetadata(source);
    expect(scan.format).toBe("other");
    expect(read).toBeLessThanOrEqual(64);
  });

  it("stops at the byte budget and reports the file as too large", async () => {
    // A PNG whose metadata sits behind 40 image chunks of 70 KB: each
    // chunk header costs a fresh 64 KB window.
    const file = pngWith(
      ...Array.from({ length: 40 }, () =>
        pngChunk("IDAT", new Uint8Array(70_000)),
      ),
      pngChunk("tEXt", text("Comment\0late")),
    );
    const scan = await scanUploadMetadata(bufferSource(file), {
      windowSize: 64 * 1024,
      budget: 1024 * 1024,
    });
    expect(scan.unchecked).toBe("too-large");
  });

  it("still strips what it saw before a truncated segment", async () => {
    // EXIF, then a segment whose length runs past the end of the file.
    const file = concat(
      BASE_JPEG.subarray(0, 2),
      exifSegment(),
      [0xff, 0xfe, 0x40, 0x00],
      text("cut off"),
    );
    const { out, scan } = await strip(file);
    expect(scan.stripped).toContain("gps");
    noSecrets(out);
  });
});

describe("sources and streams", () => {
  it("scans a Blob by slices and patches it through a stream", async () => {
    const file = jpegWith(
      exifSegment({ orientation: 6 }),
      jpegSegment(0xfe, text("shot at home")),
    );
    const scan = await scanUploadMetadata(
      blobSource(new Blob([file.slice()])),
      {
        windowSize: 16,
      },
    );
    expect(scan.stripped).toContain("gps");

    // Feed it through in awkward 7-byte chunks: same result as in place.
    const expected = file.slice();
    applyPatches(expected, scan.patches);
    const chunks: Uint8Array[] = [];
    for (let i = 0; i < file.length; i += 7) chunks.push(file.slice(i, i + 7));
    const streamed = await new Response(
      new ReadableStream({
        start(controller) {
          for (const chunk of chunks) controller.enqueue(chunk);
          controller.close();
        },
      }).pipeThrough(patchingStream(scan.patches)),
    ).arrayBuffer();
    expect(new Uint8Array(streamed)).toEqual(expected);
  });
});

describe("hostile files", () => {
  /** Scans `file` and returns the scan and the heap it grew by, in MB. */
  async function scanMeasured(file: Uint8Array) {
    const before = process.memoryUsage().heapUsed;
    const scan = await scanUploadMetadata(bufferSource(file));
    const grownMb = (process.memoryUsage().heapUsed - before) / 1e6;
    return { scan, grownMb };
  }

  it("stops a GIF of ten million 1-byte comment blocks", async () => {
    const blocks = 10_000_000;
    const file = new Uint8Array(13 + 2 + blocks * 2 + 2);
    file.set([0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 1, 0, 1, 0, 0, 0, 0]);
    let p = 13;
    file[p++] = 0x21;
    file[p++] = 0xfe;
    for (let k = 0; k < blocks; k += 1) {
      file[p++] = 1;
      file[p++] = 0x41;
    }
    file[p++] = 0;
    file[p] = 0x3b;
    const { scan, grownMb } = await scanMeasured(file);
    expect(scan.unchecked).toBe("malformed");
    expect(scan.patches.length).toBeLessThanOrEqual(MAX_PATCHES);
    expect(grownMb).toBeLessThan(32);
  });

  it("stops a PNG of a million empty text chunks", async () => {
    const chunks = 1_000_000;
    const file = new Uint8Array(8 + chunks * 12 + 12);
    file.set([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
    for (let k = 0; k < chunks; k += 1) {
      file.set([0, 0, 0, 0, 0x74, 0x45, 0x58, 0x74], 8 + k * 12);
    }
    file.set([0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44], 8 + chunks * 12);
    const { scan, grownMb } = await scanMeasured(file);
    expect(scan.unchecked).toBe("malformed");
    expect(scan.patches.length).toBeLessThanOrEqual(MAX_PATCHES);
    expect(grownMb).toBeLessThan(32);
    // The padding chunks share their type bytes and CRC.
    expect(new Set(scan.patches.map((patch) => patch.bytes)).size).toBe(2);
  });

  it("stops an MP4 of millions of 8-byte location boxes", async () => {
    const atoms = 3_000_000;
    const udtaSize = 8 + atoms * 8;
    const moov = new Uint8Array(8 + udtaSize);
    const view = new DataView(moov.buffer);
    view.setUint32(0, moov.length);
    moov.set(text("moov"), 4);
    view.setUint32(8, udtaSize);
    moov.set(text("udta"), 12);
    for (let k = 0; k < atoms; k += 1) {
      view.setUint32(16 + k * 8, 8);
      moov.set([0xa9, 0x78, 0x79, 0x7a], 20 + k * 8);
    }
    const file = concat(box("ftyp", text("isom"), u32(0), text("isom")), moov);
    const { scan, grownMb } = await scanMeasured(file);
    expect(scan.unchecked).toBe("malformed");
    expect(scan.patches.length).toBeLessThanOrEqual(MAX_PATCHES);
    expect(grownMb).toBeLessThan(64);
  });

  it("merges zeroed ranges that touch", () => {
    const state: ScanState = { kinds: new Set(), patches: [] };
    const head = Uint8Array.of(1, 2);
    write(state, 0, 4, head);
    zero(state, 4, 10); // starts where the last patch ends
    zero(state, 8, 20); // overlaps it
    zero(state, 30, 2); // a gap: a new patch
    expect(state.patches).toEqual([
      { offset: 0, length: 28, bytes: head },
      { offset: 30, length: 2 },
    ]);
  });

  it("does not merge zeros into the bytes a patch writes", () => {
    const state: ScanState = { kinds: new Set(), patches: [] };
    write(state, 0, 8, Uint8Array.of(1, 2, 3, 4));
    zero(state, 2, 4);
    expect(state.patches).toHaveLength(2);
  });
});

describe("early exits", () => {
  /** BASE_JPEG with `before` bytes, then a GPS EXIF, after SOI. */
  const jpegAfter = (before: number[]) =>
    concat(
      BASE_JPEG.subarray(0, 2),
      before,
      exifSegment(),
      BASE_JPEG.subarray(2),
    );

  it("finds EXIF past a stray byte between segments", async () => {
    const { out, scan } = await strip(jpegAfter([0x00]));
    expect(scan.stripped).toContain("gps");
    expect(scan.unchecked).toBeNull();
    noSecrets(out);
  });

  it("finds EXIF past stray 0xFF 0x00 pairs and other junk", async () => {
    const { out, scan } = await strip(jpegAfter([0x12, 0xff, 0x00, 0x34]));
    expect(scan.stripped).toContain("gps");
    noSecrets(out);
  });

  it("does not count 5000 fill bytes as segments", async () => {
    const { out, scan } = await strip(
      jpegAfter(Array.from({ length: 5000 }, () => 0xff)),
    );
    expect(scan.stripped).toContain("gps");
    expect(scan.unchecked).toBeNull();
    noSecrets(out);
  });

  it("flags a JPEG with more segments than it walks", async () => {
    const empty = jpegSegment(0xfe, new Uint8Array(0));
    const file = jpegWith(
      ...Array.from({ length: 5000 }, () => empty),
      exifSegment(),
    );
    const { scan } = await strip(file);
    expect(scan.unchecked).toBe("malformed");
  });

  it("flags a JPEG segment with a bad length", async () => {
    const file = jpegWith(Uint8Array.of(0xff, 0xfe, 0x00, 0x01), exifSegment());
    const { scan } = await strip(file);
    expect(scan.unchecked).toBe("malformed");
  });

  it("flags a JPEG that ends before its scan", async () => {
    const file = concat(BASE_JPEG.subarray(0, 2), exifSegment());
    const { out, scan } = await strip(file);
    expect(scan.unchecked).toBe("malformed");
    noSecrets(out);
  });

  it("flags a GIF block it doesn't know", async () => {
    const file = gifWith({ comment: "shot at home" });
    const at = file.indexOf(0x2c, 13); // the frame
    const bad = concat(file.subarray(0, at), [0x99], file.subarray(at));
    const { scan } = await strip(bad);
    expect(scan.unchecked).toBe("malformed");
    expect(scan.stripped).toContain("comment");
  });

  it("flags a PNG without IEND", async () => {
    const file = pngWith(pngChunk("tEXt", text("Comment\0hi")));
    const { scan } = await strip(file.subarray(0, file.length - 12));
    expect(scan.unchecked).toBe("malformed");
  });

  it("flags a WebP chunk that runs past the file", async () => {
    const file = webpWith({ xmp: xmpPacket() });
    const cut = file.slice(0, file.length - 10);
    const { scan } = await strip(cut);
    expect(scan.unchecked).toBe("malformed");
  });
});

describe("fail-closed rule", () => {
  const scanOf = (
    format: MetadataScan["format"],
    unchecked: MetadataScan["unchecked"],
  ): MetadataScan => ({ format, stripped: [], unchecked, patches: [] });

  it("refuses an image whose scan stopped early", () => {
    for (const format of ["jpeg", "png", "webp", "gif"] as const) {
      expect(uncheckedUploadError(scanOf(format, "malformed"))).toBe(
        UNCHECKED_IMAGE_MESSAGE,
      );
      expect(uncheckedUploadError(scanOf(format, "too-large"))).toBe(
        UNCHECKED_IMAGE_MESSAGE,
      );
      expect(uncheckedUploadError(scanOf(format, null))).toBeNull();
    }
  });

  it("refuses an image format it can't clean", () => {
    for (const format of ["heif", "avif", "tiff", "jxl"] as const) {
      expect(uncheckedUploadError(scanOf(format, "unsupported-format"))).toBe(
        UNSUPPORTED_IMAGE_MESSAGE,
      );
    }
    expect(uncheckedUploadError(scanOf("isobmff", "unsupported-format"))).toBe(
      UNSUPPORTED_FILE_MESSAGE,
    );
  });

  it("refuses a video it couldn't finish or that has a location track", () => {
    expect(uncheckedUploadError(scanOf("video", "malformed"))).toBe(
      UNCHECKED_VIDEO_MESSAGE,
    );
    expect(uncheckedUploadError(scanOf("video", "too-large"))).toBe(
      UNCHECKED_VIDEO_MESSAGE,
    );
    expect(uncheckedUploadError(scanOf("video", "timed-metadata"))).toBe(
      LOCATION_TRACK_MESSAGE,
    );
    expect(uncheckedUploadError(scanOf("video", "vendor-data"))).toBe(
      LOCATION_TRACK_MESSAGE,
    );
    expect(uncheckedUploadError(scanOf("video", null))).toBeNull();
  });

  it("lets a format with nothing to check through", () => {
    expect(uncheckedUploadError(scanOf("other", null))).toBeNull();
  });

  it("refuses a real AVIF and HEIC header", async () => {
    for (const major of ["avif", "mif1", "heic"]) {
      const file = concat(
        box("ftyp", text(major), [0, 0, 0, 0], text("mif1"), text(major)),
        box("meta", u32(0)),
      );
      const scan = await scanUploadMetadata(bufferSource(file));
      expect(scan.unchecked).toBe("unsupported-format");
      expect(uncheckedUploadError(scan)).toBe(UNSUPPORTED_IMAGE_MESSAGE);
    }
  });
});

describe("container brands", () => {
  const ftyp = (major: string, ...compatible: string[]) =>
    concat(
      box("ftyp", text(major), [0, 0, 0, 0], ...compatible.map(text)),
      box(
        "moov",
        box("udta", box([0xa9, 0x78, 0x79, 0x7a], text(FAKE_ISO6709))),
      ),
    );

  it("walks known video brands as video", async () => {
    for (const brand of [
      "isom",
      "iso6",
      "mp41",
      "mp42",
      "avc1",
      "qt  ",
      "M4V ",
      "M4A ",
      "3gp5",
      "3g2a",
    ]) {
      const { out, scan } = await strip(ftyp(brand, "isom"));
      expect(scan.format).toBe("video");
      expect(scan.stripped).toEqual(["gps"]);
      expect(contains(out, text(FAKE_ISO6709))).toBe(false);
    }
  });

  it("walks a vendor major brand over MP4 compatible brands as video", async () => {
    for (const file of [
      ftyp("XAVC", "XAVC", "mp42", "iso2"),
      ftyp("abcd", "mp42"),
      ftyp("abcd", "abcd", "isom", "avc1"),
    ]) {
      const { out, scan } = await strip(file);
      expect(scan.format).toBe("video");
      expect(scan.stripped).toEqual(["gps"]);
      expect(contains(out, text(FAKE_ISO6709))).toBe(false);
    }
  });

  it("flags Canon CR3 and other brands as unchecked", async () => {
    for (const file of [
      ftyp("crx ", "crx ", "isom"),
      ftyp("abcd", "abcd"),
      ftyp("abcd", "mp42", "mif1"),
      ftyp("abcd", "isom", "heif"),
    ]) {
      const { out, scan } = await strip(file);
      expect(scan.format).toBe("isobmff");
      expect(scan.unchecked).toBe("unsupported-format");
      expect(out).toEqual(file);
    }
  });

  it("flags a JPEG XL container as unchecked", async () => {
    const file = concat(
      [0, 0, 0, 12],
      text("JXL \r\n"),
      [0x87, 0x0a],
      box("ftyp", text("jxl "), [0, 0, 0, 0], text("jxl ")),
      box("Exif", [0, 0, 0, 0], tiff()),
    );
    const { scan } = await strip(file);
    expect(scan.format).toBe("jxl");
    expect(scan.unchecked).toBe("unsupported-format");
    expect(scan.patches).toEqual([]);
  });
});

describe("timed metadata tracks", () => {
  const hdlr = (handler: string) =>
    box("hdlr", u32(0), u32(0), text(handler), new Uint8Array(12), [0]);
  const track = (entries: Uint8Array[], handler?: string) =>
    box(
      "trak",
      box(
        "mdia",
        ...(handler ? [hdlr(handler)] : []),
        box(
          "minf",
          box("stbl", box("stsd", u32(0), u32(entries.length), ...entries)),
        ),
      ),
    );
  const video = (...moovChildren: Uint8Array[]) =>
    concat(
      box("ftyp", text("mp41"), [0, 0, 0, 0], text("mp41")),
      box(
        "moov",
        ...moovChildren,
        box("udta", box([0xa9, 0x78, 0x79, 0x7a], text(FAKE_ISO6709))),
      ),
    );
  const withTrack = (entry: string, handler?: string) =>
    video(track([box(entry, new Uint8Array(16))], handler));
  /** Apple's timed-metadata entry naming these keys. */
  const mebx = (...keys: string[]) =>
    box(
      "mebx",
      new Uint8Array(6),
      [0, 1],
      box(
        "keys",
        ...keys.map((key, k) =>
          box(
            u32(k + 1),
            box("keyd", text("mdta"), text(key)),
            box("dtyp", u32(0), text("com.apple.quicktime.text")),
          ),
        ),
      ),
    );

  it("flags a GPS, sensor or subtitle track and still cleans the rest", async () => {
    for (const entry of [
      "gpmd",
      "camm",
      "djmd",
      "tx3g",
      "text",
      "sbtl",
      "wvtt",
      "stpp",
    ]) {
      const { out, scan } = await strip(withTrack(entry));
      expect(scan.format).toBe("video");
      expect(scan.unchecked).toBe("timed-metadata");
      expect(scan.stripped).toEqual(["gps"]);
      expect(contains(out, text(FAKE_ISO6709))).toBe(false);
    }
  });

  it("flags any track with a subtitle or text handler", async () => {
    for (const handler of ["text", "sbtl", "subt"]) {
      const { scan } = await strip(withTrack("abcd", handler));
      expect(scan.unchecked).toBe("timed-metadata");
    }
  });

  it("flags a timed-metadata track of a kind it can't read", async () => {
    for (const entry of ["mett", "metx", "urim", "abcd"]) {
      const { scan } = await strip(withTrack(entry, "meta"));
      expect(scan.unchecked).toBe("timed-metadata");
    }
  });

  it("flags an Apple mebx track whose keys name location", async () => {
    const { scan } = await strip(
      video(
        track(
          [
            mebx(
              "com.apple.quicktime.video-orientation",
              "com.apple.quicktime.location.ISO6709",
            ),
          ],
          "meta",
        ),
      ),
    );
    expect(scan.unchecked).toBe("timed-metadata");
  });

  it("flags an Apple mebx track with no key it can read", async () => {
    const { scan } = await strip(video(track([mebx()], "meta")));
    expect(scan.unchecked).toBe("timed-metadata");
  });

  it("leaves an iPhone's mebx track without location alone", async () => {
    const { scan } = await strip(
      video(
        track(
          [
            mebx(
              "com.apple.quicktime.video-orientation",
              "com.apple.quicktime.live-photo-info",
            ),
          ],
          "meta",
        ),
        track([box("avc1", new Uint8Array(16))], "vide"),
      ),
    );
    expect(scan.unchecked).toBeNull();
    expect(scan.stripped).toEqual(["gps"]);
  });

  it("leaves ordinary video, sound and timecode tracks alone", async () => {
    for (const [entry, handler] of [
      ["avc1", "vide"],
      ["mp4a", "soun"],
      ["tmcd", "tmcd"],
      ["avc1", undefined],
    ] as const) {
      const { scan } = await strip(withTrack(entry, handler));
      expect(scan.unchecked).toBeNull();
      expect(scan.stripped).toEqual(["gps"]);
    }
  });
});

describe("dashcam GPS", () => {
  const head = box("ftyp", text("mp42"), [0, 0, 0, 0], text("isom"));
  const moov = box(
    "moov",
    box("udta", box([0xa9, 0x78, 0x79, 0x7a], text(FAKE_ISO6709))),
  );

  it("flags a Novatek gps atom in moov or at the top level", async () => {
    for (const file of [
      concat(
        head,
        box(
          "moov",
          box("gps ", u32(0x101), u32(1), u32(100), u32(4096)),
          box("udta", box([0xa9, 0x78, 0x79, 0x7a], text(FAKE_ISO6709))),
        ),
      ),
      concat(head, moov, box("GPS ", text("$GPRMC,1,A"))),
    ]) {
      const { out, scan } = await strip(file);
      expect(scan.unchecked).toBe("vendor-data");
      // The rest is still cleaned.
      expect(scan.stripped).toEqual(["gps"]);
      expect(contains(out, text(FAKE_ISO6709))).toBe(false);
    }
  });

  it("zeroes a top-level free or skip box that holds data", async () => {
    for (const type of ["free", "skip"]) {
      const file = concat(
        head,
        box(type, text("freeGPS "), new Uint8Array(64).fill(7)),
        moov,
      );
      const { out, scan } = await strip(file);
      expect(scan.unchecked ?? null).toBe(null);
      expect(scan.stripped).toContain("other");
      expect(contains(out, text("freeGPS "))).toBe(false);
      expect(out.byteLength).toBe(file.byteLength);
    }
  });

  it("zeroes an encoder note in a free box without refusing the file", async () => {
    const file = concat(
      head,
      box("free", text("IsoMedia File Produced by Google, 5-11-2011")),
      moov,
    );
    const { out, scan } = await strip(file);
    expect(scan.unchecked ?? null).toBe(null);
    expect(contains(out, text("Produced by Google"))).toBe(false);
  });

  it("finds data at the end of a big free box", async () => {
    const payload = new Uint8Array(200 * 1024);
    payload[payload.length - 1] = 1;
    const file = concat(head, box("free", payload), moov);
    const { out, scan } = await strip(file);
    expect(scan.unchecked ?? null).toBe(null);
    expect(out.byteLength).toBe(file.byteLength);
    const start = head.byteLength + 8;
    expect(
      out.subarray(start, start + payload.length).every((b) => b === 0),
    ).toBe(true);
  });

  it("leaves zeroed and empty padding alone", async () => {
    const file = concat(
      head,
      box("free"),
      box("free", new Uint8Array(200 * 1024)),
      box("skip", new Uint8Array(16)),
      moov,
    );
    const { scan } = await strip(file);
    expect(scan.unchecked).toBeNull();
    expect(scan.stripped).toEqual(["gps"]);
  });
});
