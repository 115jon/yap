/**
 * Dev-only fixtures for the profile's Activity and Wishlist tabs, mounted by
 * `profile-board-fixtures` for every `pb-activity*` and `pb-wishlist*`
 * surface. Each renders the real tab with fixture data (no network; edits
 * stay local) inside a 744px panel shaped like the profile modal's right
 * column, full width on a phone.
 *
 *   /ui-fixtures?surface=pb-activity          someone else's timeline
 *   /ui-fixtures?surface=pb-activity-own      yours: settings, a hidden event
 *   /ui-fixtures?surface=pb-activity-empty    nothing yet (add &own=1 for yours)
 *   /ui-fixtures?surface=pb-wishlist          someone else's, read-only
 *   /ui-fixtures?surface=pb-wishlist-own      yours: add, remove, reorder
 *   /ui-fixtures?surface=pb-wishlist-empty    nothing yet (add &own=1 for yours)
 *
 * Add `&theme=light` (or miku-dark, spiderman-light, …) for other themes.
 */

import { lazy, Suspense, useState } from "react";

import {
  activityFixture,
  emptyActivityFixture,
  FIXTURE_OWNER,
  fixtureCatalog,
  ownActivityFixture,
  WISHLIST_FIXTURE,
} from "@/components/profile-board/tab-fixtures-data";
import { primeCollectiblesCatalogCache } from "@/lib/collectibles-catalog-client";
import { cn } from "@/lib/utils";

const ProfileActivity = lazy(
  () => import("@/components/profile-board/ProfileActivity"),
);
const ProfileWishlist = lazy(
  () => import("@/components/profile-board/ProfileWishlist"),
);

let catalogPrimed = false;
function primeCatalog() {
  if (catalogPrimed) return;
  catalogPrimed = true;
  primeCollectiblesCatalogCache(fixtureCatalog());
}

function ownParam(): boolean {
  if (typeof window === "undefined") return false;
  return new URLSearchParams(window.location.search).get("own") === "1";
}

const TABS = ["Board", "Activity", "Wishlist"] as const;

/** The modal's right column: its tab strip over a scrolling body. */
function Panel({
  tab,
  children,
}: {
  tab: (typeof TABS)[number];
  children: React.ReactNode;
}) {
  return (
    <div className="flex min-h-full w-full justify-center bg-ply-0 md:items-start md:p-10">
      <section
        aria-label={`Profile, ${tab}`}
        className="flex w-full max-w-[744px] flex-col bg-ply-3 md:rounded-[2px] md:border md:border-line-strong md:shadow-modal"
      >
        <div
          aria-hidden="true"
          className="flex h-14 shrink-0 items-end gap-6 border-b border-line px-4 md:px-7"
        >
          {TABS.map((name) => (
            <span
              key={name}
              className={cn(
                "flex h-full items-center border-b-2 font-display text-[15px] font-semibold",
                name === tab
                  ? "border-ink-1 text-ink-1"
                  : "border-transparent text-ink-3",
              )}
            >
              {name}
              {name === "Board" ? (
                <span className="ms-1.5 text-num-xs text-ink-3">11</span>
              ) : null}
            </span>
          ))}
        </div>
        <div className="px-4 py-5 md:px-7 md:py-6">{children}</div>
      </section>
    </div>
  );
}

export default function TabFixtures({ surface }: { surface: string }) {
  // Built once per mount: the fixed-clock data is relative to today.
  const [data] = useState(() => {
    const own = ownParam();
    switch (surface) {
      case "pb-activity-own":
        return {
          tab: "Activity" as const,
          owner: true,
          activity: ownActivityFixture(),
        };
      case "pb-activity-empty":
        return {
          tab: "Activity" as const,
          owner: own,
          activity: emptyActivityFixture(own),
        };
      case "pb-wishlist":
        return {
          tab: "Wishlist" as const,
          owner: false,
          wishlist: WISHLIST_FIXTURE,
        };
      case "pb-wishlist-own":
        return {
          tab: "Wishlist" as const,
          owner: true,
          wishlist: WISHLIST_FIXTURE,
        };
      case "pb-wishlist-empty":
        return { tab: "Wishlist" as const, owner: own, wishlist: [] };
      default:
        return {
          tab: "Activity" as const,
          owner: false,
          activity: activityFixture(),
        };
    }
  });
  if (data.tab === "Wishlist") primeCatalog();

  return (
    <Panel tab={data.tab}>
      <Suspense fallback={null}>
        {data.tab === "Activity" ? (
          <ProfileActivity
            userId={FIXTURE_OWNER.id}
            isOwner={data.owner}
            displayName={FIXTURE_OWNER.name}
            fixture={data.activity}
          />
        ) : (
          <ProfileWishlist
            userId={FIXTURE_OWNER.id}
            isOwner={data.owner}
            displayName={FIXTURE_OWNER.name}
            fixture={data.wishlist}
          />
        )}
      </Suspense>
    </Panel>
  );
}
