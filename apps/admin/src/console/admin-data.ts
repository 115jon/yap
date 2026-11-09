/**
 * The console's data layer: one transport (the real API, or a fixture's
 * mock) behind a context, and TanStack Query for every read.
 *
 * Every answer comes from /api/*, which 404s for anyone who is not a site
 * admin: nothing on the client decides who may see what. A bare 404 (no
 * JSON) is the gate's answer, but one bare 404 is not proof the session is
 * gone (a Kova hiccup on one request looks the same), so the transport asks
 * GET /api/session once more after a short pause:
 *
 *   session still good   the request is tried once more; a second bare 404
 *                        is that request's own error (the section shows it)
 *   session bare 404s    the session is gone: `onSessionLost` (the boot page
 *                        asks for a sign-in in place, the route kept)
 *
 * Reads retry transient failures (network, 408, 429, 5xx) with backoff
 * through Query; writes retry the same failures in the transport. Polling
 * runs only while the page is visible.
 */

import { QueryClient, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";

export interface AdminTransport {
  get: <T>(path: string) => Promise<T>;
  send: <T>(method: "POST" | "PUT", path: string, body?: unknown) => Promise<T>;
}

/** The code on an error that means the admin session has ended. */
export const SESSION_LOST = "SESSION_LOST";

/** An error the console shows: the status, the server's code and sentence. */
export class AdminApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
    /** Seconds the server asked us to wait (Retry-After), if any. */
    readonly retryAfterS?: number,
  ) {
    super(message);
  }
}

/**
 * Worth trying again: no answer, a timeout, a rate limit or a 5xx. Reads
 * only; writes use isRetryableWrite.
 */
export function isTransientError(error: unknown): boolean {
  if (error instanceof AdminApiError) {
    return (
      error.status === 0 ||
      error.status === 408 ||
      error.status === 429 ||
      error.status >= 500
    );
  }
  return error instanceof TypeError;
}

/**
 * A write is tried again only when the API certainly did not run it: a
 * transient status with no API error code (an edge 429/503, answered before
 * the Worker). A lost response (status 0) may have run already, and a coded
 * answer (a cooldown, a refresh that failed upstream and was logged) is the
 * server's decision; repeating either would repeat its effects and its
 * audit row.
 */
export function isRetryableWrite(error: unknown): boolean {
  return (
    error instanceof AdminApiError &&
    error.status !== 0 &&
    !error.code &&
    isTransientError(error)
  );
}

/** 0.5 s, 1 s, 2 s... capped at 8 s, with a little jitter. */
export function backoffMs(attempt: number): number {
  const base = Math.min(8_000, 500 * 2 ** attempt);
  return base + Math.round(base * 0.2 * Math.random());
}

type Raw = {
  status: number;
  ok: boolean;
  json: { error?: string; code?: string } | null;
  retryAfterS?: number;
};

export interface HttpTransportOptions {
  /** The session is gone (a bare 404, then /api/session also bare 404s). */
  onSessionLost: () => void;
  /** A write succeeded (the console invalidates what it touched). */
  onWrite?: (path: string) => void;
  fetch?: typeof fetch;
  /** Pause before the session re-check. */
  recheckDelayMs?: number;
  /** Write retries for transient failures. */
  writeRetries?: number;
  sleep?: (ms: number) => Promise<void>;
}

const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

/** The real API over fetch, with the session re-check and write retries. */
export function createHttpTransport(
  options: HttpTransportOptions,
): AdminTransport {
  const doFetch = options.fetch ?? ((input, init) => fetch(input, init));
  const sleep = options.sleep ?? wait;
  const recheckDelay = options.recheckDelayMs ?? 400;
  const writeRetries = options.writeRetries ?? 2;
  let recheck: Promise<"ok" | "lost" | "unknown"> | null = null;

  async function once(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<Raw> {
    let response: Response;
    try {
      response = await doFetch(path, {
        method,
        credentials: "same-origin",
        headers:
          body === undefined
            ? undefined
            : { "Content-Type": "application/json" },
        body: body === undefined ? undefined : JSON.stringify(body),
      });
    } catch {
      return { status: 0, ok: false, json: null };
    }
    const json = (await response.json().catch(() => null)) as Raw["json"];
    const retryAfter = Number(response.headers.get("retry-after"));
    return {
      status: response.status,
      ok: response.ok,
      json,
      retryAfterS:
        Number.isFinite(retryAfter) && retryAfter > 0 ? retryAfter : undefined,
    };
  }

  const bare404 = (raw: Raw) => raw.status === 404 && raw.json === null;

  /** One shared re-check for every request that hit a bare 404 at once. */
  function checkSession() {
    recheck ??= (async () => {
      await sleep(recheckDelay);
      const raw = await once("GET", "/api/session");
      if (raw.ok) return "ok" as const;
      if (bare404(raw)) return "lost" as const;
      return "unknown" as const;
    })().finally(() => {
      recheck = null;
    });
    return recheck;
  }

  function toError(raw: Raw): AdminApiError {
    if (raw.status === 0) {
      return new AdminApiError("Couldn't reach the console's server", 0);
    }
    return new AdminApiError(
      raw.json?.error ?? `Request failed (${raw.status})`,
      raw.status,
      raw.json?.code,
      raw.retryAfterS,
    );
  }

  function sessionLost(): AdminApiError {
    // The boot page ignores repeats; every caller still gets the error.
    options.onSessionLost();
    return new AdminApiError("Your session ended", 401, SESSION_LOST);
  }

  async function request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    let raw = await once(method, path, body);
    if (bare404(raw)) {
      const verdict = await checkSession();
      if (verdict === "lost") throw sessionLost();
      if (verdict === "unknown") throw toError({ ...raw, status: 503 });
      raw = await once(method, path, body);
      if (bare404(raw)) throw new AdminApiError("Not found", 404);
    }
    if (!raw.ok) throw toError(raw);
    return raw.json as T;
  }

  return {
    get: (path) => request("GET", path),
    send: async <T>(method: "POST" | "PUT", path: string, body?: unknown) => {
      for (let attempt = 0; ; attempt += 1) {
        try {
          const result = await request<T>(method, path, body ?? {});
          options.onWrite?.(path);
          return result;
        } catch (error) {
          if (attempt >= writeRetries || !isRetryableWrite(error)) throw error;
          const asked =
            error instanceof AdminApiError && error.retryAfterS
              ? error.retryAfterS * 1000
              : 0;
          // A long Retry-After is the caller's to show, not ours to sit out.
          if (asked > 10_000) throw error;
          await sleep(Math.max(asked, backoffMs(attempt)));
        }
      }
    },
  };
}

