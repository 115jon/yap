import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  env: {} as Record<string, unknown>,
  requireAuth: vi.fn(),
  syncCollectiblesCatalog: vi.fn(),
  getCollectiblesCatalog: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status = 400, code?: string) =>
    Response.json({ error: message, code }, { status }),
  apiSuccess: (data: unknown, status = 200) => Response.json(data, { status }),
  getDB: () => ({}),
  getEnv: () => mocks.env,
  requireAuth: mocks.requireAuth,
}));

vi.mock("@/lib/collectibles-catalog", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/lib/collectibles-catalog")>();
  return {
    ...actual,
    syncCollectiblesCatalog: mocks.syncCollectiblesCatalog,
    getCollectiblesCatalog: mocks.getCollectiblesCatalog,
  };
});

import { GET } from "../collectibles/catalog";

const catalog = { version: 1, items: [], categories: [] };

function get(query = "") {
  return GET({
    request: new Request(`https://meet.test/api/collectibles/catalog${query}`),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  // A catalog admin under the old gate: it must not unlock anything here.
  mocks.env = { COLLECTIBLES_ADMIN_USER_IDS: "admin-1" };
  mocks.requireAuth.mockResolvedValue({ userId: "admin-1" });
  mocks.getCollectiblesCatalog.mockResolvedValue(catalog);
  mocks.syncCollectiblesCatalog.mockResolvedValue(catalog);
});

describe("GET /api/collectibles/catalog", () => {
  it("passes through an auth failure", async () => {
    mocks.requireAuth.mockResolvedValue(
      Response.json({ error: "Unauthorized" }, { status: 401 }),
    );
    expect((await get()).status).toBe(401);
    expect(mocks.getCollectiblesCatalog).not.toHaveBeenCalled();
  });

  it("serves the cached catalog", async () => {
    const response = await get();
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({ catalog });
  });

  it("never forces a refresh, not even ?refresh=1 from a catalog admin", async () => {
    expect((await get("?refresh=1")).status).toBe(200);
    expect(mocks.getCollectiblesCatalog).toHaveBeenCalledTimes(1);
    const [, options] = mocks.getCollectiblesCatalog.mock.calls[0]!;
    expect(Object.keys(options ?? {})).toEqual(["waitUntil"]);
    expect(mocks.syncCollectiblesCatalog).not.toHaveBeenCalled();
  });

  it("returns 503 when there is no catalog at all", async () => {
    mocks.getCollectiblesCatalog.mockRejectedValue(new Error("empty"));
    expect((await get()).status).toBe(503);
  });
});
