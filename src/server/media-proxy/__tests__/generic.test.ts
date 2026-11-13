import { describe, expect, it, vi } from "vitest";
import {
  allowedMediaType,
  cacheTtlSeconds,
  fetchPublicMedia,
  isSelfTarget,
  MEDIA_PROXY_LIMITS,
  serveGenericMedia,
  sniffMediaType,
  type GenericProxyDeps,
} from "../generic";
import type { ResolveHost } from "../ssrf";

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1]);

const resolve: ResolveHost = async (hostname) => {
  if (hostname.startsWith("internal.")) return ["10.0.0.7"];
  return ["93.184.215.14"];
};

type Route = (request: Request) => Response | Promise<Response>;

function fakeFetch(routes: Record<string, Route>) {
  const calls: Request[] = [];
  const fetchImpl = vi.fn(
    async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = new Request(String(input), init);
      calls.push(request);
      const route = routes[request.url];
      if (!route) return new Response("missing", { status: 404 });
      return route(request);
    },
  );
  return { fetchImpl: fetchImpl as unknown as typeof fetch, calls };
}

function proxyRequest(target: string, headers: HeadersInit = {}): Request {
  return new Request(
    `https://yap.example/api/proxy-media?url=${encodeURIComponent(target)}`,
    { headers },
  );
}

function deps(
  routes: Record<string, Route>,
  extra: Partial<GenericProxyDeps> = {},
) {
  const fake = fakeFetch(routes);
  return {
    fake,
    deps: { resolve, fetchImpl: fake.fetchImpl, ...extra } as GenericProxyDeps,
  };
}

