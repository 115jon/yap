// /api/account/deletion on the Effect edge: self-service account deletion
// (services/account-deletion.service.ts).
//
//   GET     { status: "active" } | { status: "pending", requested_at,
//             delete_after, purging }
//   POST    { confirm_username }  → schedule deletion in 30 days. 409
//             OWNS_SERVERS (with `servers`) while the account owns a server,
//             400 CONFIRMATION_MISMATCH when the username does not match.
//   DELETE  restore the account (cancel the deletion).
//
// All three work while the account is pending; every other route refuses it
// with 403 ACCOUNT_PENDING_DELETION (lib/account-deletion.ts). After a POST
// the route signs out the account's other Yap sessions at Kova and closes its
// sockets; the client that asked signs itself out the usual way.

import { Effect, Option, Schema, type Layer } from "effect";

import {
  deletionDateFrom,
  invalidatePendingDeletionCache,
} from "@/lib/account-deletion";
import { apiSuccess, getCorsHeaders } from "@/lib/api-helpers";
import { validateBody } from "@/lib/validate-body";
import { Database, Gateway, RateLimiter, WorkerEnv } from "@/server/bindings";
import { runRoute, type RouteServices } from "@/server/edge";
import { service } from "@/server/errors";
import {
  appSessionCredentials,
  KovaAuth,
  signedInIdentity,
} from "@/server/kova-auth";
import {
  getDeletionStatus,
  OwnsServersError,
  requestAccountDeletion,
  restoreAccount,
  type DeletionEffects,
} from "@/services/account-deletion.service";
import { executeInvalidation } from "@/services/service-helpers";
import { disconnectUserEverywhere } from "@/services/user-sockets.service";

const RATE_LIMIT = { limit: 10, windowMs: 60 * 60 * 1000 };

const requestSchema = Schema.Struct({
  confirm_username: Schema.String.check(Schema.isMaxLength(64)),
});

/** The caller, allowed through while their account is pending deletion. */
function caller(request: Request) {
  const credentials = appSessionCredentials(request);
  return signedInIdentity(request, credentials, {
    allowPendingDeletion: true,
  }).pipe(Effect.map((identity) => ({ identity, credentials })));
}

const applyEffects = Effect.fn("accountDeletion.effects")(function* (
  effects: DeletionEffects,
) {
  const gateway = yield* Gateway;
  yield* Effect.promise(() =>
    executeInvalidation(effects.cacheKeysToInvalidate).catch(() => undefined),
  );
  for (const broadcast of effects.broadcasts) yield* gateway.send(broadcast);
});

/** GET /api/account/deletion */
const getStatus = Effect.fn("accountDeletion.status")(function* (
  request: Request,
) {
  const { identity } = yield* caller(request);
  const db = yield* Database;
  const status = yield* service(() => getDeletionStatus(db, identity.user.id));
  return apiSuccess(status, 200, request);
});

/** POST /api/account/deletion — `{ confirm_username }` */
const requestDeletion = Effect.fn("accountDeletion.request")(function* (
  request: Request,
) {
  const { identity, credentials } = yield* caller(request);
  const userId = identity.user.id;
  const limited = yield* (yield* RateLimiter).check(
    userId,
    "account-deletion",
    RATE_LIMIT,
  );
  if (limited) return limited;

  const body = yield* Effect.promise(() =>
    validateBody(request, requestSchema, request),
  );
  if (body instanceof Response) return body;

  const db = yield* Database;
  const now = Date.now();
  const result = yield* Effect.tryPromise({
    try: () =>
      requestAccountDeletion(db, userId, {
        confirmUsername: body.confirm_username,
        now,
        deleteAfter: deletionDateFrom(now),
      }),
    catch: (cause) => cause,
  }).pipe(
    Effect.catch((cause) =>
      cause instanceof OwnsServersError
        ? Effect.succeed(
            Response.json(
              {
                error: cause.message,
                code: cause.code,
                servers: cause.servers,
              },
              { status: 409, headers: getCorsHeaders(request) },
            ),
          )
        : service(() => Promise.reject(cause)),
    ),
  );
  if (result instanceof Response) return result;

  invalidatePendingDeletionCache();
  yield* applyEffects(result);

  // Other devices: signed out at Kova where it can, and refused here anyway.
  const kova = yield* KovaAuth;
  const revoked = yield* kova
    .revokeAppSessions(credentials, { allOthers: true })
    .pipe(
      Effect.map((count) => Option.getOrNull(count)),
      Effect.catch(() => Effect.succeed(null)),
    );
  const env = yield* WorkerEnv;
  yield* Effect.promise(() =>
    disconnectUserEverywhere(
      { db, meetingRooms: env.MEETING_ROOM, voiceRooms: env.VOICE_ROOM },
      userId,
      "Account scheduled for deletion",
    ).catch(() => null),
  );
  yield* Effect.logInfo("account.deletion_requested").pipe(
    Effect.annotateLogs({
      servers_left: result.serverIds.length,
      sessions_revoked: revoked,
    }),
  );

  return apiSuccess(result.status, 200, request);
});

/** DELETE /api/account/deletion — restore the account. */
const restore = Effect.fn("accountDeletion.restore")(function* (
  request: Request,
) {
  const { identity } = yield* caller(request);
  const db = yield* Database;
  const result = yield* service(() =>
    restoreAccount(db, identity.user.id, Date.now()),
  );
  invalidatePendingDeletionCache();
  yield* applyEffects(result);
  return apiSuccess(result.status, 200, request);
});

/** The handlers, on the given services (the live ones by default). */
export function accountDeletionRoutes(services?: Layer.Layer<RouteServices>) {
  return {
    GET: ({ request }: { request: Request }) =>
      runRoute("accountDeletion.status", request, getStatus(request), services),
    POST: ({ request }: { request: Request }) =>
      runRoute(
        "accountDeletion.request",
        request,
        requestDeletion(request),
        services,
      ),
    DELETE: ({ request }: { request: Request }) =>
      runRoute("accountDeletion.restore", request, restore(request), services),
  };
}
