// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { User } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";

const api = vi.hoisted(() => ({
  apiGet: vi.fn((): Promise<unknown> => new Promise(() => {})),
  apiPost: vi.fn(() => Promise.resolve({})),
  apiPut: vi.fn(() => Promise.resolve({})),
  apiPatch: vi.fn(() => Promise.resolve({})),
  apiDelete: vi.fn(() => Promise.resolve({})),
}));

vi.mock("@/lib/api-client", () => api);

import { TooltipProvider } from "@/components/ui/tooltip";
import { OPEN_PROFILE_EDITOR_EVENT } from "@/lib/profile-editor-events";
import FriendsView from "../FriendsView";
import ProfileModalHost from "../ProfileModalHost";
import UserProfilePopover from "../UserProfilePopover";

class ResizeObserverMock {
  observe() {}
  disconnect() {}
  unobserve() {}
}

const me = { id: "u-me", username: "me", display_name: "Me" } as User;
const grace = {
  id: "u-grace",
  username: "grace",
  display_name: "Grace Hopper",
  status: "online",
  bio: "Wrote the **first** compiler.",
} as User;

beforeEach(() => {
  vi.stubGlobal("ResizeObserver", ResizeObserverMock);
  useChatStore.setState({
    user: me,
    members: [],
    relationships: [],
    activeServerId: null,
    profileUser: null,
    onlineUsers: new Set<string>(),
  } as never);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
  api.apiGet.mockImplementation(() => new Promise(() => {}));
});

function renderPopover(anchor: HTMLElement, onClose = vi.fn()) {
  render(
    <>
      <UserProfilePopover
        userId="u-grace"
        username="grace"
        seedUser={grace}
        anchorEl={anchor}
        onClose={onClose}
      />
      <ProfileModalHost />
    </>,
  );
  return onClose;
}

function makeAnchor() {
  const anchor = document.createElement("button");
  anchor.textContent = "Grace's avatar";
  document.body.appendChild(anchor);
  return anchor;
}

describe("the full profile", () => {
  it("opens from the popover's avatar, which is a labelled button", async () => {
    const anchor = makeAnchor();
    const onClose = renderPopover(anchor);

    const avatar = await screen.findByRole("button", {
      name: "View Grace Hopper's full profile",
    });
    expect(avatar.tagName).toBe("BUTTON");
    expect(avatar).not.toHaveAttribute("tabindex", "-1");

    fireEvent.click(avatar);

    expect(onClose).toHaveBeenCalled();
    expect(useChatStore.getState().profileUser?.id).toBe("u-grace");
    const dialog = await screen.findByRole("dialog", {
      name: "Grace Hopper's profile",
    });
    // The larger card, with the relationship actions and Call.
    expect(
      within(dialog).getByRole("button", { name: "Message" }),
    ).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "Call" })).toBeTruthy();
    // The bio renders its markdown.
    expect(dialog.querySelector("[data-profile-bio] strong")).toHaveTextContent(
      "first",
    );
    anchor.remove();
  });

  it("opens from a click on the popover's banner", async () => {
    const anchor = makeAnchor();
    renderPopover(anchor);

    await screen.findByRole("dialog", { name: "User profile for grace" });
    const banner = document.querySelector<HTMLElement>(
      "[data-profile-view-banner]",
    );
    expect(banner).toHaveAttribute("tabindex", "-1");
    fireEvent.click(banner!);

    expect(
      await screen.findByRole("dialog", { name: "Grace Hopper's profile" }),
    ).toBeInTheDocument();
    anchor.remove();
  });

  it("opens from the context menu's Profile item", async () => {
    useChatStore.setState({
      relationships: [{ user: grace, type: 0, created_at: "" }],
      onlineUsers: new Set(["u-grace"]),
    } as never);
    render(
      <TooltipProvider>
        <FriendsView onMenuClick={() => {}} onSelectDm={() => {}} />
        <ProfileModalHost />
      </TooltipProvider>,
    );

    fireEvent.contextMenu(
      screen.getByRole("button", { name: "Message Grace Hopper" }),
    );
    fireEvent.click(await screen.findByRole("menuitem", { name: "Profile" }));

    expect(
      await screen.findByRole("dialog", { name: "Grace Hopper's profile" }),
    ).toBeInTheDocument();
  });

  it("closes on Escape and gives focus back to what opened the popover", async () => {
    const anchor = makeAnchor();
    renderPopover(anchor);

    fireEvent.click(
      await screen.findByRole("button", {
        name: "View Grace Hopper's full profile",
      }),
    );
    const dialog = await screen.findByRole("dialog", {
      name: "Grace Hopper's profile",
    });

    fireEvent.keyDown(dialog, { key: "Escape" });

    await waitFor(() => expect(useChatStore.getState().profileUser).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(anchor));
    anchor.remove();
  });

  it("shows Edit profile on your own profile, without relationship actions", async () => {
    const opened = vi.fn();
    window.addEventListener(OPEN_PROFILE_EDITOR_EVENT, opened);
    render(<ProfileModalHost />);
    act(() => {
      useChatStore.setState({ profileUser: me } as never);
    });

    const dialog = await screen.findByRole("dialog", { name: "Me's profile" });
    expect(
      within(dialog).queryByRole("button", { name: "Message" }),
    ).toBeNull();
    expect(within(dialog).queryByRole("button", { name: "Call" })).toBeNull();
    expect(api.apiGet).not.toHaveBeenCalledWith("/api/users/u-me/profile");

    fireEvent.click(
      within(dialog).getByRole("button", { name: "Edit profile" }),
    );

    expect(opened).toHaveBeenCalled();
    await waitFor(() => expect(useChatStore.getState().profileUser).toBeNull());
    window.removeEventListener(OPEN_PROFILE_EDITOR_EVENT, opened);
  });
});
