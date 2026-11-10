/**
 * Desktop release lookup for the landing page. The GitHub releases feed is
 * fetched after mount; until then (and if it fails) the pinned fallback is
 * shown so the download button always has a target.
 */

export interface DesktopRelease {
  tagName: string;
  name: string;
  downloadUrl: string;
  fileName: string;
  isLatest: boolean;
}

interface GithubReleaseAsset {
  name: string;
  browser_download_url: string;
}

interface GithubRelease {
  tag_name: string;
  name: string | null;
  assets?: GithubReleaseAsset[];
}

export const REPO_URL = "https://github.com/115jon/yap";
// `/releases/latest` only returns the release GitHub marks as "latest", so
// prerelease-line builds (make_latest=false) never reach the download button.
// Promoting a different release to latest (or rolling back) switches the site.
const LATEST_RELEASE_API_URL =
  "https://api.github.com/repos/115jon/yap/releases/latest";

/** Installer asset names in priority order: native first, then Tauri. */
const INSTALLER_ASSET_NAMES = ["Yap.App-win-Setup.exe", "YapSetup.exe"];

export const FALLBACK_RELEASES: DesktopRelease[] = [
  {
    tagName: "latest",
    name: "Latest",
    downloadUrl: `${REPO_URL}/releases/latest/download/YapSetup.exe`,
    fileName: "YapSetup.exe",
    isLatest: true,
  },
];

function isGithubRelease(value: unknown): value is GithubRelease {
  if (!value || typeof value !== "object") return false;
  return typeof Reflect.get(value, "tag_name") === "string";
}

/** Maps the `/releases/latest` payload, or `null` when it has no installer. */
export function mapLatestRelease(payload: unknown): DesktopRelease | null {
  if (!isGithubRelease(payload)) return null;
  const assets = Array.isArray(payload.assets) ? payload.assets : [];
  for (const wanted of INSTALLER_ASSET_NAMES) {
    const asset = assets.find((item) => item?.name === wanted);
    if (!asset) continue;
    return {
      tagName: payload.tag_name,
      name: payload.name || payload.tag_name,
      downloadUrl: asset.browser_download_url,
      fileName: asset.name,
      isLatest: true,
    };
  }
  return null;
}

/** Resolves to the latest release (one entry), or `[]` on any network or shape error. */
export async function fetchDesktopReleases(
  fetcher: typeof fetch = fetch,
): Promise<DesktopRelease[]> {
  try {
    const response = await fetcher(LATEST_RELEASE_API_URL);
    if (!response.ok) return [];
    const release = mapLatestRelease(await response.json());
    return release ? [release] : [];
  } catch {
    return [];
  }
}
