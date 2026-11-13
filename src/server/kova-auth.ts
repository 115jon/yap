// ── KovaAuth ────────────────────────────────────────────────────────────────
// Who is calling, as Kova sees it. A lookup either finds a session or fails
// with `Unauthenticated` (Kova's answer is "no") or `AuthUnavailable` (Kova
// could not answer). The two must never be confused: a 401 tells clients to
// stop and sign in, a 503 tells them to retry.

import { env } from "cloudflare:workers";
import { Context, Effect, Layer, Option, Schema, type Duration } from "effect";

import {
  ACCOUNT_PENDING_DELETION,
  pendingDeletionDate,
  pendingDeletionMessage,
} from "@/lib/account-deletion";
import { looseStruct } from "@/lib/schema";
import {
  DEFAULT_KOVA_AUTH_PUBLISHABLE_KEY,
  DEFAULT_KOVA_AUTH_URL,
} from "@/lib/site-defaults";
import {
  ACCOUNT_SUSPENDED,
  isSuspended,
  SUSPENDED_MESSAGE,
} from "@/lib/suspension";
import {
  AuthUnavailable,
  ServiceFailure,
  Unauthenticated,
} from "@/server/errors";

export interface KovaAuthUser {
  id: string;
  name?: string | null;
  email?: string | null;
  image?: string | null;
  imageUrl?: string | null;
  username?: string | null;
  firstName?: string | null;
  lastName?: string | null;
  bio?: string | null;
  primaryEmailAddress?: { emailAddress?: string | null } | null;
}

export interface KovaAuthSession {
  id?: string;
  token?: string;
  userId?: string;
  expiresAt?: string | number | Date;
  createdAt?: string | number | Date;
  updatedAt?: string | number | Date;
  ipAddress?: string | null;
  userAgent?: string | null;
}

export interface KovaIdentity {
  readonly user: KovaAuthUser;
  readonly session: KovaAuthSession;
}

/**
 * One of the caller's sessions for this app, as Kova lists them: never a
 * token. Only `id` is required; the device fields are there when Kova kept
 * what a device sign-in reported.
 */
export interface KovaAppSession {
  id: string;
  createdAt?: string | number | null;
  updatedAt?: string | number | null;
  /** When the session's token was last used, if Kova records it. */
  lastActiveAt?: string | number | null;
  expiresAt?: string | number | null;
  userAgent?: string | null;
  /** The device sign-in's `platform` (`windows`, `android`, ...). */
  devicePlatform?: string | null;
  /** The device sign-in's `hostname`. */
  deviceName?: string | null;
  city?: string | null;
  country?: string | null;
}

/** Which of the caller's other sessions for this app to sign out. */
export type AppSessionRevocation =
  | { readonly sessionId: string }
  | { readonly allOthers: true };

/** The headers Kova reads a session from. Forwarded as they are, never logged. */
export interface SessionHeaders {
  readonly authorization: string | null;
  readonly cookie: string | null;
}

/**
 * Everything a request may prove its session with: its headers. Never the
 * query string: a URL leaks into logs, the clipboard and shared links, so
 * the private media routes take a capability there instead (lib/media-auth).
 */
export type Credentials = SessionHeaders;

export interface KovaOptions {
  readonly url: string;
  readonly publishableKey: string;
  /** A hung Kova must not hang every request behind it. */
  readonly timeout: Duration.Input;
  /** Read at call time, so a test can hand in a fake Kova. */
  readonly fetch: typeof fetch;
}

const KOVA_SESSION_TIMEOUT: Duration.Input = "5 seconds";

/** Statuses that mean "try again", not "no session". */
function isKovaUnavailableStatus(status: number): boolean {
  return status >= 500 || status === 429 || status === 408;
}

export class KovaAuth extends Context.Service<
  KovaAuth,
  {
    /** One get-session call with these headers. */
    readonly lookup: (
      headers: SessionHeaders,
    ) => Effect.Effect<KovaIdentity, Unauthenticated | AuthUnavailable>;
    /**
     * The caller's sessions for this app. `None`: this Kova has no such
     * endpoint yet (it answers 404), so only the caller's own is known.
     */
    readonly listAppSessions: (
      headers: SessionHeaders,
    ) => Effect.Effect<
      Option.Option<ReadonlyArray<KovaAppSession>>,
      Unauthenticated | AuthUnavailable | ServiceFailure
    >;
    /**
     * Signs out other sessions of the caller's for this app; how many went
     * (null when Kova did not say). `None`: this Kova cannot revoke another
     * session yet.
     */
    readonly revokeAppSessions: (
      headers: SessionHeaders,
      revocation: AppSessionRevocation,
    ) => Effect.Effect<
      Option.Option<number | null>,
      Unauthenticated | AuthUnavailable | ServiceFailure
    >;
  }
