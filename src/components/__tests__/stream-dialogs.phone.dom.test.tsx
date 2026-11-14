// @vitest-environment jsdom

import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";

import { ScreenShareModal } from "../ScreenShareModal";
import { UnifiedScreenShareModal } from "../UnifiedScreenShareModal";

// Every dialog here goes through ResponsiveDialog; pretend to be a phone.
vi.mock("@/hooks/useMediaQuery", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/hooks/useMediaQuery")>()),
  useIsMobile: () => true,
}));

vi.mock("@/stores/useVoiceSettingsStore", () => ({
  useVoiceSettingsStore: (selector: (state: unknown) => unknown) =>
    selector({
      getSettings: () => ({
        screenShareQuality: "1080p30",
        screenShareWithAudio: false,
      }),
      setScreenShareDefaults: () => {},
    }),
}));

beforeAll(() => {
  HTMLElement.prototype.scrollIntoView = vi.fn();
  HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
  HTMLElement.prototype.setPointerCapture = vi.fn();
  HTMLElement.prototype.releasePointerCapture = vi.fn();
});

afterEach(() => {
  cleanup();
  localStorage.clear();
});

const QUALITIES = ["720p30", "1080p30", "1080p60"];

function drawer() {
  const el = document.querySelector<HTMLElement>(
    '[data-slot="drawer-content"]',
  );
  expect(el).not.toBeNull();
  return el!;
}

describe("stream dialogs on a phone", () => {
  it("web share opens as a bottom drawer with quality chips, a switch and stacked actions", async () => {
    const onStart = vi.fn();
    const user = userEvent.setup();
    render(
      <ScreenShareModal
        isOpen
        onClose={() => {}}
        onStart={onStart}
        availableQualities={QUALITIES}
      />,
    );
    await waitFor(() => drawer());
    expect(document.querySelector('[data-slot="dialog-content"]')).toBeNull();
    // No fixed-width hand-rolled panel left over.
    expect(drawer().innerHTML).not.toMatch(/max-w-xl|w-\[calc\(100%-2rem\)\]/);

    // One source row hands off to the browser; quality is two chip rows.
    expect(screen.getByText("Choose what to share")).toBeVisible();
    const resolution = screen.getByRole("radiogroup", { name: "Resolution" });
    const frameRate = screen.getByRole("radiogroup", { name: "Frame rate" });
    expect(within(resolution).getAllByRole("radio")).toHaveLength(2);
    expect(within(frameRate).getAllByRole("radio")).toHaveLength(2);
    expect(
      within(resolution).getByRole("radio", { name: "1080p" }),
    ).toBeChecked();
    expect(
      within(frameRate).getByRole("radio", { name: "30 fps" }),
    ).toBeChecked();
    // 720p only comes at 30, so switching to it disables 60.
    await user.click(within(resolution).getByRole("radio", { name: "720p" }));
    expect(
      within(frameRate).getByRole("radio", { name: "60 fps" }),
    ).toBeDisabled();
    await user.click(within(resolution).getByRole("radio", { name: "1080p" }));
    await user.click(within(frameRate).getByRole("radio", { name: "60 fps" }));

    const audio = screen.getByRole("switch", { name: "Include audio" });
    await user.click(audio);
    expect(audio).toHaveAttribute("aria-checked", "true");

    const goLive = screen.getByRole("button", { name: "Go live" });
    // Phone footer buttons are 44px targets on touch.
    expect(goLive.className).toContain("pointer-coarse:min-h-11");
    await user.click(goLive);
    expect(onStart).toHaveBeenCalledWith({
      quality: "1080p60",
      withAudio: true,
    });
  });

  it("explains instead of offering a picker where sharing can't run", async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    render(
      <UnifiedScreenShareModal
        isOpen
        onClose={onClose}
        onStart={() => {}}
        availableQualities={QUALITIES}
        support={{ supported: false, reason: "mobile-browser" }}
      />,
    );
    await waitFor(() => drawer());
    expect(
      screen.getByText("Screen sharing isn't available here"),
    ).toBeVisible();
    expect(screen.getByText(/Phone and tablet browsers/)).toBeVisible();
    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("button", { name: "Go live" })).toBeNull();
    // A phone browser can hand the link to a computer.
    expect(
      screen.getByRole("button", { name: "Copy link for my computer" }),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Got it" }));
    expect(onClose).toHaveBeenCalled();
  });
});
