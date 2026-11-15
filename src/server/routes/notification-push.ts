// /api/notifications/:id/push on the Effect edge: the text of one of the
// caller's notifications, which the phone fetches for a push that carries
// ids only (push.service.ts). The route file only wires this handler.

import { Effect, type Layer } from "effect";

import { apiError, apiSuccess } from "@/lib/api-helpers";
import { Database } from "@/server/bindings";
import { runRoute, type RouteServices } from "@/server/edge";
import { signedInUserId } from "@/server/kova-auth";
import { notificationPushText } from "@/services/push.service";

/** Notification ids are UUIDs or `message:<id>:<type>:<user>` keys. */
const NOTIFICATION_ID = /^[\w:.-]{1,256}$/;

/**
 * GET /api/notifications/:id/push — 404 alike for a notification that is
 * not the caller's, a deleted message, a channel the caller can no longer
 * see, a block either way, a read notification and Do Not Disturb, so the
 * answer says nothing about which. A database failure is a 500 (the device
 * then shows its fallback), never this 404 (which shows nothing).
 */
const pushText = Effect.fn("notifications.pushText")(function* (
  request: Request,
  notificationId: string,
) {
  const userId = yield* signedInUserId(request);
  const notFound = () =>
    apiError("Notification not found", 404, "NOTIFICATION_NOT_FOUND", request);
  if (!NOTIFICATION_ID.test(notificationId)) return notFound();

  const db = yield* Database;
  const text = yield* Effect.promise(() =>
    notificationPushText(db, userId, notificationId),
  );
  if (!text) return notFound();
  const response = apiSuccess(text, 200, request);
  response.headers.set("Cache-Control", "no-store");
  return response;
});

/** The handler, on the given services (the live ones by default). */
export function notificationPushRoutes(services?: Layer.Layer<RouteServices>) {
  return {
    GET: ({ request, params }: { request: Request; params: { id: string } }) =>
      runRoute(
        "notifications.pushText",
        request,
        pushText(request, params.id),
        services,
      ),
  };
}
