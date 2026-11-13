// ── Upload metadata stripping ───────────────────────────────────────────────
//
// `scanUploadMetadata` finds the privacy-sensitive metadata in an uploaded
// file (EXIF with its GPS, camera and time tags, XMP, IPTC, comments, video
// location atoms) and returns same-length patches that blank it, plus the
// kinds it found, never the values. JPEG, PNG, WebP, GIF and MP4/MOV are
// cleaned; HEIF/HEIC, AVIF, TIFF, JPEG XL and other ISO BMFF brands (Canon
// CR3) are reported as unchecked and left as they are; anything else
// passes through.
//
// Pure TypeScript and bounded: an in-memory file is patched where it lies,
// a big one in R2 is read by range within a byte budget and rewritten
// through `patchingStream` (see strip-upload.ts for the route side).

import {
  type ByteSource,
  type Patch,
  Reader,
  type ReaderOptions,
  ScanBudgetExceeded,
  startsWith,
  u32be,
} from "./bytes";
import { METADATA_KIND_ORDER, type MetadataKind } from "./exif";
import { isGif, scanGif } from "./gif";
import { isoBrand, MoovTooLarge, scanIsoVideo } from "./isobmff";
import { isJpeg, scanJpeg } from "./jpeg";
import { isPng, scanPng } from "./png";
import type { ScanState, UncheckedReason } from "./scan-state";
import { isWebp, scanWebp } from "./webp";

export {
  applyPatches,
  blobSource,
  bufferSource,
  type ByteSource,
  type Patch,
  patchingStream,
  r2Source,
} from "./bytes";
export type { MetadataKind } from "./exif";
export type { UncheckedReason } from "./scan-state";

export type MetadataFormat =
  | "jpeg"
  | "png"
  | "webp"
  | "gif"
  | "video"
  | "heif"
  | "avif"
  | "tiff"
  | "jxl"
  /** ISO BMFF with a brand that is neither a known video nor HEIF/AVIF. */
  | "isobmff"
  | "other";

export interface MetadataScan {
  format: MetadataFormat;
  /** Kinds of metadata the patches remove, in a fixed order. */
  stripped: MetadataKind[];
  /** Set when the file may still carry metadata. */
  unchecked: UncheckedReason | null;
  patches: Patch[];
}

const HEAD_BYTES = 64;
/** Bytes 4..11 of the JPEG XL container signature box. */
const JXL_SIGNATURE = "JXL \r\n\x87\n";

/** Formats that may carry metadata this scanner can't remove. */
const UNSUPPORTED_FORMATS: ReadonlySet<MetadataFormat> = new Set([
  "heif",
  "avif",
  "tiff",
  "jxl",
  "isobmff",
]);

type Scanner = (reader: Reader, state: ScanState) => Promise<void>;

const SCANNERS: Partial<Record<MetadataFormat, Scanner>> = {
  jpeg: (reader, state) => scanJpeg(reader, state),
  png: scanPng,
  webp: scanWebp,
  gif: scanGif,
  video: scanIsoVideo,
};

export function detectFormat(head: Uint8Array): MetadataFormat {
  if (isJpeg(head)) return "jpeg";
  if (isPng(head)) return "png";
  if (isWebp(head)) return "webp";
  if (isGif(head)) return "gif";
  if (startsWith(head, 0, "II*\0") || startsWith(head, 0, "MM\0*")) {
    return "tiff";
  }
  // The JPEG XL container (its bare codestream can't hold EXIF or XMP).
  if (u32be(head, 0) === 12 && startsWith(head, 4, JXL_SIGNATURE)) {
    return "jxl";
  }
  return isoBrand(head) ?? "other";
}

export async function scanUploadMetadata(
  source: ByteSource,
  options: ReaderOptions = {},
): Promise<MetadataScan> {
  const head = await source.read(0, Math.min(source.size, HEAD_BYTES));
  const format = detectFormat(head);
  const state: ScanState = { kinds: new Set(), patches: [] };
  let unchecked: UncheckedReason | null = null;

  const scanner = SCANNERS[format];

  if (UNSUPPORTED_FORMATS.has(format)) {
    unchecked = "unsupported-format";
  } else if (scanner) {
    try {
      await scanner(new Reader(source, options), state);
      unchecked = state.unchecked ?? null;
    } catch (error) {
      // Whatever was found before the scan stopped is still stripped.
      unchecked =
        error instanceof ScanBudgetExceeded || error instanceof MoovTooLarge
          ? "too-large"
          : "malformed";
    }
  }

  return {
    format,
    stripped: METADATA_KIND_ORDER.filter((kind) => state.kinds.has(kind)),
    unchecked,
    patches: state.patches,
  };
}
