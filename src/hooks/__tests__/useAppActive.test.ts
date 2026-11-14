// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";

import { cleanup, renderHook } from "@testing-library/react";
import { useAppActive } from "@/hooks/useAppActive";

describe("useAppActive listener cleanup", () => {
  it("removes its visibility listener after the last subscriber", () => {
    const remove = vi.spyOn(document, "removeEventListener");
    const first = renderHook(() => useAppActive());
    const second = renderHook(() => useAppActive());
    first.unmount();
    expect(remove).not.toHaveBeenCalledWith(
      "visibilitychange",
      expect.any(Function),
    );
    second.unmount();
    expect(remove).toHaveBeenCalledWith(
      "visibilitychange",
      expect.any(Function),
    );
    remove.mockRestore();
    cleanup();
  });
});
