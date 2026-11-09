/**
 * The client half of chunked attachment uploads (see
 * services/multipart-upload.service for the routes).
 *
 * `uploadInParts` starts (or resumes) an upload, sends the parts the server
 * has not stored yet with up to `concurrency` in flight, retries a part that
 * hits a network error or a retryable status with exponential backoff, and
 * completes it. The upload id is saved in a `ResumeStore` keyed by the
 * channel and the file's name, size and modified time, so a reload, a
 * dropped connection or a Retry within the expiry window picks up the parts
 * already stored instead of starting over.
 *
 * The transport is injected: `createApiMultipartTransport` is the real one
 * (api-client, XHR for part progress); tests pass a fake.
 */

export interface MultipartSession {
  upload_id: string;
  part_size: number;
  part_count: number;
  size: number;
  expires_at: string;
}

export interface MultipartStatus extends MultipartSession {
  status: "open" | "completing";
  parts: { part_number: number; size: number }[];
}

export interface UploadedAttachment {
  id: string;
  file_url: string;
  file_name: string;
  file_size: number;
  content_type: string;
}

export interface MultipartTransport {
  create(
    input: { filename: string; size: number; content_type: string },
    signal?: AbortSignal,
  ): Promise<MultipartSession>;
  status(uploadId: string, signal?: AbortSignal): Promise<MultipartStatus>;
  putPart(
    uploadId: string,
    partNumber: number,
    body: Blob,
    options: { signal?: AbortSignal; onProgress?: (loaded: number) => void },
  ): Promise<void>;
  complete(uploadId: string, signal?: AbortSignal): Promise<UploadedAttachment>;
  abort(uploadId: string): Promise<void>;
}

/** What a resume needs to find the upload again. */
export interface SavedUpload {
  uploadId: string;
  size: number;
  expiresAt: string;
}

export interface ResumeStore {
  get(key: string): SavedUpload | null;
  set(key: string, value: SavedUpload): void;
  delete(key: string): void;
}

export interface UploadProgress {
  loaded: number;
  total: number;
}

export interface RetryPolicy {
  /** Tries per request, the first included. A Retry-After wait isn't one. */
  attempts: number;
  baseDelayMs: number;
  /** Cap on the exponential backoff. */
  maxDelayMs: number;
  /** Cap on one server-named Retry-After wait. Default 60 s. */
  maxRetryAfterMs?: number;
  /** Retry-After waits per request before it counts as failed. Default 5. */
  maxRetryAfterWaits?: number;
}

export const DEFAULT_RETRY: RetryPolicy = {
  attempts: 5,
  baseDelayMs: 500,
  maxDelayMs: 10_000,
  maxRetryAfterMs: 60_000,
  maxRetryAfterWaits: 5,
};

const DEFAULT_MAX_RETRY_AFTER_MS = 60_000;
const DEFAULT_MAX_RETRY_AFTER_WAITS = 5;

export const DEFAULT_PART_CONCURRENCY = 3;

export interface UploadInPartsOptions {
  transport: MultipartTransport;
  /** The resume key (`multipartResumeKey`). */
  resumeKey: string;
  store?: ResumeStore;
  signal?: AbortSignal;
  concurrency?: number;
  retry?: RetryPolicy;
  onProgress?: (progress: UploadProgress) => void;
  /** Injected for tests. */
  sleep?: (ms: number, signal?: AbortSignal) => Promise<void>;
  random?: () => number;
  now?: () => number;
}

export interface UploadableFile {
  name: string;
  size: number;
  type: string;
  lastModified?: number;
  slice(start?: number, end?: number): Blob;
}

/** The resume key for a file in a channel. */
export function multipartResumeKey(
  channelId: string,
  file: Pick<UploadableFile, "name" | "size" | "lastModified">,
): string {
  return `${channelId}:${file.size}:${file.lastModified ?? 0}:${file.name}`;
}

const RETRYABLE_STATUSES = new Set([408, 425, 429, 500, 502, 503, 504]);

function statusOf(error: unknown): number | undefined {
  const status = (error as { status?: unknown } | null)?.status;
  return typeof status === "number" ? status : undefined;
}

function codeOf(error: unknown): string | undefined {
  const code = (error as { code?: unknown } | null)?.code;
  return typeof code === "string" ? code : undefined;
}

export function isAbortError(error: unknown): boolean {
  return (error as { name?: unknown } | null)?.name === "AbortError";
}

/**
 * Worth another try: a network failure (no status) or a status that says
 * "later" (timeouts, 429, 5xx). A 4xx that names the request as wrong never
 * is, nor is an abort.
 */
