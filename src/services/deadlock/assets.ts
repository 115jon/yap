/**
 * Deadlock's static data and art, copied to Yap. The manifest (hero names,
 * keys and colours, shop item names, ranked seasons, and where each image
 * comes from) lives in KV; the images live in R2 under
 * `game-assets/<Deadlock app id>/<key>.png` and are served by
 * /api/game-assets/. Clients only ever load those.
 *
 *   syncDeadlockAssets   daily: re-reads the manifest when the game's client
 *                        version moved, then mirrors missing hero and rank
 *                        art a batch at a time
 *   mirrorDeadlockAssets copies named keys now (a digest's items)
 *   loadManifest         the stored manifest, or a fresh one when none
 */

import {
  DEADLOCK_APP_ID,
  DEADLOCK_ASSET_KEY_PATTERN,
  heroAssetKey,
  itemAssetKey,
  type DeadlockHeroInfo,
  type DeadlockManifest,
  type DeadlockSeason,
} from "@/lib/deadlock";

import { DEADLOCK_ASSET_HOSTS, type DeadlockApi } from "./api";

export const MANIFEST_KV_KEY = "deadlock:manifest:v1";
/** Images mirrored per run; the rest wait for the next run. */
const MIRROR_BATCH = 40;
const MAX_IMAGE_BYTES = 2 * 1024 * 1024;
const IMAGE_TYPES = new Set(["image/png", "image/webp", "image/jpeg"]);

export interface StoredManifest extends DeadlockManifest {
  /** The game's client version it was read at. */
  version: number;
  /** Unix ms. */
  read_at: number;
  /** Asset key → upstream image URL (on an allowed host). */
  sources: Record<string, string>;
}

export interface AssetDeps {
  api: DeadlockApi;
  kv: KVNamespace;
  bucket: R2Bucket;
  /** Fetch for the image bytes (the art host, never a client). */
  fetch: typeof fetch;
  now?: () => number;
}

export function assetObjectKey(key: string): string {
  return `game-assets/${DEADLOCK_APP_ID}/${key}.png`;
}

function record(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

function allowedImage(url: unknown): string | null {
  if (typeof url !== "string") return null;
  try {
    const parsed = new URL(url);
    return parsed.protocol === "https:" &&
      DEADLOCK_ASSET_HOSTS.has(parsed.hostname)
      ? parsed.toString()
      : null;
  } catch {
    return null;
  }
}

function hexColor(value: unknown): string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value)
    ? value.toUpperCase()
    : "#5A5A5A";
}

/** Builds the manifest from the API's asset answers (exported for tests). */
export function buildManifest(input: {
  heroes: unknown;
  items: unknown;
  ranks: unknown;
  seasons: unknown;
  version: number;
  now: number;
}): StoredManifest {
  const heroes: Record<string, DeadlockHeroInfo> = {};
  const items: Record<string, { name: string }> = {};
  const sources: Record<string, string> = {};
  const seasons: DeadlockSeason[] = [];

  for (const raw of Array.isArray(input.heroes) ? input.heroes : []) {
    const hero = record(raw);
    const id = Number(hero.id);
    const className =
      typeof hero.class_name === "string" ? hero.class_name : "";
    const key = className.replace(/^hero_/, "").toLowerCase();
    if (!Number.isInteger(id) || !key || !DEADLOCK_ASSET_KEY_PATTERN.test(key))
      continue;
    const colors = record(hero.colors);
    heroes[String(id)] = {
      key,
      name: typeof hero.name === "string" ? hero.name.slice(0, 40) : key,
      color: hexColor(colors.style_hex),
    };
    const images = record(hero.images);
    const small = allowedImage(images.icon_image_small);
    const card = allowedImage(images.icon_hero_card);
    if (small) sources[heroAssetKey(key, "sm")] = small;
    if (card) sources[heroAssetKey(key, "card")] = card;
  }

  for (const raw of Array.isArray(input.items) ? input.items : []) {
    const item = record(raw);
    const id = Number(item.id);
    if (!Number.isInteger(id) || id <= 0) continue;
    if (item.type !== undefined && item.type !== "upgrade") continue;
    items[String(id)] = {
      name: typeof item.name === "string" ? item.name.slice(0, 40) : "Item",
    };
    const image = allowedImage(item.shop_image) ?? allowedImage(item.image);
    if (image) sources[itemAssetKey(id)] = image;
  }

  for (const raw of Array.isArray(input.ranks) ? input.ranks : []) {
    const rank = record(raw);
    const tier = Number(rank.tier);
    if (!Number.isInteger(tier) || tier < 0 || tier > 20) continue;
    const images = record(rank.images);
    const large = allowedImage(images.large);
    if (large) sources[`rank_${tier}`] = large;
    for (let sub = 1; sub <= 6; sub += 1) {
      const image = allowedImage(images[`large_subrank${sub}`]);
      if (image) sources[`rank_${tier}_${sub}`] = image;
    }
  }

  for (const raw of Array.isArray(input.seasons) ? input.seasons : []) {
    const season = record(raw);
    const intervals = Array.isArray(season.intervals) ? season.intervals : [];
    const start = Math.min(
      ...intervals.map((interval) => Number(record(interval).start_timestamp)),
    );
    if (!Number.isFinite(start)) continue;
    seasons.push({
      name:
        typeof season.name === "string" ? season.name.slice(0, 40) : "Season",
      start,
      calibration_matches: Math.max(0, Number(season.calibration_matches) || 0),
    });
  }

  return {
    version: input.version,
    read_at: input.now,
    heroes,
    items,
    seasons: seasons.sort((a, b) => a.start - b.start),
    sources,
  };
}

