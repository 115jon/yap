// @vitest-environment jsdom

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
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

import * as api from "@/lib/profile-board-api";
import type { BoardResponse, ProfileBoard } from "@/lib/profile-board";

import BoardEditor from "../BoardEditor";
import { ProfilePanel } from "../ProfilePanel";
import { draftStorageKey } from "../useBoardEditor";
import {
  boardOf,
  cellOf,
  LOUD,
  NOW,
  OWNER,
  Providers,
  rackOrder,
  WIDGETS,
} from "./editor-helpers";

const save = vi.mocked(api.saveProfileBoard);

function renderEditor(onExit = vi.fn(), initial = boardOf()) {
  const view = render(
    <BoardEditor
      initial={initial}
      onExit={onExit}
      owner={OWNER}
      entitlement={LOUD}
      now={NOW}
    />,
    { wrapper: Providers },
  );
  return { ...view, onExit };
}

function conflictError(board: ProfileBoard, version: number) {
  return Object.assign(new Error("Your board changed on another device."), {
    status: 409,
    body: { error: "x", code: "BOARD_VERSION_CONFLICT", board, version },
  });
}

beforeEach(() => {
  localStorage.clear();
  save.mockReset();
});

describe("edit in place: saving", () => {
  it("Done saves the draft and hands back the saved board", async () => {
    save.mockImplementation(async (board, version) => ({
      board,
      version: version + 1,
      drafts: [],
      dropped: [],
      updated_at: "2026-11-27T20:31:00Z",
    }));
    const { container, onExit } = renderEditor();
    fireEvent.keyDown(cellOf(container, "w-links"), { key: "Delete" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));

    await waitFor(() => expect(onExit).toHaveBeenCalledTimes(1));
    expect(save).toHaveBeenCalledWith(
      expect.objectContaining({
        widgets: [
          expect.objectContaining({ id: "w-mood" }),
          expect.objectContaining({ id: "w-count" }),
        ],
      }),
      3,
    );
    const next = onExit.mock.calls[0]![0] as BoardResponse;
    expect(next.version).toBe(4);
    expect(next.board.widgets.map((w) => w.id)).toEqual(["w-mood", "w-count"]);
    expect(localStorage.getItem(draftStorageKey(OWNER.id))).toBeNull();
  });

  it("Done with nothing changed exits without saving", () => {
    const { onExit } = renderEditor();
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(save).not.toHaveBeenCalled();
    expect(onExit).toHaveBeenCalledWith(
      expect.objectContaining({ version: 3 }),
    );
  });

  it("shows the conflict on a 409, keeps the draft, and Use theirs loads the server copy", async () => {
    const theirs: ProfileBoard = { v: 1, widgets: [WIDGETS[2]!] };
    save.mockRejectedValueOnce(conflictError(theirs, 9));
    const { container, onExit } = renderEditor();
    fireEvent.keyDown(cellOf(container, "w-mood"), { key: "]" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));

    const notice = await screen.findByText(
      "Your board changed on another device.",
    );
    expect(notice).toBeInTheDocument();
    expect(onExit).not.toHaveBeenCalled();
    expect(rackOrder(container)).toEqual(["w-mood", "w-links", "w-count"]);

    fireEvent.click(screen.getByRole("button", { name: "Use theirs" }));
    expect(rackOrder(container)).toEqual(["w-count"]);
    expect(
      screen.queryByText("Your board changed on another device."),
    ).toBeNull();

    // The next save goes from their version.
    save.mockImplementation(async (board, version) => ({
      board,
      version: version + 1,
      drafts: [],
      dropped: [],
      updated_at: "2026-11-27T20:32:00Z",
    }));
    fireEvent.keyDown(cellOf(container, "w-count"), { key: "]" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(save).toHaveBeenLastCalledWith(expect.anything(), 9);
  });

  it("Keep mine saves the draft over the newer version", async () => {
    const theirs: ProfileBoard = { v: 1, widgets: [WIDGETS[2]!] };
    save.mockRejectedValueOnce(conflictError(theirs, 9));
    save.mockImplementation(async (board, version) => ({
      board,
      version: version + 1,
      drafts: [],
      dropped: [],
      updated_at: "2026-11-27T20:32:00Z",
    }));
    const { container, onExit } = renderEditor();
    fireEvent.keyDown(cellOf(container, "w-links"), { key: "Delete" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    fireEvent.click(await screen.findByRole("button", { name: "Keep mine" }));
    await waitFor(() => expect(onExit).toHaveBeenCalled());
    expect(save).toHaveBeenLastCalledWith(
      expect.objectContaining({
        widgets: [
          expect.objectContaining({ id: "w-mood" }),
          expect.objectContaining({ id: "w-count" }),
        ],
      }),
      9,
    );
  });

  it("says how long to wait after a 429, with a retry", async () => {
    save.mockRejectedValueOnce(
      Object.assign(new Error("slow down"), {
        status: 429,
        retryAfterMs: 12_000,
      }),
    );
    const { container } = renderEditor();
    fireEvent.keyDown(cellOf(container, "w-links"), { key: "Delete" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    expect(
      await screen.findByText(/Try again in 12 seconds/),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
  });

  it("autosaves the draft locally and restores it on reopen", async () => {
    vi.useFakeTimers();
    try {
      const first = renderEditor();
      fireEvent.keyDown(cellOf(first.container, "w-links"), { key: "Delete" });
      act(() => {
        vi.advanceTimersByTime(1000);
      });
      expect(localStorage.getItem(draftStorageKey(OWNER.id))).not.toBeNull();
      first.unmount();
    } finally {
      vi.useRealTimers();
    }

    const second = renderEditor();
    expect(rackOrder(second.container)).toEqual(["w-mood", "w-count"]);
    expect(
      screen.getByText("Restored your unsaved changes."),
    ).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Drop them" }));
    expect(rackOrder(second.container)).toEqual([
      "w-mood",
      "w-links",
      "w-count",
    ]);
    expect(localStorage.getItem(draftStorageKey(OWNER.id))).toBeNull();
  });
});

describe("the profile panel after a save", () => {
  it("fetches the board again, so a plate just added gets its data", async () => {
    const fetchBoard = vi.mocked(api.fetchProfileBoard);
    fetchBoard.mockReset();
    fetchBoard.mockResolvedValue(boardOf());
    save.mockImplementation(async (board, version) => ({
      board,
      version: version + 1,
      drafts: [],
      dropped: [],
      updated_at: "2026-11-27T20:31:00Z",
    }));
    const { container } = render(
      <ProfilePanel userId={OWNER.id} isOwner displayName="Jon" now={NOW} />,
      { wrapper: Providers },
    );
    fireEvent.click(await screen.findByRole("button", { name: "Edit board" }));
    await screen.findByRole("button", { name: "Done" });
    expect(fetchBoard).toHaveBeenCalledTimes(1);
    fireEvent.keyDown(cellOf(container, "w-links"), { key: "Delete" });
    fireEvent.click(screen.getByRole("button", { name: "Done" }));
    // The save answers with the board but no widget data; the gateway's
    // update for our own save is skipped (same version), so the panel
    // refetches by itself.
    await waitFor(() => expect(fetchBoard).toHaveBeenCalledTimes(2));
  });
});
