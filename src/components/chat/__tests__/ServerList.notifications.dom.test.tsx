// @vitest-environment jsdom

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";

import { initialState } from "@/lib/chat-reducer";
import type { Channel, Server } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";
import ServerList from "../ServerList";

const servers = [
  { id: "s-1", name: "Loud", icon_url: null, owner_id: "u-1" },
  { id: "s-2", name: "Quiet", icon_url: null, owner_id: "u-1" },
] as unknown as Server[];

const channels = [
  { id: "c-1", server_id: "s-1", name: "general" },
  { id: "c-2", server_id: "s-2", name: "general" },
] as unknown as Channel[];

const lastMessageAt = {
  "c-1": "2026-11-14T12:00:00.000Z",
  "c-2": "2026-11-14T12:00:00.000Z",
};

function pillFor(name: string) {
  return screen
    .getByRole("button", { name })
    .closest(".group")
    ?.querySelector("[data-pill]")
    ?.getAttribute("data-pill");
}

describe("ServerList notification mutes", () => {
  beforeEach(() => {
    useChatStore.setState({
      ...initialState,
      notificationSettings: {
        "server:s-2": {
          target_type: "server",
          target_id: "s-2",
          level: null,
          muted: true,
          muted_until: null,
        },
      },
      notificationSettingsNow: Date.now(),
    });
  });

  it("lights no unread pill for a muted server", () => {
    render(
      <ServerList
        servers={servers}
        activeServerId={null}
        onSelect={() => {}}
        channels={channels}
        lastMessageAt={lastMessageAt}
      />,
    );
    expect(pillFor("Loud")).toBe("unread");
    expect(pillFor("Quiet")).toBe("none");
  });

  it("skips muted channels when it adds up a server's unread", () => {
    useChatStore.setState({
      notificationSettings: {
        "channel:c-1": {
          target_type: "channel",
          target_id: "c-1",
          level: null,
          muted: true,
          muted_until: null,
        },
      },
    });
    render(
      <ServerList
        servers={servers}
        activeServerId={null}
        onSelect={() => {}}
        channels={channels}
        lastMessageAt={lastMessageAt}
      />,
    );
    expect(pillFor("Loud")).toBe("none");
    expect(pillFor("Quiet")).toBe("unread");
  });

  it("offers notification settings in the server menu", async () => {
    render(
      <ServerList
        servers={servers}
        activeServerId={null}
        onSelect={() => {}}
      />,
    );
    fireEvent.contextMenu(screen.getByRole("button", { name: "Quiet" }));
    expect(await screen.findByText("Notifications")).toBeInTheDocument();
    expect(screen.getByText("Muted")).toBeInTheDocument();
  });
});
