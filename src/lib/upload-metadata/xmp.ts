// ── XMP packets ─────────────────────────────────────────────────────────────
//
// XMP is stripped unless every namespace it declares is one that only
// describes how to draw the picture: an Ultra HDR photo's gain-map packets
// (hdrgm, the Google container directory). Anything else (dates, tools,
// location, people, ratings) goes with the packet.

import type { MetadataKind } from "./exif";

const SAFE_XMP_NAMESPACES: ReadonlySet<string> = new Set([
  "adobe:ns:meta/",
  "http://www.w3.org/1999/02/22-rdf-syntax-ns#",
  "http://ns.adobe.com/hdr-gain-map/1.0/",
  "http://ns.google.com/photos/1.0/container/",
  "http://ns.google.com/photos/1.0/container/item/",
]);

const GPS_PATTERN =
  /GPS(?:Latitude|Longitude|Altitude|Coordinates)|LocationCreated|LocationShown/;
const CAMERA_PATTERN =
  /tiff:Make|tiff:Model|SerialNumber|aux:Lens|exifEX:Lens|exif:Lens/;
const DATE_PATTERN = /DateTimeOriginal|xmp:CreateDate|photoshop:DateCreated/;
const AUTHOR_PATTERN = /dc:creator|dc:rights|CameraOwnerName/;
const NAMESPACE_PATTERN = /xmlns:[A-Za-z0-9_.-]+\s*=\s*["']([^"']*)["']/g;

function text(bytes: Uint8Array): string {
  return new TextDecoder("utf-8", { fatal: false }).decode(bytes);
}

/** What an XMP packet holds, by kind. Always includes "xmp". */
export function summarizeXmp(bytes: Uint8Array): Set<MetadataKind> {
  const xml = text(bytes);
  const kinds = new Set<MetadataKind>(["xmp"]);
  if (GPS_PATTERN.test(xml)) kinds.add("gps");
  if (CAMERA_PATTERN.test(xml)) kinds.add("camera");
  if (DATE_PATTERN.test(xml)) kinds.add("timestamp");
  if (AUTHOR_PATTERN.test(xml)) kinds.add("author");
  return kinds;
}

/** Whether an XMP packet only carries rendering data and may stay. */
export function isRenderOnlyXmp(bytes: Uint8Array): boolean {
  const xml = text(bytes);
  if (GPS_PATTERN.test(xml)) return false;
  let declared = 0;
  for (const match of xml.matchAll(NAMESPACE_PATTERN)) {
    declared += 1;
    if (!SAFE_XMP_NAMESPACES.has(match[1])) return false;
  }
  return declared > 0;
}
