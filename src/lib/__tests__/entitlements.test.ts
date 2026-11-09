import { describe, expect, it } from "vitest";

import {
  effectiveTier,
  FREE_ENTITLEMENT,
  hasPerk,
  parseEntitlement,
  PERKS,
  type Entitlement,
} from "@/lib/entitlements";
import {
  BIG_UPLOAD_LIMITS,
  MB,
  UPLOAD_LIMITS,
  uploadLimitFor,
  WORKER_UPLOAD_CAP,
  CHUNKED_UPLOADS_ENABLED,
  clientUploadLimitFor,
} from "@/lib/upload-limits";
import {
  getAvailableStreamQualities,
  isHdStreamQuality,
} from "@/lib/voice/utils";

const NOW = Date.parse("2026-11-09T12:00:00Z");
const founder: Entitlement = { tier: "founder", source: "grant" };
const loud: Entitlement = {
  tier: "loud",
  source: "stripe",
  started_at: "2026-10-01T00:00:00Z",
  expires_at: "2026-12-01T00:00:00Z",
};
const lapsed: Entitlement = { ...loud, expires_at: "2026-11-01T00:00:00Z" };

describe("hasPerk", () => {
  it("gives free nothing", () => {
    for (const perk of PERKS) {
      expect(hasPerk(FREE_ENTITLEMENT, perk, NOW)).toBe(false);
      expect(hasPerk(null, perk, NOW)).toBe(false);
      expect(hasPerk(undefined, perk, NOW)).toBe(false);
    }
  });

  it("gives Loud everything but the Founder effect", () => {
    for (const perk of PERKS) {
      expect(hasPerk(loud, perk, NOW)).toBe(perk !== "founderEffect");
    }
  });

  it("gives founders every perk, with no expiry", () => {
    for (const perk of PERKS) expect(hasPerk(founder, perk, NOW)).toBe(true);
    expect(hasPerk(founder, "hdStreams", Date.parse("2099-01-01"))).toBe(true);
  });

  it("treats a lapsed plan as free", () => {
    expect(effectiveTier(lapsed, NOW)).toBe("free");
    expect(hasPerk(lapsed, "bigUploads", NOW)).toBe(false);
    expect(effectiveTier(loud, NOW)).toBe("loud");
  });

  it("treats an unknown tier as free", () => {
    expect(effectiveTier({ tier: "platinum" } as never, NOW)).toBe("free");
  });
});

describe("parseEntitlement", () => {
  it("reads a well-formed entitlement", () => {
    expect(parseEntitlement(loud)).toEqual(loud);
  });

  it("falls back to free for anything unreadable", () => {
    for (const value of [null, undefined, 42, "founder", {}, { tier: 1 }]) {
      expect(parseEntitlement(value)).toEqual(FREE_ENTITLEMENT);
    }
  });

  it("drops an unknown source and blank dates", () => {
    expect(
      parseEntitlement({ tier: "founder", source: "gift", started_at: "" }),
    ).toEqual({
      tier: "founder",
      source: null,
      started_at: null,
      expires_at: null,
    });
  });
});

describe("uploadLimitFor", () => {
  it("keeps the free limits on the free tier", () => {
    expect(uploadLimitFor(FREE_ENTITLEMENT, "attachment")).toBe(25 * MB);
    expect(uploadLimitFor(null, "attachment")).toBe(UPLOAD_LIMITS.attachment);
  });

  it("holds every tier to 25 MB on the single-request Worker path", () => {
    // The isolate buffers the multipart body (128 MB in all): big files go
    // through the chunked direct path instead.
    expect(WORKER_UPLOAD_CAP).toBe(25 * MB);
    expect(BIG_UPLOAD_LIMITS.attachment).toBe(500 * MB);
    expect(uploadLimitFor(founder, "attachment")).toBe(25 * MB);
    expect(uploadLimitFor(loud, "attachment")).toBe(25 * MB);
    // A kind whose free limit is above the cap keeps it.
    expect(uploadLimitFor(founder, "soundboard")).toBe(50 * MB);
  });

  it("offers the client the chunked limit now that chunked uploads are live", () => {
    expect(CHUNKED_UPLOADS_ENABLED).toBe(true);
    expect(clientUploadLimitFor(founder, "attachment")).toBe(500 * MB);
    expect(clientUploadLimitFor(loud, "attachment")).toBe(500 * MB);
    expect(clientUploadLimitFor(FREE_ENTITLEMENT, "attachment")).toBe(25 * MB);
    // Avatars and banners: 25 MB with the perk, 8 MB without.
    expect(clientUploadLimitFor(founder, "avatar")).toBe(25 * MB);
    expect(clientUploadLimitFor(loud, "banner")).toBe(25 * MB);
    expect(clientUploadLimitFor(FREE_ENTITLEMENT, "avatar")).toBe(
      UPLOAD_LIMITS.avatar,
    );
    // Kinds without a perk limit offer what they always did.
    expect(clientUploadLimitFor(founder, "gif")).toBe(UPLOAD_LIMITS.gif);
  });

  it("gives direct uploads the whole perk limit", () => {
    expect(uploadLimitFor(founder, "attachment", { direct: true })).toBe(
      500 * MB,
    );
    expect(
      uploadLimitFor(FREE_ENTITLEMENT, "attachment", { direct: true }),
    ).toBe(25 * MB);
  });

  it("leaves kinds without a perk limit alone", () => {
    for (const kind of Object.keys(UPLOAD_LIMITS) as Array<
      keyof typeof UPLOAD_LIMITS
    >) {
      if (kind in BIG_UPLOAD_LIMITS) continue;
      expect(uploadLimitFor(founder, kind)).toBe(UPLOAD_LIMITS[kind]);
    }
  });

  it("gives avatars and banners 25 MB with the perk, through the Worker", () => {
    expect(uploadLimitFor(founder, "avatar")).toBe(25 * MB);
    expect(uploadLimitFor(loud, "banner")).toBe(25 * MB);
    expect(uploadLimitFor(FREE_ENTITLEMENT, "avatar")).toBe(8 * MB);
    expect(uploadLimitFor(FREE_ENTITLEMENT, "banner")).toBe(8 * MB);
  });
});

describe("stream qualities", () => {
  it("hides 1440p and 4K without hdStreams", () => {
    const free = getAvailableStreamQualities(FREE_ENTITLEMENT);
    expect(free).toEqual(["720p30", "720p60", "1080p30", "1080p60"]);
    expect(getAvailableStreamQualities()).toEqual(free);
    expect(free.some(isHdStreamQuality)).toBe(false);
  });

  it("offers them to founders and Loud", () => {
    for (const entitlement of [founder, loud]) {
      const qualities = getAvailableStreamQualities(entitlement);
      expect(qualities).toContain("1440p60");
      expect(qualities).toContain("4k60");
    }
    expect(isHdStreamQuality("4k30")).toBe(true);
    expect(isHdStreamQuality("1080p60")).toBe(false);
  });
});
