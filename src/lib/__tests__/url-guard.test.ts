import { afterEach, describe, expect, it, vi } from "vitest";
import {
  BlockedUrlError,
  MAX_PREVIEW_REDIRECTS,
  TooManyRedirectsError,
  guardedFetch,
  previewUrlBlockReason,
} from "../url-guard";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("previewUrlBlockReason", () => {
  it.each([
    "https://www.instagram.com/p/DfajF1zvsbl/",
    "http://example.com/article",
    "https://example.com:8443/x",
    "https://8.8.8.8/",
    "https://[2606:4700:4700::1111]/",
  ])("lets a public URL through: %s", (url) => {
    expect(previewUrlBlockReason(url)).toBeNull();
  });

  it.each([
    ["http://localhost:8787/", "port"],
    ["http://localhost/", "local-host"],
    ["http://api.localhost/", "local-host"],
    ["http://printer.local/", "local-host"],
    ["http://metadata.google.internal/", "local-host"],
    ["http://intranet/", "single-label-host"],
    ["http://127.0.0.1/", "private-address"],
    ["http://127.1/", "private-address"],
    ["http://2130706433/", "private-address"],
    ["http://0x7f.0.0.1/", "private-address"],
    ["http://0.0.0.0/", "private-address"],
    ["http://10.1.2.3/", "private-address"],
    ["http://172.20.0.1/", "private-address"],
    ["http://192.168.1.1/", "private-address"],
    ["http://169.254.169.254/latest/meta-data/", "private-address"],
    ["http://100.64.0.1/", "private-address"],
    ["http://224.0.0.1/", "private-address"],
    ["http://[::1]/", "private-address"],
    ["http://[::]/", "private-address"],
    ["http://[fd00::1]/", "private-address"],
    ["http://[fe80::1]/", "private-address"],
    ["http://[::ffff:127.0.0.1]/", "private-address"],
    ["http://[::ffff:a9fe:a9fe]/", "private-address"],
    ["http://[2002:7f00:1::]/", "private-address"],
    ["http://[2001:0:4136:e378::]/", "private-address"],
    ["http://198.51.100.7/", "private-address"],
    ["https://user:pass@example.com/", "credentials"],
    ["https://example.com:22/", "port"],
    ["ftp://example.com/", "protocol"],
    ["file:///etc/passwd", "protocol"],
    ["javascript:alert(1)", "protocol"],
    ["not a url", "invalid-url"],
  ])("blocks %s (%s)", (url, reason) => {
    expect(previewUrlBlockReason(url)).toBe(reason);
  });
});

describe("guardedFetch", () => {
  it("checks every redirect hop and refuses one into private space", async () => {
    const fetchMock = vi.fn(
      async (input: RequestInfo | URL, _init?: RequestInit) => {
        const url = String(input);
        if (url === "https://short.example/abc") {
          return new Response(null, {
            status: 302,
            headers: { location: "http://169.254.169.254/latest/meta-data/" },
          });
        }
        return new Response("secret");
      },
    );
    vi.stubGlobal("fetch", fetchMock);

    await expect(
      guardedFetch("https://short.example/abc"),
    ).rejects.toBeInstanceOf(BlockedUrlError);
    // The metadata address was never requested.
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0]?.[1]).toMatchObject({ redirect: "manual" });
  });

  it("follows relative redirects and reports the final URL", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) =>
        String(input) === "https://example.com/old"
          ? new Response(null, { status: 301, headers: { location: "/new" } })
          : new Response("<html></html>", {
              headers: { "content-type": "text/html" },
            }),
      ),
    );

    const { response, finalUrl } = await guardedFetch(
      "https://example.com/old",
    );
    expect(response.status).toBe(200);
    expect(finalUrl).toBe("https://example.com/new");
  });

  it("gives up after the hop limit", async () => {
    let hops = 0;
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => {
        hops += 1;
        return new Response(null, {
          status: 302,
          headers: { location: `https://example.com/${hops}` },
        });
      }),
    );

    await expect(guardedFetch("https://example.com/0")).rejects.toBeInstanceOf(
      TooManyRedirectsError,
    );
    expect(hops).toBe(MAX_PREVIEW_REDIRECTS + 1);
  });

  it("refuses a blocked URL without making a request", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    await expect(guardedFetch("http://127.0.0.1:80/")).rejects.toThrow(
      BlockedUrlError,
    );
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
