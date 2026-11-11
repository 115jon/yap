import { describe, expect, it } from "vitest";

import { describeSessionClient } from "@/lib/session-activity";

const CHROME_WINDOWS =
  "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36";
const EDGE_WINDOWS = `${CHROME_WINDOWS} Edg/142.0.0.0`;
const SAFARI_IPHONE =
  "Mozilla/5.0 (iPhone; CPU iPhone OS 18_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.1 Mobile/15E148 Safari/604.1";
const CHROME_ANDROID =
  "Mozilla/5.0 (Linux; Android 15; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Mobile Safari/537.36";
const FIREFOX_MAC =
  "Mozilla/5.0 (Macintosh; Intel Mac OS X 14.5; rv:133.0) Gecko/20100101 Firefox/133.0";

function title(info: Parameters<typeof describeSessionClient>[0]) {
  const activity = describeSessionClient(info);
  return `${activity.deviceType} · ${activity.browserName}`;
}

describe("describeSessionClient", () => {
  it("names the browser and OS of a web session", () => {
    expect(title({ userAgent: CHROME_WINDOWS })).toBe(
      "Desktop · Chrome on Windows",
    );
    expect(title({ userAgent: EDGE_WINDOWS })).toBe(
      "Desktop · Edge on Windows",
    );
    expect(title({ userAgent: FIREFOX_MAC })).toBe(
      "Desktop · Firefox on macOS",
    );
    expect(title({ userAgent: SAFARI_IPHONE })).toBe("Mobile · Safari on iOS");
    expect(title({ userAgent: CHROME_ANDROID })).toBe(
      "Mobile · Chrome on Android",
    );
  });

  it("names the Yap apps by their HTTP clients", () => {
    expect(title({ userAgent: "YapMobile/0.6.0" })).toBe(
      "Mobile · Yap mobile app",
    );
    expect(title({ userAgent: "YapNative/0.6.0" })).toBe(
      "Desktop · Yap desktop app",
    );
    expect(title({ userAgent: "okhttp/4.12.0" })).toBe(
      "Mobile · Yap mobile app on Android",
    );
    expect(
      title({ userAgent: "Yap/42 CFNetwork/1568.200.51 Darwin/24.1.0" }),
    ).toBe("Mobile · Yap mobile app on iOS");
  });

  it("prefers what a device sign-in reported", () => {
    const activity = describeSessionClient({
      userAgent: "YapMobile/0.6.0",
      platform: "android",
      deviceName: " Pixel 8 ",
    });

    expect(activity).toMatchObject({
      deviceType: "Mobile",
      isMobile: true,
      browserName: "Yap mobile app on Android",
      appName: "Yap mobile app",
      browser: null,
      osName: "Android",
      deviceName: "Pixel 8",
    });
    expect(title({ userAgent: "YapNative/0.6.0", platform: "windows" })).toBe(
      "Desktop · Yap desktop app on Windows",
    );
  });

  it("does not read Windows tokens as ChromeOS or a tablet", () => {
    expect(
      title({
        userAgent:
          "Mozilla/5.0 (Windows NT 10.0; Win64; x64; Microsoft; Surface) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36",
      }),
    ).toBe("Desktop · Chrome on Windows");
    expect(
      title({
        userAgent:
          "Mozilla/5.0 (Windows NT 10.0; Win64; x64; Tablet PC 2.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36",
      }),
    ).toBe("Desktop · Chrome on Windows");
    expect(
      title({
        userAgent:
          "Mozilla/5.0 (X11; CrOS x86_64 14541.0.0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/142.0.0.0 Safari/537.36",
      }),
    ).toBe("Desktop · Chrome on ChromeOS");
  });

  it("falls back to the old generic title for an unknown client", () => {
    expect(title({ userAgent: null })).toBe("Desktop · Desktop client");
    expect(title({ userAgent: "curl/8.9.0" })).toBe("Desktop · Desktop client");
  });

  it("passes the location through, blanks as null", () => {
    expect(
      describeSessionClient({
        userAgent: CHROME_WINDOWS,
        city: "Austin",
        country: " ",
      }),
    ).toMatchObject({ city: "Austin", country: null });
  });
});
