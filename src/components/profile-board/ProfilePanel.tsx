/**
 * The full profile modal's right column: Board · Activity · Wishlist rule
 * tabs over the chosen tab's panel. The Board tab shows the Rack; the owner
 * gets "Edit board", which swaps the Rack for the editor in place (loaded on
 * demand, like Activity and Wishlist).
 *
 * The panel is a size container: below 540px (the phone sheet) its gutters
 * drop to 16px. It does not scroll itself; the modal owns the scroller, and
 * the tab strip sticks to its top.
 */

import { lazy, Suspense, useId, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import { Tabs, tabId, type TabItem } from "@/components/ui/tabs";
import type {
  ActivityPage,
  BoardResponse,
  QuoteSnapshot,
  WishlistItem,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { visibleWidgetCount } from "./board-format";
import { ProfileBoard } from "./ProfileBoard";
import { useProfileBoard } from "./useProfileBoard";

const ProfileActivity = lazy(() => import("./ProfileActivity"));
const ProfileWishlist = lazy(() => import("./ProfileWishlist"));
const BoardEditor = lazy(() => import("./BoardEditor"));

export type ProfilePanelTab = "board" | "activity" | "wishlist";

export interface ProfilePanelProps {
  userId: string;
  isOwner: boolean;
  displayName: string;
  /** The owner's avatar (the pinned message's byline). */
  avatarUrl?: string | null;
  /** /ui-fixtures and tests: render these instead of fetching. */
  fixtures?: {
    board?: BoardResponse;
    activity?: ActivityPage;
    wishlist?: WishlistItem[];
  };
  initialTab?: ProfilePanelTab;
  /** "Open Studio" from the editor. */
  onOpenStudio?(): void;
  /** Jump to a pinned message (shows the Jump key on it). */
  onOpenMessage?(messageId: string, snapshot: QuoteSnapshot): void;
  /** Frozen clock (fixtures, tests). */
  now?: number;
  className?: string;
}

export function ProfilePanel({
  userId,
  isOwner,
  displayName,
  avatarUrl,
  fixtures,
  initialTab = "board",
  onOpenStudio,
  onOpenMessage,
  now,
  className,
}: ProfilePanelProps) {
  const idBase = useId();
  const panelId = `${idBase}-panel`;
  const [tab, setTab] = useState<ProfilePanelTab>(initialTab);
  const [editing, setEditing] = useState(false);
  const board = useProfileBoard(userId, { fixture: fixtures?.board });
  const response = board.board;
  const owns = isOwner && (response?.is_owner ?? true);
  const count = response ? visibleWidgetCount(response) : null;

  const items: TabItem<ProfilePanelTab>[] = [
    {
      value: "board",
      label: "Board",
      adornment:
        count !== null && count > 0 ? (
          <Num className="font-normal text-ink-3">{count}</Num>
        ) : undefined,
    },
    { value: "activity", label: "Activity" },
    { value: "wishlist", label: "Wishlist" },
  ];

  const showEdit =
    owns &&
    tab === "board" &&
    !editing &&
    !!response &&
    response.board.widgets.length > 0;

  let body: ReactNode;
  if (tab === "activity") {
    body = (
      <Suspense fallback={<TabLoading />}>
        <ProfileActivity
          userId={userId}
          isOwner={isOwner}
          displayName={displayName}
          fixture={fixtures?.activity}
        />
      </Suspense>
    );
  } else if (tab === "wishlist") {
    body = (
      <Suspense fallback={<TabLoading />}>
        <ProfileWishlist
          userId={userId}
          isOwner={isOwner}
          displayName={displayName}
          fixture={fixtures?.wishlist}
        />
      </Suspense>
    );
  } else if (editing && response) {
    body = (
      <Suspense fallback={<RackSkeleton />}>
        <BoardEditor
          initial={response}
          onExit={(next) => {
            if (next) board.replace(next);
            setEditing(false);
          }}
          onOpenStudio={onOpenStudio}
        />
      </Suspense>
    );
  } else if (!response) {
    body =
      board.status === "error" ? (
        <div role="alert" className="flex flex-col items-start gap-3 py-6">
          <p className="text-ui-sm text-ink-2">{board.error}</p>
          <Button variant="secondary" size="sm" onClick={board.retry}>
            <Icon name="refresh" size="sm" />
            Try again
          </Button>
        </div>
      ) : (
        <RackSkeleton />
      );
  } else if (response.board.widgets.length === 0 || count === 0) {
    body = owns ? (
      <EmptyState
        data-slot="board-empty"
        title="Your board is empty"
        description="Add what you're playing, your mood, photos or a countdown. Everyone who opens your profile sees it."
        action={
          <Button variant="primary" onClick={() => setEditing(true)}>
            Edit board
          </Button>
        }
        className="min-h-[320px]"
      />
    ) : (
      <p
        data-slot="board-empty"
        className="py-6 text-ui-sm text-pretty text-ink-3"
      >
        {displayName} hasn't added anything to their board yet.
      </p>
    );
  } else {
    body = (
      <ProfileBoard
        response={response}
        owner={{ id: userId, name: displayName, avatarUrl }}
        now={now}
        onOpenMessage={onOpenMessage}
      />
    );
  }

  return (
    <div
      data-slot="profile-panel"
      className={cn(
        "@container/panel flex min-w-0 flex-col bg-ply-3",
        className,
      )}
    >
      <div className="sticky top-0 z-10 flex h-14 items-end gap-3 border-b border-line bg-ply-3 px-7 @max-[540px]/panel:h-12 @max-[540px]/panel:px-4">
        <Tabs
          label={`${displayName}'s profile`}
          items={items}
          value={tab}
          onValueChange={(next) => {
            setTab(next);
            setEditing(false);
          }}
          idBase={idBase}
          panelId={panelId}
          scrollable
          className="min-w-0 flex-1 self-stretch border-b-0 [&>[role=tab]]:min-h-full"
        />
        {showEdit ? (
          <Button
            variant="secondary"
            size="sm"
            className="mb-2 shrink-0 @max-[540px]/panel:mb-1.5"
            onClick={() => setEditing(true)}
          >
            <Icon name="edit" size="sm" />
            Edit board
          </Button>
        ) : null}
      </div>
      <div
        id={panelId}
        role="tabpanel"
        aria-labelledby={tabId(idBase, tab)}
        className="px-7 pt-5 pb-7 @max-[540px]/panel:px-4 @max-[540px]/panel:pt-4 @max-[540px]/panel:pb-6"
      >
        {body}
      </div>
    </div>
  );
}

function TabLoading() {
  return (
    <p role="status" className="py-6 text-ui-sm text-ink-3">
      Loading…
    </p>
  );
}

/** Static plates in the Rack's shape while the board loads. No shimmer. */
function RackSkeleton() {
  const cells = [
    "col-span-2 row-span-1",
    "row-span-1",
    "row-span-1",
    "row-span-2",
    "row-span-2",
  ];
  return (
    <div
      role="status"
      aria-label="Loading board"
      className="@container/rack pt-2"
    >
      <div className="grid grid-cols-2 gap-x-3.5 gap-y-[22px] [grid-auto-rows:168px] @max-[540px]/rack:gap-x-2.5 @max-[540px]/rack:gap-y-5 @max-[540px]/rack:[grid-auto-rows:151px]">
        {cells.map((span, index) => (
          <span
            key={index}
            aria-hidden="true"
            className={cn("rounded-[2px] border border-line bg-tint-1", span)}
          />
        ))}
      </div>
    </div>
  );
}

export default ProfilePanel;
