/**
 * "Who can DM you" (users.dm_permission, migration 049), enforced wherever
 * someone can reach a user in a 1:1 DM (src/lib/dm-permission.ts has the
 * values and copy):
 *
 * - a new DM (POST /api/dms) needs the recipient's setting to allow the
 *   sender;
 * - in an existing DM, a message, reaction, typing or ring needs the same,
 *   unless the recipient has written in that DM (a message, or a call they
 *   placed, which is stored as their message): a conversation they took part
 *   in stays open under every setting;
 * - everyone: anyone; friends_and_servers: friends (either row of the
 *   friendship) and anyone in a server with them; nobody: no one;
 * - a block either way is checked first and wins (block.service.ts); a
 *   server admin or owner is treated like anyone else.
 *
 * The setting only limits who reaches its owner: it never stops the owner
 * from starting a DM with someone else.
 */

import {
  DM_NOT_ACCEPTING_CODE,
  dmNotAcceptingCall,
  dmNotAcceptingMessage,
  dmPermissionOf,
  type DmPermission,
} from "@/lib/dm-permission";
import { ServiceError } from "@/lib/service-error";
import type { D1Database } from "@cloudflare/workers-types";
import {
  BLOCK_MESSAGE_COPY,
  blockRefusal,
  dmBlockState,
  type BlockState,
} from "./block.service";

/** The recipient a refusal is about, as it names them. */
export interface DmRecipient {
  id: string;
  name: string | null;
}

/** Whether a friendship or a shared server joins the two users. */
async function friendsOrShareServer(
  db: D1Database,
  ownerId: string,
  actorId: string,
): Promise<boolean> {
  const row = await db
    .prepare(
      `SELECT
         EXISTS (
           SELECT 1 FROM relationships
            WHERE type = 0
              AND ((user_id = ?1 AND target_user_id = ?2)
                OR (user_id = ?2 AND target_user_id = ?1))
         ) AS friends,
         EXISTS (
           SELECT 1 FROM server_members mine
             JOIN server_members theirs ON theirs.server_id = mine.server_id
            WHERE mine.user_id = ?1 AND theirs.user_id = ?2
         ) AS shared`,
    )
    .bind(ownerId, actorId)
    .first<{ friends: number; shared: number }>();
  return Number(row?.friends ?? 0) === 1 || Number(row?.shared ?? 0) === 1;
}

/** Whether `permission` (the owner's) lets `actorId` start a DM with them. */
export async function dmPermissionAllows(
  db: D1Database,
  ownerId: string,
  actorId: string,
  permission: DmPermission,
): Promise<boolean> {
  if (ownerId === actorId) return true;
  switch (permission) {
    case "everyone":
      return true;
    case "nobody":
      return false;
    case "friends_and_servers":
      return friendsOrShareServer(db, ownerId, actorId);
  }
}

/** The refusal for the recipient's setting (403 NOT_ACCEPTING_DMS). */
export function dmNotAcceptingRefusal(
  recipient: DmRecipient | null,
  kind: "message" | "call" = "message",
): ServiceError {
  const name = recipient?.name ?? null;
  return ServiceError.forbidden(
    kind === "call" ? dmNotAcceptingCall(name) : dmNotAcceptingMessage(name),
    DM_NOT_ACCEPTING_CODE,
  );
}

/**
 * The recipient whose setting refuses `actorId` a new DM with them, or null
 * when it allows one. `target` is the users row the caller already read.
 */
export async function newDmRefusedBy(
  db: D1Database,
  actorId: string,
  target: {
    id: string;
    username?: unknown;
    display_name?: unknown;
    dm_permission?: unknown;
  },
): Promise<DmRecipient | null> {
  const permission = dmPermissionOf(target.dm_permission);
  if (await dmPermissionAllows(db, target.id, actorId, permission)) return null;
  return { id: target.id, name: displayName(target) };
}

function displayName(row: {
  username?: unknown;
  display_name?: unknown;
}): string | null {
  if (typeof row.display_name === "string" && row.display_name.trim()) {
    return row.display_name;
  }
  return typeof row.username === "string" ? row.username : null;
}

/**
 * The other recipient of a 1:1 DM when their setting refuses `actorId`
 * here: they have not written in it and their setting does not allow the
 * actor. Null when allowed, or when the channel is not a 1:1 DM.
 */
export async function dmPermissionRefusedBy(
  db: D1Database,
  actorId: string,
  channelId: string,
): Promise<DmRecipient | null> {
  const other = await db
    .prepare(
      `SELECT u.id, u.username, u.display_name, u.dm_permission,
              -- Only looked up when the setting can refuse: the default
              -- (everyone) costs this one row read and nothing else.
              CASE WHEN COALESCE(u.dm_permission, 'everyone') = 'everyone'
                THEN 0
                ELSE EXISTS (
                  SELECT 1 FROM messages m
                   WHERE m.channel_id = other.channel_id
                     AND m.author_id = other.user_id
                )
              END AS wrote_here
         FROM dm_recipients other
         JOIN channels c ON c.id = other.channel_id AND c.channel_type = 'dm'
         JOIN users u ON u.id = other.user_id
        WHERE other.channel_id = ?2 AND other.user_id != ?1
        LIMIT 1`,
    )
    .bind(actorId, channelId)
    .first<{
      id: string;
      username: string | null;
      display_name: string | null;
      dm_permission: string | null;
      wrote_here: number;
    }>();
  if (!other) return null;
  const permission = dmPermissionOf(other.dm_permission);
  if (permission === "everyone" || Number(other.wrote_here) === 1) return null;
  if (await dmPermissionAllows(db, other.id, actorId, permission)) return null;
  return { id: other.id, name: displayName(other) };
}

/** Why `actorId` may not reach the other recipient of a 1:1 DM. */
export type DmContactRefusal =
  | { kind: "block"; block: BlockState }
  | { kind: "not_accepting"; recipient: DmRecipient };

/**
 * The block (first: it wins) or the recipient's setting that closes this
 * 1:1 DM to `actorId`'s messages, reactions, typing and rings; null when
 * neither does (or the channel is not a 1:1 DM).
 */
export async function dmContactRefusal(
  db: D1Database,
  actorId: string,
  channelId: string,
): Promise<DmContactRefusal | null> {
  const block = await dmBlockState(db, actorId, channelId);
  if (block) return { kind: "block", block };
  const recipient = await dmPermissionRefusedBy(db, actorId, channelId);
  return recipient ? { kind: "not_accepting", recipient } : null;
}

/** The API refusal (403 with a code) for a contact refusal. */
export function contactRefusalError(
  refusal: DmContactRefusal,
  copy: Record<BlockState, string> = BLOCK_MESSAGE_COPY,
  kind: "message" | "call" = "message",
): ServiceError {
  return refusal.kind === "block"
    ? blockRefusal(refusal.block, copy)
    : dmNotAcceptingRefusal(refusal.recipient, kind);
}
