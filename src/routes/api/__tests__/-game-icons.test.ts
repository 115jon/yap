import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  getBucket: vi.fn(),
}));

vi.mock("@/lib/api-helpers", () => ({
  apiError: (message: string, status: number) =>
    Response.json({ error: message }, { status }),
  getBucket: mocks.getBucket,
  getEnv: () => ({}),
}));

import { GET, resetGameIconCaches } from "../game-icons/$";

const APP_ID = "356875221078245376";
const HASH = "a60bb76ba4d4acafbd4cb9aad6e61739";
const SPLAT = `${APP_ID}/${HASH}.png`;
const KEY = `game-icons/${APP_ID}/${HASH}.png`;
const UPSTREAM_URL = `https://cdn.discordapp.com/app-icons/${APP_ID}/${HASH}.png?size=64`;
const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1, 2, 3]);

const CATALOG_URL = "https://yap.test/desktop/game-catalog.json";
const OTHER_APP = "356875762940379136";

function call(splat: string) {
  return GET({
    request: new Request(`https://yap.test/api/game-icons/${splat}`),
    params: { _splat: splat },
  });
}

function catalogResponse() {
  return Response.json({
    apps: [
      { id: APP_ID, icon_hash: HASH },
      { id: OTHER_APP, name: "No icon" },
    ],
  });
}

function upstreamResponse(
  body: BodyInit | null,
  init: { status?: number; contentType?: string | null } = {},
) {
  const headers = new Headers();
  const contentType =
    init.contentType === undefined ? "image/png" : init.contentType;
  if (contentType) headers.set("content-type", contentType);
  return new Response(body, { status: init.status ?? 200, headers });
}

