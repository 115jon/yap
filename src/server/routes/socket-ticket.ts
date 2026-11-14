// /api/voice/socket-ticket on the Effect edge: the chat socket's ticket
// (audience "global"). The call sockets get theirs from /api/voice/admission
// (src/server/routes/voice-admission.ts, realtime/ADMISSION.md). The route
// file only wires these handlers.

import { Effect, Schema, type Layer } from "effect";

import { apiError } from "@/lib/api-helpers";
import { looseStruct } from "@/lib/schema";
import { validateBody } from "@/lib/validate-body";
import { issueSocketTicket } from "@/lib/voice/socket-ticket";
import { WorkerEnv } from "@/server/bindings";
import { runRoute, type RouteServices } from "@/server/edge";
import {
  SOCKET_TICKET_TTL_MS,
  admitGlobalSocket,
  createServerTiming,
  getSocketTicketConfig,
  noStore,
} from "./socket-admission";

export const socketTicketRequestBodySchema = looseStruct({
  audience: Schema.optional(Schema.Unknown),
});

/** POST /api/voice/socket-ticket — a short-lived ticket for the chat socket. */
const issueTicket = Effect.fn("voice.socket_ticket")(function* (
  request: Request,
) {
  const timing = createServerTiming();
  const env = yield* WorkerEnv;
  const config = getSocketTicketConfig(env);
  if (!config.ok) {
    return apiError(
      "Realtime admission is not configured",
      503,
      config.code,
      request,
    );
  }

  const body = yield* Effect.promise(() =>
    validateBody(request, socketTicketRequestBodySchema, request),
  );
  if (body instanceof Response) return body;

  // "room" and "voice" were the two-request call join that
  // /api/voice/admission replaced; no client sends them any more.
  if (body.audience !== "global") {
    return apiError(
      "Invalid socket audience",
      400,
      "INVALID_AUDIENCE",
      request,
    );
  }

  const admission = yield* timing.measure(
    "admission",
    admitGlobalSocket(request),
  );
  if (admission instanceof Response) return admission;

  const expiresAt = Date.now() + SOCKET_TICKET_TTL_MS;
  const nonce = crypto.randomUUID();
  const ticket = yield* Effect.promise(() =>
    issueSocketTicket(
      {
        accessMode: admission.accessMode,
        audience: "global",
        expiresAt,
        nonce,
        roomSlug: admission.roomSlug,
        ...(admission.serverId ? { serverId: admission.serverId } : {}),
        ...(admission.channelId ? { channelId: admission.channelId } : {}),
        subject: admission.subject,
      },
      config.ticketSecret,
    ),
  );

  const response = Response.json({
    access_mode: admission.accessMode,
    audience: "global",
    expires_at: expiresAt,
    room_slug: admission.roomSlug,
    ticket,
  });
  if (admission.cookie) response.headers.append("Set-Cookie", admission.cookie);
  response.headers.set("Server-Timing", timing.header());
  return response;
});

/** The handlers, on the given services (the live ones by default). */
export function socketTicketRoutes(services?: Layer.Layer<RouteServices>) {
  return {
    POST: async ({ request }: { request: Request }) =>
      noStore(
        await runRoute(
          "voice.socket_ticket",
          request,
          issueTicket(request),
          services,
        ),
      ),
  };
}
