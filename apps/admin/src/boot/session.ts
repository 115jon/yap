/**
 * The boot page's calls: the session cookie (GET/POST /api/session, POST
 * /api/sign-out) and loading the gated console bundle. Every refusal from
 * the Worker is a bare 404, so a 404 here means "not an admin session" and
 * nothing more.
 */

import type {
  AdminSessionInfo,
  ConsoleModule,
} from "../shared/console-contract";

export type CookieCheck =
  | { kind: "admin"; session: AdminSessionInfo }
  | { kind: "none" }
  | { kind: "unreachable" };

/** Is there an admin session cookie? */
export async function checkCookie(fetcher = fetch): Promise<CookieCheck> {
  const response = await fetcher("/api/session", {
    credentials: "same-origin",
    cache: "no-store",
  }).catch(() => null);
  if (!response) return { kind: "unreachable" };
  if (response.status === 404) return { kind: "none" };
  if (!response.ok) return { kind: "unreachable" };
  const session = (await response
    .json()
    .catch(() => null)) as AdminSessionInfo | null;
  return session?.user_id
    ? { kind: "admin", session }
    : { kind: "unreachable" };
}

export type Exchange = "granted" | "refused" | "unreachable";

/** Trade a Kova app token for the cookie: 204 is yes, a bare 404 is no. */
export async function exchangeToken(
  token: string,
  fetcher = fetch,
): Promise<Exchange> {
  const response = await fetcher("/api/session", {
    method: "POST",
    credentials: "same-origin",
    headers: { Authorization: `Bearer ${token}` },
  }).catch(() => null);
  if (!response) return "unreachable";
  if (response.status === 204) return "granted";
  if (response.status === 404) return "refused";
  return "unreachable";
}

export async function clearCookie(fetcher = fetch): Promise<void> {
  await fetcher("/api/sign-out", {
    method: "POST",
    credentials: "same-origin",
  }).catch(() => null);
}

/** A stylesheet in <head>, resolved once it has loaded (or failed). */
function loadStyle(href: string): Promise<void> {
  const existing = document.querySelector<HTMLLinkElement>(
    `link[data-console-style="${CSS.escape(href)}"]`,
  );
  if (existing) return Promise.resolve();
  return new Promise((resolve) => {
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = href;
    link.dataset.consoleStyle = href;
    link.onload = () => resolve();
    link.onerror = () => resolve();
    document.head.appendChild(link);
  });
}

let consoleModule: Promise<ConsoleModule> | null = null;

/**
 * The console bundle: its stylesheets first (so it never paints unstyled),
 * then its entry module. The paths come from an admin's GET /api/session;
 * the Worker serves those files to an admin session only. `vite dev` serves
 * the source instead.
 */
export function loadConsole(session: AdminSessionInfo): Promise<ConsoleModule> {
  consoleModule ??= (async () => {
    if (import.meta.env.DEV) {
      const source = "/src/console/app.tsx";
      return (await import(/* @vite-ignore */ source)) as ConsoleModule;
    }
    const files = session.console;
    if (!files) throw new Error("The session did not name the console files");
    await Promise.all(files.styles.map(loadStyle));
    return (await import(/* @vite-ignore */ files.script)) as ConsoleModule;
  })().catch((error: unknown) => {
    consoleModule = null;
    throw error;
  });
  return consoleModule;
}

const RETURN_KEY = "yap-admin:return";

/** Remember where the admin was, across a hosted (redirect) sign-in. */
export function rememberReturnPath(path: string): void {
  try {
    sessionStorage.setItem(RETURN_KEY, path);
  } catch {
    // Storage off: the console opens on People instead.
  }
}

export function takeReturnPath(): string | null {
  try {
    const path = sessionStorage.getItem(RETURN_KEY);
    sessionStorage.removeItem(RETURN_KEY);
    return path;
  } catch {
    return null;
  }
}
