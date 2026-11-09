import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  requireAuth: vi.fn(),
  prefetchEmbedUrls: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status: number, code?: string) =>
    Response.json({ error: message, code }, { status }),
  apiSuccess: (data: unknown, status = 200) => Response.json(data, { status }),
  requireAuth: mocks.requireAuth,
}));

vi.mock("@/services/embed-fetcher", () => ({
  prefetchEmbedUrls: mocks.prefetchEmbedUrls,
}));

import { POST } from "../embeds/prefetch";

function post(body: unknown) {
  return POST({
    request: new Request("https://meet.test/api/embeds/prefetch", {
      method: "POST",
      headers: { "content-type": "application/json", cookie: "session=1" },
      body: JSON.stringify(body),
    }),
  });
}

let user = 0;
beforeEach(() => {
  mocks.prefetchEmbedUrls.mockReset();
  mocks.prefetchEmbedUrls.mockResolvedValue([]);
  // A fresh user per test so the per-user rate limit starts clean.
  user += 1;
  mocks.requireAuth.mockResolvedValue({ userId: `user_${user}` });
});

describe("POST /api/embeds/prefetch", () => {
  it("needs a signed-in user", async () => {
    mocks.requireAuth.mockResolvedValue(
      Response.json({ error: "Unauthorized" }, { status: 401 }),
    );
    const response = await post({ urls: ["https://example.com/a"] });
    expect(response.status).toBe(401);
    expect(mocks.prefetchEmbedUrls).not.toHaveBeenCalled();
  });

  it("resolves the URLs the send path would embed, normalised the same way", async () => {
    mocks.prefetchEmbedUrls.mockResolvedValue([{ id: "e", url: "x" }]);
    const response = await post({
      urls: ["(https://www.instagram.com/reel/DfajF1zvsbl/)."],
    });
    expect(response.status).toBe(200);
    expect(mocks.prefetchEmbedUrls).toHaveBeenCalledWith([
      "https://www.instagram.com/reel/DfajF1zvsbl/",
    ]);
    expect(await response.json()).toEqual({ resolved: 1 });
  });

  it("drops private and loopback targets before anything is fetched", async () => {
    const response = await post({
      urls: ["http://127.0.0.1/", "http://169.254.169.254/latest/"],
    });
    expect(response.status).toBe(400);
    expect(mocks.prefetchEmbedUrls).not.toHaveBeenCalled();
  });

  it("takes at most as many URLs as a message embeds", async () => {
    const response = await post({
      urls: [
        "https://a.example/1",
        "https://a.example/2",
        "https://a.example/3",
        "https://a.example/4",
      ],
    });
    expect(response.status).toBe(400);
  });

  it("rate limits each user", async () => {
    let last: Response | null = null;
    for (let i = 0; i < 31; i += 1) {
      last = await post({ urls: [`https://example.com/${i}`] });
    }
    expect(last?.status).toBe(429);
    expect(mocks.prefetchEmbedUrls).toHaveBeenCalledTimes(30);
  });
});
