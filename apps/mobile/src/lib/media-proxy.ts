// Third-party media go through the API's /api/proxy-media, so a link someone
// posts never makes this device fetch from the link's host (which would hand
// that host the device's address). `mediaSource` sends the session's Bearer
// with it, which is what lets the proxy serve an URL the server did not sign.
// Pure: no React Native here, so it runs under vitest.

const PROXY_PATH = "/api/proxy-media";

/** `uri` as the app should fetch it, given the API's absolute base. */
export function viaMediaProxy(uri: string, apiBaseUrl: string): string {
  if (!/^https?:\/\//i.test(uri)) return uri;
  // The API's own files, and proxy URLs the server already built (signed,
  // or the social form with its post), load as they are.
  if (uri.startsWith(`${apiBaseUrl}/`)) return uri;
  // A proxy path on another host goes to our API with its query: never to
  // that host, which only has to name a path like ours.
  const match = /^https?:\/\/[^/?#]+(\/[^?#]*)(\?[^#]*)?/i.exec(uri);
  if (match?.[1] === PROXY_PATH)
    return `${apiBaseUrl}${PROXY_PATH}${match[2] ?? ""}`;
  return `${apiBaseUrl}${PROXY_PATH}?${new URLSearchParams({ url: uri })}`;
}