>()("ralph-meet/server/KovaAuth") {
  /** Kova over HTTP with these options. Tests point `fetch` at a fake Kova. */
  static layerWith(options: KovaOptions): Layer.Layer<KovaAuth> {
    return Layer.succeed(KovaAuth, kovaOver(options));
  }

  /** Kova from the Worker's env, falling back to the site defaults. */
  static readonly layer: Layer.Layer<KovaAuth> = Layer.sync(KovaAuth, () =>
    kovaOver(liveOptions()),
  );
}

function kovaOver(options: KovaOptions) {
  return KovaAuth.of({
    lookup: (headers) => lookupSession(options, headers),
    listAppSessions: (headers) => listAppSessions(options, headers),
    revokeAppSessions: (headers, revocation) =>
      revokeAppSessions(options, headers, revocation),
  });
}

function liveOptions(): KovaOptions {
  const authEnv = env as unknown as {
    KOVA_AUTH_URL?: string;
    VITE_KOVA_AUTH_PUBLISHABLE_KEY?: string;
  };

  return {
    url: (authEnv.KOVA_AUTH_URL ?? DEFAULT_KOVA_AUTH_URL).replace(/\/$/, ""),
    // Under `vite dev` the define from vite.config.ts comes first: it is the
    // client's key too, and .dev.vars cannot override it (lib/dev-auth-target).
    publishableKey:
      import.meta.env.RALPH_DEV_KOVA_KEY ??
      authEnv.VITE_KOVA_AUTH_PUBLISHABLE_KEY ??
      DEFAULT_KOVA_AUTH_PUBLISHABLE_KEY,
    timeout: KOVA_SESSION_TIMEOUT,
    fetch: (input, init) => fetch(input, init),
  };
}

// Kova's get-session answer. Only `user.id` is required; the rest of the
// user and session objects pass through as Kova sent them.
const decodeSessionBody = Schema.decodeUnknownOption(
  Schema.Struct({
    user: looseStruct({ id: Schema.String }),
    session: looseStruct({}),
  }),
);

function lookupSession(
  options: KovaOptions,
  headers: SessionHeaders,
): Effect.Effect<KovaIdentity, Unauthenticated | AuthUnavailable> {
  // No credentials: nothing to ask Kova about, and nothing it could say yes to.
  if (!headers.authorization && !headers.cookie) {
    return Effect.fail(new Unauthenticated());
  }

  const request = new Headers();

  if (headers.authorization)
    request.set("authorization", headers.authorization);
  if (headers.cookie) request.set("cookie", headers.cookie);
  if (options.publishableKey) {
    request.set("x-publishable-key", options.publishableKey);
  }

  return Effect.gen(function* () {
    const res = yield* Effect.tryPromise({
      try: (signal) =>
        options.fetch(`${options.url}/api/auth/get-session`, {
          headers: request,
          signal,
        }),
      catch: (error) =>
        new AuthUnavailable({
          reason: error instanceof Error ? error.name : "fetch failed",
        }),
    });

    if (isKovaUnavailableStatus(res.status)) {
      return yield* new AuthUnavailable({ reason: `HTTP ${res.status}` });
    }

    // Any other refusal (401, 403, 400...) is Kova's answer: no session.
    if (!res.ok) return yield* new Unauthenticated();

    // A 2xx that is not JSON is a proxy or edge error page, not an answer.
    const body: unknown = yield* Effect.tryPromise({
      try: () => res.json(),
      catch: () => new AuthUnavailable({ reason: "unreadable body" }),
    });

    const decoded = decodeSessionBody(body);

    if (Option.isNone(decoded)) return yield* new Unauthenticated();

    return {
      // SAFETY: Kova's user and session objects; only `id` is checked, the
      // other fields stay optional and are read defensively by callers.
      user: decoded.value.user as KovaAuthUser,
      session: decoded.value.session as KovaAuthSession,
    };
  }).pipe(
    // Covers the body read too: a hung stream is as bad as a hung connect.
    Effect.timeoutOrElse({
      duration: options.timeout,
      orElse: () => Effect.fail(new AuthUnavailable({ reason: "timeout" })),
    }),
  );
}