describe("game icon GET", () => {
  const bucket = { get: vi.fn(), put: vi.fn() };
  const fetcher = vi.fn();
  const catalogReads: string[] = [];

  beforeEach(() => {
    bucket.get.mockReset().mockResolvedValue(null);
    bucket.put.mockReset().mockResolvedValue(undefined);
    fetcher.mockReset();
    catalogReads.length = 0;
    resetGameIconCaches();
    mocks.getBucket.mockReset().mockReturnValue(bucket);
    // The catalog is the site's own file; every other fetch is Discord.
    vi.stubGlobal("fetch", (url: unknown, init?: RequestInit) => {
      if (String(url) === CATALOG_URL) {
        catalogReads.push(String(url));
        return Promise.resolve(catalogResponse());
      }
      return fetcher(url, init);
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("serves a stored icon without calling Discord", async () => {
    bucket.get.mockResolvedValue({
      body: new Blob([PNG]).stream(),
      httpMetadata: { contentType: "image/webp" },
    });

    const response = await call(SPLAT);

    expect(response.status).toBe(200);
    expect(bucket.get).toHaveBeenCalledWith(KEY);
    expect(response.headers.get("Content-Type")).toBe("image/webp");
    expect(response.headers.get("Cache-Control")).toBe(
      "public, max-age=31536000, immutable",
    );
    expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
    expect(response.headers.get("Content-Security-Policy")).toBe(
      "default-src 'none'; style-src 'unsafe-inline'",
    );
    expect(response.headers.get("Content-Disposition")).toBe("inline");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
    expect(fetcher).not.toHaveBeenCalled();
    expect(bucket.put).not.toHaveBeenCalled();
  });

  it("fetches a missing icon from Discord once, stores it and serves it", async () => {
    fetcher.mockResolvedValue(upstreamResponse(PNG));

    const response = await call(SPLAT);

    expect(response.status).toBe(200);
    expect(fetcher).toHaveBeenCalledTimes(1);
    const [url, init] = fetcher.mock.calls[0] as [string, RequestInit];
    expect(url).toBe(UPSTREAM_URL);
    expect(init.redirect).toBe("manual");
    expect(init.signal).toBeInstanceOf(AbortSignal);
    expect(init.headers).toBeUndefined();
    expect(init.credentials).toBeUndefined();
    expect(bucket.put).toHaveBeenCalledTimes(1);
    const [key, stored, options] = bucket.put.mock.calls[0] as [
      string,
      ArrayBuffer,
      { httpMetadata: { contentType: string } },
    ];
    expect(key).toBe(KEY);
    expect(new Uint8Array(stored)).toEqual(PNG);
    expect(options).toEqual({ httpMetadata: { contentType: "image/png" } });
    expect(response.headers.get("Content-Type")).toBe("image/png");
    expect(response.headers.get("Cache-Control")).toBe(
      "public, max-age=31536000, immutable",
    );
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
  });

  it("keeps the content type Discord sent when it is webp or jpeg", async () => {
    fetcher.mockResolvedValue(
      upstreamResponse(PNG, { contentType: "image/webp; charset=binary" }),
    );

    const response = await call(SPLAT);

    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/webp");
    expect(bucket.put.mock.calls[0][2]).toEqual({
      httpMetadata: { contentType: "image/webp" },
    });
  });

  it.each([
    "",
    "123",
    `${APP_ID}/${HASH}`,
    `${APP_ID}/${HASH}.jpg`,
    `${APP_ID}/${HASH}.png/extra`,
    `${APP_ID}/${HASH.toUpperCase()}.png`,
    `${APP_ID}/${HASH.slice(1)}.png`,
    `abc/${HASH}.png`,
    `${"1".repeat(21)}/${HASH}.png`,
    `../server-icons/${HASH}.png`,
    `${APP_ID}/../${HASH}.png`,
    `${APP_ID}%2F${HASH}.png`,
    `${APP_ID}/${HASH}.png?size=4096`,
    `${APP_ID}/${HASH}.png\n`,
  ])(
    "returns 404 for the invalid path %j without any lookup",
    async (splat) => {
      const response = await call(splat);

      expect(response.status).toBe(404);
      expect(bucket.get).not.toHaveBeenCalled();
      expect(fetcher).not.toHaveBeenCalled();
      expect(bucket.put).not.toHaveBeenCalled();
    },
  );

  it("returns 404 and stores nothing when Discord has no such icon", async () => {
    fetcher.mockResolvedValue(upstreamResponse("not found", { status: 404 }));

    const response = await call(SPLAT);

    expect(response.status).toBe(404);
    expect(bucket.put).not.toHaveBeenCalled();
  });

  it.each([
    [
      "an HTML page",
      upstreamResponse("<html></html>", { contentType: "text/html" }),
    ],
    ["an SVG", upstreamResponse("<svg/>", { contentType: "image/svg+xml" })],
    [
      "a response with no content type",
      upstreamResponse(PNG, { contentType: null }),
    ],
    ["a server error", upstreamResponse("oops", { status: 500 })],
    ["a rate limit", upstreamResponse("slow down", { status: 429 })],
    ["an empty body", upstreamResponse(new Uint8Array(0))],
    [
      "a declared oversized body",
      new Response(PNG, {
        headers: {
          "content-type": "image/png",
          "content-length": String(256 * 1024 + 1),
        },
      }),
    ],
    ["an oversized body", upstreamResponse(new Uint8Array(256 * 1024 + 1))],
  ])("answers 502 and stores nothing for %s", async (_label, upstream) => {
    fetcher.mockResolvedValue(upstream);

    const response = await call(SPLAT);

    expect(response.status).toBe(502);
    expect(bucket.put).not.toHaveBeenCalled();
  });

  it("refuses a redirect from Discord and stores nothing", async () => {
    const headers = new Headers({ location: "https://example.com/x.png" });
    fetcher.mockResolvedValue(new Response(null, { status: 302, headers }));

    const response = await call(SPLAT);

    expect(response.status).toBe(502);
    expect(bucket.put).not.toHaveBeenCalled();
  });

  it("answers 502 when the Discord fetch fails or times out", async () => {
    fetcher.mockRejectedValue(new DOMException("timed out", "TimeoutError"));

    const response = await call(SPLAT);

    expect(response.status).toBe(502);
    expect(bucket.put).not.toHaveBeenCalled();
  });

  it("never fetches an icon that is not in the catalog, and remembers the miss", async () => {
    const other = `${OTHER_APP}/${HASH}.png`;
    expect((await call(other)).status).toBe(404);
    expect(fetcher).not.toHaveBeenCalled();
    expect(bucket.put).not.toHaveBeenCalled();
    expect(catalogReads).toHaveLength(1);

    expect((await call(other)).status).toBe(404);
    // The catalog is cached and the miss remembered.
    expect(catalogReads).toHaveLength(1);
    expect(fetcher).not.toHaveBeenCalled();
  });

  it("remembers an upstream 404 instead of asking Discord again", async () => {
    fetcher.mockResolvedValue(upstreamResponse("not found", { status: 404 }));
    expect((await call(SPLAT)).status).toBe(404);
    expect((await call(SPLAT)).status).toBe(404);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });

  it("serves the icon even when storing it fails", async () => {
    fetcher.mockResolvedValue(upstreamResponse(PNG));
    bucket.put.mockRejectedValue(new Error("r2 down"));

    const response = await call(SPLAT);

    expect(response.status).toBe(200);
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
  });
});
