import { describe, expect, it, vi } from "vitest";

import {
  backoffDelay,
  DEFAULT_RETRY,
  isRetryableUploadError,
  localStorageResumeStore,
  memoryResumeStore,
  multipartResumeKey,
  partRange,
  uploadInParts,
  type MultipartStatus,
  type MultipartTransport,
  type UploadProgress,
} from "../multipart-upload";

const PART = 10;

function file(size: number, name = "big.bin") {
  const data = new Uint8Array(size).map((_, i) => i % 251);
  const blob = new Blob([data]);
  return {
    data,
    file: Object.assign(blob, { name, lastModified: 1700000000000 }) as Blob & {
      name: string;
      lastModified: number;
    },
  };
}

function httpError(status: number, extra: Record<string, unknown> = {}) {
  return Object.assign(new Error(`HTTP ${status}`), { status, ...extra });
}

/** A transport over an in-memory "server" that records what arrives. */
function fakeServer(options: { partSize?: number } = {}) {
  const partSize = options.partSize ?? PART;
  const uploads = new Map<
    string,
    { size: number; parts: Map<number, Uint8Array>; aborted: boolean }
  >();
  let nextId = 1;
  let inFlight = 0;
  let maxInFlight = 0;
  const putCalls: number[] = [];
  /** Per-part hooks: throw or delay before a part is stored. */
  const beforePut: Array<(n: number, attempt: number) => Promise<void> | void> =
    [];
  const attempts = new Map<number, number>();

  const transport: MultipartTransport = {
    create: vi.fn(async ({ size }) => {
      const id = `up-${nextId++}`;
      uploads.set(id, { size, parts: new Map(), aborted: false });
      return {
        upload_id: id,
        part_size: partSize,
        part_count: Math.ceil(size / partSize),
        size,
        expires_at: new Date(Date.now() + 86_400_000).toISOString(),
      };
    }),
    status: vi.fn(async (uploadId): Promise<MultipartStatus> => {
      const upload = uploads.get(uploadId);
      if (!upload || upload.aborted)
        throw httpError(404, { code: "UPLOAD_NOT_FOUND" });
      return {
        upload_id: uploadId,
        status: "open",
        size: upload.size,
        part_size: partSize,
        part_count: Math.ceil(upload.size / partSize),
        expires_at: new Date(Date.now() + 86_400_000).toISOString(),
        parts: [...upload.parts.entries()].map(([n, bytes]) => ({
          part_number: n,
          size: bytes.length,
        })),
      };
    }),
    putPart: vi.fn(async (uploadId, n, body, { signal, onProgress }) => {
      const attempt = (attempts.get(n) ?? 0) + 1;
      attempts.set(n, attempt);
      putCalls.push(n);
      inFlight += 1;
      maxInFlight = Math.max(maxInFlight, inFlight);
      try {
        for (const hook of beforePut) await hook(n, attempt);
        // Let the other workers start before this one finishes.
        await new Promise((resolve) => setTimeout(resolve, 1));
        if (signal?.aborted) {
          throw new DOMException("aborted", "AbortError");
        }
        const bytes = new Uint8Array(await body.arrayBuffer());
        onProgress?.(Math.floor(bytes.length / 2));
        onProgress?.(bytes.length);
        uploads.get(uploadId)!.parts.set(n, bytes);
      } finally {
        inFlight -= 1;
      }
    }),
    complete: vi.fn(async (uploadId) => {
      const upload = uploads.get(uploadId)!;
      const count = Math.ceil(upload.size / partSize);
      const chunks: Uint8Array[] = [];
      for (let n = 1; n <= count; n += 1) {
        const part = upload.parts.get(n);
        if (!part) throw httpError(400, { code: "UPLOAD_INCOMPLETE" });
        chunks.push(part);
      }
      const joined = new Uint8Array(upload.size);
      let offset = 0;
      for (const chunk of chunks) {
        joined.set(chunk, offset);
        offset += chunk.length;
      }
      (transport as { assembled?: Uint8Array }).assembled = joined;
      return {
        id: `att-${uploadId}`,
        file_url: `/api/attachments/c/${uploadId}/big.bin`,
        file_name: "big.bin",
        file_size: upload.size,
        content_type: "application/octet-stream",
      };
    }),
    abort: vi.fn(async (uploadId) => {
      const upload = uploads.get(uploadId);
      if (upload) upload.aborted = true;
    }),
  };

  return {
    transport: transport as MultipartTransport & { assembled?: Uint8Array },
    uploads,
    putCalls,
    beforePut,
    get maxInFlight() {
      return maxInFlight;
    },
  };
}

