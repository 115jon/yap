// The public origin to put in absolute proxy URLs. Payloads carry absolute
// URLs because the native clients drop anything that is not http(s).
//
// The Worker entry runs each request inside `withRequestOrigin`, so code
// that builds a payload during a request uses that request's origin. Work
// outside a request (queue consumers, alarms) falls back to the last origin
// this isolate served, then to the deployment default. The proxy signature
// covers only the target URL, so any origin of this deployment serves it.

// Server only. The namespace import keeps any client bundle that reaches it
// building against Vite's empty browser stub of the built-in, where a named
// import fails.
import * as asyncHooks from "node:async_hooks";
import { DEFAULT_PUBLIC_WEB_URL } from "@/lib/site-defaults";

const storage =
  typeof asyncHooks.AsyncLocalStorage === "function"
    ? new asyncHooks.AsyncLocalStorage<string>()
    : null;
let lastSeenOrigin: string | null = null;

export function withRequestOrigin<T>(request: Request, run: () => T): T {
  let origin: string;
  try {
    origin = new URL(request.url).origin;
  } catch {
    return run();
  }
  lastSeenOrigin = origin;
  return storage ? storage.run(origin, run) : run();
}

/**
 * For Durable Objects, which build payloads outside a Worker request: a
 * client's WebSocket upgrade arrives on the public URL, so its origin is
 * remembered. Internal stub calls use made-up hosts (`https://internal`,
 * `https://do`) and are ignored, upgrade or not, so a client never gets a
 * proxy URL on a host it cannot reach.
 */
export function notePublicOrigin(request: Request): void {
  if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return;
  const origin = publicOriginOf(request.url);
  if (origin) lastSeenOrigin = origin;
}

/**
 * `rawUrl`'s origin when it can be a client-facing one: http(s) on a dotted
 * name, an IP literal, or localhost (dev). Single-label names are the
 * made-up hosts of internal calls.
 */
export function publicOriginOf(rawUrl: string): string | null {
  let url: URL;
  try {
    url = new URL(rawUrl);
  } catch {
    return null;
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") return null;
  const hostname = url.hostname.toLowerCase().replace(/\.$/, "");
  if (!hostname) return null;
  const dotted = hostname.includes(".") || hostname.startsWith("[");
  if (!dotted && hostname !== "localhost") return null;
  return url.origin;
}

export function currentPublicOrigin(): string {
  return (
    storage?.getStore() ??
    lastSeenOrigin ??
    DEFAULT_PUBLIC_WEB_URL.replace(/\/$/, "")
  );
}

/** Tests only. */
export function resetPublicOriginForTests(): void {
  lastSeenOrigin = null;
}