const unconfigured: AdminTransport = {
  get: () => Promise.reject(new Error("No admin transport")),
  send: () => Promise.reject(new Error("No admin transport")),
};

export const AdminTransportContext =
  createContext<AdminTransport>(unconfigured);

export function useAdminTransport(): AdminTransport {
  return useContext(AdminTransportContext);
}

/** False while the boot page is getting a new session (queries hold). */
export const AdminActiveContext = createContext(true);

/** The query cache key for a GET path. */
export function adminQueryKey(path: string): readonly ["admin", string] {
  return ["admin", path];
}

/**
 * The query client the console runs on: transient failures retry with
 * backoff, a lost session or a real 4xx does not.
 */
export function createAdminQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        staleTime: 15_000,
        gcTime: 10 * 60_000,
        retry: (failures, error) => failures < 3 && isTransientError(error),
        retryDelay: (attempt) => backoffMs(attempt),
        refetchOnWindowFocus: true,
        // Polling stops while the tab is hidden (the default, kept explicit).
        refetchIntervalInBackground: false,
      },
    },
  });
}

/** Invalidate what a successful write changed (and the audit log). */
export function invalidateAfterWrite(client: QueryClient, path: string): void {
  const touched = (key: string) => {
    if (key.startsWith("/api/audit")) return true;
    if (path.startsWith("/api/users/")) {
      const id = path.split("/")[3];
      return key === "/api/users" || key === `/api/users/${id}`;
    }
    if (path.startsWith("/api/settings")) return key === "/api/settings";
    if (path.startsWith("/api/catalog")) return key === "/api/live";
    return false;
  };
  void client.invalidateQueries({
    predicate: (query) =>
      query.queryKey[0] === "admin" &&
      typeof query.queryKey[1] === "string" &&
      touched(query.queryKey[1]),
  });
}

export interface AdminResource<T> {
  data: T | null;
  error: unknown;
  loading: boolean;
  /** When the data last arrived (epoch ms). */
  updatedAt: number | null;
  refresh: () => Promise<void>;
  /** Local edit (an optimistic change); the next fetch replaces it. */
  mutate: (update: (current: T | null) => T | null) => void;
}

/**
 * GET `path` through Query: cached under ["admin", path], polled every
 * `refreshMs` while the page is visible, refetched on focus once stale.
 * A failed refresh keeps the last data and sets `error`.
 */
export function useAdminResource<T>(
  path: string | null,
  { refreshMs, staleMs }: { refreshMs?: number; staleMs?: number } = {},
): AdminResource<T> {
  const transport = useAdminTransport();
  const active = useContext(AdminActiveContext);
  const client = useQueryClient();
  const query = useQuery({
    queryKey: adminQueryKey(path ?? ""),
    queryFn: () => transport.get<T>(path!),
    enabled: !!path && active,
    refetchInterval: active ? refreshMs || false : false,
    staleTime: staleMs ?? (refreshMs ? Math.round(refreshMs / 2) : undefined),
  });
  const { refetch } = query;

  const refresh = useCallback(async () => {
    await refetch();
  }, [refetch]);
  const mutate = useCallback(
    (update: (current: T | null) => T | null) => {
      if (!path) return;
      client.setQueryData<T | null>(adminQueryKey(path), (current) =>
        update(current ?? null),
      );
    },
    [client, path],
  );

  return useMemo(
    () => ({
      data: (query.data as T | undefined) ?? null,
      error: query.error,
      loading: !!path && query.isPending,
      updatedAt: query.dataUpdatedAt || null,
      refresh,
      mutate,
    }),
    [
      query.data,
      query.error,
      query.isPending,
      query.dataUpdatedAt,
      path,
      refresh,
      mutate,
    ],
  );
}

/** True while the document is visible. */
export function usePageVisible(): boolean {
  const [visible, setVisible] = useState(
    () =>
      typeof document === "undefined" || document.visibilityState !== "hidden",
  );
  useEffect(() => {
    const update = () => setVisible(document.visibilityState !== "hidden");
    document.addEventListener("visibilitychange", update);
    return () => document.removeEventListener("visibilitychange", update);
  }, []);
  return visible;
}

/** A 1 Hz clock for "updated 4s ago" readouts; stops while hidden. */
export function useSecondTick(enabled = true): number {
  const visible = usePageVisible();
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!enabled || !visible) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [enabled, visible]);
  return now;
}
