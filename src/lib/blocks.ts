// Blocks on the client, Discord-style (the server enforces them; see
// src/services/block.service.ts):
//
// - a 1:1 DM with someone you blocked shows a read-only composer with
//   Unblock; one with someone who blocked you shows "You can't message this
//   user" once the server refuses a send, reaction or ring;
// - messages from someone you blocked are collapsed to "Blocked message"
//   with Show, in servers and DMs;
// - their typing is not shown, and their channel activity raises no toast.

import type { Relationship } from "@/lib/types";

/** `you_blocked`: you blocked them; `blocked_you`: they blocked you. */
export type BlockState = "you_blocked" | "blocked_you";

/** relationships.type for a block of `user` by the signed-in user. */
export const BLOCKED_RELATIONSHIP = 1;

/** The API's error codes for a refusal caused by a block. */
const BLOCK_CODES = {
  USER_BLOCKED: "you_blocked",
  BLOCKED_BY_USER: "blocked_you",
} as const satisfies Record<string, BlockState>;

/** The DM composer's notice for each side of a block. */
export const DM_BLOCK_COPY: Record<BlockState, string> = {
  you_blocked: "You blocked this user. Unblock to message.",
  blocked_you: "You can't message this user.",
};

/** The notice for a call the server refused over a block. */
export const CALL_BLOCKED_COPY = "You can't call this user.";

/** The block behind an API error (its `code`), or null. */
export function blockStateFromError(error: unknown): BlockState | null {
  const code = (error as { code?: unknown } | null)?.code;
  if (code === "USER_BLOCKED" || code === "BLOCKED_BY_USER") {
    return BLOCK_CODES[code];
  }
  return null;
}

/** The server's own sentence for a block refusal (user-facing), or null. */
export function blockErrorMessage(error: unknown): string | null {
  if (!blockStateFromError(error)) return null;
  const message = (error as { message?: unknown } | null)?.message;
  return typeof message === "string" && message.trim() ? message : null;
}

/** The ids of the users the signed-in user blocked. */
export function blockedUserIds(
  relationships: readonly Relationship[],
): ReadonlySet<string> {
  const ids = new Set<string>();
  for (const relationship of relationships) {
    if (relationship.type === BLOCKED_RELATIONSHIP) {
      ids.add(relationship.user.id);
    }
  }
  return ids;
}

/** Whether the signed-in user blocked `userId`. */
export function hasBlocked(
  relationships: readonly Relationship[],
  userId: string | null | undefined,
): boolean {
  return (
    !!userId &&
    relationships.some(
      (relationship) =>
        relationship.user.id === userId &&
        relationship.type === BLOCKED_RELATIONSHIP,
    )
  );
}

/**
 * The block that closes a DM's composer: your own block (from the
 * relationships, so it follows unblocking anywhere) wins; otherwise the
 * last refusal the server gave in this DM.
 */
export function dmComposerBlock(
  youBlocked: boolean,
  refusal: BlockState | undefined,
): BlockState | null {
  if (youBlocked) return "you_blocked";
  return refusal === "blocked_you" ? "blocked_you" : null;
}
