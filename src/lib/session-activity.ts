// What a signed-in session is running on, for the Devices list
// (GET /api/sessions). Kova keeps one User-Agent per session: the browser's
// for a web sign-in, the app's HTTP client's for a device sign-in (the Qt
// apps send `YapNative/<version>` and `YapMobile/<version>`, see
// native/rust/crates/yap-core/src/api.rs and
// native/mobile/rust/src/platform.rs). When Kova also stores what the
// device sign-in reported (platform, hostname), that wins over the guess.
//
// The shape is the one the clients already read (`deviceType`,
// `browserName`, `isMobile`, `city`, `country`); the rest is additive.

export type SessionDeviceType = "Desktop" | "Mobile" | "Tablet";

export interface SessionActivity {
  /** "Desktop", "Mobile" or "Tablet". */
  deviceType: SessionDeviceType;
  isMobile: boolean;
  /**
   * The row's title after the device type: the browser or app, and the OS
   * when known ("Chrome on Windows", "Yap mobile app on Android").
   */
  browserName: string;
  /** The browser alone ("Chrome"), or null for an app. */
  browser: string | null;
  /** "Yap desktop app" / "Yap mobile app", or null for a browser. */
  appName: string | null;
  /** "Windows", "macOS", "Linux", "ChromeOS", "Android", "iOS", or null. */
  osName: string | null;
  /** The device's own name, from a device sign-in, when Kova kept it. */
  deviceName: string | null;
  city: string | null;
  country: string | null;
}

/** What is known about one session's client. All optional. */
export interface SessionClientInfo {
  userAgent?: string | null;
  /** The device sign-in's `platform` (`windows`, `android`, ...). */
  platform?: string | null;
  /** The device sign-in's `hostname`. */
  deviceName?: string | null;
  city?: string | null;
  country?: string | null;
}

const PLATFORM_OS = new Map([
  ["windows", "Windows"],
  ["win32", "Windows"],
  ["macos", "macOS"],
  ["darwin", "macOS"],
  ["mac", "macOS"],
  ["linux", "Linux"],
  ["android", "Android"],
  ["ios", "iOS"],
]);

const MOBILE_OS = new Set(["Android", "iOS"]);

function detectOs(ua: string): string | null {
  if (/iP(?:hone|od|ad)/i.test(ua) || /\b(?:iOS|iPadOS)\b/i.test(ua))
    return "iOS";
  if (/Android/i.test(ua)) return "Android";
  // Case-sensitive: "Microsoft" contains "cros".
  if (/\bCrOS\b/.test(ua)) return "ChromeOS";
  if (/Windows/i.test(ua)) return "Windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "macOS";
  if (/Linux|X11/i.test(ua)) return "Linux";
  return null;
}

function detectBrowser(ua: string): string | null {
  if (/Edg(?:e|A|iOS)?\//i.test(ua)) return "Edge";
  if (/OPR\/|Opera/i.test(ua)) return "Opera";
  if (/SamsungBrowser\//i.test(ua)) return "Samsung Internet";
  if (/Vivaldi\//i.test(ua)) return "Vivaldi";
  if (/Firefox\/|FxiOS\//i.test(ua)) return "Firefox";
  if (/CriOS\/|Chrome\/|Chromium\//i.test(ua)) return "Chrome";
  if (/Version\/[\d.]+.*Safari\//i.test(ua)) return "Safari";
  return null;
}

/** The Yap app the User-Agent names, if it is one of ours. */
function detectApp(ua: string): { mobile: boolean } | null {
  if (/^YapMobile\//i.test(ua)) return { mobile: true };
  if (/^YapNative\//i.test(ua)) return { mobile: false };
  return null;
}

function detectDeviceType(
  ua: string,
  os: string | null,
  appMobile: boolean | null,
): SessionDeviceType {
  if (/iPad/.test(ua)) return "Tablet";
  // Not Windows: its "Tablet PC 2.0" token is on ordinary PCs.
  if (/\bTablet\b/i.test(ua) && os !== "Windows") return "Tablet";
  if (/Android/i.test(ua) && !/Mobile/i.test(ua) && appMobile === null)
    return "Tablet";
  if (appMobile !== null) return appMobile ? "Mobile" : "Desktop";
  if (/Mobi|iPhone|iPod/i.test(ua)) return "Mobile";
  return os && MOBILE_OS.has(os) ? "Mobile" : "Desktop";
}

function clean(value: string | null | undefined): string | null {
  const trimmed = value?.trim();
  return trimmed ? trimmed : null;
}

/** The activity for one session, from what Kova and the request say. */
export function describeSessionClient(
  info: SessionClientInfo,
): SessionActivity {
  const ua = info.userAgent?.trim() ?? "";
  const platform = clean(info.platform)?.toLowerCase() ?? null;
  const platformOs = platform ? (PLATFORM_OS.get(platform) ?? null) : null;

  const app = detectApp(ua);
  const browser = app ? null : detectBrowser(ua);
  const osName = platformOs ?? detectOs(ua);

  // A device sign-in's platform settles phone vs computer for our apps.
  const appMobile = app
    ? platformOs
      ? MOBILE_OS.has(platformOs)
      : app.mobile
    : null;
  const deviceType = detectDeviceType(ua, osName, appMobile);
  const isMobile = deviceType !== "Desktop";

  const appName = app
    ? appMobile
      ? "Yap mobile app"
      : "Yap desktop app"
    : null;
  const client = appName ?? browser;
  const browserName = client
    ? osName
      ? `${client} on ${osName}`
      : client
    : (osName ?? (isMobile ? "Mobile client" : "Desktop client"));

  return {
    deviceType,
    isMobile,
    browserName,
    browser,
    appName,
    osName,
    deviceName: clean(info.deviceName),
    city: clean(info.city),
    country: clean(info.country),
  };
}

/** `request.cf` city and country, when Cloudflare set them. */
export function requestGeo(request: Request) {
  const cf = (request as { cf?: { city?: unknown; country?: unknown } }).cf;
  return {
    city: typeof cf?.city === "string" ? clean(cf.city) : null,
    country: typeof cf?.country === "string" ? clean(cf.country) : null,
  };
}
