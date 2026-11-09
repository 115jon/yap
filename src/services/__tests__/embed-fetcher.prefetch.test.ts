import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const hoisted = vi.hoisted(() => ({
  store: new Map<string, unknown>(),
  writes: [] as string[],
  resolveInstagramMock: vi.fn(),
  oembedMock: vi.fn(),
}));

vi.mock("@/lib/cache", () => ({
  cacheGet: vi.fn(async (key: string) => hoisted.store.get(key) ?? null),
  cacheGetMany: vi.fn(async (keys: string[]) => {
    const found = new Map<string, unknown>();
    for (const key of keys) {
      if (hoisted.store.has(key)) found.set(key, hoisted.store.get(key));
    }
    return found;
  }),
  cacheSet: vi.fn(async (key: string, value: unknown) => {
    hoisted.writes.push(key);
    hoisted.store.set(key, JSON.parse(JSON.stringify(value)));
  }),
}));

vi.mock("@/lib/instagram-video-resolver", () => ({
  resolveInstagramVideoMetadata: hoisted.resolveInstagramMock,
}));

vi.mock("@/lib/share-preview-proxy", () => ({
  lookupInstagramOEmbed: hoisted.oembedMock,
  fetchTikTokProxyLookup: vi.fn(),
}));

import {
  embedCacheKey,
  extractAndProcessEmbedsWithStatus,
  lookupCachedEmbeds,
  prefetchEmbedUrls,
  type EmbedCacheEntry,
} from "../embed-fetcher";
import type { EmbedInfo } from "@/lib/types";

const ARTICLE = "https://example.com/story";
const REEL = "https://www.instagram.com/reel/DfajF1zvsbl/";

function html(body: string): Response {
  return new Response(body, {
    headers: { "content-type": "text/html; charset=utf-8" },
  });
}

function stubFetch(
  route: (url: string, init?: RequestInit) => Response | Promise<Response>,
) {
  const mock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) =>
    route(String(input), init),
  );
  vi.stubGlobal("fetch", mock);
  return mock;
}

function cached(url: string, embed: Partial<EmbedInfo>, freshFor = 60_000) {
  const entry: EmbedCacheEntry = {
    v: 2,
    status: "ok",
    embed: { id: "e", url, fields: [], ...embed } as EmbedInfo,
    freshUntil: Date.now() + freshFor,
  };
  hoisted.store.set(embedCacheKey(url), entry);
}

