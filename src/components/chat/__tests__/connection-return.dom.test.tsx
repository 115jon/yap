// @vitest-environment jsdom

import { render } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  openSettings: vi.fn(),
  toast: { success: vi.fn(), info: vi.fn(), error: vi.fn() },
}));

vi.mock("@/components/chat/settings/settings-links", () => ({
  openSettings: mocks.openSettings,
}));
vi.mock("@/components/ui/toast", () => ({ toast: mocks.toast }));

import {
  ConnectionReturn,
  readConnectionReturn,
} from "@/components/chat/ConnectionReturn";

beforeEach(() => {
  vi.clearAllMocks();
  window.history.replaceState(null, "", "/chat");
});

describe("readConnectionReturn", () => {
  it("reads a callback outcome and strips only its own parameters", () => {
    expect(
      readConnectionReturn(
        "https://yap.115jon.com/chat?x=1&connection=steam&connection_status=taken#m",
      ),
    ).toEqual({
      intent: { kind: "result", provider: "steam", status: "taken" },
      cleaned: "/chat?x=1#m",
    });
  });

  it("opens Connections without a toast for a mangled outcome or ?settings=connections", () => {
    expect(
      readConnectionReturn(
        "https://yap.115jon.com/chat?connection=steam&connection_status=pwned",
      )?.intent,
    ).toEqual({ kind: "open" });
    expect(
      readConnectionReturn("https://yap.115jon.com/chat?settings=connections"),
    ).toEqual({ intent: { kind: "open" }, cleaned: "/chat" });
  });

  it("ignores everything else", () => {
    expect(readConnectionReturn("https://yap.115jon.com/chat")).toBeNull();
    expect(
      readConnectionReturn("https://yap.115jon.com/chat?settings=voice"),
    ).toBeNull();
  });
});

describe("<ConnectionReturn />", () => {
  it("lands in Settings › Connections with a toast and cleans the address", () => {
    window.history.replaceState(
      null,
      "",
      "/chat?connection=steam&connection_status=linked",
    );
    render(<ConnectionReturn />);
    expect(mocks.openSettings).toHaveBeenCalledWith({ panel: "connections" });
    expect(mocks.toast.success).toHaveBeenCalledWith("Steam connected", {
      id: "connection-result",
      duration: 10_000,
    });
    expect(window.location.search).toBe("");
  });

  it("says why a link failed", () => {
    window.history.replaceState(
      null,
      "",
      "/chat?connection=steam&connection_status=taken",
    );
    render(<ConnectionReturn />);
    expect(mocks.toast.error).toHaveBeenCalledWith(
      "That Steam account is already connected to another Yap account.",
      { id: "connection-result" },
    );
  });

  it("does nothing on a plain /chat", () => {
    render(<ConnectionReturn />);
    expect(mocks.openSettings).not.toHaveBeenCalled();
    expect(mocks.toast.success).not.toHaveBeenCalled();
  });
});
