/**
 * A URL's host, for logs. User-posted URLs are logged by host only: their
 * paths and query strings can carry tokens and say what someone shared.
 * Anything that does not parse logs as "invalid-url".
 */
export function logHost(url: string | URL | null | undefined): string | null {
  if (url === null || url === undefined || url === "") return null;
  try {
    return (url instanceof URL ? url : new URL(url)).hostname || "invalid-url";
  } catch {
    return "invalid-url";
  }
}
