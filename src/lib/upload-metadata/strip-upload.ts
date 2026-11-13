// ── Stripping metadata from uploads, route side ─────────────────────────────
//
// Every route that stores user bytes calls one of these before (or, for a
// chunked upload, right after) the bytes reach R2:
//
//   stripMetadataInPlace     an ArrayBuffer the route already holds
//   stripMetadataFromFile    a multipart/form-data File (attachments)
//   stripMetadataFromR2Object an object R2 assembled from parts
//
// Each logs what it removed by kind ("gps", "camera", ...), never a value,
// and `metadataResponseFields` gives the routes the same list to return, so
// a client can say "location removed".
//
// Fail-closed rule: a route that only takes images (avatar, banner,
// nameplate, server icon, profile board, camera background, voice status
// media) refuses a JPEG, PNG, WebP or GIF the scanner couldn't finish
// (`uncleanImageError`), since it may still say where it was taken.
// Attachments are stored and flagged instead (`metadata_unchecked`), and
// formats the scanner doesn't clean (HEIF, AVIF, TIFF, video, anything
// else) pass through on every route, flagged when they may carry metadata.

import { logger } from "@/lib/logger";

import {
  applyPatches,
  blobSource,
  bufferSource,
  type MetadataFormat,
  type MetadataKind,
  type MetadataScan,
  patchingStream,
  r2Source,
  scanUploadMetadata,
  type UncheckedReason,
} from "./index";

export interface UploadMetadataContext {
  /** Which upload path, for the log line ("avatar", "attachment", ...). */
  route: string;
  userId?: string;
  [field: string]: unknown;
}

export interface UploadMetadataFields {
  /** Kinds of metadata removed from the stored file. */
  metadata_stripped?: MetadataKind[];
  /** Set when the stored file may still carry metadata. */
  metadata_unchecked?: UncheckedReason;
}

/** Bytes an R2 scan may read (by range) before it gives up. */
const R2_SCAN_BUDGET = 64 * 1024 * 1024;

function report(scan: MetadataScan, context: UploadMetadataContext) {
  if (scan.stripped.length > 0) {
    logger.info("upload_metadata_stripped", {
      ...context,
      format: scan.format,
      kinds: scan.stripped,
    });
  }
  if (scan.unchecked) {
    logger.info("upload_metadata_unchecked", {
      ...context,
      format: scan.format,
      reason: scan.unchecked,
    });
  }
}

export function metadataResponseFields(
  scan: MetadataScan,
): UploadMetadataFields {
  const fields: UploadMetadataFields = {};
  if (scan.stripped.length > 0) fields.metadata_stripped = scan.stripped;
  if (scan.unchecked) fields.metadata_unchecked = scan.unchecked;
  return fields;
}

/** The formats the scanner cleans, so the ones it can fail on. */
const CLEANED_IMAGE_FORMATS: ReadonlySet<MetadataFormat> = new Set([
  "jpeg",
  "png",
  "webp",
  "gif",
]);

export const UNCHECKED_IMAGE_MESSAGE =
  "This image couldn't be checked for location data, so it wasn't uploaded. Save it again as a JPEG or PNG and try again.";

/**
 * The error an image-only route returns (as a 415) instead of storing the
 * file, or null when the file may be stored: a JPEG, PNG, WebP or GIF
 * whose scan stopped early (malformed, too large) may still carry its
 * location.
 */
export function uncleanImageError(scan: MetadataScan): string | null {
  return scan.unchecked && CLEANED_IMAGE_FORMATS.has(scan.format)
    ? UNCHECKED_IMAGE_MESSAGE
    : null;
}

/** Cleans an upload the route holds in memory; `buffer` is changed. */
export async function stripMetadataInPlace(
  buffer: ArrayBuffer | Uint8Array,
  context: UploadMetadataContext,
): Promise<MetadataScan> {
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  const scan = await scanUploadMetadata(bufferSource(bytes));
  applyPatches(bytes, scan.patches);
  report(scan, context);
  return scan;
}

/**
 * Cleans a form upload. The File is scanned by slices; only a file that
 * needs a change is copied into memory, so a big zip still goes to R2 as
 * the File itself.
 */
export async function stripMetadataFromFile(
  file: File,
  context: UploadMetadataContext,
): Promise<{ body: File | Uint8Array; scan: MetadataScan }> {
  const scan = await scanUploadMetadata(blobSource(file));
  report(scan, context);
  if (scan.patches.length === 0) return { body: file, scan };
  const bytes = new Uint8Array(await file.arrayBuffer());
  applyPatches(bytes, scan.patches);
  return { body: bytes, scan };
}

/**
 * Cleans an object already in R2 (a chunked upload R2 assembled): scans it
 * by range within R2_SCAN_BUDGET and, when something has to go, streams it
 * back to the same key with the patches applied. Same size, same metadata
 * headers. Throws if the rewrite fails; the object is then unchanged.
 */
export async function stripMetadataFromR2Object(
  bucket: R2Bucket,
  key: string,
  size: number,
  context: UploadMetadataContext,
): Promise<MetadataScan> {
  const scan = await scanUploadMetadata(r2Source(bucket, key, size), {
    budget: R2_SCAN_BUDGET,
  });
  if (scan.patches.length > 0) {
    const object = await bucket.get(key);
    if (!object) throw new Error("uploaded object is gone");
    const { readable, writable } = new FixedLengthStream(object.size);
    const piped = object.body
      .pipeThrough(patchingStream(scan.patches))
      .pipeTo(writable);
    await Promise.all([
      bucket.put(key, readable, {
        httpMetadata: object.httpMetadata,
        customMetadata: object.customMetadata,
      }),
      piped,
    ]);
  }
  report(scan, context);
  return scan;
}
