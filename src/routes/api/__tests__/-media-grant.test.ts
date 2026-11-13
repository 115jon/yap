import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  secret: "ticket-secret" as string | null,
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status: number, code?: string) =>
    Response.json({ error: message, code }, { status }),
  apiSuccess: (data: unknown, status = 200) => Response.json(data, { status }),
  requireAuth: mocks.requireAuth,
}));

vi.mock("@/lib/media-auth", () => ({
  mediaCapSecret: () => mocks.secret,
}));

import {
  type MediaGrant,
  verifyMediaCap,
  withMediaCap,
} from "@/lib/media-capability";
import { POST } from "../media/grant";

beforeEach(() => {
  mocks.requireAuth.mockReset();
  mocks.secret = "ticket-secret";
});

describe("POST /api/media/grant", () => {
  it("issues a grant for the signed-in caller that signs verifiable caps", async () => {
    mocks.requireAuth.mockResolvedValue({ userId: "user-1" });

    const response = await POST({
      request: new Request("https://yap.test/api/media/grant", {
        method: "POST",
        headers: { authorization: "Bearer session" },
      }),
    });

    expect(response.status).toBe(200);
    expect(response.headers.get("Cache-Control")).toBe("no-store");
    const grant = (await response.json()) as MediaGrant & {
      issuedAt: unknown;
    };
    expect(grant).toMatchObject({ userId: "user-1" });
    expect(typeof grant.issuedAt).toBe("number");
    expect(JSON.stringify(grant)).not.toContain("session");
    const url = withMediaCap(
      grant,
      new URL("https://yap.test/api/attachments/c/a/f.png"),
    );
    expect(
      verifyMediaCap(
        url.searchParams.get("cap") ?? "",
        url,
        "ticket-secret",
        Date.now(),
      ),
    ).toMatchObject({ ok: true, userId: "user-1" });
  });

  it("answers the auth failure for a request without a session", async () => {
    mocks.requireAuth.mockResolvedValue(
      Response.json({ error: "Unauthorized" }, { status: 401 }),
    );

    const response = await POST({
      request: new Request("https://yap.test/api/media/grant?token=leaked", {
        method: "POST",
      }),
    });

    expect(response.status).toBe(401);
  });

  it("is unavailable without a secret", async () => {
    mocks.requireAuth.mockResolvedValue({ userId: "user-1" });
    mocks.secret = null;

    const response = await POST({
      request: new Request("https://yap.test/api/media/grant", {
        method: "POST",
      }),
    });

    expect(response.status).toBe(503);
  });
});
