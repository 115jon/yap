import { createFileRoute } from "@tanstack/react-router";

import { broadcastToChannel, getDB, requireAuth } from "@/lib/api-helpers";
import { requireChannelAccess } from "@/lib/require-channel-access";
import { dmBlockState } from "@/services/block.service";

// POST /api/channels/:id/typing — send typing indicator
const POST = async ({ request: _request, params }: any) => {
  const authResult = await requireAuth();
  if (authResult instanceof Response) return authResult;
  const { userId } = authResult;

  const { id: channelId } = params;

  // Verify channel access
  const accessResult = await requireChannelAccess(userId, channelId);
  if (accessResult instanceof Response) return accessResult;

  // Nobody can type into a 1:1 DM closed by a block: drop it quietly.
  if (
    accessResult.serverId === null &&
    (await dmBlockState(getDB(), userId, channelId))
  ) {
    return new Response(null, { status: 204 });
  }

  await broadcastToChannel(channelId, "TYPING_START", {
    channel_id: channelId,
    user_id: userId,
  });

  return new Response(null, { status: 204 });
};

export const Route = createFileRoute("/api/channels/$id/typing")({
  server: {
    handlers: {
      POST,
    },
  },
});
