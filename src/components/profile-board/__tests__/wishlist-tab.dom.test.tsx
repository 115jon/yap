// @vitest-environment jsdom

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  primeCollectiblesCatalogCache,
  resetCollectiblesCatalogClient,
} from "@/lib/collectibles-catalog-client";
import { MAX_WISHLIST_ITEMS, type WishlistItem } from "@/lib/profile-board";

const api = vi.hoisted(() => ({
  fetchWishlist: vi.fn(),
  saveWishlist: vi.fn(),
}));
vi.mock("@/lib/profile-board-api", () => api);
const shop = vi.hoisted(() => ({ openShopFromWishlist: vi.fn() }));
vi.mock("../wishlist-open-shop", () => shop);

import ProfileWishlist from "../ProfileWishlist";
import { fixtureCatalog, WISHLIST_FIXTURE } from "../tab-fixtures-data";

function renderTab(owner: boolean, items: WishlistItem[] = WISHLIST_FIXTURE) {
  api.fetchWishlist.mockResolvedValue({ items, is_owner: owner });
  return render(
    <ProfileWishlist userId="u1" isOwner={owner} displayName="Kiwi" />,
  );
}

function savedSkus() {
  const items = api.saveWishlist.mock.lastCall![0] as WishlistItem[];
  return items.map((item) =>
    item.kind === "shop" ? item.sku : `hint:${item.text}`,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
  resetCollectiblesCatalogClient();
  primeCollectiblesCatalogCache(fixtureCatalog());
  api.saveWishlist.mockImplementation(async (items: WishlistItem[]) => ({
    items,
    is_owner: true,
  }));
});

describe("ProfileWishlist", () => {
  it("is read-only for others and opens the shop from a card", async () => {
    renderTab(false);
    const list = await screen.findByRole("list", { name: "Kiwi's wishlist" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(6);
    expect(screen.getByText("6 items")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^Remove/ })).toBeNull();
    expect(screen.queryByLabelText("Add a hint")).toBeNull();
    expect(screen.queryByText(/gift/i)).toBeNull();

    // A dropped item falls back to its stored name and is not a link.
    expect(screen.getByText("Ember Ring")).toBeInTheDocument();
    expect(screen.getByText("No longer in the shop")).toBeInTheDocument();

    fireEvent.click(
      screen.getByRole("button", {
        name: "Night Drive, Nameplate. Open the shop",
      }),
    );
    expect(shop.openShopFromWishlist).toHaveBeenCalled();
  });

  it("shows an empty state to others", async () => {
    renderTab(false, []);
    expect(
      await screen.findByText("Nothing on Kiwi's wishlist yet"),
    ).toBeInTheDocument();
  });

  it("adds a hint with a counter", async () => {
    renderTab(true, []);
    const field = await screen.findByLabelText("Add a hint");
    const add = screen.getByRole("button", { name: "Add hint" });
    expect(add).toBeDisabled();

    fireEvent.change(field, { target: { value: "x".repeat(81) } });
    expect(screen.getByText("81 / 80")).toBeInTheDocument();
    expect(add).toBeDisabled();

    fireEvent.change(field, { target: { value: "  anything with cats " } });
    fireEvent.click(add);
    expect(savedSkus()).toEqual(["hint:anything with cats"]);
    expect(await screen.findByText("anything with cats")).toBeInTheDocument();
    expect(field).toHaveValue("");
  });

  it("adds a shop item from the picker", async () => {
    renderTab(true);
    fireEvent.click(
      await screen.findByRole("button", { name: "Add from shop" }),
    );
    const picker = screen.getByRole("region", { name: "Add from the shop" });
    fireEvent.change(within(picker).getByLabelText("Search the shop"), {
      target: { value: "copper" },
    });
    expect(
      within(picker).getByRole("button", {
        name: "Add Copperline, Profile effect",
      }),
    ).toBeEnabled();
    fireEvent.click(
      within(picker).getByRole("button", {
        name: "Add Copperline, Profile effect",
      }),
    );
    expect(savedSkus().at(-1)).toBe("fx-copperline");
    expect(
      within(picker).getByRole("button", {
        name: "Copperline, Profile effect, already on your wishlist",
      }),
    ).toBeDisabled();
  });

  it("removes and reorders, and reverts when the save fails", async () => {
    renderTab(true);
    fireEvent.click(
      await screen.findByRole("button", { name: "Remove Glasshouse" }),
    );
    expect(savedSkus()).not.toContain("fx-glasshouse");
    expect(screen.queryByText("Glasshouse")).toBeNull();

    fireEvent.click(
      screen.getByRole("button", { name: "Move Tidelines earlier" }),
    );
    expect(savedSkus().slice(0, 3)).toEqual([
      "np-nightdrive",
      "np-tidelines",
      "hint:Anything with cats. Or a cat. Mostly cats.",
    ]);

    api.saveWishlist.mockRejectedValueOnce(
      Object.assign(new Error("x"), { status: 500 }),
    );
    fireEvent.click(screen.getByRole("button", { name: "Remove Night Drive" }));
    expect(screen.queryByText("Night Drive")).toBeNull();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Your list is back to how it was.",
    );
    expect(screen.getByText("Night Drive")).toBeInTheDocument();
  });

  it("stops adding at the limit", async () => {
    const full: WishlistItem[] = Array.from(
      { length: MAX_WISHLIST_ITEMS },
      (_, i) => ({
        id: `h${i}`,
        kind: "hint",
        text: `hint ${i}`,
      }),
    );
    renderTab(true, full);
    expect(
      await screen.findByText(`${MAX_WISHLIST_ITEMS} / ${MAX_WISHLIST_ITEMS}`),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Your wishlist is full. Remove something to add more."),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Add a hint")).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "Add from shop" }),
    ).toBeDisabled();

    fireEvent.click(screen.getByRole("button", { name: "Remove hint 0" }));
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Add from shop" }),
      ).toBeEnabled(),
    );
  });
});
