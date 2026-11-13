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

/** `url`, `exp` (an hour ahead) and `sig`, as the server signs them. */
function signed(
  target: string,
  withKey: Buffer = key,
): { url: string; exp: string; sig: string } {
  const exp = Math.floor(Date.now() / 1000) + 3600;
  return {
    url: target,
    exp: String(exp),
    sig: signMediaUrl(target, withKey, exp),
  };
}

const allow = async () => null;

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
  const noBearer = async () => null;

  it("refuses a signature without exp (none is permanent)", async () => {
    const { sig } = signed(target);
    const request = new Request(proxyUrl({ url: target, sig }));
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toBeNull();
  });

  it("accepts the server's signature for exactly that URL", async () => {
    const request = new Request(proxyUrl(signed(target)));
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toEqual({ via: "signature" });
  });

  it("accepts an expiring signature only until it expires", async () => {
    const future = String(Math.floor(Date.now() / 1000) + 3600);
    const past = String(Math.floor(Date.now() / 1000) - 1);
    const fresh = new Request(
      proxyUrl({
        url: target,
        exp: future,
        sig: signMediaUrl(target, key, Number(future)),
      }),
    );
    expect(
      await authorizeGenericMediaRequest(fresh, target, key, noBearer),
    ).toEqual({ via: "signature" });
    const expired = new Request(
      proxyUrl({
        url: target,
        exp: past,
        sig: signMediaUrl(target, key, Number(past)),
      }),
    );
    expect(
      await authorizeGenericMediaRequest(expired, target, key, noBearer),
    ).toBeNull();
    // Moving `exp` breaks the signature.
    const moved = new Request(
      proxyUrl({
        url: target,
        exp: String(Number(future) + 86_400),
        sig: signMediaUrl(target, key, Number(future)),
      }),
    );
    expect(
      await authorizeGenericMediaRequest(moved, target, key, noBearer),
    ).toBeNull();
  });

  it("refuses a signature made for another URL", async () => {
    const other = "http://169.254.169.254/latest/meta-data";
    const request = new Request(proxyUrl({ ...signed(target), url: other }));
    expect(
      await authorizeGenericMediaRequest(request, other, key, noBearer),
    ).toBeNull();
    // A pass holder may proxy any public URL anyway, so a bad signature
    // falls through to it (the SSRF guard still decides what is fetched).
    const pass = issueMediaPass(key, "user-1").value;
    const withPass = new Request(request, {
      headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
    });
    expect(
      await authorizeGenericMediaRequest(withPass, other, key, noBearer),
    ).toEqual({ via: "pass", userId: "user-1" });
  });

  it("accepts the media pass cookie, with its user", async () => {
    const pass = issueMediaPass(key, "user-1").value;
    const request = new Request(proxyUrl({ url: target }), {
      headers: { Cookie: `other=1; ${MEDIA_PASS_COOKIE}=${pass}` },
    });
    expect(
      await authorizeGenericMediaRequest(request, target, key, noBearer),
    ).toEqual({ via: "pass", userId: "user-1" });
  });

  it("refuses an expired or forged pass", async () => {
    const expired = issueMediaPass(
      key,
      "user-1",
      Date.now() - 120_000,
      60,
    ).value;
    const forged = issueMediaPass(
      mediaProxyKey({ MEDIA_PROXY_SECRET: "attacker" })!,
      "user-1",
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

  it("falls back to a signed-in bearer, with its user", async () => {
    const request = new Request(proxyUrl({ url: target }), {
      headers: { Authorization: "Bearer session" },
    });
    expect(
      await authorizeGenericMediaRequest(
        request,
        target,
        key,
        async () => "user-2",
      ),
    ).toEqual({ via: "bearer", userId: "user-2" });
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
    const inner = proxyUrl(signed(target));
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
    const inner = proxyUrl(signed(target));
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
    const stale = signed(
      target,
      mediaProxyKey({ MEDIA_PROXY_SECRET: "old-secret" })!,
    );
    const pass = issueMediaPass(key, "user-1").value;
    const response = await handleProxyMedia(
      new Request(proxyUrl(stale), {
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
      new Request(proxyUrl(signed(target))),
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
      new Request(proxyUrl(signed(target))),
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
      new Request(proxyUrl(signed(target))),
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

  it("limits unsigned requests per user and signed ones per IP", async () => {
    const target = "https://cdn.example.com/og.png";
    const fetchMock = stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const tooMany = () =>
      Response.json({ error: "Rate limit exceeded" }, { status: 429 });
    const user = vi.fn(async (userId: string) =>
      userId === "user-1" ? tooMany() : null,
    );
    const signedIp = vi.fn(async (ip: string) =>
      ip === "203.0.113.9" ? tooMany() : null,
    );
    const pass = issueMediaPass(key, "user-1").value;
    const unsigned = await handleProxyMedia(
      new Request(proxyUrl({ url: target }), {
        headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
      }),
      true,
      false,
      { user, signedIp },
    );
    expect(unsigned.status).toBe(429);
    expect(user).toHaveBeenCalledWith("user-1");
    expect(signedIp).not.toHaveBeenCalled();

    user.mockClear();
    const fromBusyIp = await handleProxyMedia(
      new Request(proxyUrl(signed(target)), {
        headers: { "CF-Connecting-IP": "203.0.113.9" },
      }),
      true,
      false,
      { user, signedIp },
    );
    expect(fromBusyIp.status).toBe(429);
    expect(signedIp).toHaveBeenCalledWith("203.0.113.9");
    expect(user).not.toHaveBeenCalled();
    // Refused before anything was fetched upstream.
    expect(
      fetchMock.mock.calls.some(([input]) => String(input) === target),
    ).toBe(false);

    const fromOtherIp = await handleProxyMedia(
      new Request(proxyUrl(signed(target)), {
        headers: { "CF-Connecting-IP": "198.51.100.7" },
      }),
      true,
      false,
      { user, signedIp },
    );
    expect(fromOtherIp.status).toBe(200);
    expect(signedIp).toHaveBeenLastCalledWith("198.51.100.7");
  });

  it("refuses non-default ports, signed or not", async () => {
    const target = "https://cdn.example.com:8443/og.png";
    const fetchMock = stubNetwork({
      [target]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const pass = issueMediaPass(key, "user-1").value;
    const unsigned = await handleProxyMedia(
      new Request(proxyUrl({ url: target }), {
        headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
      }),
      true,
      false,
      { user: allow, signedIp: allow },
    );
    expect(unsigned.status).toBe(400);
    const signedRequest = await handleProxyMedia(
      new Request(proxyUrl(signed(target))),
      true,
      false,
      { user: allow, signedIp: allow },
    );
    expect(signedRequest.status).toBe(400);
    expect(
      fetchMock.mock.calls.some(([input]) => String(input) === target),
    ).toBe(false);
  });

  it("holds every redirect hop to the default ports", async () => {
    const target = "https://cdn.example.com/og.png";
    const hop = "https://cdn.example.com:8080/og.png";
    const fetchMock = stubNetwork({
      [target]: () =>
        new Response(null, { status: 302, headers: { Location: hop } }),
      [hop]: () =>
        new Response(PNG, { headers: { "Content-Type": "image/png" } }),
    });
    const pass = issueMediaPass(key, "user-1").value;
    for (const request of [
      new Request(proxyUrl({ url: target }), {
        headers: { Cookie: `${MEDIA_PASS_COOKIE}=${pass}` },
      }),
      new Request(proxyUrl(signed(target))),
    ]) {
      const response = await handleProxyMedia(request, true, false, {
        user: allow,
        signedIp: allow,
      });
      expect(response.status).toBe(403);
      expect(await response.text()).toContain("port");
    }
    expect(fetchMock.mock.calls.some(([input]) => String(input) === hop)).toBe(
      false,
    );
  });

  it("refuses our own zones and the trailing-dot form of its host", async () => {
    const fetchMock = stubNetwork({});
    for (const target of [
      "https://yap.example./api/avatars/u.png",
      "https://YAP.EXAMPLE/api/avatars/u.png",
      "https://cdn.115jon.com/a.png",
      "https://Cdn.115Jon.Com./a.png",
      "https://meet.115jon.site/a.png",
      "https://other.example/api/proxy-media/?url=x",
      "https://other.example/API/Proxy-Media",
    ]) {
      const response = await handleProxyMedia(
        new Request(proxyUrl(signed(target))),
        true,
        false,
        { user: allow, signedIp: allow },
      );
      expect(response.status, target).toBeGreaterThanOrEqual(400);
      expect(response.status, target).toBeLessThan(500);
    }
    expect(
      fetchMock.mock.calls.some(
        ([input]) => new URL(String(input)).hostname !== "cloudflare-dns.com",
      ),
    ).toBe(false);
  });

  it("is off (allowlist only) when no secret is configured", async () => {
    delete (env as unknown as Record<string, unknown>).MEDIA_PROXY_SECRET;
    const target = "https://cdn.example.com/og.png";
    stubNetwork({});
    const response = await handleProxyMedia(
      new Request(proxyUrl(signed(target))),
      true,
    );
    expect(response.status).toBe(401);
  });
});
