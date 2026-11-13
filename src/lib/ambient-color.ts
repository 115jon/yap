/**
 * A picture's ambient colour (YouTube's ambient mode), for the focused stage's
 * person card: the picture drawn at 24 px, each pixel weighted by its
 * saturation so white, black and grey backgrounds do not wash it out, then
 * the saturation lifted and the lightness held in a band that glows. The same
 * load also draws the card's wash, a 32x18 thumbnail of the picture.
 *
 * Each URL is loaded once (CORS mode, so the pixels can be read) and only the
 * small results are kept: the full-size picture is dropped once measured.
 * When the host does not allow CORS the picture is loaded again without it:
 * it can still be drawn (the wash), only its colour is unknown, and the card
 * falls back to the accent. A picture that fails to load is not remembered,
 * so the next card tries again.
 */

import { unwrapProxyMediaUrl } from "@/lib/proxy-media-url";

const SAMPLE = 24;
/** The wash: a 16:9 thumbnail the card stretches (that is the blur). */
export const WASH_W = 32;
export const WASH_H = 18;
const MAX_ENTRIES = 48;

export type AmbientPicture = {
  /** The picture's first frame at WASH_W x WASH_H, cover-cropped. */
  wash: HTMLCanvasElement | null;
  /** `hsl(...)`, or null when the pixels cannot be read (CORS) or are empty. */
  color: string | null;
};

const pictures = new Map<string, AmbientPicture>();
const inflight = new Map<string, Promise<AmbientPicture | null>>();

function loadImage(url: string, cors: boolean): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    if (cors) img.crossOrigin = "anonymous";
    img.decoding = "async";
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("image failed to load"));
    img.src = url;
  });
}

/** CORS first, then plain (drawable but unreadable); null when both fail. */
function loadEither(url: string): Promise<HTMLImageElement | null> {
  return loadImage(url, true)
    .catch(() => loadImage(url, false))
    .catch(() => null);
}

/** The widest 16:9 slice of the picture, centred, at WASH_W x WASH_H. */
function drawWash(img: HTMLImageElement): HTMLCanvasElement | null {
  const canvas = document.createElement("canvas");
  canvas.width = WASH_W;
  canvas.height = WASH_H;
  const ctx = canvas.getContext("2d");
  const sw = img.naturalWidth;
  const sh = img.naturalHeight;
  if (!ctx || !sw || !sh) return null;
  const scale = Math.min(sw / WASH_W, sh / WASH_H);
  const cw = WASH_W * scale;
  const ch = WASH_H * scale;
  ctx.drawImage(
    img,
    (sw - cw) / 2,
    (sh - ch) / 2,
    cw,
    ch,
    0,
    0,
    WASH_W,
    WASH_H,
  );
  return canvas;
}

/**
 * The colourful average of RGBA pixels, as `hsl(h s% l%)`, or null when every
 * pixel is transparent. Pure, so it is tested without a canvas.
 */
export function ambientColorFromPixels(data: ArrayLike<number>): string | null {
  let r = 0;
  let g = 0;
  let b = 0;
  let sum = 0;
  for (let i = 0; i + 3 < data.length; i += 4) {
    if (data[i + 3]! < 128) continue;
    const pr = data[i]! / 255;
    const pg = data[i + 1]! / 255;
    const pb = data[i + 2]! / 255;
    const max = Math.max(pr, pg, pb);
    const min = Math.min(pr, pg, pb);
    const l = (max + min) / 2;
    const chroma = 1 - Math.abs(2 * l - 1);
    const s = max === min || chroma === 0 ? 0 : (max - min) / chroma;
    const w = s * chroma + 0.02;
    r += pr * w;
    g += pg * w;
    b += pb * w;
    sum += w;
  }
  if (sum <= 0) return null;
  const [h, s, l] = rgbToHsl(r / sum, g / sum, b / sum);
  const sat = Math.min(1, s * 1.6 + 0.15);
  const light = Math.max(0.35, Math.min(0.5, l));
  return `hsl(${Math.round(h)} ${Math.round(sat * 100)}% ${Math.round(light * 100)}%)`;
}

function rgbToHsl(r: number, g: number, b: number): [number, number, number] {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  if (max === min) return [0, 0, l];
  const d = max - min;
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
  let h: number;
  if (max === r) h = (g - b) / d + (g < b ? 6 : 0);
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  return [h * 60, s, l];
}

function measure(img: HTMLImageElement): string | null {
  const canvas = document.createElement("canvas");
  canvas.width = SAMPLE;
  canvas.height = SAMPLE;
  const ctx = canvas.getContext("2d", { willReadFrequently: true });
  if (!ctx) return null;
  try {
    ctx.drawImage(img, 0, 0, SAMPLE, SAMPLE);
    return ambientColorFromPixels(ctx.getImageData(0, 0, SAMPLE, SAMPLE).data);
  } catch {
    // A tainted canvas (no CORS) cannot be read.
    return null;
  }
}

/** The memoised result, if this picture has already been measured. */
export function peekAmbientPicture(url: string): AmbientPicture | undefined {
  return pictures.get(url);
}

/**
 * Loads and measures a picture once per URL. Null when it cannot be loaded
 * at all (not remembered, so a later card retries).
 */
export function readAmbientPicture(
  url: string,
): Promise<AmbientPicture | null> {
  const known = pictures.get(url);
  if (known) return Promise.resolve(known);
  let pending = inflight.get(url);
  if (!pending) {
    pending = loadEither(url).then((img) => {
      inflight.delete(url);
      if (!img) return null;
      const picture = { wash: drawWash(img), color: measure(img) };
      pictures.set(url, picture);
      if (pictures.size > MAX_ENTRIES) {
        pictures.delete(pictures.keys().next().value!);
      }
      return picture;
    });
    inflight.set(url, pending);
  }
  return pending;
}

/**
 * The full-size picture for a URL that names a small rendition: a Google
 * photo's `=s96-c` and a Discord CDN `?size=` are raised to 512. Everything
 * else (our own /api/avatars uploads) is already the original.
 */
export function fullSizeAvatarUrl(deliveredUrl: string): string {
  // A server-proxied avatar: resize the original (it loads proxied again).
  const url = unwrapProxyMediaUrl(deliveredUrl);
  try {
    const parsed = new URL(url);
    const host = parsed.hostname.toLowerCase();
    if (host.endsWith(".googleusercontent.com")) {
      return url.replace(/=s\d+((?:-[a-z0-9]+)*)$/i, "=s512$1");
    }
    if (
      (host === "cdn.discordapp.com" || host === "media.discordapp.net") &&
      parsed.searchParams.has("size")
    ) {
      parsed.searchParams.set("size", "512");
      return parsed.toString();
    }
  } catch {
    // A relative path: our own upload, already full size.
  }
  return url;
}
