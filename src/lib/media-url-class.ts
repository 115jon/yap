// How a media URL from a payload or a user reads, the same way on the
// server and in the web app. No imports: the server's payload rewriter and
// platform.ts both use it.
//
// Browsers and URL libraries clean a URL before they read it: they drop ASCII
// tab and newline anywhere and C0 controls or spaces at either end, and read
// `\` as `/` in http(s). A check that looks at the raw string sees
// "/\t/evil.example/a.png" as a path of ours, while the browser loads
// //evil.example/a.png. So every check here runs on the cleaned string.

/** `value` as the WHATWG URL parser sees it before parsing. */
export function cleanUrlInput(value: string): string {
  return (
    value
      .replace(/[\t\n\r]/g, "")
      // C0 controls and space at either end.
      // eslint-disable-next-line no-control-regex
      .replace(/^[\u0000- ]+|[\u0000- ]+$/g, "")
  );
}

export type MediaUrlClass =
  /** A path on the host that serves the page or payload: "/api/x". */
  | { kind: "path"; value: string }
  /**
   * http(s) with a host, read the same by every parser. Protocol-relative
   * input (`//host/x`, `/\host/x`) is another host's and reads as https.
   */
  | { kind: "absolute"; url: URL }
  /**
   * Anything else: other schemes, relative paths without a leading slash,
   * unparsable input, and `https:host/x` (a browser resolves that against
   * the page, native URL libraries read `host` as the host).
   */
  | { kind: "other"; value: string };

export function classifyMediaUrl(input: string): MediaUrlClass {
  const value = cleanUrlInput(typeof input === "string" ? input : "");
  if (/^\/(?![\\/])/.test(value)) return { kind: "path", value };
  const candidate = /^[\\/]{2}/.test(value) ? `https:${value}` : value;
  if (!/^https?:[\\/]{2}/i.test(candidate)) return { kind: "other", value };
  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    return { kind: "other", value };
  }
  if (
    (url.protocol !== "https:" && url.protocol !== "http:") ||
    !url.hostname
  ) {
    return { kind: "other", value };
  }
  return { kind: "absolute", url };
}

/**
 * The deployment's own DNS zones (the current one and the pre-rename one).
 * Media there loads directly, and the proxy never fetches from them.
 */
const OWN_ZONES = ["115jon.com", "115jon.site"];

/** True for an own-zone host name, any case, with or without the root dot. */
export function isOwnZoneHostname(hostname: string): boolean {
  const name = hostname.toLowerCase().replace(/\.+$/, "");
  return OWN_ZONES.some((zone) => name === zone || name.endsWith(`.${zone}`));
}
