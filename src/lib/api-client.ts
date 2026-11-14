import { announcePendingDeletion } from "@/lib/account-deletion-events";
import { recoverAppSession } from "@/lib/app-session-recovery";
import {
  getStoredKovaAuthSessionToken,
  waitForDesktopToken,
} from "@/lib/desktop-auth";
import { apiUrl } from "@/lib/platform";
import { clog } from "@/lib/console-logger";

const log = clog("api-client");

/**
 * Statuses callers treat as answers, not faults: 401 (signed out, session
 * probe) and 404 (bad invite, missing share). The caller decides what to
 * show; logging them as console errors only adds noise to every page.
 */
const EXPECTED_FAILURE_STATUSES = new Set([401, 404]);

function logHttpFailure(status: number, message: string, ...args: unknown[]) {
  if (EXPECTED_FAILURE_STATUSES.has(status)) log.debug(message, ...args);
  else log.error(message, ...args);
}

function getClientBearerToken(): string | null {
  return getStoredKovaAuthSessionToken();
}

function createAuthRequiredError(): Error {
  const error = new Error("Authentication required");
  (error as any).code = "AUTH_REQUIRED";
  (error as any).status = 401;
  return error;
}

const inFlightGetRequests = new Map<string, Promise<unknown>>();

function createHttpResponseError(
  res: Response,
  json: any,
  fallbackMessage: string,
): Error {
  const message =
    (json &&
      typeof json === "object" &&
      typeof json.error === "string" &&
      json.error) ||
    (json &&
      typeof json === "object" &&
      typeof json.message === "string" &&
      json.message) ||
    fallbackMessage;

  const error = new Error(message);
  (error as any).code =
    json && typeof json === "object" ? json.code : undefined;
  if (res.status === 403) announcePendingDeletion((error as any).code);
  (error as any).status = res.status;
  // The parsed error body, for answers that carry data (a 409's server copy).
  (error as any).body = json && typeof json === "object" ? json : undefined;
  const retryAfterMs = parseRetryAfterMs(res.headers.get("Retry-After"));
  if (retryAfterMs !== null) (error as any).retryAfterMs = retryAfterMs;
  return error;
}

/** Retry-After in ms: delta-seconds or an HTTP date. Null when absent/bad. */
function parseRetryAfterMs(value: string | null): number | null {
  if (!value) return null;
  const seconds = Number(value);
  if (Number.isFinite(seconds)) return seconds > 0 ? seconds * 1000 : null;
  const at = Date.parse(value);
  return Number.isFinite(at) && at > Date.now() ? at - Date.now() : null;
}

/**
 * A part upload with no progress (bytes sent, or the answer arriving) for
 * this long is treated as a network failure, so the engine's retry engages
 * instead of waiting on a dead connection forever.
 */
export const UPLOAD_STALL_TIMEOUT_MS = 60_000;

async function getInitialBearerToken(): Promise<string | null> {
  return getStoredKovaAuthSessionToken() ?? (await waitForDesktopToken(750));
}

/**
 * A 401 for `rejectedToken`: ask Kova for a fresh app token (one mint shared
 * by every refused call, see app-session-recovery). The fresh token to retry
 * with, or null.
 */
async function recoverFrom401(
  rejectedToken: string | null,
): Promise<string | null> {
  const freshToken = await recoverAppSession(rejectedToken);
  if (freshToken && freshToken !== rejectedToken) return freshToken;
  return null;
}

function apiGetDedupeKey(url: string, opts?: ApiOptions) {
  return JSON.stringify({
    url,
    headers: opts?.headers ?? {},
    skipAuth: opts?.skipAuth ?? false,
  });
}

/**
 * Core fetcher that handles our API error convention.
 * If the response contains an `error` key, it throws.
 * Otherwise it returns the parsed JSON directly as T.
 *
 * Accepts an optional AbortSignal for request cancellation.
 * Automatically prefixes relative paths with the API base URL.
 *
 * On a 401 it asks Kova for a fresh app session token from the still-valid
 * auth-domain session and retries the request once.
 */
interface ApiFetchInit extends RequestInit {
  skipAuth?: boolean;
}

