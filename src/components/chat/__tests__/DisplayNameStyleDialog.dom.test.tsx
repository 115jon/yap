// @vitest-environment jsdom

import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/components/ui/tooltip";
import type { DisplayNameStyle } from "@/lib/profile-customization";

vi.mock("@kova/react", () => ({
  useUser: () => ({ isLoaded: true, user: null }),
}));
vi.mock("@/hooks/useCustomEmojiLookup", () => ({
  useCustomEmojiLookup: () => ({}),
}));

import { DisplayNameStyleDialog } from "../SettingsAccountTab";

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

function open(
  props: {
    canUseYapStyles?: boolean;
    initialStyle?: DisplayNameStyle | null;
  } = {},
) {
  const onApply = vi.fn();
  render(
    <TooltipProvider>
      <DisplayNameStyleDialog
        open
        displayName="Peggy Quayle"
        username="peggy"
        initialStyle={props.initialStyle ?? null}
        themeStyle={{}}
        themeBackgroundColor={null}
        themeTextColor={null}
        canUseYapStyles={props.canUseYapStyles}
        onClose={() => {}}
        onApply={onApply}
      />
    </TooltipProvider>,
  );
  return { onApply };
}

const group = (id: "classic" | "yap") =>
  document.querySelector(`[data-style-group="${id}"]`) as HTMLElement;

describe("DisplayNameStyleDialog", () => {
  it("groups effects as Classic and Yap, each previewed on the real name", () => {
    open();
    const classic = within(group("classic")).getAllByRole("button");
    expect(classic.map((b) => b.getAttribute("aria-label"))).toEqual([
      "Solid",
      "Gradient",
      "Neon",
      "Toon",
      "Pop",
    ]);
    const yap = within(group("yap"))
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label"));
    expect(yap).toEqual([
      "Dither fade",
      "Holo",
      "Chrome",
      "Pixel shine",
      "CRT",
      "Glitch",
      "Ember",
      "Sticker",
      "Candlelit serif",
    ]);
    for (const tile of [
      ...classic,
      ...within(group("yap")).getAllByRole("button"),
    ]) {
      expect(tile.querySelector(".rm-name")).toHaveTextContent("Peggy Quayle");
    }
    expect(screen.getByRole("switch", { name: "Wave" })).toBeInTheDocument();
  });

  it("brings an effect's recommended font, keeps fonts free, and adds Wave", () => {
    const { onApply } = open();
    fireEvent.click(screen.getByRole("button", { name: "CRT" }));
    // A fixed palette hides the colour pickers.
    expect(screen.getByText("CRT uses its own colors.")).toBeInTheDocument();
    expect(screen.queryByText("Base color")).not.toBeInTheDocument();
    // Any font still works with it.
    fireEvent.click(screen.getByRole("button", { name: /Ag ?Candle$/ }));
    fireEvent.click(screen.getByRole("switch", { name: "Wave" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith(
      expect.objectContaining({
        effect: "crt",
        font: "instrument-serif",
        wave: true,
      }),
    );
  });

  it("names Dither fade's two colours", () => {
    open();
    fireEvent.click(screen.getByRole("button", { name: "Dither fade" }));
    expect(screen.getByText("First color")).toBeInTheDocument();
    expect(screen.getByText("Second color")).toBeInTheDocument();
  });

  it("locks the Yap group when canUseYapStyles is false", () => {
    open({ canUseYapStyles: false });
    for (const tile of within(group("yap")).getAllByRole("button"))
      expect(tile).toBeDisabled();
    expect(screen.getByRole("switch", { name: "Wave" })).toBeDisabled();
    for (const tile of within(group("classic")).getAllByRole("button"))
      expect(tile).toBeEnabled();
  });

  it("will not apply a saved Yap style on a locked account", () => {
    open({
      canUseYapStyles: false,
      initialStyle: {
        font: "syne",
        effect: "holo",
        primaryColor: "#F8FAFC",
        secondaryColor: "#FB7185",
      },
    });
    expect(screen.getByRole("button", { name: "Apply" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Solid" }));
    fireEvent.click(screen.getByRole("button", { name: /Ag ?gg sans$/ }));
    expect(screen.getByRole("button", { name: "Apply" })).toBeEnabled();
  });
});
