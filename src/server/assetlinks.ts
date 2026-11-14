// Android App Links: /.well-known/assetlinks.json tells Android which app
// builds may open this site's links. Served from the Worker rather than
// public/ because the route tree and static pipeline can skip dot-directories.

const HANDLE_ALL_URLS = ["delegate_permission/common.handle_all_urls"];

/** SHA-256 of the Qt Android app's (dev.yap.app) release signing key. */
const QT_RELEASE_KEY =
  "75:E0:D8:E0:22:4A:9F:32:D8:CC:40:C7:B9:5A:E7:82:43:B3:97:21:5A:99:C1:FE:D6:01:50:FC:6E:FF:B8:9C";

function statement(packageName: string, fingerprints: readonly string[]) {
  return {
    relation: HANDLE_ALL_URLS,
    target: {
      namespace: "android_app",
      package_name: packageName,
      sha256_cert_fingerprints: [...fingerprints],
    },
  };
}

export const ASSET_LINKS_PATH = "/.well-known/assetlinks.json";

export const ASSET_LINKS = [statement("dev.yap.app", [QT_RELEASE_KEY])];

export function assetLinksResponse(): Response {
  return new Response(JSON.stringify(ASSET_LINKS), {
    headers: {
      "Content-Type": "application/json",
      "Cache-Control": "public, max-age=3600",
    },
  });
}

/** The assetlinks answer for a GET or HEAD to its path, else null. */
export function handleAssetLinksRequest(request: Request): Response | null {
  if (request.method !== "GET" && request.method !== "HEAD") return null;
  if (new URL(request.url).pathname !== ASSET_LINKS_PATH) return null;
  return assetLinksResponse();
}
