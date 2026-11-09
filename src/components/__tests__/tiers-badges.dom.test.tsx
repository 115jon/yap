// @vitest-environment jsdom

import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";

import { TierCards } from "@/components/chat/settings/TierCards";
import { TooltipProvider } from "@/components/ui/tooltip";
import { UserBadges } from "@/components/ui/user-badges";
import { BADGES, type UserBadge } from "@/lib/badges";

const NOW = Date.parse("2026-11-09T12:00:00Z");

const ALL: UserBadge[] = [...BADGES]
  .reverse()
  .map((badge) => ({ id: badge.id, awarded_at: "2026-11-09T00:00:00Z" }));

function renderBadges(props: Parameters<typeof UserBadges>[0]) {
  return render(
    <TooltipProvider>
      <UserBadges now={NOW} {...props} />
    </TooltipProvider>,
  );
}

describe("UserBadges", () => {
  it("renders nothing without badges", () => {
    const { container } = renderBadges({ badges: [] });
    expect(container).toBeEmptyDOMElement();
  });

  it("wraps every badge, in rank order, when there is no limit", () => {
    renderBadges({ badges: ALL });
    const list = screen.getByRole("list", { name: "Badges" });
    expect(list.className).toContain("flex-wrap");
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(BADGES.length);
    expect(items.map((item) => item.getAttribute("data-badge"))).toEqual(
      BADGES.map((badge) => badge.id),
    );
    expect(items[0]).toHaveAccessibleName(
      "Founder: Here before launch. Lifetime Loud.",
    );
  });

  it("folds the rest into +N past maxVisible, naming them", () => {
    renderBadges({ badges: ALL, maxVisible: 4 });
    const list = screen.getByRole("list", { name: "Badges" });
    expect(list.className).toContain("flex-nowrap");
    const items = within(list).getAllByRole("listitem");
    // Three badges and the +N key fill the four slots.
    expect(items).toHaveLength(4);
    const more = items[3]!;
    expect(more).toHaveTextContent(`+${BADGES.length - 3}`);
    expect(more.getAttribute("aria-label")).toBe(
      `${BADGES.length - 3} more: ${BADGES.slice(3)
        .map((badge) => badge.name)
        .join(", ")}`,
    );
  });

  it("shows all of them when they fit maxVisible", () => {
    renderBadges({ badges: ALL.slice(0, 2), maxVisible: 4 });
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.queryByText(/^\+\d/)).toBeNull();
  });

  it("drops badges this client does not know", () => {
    renderBadges({
      badges: [
        { id: "from-the-future", awarded_at: "" } as unknown as UserBadge,
        { id: "founder", awarded_at: "" },
      ],
    });
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
  });

  it("toggles the tooltip on tap, with the earned date", async () => {
    const user = userEvent.setup();
    renderBadges({
      badges: [{ id: "voice-regular", awarded_at: "2026-10-02T00:00:00Z" }],
    });
    await user.click(screen.getByRole("listitem"));
    expect(
      (await screen.findAllByText("Earned Oct 2, 2026")).length,
    ).toBeGreaterThan(0);
  });
});

describe("TierCards", () => {
  it("shows founders their lifetime plan with nothing to buy", () => {
    render(
      <TierCards
        entitlement={{ tier: "founder", source: "grant" }}
        billingEnabled
        now={NOW}
      />,
    );
    const current = screen.getByRole("article", { name: "Founder, your plan" });
    expect(current).toHaveTextContent("Founder · lifetime");
    expect(screen.getAllByRole("article")).toHaveLength(3);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getAllByText("Included")).toHaveLength(2);
  });

  it("keeps Upgrade disabled with Coming soon until billing is on", () => {
    render(<TierCards entitlement={{ tier: "free" }} billingEnabled={false} />);
    expect(
      screen.getByRole("article", { name: "Yap, your plan" }),
    ).toBeInTheDocument();
    const loud = screen.getByRole("article", { name: "Yap Loud" });
    expect(
      within(loud).getByRole("button", { name: "Upgrade" }),
    ).toBeDisabled();
    expect(within(loud).getByText("Coming soon")).toBeInTheDocument();
    // No purchase on the Founder card.
    const founder = screen.getByRole("article", { name: "Founder" });
    expect(within(founder).queryByRole("button")).toBeNull();
  });

  it("never promises a bigger upload than the server takes", () => {
    render(<TierCards entitlement={{ tier: "free" }} billingEnabled={false} />);
    // Chunked uploads are off: every plan is held to the Worker's 25 MB.
    const loud = screen.getByRole("article", { name: "Yap Loud" });
    expect(within(loud).getByText("25 MB uploads")).toBeInTheDocument();
    expect(screen.queryByText(/500 MB/)).toBeNull();
    expect(screen.queryByText("{uploads}")).toBeNull();
  });

  it("enables Upgrade once billing is on", () => {
    render(<TierCards entitlement={{ tier: "free" }} billingEnabled />);
    expect(screen.getByRole("button", { name: "Upgrade" })).toBeEnabled();
    expect(screen.queryByText("Coming soon")).toBeNull();
  });

  it("shows a subscriber their plan with Manage", () => {
    render(
      <TierCards
        entitlement={{
          tier: "loud",
          source: "stripe",
          started_at: "2026-09-01T00:00:00Z",
          expires_at: "2026-12-01T00:00:00Z",
        }}
        billingEnabled={false}
        now={NOW}
      />,
    );
    const loud = screen.getByRole("article", { name: "Yap Loud, your plan" });
    expect(loud).toHaveTextContent("Your plan · renews Dec 1, 2026");
    expect(within(loud).getByRole("button", { name: "Manage" })).toBeDisabled();
    expect(screen.queryByRole("button", { name: "Upgrade" })).toBeNull();
  });

  it("treats a lapsed subscription as free", () => {
    render(
      <TierCards
        entitlement={{ tier: "loud", expires_at: "2026-11-01T00:00:00Z" }}
        billingEnabled={false}
        now={NOW}
      />,
    );
    expect(
      screen.getByRole("article", { name: "Yap, your plan" }),
    ).toBeInTheDocument();
  });
});
