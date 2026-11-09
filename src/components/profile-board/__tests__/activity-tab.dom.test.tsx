// @vitest-environment jsdom

import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { ActivityEvent, ActivityPage } from "@/lib/profile-board";

const api = vi.hoisted(() => ({
  fetchProfileActivity: vi.fn(),
  setActivityEventHidden: vi.fn(),
  updateProfilePrefs: vi.fn(),
}));
vi.mock("@/lib/profile-board-api", () => api);

import ProfileActivity from "../ProfileActivity";

const DAY = 86_400_000;
const now = Date.now();

function ev(id: string, partial: Partial<ActivityEvent> = {}): ActivityEvent {
  return {
    id,
    kind: "game",
    ref: null,
    data: { name: `Game ${id}` },
    started_at: new Date(now - 60_000).toISOString(),
    ended_at: null,
    seconds: 600,
    hidden: false,
    ...partial,
  };
}

function page(
  events: ActivityEvent[],
  extra: Partial<ActivityPage> = {},
): ActivityPage {
  return { events, next_cursor: null, is_owner: false, ...extra };
}

const ownPage = (events: ActivityEvent[], extra: Partial<ActivityPage> = {}) =>
  page(events, {
    is_owner: true,
    settings: { retention_days: 30, hidden_kinds: [] },
    ...extra,
  });

