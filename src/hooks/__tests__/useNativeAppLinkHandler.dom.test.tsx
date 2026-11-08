// @vitest-environment jsdom

import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  canOffer: vi.fn(() => true),
  launch: vi.fn(),
  toastInfo: vi.fn(),
}));

vi.mock("@/lib/native-app-link", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/native-app-link")>()),
  canOfferNativeApp: mocks.canOffer,
  launchNativeApp: mocks.launch,
}));

vi.mock("@/components/ui/toast", () => ({
  toast: { info: mocks.toastInfo },
}));

import { useNativeAppLinkHandler } from "@/hooks/useNativeAppLinkHandler";
import { writeOpenLinksInNative } from "@/lib/native-app-link";

const origin = window.location.origin;

function Harness() {
  useNativeAppLinkHandler();
  return (
    <>
      <a data-message-link="" href={`${origin}/invite/quiet-pro`}>
        invite
      </a>
      <a
        data-message-link=""
        href={`${origin}/chat/s1/c1?message=m1`}
        target="_blank"
        rel="noreferrer"
      >
        message
      </a>
      <a data-message-link="" href="https://example.org/invite/quiet-pro">
        elsewhere
      </a>
      <a href={`${origin}/chat/s1/c1`}>sidebar</a>
    </>
  );
}

/** Clicks a link; true when the click was left to the browser. */
function click(name: string, init: MouseEventInit = {}): boolean {
  const event = new MouseEvent("click", {
    bubbles: true,
    cancelable: true,
    button: 0,
    ...init,
  });
  screen.getByText(name).dispatchEvent(event);
  return !event.defaultPrevented;
}

describe("useNativeAppLinkHandler", () => {
  beforeEach(() => {
    mocks.canOffer.mockReturnValue(true);
    // jsdom does not navigate; keep stray defaults quiet.
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(() => {
    writeOpenLinksInNative(false);
    vi.clearAllMocks();
    vi.restoreAllMocks();
  });

  it("does nothing while the setting is off", () => {
    render(<Harness />);
    expect(click("invite")).toBe(true);
    expect(mocks.launch).not.toHaveBeenCalled();
  });

  it("hands Yap message and invite links to the app when on", () => {
    writeOpenLinksInNative(true);
    render(<Harness />);
    expect(click("invite")).toBe(false);
    expect(mocks.launch).toHaveBeenLastCalledWith(
      "yap-native://invite/quiet-pro",
    );
    expect(click("message")).toBe(false);
    expect(mocks.launch).toHaveBeenLastCalledWith(
      "yap-native://chat/s1/c1?message=m1",
    );
    expect(mocks.toastInfo).toHaveBeenCalledWith(
      "Opening in Yap Native",
      expect.objectContaining({
        action: expect.objectContaining({ label: "Open here" }),
      }),
    );
  });

  it("keeps the web page one click away", () => {
    writeOpenLinksInNative(true);
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    render(<Harness />);
    click("invite");
    const { action } = mocks.toastInfo.mock.calls[0][1];
    action.onClick();
    expect(open).toHaveBeenCalledWith(
      `${origin}/invite/quiet-pro`,
      "_blank",
      "noopener,noreferrer",
    );
  });

  it("leaves other links and modified clicks alone", () => {
    writeOpenLinksInNative(true);
    render(<Harness />);
    expect(click("elsewhere")).toBe(true);
    expect(click("sidebar")).toBe(true);
    expect(click("invite", { ctrlKey: true })).toBe(true);
    expect(click("invite", { button: 1 })).toBe(true);
    expect(mocks.launch).not.toHaveBeenCalled();
  });

  it("stays off where the app can't be offered (Tauri, mobile)", () => {
    mocks.canOffer.mockReturnValue(false);
    writeOpenLinksInNative(true);
    render(<Harness />);
    expect(click("invite")).toBe(true);
    expect(mocks.launch).not.toHaveBeenCalled();
  });
});
