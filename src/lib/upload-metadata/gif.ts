// ── GIF ─────────────────────────────────────────────────────────────────────
//
// Comment extensions keep their block structure (every sub-block length
// byte stays) and have their data zeroed, so a decoder walks past them as
// before. Frames, colour tables, the NETSCAPE loop block and ICC
// application blocks are never touched.
//
// XMP is different: Adobe writes the packet as raw bytes, not sub-blocks,
// followed by a 258-byte "magic trailer" (0x01, 0xFF down to 0x00, then the
// terminator) that brings a decoder reading packet bytes as block lengths
// back to the terminator wherever it lands. The identifier is zeroed and
// the packet becomes zeros with a 255 length byte every 256 bytes: a chain
// of blocks that runs into the trailer, which stays as it is. An XMP
// extension without the trailer is cleaned like a comment.

import { MalformedFile, type Reader, startsWith } from "./bytes";
import { addKinds, type ScanState, write, zero } from "./scan-state";
import { summarizeXmp } from "./xmp";

/**
 * Blocks and sub-blocks walked per file: a 255-byte sub-block each is
 * about 500 MB of frames, more than any upload limit.
 */
const MAX_BLOCKS = 2_000_000;
const XMP_ID = "XMP DataXMP";
const XMP_TRAILER_LENGTH = 258;
/** XMP packets bigger than this are not read or rewritten: flagged. */
const MAX_XMP = 1024 * 1024;

function isXmpTrailer(b: Uint8Array): boolean {
  if (b.byteLength !== XMP_TRAILER_LENGTH || b[0] !== 1 || b[257] !== 0) {
    return false;
  }
  for (let k = 1; k <= 256; k += 1) if (b[k] !== 256 - k) return false;
  return true;
}

/**
 * `length` bytes of zeros with 255 at every 256th byte: from its start a
 * decoder skips 256 bytes at a time and ends up inside the magic trailer
 * that follows, at most 255 bytes in.
 */
function blockChain(length: number): Uint8Array {
  const out = new Uint8Array(length);
  for (let k = 0; k < length; k += 256) out[k] = 255;
  return out;
}

export function isGif(head: Uint8Array): boolean {
  return startsWith(head, 0, "GIF87a") || startsWith(head, 0, "GIF89a");
}

function colorTableSize(packed: number): number {
  return packed & 0x80 ? 3 * (1 << ((packed & 0x07) + 1)) : 0;
}

export async function scanGif(reader: Reader, state: ScanState): Promise<void> {
  const screen = await reader.exact(0, 13);
  let pos = 13 + colorTableSize(screen[10]);
  let blocks = 0;

  /** Walks sub-blocks from `at`, zeroing their data when `wipe`. */
  const subBlocks = async (at: number, wipe: boolean): Promise<number> => {
    let p = at;
    for (; ; blocks += 1) {
      if (blocks >= MAX_BLOCKS) throw new MalformedFile("too many blocks");
      const size = await reader.u8(p);
      if (size === 0) return p + 1;
      if (wipe) zero(state, p + 1, size);
      p += 1 + size;
    }
  };

  /** Cleans the XMP application extension whose identifier is at `at`. */
  const xmpExtension = async (at: number): Promise<number> => {
    state.kinds.add("xmp");
    const packetStart = at + 12;
    // Where a decoder ends up: past the trailer's terminator in Adobe's
    // layout, past the last block's in a sub-block one.
    const end = await subBlocks(packetStart, false);
    const trailerStart = end - XMP_TRAILER_LENGTH;
    const adobe =
      trailerStart >= packetStart &&
      isXmpTrailer(await reader.exact(trailerStart, XMP_TRAILER_LENGTH));
    const packetEnd = adobe ? trailerStart : end;
    zero(state, at + 1, 11);
    if (packetEnd - packetStart > MAX_XMP) {
      throw new MalformedFile("XMP packet too large");
    }
    const packet = await reader.exact(packetStart, packetEnd - packetStart);
    addKinds(state, summarizeXmp(packet));
    if (adobe) {
      write(
        state,
        packetStart,
        packet.byteLength,
        blockChain(packet.byteLength),
      );
      return end;
    }
    return subBlocks(packetStart, true);
  };

  // Every way out of the walk but the trailer or the end of the file
  // throws: a scan that stopped early can't say the rest is clean.
  while (pos < reader.size) {
    blocks += 1;
    if (blocks >= MAX_BLOCKS) throw new MalformedFile("too many blocks");
    const introducer = await reader.u8(pos);
    if (introducer === 0x3b) return; // trailer
    if (introducer === 0x2c) {
      const descriptor = await reader.exact(pos, 10);
      pos += 10 + colorTableSize(descriptor[9]) + 1; // + LZW minimum code size
      pos = await subBlocks(pos, false);
    } else if (introducer === 0x21) {
      const label = await reader.u8(pos + 1);
      pos += 2;
      if (label === 0xfe) {
        state.kinds.add("comment");
        pos = await subBlocks(pos, true);
      } else if (label === 0xff) {
        const id = await reader.bytes(pos, 12);
        if (id[0] === 11 && startsWith(id, 1, XMP_ID)) {
          pos = await xmpExtension(pos);
        } else {
          pos = await subBlocks(pos, false);
        }
      } else {
        pos = await subBlocks(pos, false);
      }
    } else {
      throw new MalformedFile(`unknown block at ${pos}`);
    }
  }
}