// ── The caller's app sessions ────────────────────────────────────────────────
// Kova's stock list-sessions and revoke-other-sessions refuse app-scoped
// tokens (they reach the platform session and its token), and its
// /:pk/revoke-session only signs out the token presented. These two calls are
// the app-scoped equivalents, scoped by Kova to `session.app_id` of the key
// and the caller's user, never returning a token:
//
//   GET  /api/pub/apps/:pk/sessions          → { sessions: KovaAppSession[] }
//   POST /api/pub/apps/:pk/sessions/revoke   { sessionId } | { allOthers: true }
//                                            → { revoked?: number }
//
// A Kova without them answers 404, which reads as "not supported" (`None`).

const decodeAppSessionsBody = Schema.decodeUnknownOption(
  Schema.Struct({
    sessions: Schema.Array(looseStruct({ id: Schema.String })),
  }),
);

const decodeRevokedBody = Schema.decodeUnknownOption(
  Schema.Struct({ revoked: Schema.optional(Schema.Number) }),
);

/**
 * One call to an app-session endpoint. `None` for a 404 or 405 (this Kova
 * has no such endpoint). Any other refusal, 401 included, is a 502: the
 * caller's session was just looked up, and a 401 here would sign the client
 * out over an answer about other sessions. The body is `null` when there is
 * none (a 204).
 */
function appSessionCall(
  options: KovaOptions,
  headers: SessionHeaders,
  path: string,
  init: { method: "GET" | "POST"; body?: unknown },
): Effect.Effect<
  Option.Option<unknown>,
  Unauthenticated | AuthUnavailable | ServiceFailure
> {
  if (!headers.authorization && !headers.cookie) {
    return Effect.fail(new Unauthenticated());
  }

  const request = new Headers();
  if (headers.authorization)
    request.set("authorization", headers.authorization);
  if (headers.cookie) request.set("cookie", headers.cookie);
  if (options.publishableKey) {
    request.set("x-publishable-key", options.publishableKey);
  }
  if (init.body !== undefined) request.set("content-type", "application/json");

  const url = `${options.url}/api/pub/apps/${encodeURIComponent(options.publishableKey)}${path}`;

  return Effect.gen(function* () {
    const res = yield* Effect.tryPromise({
      try: (signal) =>
        options.fetch(url, {
          method: init.method,
          headers: request,
          body: init.body === undefined ? undefined : JSON.stringify(init.body),
          signal,
        }),
      catch: (error) =>
        new AuthUnavailable({
          reason: error instanceof Error ? error.name : "fetch failed",
        }),
    });

    if (isKovaUnavailableStatus(res.status)) {
      return yield* new AuthUnavailable({ reason: `HTTP ${res.status}` });
    }
    if (res.status === 404 || res.status === 405) return Option.none();
    if (!res.ok) {
      return yield* new ServiceFailure({
        message: "The sign-in service refused to change this session.",
        status: 502,
        code: "KOVA_REFUSED",
      });
    }

    const text = yield* Effect.tryPromise({
      try: () => res.text(),
      catch: () => new AuthUnavailable({ reason: "unreadable body" }),
    });
    if (!text.trim()) return Option.some<unknown>(null);
    // A 2xx that is not JSON is a proxy or edge error page, not an answer.
    return Option.some(
      yield* Effect.try({
        try: (): unknown => JSON.parse(text),
        catch: () => new AuthUnavailable({ reason: "unreadable body" }),
      }),
    );
  }).pipe(
    Effect.timeoutOrElse({
      duration: options.timeout,
      orElse: () => Effect.fail(new AuthUnavailable({ reason: "timeout" })),
    }),
  );
}

function listAppSessions(options: KovaOptions, headers: SessionHeaders) {
  return appSessionCall(options, headers, "/sessions", { method: "GET" }).pipe(
    Effect.flatMap(
      Option.match({
        onNone: () => Effect.succeed(Option.none()),
        onSome: (body) => {
          const decoded = decodeAppSessionsBody(body);
          return Option.isNone(decoded)
            ? Effect.fail(new AuthUnavailable({ reason: "unreadable body" }))
            : // SAFETY: Kova's session rows; only `id` is checked, the other
              // fields stay optional and are read defensively by callers.
              Effect.succeed(
                Option.some(
                  decoded.value.sessions as ReadonlyArray<KovaAppSession>,
                ),
              );
        },
      }),
    ),
  );
}

