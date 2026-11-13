// SSRF guard for the generic media proxy: only http(s) on public addresses.
//
// Workers have no DNS API, so names resolve over DNS-over-HTTPS (A and AAAA).
// A name is refused when any answer is private, loopback, link-local or
// otherwise special, which also catches a rebinding record set that mixes a
// public and a private address. Each redirect hop is checked again.
//
// Between our lookup and the fetch the runtime resolves the name again, so a
// rebinding server could still flip the answer. Workers' fetch only reaches
// the public internet (it cannot connect to Cloudflare-internal or RFC 1918
// addresses), which closes that gap in production; the check here keeps the
// proxy honest anywhere else it runs (local dev, tests).

export type ResolveHost = (hostname: string) => Promise<string[]>;

export type TargetCheck =
  | { ok: true; url: URL }
  | { ok: false; reason: TargetRejection };

export type TargetRejection =
  | "invalid_url"
  | "scheme"
  | "credentials"
  | "port"
  | "hostname"
  | "private_address"
  | "unresolved";

const ALLOWED_PORTS = new Set(["", "80", "443", "8080", "8443"]);
const BLOCKED_SUFFIXES = [
  ".localhost",
  ".local",
  ".internal",
  ".intranet",
  ".lan",
  ".home.arpa",
  ".onion",
];

function parseIPv4(text: string): number[] | null {
  const parts = text.split(".");
  if (parts.length !== 4) return null;
  const octets = parts.map((part) =>
    /^\d{1,3}$/.test(part) ? Number(part) : Number.NaN,
  );
  return octets.every((o) => Number.isInteger(o) && o >= 0 && o <= 255)
    ? octets
    : null;
}

function isBlockedIPv4([a, b, c]: number[]): boolean {
  return (
    a === 0 || // "this" network
    a === 10 || // private
    a === 127 || // loopback
    (a === 100 && b >= 64 && b <= 127) || // carrier-grade NAT
    (a === 169 && b === 254) || // link-local, cloud metadata
    (a === 172 && b >= 16 && b <= 31) || // private
    (a === 192 && b === 0 && c === 0) || // IETF protocol assignments
    (a === 192 && b === 0 && c === 2) || // TEST-NET-1
    (a === 192 && b === 88 && c === 99) || // 6to4 relay anycast
    (a === 192 && b === 168) || // private
    (a === 198 && (b === 18 || b === 19)) || // benchmarking
    (a === 198 && b === 51 && c === 100) || // TEST-NET-2
    (a === 203 && b === 0 && c === 113) || // TEST-NET-3
    a >= 224 // multicast, reserved, broadcast
  );
}

/** Eight 16-bit groups, or null when `text` is not an IPv6 address. */
function parseIPv6(text: string): number[] | null {
  let value = text.toLowerCase();
  if (value.startsWith("[") && value.endsWith("]")) value = value.slice(1, -1);
  const zone = value.indexOf("%");
  if (zone >= 0) value = value.slice(0, zone);
  if (!value.includes(":")) return null;

  // An embedded IPv4 tail (::ffff:1.2.3.4) becomes two groups.
  const lastColon = value.lastIndexOf(":");
  const tail = value.slice(lastColon + 1);
  if (tail.includes(".")) {
    const v4 = parseIPv4(tail);
    if (!v4) return null;
    value = `${value.slice(0, lastColon + 1)}${((v4[0] << 8) | v4[1]).toString(16)}:${((v4[2] << 8) | v4[3]).toString(16)}`;
  }

  const halves = value.split("::");
  if (halves.length > 2) return null;
  const parseGroups = (part: string) =>
    part === ""
      ? []
      : part
          .split(":")
          .map((g) =>
            /^[0-9a-f]{1,4}$/.test(g) ? parseInt(g, 16) : Number.NaN,
          );
  const head = parseGroups(halves[0]);
  const rest = halves.length === 2 ? parseGroups(halves[1]) : [];
  if ([...head, ...rest].some((g) => Number.isNaN(g))) return null;
  if (halves.length === 1) return head.length === 8 ? head : null;
  const missing = 8 - head.length - rest.length;
  if (missing < 1) return null;
  return [...head, ...new Array<number>(missing).fill(0), ...rest];
}

function embeddedIPv4(high: number, low: number): number[] {
  return [high >> 8, high & 0xff, low >> 8, low & 0xff];
}

function isBlockedIPv6(groups: number[]): boolean {
  const [g0, g1, g2, g3, g4, g5, g6, g7] = groups;
  const allZeroUntil = (n: number) => groups.slice(0, n).every((g) => g === 0);

  if (allZeroUntil(7) && (g7 === 0 || g7 === 1)) return true; // :: and ::1
  // IPv4-mapped (::ffff:a.b.c.d) and IPv4-compatible (::a.b.c.d).
  if (allZeroUntil(5) && (g5 === 0xffff || g5 === 0)) {
    return isBlockedIPv4(embeddedIPv4(g6, g7));
  }
  // IPv4-translated (::ffff:0:a.b.c.d).
  if (allZeroUntil(4) && g4 === 0xffff && g5 === 0) {
    return isBlockedIPv4(embeddedIPv4(g6, g7));
  }
  // Local-use NAT64 (64:ff9b:1::/48): site-specific, never ours to reach.
  if (g0 === 0x64 && g1 === 0xff9b && g2 === 1) return true;
  // NAT64 (64:ff9b::/96): the IPv4 address it reaches decides.
  if (
    g0 === 0x64 &&
    g1 === 0xff9b &&
    g2 === 0 &&
    g3 === 0 &&
    g4 === 0 &&
    g5 === 0
  ) {
    return isBlockedIPv4(embeddedIPv4(g6, g7));
  }
  // 6to4 (2002::/16) embeds an IPv4 address in groups 1-2.
  if (g0 === 0x2002) return isBlockedIPv4(embeddedIPv4(g1, g2));
  if ((g0 & 0xfe00) === 0xfc00) return true; // unique local fc00::/7
  if ((g0 & 0xffc0) === 0xfe80) return true; // link-local fe80::/10
  if ((g0 & 0xffc0) === 0xfec0) return true; // site-local (deprecated)
  if ((g0 & 0xff00) === 0xff00) return true; // multicast
  if (g0 === 0x2001 && g1 === 0x0db8) return true; // documentation
  if (g0 === 0x2001 && g1 === 0) return true; // Teredo
  if (g0 === 0x0100 && g1 === 0 && g2 === 0 && g3 === 0) return true; // discard
  return false;
}