export function isRetryableUploadError(error: unknown): boolean {
  if (isAbortError(error)) return false;
  const status = statusOf(error);
  if (status === undefined || status === 0) return true;
  return RETRYABLE_STATUSES.has(status);
}

/** The upload is gone on the server (expired, aborted elsewhere). */
function isUploadGone(error: unknown): boolean {
  return statusOf(error) === 404 || codeOf(error) === "UPLOAD_NOT_FOUND";
}

/** The wait a server asked for (Retry-After), in ms, or null. */
export function retryAfterOf(error: unknown): number | null {
  const retryAfterMs = (error as { retryAfterMs?: unknown } | null)
    ?.retryAfterMs;
  return typeof retryAfterMs === "number" && retryAfterMs > 0
    ? retryAfterMs
    : null;
}

/**
 * The wait before the next try: a server Retry-After as given (up to
 * `maxRetryAfterMs`), otherwise exponential backoff with equal jitter.
 */
export function backoffDelay(
  attempt: number,
  policy: RetryPolicy,
  random: () => number,
  error?: unknown,
): number {
  const retryAfterMs = retryAfterOf(error);
  if (retryAfterMs !== null) {
    return Math.min(
      policy.maxRetryAfterMs ?? DEFAULT_MAX_RETRY_AFTER_MS,
      retryAfterMs,
    );
  }
  const ceiling = Math.min(
    policy.maxDelayMs,
    policy.baseDelayMs * 2 ** Math.max(0, attempt - 1),
  );
  return Math.round(ceiling / 2 + random() * (ceiling / 2));
}

function abortError(): Error {
  return new DOMException("The upload was cancelled", "AbortError");
}

function defaultSleep(ms: number, signal?: AbortSignal): Promise<void> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(abortError());
      return;
    }
    const timer = setTimeout(() => {
      signal?.removeEventListener("abort", onAbort);
      resolve();
    }, ms);
    const onAbort = () => {
      clearTimeout(timer);
      reject(abortError());
    };
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

/** The byte range [start, end) of part `partNumber` (1-based). */
export function partRange(
  partNumber: number,
  partSize: number,
  size: number,
): [number, number] {
  const start = (partNumber - 1) * partSize;
  return [start, Math.min(size, start + partSize)];
}

/** A localStorage-backed store; every access survives storage being blocked. */
export function localStorageResumeStore(
  storageKey = "yap.chunked-uploads.v1",
  storage: () => Storage | undefined = () =>
    typeof localStorage === "undefined" ? undefined : localStorage,
): ResumeStore {
  const read = (): Record<string, SavedUpload> => {
    try {
      const raw = storage()?.getItem(storageKey);
      const parsed: unknown = raw ? JSON.parse(raw) : {};
      return parsed && typeof parsed === "object"
        ? (parsed as Record<string, SavedUpload>)
        : {};
    } catch {
      return {};
    }
  };
  const write = (value: Record<string, SavedUpload>) => {
    try {
      const now = Date.now();
      // Drop expired entries on every write so the map can't grow forever.
      const live = Object.fromEntries(
        Object.entries(value).filter(
          ([, saved]) => Date.parse(saved.expiresAt) > now,
        ),
      );
      storage()?.setItem(storageKey, JSON.stringify(live));
    } catch {
      // Storage blocked or full: the upload still works, it just can't resume.
    }
  };
  return {
    get(key) {
      return read()[key] ?? null;
    },
    set(key, value) {
      write({ ...read(), [key]: value });
    },
    delete(key) {
      const all = read();
      if (!(key in all)) return;
      delete all[key];
      write(all);
    },
  };
}

export function memoryResumeStore(): ResumeStore {
  const map = new Map<string, SavedUpload>();
  return {
    get: (key) => map.get(key) ?? null,
    set: (key, value) => void map.set(key, value),
    delete: (key) => void map.delete(key),
  };
}

/**
 * Upload `file` in parts and return the attachment the complete route made.
 * Rejects with an AbortError when `signal` fires (the server upload is
 * aborted too), or with the last error once a request is out of retries; in
 * that case the saved upload id stays so the next try resumes.
 */
