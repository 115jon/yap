import { describe, expect, it, vi } from "vitest";
import {
  checkMediaTarget,
  createDohResolver,
  isBlockedAddress,
  type ResolveHost,
} from "../ssrf";

const publicResolver: ResolveHost = async () => ["93.184.215.14"];

function resolverFrom(table: Record<string, string[]>): ResolveHost {
  return async (hostname) => {
    const answer = table[hostname];
    if (!answer) throw new Error(`no record for ${hostname}`);
    return answer;
  };
}

describe("isBlockedAddress", () => {
  it.each([
    "0.0.0.0",
    "10.1.2.3",
    "100.64.0.1",
    "127.0.0.1",
    "169.254.169.254",
    "172.16.0.1",
    "172.31.255.255",
    "192.0.0.8",
    "192.0.2.1",
    "192.168.1.1",
    "198.18.0.1",
    "224.0.0.1",
    "255.255.255.255",
  ])("blocks IPv4 %s", (address) => {
    expect(isBlockedAddress(address)).toBe(true);
  });

  it.each([
    "::",
    "::1",
    "[::1]",
    "fc00::1",
    "fd12:3456::1",
    "fe80::1",
    "fe80::1%eth0",
    "ff02::1",
    "2001:db8::1",
    "::ffff:127.0.0.1",
    "::ffff:7f00:1",
    "::ffff:10.0.0.1",
    "64:ff9b::a9fe:a9fe", // NAT64 of 169.254.169.254
    "2002:c0a8:0101::1", // 6to4 of 192.168.1.1
    "2001:0:4136:e378::1", // Teredo
    "::ffff:0:7f00:1", // IPv4-translated 127.0.0.1
    "64:ff9b:1::a00:1", // local-use NAT64
  ])("blocks IPv6 %s", (address) => {
    expect(isBlockedAddress(address)).toBe(true);
  });

  it.each([
    "93.184.215.14",
    "1.1.1.1",
    "172.32.0.1",
    "100.128.0.1",
    "2606:4700:4700::1111",
    "::ffff:93.184.215.14",
    "2002:5db8:d70e::1", // 6to4 of a public address
  ])("allows public %s", (address) => {
    expect(isBlockedAddress(address)).toBe(false);
  });

  it("refuses what it cannot parse", () => {
    expect(isBlockedAddress("not-an-ip")).toBe(true);
    expect(isBlockedAddress("1.2.3")).toBe(true);
    expect(isBlockedAddress("1:2:3:4:5:6:7:8:9")).toBe(true);
  });
});

