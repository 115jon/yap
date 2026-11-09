// @vitest-environment jsdom

import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ProfileSections } from "@/components/chat/ProfileCard";
import type { PublicConnection } from "@/lib/connections";

const steam: PublicConnection = {
  provider: "steam",
  display_name: "Gordon",
  profile_url: "https://steamcommunity.com/profiles/76561197960287930",
  avatar_url: "https://avatars.steamstatic.com/abc_full.jpg",
};

describe("the profile's Connections block", () => {
  it("links each shown account out in a new tab, without an opener or referrer", () => {
    render(<ProfileSections bio="hello" connections={[steam]} />);
    const section = screen
      .getByRole("heading", { name: "Connections" })
      .closest("section")!;
    const link = within(section).getByRole("link", {
      name: /Steam: Gordon/,
    });
    expect(link).toHaveAttribute("href", steam.profile_url);
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
    expect(link).toHaveAccessibleName("Steam: Gordon (opens in a new tab)");
    // A glyph, not Steam's avatar: opening a profile doesn't call Steam.
    expect(section.querySelector("img")).toBeNull();
  });

  it("is absent when nothing is shown", () => {
    const { rerender } = render(<ProfileSections bio="hi" connections={[]} />);
    expect(screen.queryByRole("heading", { name: "Connections" })).toBeNull();
    rerender(<ProfileSections bio="hi" />);
    expect(screen.queryByRole("heading", { name: "Connections" })).toBeNull();
  });

  it("prints anything but an https profile URL as text, and skips unknown providers", () => {
    render(
      <ProfileSections
        connections={[
          { ...steam, profile_url: "javascript:alert(1)" },
          {
            provider: "myspace" as never,
            display_name: "Tom",
            profile_url: "https://myspace.example/tom",
            avatar_url: null,
          },
        ]}
      />,
    );
    expect(screen.queryByRole("link")).toBeNull();
    expect(screen.getByText("Gordon")).toBeInTheDocument();
    expect(screen.queryByText("Tom")).toBeNull();
  });
});