beforeEach(() => {
  hoisted.store.clear();
  hoisted.writes.length = 0;
  hoisted.oembedMock.mockResolvedValue({ status: 200, metadata: null });
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

describe("prefetchEmbedUrls (composer cache warm)", () => {
  it("resolves and caches a link before it is sent", async () => {
    stubFetch(() =>
      html('<head><meta property="og:title" content="Story"></head>'),
    );

    const embeds = await prefetchEmbedUrls([ARTICLE]);

    expect(embeds).toEqual([
      expect.objectContaining({ url: ARTICLE, rawTitle: "Story" }),
    ]);
    expect(hoisted.writes).toEqual([embedCacheKey(ARTICLE)]);
    // The send path now finds it.
    expect(await lookupCachedEmbeds(`look ${ARTICLE}`)).toEqual([
      expect.objectContaining({ rawTitle: "Story" }),
    ]);
  });

  it("does not spend a KV write on a URL with no embed", async () => {
    stubFetch(() => html("<head></head>"));

    expect(await prefetchEmbedUrls(["https://example.com/half-typ"])).toEqual(
      [],
    );
    expect(hoisted.writes).toEqual([]);
  });

  it("never fetches a private or loopback address", async () => {
    const fetchMock = stubFetch(() => html("<head><title>x</title></head>"));

    expect(
      await prefetchEmbedUrls([
        "http://127.0.0.1/admin",
        "http://169.254.169.254/latest/meta-data/",
        "http://localhost/",
      ]),
    ).toEqual([]);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("stops at a redirect into private space", async () => {
    const fetchMock = stubFetch((url) =>
      url === "https://short.example/x"
        ? new Response(null, {
            status: 302,
            headers: { location: "http://10.0.0.5/" },
          })
        : html("<head><title>internal</title></head>"),
    );

    expect(await prefetchEmbedUrls(["https://short.example/x"])).toEqual([]);
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it("sends no cookie or auth header upstream", async () => {
    const fetchMock = stubFetch(() =>
      html('<head><meta property="og:title" content="Story"></head>'),
    );
    await prefetchEmbedUrls([ARTICLE]);
    const headers = new Headers(fetchMock.mock.calls[0]?.[1]?.headers);
    expect(headers.get("cookie")).toBeNull();
    expect(headers.get("authorization")).toBeNull();
  });
});

describe("lookupCachedEmbeds (MESSAGE_CREATE)", () => {
  it("returns fresh cached embeds in message order and skips the rest", async () => {
    cached(REEL, { rawTitle: "reel" });
    cached(ARTICLE, { rawTitle: "stale" }, -1);

    const embeds = await lookupCachedEmbeds(
      `${ARTICLE} and ${REEL} and https://example.org/new`,
    );

    expect(embeds.map((embed) => embed.rawTitle)).toEqual(["reel"]);
    expect(embeds[0]?.url).toBe(REEL);
  });

  it("gives up after its budget instead of holding the send", async () => {
    const { cacheGetMany } = await import("@/lib/cache");
    vi.mocked(cacheGetMany).mockImplementationOnce(() => new Promise(() => {}));
    const start = Date.now();
    expect(await lookupCachedEmbeds(ARTICLE, 20)).toEqual([]);
    expect(Date.now() - start).toBeLessThan(500);
  });

  it("does nothing for a message without links", async () => {
    const { cacheGetMany } = await import("@/lib/cache");
    expect(await lookupCachedEmbeds("no links here")).toEqual([]);
    expect(cacheGetMany).not.toHaveBeenCalled();
  });
});

describe("Instagram link embeds", () => {
  it("asks oEmbed and the media resolver at the same time", async () => {
    let resolveOEmbed: (value: unknown) => void = () => {};
    hoisted.oembedMock.mockReturnValue(
      new Promise((resolve) => {
        resolveOEmbed = resolve;
      }),
    );
    hoisted.resolveInstagramMock.mockResolvedValue({
      videoUrl: "https://scontent.cdninstagram.com/v.mp4",
      thumbnailUrl: "https://scontent.cdninstagram.com/p.jpg",
      title: "Legend says",
      durationSeconds: 20,
      media: [
        {
          type: "video",
          url: "https://scontent.cdninstagram.com/v.mp4",
          width: 1096,
          height: 1950,
        },
      ],
      authorName: "instagram",
      authorUrl: "https://www.instagram.com/instagram",
      authorAvatarUrl: "https://scontent.cdninstagram.com/a.jpg",
      authorVerified: true,
      likeCount: 807085,
    });

    const pending = extractAndProcessEmbedsWithStatus(REEL);
    await vi.waitFor(() =>
      expect(hoisted.resolveInstagramMock).toHaveBeenCalled(),
    );
    resolveOEmbed({ status: 200, metadata: null });
    const { embeds } = await pending;

    expect(embeds[0]).toMatchObject({
      provider: { name: "Instagram" },
      rawTitle: "Legend says",
      author: {
        name: "instagram",
        iconURL: "https://scontent.cdninstagram.com/a.jpg",
        isVerified: true,
      },
      video: {
        url: "https://scontent.cdninstagram.com/v.mp4",
        kind: "direct",
        width: 1096,
        height: 1950,
      },
      metrics: { likes: 807085 },
    });
  });

  it("falls back to a minimal card when Instagram shows nothing", async () => {
    hoisted.resolveInstagramMock.mockRejectedValue(
      new Error("Instagram metadata could not be resolved"),
    );
    hoisted.oembedMock.mockResolvedValue({ status: 403, metadata: null });

    const { embeds, hadFailures } =
      await extractAndProcessEmbedsWithStatus(REEL);

    expect(hadFailures).toBe(false);
    expect(embeds).toEqual([
      expect.objectContaining({
        url: REEL,
        type: "rich",
        provider: { name: "Instagram", url: "https://www.instagram.com" },
      }),
    ]);
    expect(embeds[0]?.rawTitle).toBeUndefined();
  });

  it("tombstones a post oEmbed says is gone", async () => {
    hoisted.resolveInstagramMock.mockRejectedValue(new Error("empty"));
    hoisted.oembedMock.mockResolvedValue({ status: 404, metadata: null });

    const { embeds } = await extractAndProcessEmbedsWithStatus(REEL);

    expect(embeds[0]).toMatchObject({ unavailable: true });
  });

  it("retries later when Instagram is rate limiting", async () => {
    hoisted.resolveInstagramMock.mockRejectedValue(
      new TypeError("fetch failed"),
    );
    hoisted.oembedMock.mockResolvedValue({ status: 429, metadata: null });

    const { embeds, hadFailures } =
      await extractAndProcessEmbedsWithStatus(REEL);

    expect(hadFailures).toBe(true);
    expect(embeds).toEqual([]);
  });
});

describe("OpenGraph previews", () => {
  it("reads only the head of the page", async () => {
    let cancelled = false;
    const encoder = new TextEncoder();
    stubFetch(
      () =>
        new Response(
          new ReadableStream<Uint8Array>({
            start(controller) {
              controller.enqueue(
                encoder.encode(
                  '<html><head><meta property="og:title" content="Big page"></head><body>',
                ),
              );
            },
            pull(controller) {
              controller.enqueue(encoder.encode("x".repeat(64 * 1024)));
            },
            cancel() {
              cancelled = true;
            },
          }),
          { headers: { "content-type": "text/html" } },
        ),
    );

    const { embeds } = await extractAndProcessEmbedsWithStatus(ARTICLE);

    expect(embeds[0]?.rawTitle).toBe("Big page");
    expect(cancelled).toBe(true);
  });
});
