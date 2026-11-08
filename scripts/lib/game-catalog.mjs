/**
 * Pure helpers behind `scripts/build-game-catalog.mjs`: trim Discord's public
 * "detectable applications" list into Yap's own game catalog. No I/O here, so
 * the unit test (src/lib/game-catalog-build.test.ts) can import it directly.
 */
import { createHash } from "node:crypto";

/** Discord application ids are decimal snowflakes. */
const APP_ID_PATTERN = /^\d{1,20}$/;
/** Icon hashes are 32 lowercase hex (the /api/game-icons route enforces it). */
const ICON_HASH_PATTERN = /^[a-f0-9]{32}$/;
/** Steam app ids as the activity protocol accepts them. */
const STEAM_APP_ID_PATTERN = /^\d{1,10}$/;

/**
 * @typedef {{ name: string, is_launcher?: true, arguments?: string }} CatalogExecutable
 * @typedef {{
 *   id: string,
 *   name: string,
 *   aliases?: string[],
 *   icon_hash?: string,
 *   executables: CatalogExecutable[],
 *   steam_app_ids?: string[],
 * }} CatalogApp
 * @typedef {{
 *   version: 1,
 *   generated_at: string,
 *   source: "discord-detectable",
 *   etag: string,
 *   count: number,
 *   apps: CatalogApp[],
 * }} Catalog
 */

/** @param {unknown} value @returns {value is string} */
function nonEmptyString(value) {
  return typeof value === "string" && value.trim().length > 0;
}

/**
 * True when the raw entry has the id and name every catalog app needs, so a
 * failed trim means "nothing to detect on Windows" rather than "malformed".
 *
 * @param {any} raw
 */
function hasIdAndName(raw) {
  return (
    typeof raw === "object" &&
    raw !== null &&
    typeof raw.id === "string" &&
    APP_ID_PATTERN.test(raw.id) &&
    nonEmptyString(raw.name)
  );
}

/**
 * Trims one detectable application. Returns null when it has no usable id or
 * name, or no win32 executable (nothing the Windows client could detect).
 *
 * @param {any} app
 * @returns {CatalogApp | null}
 */
export function trimApp(app) {
  if (!hasIdAndName(app)) return null;
  const { id, name, aliases, icon_hash, executables, third_party_skus } = app;

  /** @type {CatalogExecutable[]} */
  const exes = [];
  if (Array.isArray(executables)) {
    for (const exe of executables) {
      if (typeof exe !== "object" || exe === null) continue;
      if (exe.os !== "win32" || !nonEmptyString(exe.name)) continue;
      /** @type {CatalogExecutable} */
      const entry = { name: exe.name };
      if (exe.is_launcher === true) entry.is_launcher = true;
      if (nonEmptyString(exe.arguments)) entry.arguments = exe.arguments;
      exes.push(entry);
    }
  }
  if (exes.length === 0) return null;

  // Keys are added in the documented order so the output is stable.
  /** @type {Omit<CatalogApp, "executables"> & { executables?: CatalogExecutable[] }} */
  const ordered = { id, name: name.trim() };
  if (Array.isArray(aliases)) {
    const list = aliases.filter(nonEmptyString);
    if (list.length > 0) ordered.aliases = list;
  }
  if (typeof icon_hash === "string" && ICON_HASH_PATTERN.test(icon_hash)) {
    ordered.icon_hash = icon_hash;
  }
  ordered.executables = exes;

  if (Array.isArray(third_party_skus)) {
    /** @type {string[]} */
    const steam = [];
    for (const sku of third_party_skus) {
      if (typeof sku !== "object" || sku === null) continue;
      if (sku.distributor !== "steam") continue;
      if (typeof sku.id !== "string" || !STEAM_APP_ID_PATTERN.test(sku.id)) {
        continue;
      }
      if (!steam.includes(sku.id)) steam.push(sku.id);
    }
    if (steam.length > 0) ordered.steam_app_ids = steam;
  }
  return /** @type {CatalogApp} */ (ordered);
}

/** @param {CatalogApp} a @param {CatalogApp} b */
function compareIds(a, b) {
  const x = BigInt(a.id);
  const y = BigInt(b.id);
  return x < y ? -1 : x > y ? 1 : 0;
}

/**
 * Builds the catalog from the raw list: trims, drops what cannot be detected,
 * de-duplicates by id (first wins) and sorts by id so re-runs diff cleanly.
 *
 * @param {unknown} rawApps
 * @param {{ generatedAt?: Date }} [options]
 * @returns {{ catalog: Catalog, stats: { input: number, kept: number, dropped: number, invalid: number } }}
 */
export function buildCatalog(rawApps, options = {}) {
  if (!Array.isArray(rawApps)) {
    throw new Error("expected the detectable list to be a JSON array");
  }
  const generatedAt = options.generatedAt ?? new Date();
  const seen = new Set();
  /** @type {CatalogApp[]} */
  const apps = [];
  let dropped = 0;
  let invalid = 0;
  for (const raw of rawApps) {
    const app = trimApp(raw);
    if (!app) {
      if (hasIdAndName(raw)) dropped += 1;
      else invalid += 1;
      continue;
    }
    if (seen.has(app.id)) continue;
    seen.add(app.id);
    apps.push(app);
  }
  apps.sort(compareIds);
  const etag = createHash("sha256")
    .update(JSON.stringify(apps))
    .digest("hex")
    .slice(0, 16);
  return {
    catalog: {
      version: 1,
      generated_at: generatedAt.toISOString(),
      source: "discord-detectable",
      etag,
      count: apps.length,
      apps,
    },
    stats: {
      input: rawApps.length,
      kept: apps.length,
      dropped,
      invalid,
    },
  };
}
