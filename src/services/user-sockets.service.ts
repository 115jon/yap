/**
 * A user's live sockets, closed everywhere they are open:
 *   - the global gateway (chat, presence)
 *   - the MeetingRoom of each voice channel or DM call the gateway's roster
 *     puts them in (/admin/disconnect-user)
 *   - the VoiceRoom of the same slug, per session ended there
 *     (/disconnect-participant, as voice-disconnect.ts does)
 *
 * Used by the admin console (suspensions, apps/admin) and account deletion.
 * Every call goes through a Durable Object stub; none of these paths is
 * reachable from a public URL. Each answers within GATEWAY_TIMEOUT_MS or
 * counts as "could not ask".
 */

const INTROSPECT_URL = "https://internal/admin/introspect";
const DISCONNECT_URL = "https://internal/admin/disconnect-user";
const DISCONNECT_PARTICIPANT_URL = "https://internal/disconnect-participant";
const GATEWAY_TIMEOUT_MS = 3000;

type Namespace = Pick<DurableObjectNamespace, "get" | "idFromName">;

export interface UserSocketBindings {
  db: D1Database;
  meetingRooms: Namespace;
  voiceRooms: Namespace;
}

/** The part of the gateway's /admin/introspect answer the disconnect reads. */
interface VoiceRoster {
  voice_channels?: Array<{
    channel_id: string;
    members: Array<{ user_id: string }>;
  }>;
}

async function post(
  namespace: Namespace,
  name: string,
  url: string,
  body: unknown,
): Promise<Response | null> {
  try {
    const response = await namespace
      .get(namespace.idFromName(name))
      .fetch(url, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: AbortSignal.timeout(GATEWAY_TIMEOUT_MS),
      });
    return response.ok ? response : null;
  } catch {
    return null;
  }
}

/** The global gateway's /admin/introspect answer, or null. */
export async function introspectGateway<T>(
  meetingRooms: Namespace,
): Promise<T | null> {
  const response = await post(
    meetingRooms,
    "global-gateway",
    INTROSPECT_URL,
    {},
  );
  return response ? ((await response.json()) as T) : null;
}

/**
 * The room slug a voice channel's call runs in: `voice-<server>-<channel>`,
 * or for a DM call `dm-call-<a>-<b>` (its two members, sorted), as
 * socket-admission.ts issues them. Null when the channel is neither.
 */
export async function callRoomSlug(
  db: D1Database,
  channelId: string,
): Promise<string | null> {
  const channel = await db
    .prepare(`SELECT server_id FROM channels WHERE id = ?`)
    .bind(channelId)
    .first<{ server_id: string | null }>();
  if (!channel) return null;
  if (channel.server_id) {
    // The same slug as buildVoiceChannelRoomSlug (src/lib/api-helpers.ts).
    return `voice-${channel.server_id}-${channelId}`;
  }
  const { results } = await db
    .prepare(
      `SELECT user_id FROM dm_recipients WHERE channel_id = ? ORDER BY user_id ASC`,
    )
    .bind(channelId)
    .all<{ user_id: string }>();
  const members = (results ?? []).map((row) => row.user_id).filter(Boolean);
  return members.length === 2 ? `dm-call-${members[0]}-${members[1]}` : null;
}

/** Ends the user's sessions in one call room; returns sockets closed. */
async function leaveCallRoom(
  bindings: UserSocketBindings,
  slug: string,
  userId: string,
  reason: string,
): Promise<number> {
  const response = await post(bindings.meetingRooms, slug, DISCONNECT_URL, {
    user_id: userId,
    reason,
  });
  if (!response) return 0;
  const body = (await response.json()) as {
    closed?: number;
    participant_ids?: string[];
  };
  // The media side: the VoiceRoom keeps its own socket per session.
  await Promise.allSettled(
    (body.participant_ids ?? []).map((participantId) =>
      post(bindings.voiceRooms, slug, DISCONNECT_PARTICIPANT_URL, {
        participant_id: participantId,
        user_id: userId,
      }),
    ),
  );
  return typeof body.closed === "number" ? body.closed : 0;
}

/** Sockets closed, or null when the gateway could not be asked. */
export async function disconnectUserEverywhere(
  bindings: UserSocketBindings,
  userId: string,
  reason: string,
): Promise<number | null> {
  const { db, meetingRooms } = bindings;
  // Where they are in voice, read before the gateway forgets them.
  const before = await introspectGateway<VoiceRoster>(meetingRooms);
  const channels = (before?.voice_channels ?? [])
    .filter((room) => room.members.some((member) => member.user_id === userId))
    .map((room) => room.channel_id);

  const global = await post(meetingRooms, "global-gateway", DISCONNECT_URL, {
    user_id: userId,
    reason,
  });
  if (!global) return null;
  const globalBody = (await global.json()) as { closed?: number };
  let closed = typeof globalBody.closed === "number" ? globalBody.closed : 0;

  const slugs = (
    await Promise.all(
      channels.map((channelId) =>
        callRoomSlug(db, channelId).catch(() => null),
      ),
    )
  ).filter((slug): slug is string => !!slug);
  const roomClosed = await Promise.all(
    [...new Set(slugs)].map((slug) =>
      leaveCallRoom(bindings, slug, userId, reason),
    ),
  );
  closed += roomClosed.reduce((sum, count) => sum + count, 0);
  return closed;
}
