// Per-person boost limiter core + AudioWorklet processor.
//
// Plain JavaScript on purpose: this file is loaded twice.
//   1. As an ordinary ES module by audio-pipeline.ts and the unit tests,
//      which use BoostLimiterCore directly.
//   2. Untransformed, as an AudioWorklet module
//      (`new URL("./boost-limiter.worklet.js", import.meta.url)`), where it
//      registers the "rm-boost-limiter" processor. Keep it free of imports.
//
// A person turned up past 100 % plays through one of these, after their gain
// and pan and before the shared mix limiter, so a boosted voice is held at the
// ceiling instead of driving the mix limiter (which would duck everyone).
// The same design as the native client (desktop-native LIBRARIES.md §11.1,
// `dsp::mix::BoostLimiter`):
//   - stereo linked: one gain from the louder channel, so a pan survives;
//   - sample-peak detector; the need is the gain that brings a peak to the
//     ceiling;
//   - 1.5 ms look-ahead: the smallest need over the next 1.5 ms (sliding
//     minimum), then the mean of the last window of those, so the gain
//     glides down over 1.5 ms and is exactly at the need when the peak plays.
//     The web has no jitter queue to read ahead from, so the look-ahead is a
//     1.5 ms delay line on the boosted path;
//   - 5 ms hold, then a 60 ms one-pole release run on the reduction's depth,
//     so it reaches unity exactly;
//   - ceiling -3 dBFS (the mix limiter's threshold), with a safety clamp at
//     the ceiling that only acts if the look-ahead guessed wrong.

export const BOOST_LIMITER_PROCESSOR = "rm-boost-limiter";
export const BOOST_CEILING_DBFS = -3;
export const BOOST_LOOKAHEAD_MS = 1.5;
export const BOOST_HOLD_MS = 5;
export const BOOST_RELEASE_MS = 60;

export class BoostLimiterCore {
  /**
   * @param {{
   *   sampleRate: number,
   *   ceilingDbfs?: number,
   *   lookaheadMs?: number,
   *   holdMs?: number,
   *   releaseMs?: number,
   * }} options
   */
  constructor(options) {
    const rate = options.sampleRate;
    this.ceiling = Math.pow(
      10,
      (options.ceilingDbfs ?? BOOST_CEILING_DBFS) / 20,
    );
    this.lookahead = Math.max(
      1,
      Math.round((rate * (options.lookaheadMs ?? BOOST_LOOKAHEAD_MS)) / 1000),
    );
    this.holdSamples = Math.round(
      (rate * (options.holdMs ?? BOOST_HOLD_MS)) / 1000,
    );
    this.releaseCoef = Math.exp(
      -1000 / (rate * (options.releaseMs ?? BOOST_RELEASE_MS)),
    );
    const window = this.lookahead + 1;
    /** The last `window` needs (sliding minimum input). */
    this.needs = new Float64Array(window).fill(1);
    /** The last `window` sliding minima (mean input). */
    this.minima = new Float64Array(window).fill(1);
    this.minimaSum = window;
    this.cursor = 0;
    /** Per-channel delay lines, `lookahead` samples long. */
    this.delays = [];
    this.delayCursor = 0;
    this.gain = 1;
    this.holdLeft = 0;
    /** Samples the safety clamp touched (diagnostics and tests). */
    this.clamped = 0;
  }

  delayFor(channel) {
    while (this.delays.length <= channel) {
      this.delays.push(new Float32Array(this.lookahead));
    }
    return this.delays[channel];
  }

  /**
   * @param {Float32Array[]} input
   * @param {Float32Array[]} output
   * @param {number} frames
   */
  process(input, output, frames) {
    const channels = output.length;
    const window = this.lookahead + 1;
    const ceiling = this.ceiling;
    for (let i = 0; i < frames; i++) {
      // Detector: the louder channel's sample peak, as a need.
      let peak = 0;
      for (let ch = 0; ch < input.length; ch++) {
        const sample = input[ch][i];
        const level = sample < 0 ? -sample : sample;
        if (level > peak) peak = level;
      }
      const need = peak > ceiling ? ceiling / peak : 1;
      this.needs[this.cursor] = need;
      let smallest = 1;
      for (let k = 0; k < window; k++) {
        const value = this.needs[k];
        if (value < smallest) smallest = value;
      }
      this.minimaSum += smallest - this.minima[this.cursor];
      this.minima[this.cursor] = smallest;
      this.cursor = (this.cursor + 1) % window;
      // Recomputed every window so the running sum cannot drift.
      if (this.cursor === 0) {
        let sum = 0;
        for (let k = 0; k < window; k++) sum += this.minima[k];
        this.minimaSum = sum;
      }
      const target = Math.min(1, this.minimaSum / window);

      // Hold, then release on the reduction's depth.
      if (target <= this.gain) {
        this.gain = target;
        this.holdLeft = this.holdSamples;
      } else if (this.holdLeft > 0) {
        this.holdLeft--;
      } else {
        const depth = 1 - this.gain;
        const targetDepth = 1 - target;
        const next = targetDepth + (depth - targetDepth) * this.releaseCoef;
        this.gain = next - targetDepth < 1e-6 ? target : 1 - next;
      }

      const gain = this.gain;
      const slot = this.delayCursor;
      for (let ch = 0; ch < channels; ch++) {
        const line = this.delayFor(ch);
        const source = input[ch] ?? input[0];
        const incoming = source ? source[i] : 0;
        let sample = line[slot] * gain;
        line[slot] = incoming;
        if (sample > ceiling || sample < -ceiling) {
          // Rounding can leave a met peak a hair over; only a real miss counts.
          if (Math.abs(sample) > ceiling * (1 + 1e-6)) this.clamped++;
          sample = sample > 0 ? ceiling : -ceiling;
        }
        output[ch][i] = sample;
      }
      this.delayCursor = (slot + 1) % this.lookahead;
    }
  }
}

/* global registerProcessor, AudioWorkletProcessor, sampleRate */
if (
  typeof registerProcessor === "function" &&
  typeof AudioWorkletProcessor === "function"
) {
  class RmBoostLimiterProcessor extends AudioWorkletProcessor {
    constructor() {
      super();
      this.core = new BoostLimiterCore({ sampleRate });
      this.alive = true;
      this.port.onmessage = (event) => {
        if ((event.data || {}).type === "stop") this.alive = false;
      };
    }

    process(inputs, outputs) {
      if (!this.alive) return false;
      const output = outputs[0] || [];
      const frames = output.length > 0 ? output[0].length : 128;
      this.core.process(inputs[0] || [], output, frames);
      return true;
    }
  }

  registerProcessor(BOOST_LIMITER_PROCESSOR, RmBoostLimiterProcessor);
}
