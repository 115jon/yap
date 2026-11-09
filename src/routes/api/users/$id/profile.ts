import { createFileRoute } from "@tanstack/react-router";

import { apiError, apiSuccess, getDB, requireAuth } from "@/lib/api-helpers";
import { effectiveTier } from "@/lib/entitlements";
import { ServiceError } from "@/lib/service-error";
import { getMe, getUserProfileMutuals } from "@/services/user.service";

const GET = async ({ request, params }: any) => {
  const authResult = await requireAuth(request);
  if (authResult instanceof Response) return authResult;
  const { userId: currentUserId } = authResult;
  const { id: targetUserId } = params;

  const db = getDB();

  try {
    const [user, profile] = await Promise.all([
      getMe(db, targetUserId),
      getUserProfileMutuals(db, targetUserId, currentUserId),
    ]);
    // Someone else's profile carries only their tier; the plan's dates and
    // source are for the user themselves.
    const shown =
      targetUserId === currentUserId
        ? user
        : {
            ...user,
            entitlement: { tier: effectiveTier(user.entitlement) },
          };
    return apiSuccess({ user: shown, ...profile });
  } catch (e) {
    if (e instanceof ServiceError) {
      return apiError(e.message, e.status, e.code);
    }
    console.error("Failed to fetch user profile:", e);
    return apiError("Failed to fetch profile", 500);
  }
};

export const Route = createFileRoute("/api/users/$id/profile")({
  server: {
    handlers: {
      GET,
    },
  },
});