async function apiFetch<T>(
  input: RequestInfo | URL,
  init?: ApiFetchInit,
): Promise<T> {
  const requestStartedAt = Date.now();
  const { skipAuth = false, ...fetchInit } = init ?? {};
  // Prefix relative paths with the platform-appropriate base URL
  const resolved =
    typeof input === "string" && input.startsWith("/") ? apiUrl(input) : input;
  const initialToken = skipAuth ? null : await getInitialBearerToken();

  if (!skipAuth && !initialToken) {
    throw createAuthRequiredError();
  }

  /** The bearer the last request carried. */
  let sentToken: string | null = null;
  const doFetch = (token?: string | null) => {
    const authHeaders: Record<string, string> = {};
    const t = skipAuth ? null : (token ?? getClientBearerToken());
    sentToken = t;
    if (t) {
      authHeaders["Authorization"] = `Bearer ${t}`;
    }

    log.info("Request", {
      url: String(resolved),
      method: fetchInit.method ?? "GET",
      hasBearerToken: !!t,
    });

    return fetch(resolved, {
      ...fetchInit,
      headers: {
        "Content-Type": "application/json",
        ...authHeaders,
        ...fetchInit.headers,
      },
    });
  };

  let res: Response;
  try {
    res = await doFetch(initialToken);
  } catch (error) {
    log.error("Network request failed", {
      url: String(resolved),
      method: fetchInit.method ?? "GET",
      error,
    });
    throw error;
  }
  log.info("Response", {
    url: String(resolved),
    status: res.status,
    ok: res.ok,
    contentType: res.headers.get("content-type"),
    durationMs: Date.now() - requestStartedAt,
  });

  // 401 recovery: mint a fresh app token and retry once.
  if (!skipAuth && res.status === 401) {
    log.warn("401 received; attempting session recovery", {
      url: String(resolved),
    });
    const freshToken = await recoverFrom401(sentToken);
    log.info("Session recovery finished", {
      url: String(resolved),
      hasFreshToken: !!freshToken,
    });
    if (freshToken) {
      try {
        res = await doFetch(freshToken);
      } catch (error) {
        log.error("Retry network request failed", {
          url: String(resolved),
          method: fetchInit.method ?? "GET",
          error,
        });
        throw error;
      }
      log.info("Retry response", {
        url: String(resolved),
        status: res.status,
        ok: res.ok,
        contentType: res.headers.get("content-type"),
        durationMs: Date.now() - requestStartedAt,
      });
    }
  }

  // Handle empty responses (204 No Content, etc.) without attempting JSON parse
  const contentLength = res.headers.get("content-length");
  if (res.status === 204 || contentLength === "0") {
    if (!res.ok) {
      throw new Error(`HTTP Error ${res.status}: ${res.statusText}`);
    }
    return undefined as T;
  }

  let json: any;
  try {
    json = await res.json();
  } catch (err) {
    if (!res.ok) {
      logHttpFailure(
        res.status,
        `HTTP Error ${res.status}: ${res.statusText} on ${resolved}`,
      );
      throw new Error(`HTTP Error ${res.status}: ${res.statusText}`);
    }
    log.error(`Parse error on ${resolved}`, err);
    throw new Error("Failed to parse API response");
  }

  if (!res.ok) {
    logHttpFailure(
      res.status,
      `Failed ${resolved} with status ${res.status}:`,
      json,
    );
    throw createHttpResponseError(
      res,
      json,
      `HTTP Error ${res.status}: ${res.statusText}`,
    );
  }

  if (
    json &&
    typeof json === "object" &&
    "error" in json &&
    typeof json.error === "string"
  ) {
    const error = new Error(json.error || "Unknown API Error");
    (error as any).code = json.code;
    (error as any).status = res.status;
    throw error;
  }

  log.info("Response body shape", {
    url: String(resolved),
    kind: Array.isArray(json) ? "array" : typeof json,
    count: Array.isArray(json) ? json.length : undefined,
    keys:
      json && typeof json === "object" && !Array.isArray(json)
        ? Object.keys(json).slice(0, 12)
        : undefined,
  });

  return json as T;
}

/** Options shared by all typed helpers */
interface ApiOptions {
  signal?: AbortSignal;
  headers?: Record<string, string>;
  skipAuth?: boolean;
}

/**
 * GET request helper.
 */
