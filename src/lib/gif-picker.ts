import {
  orElse,
  readList,
  readNonBlankString,
  readNumber,
  readObject,
  readStoredJson,
  readString,
  required,
} from "@/lib/persisted-guard";
import { unwrapProxyMediaUrl } from "@/lib/proxy-media-url";
import { UPLOAD_LIMITS } from "@/lib/upload-limits";

export const GIF_FAVORITES_STORAGE_KEY = "chat:gifs:favorites";
export const MAX_GIF_FAVORITES = 48;
export const MAX_GIF_UPLOAD_BYTES = UPLOAD_LIMITS.gif;
export const DEFAULT_GIF_PROVIDER = "klipy";

export type GifProvider = "klipy" | "tenor" | "external";
export type GifPickerContentType =
  | "image/gif"
  | "image/apng"
  | "image/webp"
  | "image/png"
  | "video/mp4";
export type GifPickerMediaType = "gifs" | "stickers" | "clips" | "memes";

export interface GifPickerAsset {
  url: string;
  width: number;
  height: number;
  sizeBytes: number;
  contentType: GifPickerContentType;
}

export interface GifPickerItem {
  id: string;
  title: string;
  provider: GifProvider;
  altText?: string;
  query?: string;
  /** What a picker tile shows. Small, but may be a video (`tinymp4`). */
  preview: GifPickerAsset;
  /**
   * The lightest picture rendition fit for a picker tile (`tinygif`, a small
   * webp): for clients that draw tiles as images, not video. Absent on older
   * servers and on saved favorites; never the full file unless it is the
   * only picture there is.
   */
  thumb?: GifPickerAsset;
  send: GifPickerAsset;
  sourceUrl: string;
  aspectRatio: number;
  duration?: number;
  mediaType?: GifPickerMediaType;
}

export interface GifPickerCategory {
  id: string;
  label: string;
  query: string;
  imageUrl: string;
}

export interface TenorConfig {
  API_V2_KEY: string;
  API_V2_URL: string;
  API_V2_CLIENT_KEY?: string;
}

export type TenorCacheParamValue = string | number | boolean | null | undefined;

interface KlipyMediaFormat {
  url?: string;
  dims?: [number, number];
  size?: number;
}

const GIF_PICKER_MEDIA_TYPES = ["gifs", "stickers", "clips", "memes"] as const;

type GifMediaTypeInferenceAsset = {
  url?: string | null;
  contentType?: string | null;
};

type GifMediaTypeInferenceInput = {
  id?: string | null;
  title?: string | null;
  sourceUrl?: string | null;
  duration?: number | null;
  mediaType?: unknown;
  preview?: GifMediaTypeInferenceAsset | null;
  send?: GifMediaTypeInferenceAsset | null;
};

function isStaticMemeUrl(value: string | null | undefined): boolean {
  return (
    typeof value === "string" && value.toLowerCase().includes("/static-memes/")
  );
}

export function isGifPickerMediaType(
  value: unknown,
): value is GifPickerMediaType {
  return (
    typeof value === "string" &&
    (GIF_PICKER_MEDIA_TYPES as readonly string[]).includes(value)
  );
}

export function normalizeGifPickerContentType(
  value: unknown,
): GifPickerContentType {
  const mime =
    typeof value === "string" ? value.toLowerCase().split(";")[0].trim() : "";
  if (mime === "image/apng") return "image/apng";
  if (mime === "image/webp") return "image/webp";
  if (mime === "image/png") return "image/png";
  if (mime === "video/mp4" || mime.startsWith("video/")) return "video/mp4";
  return "image/gif";
}

export function inferGifPickerMediaType(
  input: GifMediaTypeInferenceInput,
): GifPickerMediaType {
  if (isGifPickerMediaType(input.mediaType)) return input.mediaType;

  const sendUrl = input.send?.url ?? "";
  const previewUrl = input.preview?.url ?? "";
  const sourceUrl = input.sourceUrl ?? "";
  const sendContentType = normalizeGifPickerContentType(
    input.send?.contentType,
  );
  const title = input.title?.toLowerCase() ?? "";
  const id = input.id?.toLowerCase() ?? "";

  if (
    (input.duration !== undefined && input.duration !== null) ||
    sendContentType === "video/mp4" ||
    sendUrl.includes(".mp4") ||
    sendUrl.includes("/clips/") ||
    id.includes("clip")
  ) {
    return "clips";
  }

  if (
    isStaticMemeUrl(sendUrl) ||
    isStaticMemeUrl(previewUrl) ||
    isStaticMemeUrl(sourceUrl)
  ) {
    return "memes";
  }

  if (
    sendContentType === "image/apng" ||
    sendUrl.includes("/stickers/") ||
    previewUrl.includes("/stickers/") ||
    sendUrl.includes("sticker") ||
    previewUrl.includes("sticker") ||
    title.includes("sticker") ||
    id.includes("sticker")
  ) {
    return "stickers";
  }

  return "gifs";
}