describe("content types", () => {
  it("allows images (gif, webp, avif included), mp4/webm and audio", () => {
    for (const type of [
      "image/jpeg",
      "image/png",
      "image/gif",
      "image/webp",
      "image/avif",
      "video/mp4",
      "video/webm",
      "audio/mpeg",
      "IMAGE/PNG; charset=binary",
    ]) {
      expect(allowedMediaType(type), type).not.toBeNull();
    }
  });

  it("refuses SVG, HTML, scripts and anything else", () => {
    for (const type of [
      "image/svg+xml",
      "text/html",
      "application/javascript",
      "application/json",
      "application/x-mpegurl",
      "application/octet-stream",
      null,
    ]) {
      expect(allowedMediaType(type), String(type)).toBeNull();
    }
  });

  it("sniffs untyped bodies by their magic bytes", () => {
    expect(sniffMediaType(PNG)).toBe("image/png");
    expect(sniffMediaType(new TextEncoder().encode("GIF89a..."))).toBe(
      "image/gif",
    );
    expect(sniffMediaType(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe(
      "image/jpeg",
    );
    expect(sniffMediaType(new TextEncoder().encode("\0\0\0\x20ftypisom"))).toBe(
      "video/mp4",
    );
    expect(sniffMediaType(new TextEncoder().encode("<svg xmlns="))).toBeNull();
    expect(sniffMediaType(new TextEncoder().encode("<html>"))).toBeNull();
  });
});

describe("fetchPublicMedia", () => {
  it("sends no viewer data: fixed UA, no cookies, no referer", async () => {
    const { fake, deps: d } = deps({
      "https://img.example/a.png": () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const result = await fetchPublicMedia(
      new URL("https://img.example/a.png"),
      { method: "GET" },
      d,
    );
    expect(result.ok).toBe(true);
    const sent = fake.calls[0];
    expect(sent.headers.get("User-Agent")).toBe(
      "Mozilla/5.0 (compatible; YapMediaProxy/1.0)",
    );
    expect(sent.headers.get("Cookie")).toBeNull();
    expect(sent.headers.get("Referer")).toBeNull();
    expect(sent.headers.get("Authorization")).toBeNull();
    expect(sent.redirect).toBe("manual");
  });

  it("follows a redirect to another public host", async () => {
    const { deps: d } = deps({
      "https://short.example/x": () =>
        new Response(null, {
          status: 302,
          headers: { Location: "https://img.example/a.png" },
        }),
      "https://img.example/a.png": () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const result = await fetchPublicMedia(
      new URL("https://short.example/x"),
      { method: "GET" },
      d,
    );
    expect(result.ok && result.url.href).toBe("https://img.example/a.png");
  });

  it("refuses a redirect to a private address", async () => {
    const { fake, deps: d } = deps({
      "https://img.example/a.png": () =>
        new Response(null, {
          status: 301,
          headers: { Location: "http://169.254.169.254/latest/meta-data/" },
        }),
    });
    const result = await fetchPublicMedia(
      new URL("https://img.example/a.png"),
      { method: "GET" },
      d,
    );
    expect(result).toMatchObject({ ok: false, status: 403 });
    expect(fake.calls).toHaveLength(1);
  });

  it("holds every redirect hop to the port policy", async () => {
    for (const location of [
      "https://img.example:8443/a.png",
      "http://img.example:8080/a.png",
      "https://img.example:22/a.png",
    ]) {
      const { fake, deps: d } = deps({
        "https://short.example/x": () =>
          new Response(null, { status: 302, headers: { Location: location } }),
        [location]: () =>
          new Response(PNG, { headers: { "Content-Type": "image/png" } }),
      });
      const result = await fetchPublicMedia(
        new URL("https://short.example/x"),
        { method: "GET" },
        d,
      );
      expect(result, location).toMatchObject({
        ok: false,
        status: 403,
        message: "Blocked media URL (port)",
      });
      expect(fake.calls.map((call) => call.url)).toEqual([
        "https://short.example/x",
      ]);
    }
  });

  it("checks hops against the ports it is given", async () => {
    const location = "https://img.example:8443/a.png";
    const { deps: d } = deps({
      "https://short.example/x": () =>
        new Response(null, { status: 302, headers: { Location: location } }),
      [location]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const result = await fetchPublicMedia(
      new URL("https://short.example/x"),
      { method: "GET" },
      { ...d, allowedPorts: new Set(["", "8443"]) },
    );
    expect(result.ok && result.url.href).toBe(location);
  });

  it("refuses a redirect to a name that resolves privately", async () => {
    const { fake, deps: d } = deps({
      "https://img.example/a.png": () =>
        new Response(null, {
          status: 307,
          headers: { Location: "https://internal.example/secret" },
        }),
    });
    const result = await fetchPublicMedia(
      new URL("https://img.example/a.png"),
      { method: "GET" },
      d,
    );
    expect(result).toMatchObject({ ok: false, status: 403 });
    expect(fake.calls.map((call) => call.url)).toEqual([
      "https://img.example/a.png",
    ]);
  });

  it("re-resolves at every hop (rebinding between hops)", async () => {
    let lookups = 0;
    const rebinding: ResolveHost = async () =>
      lookups++ === 0 ? ["93.184.215.14"] : ["127.0.0.1"];
    const { fake } = deps({});
    const fetchImpl = vi.fn(
      async () =>
        new Response(null, {
          status: 302,
          headers: { Location: "https://flip.example/again" },
        }),
    );
    const result = await fetchPublicMedia(
      new URL("https://flip.example/start"),
      { method: "GET" },
      { resolve: rebinding, fetchImpl: fetchImpl as unknown as typeof fetch },
    );
    expect(result).toMatchObject({ ok: false, status: 403 });
    expect(fetchImpl).toHaveBeenCalledTimes(1);
    expect(fake.calls).toHaveLength(0);
  });

  it("never follows a redirect back to us or to a proxy path", async () => {
    for (const location of [
      "https://yap.example/api/avatars/u.png",
      "https://elsewhere.example/api/proxy-media?url=x",
    ]) {
      const { fake, deps: d } = deps(
        {
          "https://img.example/a.png": () =>
            new Response(null, {
              status: 302,
              headers: { Location: location },
            }),
        },
        { ownHost: "yap.example" },
      );
      const result = await fetchPublicMedia(
        new URL("https://img.example/a.png"),
        { method: "GET" },
        d,
      );
      expect(result, location).toMatchObject({ ok: false, status: 403 });
      expect(fake.calls).toHaveLength(1);
    }
  });

  it("stops after the redirect limit", async () => {
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const n = Number(new URL(String(input)).searchParams.get("n") ?? 0);
      return new Response(null, {
        status: 302,
        headers: { Location: `https://loop.example/?n=${n + 1}` },
      });
    });
    const result = await fetchPublicMedia(
      new URL("https://loop.example/?n=0"),
      { method: "GET" },
      { resolve, fetchImpl: fetchImpl as unknown as typeof fetch },
    );
    expect(result).toMatchObject({ ok: false, status: 508 });
    expect(fetchImpl).toHaveBeenCalledTimes(
      MEDIA_PROXY_LIMITS.maxRedirects + 1,
    );
  });

  it("times out an upstream that never answers", async () => {
    vi.useFakeTimers();
    try {
      const fetchImpl = vi.fn(
        (_input: RequestInfo | URL, init?: RequestInit) =>
          new Promise<Response>((_, reject) => {
            init?.signal?.addEventListener("abort", () =>
              reject(new DOMException("aborted", "AbortError")),
            );
          }),
      );
      const pending = fetchPublicMedia(
        new URL("https://slow.example/a.png"),
        { method: "GET" },
        { resolve, fetchImpl: fetchImpl as unknown as typeof fetch },
      );
      await vi.advanceTimersByTimeAsync(MEDIA_PROXY_LIMITS.headersTimeoutMs);
      await expect(pending).resolves.toMatchObject({ ok: false, status: 504 });
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("serveGenericMedia", () => {
  it("streams an allowed image with safe headers", async () => {
    const { deps: d } = deps({
      "https://img.example/a.png": () =>
        new Response(PNG, {
          headers: {
            "Content-Type": "image/png",
            "Content-Length": String(PNG.byteLength),
            "Set-Cookie": "tracker=1",
            "Cache-Control": "max-age=60",
          },
        }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/a.png"),
      new URL("https://img.example/a.png"),
      true,
      d,
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/png");
    expect(response.headers.get("Set-Cookie")).toBeNull();
    expect(response.headers.get("X-Content-Type-Options")).toBe("nosniff");
    expect(response.headers.get("Content-Security-Policy")).toContain(
      "sandbox",
    );
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=3600");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
  });

  it("refuses SVG and HTML with 415", async () => {
    const { deps: d } = deps({
      "https://img.example/a.svg": () =>
        new Response("<svg onload=alert(1)>", {
          headers: { "Content-Type": "image/svg+xml" },
        }),
      "https://img.example/page": () =>
        new Response("<html>", { headers: { "Content-Type": "text/html" } }),
    });
    for (const target of [
      "https://img.example/a.svg",
      "https://img.example/page",
    ]) {
      const response = await serveGenericMedia(
        proxyRequest(target),
        new URL(target),
        true,
        d,
      );
      expect(response.status, target).toBe(415);
    }
  });

  it("types an octet-stream body by its bytes", async () => {
    const { deps: d } = deps({
      "https://img.example/blob": () =>
        new Response(PNG, {
          headers: { "Content-Type": "application/octet-stream" },
        }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/blob"),
      new URL("https://img.example/blob"),
      true,
      d,
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/png");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
  });

  it("refuses an image over the cap by its Content-Length", async () => {
    const { deps: d } = deps({
      "https://img.example/huge.png": () =>
        new Response(PNG, {
          headers: {
            "Content-Type": "image/png",
            "Content-Length": String(MEDIA_PROXY_LIMITS.imageBytes + 1),
          },
        }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/huge.png"),
      new URL("https://img.example/huge.png"),
      true,
      d,
    );
    expect(response.status).toBe(413);
  });

  it("cuts a body that runs past the cap without a length", async () => {
    const chunk = new Uint8Array(1024 * 1024);
    let sent = 0;
    const endless = new ReadableStream<Uint8Array>({
      pull(controller) {
        sent += chunk.byteLength;
        controller.enqueue(chunk);
        if (sent > MEDIA_PROXY_LIMITS.imageBytes * 2) controller.close();
      },
    });
    const { deps: d } = deps({
      "https://img.example/stream.gif": () =>
        new Response(endless, { headers: { "Content-Type": "image/gif" } }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/stream.gif"),
      new URL("https://img.example/stream.gif"),
      true,
      d,
    );
    expect(response.status).toBe(200);
    await expect(response.arrayBuffer()).rejects.toThrow();
    expect(sent).toBeLessThan(MEDIA_PROXY_LIMITS.imageBytes * 2);
  });

  it("passes a video range through and caps on the whole file", async () => {
    const { fake, deps: d } = deps({
      "https://vid.example/clip.mp4": (request) =>
        new Response(new Uint8Array(100), {
          status: 206,
          headers: {
            "Content-Type": "video/mp4",
            "Content-Length": "100",
            "Content-Range": `bytes 0-99/${request.url.includes("big") ? 0 : 5000}`,
            "Accept-Ranges": "bytes",
          },
        }),
      "https://vid.example/big.mp4": () =>
        new Response(new Uint8Array(100), {
          status: 206,
          headers: {
            "Content-Type": "video/mp4",
            "Content-Range": `bytes 0-99/${MEDIA_PROXY_LIMITS.videoBytes + 1}`,
          },
        }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://vid.example/clip.mp4", { Range: "bytes=0-99" }),
      new URL("https://vid.example/clip.mp4"),
      true,
      d,
    );
    expect(fake.calls[0].headers.get("Range")).toBe("bytes=0-99");
    expect(response.status).toBe(206);
    expect(response.headers.get("Content-Range")).toBe("bytes 0-99/5000");
    expect(response.headers.get("Accept-Ranges")).toBe("bytes");

    const tooBig = await serveGenericMedia(
      proxyRequest("https://vid.example/big.mp4", { Range: "bytes=0-99" }),
      new URL("https://vid.example/big.mp4"),
      true,
      d,
    );
    expect(tooBig.status).toBe(413);
  });

  it("answers HEAD without a body", async () => {
    const { fake, deps: d } = deps({
      "https://img.example/a.png": () =>
        new Response(null, {
          headers: { "Content-Type": "image/png", "Content-Length": "9" },
        }),
    });
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/a.png"),
      new URL("https://img.example/a.png"),
      false,
      d,
    );
    expect(fake.calls[0].method).toBe("HEAD");
    expect(response.status).toBe(200);
    expect(response.body).toBeNull();
  });

  it("maps upstream errors without passing their bodies", async () => {
    const { deps: d } = deps({
      "https://img.example/gone.png": () =>
        new Response("secret error page", { status: 404 }),
      "https://img.example/err.png": () =>
        new Response("oops", { status: 500 }),
    });
    const gone = await serveGenericMedia(
      proxyRequest("https://img.example/gone.png"),
      new URL("https://img.example/gone.png"),
      true,
      d,
    );
    expect(gone.status).toBe(404);
    expect(await gone.text()).not.toContain("secret");
    const err = await serveGenericMedia(
      proxyRequest("https://img.example/err.png"),
      new URL("https://img.example/err.png"),
      true,
      d,
    );
    expect(err.status).toBe(502);
  });

  it("stores full responses in the edge cache and serves hits from it", async () => {
    const store = new Map<string, Response>();
    const cache = {
      match: vi.fn(async (request: Request) => {
        const hit = store.get(request.url);
        return hit ? hit.clone() : undefined;
      }),
      put: vi.fn(async (key: string, response: Response) => {
        store.set(key, new Response(await response.arrayBuffer(), response));
      }),
    } as unknown as Cache;
    const pending: Promise<unknown>[] = [];
    const beforeUpstream = vi.fn(async () => null);
    const { fake, deps: d } = deps(
      {
        "https://img.example/a.png": () =>
          new Response(PNG, {
            headers: {
              "Content-Type": "image/png",
              "Content-Length": String(PNG.byteLength),
            },
          }),
      },
      { cache, waitUntil: (promise) => pending.push(promise), beforeUpstream },
    );

    const first = await serveGenericMedia(
      proxyRequest("https://img.example/a.png"),
      new URL("https://img.example/a.png"),
      true,
      d,
    );
    expect(new Uint8Array(await first.arrayBuffer())).toEqual(PNG);
    await Promise.all(pending);
    expect(cache.put).toHaveBeenCalledTimes(1);
    const [cacheKey] = (cache.put as ReturnType<typeof vi.fn>).mock.calls[0];
    expect(cacheKey).toBe(
      "https://yap.example/__yap-media-cache/v1?u=https%3A%2F%2Fimg.example%2Fa.png",
    );

    const second = await serveGenericMedia(
      proxyRequest("https://img.example/a.png"),
      new URL("https://img.example/a.png"),
      true,
      d,
    );
    expect(new Uint8Array(await second.arrayBuffer())).toEqual(PNG);
    expect(fake.calls).toHaveLength(1);
    // The upstream hook (the signed per-IP limit) runs on the miss only.
    expect(beforeUpstream).toHaveBeenCalledTimes(1);
  });

  it("serves what the upstream hook returns instead of fetching", async () => {
    const { fake, deps: d } = deps(
      {
        "https://img.example/a.png": () =>
          new Response(PNG, { headers: { "Content-Type": "image/png" } }),
      },
      {
        beforeUpstream: async () => new Response("slow down", { status: 429 }),
      },
    );
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/a.png"),
      new URL("https://img.example/a.png"),
      true,
      d,
    );
    expect(response.status).toBe(429);
    expect(fake.calls).toHaveLength(0);
  });

  it("keeps the declared length and skips the cache for large files", async () => {
    const cache = {
      match: vi.fn(async () => undefined),
      put: vi.fn(async () => {}),
    } as unknown as Cache;
    const big = 9 * 1024 * 1024;
    const { deps: d } = deps(
      {
        "https://img.example/big.gif": () =>
          new Response(new Uint8Array(big), {
            headers: {
              "Content-Type": "image/gif",
              "Content-Length": String(big),
            },
          }),
      },
      { cache, waitUntil: () => {} },
    );
    const response = await serveGenericMedia(
      proxyRequest("https://img.example/big.gif"),
      new URL("https://img.example/big.gif"),
      true,
      d,
    );
    expect(response.headers.get("Content-Length")).toBe(String(big));
    expect((await response.arrayBuffer()).byteLength).toBe(big);
    expect(cache.put).not.toHaveBeenCalled();
  });

  it("does not store a ranged response", async () => {
    const cache = {
      match: vi.fn(async () => undefined),
      put: vi.fn(async () => {}),
    } as unknown as Cache;
    const { deps: d } = deps(
      {
        "https://vid.example/clip.mp4": () =>
          new Response(new Uint8Array(10), {
            status: 206,
            headers: {
              "Content-Type": "video/mp4",
              "Content-Range": "bytes 0-9/100",
            },
          }),
      },
      { cache, waitUntil: () => {} },
    );
    await serveGenericMedia(
      proxyRequest("https://vid.example/clip.mp4", { Range: "bytes=0-9" }),
      new URL("https://vid.example/clip.mp4"),
      true,
      d,
    );
    expect(cache.put).not.toHaveBeenCalled();
  });
});

describe("cacheTtlSeconds", () => {
  it("clamps upstream max-age between an hour and a week", () => {
    expect(cacheTtlSeconds(null)).toBe(86400);
    expect(cacheTtlSeconds("no-cache")).toBe(86400);
    expect(cacheTtlSeconds("public, max-age=5")).toBe(3600);
    expect(cacheTtlSeconds("max-age=172800")).toBe(172800);
    expect(cacheTtlSeconds("max-age=99999999")).toBe(604800);
  });
});

describe("isSelfTarget", () => {
  it.each([
    ["https://yap.example/a.png", "yap.example"],
    ["https://yap.example./a.png", "yap.example"],
    ["https://YAP.Example/a.png", "yap.example"],
    ["https://yap.example:8443/a.png", "yap.example"],
    ["https://other.example/api/proxy-media", undefined],
    ["https://other.example/api/proxy-media/", undefined],
    ["https://other.example/api/proxy-media//", undefined],
    ["https://other.example/API/Proxy-Media?url=x", undefined],
  ])("catches %s", (url, ownHost) => {
    expect(isSelfTarget(new URL(url), ownHost)).toBe(true);
  });

  it("lets other hosts and paths through", () => {
    expect(
      isSelfTarget(new URL("https://cdn.example/a.png"), "yap.example"),
    ).toBe(false);
    expect(
      isSelfTarget(
        new URL("https://cdn.example/api/proxy-media-x"),
        "yap.example",
      ),
    ).toBe(false);
  });
});