export async function apiGet<T>(url: string, opts?: ApiOptions): Promise<T> {
  if (opts?.signal) {
    return apiFetch<T>(url, {
      method: "GET",
      signal: opts.signal,
      headers: opts.headers,
      skipAuth: opts.skipAuth,
    });
  }

  const key = apiGetDedupeKey(url, opts);
  const existing = inFlightGetRequests.get(key);
  if (existing) return existing as Promise<T>;

  const request = apiFetch<T>(url, {
    method: "GET",
    headers: opts?.headers,
    skipAuth: opts?.skipAuth,
  }).finally(() => {
    inFlightGetRequests.delete(key);
  });
  inFlightGetRequests.set(key, request);
  return request;
}

/**
 * POST request helper (JSON body).
 */
export async function apiPost<T, B = unknown>(
  url: string,
  body: B,
  opts?: ApiOptions,
): Promise<T> {
  return apiFetch<T>(url, {
    method: "POST",
    body: JSON.stringify(body),
    signal: opts?.signal,
    headers: opts?.headers,
    skipAuth: opts?.skipAuth,
  });
}

/**
 * PUT request helper (JSON body).
 */
export async function apiPut<T, B = unknown>(
  url: string,
  body: B,
  opts?: ApiOptions,
): Promise<T> {
  return apiFetch<T>(url, {
    method: "PUT",
    body: JSON.stringify(body),
    signal: opts?.signal,
    headers: opts?.headers,
    skipAuth: opts?.skipAuth,
  });
}

/**
 * PATCH request helper (JSON body).
 */
export async function apiPatch<T, B = unknown>(
  url: string,
  body: B,
  opts?: ApiOptions,
): Promise<T> {
  return apiFetch<T>(url, {
    method: "PATCH",
    body: JSON.stringify(body),
    signal: opts?.signal,
    headers: opts?.headers,
    skipAuth: opts?.skipAuth,
  });
}

/**
 * DELETE request helper (optional JSON body).
 */
export async function apiDelete<T, B = unknown>(
  url: string,
  body?: B,
  opts?: ApiOptions,
): Promise<T> {
  return apiFetch<T>(url, {
    method: "DELETE",
    ...(body ? { body: JSON.stringify(body) } : {}),
    signal: opts?.signal,
    headers: opts?.headers,
    skipAuth: opts?.skipAuth,
  });
}

/**
 * Upload helper for FormData (multipart/form-data).
 * Does NOT set Content-Type header — the browser sets it automatically
 * with the correct boundary.
 */
export async function apiUpload<T>(
  url: string,
  formData: FormData,
  opts?: ApiOptions,
): Promise<T> {
  const resolved = url.startsWith("/") ? apiUrl(url) : url;
  const initialToken = await getInitialBearerToken();

  if (!initialToken) {
    throw createAuthRequiredError();
  }

  let sentToken: string | null = null;
  const doFetch = (token?: string | null) => {
    const headers: Record<string, string> = { ...(opts?.headers ?? {}) };
    const t = token ?? getClientBearerToken();
    sentToken = t;
    if (t) {
      headers["Authorization"] = `Bearer ${t}`;
    }
    return fetch(resolved, {
      method: "POST",
      body: formData,
      signal: opts?.signal,
      headers,
    });
  };

  let res = await doFetch(initialToken);

  // 401 recovery: mint a fresh app token and retry once.
  if (res.status === 401) {
    const freshToken = await recoverFrom401(sentToken);
    if (freshToken) res = await doFetch(freshToken);
  }

  let json: any;
  try {
    json = await res.json();
  } catch {
    if (!res.ok) {
      throw new Error(`Upload failed: HTTP ${res.status}`);
    }
    throw new Error("Failed to parse upload response");
  }

  if (!res.ok) {
    throw createHttpResponseError(
      res,
      json,
      `Upload failed: HTTP ${res.status}`,
    );
  }

  if (
    json &&
    typeof json === "object" &&
    "error" in json &&
    typeof json.error === "string"
  ) {
    const error = new Error(json.error || "Upload failed");
    (error as any).code = json.code;
    (error as any).status = res.status;
    throw error;
  }

  return json as T;
}

