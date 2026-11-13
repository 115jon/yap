// ── Byte access for the metadata scanners ───────────────────────────────────
//
// The scanners never hold a whole upload unless it is already in memory: they
// read what they need through a `Reader`, which fetches fixed windows from a
// `ByteSource` (a buffer, a Blob, or an R2 object by range) and stops at a
// byte budget. What they find comes back as `Patch`es: same-length overwrites,
// so a cleaned file keeps every offset (and so its MPF, iloc and chunk
// tables) and can be written back as a stream of known length.

export interface ByteSource {
  readonly size: number;
  /** Bytes [offset, offset + length), cut short at the end of the source. */
  read(offset: number, length: number): Promise<Uint8Array>;
}

/**
 * An overwrite of `length` bytes at `offset`: `bytes` (when given) first,
 * zeros after it.
 */
export interface Patch {
  offset: number;
  length: number;
  bytes?: Uint8Array;
}

/** A scan read more than its budget allows (a huge file in R2). */
export class ScanBudgetExceeded extends Error {
  constructor() {
    super("metadata scan budget exceeded");
  }
}

/** A structure the scanner can't follow (truncated, lying lengths). */
export class MalformedFile extends Error {}

export function bufferSource(bytes: Uint8Array): ByteSource {
  return {
    size: bytes.byteLength,
    read: async (offset, length) =>
      bytes.subarray(offset, Math.min(bytes.byteLength, offset + length)),
  };
}

export function blobSource(blob: Blob): ByteSource {
  return {
    size: blob.size,
    read: async (offset, length) =>
      new Uint8Array(
        await blob
          .slice(offset, Math.min(blob.size, offset + length))
          .arrayBuffer(),
      ),
  };
}

export function r2Source(
  bucket: R2Bucket,
  key: string,
  size: number,
): ByteSource {
  return {
    size,
    read: async (offset, length) => {
      const end = Math.min(size, offset + length);
      if (end <= offset) return new Uint8Array(0);
      const object = await bucket.get(key, {
        range: { offset, length: end - offset },
      });
      if (!object) throw new MalformedFile("object disappeared mid-scan");
      return new Uint8Array(await object.arrayBuffer());
    },
  };
}

export interface ReaderOptions {
  /** Bytes fetched per read (small reads share one window). */
  windowSize?: number;
  /** Total bytes the reader may fetch before it gives up. */
  budget?: number;
}

/** Windowed, budgeted reads over a `ByteSource`. */
export class Reader {
  readonly size: number;
  private readonly windowSize: number;
  private readonly budget: number;
  private fetched = 0;
  private window: Uint8Array = new Uint8Array(0);
  private windowStart = 0;

  constructor(
    private readonly source: ByteSource,
    options: ReaderOptions = {},
  ) {
    this.size = source.size;
    this.windowSize = options.windowSize ?? 1024 * 1024;
    this.budget = options.budget ?? Number.POSITIVE_INFINITY;
  }

  /** Bytes [offset, offset + length), shorter only at the end of the file. */
  async bytes(offset: number, length: number): Promise<Uint8Array> {
    const end = Math.min(this.size, offset + length);
    if (offset < 0 || end <= offset) return new Uint8Array(0);
    const windowEnd = this.windowStart + this.window.byteLength;
    if (offset >= this.windowStart && end <= windowEnd) {
      return this.window.subarray(
        offset - this.windowStart,
        end - this.windowStart,
      );
    }
    if (end - offset > this.windowSize) {
      return this.fetch(offset, end - offset);
    }
    this.window = await this.fetch(offset, this.windowSize);
    this.windowStart = offset;
    return this.window.subarray(0, end - offset);
  }

  /** Exactly `length` bytes, or `MalformedFile`. */
  async exact(offset: number, length: number): Promise<Uint8Array> {
    const bytes = await this.bytes(offset, length);
    if (bytes.byteLength !== length) {
      throw new MalformedFile(`truncated at ${offset}`);
    }
    return bytes;
  }

