import { beforeEach, describe, expect, it, vi } from "vitest";

// "Share my activity" (users.share_activity) through PATCH /api/update-profile:
// validated, written as 0/1, returned as a boolean, and told only to the
// user's own sessions and the gateway (never to their servers).

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  getDB: vi.fn(),
  broadcastToUser: vi.fn(),
  broadcastToUserServers: vi.fn(),
  cacheDel: vi.fn(),
  statements: [] as Array<{ sql: string; binds: unknown[] }>,
  shareActivity: 1 as number,
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (error: string, status = 400) =>
    Response.json({ error }, { status }),
  apiSuccess: (data: unknown) => Response.json(data),
  broadcastToUser: mocks.broadcastToUser,
  broadcastToUserServers: mocks.broadcastToUserServers,
  getDB: mocks.getDB,
  requireAuth: mocks.requireAuth,
}));
vi.mock("@/lib/cache", () => ({
  cacheDel: mocks.cacheDel,
  CacheKey: {
    userProfile: (id: string) => `profile:${id}`,
    userServers: (id: string) => `servers:${id}`,
    serverMembers: (id: string) => `members:${id}`,
  },
}));
vi.mock("@/lib/ensure-user-profile-schema", () => ({
  ensureUserProfileSchema: vi.fn(),
}));

import { PATCH } from "../update-profile";

function fakeDb() {
  return {
    prepare(sql: string) {
      return {
        bind(...binds: unknown[]) {
          mocks.statements.push({ sql, binds });
          if (sql.startsWith("UPDATE users SET")) {
            const index = sql.indexOf("share_activity = ?");
            if (index !== -1) mocks.shareActivity = binds[0] as number;
          }
          return {
            run: async () => ({ meta: { changes: 1 } }),
            first: async () =>
              sql.includes("FROM users WHERE id = ?")
                ? {
                    id: "user-1",
                    username: "player",
                    display_name: null,
                    share_activity: mocks.shareActivity,
                    media_content_filter: "high",
                    theme_sync_enabled: 0,
                    updated_at: "2026-11-08T00:00:00.000Z",
                  }
                : null,
            all: async () => ({ results: [{ server_id: "server-1" }] }),
          };
        },
      };
    },
  };
}

function patch(body: unknown) {
  return PATCH({
    request: new Request("https://yap.test/api/update-profile", {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  });
}

describe("PATCH /api/update-profile shareActivity", () => {
  beforeEach(() => {
    mocks.statements.length = 0;
    mocks.shareActivity = 1;
    mocks.broadcastToUser.mockReset();
    mocks.broadcastToUserServers.mockReset();
    mocks.requireAuth.mockResolvedValue({ userId: "user-1" });
    mocks.getDB.mockReturnValue(fakeDb());
  });

  it("turns sharing off, returns it, and tells only the user's sessions", async () => {
    const response = await patch({ shareActivity: false });
    expect(response.status).toBe(200);
    const body = (await response.json()) as { user: Record<string, unknown> };
    expect(body.user.share_activity).toBe(false);

    const update = mocks.statements.find((s) =>
      s.sql.startsWith("UPDATE users SET"),
    );
    expect(update?.sql).toContain("share_activity = ?");
    expect(update?.binds[0]).toBe(0);

    expect(mocks.broadcastToUser).toHaveBeenCalledWith(
      "user-1",
      "USER_PROFILE_UPDATE",
      expect.objectContaining({ user_id: "user-1", share_activity: false }),
    );
    // The server-wide profile broadcast never carries the private flag.
    for (const call of mocks.broadcastToUserServers.mock.calls) {
      expect(call[2]).not.toHaveProperty("share_activity");
    }
  });

  it("turns sharing back on", async () => {
    mocks.shareActivity = 0;
    const response = await patch({ shareActivity: true });
    const body = (await response.json()) as { user: Record<string, unknown> };
    expect(body.user.share_activity).toBe(true);
    expect(mocks.broadcastToUser).toHaveBeenCalledWith(
      "user-1",
      "USER_PROFILE_UPDATE",
      expect.objectContaining({ share_activity: true }),
    );
  });

  it("rejects a value that is not a boolean", async () => {
    const response = await patch({ shareActivity: "no" });
    expect(response.status).toBe(400);
    expect(mocks.statements).toEqual([]);
  });

  it("leaves the flag alone and says nothing when it is not sent", async () => {
    await patch({ bio: "hi" });
    const update = mocks.statements.find((s) =>
      s.sql.startsWith("UPDATE users SET"),
    );
    expect(update?.sql).not.toContain("share_activity");
    expect(
      mocks.broadcastToUser.mock.calls.some(
        (call) => "share_activity" in (call[2] as object),
      ),
    ).toBe(false);
  });
});
