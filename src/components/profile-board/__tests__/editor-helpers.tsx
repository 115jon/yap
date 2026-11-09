/** Shared boards and plans for the editor DOM tests. */

import type { ReactNode } from "react";

import { TooltipProvider } from "@/components/ui/tooltip";

import type { Entitlement } from "@/lib/entitlements";
import type {
  AnyProfileWidget,
  BoardResponse,
  WidgetType,
} from "@/lib/profile-board";
import { createWidget } from "@/lib/profile-board";

export const OWNER = { id: "u-me", name: "Jon" };

export const LOUD: Entitlement = { tier: "loud", source: "grant" };
export const FREE: Entitlement = { tier: "free" };

export const NOW = Date.parse("2026-11-27T20:30:00Z");

export const WIDGETS: AnyProfileWidget[] = [
  {
    id: "w-mood",
    type: "mood",
    size: "S",
    cfg: { pose: "bark", text: "ranked grind", clears_at: null },
  },
  {
    id: "w-links",
    type: "links",
    size: "S",
    cfg: { links: [{ label: "Art", url: "https://kiwi.page/art" }] },
  },
  {
    id: "w-count",
    type: "countdown",
    size: "S",
    cfg: { title: "Raid night", at: "2026-11-29T21:00:00Z" },
  },
];

export function boardOf(
  widgets: AnyProfileWidget[] = WIDGETS,
  version = 3,
): BoardResponse {
  return {
    user_id: OWNER.id,
    board: { v: 1, widgets },
    version,
    drafts: [],
    is_owner: true,
    data: {},
    updated_at: null,
  };
}

/** A board of `count` widgets (types with room for two count twice). */
export function fullBoard(count: number): BoardResponse {
  const types: WidgetType[] = [
    "now_playing",
    "top_games",
    "voice_hours",
    "listening",
    "badges",
    "servers",
    "quote",
    "guestbook",
    "mood",
    "countdown",
    "photos",
    "photos",
    "links",
  ];
  return boardOf(
    types.slice(0, count).map((type) => createWidget(type) as AnyProfileWidget),
  );
}

/** The ids of the widgets on the rack, in DOM order. */
export function rackOrder(container: HTMLElement): string[] {
  return [...container.querySelectorAll("li[data-widget-id]")].map((li) =>
    li.getAttribute("data-widget-id")!,
  );
}

export function cellOf(container: HTMLElement, id: string): HTMLElement {
  return container.querySelector(`[data-edit-cell="${id}"]`)!;
}

export function Providers({ children }: { children: ReactNode }) {
  return <TooltipProvider>{children}</TooltipProvider>;
}
