// The public origin to put in absolute proxy URLs. Payloads carry absolute
// URLs because the native clients drop anything that is not http(s).
//
// The Worker entry runs each request inside `withRequestOrigin`, so code
// that builds a payload during a request uses that request's origin. Work
// outside a request (queue consumers, alarms) falls back to the last origin
// this isolate served, then to the deployment default. The proxy signature
// covers only the target URL, so any origin of this deployment serves it.

// A namespace import: the desktop build bundles this module (the share
// page's server function) against an empty browser stub, where a named
// import of a Node built-in fails the build. It only runs on the Worker.
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
 * remembered (internal stub calls use made-up hosts and are ignored).
 */
export function notePublicOrigin(request: Request): void {
  if (request.headers.get("Upgrade")?.toLowerCase() !== "websocket") return;
  try {
    lastSeenOrigin = new URL(request.url).origin;
  } catch {
    // Keep the previous origin.
  }
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
