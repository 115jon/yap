import { describe, expect, it } from "vitest";

import {
  activeAnnouncement,
  DEFAULT_SITE_FLAGS,
  parseAnnouncement,
  parseSiteFlags,
  safeAnnouncementUrl,
} from "@/lib/site-flags";

describe("site flags", () => {
  it("reads only booleans for known switches; the rest stay on", () => {
    expect(parseSiteFlags(null)).toEqual(DEFAULT_SITE_FLAGS);
    expect(
      parseSiteFlags({ signups: false, linkEmbeds: "no", nonsense: false }),
    ).toEqual({ ...DEFAULT_SITE_FLAGS, signups: false });
  });

  it("keeps only http(s) announcement links", () => {
    expect(safeAnnouncementUrl("https://yap.115jon.com/status")).toBe(
      "https://yap.115jon.com/status",
    );
    expect(safeAnnouncementUrl("javascript:alert(1)")).toBeNull();
    expect(safeAnnouncementUrl("not a url")).toBeNull();
  });

  it("parses an announcement and drops it once it has ended", () => {
    const announcement = parseAnnouncement({
      id: "a",
      text: "  Maintenance tonight  ",
      tone: "shouting",
      link_url: "ftp://x",
      link_label: "Status",
      expires_at: "2026-11-09T12:00:00Z",
    });
    expect(announcement).toMatchObject({
      text: "Maintenance tonight",
      tone: "info",
      link_url: null,
      link_label: null,
    });
    expect(
      activeAnnouncement(announcement, Date.parse("2026-11-09T11:00:00Z")),
    ).not.toBeNull();
    expect(
      activeAnnouncement(announcement, Date.parse("2026-11-09T12:00:00Z")),
    ).toBeNull();
    expect(parseAnnouncement({ text: "   " })).toBeNull();
  });
});
