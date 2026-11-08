import { describe, expect, it } from "vitest";

import {
  getSectionPosition,
  isRowAvailable,
  searchSettings,
} from "@/components/chat/settings/schema";

describe("the Yap Native links setting", () => {
  it("shows only where the browser can offer the app", () => {
    expect(
      isRowAvailable("open-links-in-native", {
        desktop: false,
        nativeApp: true,
      }),
    ).toBe(true);
    expect(isRowAvailable("open-links-in-native", { desktop: false })).toBe(
      false,
    );
    expect(isRowAvailable("open-links-in-native", { desktop: true })).toBe(
      false,
    );
  });

  it("keeps the section count honest where it is hidden", () => {
    expect(getSectionPosition("motion", { desktop: false })).toEqual({
      index: 2,
      total: 2,
    });
    expect(
      getSectionPosition("links", { desktop: false, nativeApp: true }),
    ).toEqual({ index: 3, total: 3 });
  });

  it("is found by search only where it shows", () => {
    const ids = (nativeApp: boolean) =>
      searchSettings("yap native", { desktop: false, nativeApp }).map(
        (hit) => hit.row.id,
      );
    expect(ids(true)).toContain("open-links-in-native");
    expect(ids(false)).not.toContain("open-links-in-native");
  });
});
