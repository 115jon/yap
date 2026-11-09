/**
 * deadlock-api.com, server side only. One client per job run: it sends
 * Yap's User-Agent (no key: Yap uses the free tier), keeps at
 * most two requests in flight, spends one token from the global budget
 * per request, and classifies every answer so the fetcher never guesses:
 *
 *   ok            the body, parsed (size-capped)
 *   protected     403: the account asked the API to delete its data
 *   rate_limited  429/503, or the budget is spent; `retryAfterMs` set
 *   timeout       504 or our own timeout; backs off 10 minutes by default
 *   unavailable   anything else (5xx, network, bad JSON)
 *   not_found     404
 *
 * The match history's 429 carries the stored history as its body, so it
 * counts as ok there (`acceptStatuses`).
 */

export const DEADLOCK_API_BASE = "https://api.deadlock-api.com";

/** Hosts mirrored art may come from. */
export const DEADLOCK_ASSET_HOSTS = new Set([
  "assets-bucket.deadlock-api.com",
  "api.deadlock-api.com",
]);

const DEFAULT_TIMEOUT_MS = 15_000;
const MAX_BODY_BYTES = 6 * 1024 * 1024;
const DEFAULT_RETRY_MS = 60_000;
const TIMEOUT_RETRY_MS = 10 * 60_000;
const MAX_RETRY_MS = 6 * 60 * 60_000;
const MAX_IN_FLIGHT = 2;

export type ApiFailureKind =
  | "protected"
  | "rate_limited"
  | "timeout"
  | "unavailable"
  | "not_found";

export type ApiResult<T> =
  | { ok: true; status: number; body: T }
  | {
      ok: false;
      kind: ApiFailureKind;
      status: number;
      /** How long to leave the API alone, when it said or when we decide. */
      retryAfterMs: number | null;
    };

export interface DeadlockApiDeps {
  fetch: typeof fetch;
  /** Takes one request from the global budget; false when it is spent. */
  budget?: () => Promise<boolean>;
  userAgent?: string;
  base?: string;
  timeoutMs?: number;
  now?: () => number;
}

export interface DeadlockApi {
  get<T>(
    path: string,
    params?: Record<string, string | number | undefined>,
    options?: { acceptStatuses?: number[]; maxBytes?: number },
  ): Promise<ApiResult<T>>;
  /** Requests sent by this client so far (tests, logs). */
  readonly sent: number;
}

/**
 * Retry-After as milliseconds: delta-seconds or an HTTP date. Null when
 * absent or unreadable; clamped to six hours.
 */
export function parseRetryAfter(
  value: string | null,
  now: number = Date.now(),
): number | null {
  if (!value) return null;
  const trimmed = value.trim();
  let ms: number;
  if (/^\d+$/.test(trimmed)) {
    ms = Number(trimmed) * 1000;
  } else {
    const at = Date.parse(trimmed);
    if (!Number.isFinite(at)) return null;
    ms = at - now;
  }
  return Math.min(MAX_RETRY_MS, Math.max(0, ms));
}

async function readCapped(
  body: ReadableStream<Uint8Array> | null,
  limit: number,
): Promise<string | null> {
  if (!body) return "";
  const reader = body.getReader();
  const decoder = new TextDecoder();
  let total = 0;
  let text = "";
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      return null;
    }
    text += decoder.decode(value, { stream: true });
  }
  return text + decoder.decode();
}

export function createDeadlockApi(deps: DeadlockApiDeps): DeadlockApi {
  const base = deps.base ?? DEADLOCK_API_BASE;
  const now = deps.now ?? Date.now;
  let inFlight = 0;
  const waiting: Array<() => void> = [];
  let sent = 0;

  const acquire = async () => {
    if (inFlight < MAX_IN_FLIGHT) {
      inFlight += 1;
      return;
    }
    await new Promise<void>((resolve) => waiting.push(resolve));
    inFlight += 1;
  };
  const release = () => {
    inFlight -= 1;
    waiting.shift()?.();
  };

  async function get<T>(
    path: string,
    params: Record<string, string | number | undefined> = {},
    options: { acceptStatuses?: number[]; maxBytes?: number } = {},
  ): Promise<ApiResult<T>> {
    if (deps.budget && !(await deps.budget())) {
      return {
        ok: false,
        kind: "rate_limited",
        status: 0,
        retryAfterMs: DEFAULT_RETRY_MS,
      };
    }
    const url = new URL(path, base);
    for (const [key, value] of Object.entries(params)) {
      if (value !== undefined) url.searchParams.set(key, String(value));
    }
    const headers: Record<string, string> = {
      Accept: "application/json",
      "User-Agent": deps.userAgent ?? "Yap (+https://yap.115jon.com)",
    };

    await acquire();
    sent += 1;
    let response: Response;
    try {
      response = await deps.fetch(url.toString(), {
        method: "GET",
        headers,
        redirect: "manual",
        signal: AbortSignal.timeout(deps.timeoutMs ?? DEFAULT_TIMEOUT_MS),
      });
    } catch (error) {
      release();
      const timedOut =
        error instanceof Error &&
        (error.name === "TimeoutError" || error.name === "AbortError");
      return {
        ok: false,
        kind: timedOut ? "timeout" : "unavailable",
        status: 0,
        retryAfterMs: timedOut ? TIMEOUT_RETRY_MS : null,
      };
    }

    try {
      const retryAfter = parseRetryAfter(
        response.headers.get("retry-after"),
        now(),
      );
      const accepted =
        response.status === 200 ||
        (options.acceptStatuses ?? []).includes(response.status);
      if (!accepted) {
        await response.body?.cancel();
        if (response.status === 403) {
          return {
            ok: false,
            kind: "protected",
            status: 403,
            retryAfterMs: null,
          };
        }
        if (response.status === 404) {
          return {
            ok: false,
            kind: "not_found",
            status: 404,
            retryAfterMs: null,
          };
        }
        if (response.status === 429 || response.status === 503) {
          return {
            ok: false,
            kind: "rate_limited",
            status: response.status,
            retryAfterMs: retryAfter ?? DEFAULT_RETRY_MS,
          };
        }
        if (response.status === 504) {
          return {
            ok: false,
            kind: "timeout",
            status: 504,
            retryAfterMs: retryAfter ?? TIMEOUT_RETRY_MS,
          };
        }
        return {
          ok: false,
          kind: "unavailable",
          status: response.status,
          retryAfterMs: retryAfter,
        };
      }
      const text = await readCapped(
        response.body,
        options.maxBytes ?? MAX_BODY_BYTES,
      );
      if (text === null) {
        return {
          ok: false,
          kind: "unavailable",
          status: response.status,
          retryAfterMs: null,
        };
      }
      try {
        return {
          ok: true,
          status: response.status,
          body: JSON.parse(text) as T,
        };
      } catch {
        return {
          ok: false,
          kind: "unavailable",
          status: response.status,
          retryAfterMs: null,
        };
      }
    } finally {
      release();
    }
  }

  return {
    get,
    get sent() {
      return sent;
    },
  };
}