async function readStored(kv: KVNamespace): Promise<StoredManifest | null> {
  try {
    const stored = await kv.get<StoredManifest>(MANIFEST_KV_KEY, "json");
    return stored && stored.heroes ? stored : null;
  } catch {
    return null;
  }
}

/** Reads the manifest from the API (five asset calls). Null on any failure. */
async function readManifest(
  api: DeadlockApi,
  version: number,
  now: number,
): Promise<StoredManifest | null> {
  const [heroes, items, ranks, seasons] = await Promise.all([
    api.get<unknown[]>("/v1/assets/heroes", { only_active: "true" }),
    api.get<unknown[]>("/v1/assets/items/by-type/upgrade"),
    api.get<unknown[]>("/v1/assets/ranks"),
    api.get<unknown[]>("/v1/assets/ranked-seasons"),
  ]);
  if (!heroes.ok || !items.ok || !ranks.ok || !seasons.ok) return null;
  return buildManifest({
    heroes: heroes.body,
    items: items.body,
    ranks: ranks.body,
    seasons: seasons.body,
    version,
    now,
  });
}

/**
 * The stored manifest; when there is none yet, reads one now and stores
 * it. Null only when there is none and the API cannot be read.
 */
export async function loadManifest(
  deps: Pick<AssetDeps, "api" | "kv" | "now">,
): Promise<StoredManifest | null> {
  const stored = await readStored(deps.kv);
  if (stored) return stored;
  const fresh = await readManifest(deps.api, 0, (deps.now ?? Date.now)());
  if (fresh) await deps.kv.put(MANIFEST_KV_KEY, JSON.stringify(fresh));
  return fresh;
}

/**
 * Copies `keys` that R2 does not have yet. Only keys the manifest knows
 * (so only allowed hosts); each image at most 2 MB and an image type.
 * Returns the keys stored.
 */
/** The copy in R2 was made from this manifest version (else re-copied). */
async function isCurrent(
  bucket: R2Bucket,
  objectKey: string,
  version: number,
): Promise<boolean> {
  const head = await bucket.head(objectKey);
  return !!head && head.customMetadata?.v === String(version);
}

/** The body, or null past `limit` bytes (read in chunks, never whole). */
async function readImage(
  response: Response,
  limit: number,
): Promise<Uint8Array | null> {
  const declared = Number(response.headers.get("content-length"));
  if (Number.isFinite(declared) && declared > limit) {
    await response.body?.cancel();
    return null;
  }
  if (!response.body) return null;
  const reader = response.body.getReader();
  const chunks: Uint8Array[] = [];
  let total = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.byteLength;
    if (total > limit) {
      await reader.cancel();
      return null;
    }
    chunks.push(value);
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

export async function mirrorDeadlockAssets(
  deps: Pick<AssetDeps, "bucket" | "fetch">,
  manifest: Pick<StoredManifest, "sources" | "version">,
  keys: readonly string[],
): Promise<string[]> {
  const stored: string[] = [];
  const version = manifest.version ?? 0;
  for (const key of keys) {
    const source = manifest.sources[key];
    if (!source || !DEADLOCK_ASSET_KEY_PATTERN.test(key)) continue;
    const objectKey = assetObjectKey(key);
    if (await isCurrent(deps.bucket, objectKey, version)) continue;
    let response: Response;
    try {
      response = await deps.fetch(source, {
        redirect: "manual",
        signal: AbortSignal.timeout(10_000),
      });
    } catch {
      continue;
    }
    const type = (response.headers.get("content-type") ?? "")
      .split(";")[0]!
      .trim()
      .toLowerCase();
    if (response.status !== 200 || !IMAGE_TYPES.has(type)) {
      await response.body?.cancel();
      continue;
    }
    const bytes = await readImage(response, MAX_IMAGE_BYTES);
    if (!bytes || bytes.byteLength === 0) continue;
    await deps.bucket.put(objectKey, bytes, {
      httpMetadata: { contentType: type },
      customMetadata: { v: String(version) },
    });
    stored.push(key);
  }
  return stored;
}

/**
 * The hourly asset job: re-reads the manifest when the client version
 * moved (or none is stored), then copies up to a batch of hero and rank
 * images that are missing or were copied for an older version (a patch can
 * redraw them). Items are mirrored on demand (mirrorDeadlockAssets).
 */
export async function syncDeadlockAssets(deps: AssetDeps): Promise<{
  version: number | null;
  refreshed: boolean;
  mirrored: string[];
}> {
  const now = (deps.now ?? Date.now)();
  const versions = await deps.api.get<number[]>("/v1/assets/client-versions");
  const version =
    versions.ok && Array.isArray(versions.body) && versions.body.length > 0
      ? Math.max(...versions.body.filter((v) => Number.isFinite(v)))
      : null;
  let manifest = await readStored(deps.kv);
  let refreshed = false;
  if (!manifest || (version !== null && version !== manifest.version)) {
    const fresh = await readManifest(deps.api, version ?? 0, now);
    if (fresh) {
      manifest = fresh;
      refreshed = true;
      await deps.kv.put(MANIFEST_KV_KEY, JSON.stringify(fresh));
    }
  }
  if (!manifest) return { version, refreshed, mirrored: [] };
  const wanted = Object.keys(manifest.sources).filter(
    (key) => key.startsWith("hero_") || key.startsWith("rank_"),
  );
  const missing: string[] = [];
  for (const key of wanted) {
    if (missing.length >= MIRROR_BATCH) break;
    if (
      !(await isCurrent(deps.bucket, assetObjectKey(key), manifest.version))
    ) {
      missing.push(key);
    }
  }
  const mirrored = await mirrorDeadlockAssets(deps, manifest, missing);
  return { version, refreshed, mirrored };
}
