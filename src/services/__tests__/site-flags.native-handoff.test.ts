import { afterEach, describe, expect, it } from "vitest";

import {
  getSiteSettings,
  invalidateSiteSettingsCache,
  nativeHandoffConfig,
  SITE_SETTINGS_TTL_MS,
} from "@/services/site-flags.service";

type Db = Parameters<typeof nativeHandoffConfig>[0];

/** A D1 stand-in answering `rows` for the settings read. */
function db(rows: Array<{ key: string; value: string }>): Db {
  return {
    prepare: () => ({
      all: async () => ({ results: rows }),
    }),
  } as unknown as Db;
}

const broken = {
  prepare: () => ({
    all: async () => {
      throw new Error("D1_ERROR: storage unavailable");
    },
  }),
} as unknown as Db;

const on = db([
  {
    key: "nativeHandoff",
    value: JSON.stringify({ enabled: true, rolloutPercent: 40 }),
  },
]);

describe("the native handoff switch fails closed", () => {
  afterEach(() => invalidateSiteSettingsCache());

  it("serves a fresh read", async () => {
    expect(await nativeHandoffConfig(on, 1_000)).toEqual({
      enabled: true,
      rolloutPercent: 40,
    });
    // Within the TTL the copy is used, even if D1 now fails.
    expect(await nativeHandoffConfig(broken, 1_000 + 5_000)).toEqual({
      enabled: true,
      rolloutPercent: 40,
    });
  });

  it("answers off when D1 fails and the copy is too old", async () => {
    expect(await nativeHandoffConfig(on, 1_000)).toMatchObject({
      enabled: true,
    });
    expect(
      await nativeHandoffConfig(broken, 1_000 + SITE_SETTINGS_TTL_MS),
    ).toEqual({ enabled: false, rolloutPercent: 0 });
    // The kill switches keep the last copy instead (they fail open).
    const settings = await getSiteSettings(
      broken,
      1_000 + SITE_SETTINGS_TTL_MS,
    );
    expect(settings.nativeHandoff.enabled).toBe(true);
  });

  it("answers off with no D1 and nothing read", async () => {
    expect(await nativeHandoffConfig(null, 1_000)).toEqual({
      enabled: false,
      rolloutPercent: 0,
    });
    expect(await nativeHandoffConfig(broken, 1_000)).toEqual({
      enabled: false,
      rolloutPercent: 0,
    });
  });
});
