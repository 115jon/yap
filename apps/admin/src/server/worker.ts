/**
 * The admin Worker (yap-admin). It serves the console and its API, and
 * nothing else, behind one gate (./gate.ts):
 *
 *   signed out, or not an admin    GET /  the neutral sign-in page and the
 *                                  files only it needs (from the build's
 *                                  manifest); POST /api/session trades a
 *                                  Kova app token for the session cookie.
 *                                  Anything else: a bare 404.
 *   a site admin's session         the console page, its files and /api/*
 *
 * Every refusal is the same bare 404 (`notFound()`): a missing session, a
 * bad or expired token, a Kova outage, a signed-in non-admin and an unknown
 * path all look alike. Assets run through this Worker first
 * (`run_worker_first`), so the console's code is not downloadable either.
 *
 * Data: the main Worker's D1 (and its KV/R2, bound for parity), and its
 * Durable Objects through cross-script bindings for live state and
 * disconnects (./admin-gateway.service.ts). The switches, the banner and
 * suspensions this Worker writes are read and enforced by the main Worker.
 * "Refresh now" on the shop catalog runs Yap's own catalog sync here
 * (@/lib/collectibles-catalog against the shared D1); the main Worker
 * offers no way to force one.
 */

import { CacheKey } from "@/lib/cache";
import { DEFAULT_KOVA_AUTH_URL } from "@/lib/site-defaults";
import { createAdminGateway } from "./admin-gateway.service";
import { createAdminHandlers, type AdminHandlers } from "./admin.service";
import {
  bearerToken,
  clearedSessionCookie,
  notFound,
  sessionCookie,
  sessionToken,
  verifyAdminToken,
  type GateDeps,
  type GateEnv,
} from "./gate";

export interface AdminEnv extends GateEnv {
  CACHE?: KVNamespace;
  BUCKET?: R2Bucket;
  MEETING_ROOM: DurableObjectNamespace;
  VOICE_ROOM: DurableObjectNamespace;
  ASSETS: Fetcher;
  /** The main Worker, by service binding: its /api/healthz build stamp. */
  YAP?: Fetcher;
}

const SIGN_IN_PAGE = "/index.html";
const CONSOLE_PAGE = "/console.html";
const MANIFEST = "/manifest.json";
const KEY_PLACEHOLDER = "__KOVA_ADMIN_PUBLISHABLE_KEY__";
const URL_PLACEHOLDER = "__KOVA_AUTH_URL__";

type Manifest = Record<
  string,
  { file: string; css?: string[]; assets?: string[]; imports?: string[] }
>;

let publicFiles: Promise<Set<string>> | null = null;

/** The sign-in entry's files and everything they import, from the manifest. */
async function signInFiles(
  env: AdminEnv,
  origin: string,
): Promise<Set<string>> {
  publicFiles ??= (async () => {
    const response = await env.ASSETS.fetch(
      new Request(`${origin}${MANIFEST}`),
    );
    const files = new Set<string>([SIGN_IN_PAGE, "/favicon.ico"]);
    if (!response.ok) return files;
    const manifest = (await response.json()) as Manifest;
    const visit = (key: string) => {
      const chunk = manifest[key];
      if (!chunk || files.has(`/${chunk.file}`)) return;
      files.add(`/${chunk.file}`);
      for (const file of [...(chunk.css ?? []), ...(chunk.assets ?? [])]) {
        files.add(`/${file}`);
      }
      for (const next of chunk.imports ?? []) visit(next);
    };
    visit("index.html");
    return files;
  })().catch(() => {
    publicFiles = null;
    return new Set([SIGN_IN_PAGE]);
  });
  return publicFiles;
}

/** Forget the manifest (tests). */
export function resetAdminWorkerCache(): void {
  publicFiles = null;
}

/** An HTML page with the Kova settings filled in, never cached. */
async function page(
  env: AdminEnv,
  request: Request,
  path: string,
): Promise<Response> {
  const asset = await env.ASSETS.fetch(new Request(new URL(path, request.url)));
  if (!asset.ok) return notFound();
  const html = (await asset.text())
    .replaceAll(
      KEY_PLACEHOLDER,
      escapeAttribute(env.KOVA_ADMIN_PUBLISHABLE_KEY ?? ""),
    )
    .replaceAll(
      URL_PLACEHOLDER,
      escapeAttribute(env.KOVA_AUTH_URL || DEFAULT_KOVA_AUTH_URL),
    );
  return new Response(request.method === "HEAD" ? null : html, {
    status: 200,
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "no-store",
      "Referrer-Policy": "no-referrer",
      "X-Frame-Options": "DENY",
      "X-Content-Type-Options": "nosniff",
      "X-Robots-Tag": "noindex, nofollow",
    },
  });
}

