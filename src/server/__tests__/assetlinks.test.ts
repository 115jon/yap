import { describe, expect, it } from "vitest";

import {
  ASSET_LINKS,
  ASSET_LINKS_PATH,
  assetLinksResponse,
  handleAssetLinksRequest,
} from "../assetlinks";

function fingerprintsOf(packageName: string): string[] {
  const found = ASSET_LINKS.find(
    (entry) => entry.target.package_name === packageName,
  );
  return found?.target.sha256_cert_fingerprints ?? [];
}

describe("assetlinks", () => {
  it("serves JSON with a modest cache lifetime", async () => {
    const response = assetLinksResponse();

    expect(response.headers.get("Content-Type")).toBe("application/json");
    expect(response.headers.get("Cache-Control")).toBe("public, max-age=3600");
    expect(await response.json()).toEqual(ASSET_LINKS);
  });

  it("delegates handle_all_urls for the Qt app only", () => {
    expect(ASSET_LINKS).toHaveLength(1);
    for (const entry of ASSET_LINKS) {
      expect(entry.relation).toEqual([
        "delegate_permission/common.handle_all_urls",
      ]);
      expect(entry.target.namespace).toBe("android_app");
    }
  });

  it("trusts only its own release key for the Qt app", () => {
    expect(fingerprintsOf("dev.yap.app")).toEqual([
      "75:E0:D8:E0:22:4A:9F:32:D8:CC:40:C7:B9:5A:E7:82:43:B3:97:21:5A:99:C1:FE:D6:01:50:FC:6E:FF:B8:9C",
    ]);
  });

  it("answers GET and HEAD on its path only", () => {
    const at = (path: string, method = "GET") =>
      handleAssetLinksRequest(
        new Request(`https://meet.test${path}`, { method }),
      );

    expect(at(ASSET_LINKS_PATH)?.status).toBe(200);
    expect(at(ASSET_LINKS_PATH, "HEAD")?.status).toBe(200);
    expect(at(ASSET_LINKS_PATH, "POST")).toBeNull();
    expect(at("/.well-known/other.json")).toBeNull();
  });
});