export function getGifProviderLabel(provider: GifProvider): string {
  if (provider === "external") return "Saved GIF";
  return provider === "klipy" ? "KLIPY" : "Tenor";
}

export function getGifProviderSearchPlaceholder(provider: GifProvider): string {
  if (provider === "external") return "Search GIFs";
  return `Search ${getGifProviderLabel(provider)}`;
}

export function getGifAttachmentProvider(
  fileKeyOrUrl: string | null | undefined,
): GifProvider | null {
  if (!fileKeyOrUrl) return null;

  const normalized = fileKeyOrUrl.toLowerCase();
  const unwrappedUrl = unwrapProxyMediaUrl(fileKeyOrUrl);
  const unwrappedNormalized = unwrappedUrl.toLowerCase();
  if (
    normalized.includes("/gifs/klipy/") ||
    normalized.includes("\\gifs\\klipy\\")
  )
    return "klipy";
  if (
    normalized.includes("/gifs/tenor/") ||
    normalized.includes("\\gifs\\tenor\\")
  )
    return "tenor";
  if (
    unwrappedNormalized.includes("/gifs/klipy/") ||
    unwrappedNormalized.includes("\\gifs\\klipy\\")
  )
    return "klipy";
  if (
    unwrappedNormalized.includes("/gifs/tenor/") ||
    unwrappedNormalized.includes("\\gifs\\tenor\\")
  )
    return "tenor";

  try {
    const parsed = new URL(unwrappedUrl);
    const hostname = parsed.hostname.toLowerCase();
    if (/^static\d*\.klipy\.com$/.test(hostname)) return "klipy";
    if (hostname === "tenor.com" || /^media\d*\.tenor\.com$/.test(hostname))
      return "tenor";
    if (
      hostname === "gif.fxtwitter.com" &&
      parsed.pathname.startsWith("/tweet_video/")
    )
      return "external";
  } catch {
    // Non-URL storage keys are handled by the path checks above.
  }

  return null;
}

function inferGifProviderFromUrl(url: string | null | undefined): GifProvider {
  if (!url) return DEFAULT_GIF_PROVIDER;

  const lowerUrl = url.toLowerCase();
  if (lowerUrl.includes("klipy.com")) return "klipy";
  if (lowerUrl.includes("tenor.com")) return "tenor";

  return getGifAttachmentProvider(url) ?? DEFAULT_GIF_PROVIDER;
}

export function getGifItemIdentityKey(
  gif: Pick<GifPickerItem, "id" | "provider">,
): string {
  return `${gif.provider}:${gif.id}`;
}

interface TenorMediaFormat {
  url?: string;
  dims?: [number, number];
  size?: number;
}

function decodeBase64Utf8(encoded: string): string {
  const binary = atob(encoded);
  const bytes = Uint8Array.from(binary, (char) => char.charCodeAt(0));
  return new TextDecoder().decode(bytes);
}