describe("checkMediaTarget", () => {
  it("accepts a public https URL", async () => {
    const result = await checkMediaTarget(
      "https://images.example.com/a.png",
      publicResolver,
    );
    expect(result.ok).toBe(true);
  });

  it.each([
    ["ftp://example.com/a.png", "scheme"],
    ["file:///etc/passwd", "scheme"],
    ["data:image/png;base64,AAAA", "scheme"],
    ["javascript:alert(1)", "scheme"],
    ["https://user:pass@example.com/a.png", "credentials"],
    ["https://example.com:22/a.png", "port"],
    ["https://example.com:8080/a.png", "port"],
    ["https://example.com:8443/a.png", "port"],
    ["http://localhost/a.png", "hostname"],
    ["http://app.localhost/a.png", "hostname"],
    ["http://intranet/a.png", "hostname"],
    ["http://printer.local/a.png", "hostname"],
    ["http://metadata.google.internal/a", "hostname"],
    ["http://127.0.0.1/a.png", "private_address"],
    ["http://[::1]/a.png", "private_address"],
    ["http://169.254.169.254/latest/meta-data", "private_address"],
    // WHATWG URL parsing turns these spellings into 127.0.0.1 first.
    ["http://0x7f000001/a.png", "private_address"],
    ["http://2130706433/a.png", "private_address"],
    ["http://0177.0.0.1/a.png", "private_address"],
    ["http://[::ffff:127.0.0.1]/a.png", "private_address"],
    ["not a url", "invalid_url"],
  ])("refuses %s (%s)", async (url, reason) => {
    const result = await checkMediaTarget(url, publicResolver);
    expect(result).toEqual({ ok: false, reason });
  });

  it("takes the caller's port policy", async () => {
    const url = "https://example.com:8443/a.png";
    expect(
      (await checkMediaTarget(url, publicResolver, new Set(["", "8443"]))).ok,
    ).toBe(true);
    expect(
      await checkMediaTarget(
        "https://example.com/a.png",
        publicResolver,
        new Set(["8443"]),
      ),
    ).toEqual({ ok: false, reason: "port" });
  });

  it("refuses a name that resolves to a private address", async () => {
    const result = await checkMediaTarget(
      "https://evil.example/a.png",
      resolverFrom({ "evil.example": ["10.0.0.5"] }),
    );
    expect(result).toEqual({ ok: false, reason: "private_address" });
  });

  it("refuses a rebinding record set that mixes public and private", async () => {
    const result = await checkMediaTarget(
      "https://rebind.example/a.png",
      resolverFrom({ "rebind.example": ["93.184.215.14", "127.0.0.1"] }),
    );
    expect(result).toEqual({ ok: false, reason: "private_address" });
  });

  it("refuses a name with a private IPv6 answer", async () => {
    const result = await checkMediaTarget(
      "https://v6.example/a.png",
      resolverFrom({ "v6.example": ["2606:4700::1", "fd00::1"] }),
    );
    expect(result).toEqual({ ok: false, reason: "private_address" });
  });

  it("refuses names that do not resolve", async () => {
    expect(
      await checkMediaTarget(
        "https://nothing.example/a.png",
        resolverFrom({ "nothing.example": [] }),
      ),
    ).toEqual({ ok: false, reason: "unresolved" });
    expect(
      await checkMediaTarget("https://broken.example/a.png", resolverFrom({})),
    ).toEqual({ ok: false, reason: "unresolved" });
  });

  it("checks a trailing-dot name like the plain one", async () => {
    const result = await checkMediaTarget(
      "http://localhost./a.png",
      publicResolver,
    );
    expect(result).toEqual({ ok: false, reason: "hostname" });
  });
});

describe("createDohResolver", () => {
  it("reads A and AAAA answers and skips CNAMEs", async () => {
    const fetchImpl = vi.fn(async (input: RequestInfo | URL) => {
      const url = new URL(String(input));
      const type = url.searchParams.get("type");
      const body =
        type === "A"
          ? {
              Status: 0,
              Answer: [
                { type: 5, data: "cdn.example.net.", TTL: 300 },
                { type: 1, data: "93.184.215.14", TTL: 120 },
              ],
            }
          : { Status: 0, Answer: [{ type: 28, data: "2606:4700::6810" }] };
      return new Response(JSON.stringify(body), {
        headers: { "Content-Type": "application/dns-json" },
      });
    });
    const resolve = createDohResolver(fetchImpl as unknown as typeof fetch);
    await expect(resolve("doh-test-1.example")).resolves.toEqual([
      "93.184.215.14",
      "2606:4700::6810",
    ]);
    expect(fetchImpl).toHaveBeenCalledTimes(2);
    // Cached for the record's TTL: no second lookup.
    await resolve("doh-test-1.example");
    expect(fetchImpl).toHaveBeenCalledTimes(2);
  });

  it("fails on SERVFAIL so the target is refused", async () => {
    const fetchImpl = vi.fn(
      async () => new Response(JSON.stringify({ Status: 2 })),
    );
    const resolve = createDohResolver(fetchImpl as unknown as typeof fetch);
    await expect(resolve("doh-test-2.example")).rejects.toThrow();
    expect(
      await checkMediaTarget("https://doh-test-2.example/a.png", resolve),
    ).toEqual({ ok: false, reason: "unresolved" });
  });
});

describe("own zones", () => {
  it.each([
    "https://115jon.com/a.png",
    "https://cdn.115jon.com/a.png",
    "https://CDN.115JON.COM/a.png",
    "https://cdn.115jon.com./a.png",
    "https://yap.115jon.com/api/attachments/x",
    "https://meet.115jon.site/a.png",
    "https://deep.sub.115jon.site./a.png",
  ])("refuses %s", async (url) => {
    await expect(checkMediaTarget(url, publicResolver)).resolves.toEqual({
      ok: false,
      reason: "hostname",
    });
  });

  it("allows names that only end like ours", async () => {
    const check = await checkMediaTarget(
      "https://not115jon.com/a.png",
      publicResolver,
    );
    expect(check.ok).toBe(true);
  });
});
