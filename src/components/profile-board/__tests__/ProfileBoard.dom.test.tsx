// @vitest-environment jsdom

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  packRack,
  type AnyProfileWidget,
  type BoardData,
  type GuestbookNote,
} from "@/lib/profile-board";

vi.mock("@/lib/profile-board-api", () => ({
  postGuestbookNote: vi.fn(),
  deleteGuestbookNote: vi.fn(),
  blockFromGuestbook: vi.fn(),
  fetchProfileBoard: vi.fn(),
}));

import * as api from "@/lib/profile-board-api";

import { BoardRack, type BoardRackProps } from "../ProfileBoard";

const NOW = Date.parse("2026-11-27T20:30:00Z");
const OWNER = { id: "u-kiwi", name: "Kiwi" };
const VIEWER = {
  id: "u-me",
  username: "jon",
  display_name: "Jon",
  avatar_url: null,
};

const links = (id: string, size: "S" | "M" = "S"): AnyProfileWidget => ({
  id,
  type: "links",
  size,
  cfg: { links: [{ label: id, url: "https://example.com/" + id }] },
});

function rack(
  props: Partial<BoardRackProps> & Pick<BoardRackProps, "widgets">,
) {
  return render(
    <BoardRack
      data={{}}
      isOwner={false}
      owner={OWNER}
      viewer={VIEWER}
      now={NOW}
      {...props}
    />,
  );
}

function cells(container: HTMLElement) {
  return [...container.querySelectorAll<HTMLLIElement>("li[data-widget-id]")];
}

describe("BoardRack placement", () => {
  it("places every widget on the cell packRack gives it", () => {
    const widgets: AnyProfileWidget[] = [
      {
        id: "w-countdown",
        type: "countdown",
        size: "S",
        cfg: { title: "", at: null },
      },
      {
        id: "w-photos",
        type: "photos",
        size: "M",
        cfg: { photos: [], caption: "" },
      },
      {
        id: "w-quote",
        type: "quote",
        size: "L",
        cfg: { message_id: null, snapshot: null },
      },
      links("w-links1"),
      links("w-links2"),
    ];
    const { container } = rack({ widgets });
    const expected = packRack(widgets.map((widget) => widget.size)).cells;
    const items = cells(container);
    expect(items.map((item) => item.dataset.widgetId)).toEqual(
      widgets.map((w) => w.id),
    );
    items.forEach((item, index) => {
      const cell = expected[index]!;
      expect(item.style.gridColumn).toBe(`${cell.col + 1} / span ${cell.cols}`);
      expect(item.style.gridRow).toBe(`${cell.row + 1} / span ${cell.rows}`);
    });
    // S and M share row 0; the first link fills the hole under the S beside
    // the M; L waits for a free full row; the last link starts a new row.
    expect(expected.map(({ col, row }) => [col, row])).toEqual([
      [0, 0],
      [1, 0],
      [0, 2],
      [0, 1],
      [0, 3],
    ]);
  });
});

describe("drafts", () => {
  const custom: AnyProfileWidget = {
    id: "w-custom",
    type: "custom",
    size: "L",
    cfg: {
      legend: "Commissions",
      title: "Commissions open",
      asset: null,
      lines: [],
    },
  };
  const widgets = [links("w-links"), custom];

  it("tags a draft for its owner", () => {
    rack({ widgets, drafts: ["w-custom"], isOwner: true });
    const plate = screen.getByRole("region", { name: /Commissions/ });
    expect(within(plate).getByText("Draft · only you")).toBeInTheDocument();
    expect(screen.getAllByText("Draft · only you")).toHaveLength(1);
  });

  it("leaves drafts out when the owner previews as anyone", () => {
    const { container } = rack({
      widgets,
      drafts: ["w-custom"],
      isOwner: true,
      preview: "anyone",
    });
    expect(screen.queryByText("Draft · only you")).toBeNull();
    expect(cells(container).map((item) => item.dataset.widgetId)).toEqual([
      "w-links",
    ]);
  });

  it("never tags anything for another viewer", () => {
    rack({ widgets: [custom], drafts: [], isOwner: false });
    expect(screen.getByText("Commissions open")).toBeInTheDocument();
    expect(screen.queryByText("Draft · only you")).toBeNull();
  });
});

