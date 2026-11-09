import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { env } from "cloudflare:workers";

const hoisted = vi.hoisted(() => ({
  fetchInstagramOEmbedMetadataMock: vi.fn(),
}));

vi.mock("@/lib/cache", () => ({
  cacheGet: vi.fn(async () => null),
  cacheSet: vi.fn(async () => {}),
}));

vi.mock("@/lib/share-preview-proxy", () => ({
  fetchInstagramOEmbedMetadata: hoisted.fetchInstagramOEmbedMetadataMock,
}));

import { resolveInstagramVideoMetadata } from "../instagram-video-resolver";

const workerEnv = env as unknown as Record<string, unknown>;
const REEL_URL = "https://www.instagram.com/reel/DfajF1zvsbl/";

function fixture(name: string): string {
  return readFileSync(
    new URL(`./fixtures/instagram/${name}`, import.meta.url),
    "utf8",
  );
}

function html(body: string): Response {
  return new Response(body, {
    headers: { "content-type": "text/html; charset=utf-8" },
  });
}

type Route = (url: string, init?: RequestInit) => Response | Promise<Response>;

function stubFetch(route: Route) {
  const calls: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      calls.push(url);
      return route(url, init);
    }),
  );
  return calls;
}

beforeEach(() => {
  hoisted.fetchInstagramOEmbedMetadataMock.mockResolvedValue({
    mediaId: "4024683550010689253_25025320",
  });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  delete workerEnv.INSTAGRAM_SESSIONID;
  delete workerEnv.INSTAGRAM_CSRFTOKEN;
});

describe("resolveInstagramVideoMetadata layers", () => {
  it("answers from the public embed page alone, even with a session set", async () => {
    workerEnv.INSTAGRAM_SESSIONID = "sessionid";
    workerEnv.INSTAGRAM_CSRFTOKEN = "csrftoken";
    const calls = stubFetch((url) =>
      url.includes("/embed/captioned/")
        ? html(fixture("embed-instagram-reel.html"))
        : new Response("unexpected", { status: 500 }),
    );

    const result = await resolveInstagramVideoMetadata(REEL_URL, {
      bypassCache: true,
    });

    expect(calls).toEqual([
      "https://www.instagram.com/p/DfajF1zvsbl/embed/captioned/",
    ]);
    expect(result?.videoUrl).toMatch(/\.mp4\?/);
    expect(result?.authorName).toBe("instagram");
    expect(result?.authorVerified).toBe(true);
    expect(hoisted.fetchInstagramOEmbedMetadataMock).not.toHaveBeenCalled();
  });

  it("fills a withheld MP4 from the session API and keeps the page's author", async () => {
    workerEnv.INSTAGRAM_SESSIONID = "sessionid";
    workerEnv.INSTAGRAM_CSRFTOKEN = "csrftoken";
    stubFetch((url) => {
      if (url.includes("/embed/captioned/")) {
        return html(fixture("embed-reel-copyright-blocked.html"));
      }
      if (url.startsWith("https://i.instagram.com/api/v1/media/")) {
        return Response.json({
          items: [
            {
              video_versions: [
                {
                  url: "https://scontent-ord5-1.cdninstagram.com/o1/v/session.mp4?oe=6B000000",
                  width: 720,
                  height: 1280,
                },
              ],
              image_versions2: {
                candidates: [
                  {
                    url: "https://scontent-ord5-1.cdninstagram.com/v/poster.jpg?oe=6B000000",
                    width: 720,
                    height: 1280,
                  },
                ],
              },
            },
          ],
        });
      }
      return new Response("unexpected", { status: 500 });
    });

    const result = await resolveInstagramVideoMetadata(REEL_URL, {
      bypassCache: true,
    });

    expect(result?.videoUrl).toContain("session.mp4");
    expect(result?.media?.[0]?.type).toBe("video");
    expect(result?.authorName).toBe("instagram");
    expect(result?.likeCount).toBeGreaterThan(0);
  });

  it("returns the embed page's card when the MP4 is withheld and no session is set", async () => {
    const calls = stubFetch((url) =>
      url.includes("/embed/captioned/")
        ? html(fixture("embed-reel-copyright-blocked.html"))
        : new Response("unexpected", { status: 500 }),
    );

    const result = await resolveInstagramVideoMetadata(REEL_URL, {
      bypassCache: true,
    });

    expect(calls).toHaveLength(1);
    expect(result?.videoUrl).toBeNull();
    expect(result?.thumbnailUrl).toContain("cdninstagram.com");
    expect(result?.authorName).toBe("instagram");
  });

  it("falls back to the post page's OpenGraph tags when the embed page is walled", async () => {
    const calls = stubFetch((url, init) => {
      if (url.includes("/embed/captioned/")) {
        return new Response(null, {
          status: 302,
          headers: { location: "https://www.instagram.com/accounts/login/" },
        });
      }
      if (url === REEL_URL) {
        expect(init?.redirect).toBe("manual");
        expect(new Headers(init?.headers).get("user-agent")).toContain(
          "Discordbot/2.0",
        );
        return html(fixture("post-page-og-reel.html"));
      }
      return new Response("unexpected", { status: 500 });
    });

    const result = await resolveInstagramVideoMetadata(REEL_URL, {
      bypassCache: true,
    });

    expect(calls).toHaveLength(2);
    expect(result?.authorName).toBe("instagram");
    expect(result?.title).toBe("Legend says you can still hear it echoing.");
    expect(result?.likeCount).toBe(808_000);
    expect(result?.commentCount).toBe(11_000);
    expect(result?.thumbnailUrl).toContain("cdninstagram.com");
  });

  it("throws when every layer fails, so the caller can ask oEmbed if the post is gone", async () => {
    stubFetch((url) => {
      if (url.includes("/embed/captioned/")) {
        throw new TypeError("network down");
      }
      return new Response("nope", {
        status: 404,
        headers: { "content-type": "text/html" },
      });
    });

    await expect(
      resolveInstagramVideoMetadata(REEL_URL, { bypassCache: true }),
    ).rejects.toThrow("network down");
  });
});