  async u8(offset: number): Promise<number> {
    return (await this.exact(offset, 1))[0];
  }

  private async fetch(offset: number, length: number): Promise<Uint8Array> {
    const want = Math.min(length, this.size - offset);
    if (this.fetched + want > this.budget) throw new ScanBudgetExceeded();
    this.fetched += want;
    return this.source.read(offset, want);
  }
}

export function u16be(b: Uint8Array, i: number): number {
  return (b[i] << 8) | b[i + 1];
}

export function u32be(b: Uint8Array, i: number): number {
  return ((b[i] << 24) | (b[i + 1] << 16) | (b[i + 2] << 8) | b[i + 3]) >>> 0;
}

export function u32le(b: Uint8Array, i: number): number {
  return (b[i] | (b[i + 1] << 8) | (b[i + 2] << 16) | (b[i + 3] << 24)) >>> 0;
}

/** Latin-1 text of `length` bytes at `i` (for four-character codes). */
export function ascii(b: Uint8Array, i: number, length: number): string {
  let out = "";
  for (let k = i; k < i + length && k < b.byteLength; k += 1) {
    out += String.fromCharCode(b[k]);
  }
  return out;
}

export function startsWith(b: Uint8Array, i: number, text: string): boolean {
  if (i + text.length > b.byteLength) return false;
  for (let k = 0; k < text.length; k += 1) {
    if (b[i + k] !== text.charCodeAt(k)) return false;
  }
  return true;
}

export function asciiBytes(text: string): Uint8Array {
  const out = new Uint8Array(text.length);
  for (let k = 0; k < text.length; k += 1) out[k] = text.charCodeAt(k);
  return out;
}

/** Writes every patch into `bytes`, which must be the scanned file. */
export function applyPatches(bytes: Uint8Array, patches: Patch[]): void {
  for (const patch of patches) {
    const end = Math.min(bytes.byteLength, patch.offset + patch.length);
    if (end <= patch.offset) continue;
    bytes.fill(0, patch.offset, end);
    if (patch.bytes) {
      bytes.set(patch.bytes.subarray(0, end - patch.offset), patch.offset);
    }
  }
}

/**
 * A stream that writes `patches` into the bytes passing through it: the same
 * file, the same length, metadata overwritten. For a file too big to hold.
 */
export function patchingStream(
  patches: Patch[],
): TransformStream<Uint8Array, Uint8Array> {
  // Scanners emit patches in file order, so this is almost always a check,
  // not a sort (and there are at most MAX_PATCHES of them either way).
  const sorted = patches.every(
    (patch, k) => k === 0 || patches[k - 1].offset <= patch.offset,
  )
    ? patches
    : [...patches].sort((a, b) => a.offset - b.offset);
  let position = 0;
  let first = 0;
  return new TransformStream<Uint8Array, Uint8Array>({
    transform(chunk, controller) {
      const start = position;
      const end = start + chunk.byteLength;
      position = end;
      while (
        first < sorted.length &&
        sorted[first].offset + sorted[first].length <= start
      ) {
        first += 1;
      }
      let out = chunk;
      for (let k = first; k < sorted.length; k += 1) {
        const patch = sorted[k];
        if (patch.offset >= end) break;
        const from = Math.max(start, patch.offset);
        const to = Math.min(end, patch.offset + patch.length);
        if (to <= from) continue;
        if (out === chunk) out = chunk.slice();
        out.fill(0, from - start, to - start);
        if (patch.bytes) {
          const bytesFrom = from - patch.offset;
          const bytesTo = Math.min(patch.bytes.byteLength, to - patch.offset);
          if (bytesTo > bytesFrom) {
            out.set(patch.bytes.subarray(bytesFrom, bytesTo), from - start);
          }
        }
      }
      controller.enqueue(out);
    },
  });
}
