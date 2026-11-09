// @vitest-environment jsdom

import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { OwnConnection } from "@/lib/connections";

const api = vi.hoisted(() => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  apiPatch: vi.fn(),
  apiDelete: vi.fn(),
}));
const loadCurrentUser = vi.hoisted(() => vi.fn(async () => true));
const runtime = vi.hoisted(() => ({
  desktop: false,
  voiceChannelStates: {} as Record<string, Array<{ user_id: string }>>,
  dmCall: false,
}));

vi.mock("@/lib/api-client", () => api);
vi.mock("@/lib/runtime", () => ({
  isDesktop: () => runtime.desktop,
  isMobile: () => false,
  isTauri: () => runtime.desktop,
}));
vi.mock("@/stores/chat-store", () => ({
  useChatStore: (select: (state: unknown) => unknown) =>
    select({
      actions: { loadCurrentUser },
      user: { id: "me" },
      voiceChannelStates: runtime.voiceChannelStates,
    }),
}));
vi.mock("@/stores/useCallVoiceStore", () => ({
  useCallVoiceStore: (select: (state: unknown) => unknown) =>
    select({ joined: runtime.dmCall }),
}));

import ConnectionsPanel from "../ConnectionsPanel";
import { SettingsContext, type SettingsContextValue } from "../primitives";

const context: SettingsContextValue = {
  env: { desktop: false },
  query: "",
  target: null,
  registerEscape: () => () => {},
  openProfileEditor: () => {},
  openThemePreview: () => {},
};

function renderPanel(query = "") {
  return render(
    <SettingsContext.Provider value={{ ...context, query }}>
      <ConnectionsPanel />
    </SettingsContext.Provider>,
  );
}

const linked: OwnConnection = {
  provider: "steam",
  display_name: "Gordon",
  profile_url: "https://steamcommunity.com/profiles/76561197960287930",
  avatar_url: "https://avatars.steamstatic.com/abc_full.jpg",
  show_on_profile: true,
  verified_at: "2026-11-09T12:00:00.000Z",
};

beforeEach(() => {
  vi.clearAllMocks();
  runtime.desktop = false;
  runtime.voiceChannelStates = {};
  runtime.dmCall = false;
});

