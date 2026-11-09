import { apiGet } from "@/lib/api-client";
import type { CollectiblesCatalog } from "@/lib/collectibles-catalog";

type CatalogListener = (catalog: CollectiblesCatalog) => void;

let cachedCatalog: CollectiblesCatalog | null = null;
let cachedAtMs = 0;
let loadPromise: Promise<CollectiblesCatalog> | null = null;
const listeners = new Set<CatalogListener>();

/** Re-ask the server this often while its copy is fresh (it's a cache read). */
const FRESH_RECHECK_MS = 1000 * 60 * 30;
/**
 * When the server says its catalog is stale (upstream down), don't hammer
 * it: the server already refreshes in the background. Re-ask at most this
 * often.
 */
const STALE_RECHECK_MS = 1000 * 60 * 10;

function shouldReuse(catalog: CollectiblesCatalog, now = Date.now()) {
  const age = now - cachedAtMs;
  return age < (catalog.stale ? STALE_RECHECK_MS : FRESH_RECHECK_MS);
}

function publishCatalog(catalog: CollectiblesCatalog) {
  cachedCatalog = catalog;
  cachedAtMs = Date.now();
  for (const listener of listeners) {
    listener(catalog);
  }
  return catalog;
}

export function getCachedCollectiblesCatalog() {
  return cachedCatalog;
}

export function subscribeCollectiblesCatalog(listener: CatalogListener) {
  listeners.add(listener);
  if (cachedCatalog) {
    listener(cachedCatalog);
  }
  return () => {
    listeners.delete(listener);
  };
}

/**
 * The server's cached catalog. Refreshing it upstream is the cron's and the
 * admin console's job; the app only reads.
 */
export async function loadCollectiblesCatalog() {
  if (cachedCatalog && shouldReuse(cachedCatalog)) {
    return cachedCatalog;
  }

  if (loadPromise) {
    return loadPromise;
  }

  const request = apiGet<{ catalog: CollectiblesCatalog }>(
    "/api/collectibles/catalog",
  )
    .then((data) => publishCatalog(data.catalog))
    .finally(() => {
      if (loadPromise === request) {
        loadPromise = null;
      }
    });

  loadPromise = request;
  return request;
}

export function primeCollectiblesCatalogCache(catalog: CollectiblesCatalog) {
  return publishCatalog(catalog);
}

/** Test hook. */
export function resetCollectiblesCatalogClient() {
  cachedCatalog = null;
  cachedAtMs = 0;
  loadPromise = null;
  listeners.clear();
}
