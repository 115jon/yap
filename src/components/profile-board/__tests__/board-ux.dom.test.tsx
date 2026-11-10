// @vitest-environment jsdom

/**
 * The board as people use it: the wheel and touch scroll through it, the
 * owner's empty board sits in the board column as a Rack-shaped call to
 * action, and the guestbook says exactly why someone can't sign.
 */

import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  AnyProfileWidget,
  BoardData,
  BoardResponse,
  GuestbookNote,
} from "@/lib/profile-board";

vi.mock("@/lib/profile-board-api", () => ({
  postGuestbookNote: vi.fn(),
  deleteGuestbookNote: vi.fn(),
  blockFromGuestbook: vi.fn(),
  fetchProfileBoard: vi.fn(() => new Promise(() => {})),
}));

import { TooltipProvider } from "@/components/ui/tooltip";

import { FIXTURE_BOARD, FIXTURE_NOW } from "../fixtures";
import { BoardRack, type BoardRackProps } from "../ProfileBoard";
import { ProfilePanel } from "../ProfilePanel";
import { guestbookDenialCopy } from "../board-format";

class ResizeObserverMock {
  observe() {}
  disconnect() {}
  unobserve() {}
}

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

const OWNER = { id: "u-kiwi", name: "Kiwi" };
const VIEWER = {
  id: "u-me",
  username: "jon",
  display_name: "Jon",
  avatar_url: null,
};

function rack(
  props: Partial<BoardRackProps> & Pick<BoardRackProps, "widgets">,
) {
  return render(
    <TooltipProvider>
      <BoardRack
        data={{}}
        isOwner={false}
        owner={OWNER}
        viewer={VIEWER}
        now={FIXTURE_NOW}
        {...props}
      />
    </TooltipProvider>,
  );
}

describe("scrolling over the board", () => {
  it("leaves the wheel and touch to the page over every widget", () => {
    const { container } = rack({
      widgets: FIXTURE_BOARD.board.widgets,
      data: FIXTURE_BOARD.data,
    });
    const plates = container.querySelectorAll('[data-slot="widget-plate"]');
    expect(plates.length).toBe(FIXTURE_BOARD.board.widgets.length);
    for (const plate of plates) {
      const wheel = new WheelEvent("wheel", {
        deltaY: 120,
        bubbles: true,
        cancelable: true,
      });
      plate.dispatchEvent(wheel);
      expect(wheel.defaultPrevented).toBe(false);
    }
    // No inner scroller, scroll trap or touch block in view mode: a wheel
    // or swipe over a plate always reaches the modal's scroller.
    const traps = container.querySelectorAll(
      '[class*="overflow-y-auto"], [class*="overflow-auto"], [class*="overscroll-"], [class*="touch-none"]',
    );
    expect([...traps].map((el) => el.className)).toEqual([]);
  });
});

describe("the owner's empty board", () => {
  const empty: BoardResponse = {
    user_id: "u-me",
    board: { v: 1, widgets: [] },
    version: 0,
    drafts: [],
    is_owner: true,
    data: {},
    updated_at: null,
  };

  it("is a Rack-shaped call to action in the board column", () => {
    render(
      <ProfilePanel
        userId="u-me"
        isOwner
        displayName="Jon"
        fixtures={{ board: empty }}
        now={FIXTURE_NOW}
      />,
    );
    const board = document.querySelector<HTMLElement>(
      '[data-slot="board-empty"]',
    )!;
    expect(board).toBeInTheDocument();
    // Inside the tab panel, under the strip.
    expect(board.closest('[role="tabpanel"]')).not.toBeNull();
    expect(within(board).getByText("Your board is empty")).toBeInTheDocument();
    // One real action for keyboard and screen readers…
    expect(
      within(board).getAllByRole("button", { name: /Edit board/ }),
    ).toHaveLength(1);
    // …and dashed Rack slots as a mouse shortcut, out of the tab order.
    const slots = board.querySelectorAll('[data-slot="board-empty-slot"]');
    expect(slots).toHaveLength(4);
    for (const slot of slots) {
      expect(slot).toHaveAttribute("tabindex", "-1");
      expect(slot).toHaveAttribute("aria-hidden", "true");
    }
  });

  it("is one quiet line for everyone else", () => {
    render(
      <ProfilePanel
        userId="u-kiwi"
        isOwner={false}
        displayName="Kiwi"
        fixtures={{ board: { ...empty, user_id: "u-kiwi", is_owner: false } }}
      />,
    );
    expect(
      screen.getByText("Kiwi hasn't added anything to their board yet."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Edit board/ })).toBeNull();
  });
});

