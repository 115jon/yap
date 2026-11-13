import { env } from "cloudflare:workers";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  issueMediaPass,
  MEDIA_PASS_COOKIE,
  mediaProxyKey,
  signMediaUrl,
} from "@/server/media-proxy/signing";
import { authorizeGenericMediaRequest, handleProxyMedia } from "../proxy-media";

const SECRET = "route-test-secret";
const key = mediaProxyKey({ MEDIA_PROXY_SECRET: SECRET })!;
const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

function proxyUrl(params: Record<string, string>): string {
  return `https://yap.example/api/proxy-media?${new URLSearchParams(params)}`;
}

/** Answers DoH lookups (public unless the name starts with "internal.") and media. */
function stubNetwork(media: Record<string, () => Response>) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
    const url = new URL(String(input));
    if (url.hostname === "cloudflare-dns.com") {
      const name = url.searchParams.get("name") ?? "";
      const type = url.searchParams.get("type");
      const data =
        type === "A"
          ? name.startsWith("internal.")
            ? "10.0.0.9"
            : "93.184.215.14"
          : null;
      return Response.json({
        Status: 0,
        Answer: data ? [{ type: 1, data, TTL: 60 }] : [],
      });
    }
    const route = media[url.href];
    return route ? route() : new Response("not found", { status: 404 });
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

beforeEach(() => {
  (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET = SECRET;
});

afterEach(() => {
  delete (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET;
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("authorizeGenericMediaRequest", () => {
  const target = "https://cdn.example.com/og.png";
  const noBearer = async () => false;

  it("accepts the server's signature for exactly that URL", async () => {
    const request = new Request(
      proxyUrl({ url: target, sig: signMediaUrl(target, key) }),
    );
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toBe("signature");
  });

  it("refuses a signature made for another URL", async () => {
    const other = "http://169.254.169.254/latest/meta-data";
    const request = new Request(
      proxyUrl({ url: other, sig: signMediaUrl(target, key) }),
    );
    expect(
      await authorizeGenericMediaRequest(request, other, key, noBearer),
    ).toBeNull();
    // A pass holder may proxy any public URL anyway, so a bad signature
    // falls through to it (the SSRF guard still decides what is fetched).
    const pass = issueMediaPass(key).value;
    const withPass = new Request(request, {
      headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
    });
    expect(
      await authorizeGenericMediaRequest(withPass, other, key, noBearer),
    ).toBe("pass");
  });

  it("accepts the media pass cookie", async () => {
    const pass = issueMediaPass(key).value;
    const request = new Request(proxyUrl({ url: target }), {
      headers: { Cookie: `other=1; ${MEDIA_PASS_COOKIE}=${pass}` },
    });
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toBe("pass");
  });

  it("refuses an expired or forged pass", async () => {
    const expired = issueMediaPass(key, Date.now() - 120_000, 60).value;
    const forged = issueMediaPass(
      mediaProxyKey({ MEDIA_PROXY_SECRET: "attacker" })!,
    ).value;
    for (const pass of [expired, forged]) {
      const request = new Request(proxyUrl({ url: target }), {
        headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
      });
      expect(
        await authorizeGenericMediaRequest(request, target, key, noBearer),
      ).toBeNull();
    }
  });

  it("falls back to a signed-in bearer", async () => {
    const request = new Request(proxyUrl({ url: target }), {
      headers: { Authorization: "Bearer session" },
    });
    expect(
      await authorizeGenericMediaRequest(
        request,
        target,
        key,
        async () => true,
      ),
    ).toBe("bearer");
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toBeNull();
  });
});

describe("handleProxyMedia", () => {
  it("serves a proxy URL wrapped in another (clients built before rewriting)", async () => {
    const target = "https://cdn.example.com/og.png";
    stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const inner = proxyUrl({ url: target, sig: signMediaUrl(target, key) });
    const response = await handleProxyMedia(
      new Request(
        proxyUrl({ url: inner, sourceUrl: "https://news.example/post" }),
      ),
      true,
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/png");
  });

  it("unwraps only one level", async () => {
    stubNetwork({});
    const target = "https://cdn.example.com/og.png";
    const inner = proxyUrl({ url: target, sig: signMediaUrl(target, key) });
    const twice = proxyUrl({ url: proxyUrl({ url: inner }) });
    const response = await handleProxyMedia(new Request(twice), true);
    expect(response.status).toBeGreaterThanOrEqual(400);
  });

  it("lets a pass through when the signature is stale (rotated key)", async () => {
    const target = "https://cdn.example.com/og.png";
    stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const stale = signMediaUrl(
      target,
      mediaProxyKey({ MEDIA_PROXY_SECRET: "old-secret" })!,
    );
    const pass = issueMediaPass(key).value;
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target, sig: stale }), {
        headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
      }),
      true,
    );
    expect(response.status).toBe(200);
  });

  it("refuses an unsigned, unauthenticated third-party URL (no open proxy)", async () => {
    const fetchMock = stubNetwork({});
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: "https://cdn.example.com/og.png" })),
      true,
    );
    expect(response.status).toBe(401);
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("serves a signed third-party image", async () => {
    const target = "https://cdn.example.com/og.png";
    stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target, sig: signMediaUrl(target, key) })),
      true,
    );
    expect(response.status).toBe(200);
    expect(response.headers.get("Content-Type")).toBe("image/png");
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(PNG);
  });

  it("refuses a signed URL whose name resolves to a private address", async () => {
    const target = "https://internal.example/admin.png";
    const fetchMock = stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target, sig: signMediaUrl(target, key) })),
      true,
    );
    expect(response.status).toBe(403);
    expect(
      fetchMock.mock.calls.some(([input]) => String(input) === target),
    ).toBe(false);
  });

  it("refuses to proxy its own host", async () => {
    const target = "https://yap.example/api/avatars/u.png";
    stubNetwork({});
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target, sig: signMediaUrl(target, key) })),
      true,
    );
    expect(response.status).toBe(400);
  });

  it("keeps the allowlisted path working unsigned for older clients", async () => {
    const target = "https://static.klipy.com/ii/a.gif";
    const fetchMock = stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/gif" } }),
    });
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target })),
      true,
    );
    expect(response.status).toBe(200);
    expect(
      fetchMock.mock.calls.some(([input]) => String(input) === target),
    ).toBe(true);
  });

  it("is off (allowlist only) when no secret is configured", async () => {
    delete (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET;
    const target = "https://cdn.example.com/og.png";
    stubNetwork({});
    const response = await handleProxyMedia(
      new Request(proxyUrl({ url: target, sig: signMediaUrl(target, key) })),
      true,
    );
    expect(response.status).toBe(401);
  });
});
