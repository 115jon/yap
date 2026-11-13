// Third-party media go through the API's /api/proxy-media, so a link someone
// posts never makes this device fetch from the link's host (which would hand
// that host the device's address). `mediaSource` sends the session's Bearer
// with it, which is what lets the proxy serve an URL the server did not sign.
// Pure: no React Native here, so it runs under vitest.

const PROXY_PATH = "/api/proxy-media";

/**
 * `uri` as an http(s) URL with its host after the `//`, the way URL
 * libraries read it, or null when it names no web host. Cleaned first of
 * what they ignore (tab and newline anywhere, C0 controls and spaces at the
 * ends: OkHttp trims those too), and lenient like them about the slashes:
 * `//host/x`, `\\host/x` and `https:host/x` all name `host`.
 */
export function webUrlOf(uri: string): string | null {
  const value = uri
    .replace(/[\t\n\r]/g, "")
    // eslint-disable-next-line no-control-regex
    .replace(/^[\u0000- ]+|[\u0000- ]+$/g, "");
  let scheme = "https";
  let rest = value;
  if (!/^[\\/]{2}/.test(value)) {
    const match = /^(https?):(.*)$/i.exec(value);
    if (!match?.[1]) return null;
    scheme = match[1].toLowerCase();
    rest = match[2] ?? "";
  }
  rest = rest.replace(/^[\\/]+/, "");
  if (!rest || /^[?#]/.test(rest)) return null;
  return `${scheme}://${rest}`;
}

/** `uri` as the app should fetch it, given the API's absolute base. */
export function viaMediaProxy(uri: string, apiBaseUrl: string): string {
  const web = webUrlOf(uri);
  if (!web) return uri;
  // The API's own files, and proxy URLs the server already built (signed,
  // or the social form with its post), load as they are.
  if (web.startsWith(`${apiBaseUrl}/`)) return web;
  // A proxy path on another host goes to our API with its query: never to
  // that host, which only has to name a path like ours.
  const match = /^https?:\/\/[^/?#]+(\/[^?#]*)(\?[^#]*)?/i.exec(web);
  if (match?.[1] === PROXY_PATH)
    return `${apiBaseUrl}${PROXY_PATH}${match[2] ?? ""}`;
  return `${apiBaseUrl}${PROXY_PATH}?${new URLSearchParams({ url: web })}`;
}