export function extractTenorConfigFromHtml(html: string): TenorConfig | null {
  const match = html.match(
    /<script\b(?=[^>]*\bid=["']data["'])(?=[^>]*\btype=["']text\/x-cache["'])[^>]*>([\s\S]*?)<\/script>/i,
  );
  const encoded = match?.[1]?.trim();
  if (!encoded) return null;

  try {
    const decoded = JSON.parse(decodeBase64Utf8(encoded));
    if (
      typeof decoded?.API_V2_KEY !== "string" ||
      typeof decoded?.API_V2_URL !== "string"
    )
      return null;

    return {
      API_V2_KEY: decoded.API_V2_KEY,
      API_V2_URL: decoded.API_V2_URL,
      API_V2_CLIENT_KEY:
        typeof decoded.API_V2_CLIENT_KEY === "string"
          ? decoded.API_V2_CLIENT_KEY
          : undefined,
    };
  } catch {
    return null;
  }
}

export function buildTenorCacheKey(
  path: string,
  params: Record<string, TenorCacheParamValue>,
): string {
  return buildGifProviderCacheKey("tenor", path, params);
}

export function buildGifProviderCacheKey(
  provider: GifProvider,
  path: string,
  params: Record<string, TenorCacheParamValue>,
): string {
  const search = new URLSearchParams();
  for (const key of Object.keys(params).sort()) {
    const value = params[key];
    if (value === undefined || value === null || value === "") continue;
    search.set(key, String(value));
  }

  const suffix = search.toString();
  return suffix
    ? `gif:${provider}:v1:${path}?${suffix}`
    : `gif:${provider}:v1:${path}`;
}

export function dedupeGifPickerItems(items: GifPickerItem[]): GifPickerItem[] {
  const seen = new Set<string>();
  return items.filter((item) => {
    const dedupeKey = getGifItemIdentityKey(item);
    if (seen.has(dedupeKey)) return false;
    seen.add(dedupeKey);
    return true;
  });
}

export function appendUniqueGifPickerItems(
  current: GifPickerItem[],
  incoming: GifPickerItem[],
): GifPickerItem[] {
  return dedupeGifPickerItems([...current, ...incoming]);
}

function normalizeTenorAsset(
  format: TenorMediaFormat | undefined,
  contentType: GifPickerAsset["contentType"],
): GifPickerAsset | null {
  if (!format?.url || !Array.isArray(format.dims) || format.dims.length < 2)
    return null;

  const [width, height] = format.dims;
  if (!width || !height) return null;

  return {
    url: format.url,
    width,
    height,
    sizeBytes: format.size ?? 0,
    contentType,
  };
}

/**
 * The renditions a GIF search asks the provider for: the sent and preview
 * ones, plus a light tile picture for pickGifThumb. Tenor's tinywebp is
 * about a sixth of its tinygif. KLIPY has no tinywebp; its webp is
 * full-size but about a third of its tinygif at median.
 */
export function gifSearchMediaFilter(provider: GifProvider): string {
  const base = "gif,mediumgif,tinygif,mp4,tinymp4";
  return provider === "tenor" ? `${base},tinywebp` : `${base},webp`;
}

/** A tile is about this wide on a dense phone screen; smaller looks soft. */
export const GIF_THUMB_MIN_EDGE = 180;

/**
 * The tile picture among `candidates` (images only): the fewest bytes among
 * those at least GIF_THUMB_MIN_EDGE on their long edge, else the biggest of
 * the small ones. Bytes decide when both sizes are known (non-zero); else
 * the smaller area wins.
 */
export function pickGifThumb(
  candidates: readonly (GifPickerAsset | null | undefined)[],
): GifPickerAsset | undefined {
  const images = candidates.filter(
    (asset): asset is GifPickerAsset =>
      !!asset && asset.contentType !== "video/mp4" && !!asset.url,
  );
  if (images.length === 0) return undefined;
  const edge = (asset: GifPickerAsset) => Math.max(asset.width, asset.height);
  const bigEnough = images.filter((a) => edge(a) >= GIF_THUMB_MIN_EDGE);
  if (bigEnough.length === 0) {
    return images.reduce((best, a) =>
      edge(a) > edge(best) ||
      (edge(a) === edge(best) && bytesRank(a) < bytesRank(best))
        ? a
        : best,
    );
  }
  // Bytes decide only when both sizes are known; otherwise the smaller
  // picture wins, so a sizeless tile rendition still beats the full file.
  return bigEnough.reduce((best, a) => {
    if (a.sizeBytes > 0 && best.sizeBytes > 0 && a.sizeBytes !== best.sizeBytes)
      return a.sizeBytes < best.sizeBytes ? a : best;
    return a.width * a.height < best.width * best.height ? a : best;
  });
}

function bytesRank(asset: GifPickerAsset): number {
  return asset.sizeBytes > 0 ? asset.sizeBytes : Number.POSITIVE_INFINITY;
}

export function normalizeTenorGifResult(result: any): GifPickerItem | null {
  if (!result?.id || !result?.media_formats) return null;

  const sendCandidates = [
    normalizeTenorAsset(result.media_formats.gif, "image/gif"),
    normalizeTenorAsset(result.media_formats.tinygif, "image/gif"),
    normalizeTenorAsset(result.media_formats.mp4, "video/mp4"),
    normalizeTenorAsset(result.media_formats.tinymp4, "video/mp4"),
  ].filter(Boolean) as GifPickerAsset[];

  const previewCandidates = [
    normalizeTenorAsset(result.media_formats.tinymp4, "video/mp4"),
    normalizeTenorAsset(result.media_formats.mp4, "video/mp4"),
    normalizeTenorAsset(result.media_formats.tinygif, "image/gif"),
    normalizeTenorAsset(result.media_formats.gif, "image/gif"),
  ].filter(Boolean) as GifPickerAsset[];

  const send =
    sendCandidates.find(
      (asset) =>
        asset.sizeBytes === 0 || asset.sizeBytes <= MAX_GIF_UPLOAD_BYTES,
    ) ?? sendCandidates[0];
  const preview = previewCandidates[0];
  const thumb = pickGifThumb([
    normalizeTenorAsset(
      result.media_formats.tinywebp_transparent,
      "image/webp",
    ),
    normalizeTenorAsset(result.media_formats.webp_transparent, "image/webp"),
    normalizeTenorAsset(result.media_formats.tinygif_transparent, "image/gif"),
    normalizeTenorAsset(result.media_formats.tinygif, "image/gif"),
    normalizeTenorAsset(result.media_formats.nanogif, "image/gif"),
    normalizeTenorAsset(result.media_formats.tinywebp, "image/webp"),
    normalizeTenorAsset(result.media_formats.nanowebp, "image/webp"),
    normalizeTenorAsset(result.media_formats.webp, "image/webp"),
    normalizeTenorAsset(result.media_formats.mediumgif, "image/gif"),
    normalizeTenorAsset(result.media_formats.gif, "image/gif"),
  ]);

  if (!send || !preview) return null;

  const title =
    result.content_description ||
    result.h1_title ||
    result.title ||
    result.long_title ||
    (Array.isArray(result.tags) && result.tags.length > 0
      ? String(result.tags[0])
      : "GIF");

  return {
    id: String(result.id),
    title,
    provider: "tenor",
    altText: result.content_description || undefined,
    query: undefined,
    preview,
    thumb,
    send,
    sourceUrl: result.itemurl || result.url || preview.url,
    aspectRatio: preview.width / preview.height,
  };
}

function normalizeKlipyAsset(
  format: KlipyMediaFormat | undefined,
  contentType: GifPickerAsset["contentType"],
): GifPickerAsset | null {
  if (!format?.url || !Array.isArray(format.dims) || format.dims.length < 2)
    return null;

  const [width, height] = format.dims;
  if (!width || !height) return null;

  return {
    url: format.url,
    width,
    height,
    sizeBytes: format.size ?? 0,
    contentType,
  };
}

export function normalizeKlipyGifResult(result: any): GifPickerItem | null {
  if (!result?.id || !result?.media_formats) return null;

  const sendCandidates = [
    normalizeKlipyAsset(result.media_formats.gif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.mediumgif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.tinygif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.mp4, "video/mp4"),
    normalizeKlipyAsset(result.media_formats.tinymp4, "video/mp4"),
  ].filter(Boolean) as GifPickerAsset[];

  const previewCandidates = [
    normalizeKlipyAsset(result.media_formats.tinymp4, "video/mp4"),
    normalizeKlipyAsset(result.media_formats.mp4, "video/mp4"),
    normalizeKlipyAsset(result.media_formats.tinygif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.mediumgif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.gif, "image/gif"),
  ].filter(Boolean) as GifPickerAsset[];

  const send =
    sendCandidates.find(
      (asset) =>
        asset.sizeBytes === 0 || asset.sizeBytes <= MAX_GIF_UPLOAD_BYTES,
    ) ?? sendCandidates[0];
  const preview = previewCandidates[0];
  const thumb = pickGifThumb([
    normalizeKlipyAsset(result.media_formats.tinygif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.nanogif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.tinywebp, "image/webp"),
    normalizeKlipyAsset(result.media_formats.nanowebp, "image/webp"),
    normalizeKlipyAsset(result.media_formats.webp, "image/webp"),
    normalizeKlipyAsset(result.media_formats.mediumgif, "image/gif"),
    normalizeKlipyAsset(result.media_formats.gif, "image/gif"),
  ]);

  if (!send || !preview) return null;

  return {
    id: String(result.id),
    title: String(
      result.title || result.content_description || result.long_title || "GIF",
    ),
    provider: "klipy",
    altText: result.content_description || result.title || undefined,
    query: undefined,
    preview,
    thumb,
    send,
    sourceUrl: send.url,
    aspectRatio: preview.width / preview.height,
    duration:
      typeof result.duration === "number"
        ? result.duration
        : typeof result.duration === "string" &&
            !isNaN(parseFloat(result.duration))
          ? parseFloat(result.duration)
          : undefined,
  };
}

export function normalizeTenorCategory(tag: any): GifPickerCategory | null {
  if (!tag?.id || !tag?.searchterm || !tag?.image) return null;

  return {
    id: String(tag.id),
    label: String(tag.searchterm),
    query: String(tag.searchterm),
    imageUrl: String(tag.image),
  };
}

export function normalizeKlipyCategory(tag: any): GifPickerCategory | null {
  if (!tag) return null;
  if (tag.category || tag.preview_url) {
    const label = tag.category || tag.query || "Category";
    const query = tag.query || tag.category || "";
    const imageUrl = tag.preview_url || "";
    return {
      id: label,
      label,
      query,
      imageUrl,
    };
  }
  if (!tag.id || !tag.searchterm || !tag.image) return null;

  return {
    id: String(tag.id),
    label: String(tag.searchterm),
    query: String(tag.searchterm),
    imageUrl: String(tag.image),
  };
}

export function toggleGifFavorite(
  favorites: GifPickerItem[],
  gif: GifPickerItem,
): GifPickerItem[] {
  const gifKey = getGifItemIdentityKey(gif);
  const existing = favorites.some(
    (item) => getGifItemIdentityKey(item) === gifKey,
  );
  if (existing) {
    return favorites.filter((item) => getGifItemIdentityKey(item) !== gifKey);
  }

  return [
    gif,
    ...favorites.filter((item) => getGifItemIdentityKey(item) !== gifKey),
  ].slice(0, MAX_GIF_FAVORITES);
}

/** An asset as stored: it needs a url; size fields from older saves may be missing. */
const readStoredGifAsset = readObject<GifPickerAsset>(
  {
    url: required(readNonBlankString),
    width: orElse(readNumber, 0),
    height: orElse(readNumber, 0),
    sizeBytes: orElse(readNumber, 0),
    contentType: normalizeGifPickerContentType,
  },
  { keepUnknown: true },
);

type StoredGifFavorite = Omit<GifPickerItem, "provider" | "mediaType"> & {
  provider?: string;
  mediaType?: string;
};

/**
 * A favorite as stored. Only an id and playable urls are required; the media
 * type and provider are re-derived below, so saves from older releases that
 * lack them still load. Fields this release does not know are kept.
 */
const readStoredGifFavorite = readObject<StoredGifFavorite>(
  {
    id: required(readString),
    title: orElse(readString, ""),
    provider: readString,
    sourceUrl: orElse(readString, ""),
    aspectRatio: orElse(readNumber, 1),
    duration: readNumber,
    mediaType: readString,
    altText: readString,
    query: readString,
    preview: required(readStoredGifAsset),
    thumb: readStoredGifAsset,
    send: required(readStoredGifAsset),
  },
  { keepUnknown: true },
);

export function parseStoredGifFavorites(
  raw: string | null | undefined,
): GifPickerItem[] {
  const stored = readStoredJson(raw, readList(readStoredGifFavorite)) ?? [];
  return stored.map((item) => {
    const provider =
      item.provider === "klipy" ||
      item.provider === "tenor" ||
      item.provider === "external"
        ? item.provider
        : inferGifProviderFromUrl(
            item.sourceUrl || item.send.url || item.preview.url,
          );
    return {
      ...item,
      provider,
      mediaType: inferGifPickerMediaType(item),
    };
  });
}

type KlipyNativeMediaType = Exclude<GifPickerMediaType, "gifs">;

const KLIPY_NATIVE_FALLBACK_TITLES: Record<KlipyNativeMediaType, string> = {
  stickers: "Sticker",
  memes: "Meme",
  clips: "Clip",
};

/** KLIPY's own API serves each item at these sizes, smallest first. */
const KLIPY_NATIVE_TIERS = ["xs", "sm", "md", "hd"] as const;

function getKlipyStaticAssetContentType(
  url: string,
  mediaType: Exclude<KlipyNativeMediaType, "clips">,
): GifPickerContentType {
  const lowerUrl = url.toLowerCase();
  if (lowerUrl.includes(".webp")) return "image/webp";
  if (lowerUrl.includes(".png"))
    return mediaType === "memes" ? "image/png" : "image/apng";
  return "image/gif";
}

function klipyNativeAsset(
  url: unknown,
  meta: any,
  contentType: GifPickerContentType,
): GifPickerAsset | null {
  if (typeof url !== "string" || !url) return null;
  return {
    url,
    width: meta?.width || 320,
    height: meta?.height || 320,
    sizeBytes: meta?.size || 0,
    contentType,
  };
}

/**
 * Every rendition of `format` a KLIPY item carries: the flat
 * `file[format]` URL (with `file_meta[format]`) and each size tier's
 * `file[tier][format]`.
 */
function klipyNativeRenditions(
  result: any,
  format: string,
  contentType: (url: string) => GifPickerContentType,
): GifPickerAsset[] {
  const file = result?.file || {};
  const meta = result?.file_meta || {};
  const assets: (GifPickerAsset | null)[] = [];
  if (typeof file[format] === "string") {
    assets.push(
      klipyNativeAsset(file[format], meta[format], contentType(file[format])),
    );
  }
  for (const tier of KLIPY_NATIVE_TIERS) {
    const asset = file[tier]?.[format];
    if (asset?.url) {
      assets.push(klipyNativeAsset(asset.url, asset, contentType(asset.url)));
    }
  }
  return assets.filter((asset): asset is GifPickerAsset => asset !== null);
}

/**
 * A result of KLIPY's own stickers, clips or memes API. `send` is the
 * biggest rendition, as before; `preview` and `thumb` are the lightest
 * picture fit for a tile (pickGifThumb), so the grid never pulls the
 * full file (a sticker's `md` webp can outweigh its `hd` one).
 */
export function normalizeKlipyNativeResult(
  result: any,
  mediaType: KlipyNativeMediaType,
): GifPickerItem | null {
  if (!result || (!result.id && !result.slug)) return null;

  const id = String(result.id || result.slug);
  const title = String(
    result.title || result.slug || KLIPY_NATIVE_FALLBACK_TITLES[mediaType],
  );

  const file = result.file || {};
  let send: GifPickerAsset | null;
  let fallbackPreview: GifPickerAsset | null;
  let pictures: GifPickerAsset[];

  if (mediaType === "clips") {
    const fileMeta = result.file_meta || {};
    const first = (format: string, contentType: GifPickerContentType) => {
      const url =
        file[format] || file.hd?.[format]?.url || file.md?.[format]?.url;
      const meta =
        fileMeta[format] || file.hd?.[format] || file.md?.[format] || {};
      return klipyNativeAsset(url, meta, contentType);
    };
    const mp4 = first("mp4", "video/mp4");
    const webp = first("webp", "image/webp");
    const gif = first("gif", "image/gif");
    send = mp4 || webp || gif;
    fallbackPreview = webp || mp4 || gif;
    pictures = [
      ...klipyNativeRenditions(result, "webp", () => "image/webp"),
      ...klipyNativeRenditions(result, "gif", () => "image/gif"),
    ];
  } else {
    const hd = file.hd || {};
    const md = file.md || file.sm || hd || {};
    const pick = (asset: any) =>
      klipyNativeAsset(
        asset?.url,
        asset,
        getKlipyStaticAssetContentType(String(asset?.url ?? ""), mediaType),
      );
    send = pick(hd.webp || hd.png || hd.gif || md.webp || md.png || md.gif);
    fallbackPreview = pick(
      md.webp || md.png || md.gif || hd.webp || hd.png || hd.gif,
    );
    const contentType = (url: string) =>
      getKlipyStaticAssetContentType(url, mediaType);
    pictures = ["webp", "png", "gif"].flatMap((format) =>
      klipyNativeRenditions(result, format, contentType),
    );
  }

  const thumb = pickGifThumb(pictures);
  const preview = thumb ?? fallbackPreview;
  if (!send || !preview) return null;

  return {
    id,
    title,
    provider: "klipy",
    preview,
    thumb,
    send,
    sourceUrl: send.url,
    aspectRatio: preview.width / preview.height,
    duration:
      mediaType === "clips"
        ? typeof result.duration === "number"
          ? result.duration
          : typeof result.duration === "string" &&
              !isNaN(parseFloat(result.duration))
            ? parseFloat(result.duration)
            : undefined
        : undefined,
    mediaType,
  };
}
