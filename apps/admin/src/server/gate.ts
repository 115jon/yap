/**
 * The admin Worker's gate. One rule: a request is let in only when it
 * carries a Kova session of the "Yap Admin" Kova app whose user is a Yap
 * account named in SITE_ADMIN_USER_IDS (and not suspended). Everything else
 * gets `notFound()`, the same bare 404 for every refusal.
 *
 * The session is the Kova app token. The sign-in page trades it once for an
 * HttpOnly cookie (POST /api/session with `Authorization: Bearer`); every
 * later request (pages, assets, API) carries the cookie and is checked again
 * here, through a short per-isolate cache, so a revoked Kova session or a
 * removed id stops working within VERIFY_TTL_MS.
 *
 * Kova user ids are Yap user ids: the main app makes the `users` row with
 * the Kova id as its primary key (users/me, auth/sync), and Kova users are
 * the same across Kova apps; only sessions are app-scoped.
 */

import { Effect } from "effect";

import { DEFAULT_KOVA_AUTH_URL } from "@/lib/site-defaults";
import { KovaAuth } from "@/server/kova-auth";

export const SESSION_COOKIE = "__Host-yap-admin";
/** How long the cookie lives. The Kova session behind it is checked anyway. */
export const SESSION_MAX_AGE_S = 12 * 60 * 60;
export const VERIFY_TTL_MS = 30_000;

export interface GateEnv {
  DB: D1Database;
  /** Secret: the site admins, comma-separated Yap user ids. Unset: nobody. */
  SITE_ADMIN_USER_IDS?: string;
  /** Var: the "Yap Admin" Kova app's publishable key. Unset: nobody. */
  KOVA_ADMIN_PUBLISHABLE_KEY?: string;
  /** Var: Kova's base URL; defaults to DEFAULT_KOVA_AUTH_URL. */
  KOVA_AUTH_URL?: string;
}

/** The answer to every refusal: no body, no hint. */
export function notFound(): Response {
  return new Response(null, { status: 404 });
}

export function siteAdminIds(raw: unknown): Set<string> {
  if (typeof raw !== "string") return new Set();
  return new Set(
    raw
      .split(/[\s,]+/)
      .map((id) => id.trim())
      .filter(Boolean),
  );
}

/** The session token in the request's admin cookie, if any. */
export function sessionToken(request: Request): string | null {
  const header = request.headers.get("cookie");
  if (!header) return null;
  for (const part of header.split(";")) {
    const [name, ...rest] = part.trim().split("=");
    if (name === SESSION_COOKIE) {
      const value = rest.join("=");
      return value ? decodeURIComponent(value) : null;
    }
  }
  return null;
}

/** The bearer token on a request (the sign-in page's exchange). */
export function bearerToken(request: Request): string | null {
  const header = request.headers.get("authorization");
  return header?.startsWith("Bearer ") ? header.slice(7).trim() || null : null;
}

export function sessionCookie(token: string): string {
  return `${SESSION_COOKIE}=${encodeURIComponent(token)}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=${SESSION_MAX_AGE_S}`;
}

export function clearedSessionCookie(): string {
  return `${SESSION_COOKIE}=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0`;
}

export interface GateDeps {
  /** Kova is asked through this (tests hand in a fake). */
  fetch?: typeof fetch;
  now?: () => number;
}

const verified = new Map<string, { at: number; userId: string | null }>();

/** Forget every cached answer (tests). */
export function clearGateCache(): void {
  verified.clear();
}

async function cacheKey(token: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(token),
  );
  return [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * What the gate decided about one token: an admin's Yap user id, a definite
 * "no" (Kova found no session, or one that is not a site admin), or
 * "unknown" when Kova or D1 could not answer (a timeout, a 429, a 5xx).
 */
export type AdminVerdict =
  | { kind: "admin"; userId: string }
  | { kind: "refused" }
  | { kind: "unknown" };

/**
 * The Yap user id behind `token` when it is a live "Yap Admin" Kova session
 * of a site admin with a Yap account that is not suspended; null for
 * anything else, a Kova outage included (the caller still answers a bare
 * 404). Only definite answers are cached: an outage is asked again on the
 * next request, so one slow Kova call cannot lock an admin out for the TTL.
 */
export async function verifyAdminToken(
  token: string | null,
  env: GateEnv,
  deps: GateDeps = {},
): Promise<string | null> {
  const verdict = await adminVerdict(token, env, deps);
  return verdict.kind === "admin" ? verdict.userId : null;
}

export async function adminVerdict(
  token: string | null,
  env: GateEnv,
  deps: GateDeps = {},
): Promise<AdminVerdict> {
  if (!token) return { kind: "refused" };
  const admins = siteAdminIds(env.SITE_ADMIN_USER_IDS);
  const publishableKey = env.KOVA_ADMIN_PUBLISHABLE_KEY?.trim();
  if (admins.size === 0 || !publishableKey) return { kind: "refused" };

  const now = (deps.now ?? Date.now)();
  const key = await cacheKey(token);
  const hit = verified.get(key);
  if (hit && now - hit.at < VERIFY_TTL_MS) {
    return hit.userId
      ? { kind: "admin", userId: hit.userId }
      : { kind: "refused" };
  }

  // Kova's two failures stay apart (kova-auth.ts): Unauthenticated is its
  // answer, AuthUnavailable means it could not give one.
  const lookup = await Effect.runPromise(
    Effect.gen(function* () {
      const kova = yield* KovaAuth;
      const identity = yield* kova.lookup({
        authorization: `Bearer ${token}`,
        cookie: null,
      });
      return { kind: "found" as const, userId: identity.user.id };
    }).pipe(
      Effect.catchTag("Unauthenticated", () =>
        Effect.succeed({ kind: "none" as const }),
      ),
      Effect.catchTag("AuthUnavailable", () =>
        Effect.succeed({ kind: "unavailable" as const }),
      ),
      Effect.provide(
        KovaAuth.layerWith({
          url: (env.KOVA_AUTH_URL || DEFAULT_KOVA_AUTH_URL).replace(/\/$/, ""),
          publishableKey,
          timeout: "5 seconds",
          fetch: (input, init) => (deps.fetch ?? fetch)(input, init),
        }),
      ),
    ),
  );
  if (lookup.kind === "unavailable") return { kind: "unknown" };

  let userId: string | null = null;
  if (lookup.kind === "found" && admins.has(lookup.userId)) {
    const row = await env.DB.prepare(
      `SELECT id FROM users WHERE id = ? AND suspended_at IS NULL`,
    )
      .bind(lookup.userId)
      .first<{ id: string }>()
      .then(
        (found) => ({ ok: true as const, found }),
        () => ({ ok: false as const }),
      );
    // A D1 error is no answer either.
    if (!row.ok) return { kind: "unknown" };
    userId = row.found?.id ?? null;
  }
  // A refusal is cached too, so a probe with a stolen-but-not-admin token
  // costs one Kova call per TTL.
  verified.set(key, { at: now, userId });
  if (verified.size > 500) {
    for (const [entry, value] of verified) {
      if (now - value.at >= VERIFY_TTL_MS) verified.delete(entry);
    }
  }
  return userId ? { kind: "admin", userId } : { kind: "refused" };
}
