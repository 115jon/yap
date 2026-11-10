/**
 * The Wishlist tab of the full profile: a hint list. The owner adds shop
 * collectibles they'd like (each card opens the shop) or free-text hints,
 * removes and reorders them; everyone else reads it. Nothing here is for
 * sale or gifting yet, so there are no prices and no gift keys.
 *
 * Shop items store `{ sku, name }`; the card resolves its preview through
 * the cached shop catalog and falls back to the stored name when the shop
 * has dropped the item. Saves are optimistic: the list changes at once and
 * goes back, with the reason in words, when the save fails.
 *
 * Previews are stills; nothing animates at rest.
 */

import * as React from "react";

import type { PreviewIdentity } from "@/components/chat/collectibles/CollectiblePreviews";
import { useCollectiblesCatalog } from "@/components/chat/collectibles/hooks";
import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { IconButton } from "@/components/ui/IconButton";
import { FieldPlate, Input } from "@/components/ui/input";
import { Num } from "@/components/ui/num";
import { Skeleton } from "@/components/ui/skeleton";
import { describeApiError } from "@/lib/api-errors";
import type {
  CollectibleCatalogItem,
  CollectiblesCatalog,
} from "@/lib/collectibles-catalog";
import {
  MAX_WISHLIST_ITEMS,
  TEXT_LIMITS,
  textLength,
  type WishlistItem,
} from "@/lib/profile-board";
import { fetchWishlist, saveWishlist } from "@/lib/profile-board-api";
import { cn } from "@/lib/utils";

import { WishlistArt, WishlistThumb } from "./wishlist-art";
import {
  addWishlistHint,
  addWishlistShopItem,
  findCatalogItem,
  isWishlistFull,
  moveWishlistItem,
  removeWishlistItem,
  searchWishlistCatalog,
  wishlistKindLabel,
} from "./wishlist-model";
import { openShopFromWishlist } from "./wishlist-open-shop";

export interface ProfileWishlistProps {
  userId: string;
  isOwner: boolean;
  displayName: string;
  /** Fixtures: render these items instead of fetching. */
  fixture?: WishlistItem[];
}

type Load =
  | { status: "loading" }
  | { status: "error"; message: string }
  | { status: "ready" };

