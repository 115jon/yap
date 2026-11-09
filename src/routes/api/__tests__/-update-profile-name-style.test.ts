import { beforeEach, describe, expect, it, vi } from "vitest";

// Display name styles through PATCH /api/update-profile: the Yap effects,
// fonts and the Wave modifier are stored as normalized JSON for a plan with
// allNameStyles, refused (403) for one without, and unknown values are still
// refused. The plan is read from user_entitlements, never from the body.

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  getDB: vi.fn(),
  broadcastToUser: vi.fn(),
  broadcastToUserServers: vi.fn(),
  cacheDel: vi.fn(),
  statements: [] as Array<{ sql: string; binds: unknown[] }>,
  shareActivity: 1 as number,
  storedStyle: null as string | null,
  tier: "founder" as "free" | "loud" | "founder",
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
            const fields = sql
              .slice("UPDATE users SET ".length, sql.indexOf(" WHERE"))
              .split(", ");
            const at = fields.indexOf("display_name_style = ?");
            if (at !== -1) mocks.storedStyle = binds[at] as string | null;
          }
          return {
            run: async () => ({ meta: { changes: 1 } }),
            first: async () =>
              sql.includes("FROM user_entitlements")
                ? mocks.tier === "free"
                  ? null
                  : {
                      tier: mocks.tier,
                      source: "grant",
                      started_at: "2026-11-01T00:00:00.000Z",
                      expires_at: null,
                    }
                : sql.includes("FROM users WHERE id = ?")
                  ? {
                      id: "user-1",
                      username: "player",
                      display_name: null,
                      share_activity: mocks.shareActivity,
                      display_name_style: mocks.storedStyle,
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

describe("PATCH /api/update-profile displayNameStyle", () => {
  beforeEach(() => {
    mocks.statements.length = 0;
    mocks.storedStyle = null;
    mocks.tier = "founder";
    mocks.requireAuth.mockResolvedValue({ userId: "user-1" });
    mocks.getDB.mockReturnValue(fakeDb());
  });

  const style = {
    font: "vt323",
    effect: "crt",
    primaryColor: "#4ade80",
    secondaryColor: "#FB7185",
    wave: true,
  };

  it("accepts a Yap effect, font and Wave, and stores them normalized", async () => {
    const response = await patch({ displayNameStyle: style });
    expect(response.status).toBe(200);
    expect(JSON.parse(mocks.storedStyle ?? "null")).toEqual({
      ...style,
      primaryColor: "#4ADE80",
    });
    const body = (await response.json()) as { user: Record<string, unknown> };
    expect(body.user.display_name_style).toEqual({
      ...style,
      primaryColor: "#4ADE80",
    });
  });

  it("accepts every Yap effect", async () => {
    for (const effect of [
      "dither",
      "holo",
      "chrome",
      "pixel",
      "crt",
      "glitch",
      "ember",
      "sticker",
      "candlelit",
    ]) {
      const response = await patch({ displayNameStyle: { ...style, effect } });
      expect(response.status, effect).toBe(200);
    }
  });

  it("still accepts a Classic style and drops wave: false", async () => {
    const classic = {
      font: "tempo",
      effect: "neon",
      primaryColor: "#38BDF8",
      secondaryColor: "#C084FC",
    };
    const response = await patch({
      displayNameStyle: { ...classic, wave: false },
    });
    expect(response.status).toBe(200);
    expect(mocks.storedStyle).toBe(JSON.stringify(classic));
  });

  it("refuses an unknown effect", async () => {
    const response = await patch({
      displayNameStyle: { ...style, effect: "sparkle" },
    });
    expect(response.status).toBe(400);
    expect(mocks.statements).toEqual([]);
  });
  describe("without allNameStyles", () => {
    beforeEach(() => {
      mocks.tier = "free";
    });

    const classic = {
      font: "tempo",
      effect: "neon",
      primaryColor: "#38BDF8",
      secondaryColor: "#C084FC",
    };

    function storedUpdate() {
      return mocks.statements.find((s) => s.sql.startsWith("UPDATE users"));
    }

    it("refuses a Yap effect, a Yap font or Wave with 403", async () => {
      for (const displayNameStyle of [
        { ...classic, effect: "crt" },
        { ...classic, font: "vt323" },
        { ...classic, wave: true },
      ]) {
        const response = await patch({ displayNameStyle });
        expect(response.status, JSON.stringify(displayNameStyle)).toBe(403);
        expect(await response.json()).toEqual({
          error: "Yap name styles come with Yap Loud",
        });
      }
      expect(storedUpdate()).toBeUndefined();
    });

    it("still saves a Classic style", async () => {
      const response = await patch({ displayNameStyle: classic });
      expect(response.status).toBe(200);
      expect(mocks.storedStyle).toBe(JSON.stringify(classic));
    });

    it("lets a lapsed plan re-send the Yap style it already has", async () => {
      mocks.storedStyle = JSON.stringify({ ...style, primaryColor: "#4ADE80" });
      const response = await patch({ displayNameStyle: style, bio: "hi" });
      expect(response.status).toBe(200);
      expect(storedUpdate()?.binds).toContain("hi");
    });

    it("does not let a lapsed plan swap one Yap style for another", async () => {
      mocks.storedStyle = JSON.stringify({ ...style, primaryColor: "#4ADE80" });
      const response = await patch({
        displayNameStyle: { ...style, effect: "holo" },
      });
      expect(response.status).toBe(403);
      expect(storedUpdate()).toBeUndefined();
    });
  });

  it("accepts a Yap style on Yap Loud", async () => {
    mocks.tier = "loud";
    const response = await patch({ displayNameStyle: style });
    expect(response.status).toBe(200);
  });
});
