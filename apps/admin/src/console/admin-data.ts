/**
 * The console's data layer: one transport (the real API, or a fixture's
 * mock) behind a context, and a polling resource hook that only refreshes
 * while the page is visible.
 *
 * Every request carries the session through the shared api-client, and
 * every answer comes from /api/*, which 404s for anyone who is not a
 * site admin: nothing on the client decides who may see what.
 */

import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";

export interface AdminTransport {
  get: <T>(path: string) => Promise<T>;
  send: <T>(method: "POST" | "PUT", path: string, body?: unknown) => Promise<T>;
}

/** An error the console shows: the status, the server's code and sentence. */
export class AdminApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
    readonly code?: string,
  ) {
    super(message);
  }
}

async function request<T>(
  method: string,
  path: string,
  body?: unknown,
): Promise<T> {
  const response = await fetch(path, {
    method,
    credentials: "same-origin",
    headers:
      body === undefined ? undefined : { "Content-Type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const json = (await response.json().catch(() => null)) as {
    error?: string;
    code?: string;
  } | null;
  if (!response.ok) {
    // A bare 404 (no JSON) is the gate: the session is gone.
    if (response.status === 404 && json === null) {
      window.location.replace("/");
    }
    throw new AdminApiError(
      json?.error ?? `Request failed (${response.status})`,
      response.status,
      json?.code,
    );
  }
  return json as T;
}

export const liveTransport: AdminTransport = {
  get: (path) => request("GET", path),
  send: (method, path, body) => request(method, path, body ?? {}),
};

export const AdminTransportContext =
  createContext<AdminTransport>(liveTransport);

export function useAdminTransport(): AdminTransport {
  return useContext(AdminTransportContext);
}

/** True while the document is visible (polling pauses when it is not). */
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
 * GET `path`, then every `refreshMs` while the page is visible (and once on
 * becoming visible again). A failed refresh keeps the last data and sets
 * `error`; the next success clears it.
 */
export function useAdminResource<T>(
  path: string | null,
  { refreshMs }: { refreshMs?: number } = {},
): AdminResource<T> {
  const transport = useAdminTransport();
  const visible = usePageVisible();
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [loading, setLoading] = useState(!!path);
  const [updatedAt, setUpdatedAt] = useState<number | null>(null);
  const seq = useRef(0);

  const load = useCallback(async () => {
    if (!path) return;
    const mine = ++seq.current;
    try {
      const next = await transport.get<T>(path);
      if (mine !== seq.current) return;
      setData(next);
      setError(null);
      setUpdatedAt(Date.now());
    } catch (err) {
      if (mine !== seq.current) return;
      setError(err);
    } finally {
      if (mine === seq.current) setLoading(false);
    }
  }, [path, transport]);

  useEffect(() => {
    setLoading(!!path);
    setData(null);
    setError(null);
    void load();
  }, [load, path]);

  useEffect(() => {
    if (!refreshMs || !visible || !path) return;
    // Back from hidden: catch up at once, then on the interval.
    void load();
    const id = window.setInterval(() => void load(), refreshMs);
    return () => window.clearInterval(id);
  }, [refreshMs, visible, path, load]);

  const mutate = useCallback(
    (update: (current: T | null) => T | null) =>
      setData((current) => update(current)),
    [],
  );

  return { data, error, loading, updatedAt, refresh: load, mutate };
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
