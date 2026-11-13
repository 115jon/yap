// /api/sessions on the Effect edge: the Devices list in settings.
//
// GET lists the caller's Yap sessions (Kova app sessions: `session.app_id`
// rows for this app's key), the caller's own first, each with the activity
// the clients title the row with. DELETE signs out one other session or all
// of them. Both go to Kova with the caller's own app token; Kova scopes them
// to this app and this user. The caller's own session is signed out the
// usual way (Kova's /:pk/revoke-session with its token), never here.
//
// Until Kova has the app-session endpoints (kova-auth.ts), GET answers the
// caller's own session alone with `othersListed: false`, and DELETE 501. GET
// does the same when the list call fails: the caller's own row is known.
//
// A revoked session's push tokens are deleted with it (push.service
// deleteSessionPushTokens), so the signed-out device stops getting pushes.

import { Effect, Option, Schema, type Layer } from "effect";

import { apiError, apiSuccess } from "@/lib/api-helpers";
import { describeSessionClient, requestGeo } from "@/lib/session-activity";
import { validateBody } from "@/lib/validate-body";
import { Database } from "@/server/bindings";
import { runRoute, type RouteServices } from "@/server/edge";
import {
  appSessionCredentials,
  KovaAuth,
  signedInIdentity,
  type KovaAppSession,
  type KovaAuthSession,
} from "@/server/kova-auth";
import { deleteSessionPushTokens } from "@/services/push.service";

/** One row of `GET /api/sessions`. Earlier fields are what clients read. */
export interface SessionRow {
  id: string;
  clientId: null;
  status: "active";
  /**
   * Epoch ms: now for the caller's own; for the others, when Kova last saw
   * the token used, or null when it does not record that.
   */
  lastActiveAt: number | null;
  createdAt: number | null;
  expireAt: number | null;
  isCurrent: boolean;
  activity: ReturnType<typeof describeSessionClient>;
}

/** Epoch ms from Kova's ISO text or epoch numbers, else null. */
function epochMs(value: unknown): number | null {
  if (value instanceof Date) {
    const ms = value.getTime();
    return Number.isNaN(ms) ? null : ms;
  }
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (typeof value !== "string" || !value.trim()) return null;
  const numeric = Number(value);
  if (Number.isFinite(numeric)) return numeric;
  const parsed = Date.parse(value);
  return Number.isNaN(parsed) ? null : parsed;
}

function currentRow(request: Request, session: KovaAuthSession): SessionRow {
  return {
    id: session.id ?? "",
    clientId: null,
    status: "active",
    lastActiveAt: Date.now(),
    createdAt: epochMs(session.createdAt),
    expireAt: epochMs(session.expiresAt),
    isCurrent: true,
    // This request is the device: its User-Agent and location are current.
    activity: describeSessionClient({
      userAgent: request.headers.get("user-agent") ?? session.userAgent,
      ...requestGeo(request),
    }),
  };
}

/**
 * Kova's own last-used time when it sends one. Its `updatedAt` only counts
 * when it moved past `createdAt`: an app session's row is written once, so
 * an unmoved one is the sign-in time, not activity.
 */
function lastActive(session: KovaAppSession): number | null {
  const reported = epochMs(session.lastActiveAt);
  if (reported !== null) return reported;
  const updated = epochMs(session.updatedAt);
  const created = epochMs(session.createdAt);
  return updated !== null && created !== null && updated > created
    ? updated
    : null;
}

function listedRow(session: KovaAppSession): SessionRow {
  return {
    id: session.id,
    clientId: null,
    status: "active",
    lastActiveAt: lastActive(session),
    createdAt: epochMs(session.createdAt),
    expireAt: epochMs(session.expiresAt),
    isCurrent: false,
    activity: describeSessionClient({
      userAgent: session.userAgent,
      platform: session.devicePlatform,
      deviceName: session.deviceName,
      city: session.city,
      country: session.country,
    }),
  };
}

