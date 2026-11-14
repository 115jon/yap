/**
 * The admin console's line into the realtime Durable Objects.
 *
 *   introspect      the global gateway's /admin/introspect (read-only)
 *   disconnectUser  closes a user's sockets everywhere they are live
 *                   (src/services/user-sockets.service.ts)
 */

import {
  disconnectUserEverywhere,
  introspectGateway,
  type UserSocketBindings,
} from "@/services/user-sockets.service";

import type { AdminGateway, GatewayIntrospection } from "./admin.service";

export { callRoomSlug } from "@/services/user-sockets.service";

export type AdminGatewayBindings = UserSocketBindings;

export function createAdminGateway(
  bindings: AdminGatewayBindings,
): AdminGateway {
  return {
    introspect: () =>
      introspectGateway<GatewayIntrospection>(bindings.meetingRooms),
    disconnectUser: (userId, reason) =>
      disconnectUserEverywhere(bindings, userId, reason),
  };
}