function escapeAttribute(value: string): string {
  return value.replace(/[&"<>]/g, (char) => `&#${char.charCodeAt(0)};`);
}

async function asset(env: AdminEnv, request: Request): Promise<Response> {
  const response = await env.ASSETS.fetch(request);
  return response.ok || response.status === 304 ? response : notFound();
}

/** A same-origin check for writes: the cookie is SameSite=Strict too. */
function sameOrigin(request: Request): boolean {
  const origin = request.headers.get("origin");
  return origin !== null && origin === new URL(request.url).origin;
}

/** What the main Worker runs, from its own /api/healthz. */
async function mainWorkerBuild(env: AdminEnv) {
  if (!env.YAP) return null;
  const response = await env.YAP.fetch("https://yap.internal/api/healthz");
  if (!response.ok) return null;
  const body = (await response.json()) as {
    version?: string;
    commit?: string;
    builtAt?: string;
  };
  return {
    version: body.version ?? "unknown",
    commit: body.commit ?? "unknown",
    built_at: body.builtAt ?? null,
  };
}

type Route = { action: keyof AdminHandlers; userId?: string };

/** /api/* paths to handlers. Unknown paths and methods are null. */
export function routeApi(method: string, pathname: string): Route | null {
  const user =
    /^\/api\/users\/([^/]+)(?:\/(plan|badges|suspension|disconnect))?$/.exec(
      pathname,
    );
  if (user) {
    const userId = decodeURIComponent(user[1]!);
    const sub = user[2];
    if (!sub) return method === "GET" ? { action: "person", userId } : null;
    if (method !== "POST") return null;
    const action = (
      {
        plan: "plan",
        badges: "badge",
        suspension: "suspension",
        disconnect: "disconnect",
      } as const
    )[sub as "plan" | "badges" | "suspension" | "disconnect"];
    return { action, userId };
  }
  const table: Record<string, Partial<Record<string, keyof AdminHandlers>>> = {
    "/api/session": { GET: "session" },
    "/api/users": { GET: "people" },
    "/api/live": { GET: "live" },
    "/api/insights": { GET: "insights" },
    "/api/settings": { GET: "settings", PUT: "putSettings" },
    "/api/catalog/refresh": { POST: "refreshCatalog" },
    "/api/audit": { GET: "audit" },
  };
  const action = table[pathname]?.[method];
  return action ? { action } : null;
}

export async function handleAdminRequest(
  request: Request,
  env: AdminEnv,
  deps: GateDeps = {},
): Promise<Response> {
  const url = new URL(request.url);
  const { pathname } = url;
  const method = request.method;

  // The sign-in page's one call: a Kova token in, the cookie out.
  if (pathname === "/api/session" && method === "POST") {
    if (!sameOrigin(request)) return notFound();
    const token = bearerToken(request);
    const adminId = await verifyAdminToken(token, env, deps);
    if (!adminId || !token) return notFound();
    return new Response(null, {
      status: 204,
      headers: {
        "Set-Cookie": sessionCookie(token),
        "Cache-Control": "no-store",
      },
    });
  }

  const adminId = await verifyAdminToken(sessionToken(request), env, deps);
  const read = method === "GET" || method === "HEAD";

  if (!adminId) {
    if (!read) return notFound();
    // The Kova redirect comes back to "/" with its code in the query.
    if (pathname === "/") return page(env, request, SIGN_IN_PAGE);
    const files = await signInFiles(env, url.origin);
    if (files.has(pathname) && pathname !== SIGN_IN_PAGE) {
      return asset(env, request);
    }
    return notFound();
  }

  if (pathname === "/api/sign-out" && method === "POST") {
    if (!sameOrigin(request)) return notFound();
    return new Response(null, {
      status: 204,
      headers: {
        "Set-Cookie": clearedSessionCookie(),
        "Cache-Control": "no-store",
      },
    });
  }

  if (pathname.startsWith("/api/")) {
    if (!read && !sameOrigin(request)) return notFound();
    const route = routeApi(method === "HEAD" ? "GET" : method, pathname);
    if (!route) return notFound();
    const handlers = createAdminHandlers({
      db: env.DB,
      gateway: createAdminGateway({
        db: env.DB,
        meetingRooms: env.MEETING_ROOM,
        voiceRooms: env.VOICE_ROOM,
      }),
      // The main Worker caches profiles (plan, badges) in the shared KV.
      afterUserChange: async (userId) => {
        await env.CACHE?.delete(CacheKey.userProfile(userId)).catch(() => {});
      },
      deploy: () => mainWorkerBuild(env),
    });
    return handlers[route.action](request, adminId, { userId: route.userId });
  }

  if (!read) return notFound();
  // The console is one page; its sections live in the query string.
  if (pathname === "/" || pathname === CONSOLE_PAGE) {
    return page(env, request, CONSOLE_PAGE);
  }
  if (pathname === MANIFEST) return notFound();
  return asset(env, request);
}

export default {
  fetch(request: Request, env: AdminEnv): Promise<Response> {
    return handleAdminRequest(request, env);
  },
} satisfies ExportedHandler<AdminEnv>;
