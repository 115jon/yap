// @vitest-environment jsdom

import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ apiDelete: vi.fn() }));

vi.mock("@/lib/api-client", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api-client")>()),
  apiDelete: mocks.apiDelete,
}));

import type { Message, Relationship } from "@/lib/types";
import { useChatStore } from "@/stores/chat-store";
import { useDmBlockStore } from "@/stores/useDmBlockStore";
import { DmBlockedComposer } from "../DmBlockedComposer";
import MessageItem from "../MessageItem";

const blocked = (id: string): Relationship => ({
  user: { id, username: id } as Relationship["user"],
  type: 1,
  created_at: "2026-11-14T00:00:00.000Z",
});

function message(authorId: string, id = `m-${authorId}`): Message {
  return {
    id,
    channel_id: "channel-1",
    author_id: authorId,
    author: { id: authorId, username: authorId, avatar_url: null },
    content: "secret words",
    is_pinned: false,
    created_at: "2026-11-14T00:00:00.000Z",
    attachments: [],
    reactions: [],
  } as Message;
}

describe("blocked users on the web", () => {
  beforeEach(() => {
    act(() => {
      useChatStore.setState({ relationships: [blocked("user-blocked")] });
      useDmBlockStore.setState({ refusals: {} });
    });
    mocks.apiDelete.mockReset();
  });

  afterEach(() => {
    act(() => {
      useChatStore.setState({ relationships: [] });
    });
  });

  it("collapses a blocked user's message until Show", () => {
    render(
      <MessageItem
        message={message("user-blocked")}
        showHeader
        currentUserId="user-1"
      />,
    );

    expect(screen.getByText("Blocked message")).toBeInTheDocument();
    expect(screen.queryByText("secret words")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Show" }));

    expect(screen.queryByText("Blocked message")).not.toBeInTheDocument();
  });

  it("never collapses a preview", () => {
    render(
      <MessageItem
        message={message("user-blocked", "m-preview")}
        showHeader
        currentUserId="user-1"
        previewOnly
      />,
    );
    expect(screen.queryByText("Blocked message")).not.toBeInTheDocument();
  });

  it("offers Unblock on your own block and reloads the relationships", async () => {
    mocks.apiDelete.mockResolvedValue({ success: true });
    const loadRelationships = vi.fn(async () => {});
    act(() => {
      useChatStore.setState((state) => ({
        actions: { ...state.actions, loadRelationships },
      }));
    });

    render(
      <DmBlockedComposer
        block="you_blocked"
        channelId="channel-1"
        recipientId="user-blocked"
      />,
    );
    expect(
      screen.getByText("You blocked this user. Unblock to message."),
    ).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Unblock" }));
    });

    expect(mocks.apiDelete).toHaveBeenCalledWith("/api/friends", {
      target_user_id: "user-blocked",
    });
    expect(loadRelationships).toHaveBeenCalled();
  });

  it("says only that you can't message someone who blocked you", () => {
    render(
      <DmBlockedComposer
        block="blocked_you"
        channelId="channel-1"
        recipientId="user-2"
      />,
    );
    expect(
      screen.getByText("You can't message this user."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Unblock" })).toBeNull();
  });
});
