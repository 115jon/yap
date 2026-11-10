import { describe, expect, it } from "vitest";

import {
  DEFAULT_NATIVE_HANDOFF,
  parseNativeHandoff,
  parseRolloutPercent,
} from "@/lib/native-handoff";

describe("native handoff switch", () => {
  it("defaults to off at 0 %", () => {
    expect(DEFAULT_NATIVE_HANDOFF).toEqual({
      enabled: false,
      rolloutPercent: 0,
    });
    expect(parseNativeHandoff(null)).toEqual(DEFAULT_NATIVE_HANDOFF);
    expect(parseNativeHandoff("on")).toEqual(DEFAULT_NATIVE_HANDOFF);
  });

  it("reads a stored value", () => {
    expect(parseNativeHandoff({ enabled: true, rolloutPercent: 25 })).toEqual({
      enabled: true,
      rolloutPercent: 25,
    });
    expect(parseNativeHandoff({ enabled: false, rolloutPercent: 100 })).toEqual(
      { enabled: false, rolloutPercent: 100 },
    );
  });

  it("fails closed on anything unreadable", () => {
    for (const value of [
      { enabled: "yes", rolloutPercent: 50 },
      { enabled: true },
      { enabled: true, rolloutPercent: 101 },
      { enabled: true, rolloutPercent: -1 },
      { enabled: true, rolloutPercent: 12.5 },
      { enabled: true, rolloutPercent: "50" },
    ]) {
      expect(parseNativeHandoff(value)).toEqual(DEFAULT_NATIVE_HANDOFF);
    }
  });

  it("takes whole percents 0–100 only", () => {
    expect(parseRolloutPercent(0)).toBe(0);
    expect(parseRolloutPercent(100)).toBe(100);
    expect(parseRolloutPercent(100.5)).toBeNull();
    expect(parseRolloutPercent(Number.NaN)).toBeNull();
    expect(parseRolloutPercent(null)).toBeNull();
  });
});
