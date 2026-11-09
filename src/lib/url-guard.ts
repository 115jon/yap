// ── url-guard ───────────────────────────────────────────────────────────────
// SSRF guard for server-side fetches of URLs that users typed (link previews,
// composer prefetch). Only public http(s) URLs on the standard web ports get
// through: no credentials in the URL, no loopback / private / link-local /
// CGNAT / multicast IP literals, no single-label or local-only hostnames.
// Redirects are followed by hand so every hop is checked again, and the
// number of hops is capped.
//
// Workers cannot reach private address space anyway, so this mostly matters
// for local dev (where the Worker runs on the developer's machine) and as
// defence in depth. DNS names that resolve to private addresses are not
// caught here: the runtime gives us no resolver, and Cloudflare's egress does
// not route them.
//
// Requests made through `guardedFetch` never carry the user's cookies or
// tokens: callers build their headers from scratch.

import { safeFetch, type SafeFetchOptions } from "@/lib/safe-fetch";

/** Hops followed after the first request. */
export const MAX_PREVIEW_REDIRECTS = 4;

const ALLOWED_PORTS = new Set(["", "80", "443", "8080", "8443"]);

const BLOCKED_HOST_SUFFIXES = [
  ".localhost",
  ".local",
  ".internal",
  ".intranet",
  ".lan",
  ".home",
  ".corp",
  ".home.arpa",
  ".in-addr.arpa",
  ".ip6.arpa",
];

function parseIPv4(hostname: string): number[] | null {
  // WHATWG URL parsing already turned decimal, hex and octal forms
  // (`2130706433`, `0x7f.1`) into dotted quads.
  const match = hostname.match(/^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})$/);
  if (!match) return null;
  const parts = match.slice(1).map(Number);
  return parts.every((part) => part <= 255) ? parts : null;
}

function isPrivateIPv4([a, b, c]: number[]): boolean {
  return (
    a === 0 || // "this network"
    a === 10 ||
    a === 127 ||
    (a === 100 && b >= 64 && b <= 127) || // CGNAT
    (a === 169 && b === 254) || // link-local, cloud metadata
    (a === 172 && b >= 16 && b <= 31) ||
    (a === 192 && b === 0) || // IETF protocol assignments, TEST-NET-1
    (a === 192 && b === 168) ||
    (a === 198 && (b === 18 || b === 19)) || // benchmarking
    (a === 198 && b === 51 && c === 100) || // TEST-NET-2
    (a === 203 && b === 0 && c === 113) || // TEST-NET-3
    a >= 224 // multicast, reserved, broadcast
  );
}

function expandIPv6(hostname: string): number[] | null {
  let host = hostname.replace(/^\[|\]$/g, "").toLowerCase();
  if (!host.includes(":")) return null;
  const zone = host.indexOf("%");
  if (zone >= 0) host = host.slice(0, zone);

  // An embedded dotted quad (`::ffff:1.2.3.4`) becomes two hextets.
  const v4 = host.match(/(\d+\.\d+\.\d+\.\d+)$/);
  if (v4) {
    const parts = parseIPv4(v4[1]);
    if (!parts) return null;
    host = `${host.slice(0, -v4[1].length)}${((parts[0] << 8) | parts[1]).toString(16)}:${((parts[2] << 8) | parts[3]).toString(16)}`;
  }

  const [head, tail, ...rest] = host.split("::");
  if (rest.length > 0) return null;
  const headParts = head ? head.split(":") : [];
  const tailParts = tail !== undefined && tail ? tail.split(":") : [];
  const missing = 8 - headParts.length - tailParts.length;
  if (tail === undefined ? missing !== 0 : missing < 1) return null;
  const hextets = [
    ...headParts,
    ...Array.from({ length: tail === undefined ? 0 : missing }, () => "0"),
    ...tailParts,
  ].map((part) =>
    /^[0-9a-f]{1,4}$/.test(part) ? Number.parseInt(part, 16) : NaN,
  );
  return hextets.length === 8 && hextets.every(Number.isFinite)
    ? hextets
    : null;
}