function renderTab(owner: boolean) {
  return render(
    <ProfileActivity userId="u1" isOwner={owner} displayName="Kiwi" />,
  );
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("ProfileActivity", () => {
  it("groups events by day with counts, read-only for others", async () => {
    api.fetchProfileActivity.mockResolvedValue(
      page([
        ev("a"),
        ev("b", { kind: "badge", ref: "night-owl", data: {} }),
        ev("c", { started_at: new Date(now - 10 * DAY).toISOString() }),
      ]),
    );
    renderTab(false);

    const today = await screen.findByRole("region", { name: /^Today/ });
    expect(within(today).getByText("Game a")).toBeInTheDocument();
    expect(within(today).getByText("2")).toBeInTheDocument();
    expect(screen.getByText("Night owl")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /Hide from others/ }),
    ).toBeNull();
    expect(
      screen.queryByRole("heading", { name: "Who sees your activity" }),
    ).toBeNull();
  });

  it("hides one event for the owner and shows it again", async () => {
    api.fetchProfileActivity.mockResolvedValue(ownPage([ev("a")]));
    api.setActivityEventHidden.mockResolvedValue({ id: "a", hidden: true });
    renderTab(true);

    fireEvent.click(
      await screen.findByRole("button", {
        name: "Hide from others: Played Game a",
      }),
    );
    expect(api.setActivityEventHidden).toHaveBeenCalledWith("a", true);
    expect(screen.getByText("Hidden")).toBeInTheDocument();

    const show = screen.getByRole("button", {
      name: "Show to others: Played Game a",
    });
    expect(show).toHaveAttribute("aria-pressed", "true");
    api.setActivityEventHidden.mockResolvedValue({ id: "a", hidden: false });
    fireEvent.click(show);
    expect(api.setActivityEventHidden).toHaveBeenLastCalledWith("a", false);
    expect(screen.queryByText("Hidden")).toBeNull();
  });

  it("puts a hidden event back when the save fails", async () => {
    api.fetchProfileActivity.mockResolvedValue(ownPage([ev("a")]));
    api.setActivityEventHidden.mockRejectedValue(
      Object.assign(new Error("x"), { status: 500 }),
    );
    renderTab(true);

    fireEvent.click(
      await screen.findByRole("button", { name: /Hide from others/ }),
    );
    await waitFor(() => expect(screen.queryByText("Hidden")).toBeNull());
    expect(
      screen.getByRole("button", { name: /Hide from others/ }),
    ).toBeInTheDocument();
  });

  it("saves a kind's switch and reverts it with an error when the save fails", async () => {
    api.fetchProfileActivity.mockResolvedValue(ownPage([ev("a")]));
    api.updateProfilePrefs.mockResolvedValueOnce({
      activity: { retention_days: 30, hidden_kinds: ["game"] },
      guestbook_enabled: true,
    });
    renderTab(true);

    const games = await screen.findByRole("switch", { name: "Games you play" });
    expect(games).toBeChecked();
    fireEvent.click(games);
    expect(api.updateProfilePrefs).toHaveBeenCalledWith({
      activity: { hidden_kinds: ["game"] },
    });
    await waitFor(() => expect(games).not.toBeChecked());
    expect(
      screen.getByRole("button", { name: "1 kind only you see" }),
    ).toBeInTheDocument();

    api.updateProfilePrefs.mockRejectedValueOnce(
      Object.assign(new Error("x"), { status: 500 }),
    );
    const voice = screen.getByRole("switch", { name: "Voice calls" });
    fireEvent.click(voice);
    expect(voice).not.toBeChecked();
    expect(api.updateProfilePrefs).toHaveBeenLastCalledWith({
      activity: { hidden_kinds: ["game", "voice"] },
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "Your settings are back to how they were.",
    );
    expect(voice).toBeChecked();
    expect(games).not.toBeChecked();
  });

  it("changes retention and drops what falls outside it", async () => {
    api.fetchProfileActivity.mockResolvedValue(
      ownPage([
        ev("new"),
        ev("old", { started_at: new Date(now - 20 * DAY).toISOString() }),
      ]),
    );
    api.updateProfilePrefs.mockResolvedValue({
      activity: { retention_days: 7, hidden_kinds: [] },
      guestbook_enabled: true,
    });
    renderTab(true);

    expect(await screen.findByText("Game old")).toBeInTheDocument();
    const rail = screen.getByRole("radiogroup", { name: "Keep activity for" });
    expect(within(rail).getByRole("radio", { name: "30 days" })).toBeChecked();
    fireEvent.click(within(rail).getByRole("radio", { name: "7 days" }));

    expect(api.updateProfilePrefs).toHaveBeenCalledWith({
      activity: { retention_days: 7 },
    });
    expect(within(rail).getByRole("radio", { name: "7 days" })).toBeChecked();
    expect(screen.queryByText("Game old")).toBeNull();
    expect(screen.getByText("Game new")).toBeInTheDocument();
  });

  it("pages with the cursor and filters by kind", async () => {
    api.fetchProfileActivity
      .mockResolvedValueOnce(page([ev("a")], { next_cursor: "c1" }))
      .mockResolvedValueOnce(page([ev("b"), ev("a")], { next_cursor: null }))
      .mockResolvedValueOnce(page([ev("v", { kind: "voice", data: {} })]));
    renderTab(false);

    fireEvent.click(await screen.findByRole("button", { name: "Show more" }));
    expect(api.fetchProfileActivity).toHaveBeenLastCalledWith("u1", {
      kind: null,
      cursor: "c1",
    });
    expect(await screen.findByText("Game b")).toBeInTheDocument();
    expect(screen.getAllByText("Game a")).toHaveLength(1);
    expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();

    fireEvent.click(screen.getByRole("radio", { name: "Voice" }));
    expect(
      await screen.findByText(/Hung out in a private server/),
    ).toBeInTheDocument();
    expect(api.fetchProfileActivity).toHaveBeenLastCalledWith(
      "u1",
      expect.objectContaining({ kind: "voice" }),
    );
  });

  it("shows empty states and a retry on error", async () => {
    api.fetchProfileActivity.mockResolvedValueOnce(ownPage([]));
    const { unmount } = renderTab(true);
    expect(await screen.findByText("Nothing yet")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Games, calls, badges and servers you join show up here.",
      ),
    ).toBeInTheDocument();
    unmount();

    api.fetchProfileActivity
      .mockRejectedValueOnce(Object.assign(new Error("x"), { status: 500 }))
      .mockResolvedValueOnce(page([]));
    renderTab(false);
    fireEvent.click(await screen.findByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Nothing to show yet")).toBeInTheDocument();
  });
});