export async function uploadInParts(
  file: UploadableFile,
  options: UploadInPartsOptions,
): Promise<UploadedAttachment> {
  const {
    transport,
    resumeKey,
    store = memoryResumeStore(),
    signal,
    concurrency = DEFAULT_PART_CONCURRENCY,
    retry = DEFAULT_RETRY,
    onProgress,
    sleep = defaultSleep,
    random = Math.random,
    now = Date.now,
  } = options;

  const maxRetryAfterWaits =
    retry.maxRetryAfterWaits ?? DEFAULT_MAX_RETRY_AFTER_WAITS;
  const withRetry = async <T>(run: () => Promise<T>): Promise<T> => {
    let attempt = 1;
    let retryAfterWaits = 0;
    for (;;) {
      if (signal?.aborted) throw abortError();
      try {
        return await run();
      } catch (error) {
        if (signal?.aborted) throw abortError();
        if (!isRetryableUploadError(error)) throw error;
        // The server said when: wait that long without spending an attempt
        // (bounded, so a server stuck on 429 can't hold the upload forever).
        if (
          retryAfterOf(error) !== null &&
          retryAfterWaits < maxRetryAfterWaits
        ) {
          retryAfterWaits += 1;
          await sleep(backoffDelay(attempt, retry, random, error), signal);
          continue;
        }
        if (attempt >= retry.attempts) throw error;
        await sleep(backoffDelay(attempt, retry, random), signal);
        attempt += 1;
      }
    }
  };

  // ── find or start the upload ──────────────────────────────────────────────
  let session: MultipartSession | null = null;
  const stored = new Set<number>();
  const saved = store.get(resumeKey);
  if (
    saved &&
    saved.size === file.size &&
    Date.parse(saved.expiresAt) > now()
  ) {
    try {
      const status = await withRetry(() =>
        transport.status(saved.uploadId, signal),
      );
      if (status.status === "open" && status.size === file.size) {
        session = status;
        for (const part of status.parts) {
          const [start, end] = partRange(
            part.part_number,
            status.part_size,
            status.size,
          );
          if (part.size === end - start) stored.add(part.part_number);
        }
      }
    } catch (error) {
      if (isAbortError(error)) throw error;
      // Gone or unreadable: start a fresh one below.
    }
    if (!session) store.delete(resumeKey);
  }

  if (!session) {
    session = await withRetry(() =>
      transport.create(
        {
          filename: file.name,
          size: file.size,
          content_type: file.type || "application/octet-stream",
        },
        signal,
      ),
    );
    store.set(resumeKey, {
      uploadId: session.upload_id,
      size: file.size,
      expiresAt: session.expires_at,
    });
  }
  const { upload_id: uploadId, part_size: partSize } = session;

  // ── parts ─────────────────────────────────────────────────────────────────
  const total = file.size;
  let done = 0;
  for (const n of stored) {
    const [start, end] = partRange(n, partSize, total);
    done += end - start;
  }
  const inFlight = new Map<number, number>();
  const report = () => {
    let loaded = done;
    for (const bytes of inFlight.values()) loaded += bytes;
    onProgress?.({ loaded: Math.min(loaded, total), total });
  };
  report();

  const queue: number[] = [];
  for (let n = 1; n <= session.part_count; n += 1) {
    if (!stored.has(n)) queue.push(n);
  }

  // One failure stops every worker: the others finish the part in hand
  // (an abort cancels it) and take no new one.
  const local = new AbortController();
  const onOuterAbort = () => local.abort();
  signal?.addEventListener("abort", onOuterAbort, { once: true });
  let failure: unknown = null;

  const worker = async () => {
    while (queue.length > 0 && failure === null && !local.signal.aborted) {
      const n = queue.shift()!;
      const [start, end] = partRange(n, partSize, total);
      const body = file.slice(start, end);
      try {
        await withRetry(() => {
          inFlight.set(n, 0);
          return transport.putPart(uploadId, n, body, {
            signal: local.signal,
            onProgress: (loaded) => {
              inFlight.set(n, Math.min(loaded, end - start));
              report();
            },
          });
        });
        inFlight.delete(n);
        done += end - start;
        report();
      } catch (error) {
        inFlight.delete(n);
        if (failure === null) failure = error;
        local.abort();
      }
    }
  };

  try {
    await Promise.all(
      Array.from(
        { length: Math.max(1, Math.min(concurrency, queue.length)) },
        worker,
      ),
    );
  } finally {
    signal?.removeEventListener("abort", onOuterAbort);
  }

  if (signal?.aborted) {
    store.delete(resumeKey);
    await transport.abort(uploadId).catch(() => {});
    throw abortError();
  }
  if (failure !== null) {
    if (isUploadGone(failure)) store.delete(resumeKey);
    throw failure;
  }

  // ── complete ──────────────────────────────────────────────────────────────
  try {
    const attachment = await withRetry(() =>
      transport.complete(uploadId, signal),
    );
    store.delete(resumeKey);
    return attachment;
  } catch (error) {
    if (signal?.aborted || isAbortError(error)) {
      store.delete(resumeKey);
      await transport.abort(uploadId).catch(() => {});
      throw abortError();
    }
    if (isUploadGone(error) || statusOf(error) === 413) store.delete(resumeKey);
    throw error;
  }
}