export default function ProfileWishlist({
  userId,
  isOwner,
  displayName,
  fixture,
}: ProfileWishlistProps) {
  const [items, setItems] = React.useState<WishlistItem[]>(() => fixture ?? []);
  const [load, setLoad] = React.useState<Load>(
    fixture ? { status: "ready" } : { status: "loading" },
  );
  const [reload, setReload] = React.useState(0);
  const [saveError, setSaveError] = React.useState<string | null>(null);
  const [announcement, setAnnouncement] = React.useState("");
  const saved = React.useRef<WishlistItem[]>(fixture ?? []);
  const saveSeq = React.useRef(0);
  const listRef = React.useRef<HTMLUListElement>(null);
  const pendingFocus = React.useRef<{
    id: string;
    dir: "earlier" | "later";
  } | null>(null);
  const {
    catalog,
    loading: catalogLoading,
    error: catalogError,
  } = useCollectiblesCatalog();

  React.useEffect(() => {
    if (fixture) {
      setItems(fixture);
      saved.current = fixture;
      return;
    }
    let cancelled = false;
    setLoad({ status: "loading" });
    fetchWishlist(userId)
      .then((response) => {
        if (cancelled) return;
        saved.current = response.items;
        setItems(response.items);
        setLoad({ status: "ready" });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        setLoad({
          status: "error",
          message: describeApiError(error, "load this wishlist"),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [userId, fixture, reload]);

  const commit = React.useCallback(
    (next: WishlistItem[], said: string) => {
      setItems(next);
      setSaveError(null);
      setAnnouncement(said);
      if (fixture) {
        saved.current = next;
        return;
      }
      const seq = ++saveSeq.current;
      saveWishlist(next)
        .then((response) => {
          saved.current = response.items;
          if (seq === saveSeq.current) setItems(response.items);
        })
        .catch((error: unknown) => {
          if (seq !== saveSeq.current) return;
          setItems(saved.current);
          setSaveError(
            `${describeApiError(error, "save your wishlist")} Your list is back to how it was.`,
          );
        });
    },
    [fixture],
  );

  // Keep focus on the moved card's control after a reorder.
  React.useLayoutEffect(() => {
    const target = pendingFocus.current;
    if (!target) return;
    pendingFocus.current = null;
    const card = listRef.current?.querySelector<HTMLElement>(
      `[data-item-id="${cssId(target.id)}"]`,
    );
    const same = card?.querySelector<HTMLButtonElement>(
      `[data-move="${target.dir}"]`,
    );
    const other = card?.querySelector<HTMLButtonElement>(
      `[data-move="${target.dir === "earlier" ? "later" : "earlier"}"]`,
    );
    (same && !same.disabled ? same : other)?.focus();
  }, [items]);

  const itemName = (item: WishlistItem) =>
    item.kind === "hint" ? item.text : item.name;

  const onMove = (item: WishlistItem, dir: "earlier" | "later") => {
    pendingFocus.current = { id: item.id, dir };
    const next = moveWishlistItem(items, item.id, dir === "earlier" ? -1 : 1);
    const position = next.findIndex((entry) => entry.id === item.id) + 1;
    commit(next, `${itemName(item)} moved to ${position} of ${next.length}`);
  };
  const onRemove = (item: WishlistItem) => {
    const index = items.findIndex((entry) => entry.id === item.id);
    const next = removeWishlistItem(items, item.id);
    commit(next, `Removed ${itemName(item)}`);
    // Focus the neighbour that took its place, or the composer when empty.
    const neighbour = next[Math.min(index, next.length - 1)];
    requestAnimationFrame(() => {
      const card = neighbour
        ? listRef.current?.querySelector<HTMLElement>(
            `[data-item-id="${cssId(neighbour.id)}"] [data-remove]`,
          )
        : null;
      card?.focus();
    });
  };

  const identity: PreviewIdentity = { displayName };
  const count = items.length;
  const full = isWishlistFull(items);

  return (
    <div
      data-slot="profile-wishlist"
      className="@container flex flex-col gap-4"
    >
      <header className="flex items-start justify-between gap-4">
        <div className="min-w-0">
          <h3 className="font-display text-[20px] leading-6 font-extrabold tracking-[-0.02em] text-balance text-ink-1">
            {isOwner ? "Your wishlist" : `${displayName}'s wishlist`}
          </h3>
          <p className="mt-1 max-w-[60ch] text-ui-sm text-pretty text-ink-2">
            {isOwner
              ? "Shop things you'd like, and hints for anything else. Anyone who can open your profile sees this list."
              : `What ${displayName} has an eye on in the shop, plus a few hints.`}
          </p>
        </div>
        {load.status === "ready" ? (
          <p className="shrink-0 pt-1 text-ink-3">
            {isOwner ? (
              <Num size="sm" className={full ? "text-ink-1" : undefined}>
                {count} / {MAX_WISHLIST_ITEMS}
                <span className="sr-only"> items</span>
              </Num>
            ) : (
              <Num size="sm">
                {count} {count === 1 ? "item" : "items"}
              </Num>
            )}
          </p>
        ) : null}
      </header>

      <p aria-live="polite" className="sr-only">
        {announcement}
      </p>

      {isOwner && load.status === "ready" ? (
        <WishlistComposer
          items={items}
          full={full}
          catalog={catalog}
          catalogLoading={catalogLoading}
          catalogError={catalogError}
          onAdd={commit}
        />
      ) : null}

      {saveError ? (
        <p
          role="alert"
          className="rounded-[2px] bg-danger-tint px-3 py-2 text-ui-sm text-danger-ink"
        >
          {saveError}
        </p>
      ) : null}

      {load.status === "loading" ? (
        <WishlistSkeleton />
      ) : load.status === "error" ? (
        <EmptyState
          size="sm"
          title="The wishlist didn't load"
          description={load.message}
          action={
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setReload((n) => n + 1)}
            >
              <Icon name="refresh" size="sm" />
              Try again
            </Button>
          }
        />
      ) : count === 0 ? (
        isOwner ? (
          <EmptyState
            size="sm"
            title="Your wishlist is empty"
            description="Add something from the shop, or a hint like “anything with cats”."
          />
        ) : (
          <EmptyState
            size="sm"
            title={`Nothing on ${displayName}'s wishlist yet`}
          />
        )
      ) : (
        <ul
          ref={listRef}
          aria-label={isOwner ? "Your wishlist" : `${displayName}'s wishlist`}
          className="grid grid-cols-2 gap-3 @xl:grid-cols-3"
        >
          {items.map((item, index) => {
            const catalogItem =
              item.kind === "shop" ? findCatalogItem(catalog, item.sku) : null;
            return (
              <WishlistCard
                key={item.id}
                item={item}
                catalogItem={catalogItem}
                background={
                  catalogItem
                    ? catalog?.categories.find(
                        (category) => category.id === catalogItem.categoryId,
                      )?.background
                    : undefined
                }
                catalogLoading={catalogLoading}
                identity={identity}
                tools={
                  isOwner ? (
                    <WishlistTools
                      name={itemName(item)}
                      first={index === 0}
                      last={index === count - 1}
                      onMove={(dir) => onMove(item, dir)}
                      onRemove={() => onRemove(item)}
                    />
                  ) : null
                }
              />
            );
          })}
        </ul>
      )}
    </div>
  );
}

/** An item id inside an attribute selector (ids are [A-Za-z0-9_-]). */
function cssId(id: string) {
  return typeof CSS !== "undefined" && CSS.escape
    ? CSS.escape(id)
    : id.replace(/["\\]/g, "\\$&");
}

// ── Cards ────────────────────────────────────────────────────────────────

const CARD =
  "flex min-w-0 flex-col overflow-hidden rounded-[2px] border border-line bg-ply-2";

function WishlistCard({
  item,
  catalogItem,
  background,
  catalogLoading,
  identity,
  tools,
}: {
  item: WishlistItem;
  catalogItem: CollectibleCatalogItem | null;
  background?: string;
  catalogLoading: boolean;
  identity: PreviewIdentity;
  tools: React.ReactNode;
}) {
  if (item.kind === "hint") {
    return (
      <li data-item-id={item.id} data-kind="hint" className={CARD}>
        <div className="flex flex-1 flex-col gap-2 p-3">
          <span className="silkscreen inline-flex items-center gap-1.5 text-ink-3">
            <Icon name="lightbulb" size="xs" />
            Hint
          </span>
          <p className="text-ui-lg break-words text-pretty text-ink-1">
            {item.text}
          </p>
        </div>
        {tools}
      </li>
    );
  }

  const kind = catalogItem ? wishlistKindLabel(catalogItem) : null;
  const body = (
    <>
      <span
        className="relative flex aspect-[4/3] w-full items-center justify-center overflow-hidden bg-tint-1"
        // A collection's own backdrop (catalog art), as the shop shows it.
        style={background ? { background } : undefined}
      >
        {catalogItem ? (
          <WishlistArt item={catalogItem} identity={identity} />
        ) : catalogLoading ? null : (
          <Icon name="shop" size="2xl" className="text-ink-3" />
        )}
      </span>
      <span className="flex min-w-0 flex-col gap-1 px-3 pt-2 pb-2.5 text-start">
        <span className="font-display text-[15px] leading-5 font-bold break-words text-ink-1">
          {item.name}
        </span>
        <span className="flex min-w-0 items-center justify-between gap-2">
          <span className="min-w-0 truncate text-ui-xs text-ink-3">
            {kind ?? (catalogLoading ? "Shop item" : "No longer in the shop")}
          </span>
          {catalogItem ? (
            <span className="silkscreen inline-flex h-5 shrink-0 items-center rounded-[2px] bg-tint-2 px-1.5 text-ink-2">
              Free
            </span>
          ) : null}
        </span>
      </span>
    </>
  );

  return (
    <li data-item-id={item.id} data-kind="shop" className={CARD}>
      {catalogItem ? (
        <button
          type="button"
          onClick={() => void openShopFromWishlist()}
          aria-label={`${item.name}, ${kind}. Open the shop`}
          className={cn(
            "relative flex flex-1 flex-col outline-none [-webkit-tap-highlight-color:transparent]",
            "after:pointer-events-none after:absolute after:inset-0 after:bg-tint-1 after:opacity-0 after:transition-opacity after:duration-(--dur-1) pointer-fine:hover:after:opacity-100",
            "focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus",
          )}
        >
          {body}
        </button>
      ) : (
        <div className="flex flex-1 flex-col">{body}</div>
      )}
      {tools}
    </li>
  );
}

function WishlistTools({
  name,
  first,
  last,
  onMove,
  onRemove,
}: {
  name: string;
  first: boolean;
  last: boolean;
  onMove: (dir: "earlier" | "later") => void;
  onRemove: () => void;
}) {
  return (
    <div className="mt-auto flex h-10 items-center gap-0.5 border-t border-line px-1">
      <IconButton
        size={32}
        variant="muted"
        icon="chevronLeft"
        data-move="earlier"
        aria-label={`Move ${name} earlier`}
        disabled={first}
        onClick={() => onMove("earlier")}
      />
      <IconButton
        size={32}
        variant="muted"
        icon="chevronRight"
        data-move="later"
        aria-label={`Move ${name} later`}
        disabled={last}
        onClick={() => onMove("later")}
      />
      <IconButton
        size={32}
        variant="destructive"
        icon="trash"
        data-remove=""
        aria-label={`Remove ${name}`}
        onClick={onRemove}
        className="ms-auto"
      />
    </div>
  );
}

function WishlistSkeleton() {
  return (
    <div role="status" className="grid grid-cols-2 gap-3 @xl:grid-cols-3">
      <span className="sr-only">Loading the wishlist</span>
      {[0, 1, 2].map((i) => (
        <div key={i} className={CARD}>
          <Skeleton className="aspect-[4/3] w-full rounded-none" />
          <span className="flex flex-col gap-2 px-3 py-3">
            <Skeleton className="h-3 w-2/3 rounded-[2px]" />
            <Skeleton className="h-2.5 w-1/2 rounded-[2px]" />
          </span>
        </div>
      ))}
    </div>
  );
}

// ── Adding ───────────────────────────────────────────────────────────────

const HINT_ERRORS: Record<
  "full" | "empty" | "too_long" | "duplicate",
  string | null
> = {
  // Full is said once, above the form.
  full: null,
  duplicate: null,
  empty: "Write a hint first.",
  too_long: `Hints are up to ${TEXT_LIMITS.wishlistHint} characters.`,
};

function WishlistComposer({
  items,
  full,
  catalog,
  catalogLoading,
  catalogError,
  onAdd,
}: {
  items: WishlistItem[];
  full: boolean;
  catalog: CollectiblesCatalog | null;
  catalogLoading: boolean;
  catalogError: string | null;
  onAdd: (next: WishlistItem[], said: string) => void;
}) {
  const id = React.useId();
  const [hint, setHint] = React.useState("");
  const [hintError, setHintError] = React.useState<string | null>(null);
  const [picking, setPicking] = React.useState(false);
  const length = textLength(hint.trim());
  const over = length > TEXT_LIMITS.wishlistHint;

  const submitHint = (event: React.FormEvent) => {
    event.preventDefault();
    const result = addWishlistHint(items, hint);
    if (!result.ok) {
      setHintError(HINT_ERRORS[result.reason]);
      return;
    }
    setHint("");
    setHintError(null);
    onAdd(result.items, `Added hint: ${hint.trim()}`);
  };

  return (
    <div className="flex flex-col gap-3">
      {full ? (
        <p className="text-ui-sm text-ink-2">
          Your wishlist is full. Remove something to add more.
        </p>
      ) : null}
      <form onSubmit={submitHint} className="flex flex-wrap items-end gap-2">
        <FieldPlate
          label="Add a hint"
          htmlFor={`${id}-hint`}
          // The profile modal is a dialog plate (ply-3).
          ground="ply-3"
          className="min-w-0 basis-full @md:basis-auto @md:flex-1"
          hint={
            <span className={cn("text-num-xs", over && "text-danger-ink")}>
              {length} / {TEXT_LIMITS.wishlistHint}
            </span>
          }
        >
          <Input
            id={`${id}-hint`}
            value={hint}
            disabled={full}
            placeholder="Anything with cats, a Balatro deck…"
            autoComplete="off"
            aria-invalid={over || Boolean(hintError) || undefined}
            aria-describedby={hintError ? `${id}-hint-error` : undefined}
            onChange={(event) => {
              setHint(event.target.value);
              if (hintError) setHintError(null);
            }}
          />
        </FieldPlate>
        <Button
          type="submit"
          variant="secondary"
          disabled={full || length === 0 || over}
        >
          Add hint
        </Button>
        <Button
          type="button"
          variant="secondary"
          disabled={full}
          aria-expanded={picking}
          aria-controls={`${id}-picker`}
          onClick={() => setPicking((open) => !open)}
        >
          <Icon name="shop" size="sm" />
          {picking ? "Done" : "Add from shop"}
        </Button>
      </form>
      {hintError ? (
        <p
          id={`${id}-hint-error`}
          role="alert"
          className="text-ui-xs text-danger-ink"
        >
          {hintError}
        </p>
      ) : null}
      {picking && !full ? (
        <WishlistPicker
          id={`${id}-picker`}
          items={items}
          catalog={catalog}
          loading={catalogLoading}
          error={catalogError}
          onPick={(item) => {
            const result = addWishlistShopItem(items, item);
            if (result.ok) onAdd(result.items, `Added ${item.name}`);
          }}
        />
      ) : null}
    </div>
  );
}

function WishlistPicker({
  id,
  items,
  catalog,
  loading,
  error,
  onPick,
}: {
  id: string;
  items: WishlistItem[];
  catalog: CollectiblesCatalog | null;
  loading: boolean;
  error: string | null;
  onPick: (item: CollectibleCatalogItem) => void;
}) {
  const [query, setQuery] = React.useState("");
  const results = React.useMemo(
    () => searchWishlistCatalog(catalog, query),
    [catalog, query],
  );
  const added = React.useMemo(
    () =>
      new Set(
        items.flatMap((item) => (item.kind === "shop" ? [item.sku] : [])),
      ),
    [items],
  );
  const searchId = `${id}-search`;
  const searchRef = React.useRef<HTMLInputElement>(null);
  // Opened from "Add from shop": start typing straight away.
  React.useEffect(() => {
    searchRef.current?.focus();
  }, []);

  return (
    <div
      id={id}
      role="region"
      aria-label="Add from the shop"
      className="flex flex-col gap-2 rounded-[2px] border border-line-strong bg-ply-1 p-3 [--plate-ground:var(--ply-1)]"
    >
      <FieldPlate label="Search the shop" htmlFor={searchId} ground="ply-1">
        <Input
          ref={searchRef}
          id={searchId}
          type="search"
          value={query}
          autoComplete="off"
          placeholder="Name, collection or kind"
          onChange={(event) => setQuery(event.target.value)}
        />
      </FieldPlate>
      {loading && !catalog ? (
        <p role="status" className="px-1 py-3 text-ui-sm text-ink-3">
          Loading the shop…
        </p>
      ) : error && !catalog ? (
        <p role="alert" className="px-1 py-3 text-ui-sm text-danger-ink">
          {error}
        </p>
      ) : results.length === 0 ? (
        <p className="px-1 py-3 text-ui-sm text-ink-3">
          {query.trim()
            ? `Nothing in the shop matches “${query.trim()}”.`
            : "The shop is empty right now."}
        </p>
      ) : (
        <ul
          aria-label="Shop items"
          className="-mx-1 flex max-h-72 flex-col gap-0.5 overflow-y-auto overscroll-contain"
        >
          {results.map((item) => {
            const isAdded = added.has(item.skuId);
            return (
              <li key={item.skuId}>
                <button
                  type="button"
                  disabled={isAdded}
                  onClick={() => onPick(item)}
                  aria-label={
                    isAdded
                      ? `${item.name}, ${wishlistKindLabel(item)}, already on your wishlist`
                      : `Add ${item.name}, ${wishlistKindLabel(item)}`
                  }
                  className="flex min-h-12 w-full items-center gap-3 rounded-[2px] px-2 py-1.5 text-start outline-none pointer-fine:hover:enabled:bg-tint-1 focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-focus disabled:cursor-default"
                >
                  <WishlistThumb item={item} />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="truncate text-ui-row font-medium text-ink-1">
                      {item.name}
                    </span>
                    <span className="truncate text-ui-xs text-ink-3">
                      {wishlistKindLabel(item)} · {item.categoryName}
                    </span>
                  </span>
                  {isAdded ? (
                    <span className="silkscreen inline-flex shrink-0 items-center gap-1 text-ink-3">
                      <Icon name="check" size="xs" />
                      Added
                    </span>
                  ) : (
                    <Icon
                      name="add"
                      size="sm"
                      className="shrink-0 text-ink-2"
                    />
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
