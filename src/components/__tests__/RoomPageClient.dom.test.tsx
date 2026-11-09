// @vitest-environment jsdom

import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// Counts how often the joined-room module is evaluated: the lobby must not
// load it (and the call stack behind it) before a guest shows intent to join.
const voiceView = vi.hoisted(() => ({ evaluations: 0 }));
const navigate = vi.hoisted(() => vi.fn());

vi.mock("@/components/RoomVoiceView", () => {
  voiceView.evaluations += 1;
  return {
    default: ({ onSignIn }: { onSignIn: () => void }) => (
      <div>
        joined room
        <button type="button" onClick={onSignIn}>
          Sign in
        </button>
      </div>
    ),
  };
});

vi.mock("@tanstack/react-router", () => ({
  useParams: () => ({ slug: "standup" }),
  useNavigate: () => navigate,
}));

import RoomPageClient from "@/components/RoomPageClient";

describe("RoomPageClient lobby", () => {
  beforeEach(() => {
    sessionStorage.clear();
    navigate.mockClear();
  });

  afterEach(() => {
    sessionStorage.clear();
  });

  it("shows the name prompt without loading the joined room", () => {
    render(<RoomPageClient />);
    expect(screen.getByText("Join standup")).toBeInTheDocument();
    expect(screen.getByLabelText("Your name")).toBeInTheDocument();
    expect(voiceView.evaluations).toBe(0);
  });

  it("loads the joined room once a name is typed and renders it on Join", async () => {
    render(<RoomPageClient />);
    fireEvent.change(screen.getByLabelText("Your name"), {
      target: { value: "Ada" },
    });
    await waitFor(() => expect(voiceView.evaluations).toBe(1));

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Join" }));
    });
    expect(await screen.findByText("joined room")).toBeInTheDocument();
    expect(sessionStorage.getItem("room-guest-name")).toBe("Ada");
  });

  it("sends a guest who signs in back to this room", async () => {
    sessionStorage.setItem("room-guest-name", "Ada");
    render(<RoomPageClient />);
    fireEvent.click(await screen.findByRole("button", { name: "Sign in" }));
    expect(navigate).toHaveBeenCalledWith({
      to: "/sign-in",
      search: { redirect_url: "/room/standup" },
    });
  });
});
