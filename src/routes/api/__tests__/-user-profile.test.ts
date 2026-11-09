import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  getMe: vi.fn(),
  getUserProfileMutuals: vi.fn(),
  hasBlocked: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status = 400, code?: string) =>
    Response.json({ error: message, code }, { status }),
  apiSuccess: (data: unknown, status = 200) => Response.json(data, { status }),
  getDB: () => ({}),
  requireAuth: mocks.requireAuth,
}));

vi.mock("@/services/user.service", () => ({
  getMe: mocks.getMe,
  getUserProfileMutuals: mocks.getUserProfileMutuals,
}));

vi.mock("@/services/connection.service", () => ({
  hasBlocked: mocks.hasBlocked,
}));

import { Route } from "../users/$id/profile";

const GET = (
  Route.options as unknown as {
    server: { handlers: { GET: (ctx: unknown) => Promise<Response> } };
  }
).server.handlers.GET;

const entitlement = {
  tier: "loud",
  source: "stripe",
  started_at: "2026-10-01T00:00:00.000Z",
  expires_at: "2099-01-01T00:00:00.000Z",
};
const badges = [{ id: "loud", awarded_at: "2026-10-01T00:00:00.000Z" }];
const connections = [
  {
    provider: "steam",
    display_name: "Gordon",
    profile_url: "https://steamcommunity.com/profiles/76561197960287930",
    avatar_url: null,
  },
];

async function profile(viewer: string, target: string) {
  mocks.requireAuth.mockResolvedValue({ userId: viewer });
  const response = await GET({
    request: new Request(`https://yap.test/api/users/${target}/profile`),
    params: { id: target },
  });
  return (await response.json()) as {
    user: { entitlement: unknown; badges: unknown; connections?: unknown };
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.getMe.mockImplementation(async (_db: unknown, id: string) => ({
    id,
    username: id,
    entitlement,
    badges,
    connections,
  }));
  mocks.getUserProfileMutuals.mockResolvedValue({});
  mocks.hasBlocked.mockResolvedValue(false);
});

describe("GET /api/users/:id/profile", () => {
  it("shows other people only the tier, with the badges", async () => {
    const { user } = await profile("viewer", "target");
    expect(user.entitlement).toEqual({ tier: "loud" });
    expect(user.badges).toEqual(badges);
  });

  it("shows a lapsed plan to others as free", async () => {
    mocks.getMe.mockResolvedValue({
      id: "target",
      entitlement: { ...entitlement, expires_at: "2020-01-01T00:00:00.000Z" },
      badges: [],
    });
    const { user } = await profile("viewer", "target");
    expect(user.entitlement).toEqual({ tier: "free" });
  });

  it("keeps the whole plan for the user themselves", async () => {
    const { user } = await profile("target", "target");
    expect(user.entitlement).toEqual(entitlement);
  });

  it("shows the shown connections, and none to someone the user blocked", async () => {
    expect((await profile("viewer", "target")).user.connections).toEqual(
      connections,
    );
    expect(mocks.hasBlocked).toHaveBeenCalledWith({}, "target", "viewer");
    mocks.hasBlocked.mockResolvedValue(true);
    expect((await profile("viewer", "target")).user.connections).toEqual([]);
  });
});
