import { createFileRoute } from "@tanstack/react-router";
import { Schema } from "effect";

import { adminUserIds } from "@/lib/admin-users";
import {
  apiError,
  apiSuccess,
  getDB,
  getEnv,
  requireAuth,
} from "@/lib/api-helpers";
import { cacheDel, CacheKey } from "@/lib/cache";
import { validateBody } from "@/lib/validate-body";
import { setManualBadge } from "@/services/badge.service";

const badgeAwardSchema = Schema.Struct({
  user_id: Schema.String.check(Schema.isMinLength(1)),
  badge_id: Schema.String.check(Schema.isMinLength(1)),
  /** Defaults to "award". */
  action: Schema.optional(Schema.Literals(["award", "revoke"])),
});

/**
 * POST /api/admin/badges: award or revoke a manual badge (Bug hunter, Yap
 * team). Admins only (COLLECTIBLES_ADMIN_USER_IDS). Automatic and granted
 * badges are refused: their rules own them.
 */
export const POST = async ({ request }: { request: Request }) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;

  const admins = adminUserIds(getEnv());
  // Refuse before reading the body: a non-admin learns nothing about it.
  if (!admins.has(authResult.userId)) {
    return apiError(
      "Only admins can award badges.",
      403,
      "BADGES_FORBIDDEN",
      request,
    );
  }

  const body = await validateBody(request, badgeAwardSchema, request);
  if (body instanceof Response) return body;

  const result = await setManualBadge(getDB(), {
    actorId: authResult.userId,
    adminIds: admins,
    targetUserId: body.user_id,
    badgeId: body.badge_id,
    action: body.action ?? "award",
  });
  if (!result.ok) {
    return apiError(result.message, result.status, result.code, request);
  }
  await cacheDel(CacheKey.userProfile(body.user_id));
  return apiSuccess(
    {
      user_id: body.user_id,
      badge_id: body.badge_id,
      action: body.action ?? "award",
      changed: result.changed,
    },
    200,
    request,
  );
};

export const Route = createFileRoute("/api/admin/badges")({
  server: {
    handlers: {
      POST,
    },
  },
});
