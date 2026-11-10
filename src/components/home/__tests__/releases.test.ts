import { describe, expect, it } from "vitest";
import {
  FALLBACK_RELEASES,
  fetchDesktopReleases,
  mapLatestRelease,
} from "@/components/home/releases";

const TAURI = {
  name: "YapSetup.exe",
  browser_download_url: "https://x/YapSetup.exe",
};
const NATIVE = {
  name: "Yap.App-win-Setup.exe",
  browser_download_url: "https://x/Yap.App-win-Setup.exe",
};
const NOTES = {
  name: "notes.txt",
  browser_download_url: "https://x/notes.txt",
};

function respond(body: unknown, ok = true): typeof fetch {
  return (() =>
    Promise.resolve({ ok, json: async () => body })) as unknown as typeof fetch;
}

describe("releases", () => {
  it("serves the Tauri installer from a latest release that only has one", async () => {
    const payload = { tag_name: "v3.13.0", name: null, assets: [NOTES, TAURI] };
    expect(mapLatestRelease(payload)).toEqual({
      tagName: "v3.13.0",
      name: "v3.13.0",
      downloadUrl: "https://x/YapSetup.exe",
      fileName: "YapSetup.exe",
      isLatest: true,
    });
    expect(await fetchDesktopReleases(respond(payload))).toEqual([
      mapLatestRelease(payload),
    ]);
  });

  it("serves the native installer from a latest release that only has one", () => {
    expect(
      mapLatestRelease({
        tag_name: "desktop-native-v4.0.0",
        name: "Yap 4.0",
        assets: [NATIVE],
      }),
    ).toEqual({
      tagName: "desktop-native-v4.0.0",
      name: "Yap 4.0",
      downloadUrl: "https://x/Yap.App-win-Setup.exe",
      fileName: "Yap.App-win-Setup.exe",
      isLatest: true,
    });
  });

  it("prefers the native installer when both are present", () => {
    const mapped = mapLatestRelease({
      tag_name: "v4.0.0",
      name: "v4.0.0",
      assets: [TAURI, NOTES, NATIVE],
    });
    expect(mapped?.fileName).toBe("Yap.App-win-Setup.exe");
    expect(mapped?.downloadUrl).toBe("https://x/Yap.App-win-Setup.exe");
  });

  it("returns nothing for a release without a matching installer, keeping the fallback", async () => {
    const payload = {
      tag_name: "v5.0.0",
      name: "v5.0.0",
      assets: [
        NOTES,
        { name: "Other.exe", browser_download_url: "https://x/o" },
      ],
    };
    expect(mapLatestRelease(payload)).toBeNull();
    expect(mapLatestRelease({ tag_name: "v5.0.0" })).toBeNull();
    expect(mapLatestRelease([])).toBeNull();
    expect(await fetchDesktopReleases(respond(payload))).toEqual([]);
    expect(FALLBACK_RELEASES[0]?.downloadUrl).toBe(
      "https://github.com/115jon/yap/releases/latest/download/YapSetup.exe",
    );
  });

  it("resolves to [] on a fetch error or non-OK response so the fallback stays", async () => {
    const failing = (() =>
      Promise.reject(new Error("offline"))) as typeof fetch;
    expect(await fetchDesktopReleases(failing)).toEqual([]);
    expect(await fetchDesktopReleases(respond({}, false))).toEqual([]);
  });

  it("requests the latest-release endpoint", async () => {
    const urls: string[] = [];
    const spy = ((url: string) => {
      urls.push(url);
      return Promise.resolve({ ok: false, json: async () => ({}) });
    }) as unknown as typeof fetch;
    await fetchDesktopReleases(spy);
    expect(urls).toEqual([
      "https://api.github.com/repos/115jon/yap/releases/latest",
    ]);
  });
});
