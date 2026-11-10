// @vitest-environment jsdom

import { act, fireEvent, render, screen } from "@testing-library/react";
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

import BoardEditor from "../BoardEditor";
import {
  boardOf,
  cellOf,
  LOUD,
  NOW,
  OWNER,
  Providers,
  rackOrder,
} from "./editor-helpers";

function renderEditor() {
  return render(
    <BoardEditor
      initial={boardOf()}
      onExit={() => {}}
      owner={OWNER}
      entitlement={LOUD}
      now={NOW}
    />,
    { wrapper: Providers },
  );
}

const announcer = () =>
  document.querySelector("[data-slot=editor-announcer]")!.textContent;

beforeEach(() => {
  localStorage.clear();
});

describe("edit in place: keyboard", () => {
  it("picks up, moves with the arrows and drops, announcing each step", () => {
    const { container } = renderEditor();
    const mood = cellOf(container, "w-mood");
    act(() => mood.focus());

    fireEvent.keyDown(mood, { key: " " });
    expect(announcer()).toContain("Mood picked up, position 1 of 3");
    expect(mood).toHaveAttribute("data-grabbed");

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "ArrowRight" });
    expect(rackOrder(container)).toEqual(["w-links", "w-mood", "w-count"]);
    expect(announcer()).toContain("Mood moved to position 2 of 3");

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "ArrowDown" });
    expect(rackOrder(container)).toEqual(["w-links", "w-count", "w-mood"]);
    expect(announcer()).toContain("moved to position 3 of 3");

    // Already last: nothing moves.
    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "ArrowRight" });
    expect(rackOrder(container)).toEqual(["w-links", "w-count", "w-mood"]);

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "ArrowLeft" });
    expect(rackOrder(container)).toEqual(["w-links", "w-mood", "w-count"]);

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: " " });
    expect(announcer()).toContain("Mood dropped, position 2 of 3");
    expect(cellOf(container, "w-mood")).not.toHaveAttribute("data-grabbed");
  });

  it("resizes with [ and ] within the sizes the type allows", () => {
    const { container } = renderEditor();
    const mood = cellOf(container, "w-mood");
    const size = () =>
      container
        .querySelector('li[data-widget-id="w-mood"] [data-slot=widget-plate]')!
        .getAttribute("data-size");

    fireEvent.keyDown(mood, { key: "]" });
    expect(size()).toBe("M");
    expect(announcer()).toContain("Mood is now Medium");

    // Mood is S or M: no Large.
    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "]" });
    expect(size()).toBe("M");
    expect(announcer()).toContain("the largest it goes");

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "[" });
    expect(size()).toBe("S");
    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "[" });
    expect(size()).toBe("S");
    expect(announcer()).toContain("the smallest it goes");
  });

  it("removes with Delete and brings it back with Ctrl Z", () => {
    const { container } = renderEditor();
    fireEvent.keyDown(cellOf(container, "w-links"), { key: "Delete" });
    expect(rackOrder(container)).toEqual(["w-mood", "w-count"]);
    expect(announcer()).toContain("Links removed");

    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "z", ctrlKey: true });
    expect(rackOrder(container)).toEqual(["w-mood", "w-links", "w-count"]);

    fireEvent.keyDown(cellOf(container, "w-mood"), {
      key: "z",
      ctrlKey: true,
      shiftKey: true,
    });
    expect(rackOrder(container)).toEqual(["w-mood", "w-count"]);
    expect(screen.getByRole("button", { name: "Undo" })).toBeEnabled();
  });
});