/** The caller's own row first, then the most recently active. */
function sessionRows(
  request: Request,
  current: KovaAuthSession,
  listed: ReadonlyArray<KovaAppSession>,
): SessionRow[] {
  const now = Date.now();
  const others = listed
    .flatMap((session) => {
      if (session.id === current.id) return [];
      const row = listedRow(session);
      // Kova lists only live sessions; this guards a stale or clock-skewed row.
      return row.expireAt === null || row.expireAt > now ? [row] : [];
    })
    .sort(
      (a, b) =>
        (b.lastActiveAt ?? b.createdAt ?? 0) -
        (a.lastActiveAt ?? a.createdAt ?? 0),
    );
  return [currentRow(request, current), ...others];
}

/** GET /api/sessions — the caller's Yap sessions. */
const listSessions = Effect.fn("sessions.list")(function* (request: Request) {
  // Identified by the very credential the list goes out with.
  const credentials = appSessionCredentials(request);
  const identity = yield* signedInIdentity(request, credentials);
  const kova = yield* KovaAuth;
  const listed = yield* kova.listAppSessions(credentials).pipe(
    Effect.catchTags({
      AuthUnavailable: (error) =>
        Effect.logWarning("sessions.list_unavailable").pipe(
          Effect.annotateLogs({ reason: error.reason }),
          Effect.as(Option.none()),
        ),
      ServiceFailure: (error) =>
        Effect.logWarning("sessions.list_refused").pipe(
          Effect.annotateLogs({ code: error.code ?? null }),
          Effect.as(Option.none()),
        ),
    }),
  );

  return apiSuccess(
    {
      sessions: sessionRows(
        request,
        identity.session,
        Option.getOrElse(listed, () => []),
      ),
      // False when Kova could not list them: other devices may exist.
      othersListed: Option.isSome(listed),
    },
    200,
    request,
  );
});

const revokeSchema = Schema.Struct({
  sessionId: Schema.optional(
    Schema.Trim.check(Schema.isMinLength(1), Schema.isMaxLength(256)),
  ),
  allOthers: Schema.optional(Schema.Boolean),
});

/** DELETE /api/sessions — sign out `{ sessionId }` or `{ allOthers: true }`. */
const revokeSessions = Effect.fn("sessions.revoke")(function* (
  request: Request,
) {
  const credentials = appSessionCredentials(request);
  const identity = yield* signedInIdentity(request, credentials);
  const body = yield* Effect.promise(() =>
    validateBody(request, revokeSchema, request),
  );
  if (body instanceof Response) return body;

  const allOthers = body.allOthers === true;
  if (allOthers === Boolean(body.sessionId)) {
    return apiError(
      "Send either sessionId or allOthers: true",
      400,
      "INVALID_REQUEST",
      request,
    );
  }
  if (body.sessionId && body.sessionId === identity.session.id) {
    return apiError(
      "This is the session making the request; sign out instead",
      400,
      "CURRENT_SESSION",
      request,
    );
  }

  const kova = yield* KovaAuth;
  const revoked = yield* kova.revokeAppSessions(
    credentials,
    body.sessionId ? { sessionId: body.sessionId } : { allOthers: true },
  );

  if (Option.isNone(revoked)) {
    return apiError(
      "Signing out another device is not available yet",
      501,
      "SESSION_REVOKE_UNSUPPORTED",
      request,
    );
  }

  const db = yield* Database;
  yield* Effect.promise(() =>
    deleteSessionPushTokens(
      db,
      identity.user.id,
      body.sessionId
        ? { sessionId: body.sessionId }
        : { allExcept: identity.session.id ?? "" },
    ),
  );

  // A session that was already gone counts 0: the device is signed out.
  // Null when Kova did not say how many.
  return apiSuccess({ revoked: revoked.value }, 200, request);
});

/** The handlers, on the given services (the live ones by default). */
export function sessionRoutes(services?: Layer.Layer<RouteServices>) {
  return {
    GET: ({ request }: { request: Request }) =>
      runRoute("sessions.list", request, listSessions(request), services),
    DELETE: ({ request }: { request: Request }) =>
      runRoute("sessions.revoke", request, revokeSessions(request), services),
  };
}
