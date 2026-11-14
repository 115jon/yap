// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from "vitest";

import { copyMediaLink, getShareableUrl } from "../media-actions";

const ORIGIN = window.location.origin;

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("getShareableUrl", () => {
  it("drops the session token and the capability from Yap's own media", () => {
    for (const raw of [
      "/api/attachments/c1/a1/f.png?token=session-token",
      `${ORIGIN}/api/attachments/c1/a1/f.png?token=session-token`,
      `${ORIGIN}/api/attachments/c1/a1/f.png?cap=m1.dXNlcg.1.sig`,
      "http://localhost/api/emojis/assets/e1?token=session-token&cap=m1.a.b.c",
      `${ORIGIN}/api/listen-together/stream?videoId=v&roomSlug=r&token=session-token`,
      "api/voice-status-media/s/a/f.gif?token=session-token",
    ]) {
      const shared = getShareableUrl(raw);
      expect(shared, raw).not.toMatch(/[?&](token|cap)=/);
      expect(shared, raw).not.toContain("session-token");
      expect(shared.startsWith(`${ORIGIN}/api/`), raw).toBe(true);
    }
  });

  it("keeps the rest of the address", () => {
    expect(
      getShareableUrl(
        `${ORIGIN}/api/listen-together/stream?videoId=v&cap=m1.a.b.c&roomSlug=r`,
      ),
    ).toBe(`${ORIGIN}/api/listen-together/stream?videoId=v&roomSlug=r`);
  });

  it("leaves other sites' links alone", () => {
    const external = "https://example.com/api/attachments/x.png?token=theirs";
    expect(getShareableUrl(external)).toBe(external);
    expect(getShareableUrl("mailto:someone@example.com")).toBe(
      "mailto:someone@example.com",
    );
  });

  it("makes relative links absolute on the web origin", () => {
    expect(getShareableUrl("/invite/abc")).toBe(`${ORIGIN}/invite/abc`);
  });
});

describe("copyMediaLink", () => {
  it("copies the shareable address, never the signed one", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", { clipboard: { writeText } });

    await copyMediaLink(
      `${ORIGIN}/api/attachments/c1/a1/f.png?token=session-token`,
    );

    expect(writeText).toHaveBeenCalledWith(
      `${ORIGIN}/api/attachments/c1/a1/f.png`,
    );
  });
});
