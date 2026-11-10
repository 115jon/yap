import { describe, expect, it } from "vitest";

import { MAX_WISHLIST_ITEMS, type WishlistItem } from "@/lib/profile-board";

import { fixtureCatalog } from "./tab-fixtures-data";
import {
  addWishlistHint,
  addWishlistShopItem,
  findCatalogItem,
  moveWishlistItem,
  removeWishlistItem,
  searchWishlistCatalog,
} from "./wishlist-model";

let n = 0;
const makeId = () => `id${++n}`;

const hints = (count: number): WishlistItem[] =>
  Array.from({ length: count }, (_, i) => ({
    id: `h${i}`,
    kind: "hint",
    text: `hint ${i}`,
  }));

describe("addWishlistHint", () => {
  it("adds a cleaned hint at the end", () => {
    const result = addWishlistHint(hints(1), "  anything   with cats ", makeId);
    expect(result.ok && result.items.at(-1)).toMatchObject({
      kind: "hint",
      text: "anything with cats",
    });
  });

  it("refuses empty, too long and full", () => {
    expect(addWishlistHint([], "   ")).toEqual({ ok: false, reason: "empty" });
    expect(addWishlistHint([], "x".repeat(81))).toEqual({
      ok: false,
      reason: "too_long",
    });
    expect(addWishlistHint([], "x".repeat(80)).ok).toBe(true);
    expect(addWishlistHint(hints(MAX_WISHLIST_ITEMS), "more")).toEqual({
      ok: false,
      reason: "full",
    });
  });

  it("counts grapheme clusters, not code units", () => {
    expect(addWishlistHint([], "👩‍👩‍👧".repeat(80)).ok).toBe(true);
  });
});

describe("addWishlistShopItem", () => {
  const item = { skuId: "np-1", name: "Night Drive (Pink)" };

  it("stores the sku and the base name", () => {
    const result = addWishlistShopItem([], item, makeId);
    expect(result.ok && result.items[0]).toMatchObject({
      kind: "shop",
      sku: "np-1",
      name: "Night Drive",
    });
  });

  it("refuses a duplicate sku and a full list", () => {
    const once = addWishlistShopItem([], item, makeId);
    expect(once.ok && addWishlistShopItem(once.items, item)).toEqual({
      ok: false,
      reason: "duplicate",
    });
    expect(addWishlistShopItem(hints(MAX_WISHLIST_ITEMS), item)).toEqual({
      ok: false,
      reason: "full",
    });
  });
});

describe("remove and move", () => {
  const list = hints(3);

  it("removes by id", () => {
    expect(removeWishlistItem(list, "h1").map((i) => i.id)).toEqual([
      "h0",
      "h2",
    ]);
  });

  it("moves one place and clamps at the ends", () => {
    expect(moveWishlistItem(list, "h2", -1).map((i) => i.id)).toEqual([
      "h0",
      "h2",
      "h1",
    ]);
    expect(moveWishlistItem(list, "h0", 1).map((i) => i.id)).toEqual([
      "h1",
      "h0",
      "h2",
    ]);
    expect(moveWishlistItem(list, "h0", -1).map((i) => i.id)).toEqual([
      "h0",
      "h1",
      "h2",
    ]);
    expect(moveWishlistItem(list, "nope", 1)).toEqual(list);
  });
});

describe("catalog", () => {
  const catalog = fixtureCatalog();

  it("finds a stored sku whatever its kind, or null when gone", () => {
    expect(findCatalogItem(catalog, "fx-glasshouse")?.kind).toBe(
      "profile_effect",
    );
    expect(findCatalogItem(catalog, "retired")).toBeNull();
    expect(findCatalogItem(null, "fx-glasshouse")).toBeNull();
  });

  it("searches name, collection and kind with every word", () => {
    expect(searchWishlistCatalog(catalog, "").length).toBe(
      catalog.items.length,
    );
    expect(searchWishlistCatalog(catalog, "night").map((i) => i.skuId)).toEqual(
      ["np-nightdrive"],
    );
    expect(
      searchWishlistCatalog(catalog, "shoreline effect").map((i) => i.skuId),
    ).toEqual(["fx-paperboats"]);
    expect(searchWishlistCatalog(catalog, "zzz")).toEqual([]);
    expect(searchWishlistCatalog(catalog, "", 2)).toHaveLength(2);
  });

  it("lists one entry per colour family", () => {
    const base = catalog.items[0]!;
    const variant = {
      ...base,
      skuId: "np-nightdrive-2",
      variantOf: "np-nightdrive",
    };
    const withVariant = {
      ...catalog,
      items: [{ ...base, variantOf: "np-nightdrive" }, variant],
    };
    expect(searchWishlistCatalog(withVariant, "night")).toHaveLength(1);
  });
});
