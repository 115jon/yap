import { beforeEach, describe, expect, it } from "vitest";

import {
  deletionDateFrom,
  invalidatePendingDeletionCache,
  pendingDeletionDate,
  pendingDeletionMessage,
  PENDING_DELETION_TTL_MS,
} from "@/lib/account-deletion";
import { confirmationMatches } from "@/services/account-deletion.service";

/** A D1 stand-in holding account_deletions rows; counts the set reads. */
function fakeDb(rows: Map<string, string>) {
  const reads = { set: 0 };
  const db = {
    prepare(sql: string) {
      return {
        async all() {
          reads.set += 1;
          return {
            results: [...rows.keys()].map((user_id) => ({ user_id })),
          };
        },
        bind(userId: string) {
          return {
            async first() {
              void sql;
              const deleteAfter = rows.get(userId);
              return deleteAfter ? { delete_after: deleteAfter } : null;
            },
          };
        },
      };
    },
  };
  return { db: db as unknown as Pick<D1Database, "prepare">, reads };
}

describe("pending deletion gate", () => {
  beforeEach(() => invalidatePendingDeletionCache());

  it("answers the date for a pending account and null otherwise", async () => {
    const { db } = fakeDb(new Map([["u1", "2026-12-13T00:00:00.000Z"]]));
    expect(await pendingDeletionDate("u1", db, 0)).toBe(
      "2026-12-13T00:00:00.000Z",
    );
    expect(await pendingDeletionDate("u2", db, 0)).toBeNull();
  });

  it("reads the set once per TTL", async () => {
    const { db, reads } = fakeDb(new Map());
    await pendingDeletionDate("u1", db, 0);
    await pendingDeletionDate("u1", db, PENDING_DELETION_TTL_MS - 1);
    expect(reads.set).toBe(1);
    await pendingDeletionDate("u1", db, PENDING_DELETION_TTL_MS);
    expect(reads.set).toBe(2);
  });

  it("lets a restore made elsewhere through before the TTL runs out", async () => {
    const rows = new Map([["u1", "2026-12-13T00:00:00.000Z"]]);
    const { db } = fakeDb(rows);
    expect(await pendingDeletionDate("u1", db, 0)).not.toBeNull();
    // Another isolate restored the account: the cached set still names it,
    // the confirming read does not.
    rows.delete("u1");
    expect(await pendingDeletionDate("u1", db, 1)).toBeNull();
  });

  it("fails open when the set cannot be read", async () => {
    const db = {
      prepare: () => ({
        all: () => Promise.reject(new Error("no such table")),
      }),
    } as unknown as Pick<D1Database, "prepare">;
    expect(await pendingDeletionDate("u1", db, 0)).toBeNull();
  });
});

describe("deletion dates and messages", () => {
  it("schedules 30 days out", () => {
    expect(deletionDateFrom(Date.UTC(2026, 10, 13))).toBe(
      "2026-12-13T00:00:00.000Z",
    );
  });

  it("names the day in the refusal", () => {
    expect(pendingDeletionMessage("2026-12-13T05:00:00.000Z")).toBe(
      "This account is scheduled for deletion on 2026-12-13. Restore it to use Yap again.",
    );
  });
});

describe("typed confirmation", () => {
  it("matches the username with or without @, in any case", () => {
    expect(confirmationMatches("alice", "alice")).toBe(true);
    expect(confirmationMatches(" @Alice ", "alice")).toBe(true);
    expect(confirmationMatches("alic", "alice")).toBe(false);
    expect(confirmationMatches("", "")).toBe(false);
  });
});
