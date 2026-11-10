/**
 * A wishlist card's preview: the shop's own still previews (decoration on
 * the owner's avatar, effect and frame on a card, the name on a nameplate),
 * never animated, so an open profile costs nothing at rest. `WishlistThumb`
 * is the picker row's small still.
 */

import {
  DecorationAvatar,
  EffectThumb,
  FrameThumb,
  NameplateRow,
  type PreviewIdentity,
} from "@/components/chat/collectibles/CollectiblePreviews";
import { Icon } from "@/components/ui/icon";
import {
  decorationStillUrl,
  effectStillUrl,
  nameplateStillUrl,
} from "@/lib/collectible-assets";
import type { CollectibleCatalogItem } from "@/lib/collectibles-catalog";
import { cn } from "@/lib/utils";

export function WishlistArt({
  item,
  identity,
}: {
  item: CollectibleCatalogItem;
  identity: PreviewIdentity;
}) {
  if (item.kind === "avatar_decoration") {
    return (
      <span className="w-[42%] max-w-28">
        <DecorationAvatar item={item} identity={identity} stillSize={160} />
      </span>
    );
  }
  if (item.kind === "profile_effect") {
    return (
      <span className="aspect-[450/880] h-[88%]">
        <EffectThumb item={item} />
      </span>
    );
  }
  if (item.kind === "profile_frame") {
    return (
      <span className="aspect-[450/880] h-[76%]">
        <FrameThumb item={item} identity={identity} />
      </span>
    );
  }
  return (
    <span className="w-[88%]">
      <NameplateRow item={item} identity={identity} />
    </span>
  );
}

function stillUrl(item: CollectibleCatalogItem): string | undefined {
  switch (item.kind) {
    case "avatar_decoration":
      return decorationStillUrl(item, 64);
    case "profile_effect":
      return effectStillUrl(item, 64);
    case "nameplate":
      return nameplateStillUrl(item);
    case "profile_frame":
      return item.staticUrl ?? item.frame?.layers[0]?.src;
  }
}

export function WishlistThumb({
  item,
  className,
}: {
  item: CollectibleCatalogItem;
  className?: string;
}) {
  const src = stillUrl(item);
  return (
    <span
      aria-hidden="true"
      className={cn(
        "flex size-9 shrink-0 items-center justify-center overflow-hidden rounded-[2px] bg-tint-2 text-ink-3",
        className,
      )}
    >
      {src ? (
        <img
          src={src}
          alt=""
          loading="lazy"
          decoding="async"
          draggable={false}
          className={cn(
            "size-full",
            item.kind === "nameplate"
              ? "object-cover object-right"
              : "object-contain",
          )}
        />
      ) : (
        <Icon name="shop" size="sm" />
      )}
    </span>
  );
}
