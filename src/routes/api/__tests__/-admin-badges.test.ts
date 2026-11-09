import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  env: {} as Record<string, unknown>,
  requireAuth: vi.fn(),
  setManualBadge: vi.fn(),
  cacheDel: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status = 400, code?: string) =>
    Response.json({ error: message, code }, { status }),
  apiSuccess: (data: unknown, status = 200) => Response.json(data, { status }),
  getDB: () => ({}),
  getEnv: () => mocks.env,
  requireAuth: mocks.requireAuth,
}));

vi.mock("@/lib/cache", () => ({
  CacheKey: { userProfile: (id: string) => `profile:${id}` },
  cacheDel: mocks.cacheDel,
}));

vi.mock("@/services/badge.service", () => ({
  setManualBadge: mocks.setManualBadge,
}));

import { POST } from "../admin/badges";
import { GET as BILLING_STATUS } from "../billing/status";
import { POST as BILLING_CHECKOUT } from "../billing/checkout";
import { POST as BILLING_WEBHOOK } from "../billing/webhook";

function post(body: unknown) {
  return POST({
    request: new Request("https://yap.test/api/admin/badges", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    }),
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  mocks.env = { COLLECTIBLES_ADMIN_USER_IDS: "admin-1, admin-2" };
  mocks.requireAuth.mockResolvedValue({ userId: "admin-1" });
  mocks.setManualBadge.mockResolvedValue({ ok: true, changed: true });
});

describe("POST /api/admin/badges", () => {
  it("passes through an auth failure", async () => {
    mocks.requireAuth.mockResolvedValue(new Response(null, { status: 401 }));
    expect((await post({ user_id: "u", badge_id: "bug-hunter" })).status).toBe(
      401,
    );
    expect(mocks.setManualBadge).not.toHaveBeenCalled();
  });

  it("forbids users who are not admins, before reading the body", async () => {
    mocks.requireAuth.mockResolvedValue({ userId: "someone" });
    const response = await post("not even an object");
    expect(response.status).toBe(403);
    await expect(response.json()).resolves.toMatchObject({
      code: "BADGES_FORBIDDEN",
    });
    expect(mocks.setManualBadge).not.toHaveBeenCalled();
  });

  it("forbids everyone when no admin list is configured", async () => {
    mocks.env = {};
    expect((await post({ user_id: "u", badge_id: "bug-hunter" })).status).toBe(
      403,
    );
  });

  it("rejects a malformed body", async () => {
    expect((await post({ user_id: "u" })).status).toBe(400);
    expect(
      (await post({ user_id: "u", badge_id: "bug-hunter", action: "nuke" }))
        .status,
    ).toBe(400);
    expect(mocks.setManualBadge).not.toHaveBeenCalled();
  });

  it("awards for an admin and clears the cached profile", async () => {
    const response = await post({ user_id: "u-1", badge_id: "bug-hunter" });
    expect(response.status).toBe(200);
    await expect(response.json()).resolves.toEqual({
      user_id: "u-1",
      badge_id: "bug-hunter",
      action: "award",
      changed: true,
    });
    expect(mocks.setManualBadge).toHaveBeenCalledWith(
      {},
      expect.objectContaining({
        actorId: "admin-1",
        targetUserId: "u-1",
        badgeId: "bug-hunter",
        action: "award",
      }),
    );
    expect(mocks.cacheDel).toHaveBeenCalledWith("profile:u-1");
  });

  it("passes on the service's refusal", async () => {
    mocks.setManualBadge.mockResolvedValue({
      ok: false,
      status: 400,
      code: "BADGE_NOT_MANUAL",
      message: "That badge is awarded automatically, not by hand.",
    });
    const response = await post({
      user_id: "u-1",
      badge_id: "voice-regular",
      action: "revoke",
    });
    expect(response.status).toBe(400);
    await expect(response.json()).resolves.toMatchObject({
      code: "BADGE_NOT_MANUAL",
    });
  });
});

describe("/api/billing/* while Stripe is not set up", () => {
  const request = (path: string, method = "POST") =>
    new Request(`https://yap.test/api/billing/${path}`, { method });

  it("answers 501 BILLING_DISABLED", async () => {
    for (const response of [
      await BILLING_STATUS({ request: request("status", "GET") }),
      await BILLING_CHECKOUT({ request: request("checkout") }),
      await BILLING_WEBHOOK({ request: request("webhook") }),
    ]) {
      expect(response.status).toBe(501);
      await expect(response.json()).resolves.toMatchObject({
        code: "BILLING_DISABLED",
      });
    }
  });

  it("stays 501 (not built) once the secrets exist", async () => {
    mocks.env = {
      STRIPE_SECRET_KEY: "sk_test_placeholder",
      STRIPE_WEBHOOK_SECRET: "whsec_placeholder",
      STRIPE_PRICE_LOUD: "price_placeholder",
    };
    const status = await BILLING_STATUS({ request: request("status", "GET") });
    expect(status.status).toBe(200);
    const checkout = await BILLING_CHECKOUT({ request: request("checkout") });
    expect(checkout.status).toBe(501);
    await expect(checkout.json()).resolves.toMatchObject({
      code: "BILLING_NOT_IMPLEMENTED",
    });
  });
});
