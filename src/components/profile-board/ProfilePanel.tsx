/**
 * The full profile modal's right column: Board · Activity · Wishlist rule
 * tabs over the chosen tab's panel. The Board tab shows the Rack; the owner
 * gets "Edit board", which swaps the Rack for the editor in place (loaded on
 * demand, like Activity and Wishlist).
 *
 * The panel is a size container: below 540px (the phone sheet) its gutters
 * drop to 16px. The host decides whether it scrolls (the desktop modal caps
 * its height and makes it the scroller; the phone sheet scrolls it with the
 * card); the tab strip sticks to the scroller's top. Neither part shrinks,
 * so a capped panel scrolls instead of squashing the strip.
 *
 * The owner's empty board is not a lone message at the foot of the
 * profile: it is the Rack's own shape (BoardEmpty), a full-width plate with
 * the call to action over dashed slots for the widgets people add first,
 * in the board column beside the profile card.
 */

import { lazy, Suspense, useId, useState, type ReactNode } from "react";

import { Button } from "@/components/ui/button";
import { EmptyState } from "@/components/ui/empty-state";
import { Icon } from "@/components/ui/icon";
import { Num } from "@/components/ui/num";
import { Tabs, tabId, type TabItem } from "@/components/ui/tabs";
import {
  WIDGET_SPECS,
  type ActivityPage,
  type BoardResponse,
  type QuoteSnapshot,
  type WidgetType,
  type WishlistItem,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";

import { visibleWidgetCount, WIDGET_ICON } from "./board-format";
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
  /**
   * Classes for the sticky tab strip (the profile modal keeps its end clear
   * of the dialog's close key).
   */
  headerClassName?: string;
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
  headerClassName,
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
            if (next) {
              board.replace(next);
              // A save answers with the board, not its data: a plate just
              // added (Guestbook, Listening, Top games…) has nothing to show
              // until the board is fetched again. The gateway's update for
              // our own save is skipped (same version), so fetch it here.
              if (next.version !== response.version) board.retry();
            }
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
      <BoardEmpty onEdit={() => setEditing(true)} />
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
      <div
        className={cn(
          "sticky top-0 z-10 flex h-14 shrink-0 items-end gap-3 border-b border-line bg-ply-3 px-7 @max-[540px]/panel:h-12 @max-[540px]/panel:px-4",
          headerClassName,
        )}
      >
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
        className="shrink-0 px-7 pt-5 pb-7 @max-[540px]/panel:px-4 @max-[540px]/panel:pt-4 @max-[540px]/panel:pb-6"
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

/** The widgets an empty board suggests first, as dashed Rack slots. */
const EMPTY_SLOTS: readonly WidgetType[] = [
  "now_playing",
  "mood",
  "photos",
  "countdown",
];

/**
 * The owner's empty board, in the Rack's shape: a full-width plate with
 * the call to action (EmptyState) over dashed S slots for the widgets
 * people add first. A slot is a mouse shortcut into the editor; keyboard
 * and screen reader users get the one "Edit board" button. Below 540px of
 * rack the slots drop to one row.
 */
function BoardEmpty({ onEdit }: { onEdit(): void }) {
  return (
    <div data-slot="board-empty" className="@container/rack pt-2">
      <div className="grid grid-cols-2 gap-x-3.5 gap-y-[22px] @max-[540px]/rack:gap-x-2.5 @max-[540px]/rack:gap-y-5">
        <section
          aria-label="Your board"
          className="relative col-span-2 rounded-[2px] border border-line-strong bg-ply-2 [--legend-ground:var(--plate-ground,var(--ply-3))]"
        >
          <span
            aria-hidden="true"
            className="silkscreen pointer-events-none absolute -top-[7px] left-2.5 inline-flex items-center gap-1.5 bg-(--legend-ground) px-1 text-ink-2"
          >
            <Icon name="sparkle" size="xs" />
            Your board
          </span>
          <EmptyState
            size="sm"
            title="Your board is empty"
            description="Add what you're playing, your mood, photos or a countdown. Everyone who opens your profile sees it."
            action={
              <Button variant="primary" size="sm" onClick={onEdit}>
                <Icon name="edit" size="sm" />
                Edit board
              </Button>
            }
            className="py-7"
          />
        </section>
        {EMPTY_SLOTS.map((type, index) => (
          <button
            key={type}
            type="button"
            tabIndex={-1}
            aria-hidden="true"
            data-slot="board-empty-slot"
            onClick={onEdit}
            className={cn(
              "group/slot flex h-[calc(var(--rack-unit,168px)*0.6)] min-w-0 flex-col items-center justify-center gap-1.5 rounded-[2px] border border-dashed border-line-strong px-3 text-ink-3 transition-colors duration-(--dur-1)",
              "pointer-fine:hover:border-ink-3 pointer-fine:hover:bg-tint-1 pointer-fine:hover:text-ink-2",
              index >= 2 && "@max-[540px]/rack:hidden",
            )}
          >
            <Icon name={WIDGET_ICON[type]} size="md" />
            <span className="silkscreen max-w-full truncate">
              {WIDGET_SPECS[type].name}
            </span>
          </button>
        ))}
      </div>
    </div>
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