describe("Settings › Connections", () => {
  it("offers Connect Steam when nothing is linked, and sends the browser to Steam", async () => {
    api.apiGet.mockResolvedValue({ connections: [] });
    api.apiPost.mockResolvedValue({
      url: "https://steamcommunity.com/openid/login?openid.mode=checkid_setup",
    });
    const assign = vi.fn();
    const original = window.location;
    Object.defineProperty(window, "location", {
      configurable: true,
      value: { ...original, assign },
    });
    try {
      renderPanel();
      const card = await screen.findByRole("region", { name: "Steam" });
      expect(within(card).getByText("Not connected")).toBeInTheDocument();
      // No profile switch before anything is linked.
      expect(within(card).queryByRole("switch")).toBeNull();

      await userEvent.click(
        within(card).getByRole("button", { name: "Connect Steam" }),
      );
      expect(api.apiPost).toHaveBeenCalledWith(
        "/api/connections/steam/start",
        {},
      );
      await waitFor(() =>
        expect(assign).toHaveBeenCalledWith(
          "https://steamcommunity.com/openid/login?openid.mode=checkid_setup",
        ),
      );
    } finally {
      Object.defineProperty(window, "location", {
        configurable: true,
        value: original,
      });
    }
  });

  it("asks before leaving the page during a call", async () => {
    runtime.voiceChannelStates = { lounge: [{ user_id: "me" }] };
    api.apiGet.mockResolvedValue({ connections: [] });
    api.apiPost.mockRejectedValue(new Error("stop here"));
    renderPanel();
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect Steam" }),
    );
    expect(api.apiPost).not.toHaveBeenCalled();
    const confirm = screen.getByRole("group", { name: /ends your call/ });
    await userEvent.click(
      within(confirm).getByRole("button", { name: "Leave call and connect" }),
    );
    expect(api.apiPost).toHaveBeenCalledWith(
      "/api/connections/steam/start",
      {},
    );
  });

  it("shows why a start failed and stays put", async () => {
    api.apiGet.mockResolvedValue({ connections: [] });
    api.apiPost.mockRejectedValue(
      Object.assign(new Error("Rate limit exceeded. Please slow down."), {
        status: 429,
      }),
    );
    renderPanel();
    await userEvent.click(
      await screen.findByRole("button", { name: "Connect Steam" }),
    );
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Connect Steam" }),
    ).not.toBeDisabled();
  });

  it("opens the web app in the browser from the desktop app", async () => {
    runtime.desktop = true;
    api.apiGet.mockResolvedValue({ connections: [] });
    renderPanel();
    const link = await screen.findByRole("link", {
      name: "Connect Steam in browser",
    });
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(link.getAttribute("href")).toMatch(/\/chat\?settings=connections$/);
    expect(link.getAttribute("href")).not.toContain(".site");
  });

  it("shows a linked account with its avatar, name, profile link and switch", async () => {
    api.apiGet.mockResolvedValue({ connections: [linked] });
    renderPanel();
    const card = await screen.findByRole("region", { name: "Steam" });
    expect(await within(card).findByText("Gordon")).toBeInTheDocument();
    expect(within(card).getByText("Verified")).toBeInTheDocument();
    const avatar = card.querySelector("img")!;
    // Through Yap's media proxy: viewing settings doesn't call Steam's CDN.
    expect(avatar.getAttribute("src")).toContain("/api/proxy-media?url=");
    const profile = within(card).getByRole("link", { name: /Steam profile/ });
    expect(profile).toHaveAttribute("href", linked.profile_url);
    expect(profile).toHaveAttribute("rel", "noopener noreferrer");
    expect(
      within(card).getByRole("switch", { name: "Show on profile" }),
    ).toHaveAttribute("aria-checked", "true");
  });

  it("saves Show on profile, refreshes the profile, and rolls back on failure", async () => {
    api.apiGet.mockResolvedValue({ connections: [linked] });
    api.apiPatch.mockResolvedValueOnce({});
    renderPanel();
    const toggle = await screen.findByRole("switch", {
      name: "Show on profile",
    });
    await userEvent.click(toggle);
    expect(api.apiPatch).toHaveBeenCalledWith("/api/connections/steam", {
      showOnProfile: false,
    });
    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-checked", "false"),
    );
    await waitFor(() => expect(loadCurrentUser).toHaveBeenCalled());

    api.apiPatch.mockRejectedValueOnce(new Error("nope"));
    await userEvent.click(toggle);
    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-checked", "false"),
    );
    expect(await screen.findByRole("alert")).toBeInTheDocument();
  });

  it("unlinks behind an inline confirm", async () => {
    api.apiGet.mockResolvedValue({ connections: [linked] });
    api.apiDelete.mockResolvedValue({ removed: true });
    renderPanel();
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Unlink Steam account Gordon",
      }),
    );
    // Nothing happens until the confirm.
    expect(api.apiDelete).not.toHaveBeenCalled();
    const confirm = screen.getByRole("group", { name: /removed from Yap/ });
    await userEvent.click(
      within(confirm).getByRole("button", { name: "Unlink Steam" }),
    );
    expect(api.apiDelete).toHaveBeenCalledWith("/api/connections/steam");
    expect(
      await screen.findByRole("button", { name: "Connect Steam" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("Gordon")).toBeNull();
    expect(loadCurrentUser).toHaveBeenCalled();
  });

  it("cancels an unlink without calling the API", async () => {
    api.apiGet.mockResolvedValue({ connections: [linked] });
    renderPanel();
    await userEvent.click(
      await screen.findByRole("button", {
        name: "Unlink Steam account Gordon",
      }),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(api.apiDelete).not.toHaveBeenCalled();
    expect(screen.getByText("Gordon")).toBeInTheDocument();
  });

  it("shows a load error with a retry", async () => {
    api.apiGet.mockRejectedValueOnce(new Error("offline"));
    api.apiGet.mockResolvedValueOnce({ connections: [] });
    renderPanel();
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(
      await screen.findByRole("button", { name: "Connect Steam" }),
    ).toBeInTheDocument();
  });

  it("is found by settings search", async () => {
    api.apiGet.mockResolvedValue({ connections: [] });
    renderPanel("deadlock");
    expect(
      await screen.findByRole("region", { name: "Steam" }),
    ).toBeInTheDocument();
  });
});
