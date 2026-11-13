// ── GIF ─────────────────────────────────────────────────────────────────────
//
// Comment extensions and XMP application extensions keep their block
// structure (every sub-block length byte stays) and have their data zeroed,
// so a decoder walks past them as before. Frames, colour tables, the
// NETSCAPE loop block and ICC application blocks are never touched.

import { type Reader, startsWith } from "./bytes";
import { type ScanState, zero } from "./scan-state";

/**
 * Blocks and sub-blocks walked per file: a 255-byte sub-block each is
 * about 500 MB of frames, more than any upload limit.
 */
const MAX_BLOCKS = 2_000_000;

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
    for (; blocks < MAX_BLOCKS; blocks += 1) {
      const size = await reader.u8(p);
      if (size === 0) return p + 1;
      if (wipe) zero(state, p + 1, size);
      p += 1 + size;
    }
    return reader.size;
  };

  while (pos < reader.size && blocks < MAX_BLOCKS) {
    blocks += 1;
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
        const xmp = id[0] === 11 && startsWith(id, 1, "XMP DataXMP");
        if (xmp) state.kinds.add("xmp");
        pos = await subBlocks(pos, xmp);
      } else {
        pos = await subBlocks(pos, false);
      }
    } else {
      return; // not a block we know: leave the rest alone
    }
  }
}