function isPrivateIPv6(hextets: number[]): boolean {
  const [first] = hextets;
  const allZeroBefore = (index: number) =>
    hextets.slice(0, index).every((part) => part === 0);
  // :: and ::1
  if (allZeroBefore(7) && (hextets[7] === 0 || hextets[7] === 1)) return true;
  // IPv4-mapped (::ffff:a.b.c.d) and IPv4-compatible (::a.b.c.d)
  if (allZeroBefore(5) && (hextets[5] === 0xffff || hextets[5] === 0)) {
    return isPrivateIPv4([
      hextets[6] >> 8,
      hextets[6] & 0xff,
      hextets[7] >> 8,
      hextets[7] & 0xff,
    ]);
  }
  return (
    (first & 0xfe00) === 0xfc00 || // unique local fc00::/7
    (first & 0xffc0) === 0xfe80 || // link-local fe80::/10
    (first & 0xff00) === 0xff00 || // multicast
    first === 0x64 || // 64:ff9b::/96 NAT64 can reach private v4
    first === 0x2002 || // 6to4 wraps any v4 address
    (first === 0x2001 && hextets[1] === 0) || // Teredo
    (first === 0x2001 && hextets[1] === 0x0db8) // documentation
  );
}

/** Why a URL may not be fetched, or `null` when it may. */
export function previewUrlBlockReason(rawUrl: string | URL): string | null {
  let url: URL;
  try {
    url = typeof rawUrl === "string" ? new URL(rawUrl) : rawUrl;
  } catch {
    return "invalid-url";
  }
  if (url.protocol !== "https:" && url.protocol !== "http:") {
    return "protocol";
  }
  if (url.username || url.password) return "credentials";
  if (!ALLOWED_PORTS.has(url.port)) return "port";

  const hostname = url.hostname.toLowerCase().replace(/\.$/, "");
  if (!hostname) return "host";

  const v6 = expandIPv6(hostname);
  if (v6) return isPrivateIPv6(v6) ? "private-address" : null;
  if (hostname.startsWith("[")) return "host";

  const v4 = parseIPv4(hostname);
  if (v4) return isPrivateIPv4(v4) ? "private-address" : null;

  if (
    hostname === "localhost" ||
    BLOCKED_HOST_SUFFIXES.some((suffix) => hostname.endsWith(suffix))
  ) {
    return "local-host";
  }
  if (!hostname.includes(".")) return "single-label-host";
  return null;
}

export function isFetchablePreviewUrl(rawUrl: string | URL): boolean {
  return previewUrlBlockReason(rawUrl) === null;
}

/** Thrown when a URL (or a redirect hop) fails {@link previewUrlBlockReason}. */
export class BlockedUrlError extends Error {
  constructor(
    readonly reason: string,
    readonly url: string,
  ) {
    super(`Refusing to fetch ${reason} URL`);
    this.name = "BlockedUrlError";
  }
}

/** Thrown when an upstream keeps redirecting past the hop limit. */
export class TooManyRedirectsError extends Error {
  constructor(readonly url: string) {
    super("Too many redirects");
    this.name = "TooManyRedirectsError";
  }
}

const REDIRECT_STATUSES = new Set([301, 302, 303, 307, 308]);

export interface GuardedFetchResult {
  response: Response;
  /** The URL that produced `response`, after redirects. */
  finalUrl: string;
}

/**
 * GET (or HEAD) `url` with every redirect hop checked by
 * {@link previewUrlBlockReason}. `init.redirect` is ignored.
 */
export async function guardedFetch(
  url: string,
  init: Omit<RequestInit, "redirect"> = {},
  options: SafeFetchOptions & { maxRedirects?: number } = {},
): Promise<GuardedFetchResult> {
  const maxRedirects = options.maxRedirects ?? MAX_PREVIEW_REDIRECTS;
  let current = url;
  for (let hop = 0; ; hop += 1) {
    const reason = previewUrlBlockReason(current);
    if (reason) throw new BlockedUrlError(reason, current);

    const response = await safeFetch(
      current,
      { ...init, redirect: "manual" },
      options,
    );
    const location = response.headers.get("location");
    if (!REDIRECT_STATUSES.has(response.status) || !location) {
      return { response, finalUrl: response.url || current };
    }
    await response.body?.cancel().catch(() => {});
    if (hop >= maxRedirects) throw new TooManyRedirectsError(current);
    try {
      current = new URL(location, current).toString();
    } catch {
      throw new BlockedUrlError("invalid-redirect", location);
    }
  }
}