const noSleep = vi.fn(async () => {});

describe("partRange", () => {
  it("splits a file into part_size ranges with a short last part", () => {
    expect(partRange(1, 10, 25)).toEqual([0, 10]);
    expect(partRange(2, 10, 25)).toEqual([10, 20]);
    expect(partRange(3, 10, 25)).toEqual([20, 25]);
  });
});

describe("uploadInParts: chunking and concurrency", () => {
  it("sends every part once and the server gets the same bytes", async () => {
    const server = fakeServer();
    const { data, file: f } = file(95);
    const result = await uploadInParts(f, {
      transport: server.transport,
      resumeKey: "k",
      sleep: noSleep,
    });
    expect(result.file_size).toBe(95);
    expect([...server.putCalls].sort((a, b) => a - b)).toEqual([
      1, 2, 3, 4, 5, 6, 7, 8, 9, 10,
    ]);
    expect(server.transport.assembled).toEqual(data);
    expect(server.transport.create).toHaveBeenCalledWith(
      {
        filename: "big.bin",
        size: 95,
        content_type: "application/octet-stream",
      },
      undefined,
    );
  });

  it("keeps at most three parts in flight by default", async () => {
    const server = fakeServer();
    await uploadInParts(file(200).file, {
      transport: server.transport,
      resumeKey: "k",
      sleep: noSleep,
    });
    expect(server.maxInFlight).toBe(3);
  });

  it("honours a lower concurrency", async () => {
    const server = fakeServer();
    await uploadInParts(file(60).file, {
      transport: server.transport,
      resumeKey: "k",
      concurrency: 1,
      sleep: noSleep,
    });
    expect(server.maxInFlight).toBe(1);
  });

  it("reports progress in bytes up to the total", async () => {
    const server = fakeServer();
    const seen: UploadProgress[] = [];
    await uploadInParts(file(45).file, {
      transport: server.transport,
      resumeKey: "k",
      onProgress: (p) => seen.push(p),
      sleep: noSleep,
    });
    expect(seen[0]).toEqual({ loaded: 0, total: 45 });
    expect(seen.at(-1)).toEqual({ loaded: 45, total: 45 });
    expect(seen.every((p) => p.loaded <= p.total)).toBe(true);
    // Part progress shows mid-part, not only per finished part.
    expect(seen.some((p) => p.loaded % PART !== 0 && p.loaded !== 45)).toBe(
      true,
    );
  });
});

