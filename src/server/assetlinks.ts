// Android App Links: /.well-known/assetlinks.json tells Android which app
// builds may open this site's links. Served from the Worker rather than
// public/ because the route tree and static pipeline can skip dot-directories.

const HANDLE_ALL_URLS = ["delegate_permission/common.handle_all_urls"];

/** SHA-256 of the Expo app's release signing key (its three packages). */
const RELEASE_KEY =
  "3E:32:29:17:07:BB:C3:1F:09:CD:0C:8D:DD:4A:B8:5C:17:63:E4:C8:85:8B:91:08:57:AB:89:04:54:4F:1E:EF";
/**
 * React Native's shared template debug key (public: every project built from
 * the template has it), which is why only the dev and preview packages list
 * it. It is the android/app/debug.keystore local debug builds use.
 */
const TEMPLATE_DEBUG_KEY =
  "FA:C6:17:45:DC:09:03:78:6F:B9:ED:E6:2A:96:2B:39:9F:73:48:F0:BB:6F:89:9B:83:32:66:75:91:03:3B:9C";
/** SHA-256 of the Qt Android app's (dev.yap.app) release signing key. */
const QT_RELEASE_KEY =
  "75:E0:D8:E0:22:4A:9F:32:D8:CC:40:C7:B9:5A:E7:82:43:B3:97:21:5A:99:C1:FE:D6:01:50:FC:6E:FF:B8:9C";
/** The developer's own ~/.android debug key. */
const DEVELOPER_DEBUG_KEY =
  "9C:92:28:44:7C:A5:10:72:AE:CB:21:73:7F:D3:8A:6A:4C:00:70:93:5F:FE:88:C7:C3:45:BB:97:D5:07:38:B8";

const NON_PRODUCTION_KEYS = [
  RELEASE_KEY,
  TEMPLATE_DEBUG_KEY,
  DEVELOPER_DEBUG_KEY,
];

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

export const ASSET_LINKS = [
  statement("dev.jontitor.ralphmeet", [RELEASE_KEY]),
  statement("dev.jontitor.ralphmeet.preview", NON_PRODUCTION_KEYS),
  statement("dev.jontitor.ralphmeet.dev", NON_PRODUCTION_KEYS),
  statement("dev.yap.app", [QT_RELEASE_KEY]),
];

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
