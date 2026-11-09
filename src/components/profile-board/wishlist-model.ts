/**
 * Pure helpers for the Wishlist tab: adding a hint or a shop item, removing,
 * reordering, resolving a stored sku against the cached shop catalog, and
 * the picker's search. No React, so they are unit-tested. The server is
 * authoritative (`parseWishlist`); these mirror its limits so the counters
 * and disabled states are right before the save returns.
 */

import { baseItemName, KIND_COPY } from "@/lib/collectible-catalog-view";
import type {
  CollectibleCatalogItem,
  CollectiblesCatalog,
} from "@/lib/collectibles-catalog";
import {
  cleanText,
  MAX_WISHLIST_ITEMS,
  newWidgetId,
  TEXT_LIMITS,
  textLength,
  type WishlistItem,
} from "@/lib/profile-board";

export type WishlistAddResult =
  | { ok: true; items: WishlistItem[] }
  | { ok: false; reason: "full" | "empty" | "too_long" | "duplicate" };

export function isWishlistFull(items: readonly WishlistItem[]): boolean {
  return items.length >= MAX_WISHLIST_ITEMS;
}

/** A free-text hint, cleaned the way the server will, at the end. */
export function addWishlistHint(
  items: readonly WishlistItem[],
  raw: string,
  makeId: () => string = newWidgetId,
): WishlistAddResult {
  if (isWishlistFull(items)) return { ok: false, reason: "full" };
  const trimmed = raw.trim();
  if (!trimmed) return { ok: false, reason: "empty" };
  if (textLength(trimmed) > TEXT_LIMITS.wishlistHint) {
    return { ok: false, reason: "too_long" };
  }
  const text = cleanText(trimmed, TEXT_LIMITS.wishlistHint);
  if (!text) return { ok: false, reason: "empty" };
  return { ok: true, items: [...items, { id: makeId(), kind: "hint", text }] };
}

/** A shop collectible, by sku, at the end. One entry per sku. */
export function addWishlistShopItem(
  items: readonly WishlistItem[],
  item: Pick<CollectibleCatalogItem, "skuId" | "name">,
  makeId: () => string = newWidgetId,
): WishlistAddResult {
  if (isWishlistFull(items)) return { ok: false, reason: "full" };
  if (
    items.some((entry) => entry.kind === "shop" && entry.sku === item.skuId)
  ) {
    return { ok: false, reason: "duplicate" };
  }
  return {
    ok: true,
    items: [
      ...items,
      {
        id: makeId(),
        kind: "shop",
        sku: item.skuId,
        name: cleanText(baseItemName(item), TEXT_LIMITS.wishlistName),
      },
    ],
  };
}

export function removeWishlistItem(
  items: readonly WishlistItem[],
  id: string,
): WishlistItem[] {
  return items.filter((item) => item.id !== id);
}

/** Moves one item by `step` places (-1 earlier, +1 later), clamped. */
export function moveWishlistItem(
  items: readonly WishlistItem[],
  id: string,
  step: number,
): WishlistItem[] {
  const from = items.findIndex((item) => item.id === id);
  if (from < 0) return [...items];
  const to = Math.max(0, Math.min(items.length - 1, from + step));
  if (to === from) return [...items];
  const next = [...items];
  const [moved] = next.splice(from, 1);
  next.splice(to, 0, moved!);
  return next;
}

/** The catalog's item for a stored sku, whatever its kind; null when gone. */
export function findCatalogItem(
  catalog: CollectiblesCatalog | null | undefined,
  sku: string,
): CollectibleCatalogItem | null {
  if (!catalog) return null;
  return catalog.items.find((item) => item.skuId === sku) ?? null;
}

/** The silkscreen-free label under a card: "Avatar decoration". */
export function wishlistKindLabel(item: CollectibleCatalogItem): string {
  return KIND_COPY[item.kind].one;
}

/**
 * The picker's results: catalog items whose name, collection or kind
 * contains every word of `query`, one per colour family (the base variant),
 * in catalog order, at most `limit`.
 */
export function searchWishlistCatalog(
  catalog: CollectiblesCatalog | null | undefined,
  query: string,
  limit = 40,
): CollectibleCatalogItem[] {
  if (!catalog) return [];
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const seenFamilies = new Set<string>();
  const results: CollectibleCatalogItem[] = [];
  for (const item of catalog.items) {
    const family = item.variantOf ?? item.skuId;
    if (seenFamilies.has(family)) continue;
    const haystack =
      `${item.name} ${item.categoryName} ${KIND_COPY[item.kind].one}`.toLowerCase();
    if (!words.every((word) => haystack.includes(word))) continue;
    seenFamilies.add(family);
    results.push(item);
    if (results.length >= limit) break;
  }
  return results;
}