/** True for an address the proxy must never fetch from. */
export function isBlockedAddress(address: string): boolean {
  const v4 = parseIPv4(address);
  if (v4) return isBlockedIPv4(v4);
  const v6 = parseIPv6(address);
  if (v6) return isBlockedIPv6(v6);
  // Not an address we can classify: refuse.
  return true;
}

function isIpLiteral(hostname: string): boolean {
  return parseIPv4(hostname) !== null || parseIPv6(hostname) !== null;
}

/**
 * Checks `rawUrl` before a fetch: http(s) only, no credentials, a usual port,
 * a public name, and every A/AAAA answer public.
 */
export async function checkMediaTarget(
  rawUrl: string | URL,
  resolve: ResolveHost,
): Promise<TargetCheck> {
  let url: URL;
  try {
    url = typeof rawUrl === "string" ? new URL(rawUrl) : new URL(rawUrl.href);
  } catch {
    return { ok: false, reason: "invalid_url" };
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    return { ok: false, reason: "scheme" };
  }
  if (url.username || url.password) {
    return { ok: false, reason: "credentials" };
  }
  if (!ALLOWED_PORTS.has(url.port)) return { ok: false, reason: "port" };

  const hostname = url.hostname.toLowerCase().replace(/\.$/, "");
  if (!hostname) return { ok: false, reason: "hostname" };

  if (isIpLiteral(hostname)) {
    return isBlockedAddress(hostname)
      ? { ok: false, reason: "private_address" }
      : { ok: true, url };
  }

  if (
    hostname === "localhost" ||
    !hostname.includes(".") ||
    BLOCKED_SUFFIXES.some((suffix) => hostname.endsWith(suffix))
  ) {
    return { ok: false, reason: "hostname" };
  }

  let addresses: string[];
  try {
    addresses = await resolve(hostname);
  } catch {
    return { ok: false, reason: "unresolved" };
  }
  if (addresses.length === 0) return { ok: false, reason: "unresolved" };
  if (addresses.some(isBlockedAddress)) {
    return { ok: false, reason: "private_address" };
  }
  return { ok: true, url };
}

// ── DNS over HTTPS ─────────────────────────────────────────────────────────

const DOH_ENDPOINT = "https://cloudflare-dns.com/dns-query";
const DNS_CACHE_MAX = 500;
const DNS_CACHE_MIN_TTL_MS = 30_000;
const DNS_CACHE_MAX_TTL_MS = 5 * 60_000;
const DNS_TIMEOUT_MS = 3_000;

interface DnsCacheEntry {
  addresses: string[];
  expiresAt: number;
}

const dnsCache = new Map<string, DnsCacheEntry>();

interface DohAnswer {
  type?: number;
  data?: string;
  TTL?: number;
}

async function queryDoh(
  hostname: string,
  type: "A" | "AAAA",
  fetchImpl: typeof fetch,
): Promise<{ addresses: string[]; ttl: number }> {
  const url = `${DOH_ENDPOINT}?name=${encodeURIComponent(hostname)}&type=${type}`;
  const response = await fetchImpl(url, {
    headers: { Accept: "application/dns-json" },
    signal: AbortSignal.timeout(DNS_TIMEOUT_MS),
  });
  if (!response.ok) throw new Error(`DoH ${type} failed: ${response.status}`);
  const body = (await response.json()) as {
    Status?: number;
    Answer?: DohAnswer[];
  };
  // NXDOMAIN and friends: no addresses. SERVFAIL: an error.
  if (body.Status === 2) throw new Error("DoH SERVFAIL");
  const wanted = type === "A" ? 1 : 28;
  const answers = (body.Answer ?? []).filter(
    (answer) => answer.type === wanted && typeof answer.data === "string",
  );
  const ttl = Math.min(
    ...answers.map((answer) => (answer.TTL ?? 60) * 1000),
    DNS_CACHE_MAX_TTL_MS,
  );
  return { addresses: answers.map((answer) => answer.data as string), ttl };
}

/** A resolver over Cloudflare's DoH JSON API, cached per isolate. */
export function createDohResolver(fetchImpl?: typeof fetch): ResolveHost {
  return async (hostname) => {
    const now = Date.now();
    const cached = dnsCache.get(hostname);
    if (cached && cached.expiresAt > now) return cached.addresses;

    const doFetch = fetchImpl ?? fetch;
    const [v4, v6] = await Promise.all([
      queryDoh(hostname, "A", doFetch),
      queryDoh(hostname, "AAAA", doFetch),
    ]);
    const addresses = [...v4.addresses, ...v6.addresses];
    const ttl = Math.max(DNS_CACHE_MIN_TTL_MS, Math.min(v4.ttl, v6.ttl));
    if (dnsCache.size >= DNS_CACHE_MAX) {
      const oldest = dnsCache.keys().next().value;
      if (oldest !== undefined) dnsCache.delete(oldest);
    }
    dnsCache.set(hostname, { addresses, expiresAt: now + ttl });
    return addresses;
  };
}
