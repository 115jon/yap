// @vitest-environment jsdom

import { fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@/lib/profile-board-api", async (importActual) => {
  const actual = await importActual<typeof import("@/lib/profile-board-api")>();
  return {
    ...actual,
    saveProfileBoard: vi.fn(),
    uploadWidgetAsset: vi.fn(),
    fetchProfileBoard: vi.fn(),
    postGuestbookNote: vi.fn(),
  };
});

import type { AnyProfileWidget } from "@/lib/profile-board";
import type { Entitlement } from "@/lib/entitlements";

import ProfileStudio from "../ProfileStudio";
import {
  boardOf,
  FREE,
  LOUD,
  NOW,
  OWNER,
  Providers,
  WIDGETS,
} from "./editor-helpers";

const CUSTOM: AnyProfileWidget = {
  id: "w-custom",
  type: "custom",
  size: "L",
  cfg: { legend: "Commissions", title: "Open", asset: null, lines: ["One"] },
};

function renderStudio(entitlement: Entitlement, selected: string) {
  return render(
    <ProfileStudio
      fixture={boardOf([...WIDGETS, CUSTOM])}
      entitlementOverride={entitlement}
      owner={OWNER}
      now={NOW}
      onClose={() => {}}
      initialSelectedId={selected}
    />,
    { wrapper: Providers },
  );
}

const inspector = () =>
  screen.getByRole("complementary", { name: "Widget settings" });

beforeEach(() => localStorage.clear());

describe("Profile studio", () => {
  it("inspector edits update the board and their counters", () => {
    const { container } = renderStudio(LOUD, "w-count");
    const title = within(inspector()).getByLabelText("Title");
    expect(within(inspector()).getByText("10/40")).toBeInTheDocument();

    fireEvent.change(title, { target: { value: "Raid night: Vault" } });
    expect(within(inspector()).getByText("17/40")).toBeInTheDocument();
    // The selected countdown edits inline on the board too.
    expect(
      container.querySelector<HTMLInputElement>(
        '[data-edit-cell="w-count"] input[aria-label="Countdown title"]',
      )!.value,
    ).toBe("Raid night: Vault");
    expect(screen.getByText("Unsaved")).toBeInTheDocument();

    // Over the limit is cut at the limit.
    fireEvent.change(title, { target: { value: "x".repeat(60) } });
    expect(within(inspector()).getByText("40/40")).toBeInTheDocument();
  });

  it("resizes from the inspector within the allowed sizes", () => {
    const { container } = renderStudio(LOUD, "w-mood");
    const sizes = within(inspector()).getByRole("radiogroup", {
      name: "Mood size",
    });
    expect(
      within(sizes)
        .getAllByRole("radio")
        .map((r) => r.textContent),
    ).toEqual(["Small", "Medium"]);
    fireEvent.click(within(sizes).getByRole("radio", { name: "Medium" }));
    expect(
      container.querySelector(
        'li[data-widget-id="w-mood"] [data-slot=widget-plate]',
      ),
    ).toHaveAttribute("data-size", "M");
  });

  it("shows the upsell plate for Custom on a free plan and keeps the draft editable", () => {
    const { container } = renderStudio(FREE, "w-custom");
    expect(
      within(inspector()).getByText("Custom widgets come with Yap Loud"),
    ).toBeInTheDocument();
    fireEvent.change(within(inspector()).getByLabelText("Title"), {
      target: { value: "Commissions open" },
    });
    expect(
      container.querySelector<HTMLInputElement>(
        '[data-edit-cell="w-custom"] input[aria-label="Custom title"]',
      )!.value,
    ).toBe("Commissions open");
    expect(
      container.querySelector(
        'li[data-widget-id="w-custom"] [data-slot=widget-draft]',
      ),
    ).toHaveTextContent("Draft · only you");
  });

  it("Preview as Anyone hides the draft and the edit chrome", () => {
    const { container } = renderStudio(FREE, "w-custom");
    fireEvent.click(screen.getAllByRole("radio", { name: "Anyone" })[0]!);
    expect(container.querySelector('li[data-widget-id="w-custom"]')).toBeNull();
    expect(container.querySelector("[data-edit-cell]")).toBeNull();
    expect(
      screen.getByText(/1 widget is hidden from them/),
    ).toBeInTheDocument();
  });
});