describe("uploadInParts: retry", () => {
  it("retries a part after network errors with backoff", async () => {
    const server = fakeServer();
    server.beforePut.push((n, attempt) => {
      if (n === 2 && attempt <= 2)
        throw new TypeError("Network request failed");
    });
    const sleep = vi.fn(async (_ms: number) => {});
    const { data, file: f } = file(30);
    await uploadInParts(f, {
      transport: server.transport,
      resumeKey: "k",
      sleep,
      random: () => 1,
    });
    expect(server.transport.assembled).toEqual(data);
    expect(sleep.mock.calls.map(([ms]) => ms)).toEqual([500, 1000]);
  });

  it("retries 429 and 5xx, honouring Retry-After", async () => {
    const server = fakeServer();
    server.beforePut.push((n, attempt) => {
      if (n === 1 && attempt === 1)
        throw httpError(429, { retryAfterMs: 3000 });
      if (n === 1 && attempt === 2) throw httpError(502);
    });
    const sleep = vi.fn(async (_ms: number) => {});
    await uploadInParts(file(10).file, {
      transport: server.transport,
      resumeKey: "k",
      concurrency: 1,
      sleep,
      random: () => 0,
    });
    // The 429 waited what it asked without spending an attempt, so the 502
    // gets the first backoff step.
    expect(sleep.mock.calls.map(([ms]) => ms)).toEqual([3000, 250]);
  });

  it("honours a Retry-After past the backoff cap, up to 60 s", async () => {
    const server = fakeServer();
    server.beforePut.push((n, attempt) => {
      if (n === 1 && attempt === 1)
        throw httpError(429, { retryAfterMs: 30_000 });
      if (n === 1 && attempt === 2)
        throw httpError(503, { retryAfterMs: 600_000 });
    });
    const sleep = vi.fn(async (_ms: number) => {});
    await uploadInParts(file(10).file, {
      transport: server.transport,
      resumeKey: "k",
      concurrency: 1,
      sleep,
    });
    expect(sleep.mock.calls.map(([ms]) => ms)).toEqual([30_000, 60_000]);
  });

  it("does not count Retry-After waits as attempts, but bounds them", async () => {
    const server = fakeServer();
    // Two attempts allowed; the server says "later" four times first.
    server.beforePut.push((n, attempt) => {
      if (attempt <= 4) throw httpError(429, { retryAfterMs: 1000 });
    });
    const sleep = vi.fn(async (_ms: number) => {});
    await uploadInParts(file(10).file, {
      transport: server.transport,
      resumeKey: "k",
      retry: { ...DEFAULT_RETRY, attempts: 2 },
      sleep,
    });
    expect(server.putCalls).toEqual([1, 1, 1, 1, 1]);

    // A server that never stops saying "later" fails once the waits and the
    // attempts are spent: 2 waits, then 2 attempts (one backoff between).
    const stuck = fakeServer();
    stuck.beforePut.push(() => {
      throw httpError(429, { retryAfterMs: 1000 });
    });
    await expect(
      uploadInParts(file(10).file, {
        transport: stuck.transport,
        resumeKey: "k2",
        retry: { ...DEFAULT_RETRY, attempts: 2, maxRetryAfterWaits: 2 },
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ status: 429 });
    expect(stuck.putCalls).toEqual([1, 1, 1, 1]);
  });

  it("does not retry a 4xx that names the request as wrong", async () => {
    const server = fakeServer();
    server.beforePut.push(() => {
      throw httpError(400, { code: "PART_SIZE_MISMATCH" });
    });
    const store = memoryResumeStore();
    await expect(
      uploadInParts(file(30).file, {
        transport: server.transport,
        resumeKey: "k",
        store,
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ status: 400 });
    expect(server.transport.complete).not.toHaveBeenCalled();
    // Kept: a later try resumes the same upload.
    expect(store.get("k")).not.toBeNull();
  });

  it("gives up after the last attempt and keeps the upload for resume", async () => {
    const server = fakeServer();
    server.beforePut.push((n) => {
      if (n === 3) throw new TypeError("offline");
    });
    const store = memoryResumeStore();
    await expect(
      uploadInParts(file(30).file, {
        transport: server.transport,
        resumeKey: "k",
        store,
        sleep: noSleep,
      }),
    ).rejects.toThrow("offline");
    expect(server.putCalls.filter((n) => n === 3).length).toBe(
      DEFAULT_RETRY.attempts,
    );
    expect(store.get("k")?.uploadId).toBe("up-1");
  });

  it("forgets an upload the server no longer has", async () => {
    const server = fakeServer();
    server.beforePut.push(() => {
      throw httpError(404, { code: "UPLOAD_NOT_FOUND" });
    });
    const store = memoryResumeStore();
    await expect(
      uploadInParts(file(30).file, {
        transport: server.transport,
        resumeKey: "k",
        store,
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ status: 404 });
    expect(store.get("k")).toBeNull();
  });
});

describe("uploadInParts: resume", () => {
  it("sends only the parts the server is missing", async () => {
    const server = fakeServer();
    const store = memoryResumeStore();
    const { data, file: f } = file(40);
    server.beforePut.push((n, attempt) => {
      if (n === 3 && attempt === 1) throw httpError(400); // first run stops here
    });
    await expect(
      uploadInParts(f, {
        transport: server.transport,
        resumeKey: "k",
        store,
        concurrency: 1,
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ status: 400 });
    expect(server.putCalls).toEqual([1, 2, 3]);

    server.putCalls.length = 0;
    const seen: UploadProgress[] = [];
    await uploadInParts(f, {
      transport: server.transport,
      resumeKey: "k",
      store,
      concurrency: 1,
      onProgress: (p) => seen.push(p),
      sleep: noSleep,
    });
    expect(server.transport.create).toHaveBeenCalledTimes(1);
    expect(server.putCalls).toEqual([3, 4]);
    expect(seen[0]).toEqual({ loaded: 20, total: 40 });
    expect(server.transport.assembled).toEqual(data);
    expect(store.get("k")).toBeNull();
  });

  it("starts over when the saved upload is gone", async () => {
    const server = fakeServer();
    const store = memoryResumeStore();
    store.set("k", {
      uploadId: "up-gone",
      size: 30,
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    await uploadInParts(file(30).file, {
      transport: server.transport,
      resumeKey: "k",
      store,
      sleep: noSleep,
    });
    expect(server.transport.status).toHaveBeenCalledWith("up-gone", undefined);
    expect(server.transport.create).toHaveBeenCalledTimes(1);
  });

  it("ignores a saved upload that has expired or is for another size", async () => {
    const server = fakeServer();
    const store = memoryResumeStore();
    store.set("a", {
      uploadId: "up-old",
      size: 30,
      expiresAt: new Date(Date.now() - 1).toISOString(),
    });
    store.set("b", {
      uploadId: "up-other",
      size: 31,
      expiresAt: new Date(Date.now() + 60_000).toISOString(),
    });
    await uploadInParts(file(30).file, {
      transport: server.transport,
      resumeKey: "a",
      store,
      sleep: noSleep,
    });
    await uploadInParts(file(30).file, {
      transport: server.transport,
      resumeKey: "b",
      store,
      sleep: noSleep,
    });
    expect(server.transport.status).not.toHaveBeenCalled();
    expect(server.transport.create).toHaveBeenCalledTimes(2);
  });

  it("keys a file by channel, name, size and modified time", () => {
    const f = { name: "a.mp4", size: 5, lastModified: 7 };
    expect(multipartResumeKey("c1", f)).not.toBe(multipartResumeKey("c2", f));
    expect(multipartResumeKey("c1", f)).not.toBe(
      multipartResumeKey("c1", { ...f, lastModified: 8 }),
    );
    expect(multipartResumeKey("c1", f)).toBe(
      multipartResumeKey("c1", { ...f }),
    );
  });
});

describe("uploadInParts: cancel", () => {
  it("stops, aborts the server upload and forgets it", async () => {
    const server = fakeServer();
    const store = memoryResumeStore();
    const controller = new AbortController();
    server.beforePut.push((n) => {
      if (n === 2) controller.abort();
    });
    await expect(
      uploadInParts(file(100).file, {
        transport: server.transport,
        resumeKey: "k",
        store,
        signal: controller.signal,
        concurrency: 1,
        sleep: noSleep,
      }),
    ).rejects.toMatchObject({ name: "AbortError" });
    expect(server.transport.abort).toHaveBeenCalledWith("up-1");
    expect(server.transport.complete).not.toHaveBeenCalled();
    expect(server.putCalls.length).toBeLessThan(10);
    expect(store.get("k")).toBeNull();
  });
});

describe("retry helpers", () => {
  it("classifies errors", () => {
    expect(isRetryableUploadError(new TypeError("Failed to fetch"))).toBe(true);
    expect(isRetryableUploadError(httpError(503))).toBe(true);
    expect(isRetryableUploadError(httpError(429))).toBe(true);
    expect(isRetryableUploadError(httpError(400))).toBe(false);
    expect(isRetryableUploadError(httpError(413))).toBe(false);
    expect(isRetryableUploadError(httpError(404))).toBe(false);
    expect(isRetryableUploadError(new DOMException("x", "AbortError"))).toBe(
      false,
    );
  });

  it("backs off exponentially with jitter, capped", () => {
    const policy = { attempts: 9, baseDelayMs: 500, maxDelayMs: 10_000 };
    expect(backoffDelay(1, policy, () => 0)).toBe(250);
    expect(backoffDelay(1, policy, () => 1)).toBe(500);
    expect(backoffDelay(3, policy, () => 1)).toBe(2000);
    expect(backoffDelay(9, policy, () => 1)).toBe(10_000);
    // A server-named wait is not clamped to the backoff cap, only to 60 s.
    expect(backoffDelay(1, policy, () => 0, { retryAfterMs: 30_000 })).toBe(
      30_000,
    );
    expect(backoffDelay(1, policy, () => 0, { retryAfterMs: 120_000 })).toBe(
      60_000,
    );
  });
});

describe("localStorageResumeStore", () => {
  function fakeStorage() {
    const map = new Map<string, string>();
    return {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => void map.set(k, v),
      removeItem: (k: string) => void map.delete(k),
      map,
    } as unknown as Storage;
  }

  it("round-trips and drops expired entries on write", () => {
    const storage = fakeStorage();
    const store = localStorageResumeStore("t", () => storage);
    const live = new Date(Date.now() + 60_000).toISOString();
    store.set("a", { uploadId: "1", size: 1, expiresAt: live });
    store.set("old", {
      uploadId: "2",
      size: 1,
      expiresAt: new Date(Date.now() - 1).toISOString(),
    });
    expect(store.get("a")).toEqual({ uploadId: "1", size: 1, expiresAt: live });
    expect(store.get("old")).toBeNull();
    store.delete("a");
    expect(store.get("a")).toBeNull();
  });

  it("works (without resuming) when storage throws", () => {
    const store = localStorageResumeStore("t", () => {
      throw new Error("blocked");
    });
    expect(() =>
      store.set("a", { uploadId: "1", size: 1, expiresAt: "2999-01-01" }),
    ).not.toThrow();
    expect(store.get("a")).toBeNull();
  });
});
