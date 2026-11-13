import type { VoiceConnectionStats } from "@/lib/sfu-client";

type Clock = () => Date;

const SENSITIVE_KEY_PATTERN = /(token|authorization|secret|user[_-]?id)/i;
/**
 * ICE stats keys that hold a network address: a candidate's address, ip,
 * port-less relatedAddress, and the STUN/TURN url it came from.
 */
const ADDRESS_KEY_PATTERN = /^(address|ip|relatedAddress|url)$/i;
// Not inside a longer word or dotted run ("Chrome/144.0.0.0" is still
// masked; harmless).
const IPV4_PATTERN = /(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])/g;
// Full eight-group addresses and "::"-compressed ones (fe80::1, 2001:db8::5),
// not inside a word ("std::string" stays).
const IPV6_PATTERN =
  /(?<![\w.:])(?:(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}|(?:[0-9a-f]{1,4}:)*[0-9a-f]{0,4}::(?:[0-9a-f]{1,4}:)*[0-9a-f]{0,4})(?![\w.:])/gi;

export interface VoiceDiagnosticsBundleInput {
  detailedStats: unknown;
  connectionStats: VoiceConnectionStats | null;
  channelName?: string;
  locationHref?: string;
  userAgent?: string;
  now?: Clock;
}

export function buildVoiceDiagnosticsBundle({
  detailedStats,
  connectionStats,
  channelName,
  locationHref,
  userAgent,
  now = () => new Date(),
}: VoiceDiagnosticsBundleInput) {
  return {
    app: "Yap",
    copiedAt: now().toISOString(),
    channelName: channelName ?? null,
    page: sanitizePageUrl(locationHref),
    userAgent: userAgent ?? null,
    connectionStats: redactDiagnosticsValue(connectionStats),
    detailedStats: redactDiagnosticsValue(detailedStats),
  };
}

function sanitizePageUrl(locationHref?: string) {
  if (!locationHref) return null;
  try {
    const url = new URL(locationHref);
    return `${url.origin}${url.pathname}`;
  } catch {
    return null;
  }
}

function redactDiagnosticsValue(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.map((entry) => redactDiagnosticsValue(entry));
  }

  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value as Record<string, unknown>).map(([key, entry]) => [
        key,
        SENSITIVE_KEY_PATTERN.test(key) || ADDRESS_KEY_PATTERN.test(key)
          ? "[redacted]"
          : redactDiagnosticsValue(entry),
      ]),
    );
  }

  // An IP anywhere else, such as inside a raw candidate line.
  if (typeof value === "string") {
    return value
      .replace(IPV4_PATTERN, "[ip]")
      .replace(IPV6_PATTERN, (match) => (match === "::" ? match : "[ip]"));
  }

  return value;
}
