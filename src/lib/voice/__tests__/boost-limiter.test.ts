import { describe, expect, it } from "vitest";
import {
  BOOST_CEILING_DBFS,
  BoostLimiterCore,
} from "../boost-limiter.worklet.js";

const RATE = 48_000;
const BLOCK = 128;
const CEILING = Math.pow(10, BOOST_CEILING_DBFS / 20);

/** Run `left`/`right` through a fresh core in 128-frame blocks. */
function run(left: Float32Array, right: Float32Array = left) {
  const core = new BoostLimiterCore({ sampleRate: RATE });
  const outLeft = new Float32Array(left.length);
  const outRight = new Float32Array(left.length);
  for (let start = 0; start < left.length; start += BLOCK) {
    const end = Math.min(left.length, start + BLOCK);
    core.process(
      [left.subarray(start, end), right.subarray(start, end)],
      [outLeft.subarray(start, end), outRight.subarray(start, end)],
      end - start,
    );
  }
  return { core, left: outLeft, right: outRight };
}

function peak(data: Float32Array): number {
  let max = 0;
  for (const value of data) max = Math.max(max, Math.abs(value));
  return max;
}

/** Deterministic noise at `amplitude`, times `gain`. */
function noise(seconds: number, amplitude: number, gain: number) {
  let seed = 1;
  const data = new Float32Array(Math.round(seconds * RATE));
  for (let i = 0; i < data.length; i++) {
    seed = (seed * 16807) % 2147483647;
    data[i] = ((seed / 2147483647) * 2 - 1) * amplitude * gain;
  }
  return data;
}

describe("BoostLimiterCore (per-person limiter past 100 %)", () => {
  const lookahead = Math.round(RATE * 0.0015);

  it("passes audio under the ceiling unchanged, 1.5 ms late", () => {
    const input = noise(0.2, 0.25, 2); // 0.5 peak, under -3 dBFS
    const { left, core } = run(input);
    for (let i = lookahead; i < input.length; i++) {
      expect(left[i]).toBe(input[i - lookahead]);
    }
    expect(core.gain).toBe(1);
    expect(core.clamped).toBe(0);
  });

  it("holds a -0.8 dBFS talker at 4x to the ceiling without the safety clamp", () => {
    const input = noise(1, 0.912, 4);
    const { left, core } = run(input);
    expect(peak(left)).toBeLessThanOrEqual(CEILING + 1e-6);
    expect(peak(left)).toBeGreaterThan(CEILING * 0.98);
    // The look-ahead met every peak; the clamp never had to.
    expect(core.clamped).toBe(0);
  });

  it("meets a hard onset with the gain already down", () => {
    const input = new Float32Array(RATE / 10);
    for (let i = 2400; i < input.length; i++) input[i] = i % 2 ? 3.6 : -3.6;
    const { left, core } = run(input);
    expect(peak(left)).toBeLessThanOrEqual(CEILING + 1e-6);
    expect(core.clamped).toBe(0);
  });

  it("links the channels, so a pan survives", () => {
    const loud = noise(0.5, 0.9, 4);
    const quiet = loud.map((value) => value * 0.25);
    const { left, right } = run(loud, quiet);
    for (let i = RATE / 10; i < loud.length; i += 997) {
      if (Math.abs(left[i]!) > 1e-4) {
        expect(right[i]! / left[i]!).toBeCloseTo(0.25, 5);
      }
    }
  });

  it("holds 5 ms, then releases to exactly unity", () => {
    const input = new Float32Array(RATE);
    // 50 ms at 4x a -1 dBFS sine, then 950 ms of a quiet one.
    for (let i = 0; i < input.length; i++) {
      const amplitude = i < RATE / 20 ? 0.89 * 4 : 0.1;
      input[i] = amplitude * Math.sin((2 * Math.PI * 220 * i) / RATE);
    }
    const core = new BoostLimiterCore({ sampleRate: RATE });
    const output = new Float32Array(input.length);
    const gains: number[] = [];
    for (let start = 0; start < input.length; start += BLOCK) {
      core.process(
        [input.subarray(start, start + BLOCK)],
        [output.subarray(start, start + BLOCK)],
        BLOCK,
      );
      gains.push(core.gain);
    }
    const loudEnd = Math.ceil(RATE / 20 / BLOCK);
    // Still reduced just after the loud part (hold + release under way).
    expect(gains[loudEnd + 1]!).toBeLessThan(0.5);
    // Back to unity well within a second.
    expect(gains.at(-1)).toBe(1);
    expect(peak(output)).toBeLessThanOrEqual(CEILING + 1e-6);
  });
});