describe("guestbook states", () => {
  const guestbook: AnyProfileWidget = {
    id: "w-gb",
    type: "guestbook",
    size: "M",
    cfg: {},
  };
  const note: GuestbookNote = {
    id: "note-mo",
    author: {
      id: "u-mo",
      username: "mo",
      display_name: "Mo",
      avatar_url: null,
    },
    body: "carried me through plat",
    created_at: "2026-11-25T20:30:00Z",
  };
  const book = (
    over: Partial<NonNullable<BoardData["guestbook"]>> = {},
  ): BoardData => ({
    guestbook: {
      enabled: true,
      notes: [note],
      total: 1,
      can_post: false,
      denial: null,
      ...over,
    },
  });

  it("is never 'unavailable' to its owner before the data lands", () => {
    // The plate was just added: the saved board has no guestbook data yet.
    rack({ widgets: [guestbook], data: {}, isOwner: true });
    expect(screen.queryByText(/unavailable/i)).toBeNull();
    expect(screen.getByText("No notes yet")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Friends who share a server or a DM with you can sign it.",
      ),
    ).toBeInTheDocument();
  });

  it("shows its owner the notes and the moderation menu", () => {
    rack({
      widgets: [guestbook],
      data: book({ denial: "GUESTBOOK_OWN" }),
      isOwner: true,
    });
    expect(screen.getByText("carried me through plat")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Note from Mo: more" }),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Leave Kiwi a note")).toBeNull();
    expect(document.querySelector('[data-slot="guestbook-note"]')).toBeNull();
  });

  it.each([
    [
      "GUESTBOOK_NO_SHARED_SPACE",
      "Only people who share a server or a DM with Kiwi can sign. Join one of their servers or message them first.",
    ],
    ["GUESTBOOK_BLOCKED", "You can't sign Kiwi's guestbook."],
    [null, guestbookDenialCopy(undefined, "Kiwi")],
  ] as const)("tells a visitor why they can't sign (%s)", (denial, copy) => {
    rack({ widgets: [guestbook], data: book({ denial }) });
    expect(screen.queryByLabelText("Leave Kiwi a note")).toBeNull();
    expect(
      document.querySelector('[data-slot="guestbook-note"]'),
    ).toHaveTextContent(copy);
    // The notes still show.
    expect(screen.getByText("carried me through plat")).toBeInTheDocument();
  });

  it("says when the owner closed it", () => {
    rack({
      widgets: [guestbook],
      data: book({
        enabled: false,
        notes: [],
        total: 0,
        denial: "GUESTBOOK_OFF",
      }),
    });
    expect(
      screen.getByText("Kiwi closed their guestbook."),
    ).toBeInTheDocument();
  });

  it("gives a visitor the field when they may sign", () => {
    rack({ widgets: [guestbook], data: book({ can_post: true }) });
    expect(screen.getByLabelText("Leave Kiwi a note")).toBeInTheDocument();
    expect(document.querySelector('[data-slot="guestbook-note"]')).toBeNull();
  });
});

describe("listening, empty", () => {
  const listening: AnyProfileWidget = {
    id: "w-listen",
    type: "listening",
    size: "M",
    cfg: {},
  };

  it("tells the owner how a session gets here", () => {
    rack({ widgets: [listening], data: { last_listen: null }, isOwner: true });
    expect(
      screen.getByText("Your last Listen Together shows here"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/open the Soundboard and pick Listen Together/),
    ).toBeInTheDocument();
  });

  it("tells a visitor what would show", () => {
    rack({ widgets: [listening], data: { last_listen: null } });
    expect(screen.getByText("No Listen Together to show")).toBeInTheDocument();
    expect(
      screen.getByText(/Kiwi's last session of 3 tracks or more/),
    ).toBeInTheDocument();
  });
});
