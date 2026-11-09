// @vitest-environment jsdom

import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/profile-board-api", async (importActual) => {
  const actual = await importActual<typeof import("@/lib/profile-board-api")>();
  return {
    ...actual,
    saveProfileBoard: vi.fn(),
    uploadWidgetAsset: vi.fn(),
    fetchProfileBoard: vi.fn(),
    postGuestbookNote: vi.fn(),
  };
});

import type { BoardResponse } from "@/lib/profile-board";
import type { Entitlement } from "@/lib/entitlements";

import BoardEditor from "../BoardEditor";
import {
  boardOf,
  FREE,
  fullBoard,
  LOUD,
  NOW,
  OWNER,
  Providers,
  rackOrder,
} from "./editor-helpers";

function openPicker(initial: BoardResponse, entitlement: Entitlement) {
  const view = render(
    <BoardEditor
      initial={initial}
      onExit={() => {}}
      owner={OWNER}
      entitlement={entitlement}
      now={NOW}
      initialPickerOpen
      pickerForceMode="popover"
    />,
    { wrapper: Providers },
  );
  const listbox = screen.getByRole("listbox", { name: "Widgets" });
  return { ...view, listbox };
}

const option = (listbox: HTMLElement, type: string) =>
  listbox.querySelector<HTMLElement>(`[role=option][data-type="${type}"]`)!;

beforeEach(() => localStorage.clear());

describe("edit in place: the widget picker", () => {
  it("puts Custom first with the Loud lock on a free plan, and adds it as a draft", () => {
    const { listbox, container } = openPicker(boardOf(), FREE);
    const options = within(listbox).getAllByRole("option");
    expect(options[0]).toHaveAttribute("data-type", "custom");
    expect(
      options[0]!.querySelector("[data-slot=loud-lock]"),
    ).toHaveTextContent("Loud");

    fireEvent.click(options[0]!);
    const ids = rackOrder(container);
    expect(ids).toHaveLength(4);
    const added = container.querySelector(
      `li[data-widget-id="${ids[3]}"] [data-slot=widget-plate]`,
    )!;
    expect(added).toHaveAttribute("data-type", "custom");
    expect(added.querySelector("[data-slot=widget-draft]")).toHaveTextContent(
      "Draft · only you",
    );
    expect(screen.queryByRole("listbox", { name: "Widgets" })).toBeNull();
  });

  it("shows no lock and no draft on a Loud plan", () => {
    const { listbox, container } = openPicker(boardOf(), LOUD);
    expect(
      option(listbox, "custom").querySelector("[data-slot=loud-lock]"),
    ).toBeNull();
    fireEvent.click(option(listbox, "custom"));
    expect(container.querySelector("[data-slot=widget-draft]")).toBeNull();
  });

  it("disables a type at its cap with the reason, and adds with the keyboard", () => {
    const { listbox, container } = openPicker(boardOf(), LOUD);
    const mood = option(listbox, "mood");
    expect(mood).toHaveAttribute("aria-disabled", "true");
    expect(mood).toHaveTextContent("Already on your board");
    fireEvent.click(mood);
    expect(rackOrder(container)).toHaveLength(3);

    // Links may be on a board twice.
    expect(option(listbox, "links")).not.toHaveAttribute("aria-disabled");

    const search = screen.getByRole("combobox");
    fireEvent.change(search, { target: { value: "top games" } });
    fireEvent.keyDown(search, { key: "Enter" });
    const ids = rackOrder(container);
    expect(ids).toHaveLength(4);
    expect(
      container.querySelector(
        `li[data-widget-id="${ids[3]}"] [data-slot=widget-plate]`,
      ),
    ).toHaveAttribute("data-type", "top_games");
  });

  it("moves with the arrow keys and closes on Esc", () => {
    const { listbox } = openPicker(boardOf(), LOUD);
    const search = screen.getByRole("combobox");
    const first = within(listbox).getAllByRole("option")[0]!;
    expect(first).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(search, { key: "ArrowDown" });
    const second = within(listbox).getAllByRole("option")[1]!;
    expect(second).toHaveAttribute("aria-selected", "true");
    expect(search).toHaveAttribute("aria-activedescendant", second.id);
    fireEvent.keyDown(search, { key: "Escape" });
    expect(screen.queryByRole("listbox", { name: "Widgets" })).toBeNull();
  });

  it("says the board is full at 12 and adds nothing", () => {
    const { listbox, container } = openPicker(fullBoard(12), LOUD);
    expect(screen.getByText("12 / 12")).toBeInTheDocument();
    expect(
      screen.getByText("Your board is full. Remove a widget to add another."),
    ).toBeInTheDocument();
    for (const item of within(listbox).getAllByRole("option")) {
      expect(item).toHaveAttribute("aria-disabled", "true");
    }
    fireEvent.click(option(listbox, "custom"));
    expect(rackOrder(container)).toHaveLength(12);
  });
});
