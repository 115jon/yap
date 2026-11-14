import { createFileRoute } from "@tanstack/react-router";
import { Schema } from "effect";

import {
  apiSuccess,
  broadcastToUser,
  getDB,
  requireAuth,
} from "@/lib/api-helpers";
import { ServiceError } from "@/lib/service-error";
import { validateBody } from "@/lib/validate-body";
import {
  listNotificationSettings,
  updateNotificationSetting,
} from "@/services/notification-settings.service";

const updateSchema = Schema.Struct({
  target_type: Schema.Literals(["server", "channel"]),
  target_id: Schema.String.check(
    Schema.isMinLength(1),
    Schema.isMaxLength(128),
  ),
  level: Schema.optional(
    Schema.NullOr(Schema.Literals(["all", "mentions", "none"])),
  ),
  mute: Schema.optional(Schema.Literals(["1h", "8h", "24h", "forever", "off"])),
});

// GET /api/notification-settings — the user's levels and mutes
const GET = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const settings = await listNotificationSettings(getDB(), authResult.userId);
  return apiSuccess({ settings, server_time: new Date().toISOString() });
};

// PUT /api/notification-settings — set one server's, channel's or DM's level
// and/or mute. Every device of the user gets NOTIFICATION_SETTINGS_UPDATE.
const PUT = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;

  const body = await validateBody(request, updateSchema, request);
  if (body instanceof Response) return body;

  try {
    const setting = await updateNotificationSetting(
      getDB(),
      userId,
      body.target_type,
      body.target_id,
      { level: body.level, mute: body.mute },
    );
    await broadcastToUser(userId, "NOTIFICATION_SETTINGS_UPDATE", setting);
    return apiSuccess({ setting });
  } catch (e) {
    if (e instanceof ServiceError) {
      return Response.json(
        { error: e.message, code: e.code },
        { status: e.status },
      );
    }
    throw e;
  }
};

export const Route = createFileRoute("/api/notification-settings")({
  server: {
    handlers: {
      GET,
      PUT,
    },
  },
});
