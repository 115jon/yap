import { describe, expect, it } from "vitest";

import {
  blockedUserIds,
  blockErrorMessage,
  blockStateFromError,
  dmComposerBlock,
  hasBlocked,
} from "./blocks";
import type { Relationship } from "./types";

const relationship = (id: string, type: number): Relationship => ({
  user: { id, username: id } as Relationship["user"],
  type,
  created_at: "2026-11-14T00:00:00.000Z",
});

describe("blocks", () => {
  it("reads the block behind an API error from its code", () => {
    const error = (code: string) =>
      Object.assign(new Error("You can't message this user."), { code });
    expect(blockStateFromError(error("BLOCKED_BY_USER"))).toBe("blocked_you");
    expect(blockStateFromError(error("USER_BLOCKED"))).toBe("you_blocked");
    expect(blockStateFromError(error("FORBIDDEN"))).toBeNull();
    expect(blockStateFromError(null)).toBeNull();
    expect(blockErrorMessage(error("BLOCKED_BY_USER"))).toBe(
      "You can't message this user.",
    );
    expect(blockErrorMessage(error("FORBIDDEN"))).toBeNull();
  });

  it("knows only blocks (type 1) as blocked users", () => {
    const relationships = [
      relationship("friend", 0),
      relationship("blocked", 1),
      relationship("incoming", 2),
    ];
    expect([...blockedUserIds(relationships)]).toEqual(["blocked"]);
    expect(hasBlocked(relationships, "blocked")).toBe(true);
    expect(hasBlocked(relationships, "friend")).toBe(false);
    expect(hasBlocked(relationships, null)).toBe(false);
  });

  it("lets your own block win the DM composer, from the live relationships", () => {
    expect(dmComposerBlock(true, undefined)).toBe("you_blocked");
    expect(dmComposerBlock(true, "blocked_you")).toBe("you_blocked");
    expect(dmComposerBlock(false, "blocked_you")).toBe("blocked_you");
    // Unblocked elsewhere: a stale refusal of our own does not hold.
    expect(dmComposerBlock(false, "you_blocked")).toBeNull();
    expect(dmComposerBlock(false, undefined)).toBeNull();
  });
});
