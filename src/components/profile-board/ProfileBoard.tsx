/**
 * The Rack: a profile board's widgets on two columns of fixed row units
 * (spec §1, "A · Rack"). S is half width × 1 row, M half width × 2 rows, L
 * full width × 1 row. Cells are placed with explicit grid lines from
 * `packRack`, the same packing the native client uses, so web and native
 * never disagree about where a widget sits and the board never has holes
 * the order did not make.
 *
 * The rack is a size container: below 540px of width (the phone sheet)
 * the row unit shrinks to 0.9 and the gutters tighten, but it keeps two
 * columns; each plate's body is its own container, so widgets pick their
 * narrow layout from the cell they are in.
 *
 *   ProfileBoard   a BoardResponse, as the profile modal shows it
 *   BoardRack      the grid, with `renderCell` for the editor's chrome
 *   WidgetPlate    the legend plate (re-exported)
 *   WidgetBody     one widget's plate and content, by type (re-exported)
 */

import { useMemo, useState, type CSSProperties, type ReactNode } from "react";

import {
  packRack,
  RACK_ROW_UNIT,
  type AnyProfileWidget,
  type BoardData,
  type BoardResponse,
  type BoardUserRef,
  type QuoteSnapshot,
  type RackCell,
} from "@/lib/profile-board";
import { cn } from "@/lib/utils";
import { useChatStore } from "@/stores/chat-store";

import type { BoardCtx, BoardPreview } from "./board-format";
import { WidgetBody } from "./widgets";

export { WidgetPlate, type WidgetPlateProps } from "./WidgetPlate";
export { WidgetBody } from "./widgets";
export { BoardMediaProvider, type BoardMedia } from "./board-context";
export {
  visibleWidgetCount,
  WIDGET_ICON,
  type BoardCtx,
  type BoardPreview,
} from "./board-format";

type Ground = "ply-0" | "ply-1" | "ply-2" | "ply-3";

export interface BoardOwner {
  id: string;
  /** Display name, for copy ("Leave Kiwi a note"). */
  name: string;
  avatarUrl?: string | null;
}

export interface BoardRackProps {
  widgets: readonly AnyProfileWidget[];
  /** Widget ids only the owner sees (BoardResponse.drafts). */
  drafts?: readonly string[];
  data: BoardData;
  /** The viewer owns the board. */
  isOwner: boolean;
  /**
   * "anyone": the owner previews what everyone else gets (drafts gone, no
   * owner-only controls). Default: the viewer's own view.
   */
  preview?: BoardPreview;
  owner: BoardOwner;
  /** The signed-in viewer; defaults to the chat store's user. */
  viewer?: BoardUserRef | null;
  /** Frozen clock for fixtures and tests. */
  now?: number;
  onOpenMessage?: (messageId: string, snapshot: QuoteSnapshot) => void;
  /** The ground the rack sits on, for the legends' backing. Default ply-3. */
  ground?: Ground;
  /**
   * Wrap a cell's content (the editor's selection and drag chrome). The
   * returned node is placed inside the cell's grid item, which keeps the
   * cell's position and size.
   */
  renderCell?: (
    widget: AnyProfileWidget,
    cell: RackCell,
    content: ReactNode,
  ) => ReactNode;
  className?: string;
}

export function BoardRack({
  widgets,
  drafts = [],
  data,
  isOwner,
  preview,
  owner,
  viewer,
  now,
  onOpenMessage,
  ground = "ply-3",
  renderCell,
  className,
}: BoardRackProps) {
  const storeUser = useChatStore((state) => state.user);
  // Read once per mount: "2h ago" readouts don't tick.
  const [mountedAt] = useState(() => Date.now());
  const asOwner = isOwner && preview !== "anyone";
  const draftSet = useMemo(() => new Set(drafts), [drafts]);
  const visible = useMemo(
    () =>
      asOwner ? widgets : widgets.filter((widget) => !draftSet.has(widget.id)),
    [asOwner, widgets, draftSet],
  );
  const { cells, rows } = useMemo(
    () => packRack(visible.map((widget) => widget.size)),
    [visible],
  );

  const viewerRef: BoardUserRef | null =
    viewer !== undefined
      ? viewer
      : storeUser
        ? {
            id: storeUser.id,
            username: storeUser.username,
            display_name: storeUser.display_name ?? null,
            avatar_url: storeUser.avatar_url ?? null,
          }
        : null;

  const ctx: BoardCtx = {
    ownerId: owner.id,
    ownerName: owner.name,
    ownerAvatarUrl: owner.avatarUrl ?? null,
    isOwner: asOwner,
    viewer: viewerRef,
    drafts: draftSet,
    now,
    at: now ?? mountedAt,
    onOpenMessage,
  };

  return (
    <div
      data-slot="board-rack"
      className={cn("@container/rack pt-2", className)}
      style={
        {
          "--plate-ground": `var(--${ground})`,
          "--rack-unit": `${RACK_ROW_UNIT}px`,
        } as CSSProperties
      }
    >
      <ul
        aria-label="Board"
        className="grid grid-cols-2 gap-x-3.5 gap-y-[22px] [--rack-row:var(--rack-unit)] @max-[540px]/rack:gap-x-2.5 @max-[540px]/rack:gap-y-5 @max-[540px]/rack:[--rack-row:calc(var(--rack-unit)*0.9)]"
        style={{ gridTemplateRows: `repeat(${rows}, var(--rack-row))` }}
      >
        {visible.map((widget, index) => {
          const cell = cells[index]!;
          const content = <WidgetBody widget={widget} data={data} ctx={ctx} />;
          return (
            <li
              key={widget.id}
              data-widget-id={widget.id}
              data-col={cell.col}
              data-row={cell.row}
              className="min-w-0"
              style={{
                gridColumn: `${cell.col + 1} / span ${cell.cols}`,
                gridRow: `${cell.row + 1} / span ${cell.rows}`,
              }}
            >
              {renderCell ? renderCell(widget, cell, content) : content}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

export interface ProfileBoardProps {
  response: BoardResponse;
  /** The owner previewing as someone else ("anyone"). */
  previewAs?: BoardPreview;
  /** Who owns the board; the id defaults to `response.user_id`. */
  owner?: Partial<BoardOwner> & { name: string };
  viewer?: BoardUserRef | null;
  now?: number;
  onOpenMessage?: BoardRackProps["onOpenMessage"];
  ground?: Ground;
  className?: string;
}

/** A fetched board as the profile modal shows it. Nothing when it is empty. */
export function ProfileBoard({
  response,
  previewAs,
  owner,
  viewer,
  now,
  onOpenMessage,
  ground,
  className,
}: ProfileBoardProps) {
  if (response.board.widgets.length === 0) return null;
  return (
    <BoardRack
      widgets={response.board.widgets}
      drafts={response.drafts}
      data={response.data}
      isOwner={response.is_owner}
      preview={previewAs}
      owner={{
        id: owner?.id ?? response.user_id,
        name: owner?.name ?? "them",
        avatarUrl: owner?.avatarUrl ?? null,
      }}
      viewer={viewer}
      now={now}
      onOpenMessage={onOpenMessage}
      ground={ground}
      className={className}
    />
  );
}