/** One XHR attempt: the status and parsed JSON body (null when none). */
function sendBlobOnce(
  url: string,
  method: string,
  body: Blob,
  headers: Record<string, string>,
  opts: {
    signal?: AbortSignal;
    onProgress?: (loaded: number) => void;
    stallTimeoutMs?: number;
  },
): Promise<{ status: number; json: any; retryAfter: string | null }> {
  return new Promise((resolve, reject) => {
    if (opts.signal?.aborted) {
      reject(new DOMException("The upload was cancelled", "AbortError"));
      return;
    }
    const xhr = new XMLHttpRequest();
    xhr.open(method, url);
    for (const [name, value] of Object.entries(headers)) {
      xhr.setRequestHeader(name, value);
    }
    xhr.responseType = "text";
    // Inactivity, not total time: a slow link that keeps moving is fine.
    let stalled = false;
    let stallTimer: ReturnType<typeof setTimeout> | undefined;
    const armStallTimer = () => {
      clearTimeout(stallTimer);
      stallTimer = setTimeout(() => {
        stalled = true;
        xhr.abort();
      }, opts.stallTimeoutMs ?? UPLOAD_STALL_TIMEOUT_MS);
    };
    const onAbort = () => xhr.abort();
    opts.signal?.addEventListener("abort", onAbort, { once: true });
    const done = () => {
      clearTimeout(stallTimer);
      opts.signal?.removeEventListener("abort", onAbort);
    };
    xhr.upload.onprogress = (event) => {
      armStallTimer();
      opts.onProgress?.(event.loaded);
    };
    // Sent in full: the timer now waits for the answer to start arriving.
    xhr.upload.onload = armStallTimer;
    xhr.onprogress = armStallTimer;
    xhr.onload = () => {
      done();
      let json: any = null;
      try {
        json = xhr.responseText ? JSON.parse(xhr.responseText) : null;
      } catch {
        json = null;
      }
      resolve({
        status: xhr.status,
        json,
        retryAfter: xhr.getResponseHeader("Retry-After"),
      });
    };
    xhr.onerror = () => {
      done();
      // No status: the upload engine reads this as a network error and retries.
      reject(new TypeError("Network request failed"));
    };
    xhr.ontimeout = xhr.onerror;
    xhr.onabort = () => {
      done();
      reject(
        stalled
          ? // No status: retried as a network failure, not a cancel.
            new TypeError("Upload stalled")
          : new DOMException("The upload was cancelled", "AbortError"),
      );
    };
    armStallTimer();
    xhr.send(body);
  });
}

/**
 * Send a raw Blob (one chunked-upload part) with upload progress, which
 * `fetch` can't report. Same auth and 401 recovery as the JSON helpers.
 * Errors carry `status`, `code` and, on a 429/503, `retryAfterMs`.
 */
export async function apiSendBlob<T>(
  url: string,
  body: Blob,
  opts: {
    method?: string;
    signal?: AbortSignal;
    onProgress?: (loaded: number) => void;
    /** Default UPLOAD_STALL_TIMEOUT_MS. */
    stallTimeoutMs?: number;
  } = {},
): Promise<T> {
  const resolved = url.startsWith("/") ? apiUrl(url) : url;
  const initialToken = await getInitialBearerToken();
  if (!initialToken) throw createAuthRequiredError();

  let sentToken: string | null = null;
  const attempt = (token?: string | null) => {
    const headers: Record<string, string> = {
      "Content-Type": "application/octet-stream",
    };
    const t = token ?? getClientBearerToken();
    sentToken = t;
    if (t) headers["Authorization"] = `Bearer ${t}`;
    return sendBlobOnce(resolved, opts.method ?? "PUT", body, headers, opts);
  };

  let res = await attempt(initialToken);
  if (res.status === 401) {
    const freshToken = await recoverFrom401(sentToken);
    if (freshToken) res = await attempt(freshToken);
  }

  if (res.status < 200 || res.status >= 300) {
    const message =
      (res.json && typeof res.json.error === "string" && res.json.error) ||
      `Upload failed: HTTP ${res.status}`;
    const error = new Error(message);
    (error as any).status = res.status;
    (error as any).code = res.json?.code;
    const retryAfterMs = parseRetryAfterMs(res.retryAfter);
    if (retryAfterMs !== null) (error as any).retryAfterMs = retryAfterMs;
    throw error;
  }
  return res.json as T;
}
