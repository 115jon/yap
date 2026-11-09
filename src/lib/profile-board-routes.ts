// Production wiring for the profile board handlers: Kova auth, the global
// rate limiter, the bound D1/R2 and the gateway broadcast. The handlers
// live in services/profile-board-handlers.

import { waitUntil } from "cloudflare:workers";

import {
  broadcastToUser,
  broadcastToUserServers,
  getBucket,
  getDB,
  requireAuth,
} from "@/lib/api-helpers";
import { checkRateLimitDO } from "@/lib/rate-limit";
import {
  createProfileBoardHandlers,
  PROFILE_BOARD_RATE_LIMITS,
  type ProfileBoardHandlers,
} from "@/services/profile-board-handlers";

/** The gateway event an open profile listens for to refetch the board. */
export const PROFILE_BOARD_UPDATE = "PROFILE_BOARD_UPDATE";

function handlers(): ProfileBoardHandlers {
  return createProfileBoardHandlers({
    db: getDB() as unknown as D1Database,
    bucket: getBucket() as unknown as R2Bucket,
    authenticate: (request) => requireAuth(request),
    rateLimit: (userId, action) =>
      checkRateLimitDO(
        userId,
        `profile-board-${action}`,
        PROFILE_BOARD_RATE_LIMITS[action],
      ),
    broadcastBoardUpdate: async (userId, version) => {
      const data = { user_id: userId, version };
      await Promise.all([
        broadcastToUserServers(userId, PROFILE_BOARD_UPDATE, data),
        // The owner's other sessions (another device's editor).
        broadcastToUser(userId, PROFILE_BOARD_UPDATE, data),
      ]);
    },
    waitUntil,
  });
}

type RouteContext = {
  request: Request;
  params: { id: string; eventId?: string; noteId?: string; userId?: string };
};

/** A TanStack server handler for one profile board action. */
export function profileBoardRoute(
  action: keyof ProfileBoardHandlers,
): (context: RouteContext) => Promise<Response> {
  return ({ request, params }) => {
    const h = handlers();
    switch (action) {
      case "patchActivityEvent":
        return h.patchActivityEvent(request, params.id, params.eventId ?? "");
      case "deleteGuestbookNote":
        return h.deleteGuestbookNote(request, params.id, params.noteId ?? "");
      case "unblockGuestbook":
        return h.unblockGuestbook(request, params.id, params.userId ?? "");
      default:
        return h[action](request, params.id);
    }
  };
}
