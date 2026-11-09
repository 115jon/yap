/**
 * Whether an uploaded profile widget image moves. Free plans keep widget
 * images still (animation is the `animatedProfile` perk), so the upload
 * route asks this of every GIF and WebP. Pure: bytes in, boolean out.
 *
 * GIF: more than one image descriptor (0x2C) after the header, found by
 * walking the blocks, not by searching bytes (pixel data can hold 0x2C).
 * WebP: the extended format's ANIM flag in the VP8X chunk.
 */

function gifFrameCount(bytes: Uint8Array, stopAt = 2): number {
  if (bytes.length < 13) return 0;
  let offset = 13;
  const packed = bytes[10];
  if (packed & 0x80) offset += 3 * (1 << ((packed & 0x07) + 1));
  let frames = 0;
  const skipSubBlocks = () => {
    while (offset < bytes.length) {
      const size = bytes[offset];
      offset += 1;
      if (size === 0) return;
      offset += size;
    }
  };
  while (offset < bytes.length) {
    const marker = bytes[offset];
    if (marker === 0x3b) break; // trailer
    if (marker === 0x21) {
      // extension: introducer, label, sub-blocks
      offset += 2;
      skipSubBlocks();
    } else if (marker === 0x2c) {
      frames += 1;
      if (frames >= stopAt) return frames;
      if (offset + 10 > bytes.length) break;
      const local = bytes[offset + 9];
      offset += 10;
      if (local & 0x80) offset += 3 * (1 << ((local & 0x07) + 1));
      offset += 1; // LZW minimum code size
      skipSubBlocks();
    } else {
      break;
    }
  }
  return frames;
}

function webpIsAnimated(bytes: Uint8Array): boolean {
  // RIFF....WEBPVP8X, flags byte at 20; bit 1 (0x02) is animation.
  if (bytes.length < 21) return false;
  const chunk = String.fromCharCode(bytes[12], bytes[13], bytes[14], bytes[15]);
  return chunk === "VP8X" && (bytes[20] & 0x02) !== 0;
}

export function isAnimatedImage(
  buffer: ArrayBuffer,
  mimeType: string,
): boolean {
  const bytes = new Uint8Array(buffer);
  if (mimeType === "image/gif") return gifFrameCount(bytes) > 1;
  if (mimeType === "image/webp") return webpIsAnimated(bytes);
  return false;
}