describe("pinned message", () => {
  const quote: AnyProfileWidget = {
    id: "w-quote",
    type: "quote",
    size: "L",
    cfg: {
      message_id: "m-1",
      snapshot: {
        text: "secret plans for raid night",
        at: "2026-11-01T12:00:00Z",
        channel_id: "c-1",
        channel_name: "plans",
        server_id: "s-1",
        server_name: "Secret Base",
      },
    },
  };

  it("hides the text from a viewer outside the server", () => {
    rack({ widgets: [quote], data: { quote_visible: false } });
    expect(
      screen.getByText("Message from a server you're not in"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/secret plans/)).toBeNull();
  });

  it("shows it to members and to the owner", () => {
    const { unmount } = rack({
      widgets: [quote],
      data: { quote_visible: true },
    });
    expect(screen.getByText("secret plans for raid night")).toBeInTheDocument();
    unmount();
    rack({ widgets: [quote], data: { quote_visible: false }, isOwner: true });
    expect(screen.getByText("secret plans for raid night")).toBeInTheDocument();
  });
});

describe("guestbook", () => {
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
  const data = (
    over: Partial<NonNullable<BoardData["guestbook"]>> = {},
  ): BoardData => ({
    guestbook: {
      enabled: true,
      notes: [note],
      total: 1,
      can_post: true,
      ...over,
    },
  });

  beforeEach(() => {
    vi.mocked(api.postGuestbookNote).mockReset();
    vi.mocked(api.deleteGuestbookNote).mockReset();
    vi.mocked(api.blockFromGuestbook).mockReset();
  });

  it("posts a note optimistically and keeps the server's copy", async () => {
    let resolve!: (value: GuestbookNote) => void;
    vi.mocked(api.postGuestbookNote).mockReturnValue(
      new Promise<GuestbookNote>((done) => {
        resolve = done;
      }),
    );
    rack({ widgets: [guestbook], data: data() });
    const field = screen.getByLabelText("Leave Kiwi a note");
    fireEvent.change(field, { target: { value: "  gg last night  " } });
    fireEvent.click(screen.getByRole("button", { name: "Post note" }));

    const pending = await screen.findByText("gg last night");
    expect(pending.closest("li")).toHaveAttribute("data-pending");
    expect(api.postGuestbookNote).toHaveBeenCalledWith(
      "u-kiwi",
      "gg last night",
    );
    expect(field).toHaveValue("");

    resolve({ ...note, id: "note-new", author: VIEWER, body: "gg last night" });
    await waitFor(() =>
      expect(
        screen.getByText("gg last night").closest("li"),
      ).not.toHaveAttribute("data-pending"),
    );
    expect(screen.getByText("2 notes")).toBeInTheDocument();
  });

  it("takes a refused note back out and shows the server's reason", async () => {
    vi.mocked(api.postGuestbookNote).mockRejectedValue(
      Object.assign(new Error("That's 5 notes today. Try again tomorrow."), {
        status: 429,
      }),
    );
    rack({ widgets: [guestbook], data: data() });
    const field = screen.getByLabelText("Leave Kiwi a note");
    fireEvent.change(field, { target: { value: "one more" } });
    fireEvent.submit(field.closest("form")!);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "That's 5 notes today. Try again tomorrow.",
    );
    expect(screen.queryByText("one more")).toBeNull();
    expect(field).toHaveValue("one more");
  });

  it("lets the owner delete a note", async () => {
    vi.mocked(api.deleteGuestbookNote).mockResolvedValue({
      id: "note-mo",
      deleted: true,
    });
    rack({
      widgets: [guestbook],
      data: data({ can_post: false }),
      isOwner: true,
    });
    expect(screen.queryByLabelText("Leave Kiwi a note")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Note from Mo: more" }));
    fireEvent.click(
      await screen.findByRole("menuitem", { name: /Delete note/ }),
    );
    await waitFor(() =>
      expect(screen.queryByText("carried me through plat")).toBeNull(),
    );
    expect(api.deleteGuestbookNote).toHaveBeenCalledWith("u-kiwi", "note-mo");
  });

  it("offers no actions on someone else's note to a visitor", () => {
    rack({ widgets: [guestbook], data: data() });
    expect(
      screen.queryByRole("button", { name: "Note from Mo: more" }),
    ).toBeNull();
  });

  it("says when the visitor can't sign or the guestbook is closed", () => {
    const { unmount } = rack({
      widgets: [guestbook],
      data: data({ can_post: false }),
    });
    expect(screen.queryByLabelText("Leave Kiwi a note")).toBeNull();
    expect(
      screen.getByText(
        "Only people who share a server or a DM with Kiwi can sign. Join one of their servers or message them first.",
      ),
    ).toBeInTheDocument();
    unmount();
    rack({
      widgets: [guestbook],
      data: data({ enabled: false, can_post: false }),
    });
    expect(
      screen.getByText("Kiwi closed their guestbook."),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("Leave Kiwi a note")).toBeNull();
  });
});