function revokeAppSessions(
  options: KovaOptions,
  headers: SessionHeaders,
  revocation: AppSessionRevocation,
) {
  return appSessionCall(options, headers, "/sessions/revoke", {
    method: "POST",
    body: revocation,
  }).pipe(
    Effect.flatMap(
      Option.match({
        onNone: () => Effect.succeed(Option.none<number | null>()),
        // Any 2xx means Kova did it; the count is a courtesy.
        onSome: (body) =>
          Effect.succeed(
            Option.some(
              Option.match(decodeRevokedBody(body), {
                onNone: () => null,
                onSome: (decoded) => decoded.revoked ?? null,
              }),
            ),
          ),
      }),
    ),
  );
}

function bearerToken(authorization: string | null): string | null {
  return authorization?.startsWith("Bearer ")
    ? authorization.slice("Bearer ".length)
    : null;
}

/** The credentials on a request: its authorization and cookie headers. */
function credentialsOf(request: Request): Credentials {
  return {
    authorization: request.headers.get("authorization"),
    cookie: request.headers.get("cookie"),
  };
}

/**
 * Who is calling. First the request's own headers; when Kova finds no
 * session there, the desktop app's bearer token alone. Either
 * lookup finding a session wins. If neither does and either could not reach
 * Kova, the answer is `AuthUnavailable`, not `Unauthenticated`.
 */
export const authenticate = Effect.fn("kova.authenticate")(function* (
  credentials: Credentials,
) {
  const kova = yield* KovaAuth;
  const token = bearerToken(credentials.authorization);

  const byToken = token
    ? kova.lookup({ authorization: `Bearer ${token}`, cookie: null })
    : Effect.fail(new Unauthenticated());

  return yield* kova
    .lookup(credentials)
    .pipe(
      Effect.catch((first) =>
        byToken.pipe(
          Effect.catchTag("Unauthenticated", () => Effect.fail(first)),
        ),
      ),
    );
});

/**
 * The one credential a call about the caller's own sessions uses: the bearer
 * alone when there is one, else the cookie. The caller is
 * identified with exactly this (`signedInIdentity(request, credentials)`),
 * so "the current session" and the session Kova acts for are the same.
 */
export function appSessionCredentials(request: Request): Credentials {
  const credentials = credentialsOf(request);
  const token = bearerToken(credentials.authorization);
  return token
    ? { authorization: `Bearer ${token}`, cookie: null }
    : credentials;
}

/**
 * The signed-in caller, user and session, from `credentials` (the request's
 * own by default). A suspended account (lib/suspension) fails with a 403
 * ACCOUNT_SUSPENDED, as `requireAuth` answers it; an account waiting for
 * deletion (lib/account-deletion) with a 403 ACCOUNT_PENDING_DELETION,
 * unless `allowPendingDeletion` (the routes that show or undo the deletion).
 */
export function signedInIdentity(
  request: Request,
  credentials: Credentials = credentialsOf(request),
  options: { allowPendingDeletion?: boolean } = {},
): Effect.Effect<
  KovaIdentity,
  Unauthenticated | AuthUnavailable | ServiceFailure,
  KovaAuth
> {
  return Effect.flatMap(authenticate(credentials), (identity) =>
    Effect.flatMap(
      Effect.promise(async () => ({
        suspended: await isSuspended(identity.user.id),
        deleteAfter: options.allowPendingDeletion
          ? null
          : await pendingDeletionDate(identity.user.id),
      })),
      ({ suspended, deleteAfter }) =>
        suspended
          ? Effect.fail(
              new ServiceFailure({
                message: SUSPENDED_MESSAGE,
                status: 403,
                code: ACCOUNT_SUSPENDED,
              }),
            )
          : deleteAfter
            ? Effect.fail(
                new ServiceFailure({
                  message: pendingDeletionMessage(deleteAfter),
                  status: 403,
                  code: ACCOUNT_PENDING_DELETION,
                }),
              )
            : Effect.succeed(identity),
    ),
  );
}

/**
 * The signed-in caller's user id: what `requireAuth` gave routes, as a typed
 * lookup. A suspended account fails as in `signedInIdentity`.
 */
export function signedInUserId(
  request: Request,
): Effect.Effect<
  string,
  Unauthenticated | AuthUnavailable | ServiceFailure,
  KovaAuth
> {
  return Effect.map(signedInIdentity(request), (identity) => identity.user.id);
}
