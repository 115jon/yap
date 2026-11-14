//! The Listen Together loudness chain (web: `ListenTogetherAudioProcessor`):
//! compressor -> makeup gain -> limiter. Volume is applied by the mixer.
//!
//! The static curve is Chromium's `DynamicsCompressorKernel`, which the Web
//! Audio spec describes for `DynamicsCompressorNode`; see the crate docs for
//! what is approximated.

use crate::dsp::{Ramp, GAIN_RAMP_FRAMES};
use crate::{Loudness, LoudnessPreset, SAMPLE_RATE};

/// `DynamicsCompressorNode` parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CompressorParams {
    pub threshold_db: f64,
    pub knee_db: f64,
    pub ratio: f64,
    pub attack_secs: f64,
    pub release_secs: f64,
}

impl CompressorParams {
    /// What the web sets when loudness is off: threshold 0, knee 0, ratio 1.
    const IDENTITY: Self = Self {
        threshold_db: 0.0,
        knee_db: 0.0,
        ratio: 1.0,
        attack_secs: 0.003,
        release_secs: 0.25,
    };

    /// The web's limiter node: knee 0, ratio 20, attack 1 ms, release 100 ms.
    fn limiter(ceiling_db: f64) -> Self {
        Self {
            threshold_db: ceiling_db,
            knee_db: 0.0,
            ratio: 20.0,
            attack_secs: 0.001,
            release_secs: 0.1,
        }
    }
}

/// One entry of `LISTEN_TOGETHER_LOUDNESS_PRESETS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PresetConfig {
    pub compressor: CompressorParams,
    pub makeup_db: f64,
    pub limiter_ceiling_db: f64,
}

pub(crate) fn preset_config(preset: LoudnessPreset) -> PresetConfig {
    match preset {
        LoudnessPreset::PeakProtection => PresetConfig {
            compressor: CompressorParams {
                threshold_db: 0.0,
                knee_db: 0.0,
                ratio: 1.0,
                attack_secs: 0.003,
                release_secs: 0.1,
            },
            makeup_db: 0.0,
            limiter_ceiling_db: -1.0,
        },
        LoudnessPreset::Balanced => PresetConfig {
            compressor: CompressorParams {
                threshold_db: -18.0,
                knee_db: 9.0,
                ratio: 2.5,
                attack_secs: 0.01,
                release_secs: 0.25,
            },
            makeup_db: 2.0,
            limiter_ceiling_db: -1.0,
        },
        LoudnessPreset::Night => PresetConfig {
            compressor: CompressorParams {
                threshold_db: -30.0,
                knee_db: 12.0,
                ratio: 6.0,
                attack_secs: 0.005,
                release_secs: 0.35,
            },
            makeup_db: 6.0,
            limiter_ceiling_db: -2.0,
        },
    }
}

fn db_to_linear(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

fn linear_to_db(linear: f64) -> f64 {
    if linear <= 0.0 {
        -1000.0
    } else {
        20.0 * linear.log10()
    }
}

/// The compressor's static input -> output curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StaticCurve {
    linear_threshold: f64,
    knee_threshold: f64,
    knee_threshold_db: f64,
    yknee_threshold_db: f64,
    slope: f64,
    k: f64,
    /// `(1 / saturate(1.0))^0.6`, the node's automatic makeup.
    makeup: f64,
}

impl StaticCurve {
    pub(crate) fn new(params: &CompressorParams) -> Self {
        let ratio = params.ratio.max(1.0);
        let knee_db = params.knee_db.max(0.0);
        let mut curve = Self {
            linear_threshold: db_to_linear(params.threshold_db),
            knee_threshold: 0.0,
            knee_threshold_db: params.threshold_db + knee_db,
            yknee_threshold_db: 0.0,
            slope: 1.0 / ratio,
            k: 5.0,
            makeup: 1.0,
        };
        curve.k = curve.k_at_slope(params.threshold_db + knee_db, 1.0 / ratio);
        curve.knee_threshold = db_to_linear(curve.knee_threshold_db);
        curve.yknee_threshold_db = linear_to_db(curve.knee_curve(curve.knee_threshold, curve.k));
        let full_range_gain = curve.saturate(1.0);
        curve.makeup = if full_range_gain > 0.0 {
            (1.0 / full_range_gain).powf(0.6)
        } else {
            1.0
        };
        curve
    }

    /// Linear up to the threshold, then an exponential approach whose slope
    /// falls towards `1/ratio`.
    fn knee_curve(&self, x: f64, k: f64) -> f64 {
        if x < self.linear_threshold {
            return x;
        }
        self.linear_threshold + (1.0 - (-k * (x - self.linear_threshold)).exp()) / k
    }

    /// The full curve: the knee, then a constant ratio.
    fn saturate(&self, x: f64) -> f64 {
        if x < self.knee_threshold {
            self.knee_curve(x, self.k)
        } else {
            let x_db = linear_to_db(x);
            db_to_linear(self.yknee_threshold_db + self.slope * (x_db - self.knee_threshold_db))
        }
    }

    fn slope_at(&self, x: f64, k: f64) -> f64 {
        if x < self.linear_threshold {
            return 1.0;
        }
        let x2 = x * 1.001;
        let x_db = linear_to_db(x);
        let x2_db = linear_to_db(x2);
        let y_db = linear_to_db(self.knee_curve(x, k));
        let y2_db = linear_to_db(self.knee_curve(x2, k));
        (y2_db - y_db) / (x2_db - x_db)
    }

    /// Chromium's `KAtSlope`: the knee's `k` whose slope at the end of the
    /// knee is `desired_slope`, by a 15-step geometric bisection.
    fn k_at_slope(&self, knee_end_db: f64, desired_slope: f64) -> f64 {
        let x = db_to_linear(knee_end_db);
        let (mut min_k, mut max_k, mut k) = (0.1f64, 10_000f64, 5f64);
        for _ in 0..15 {
            if self.slope_at(x, k) < desired_slope {
                max_k = k;
            } else {
                min_k = k;
            }
            k = (min_k * max_k).sqrt();
        }
        k
    }

    /// The gain (makeup excluded) for a detector level of `x`.
    pub(crate) fn gain(&self, x: f64) -> f64 {
        if x <= self.linear_threshold || x <= 0.0 {
            1.0
        } else {
            self.saturate(x) / x
        }
    }

    pub(crate) fn makeup(&self) -> f64 {
        self.makeup
    }

    /// Steady-state output level in dB for an input level in dB.
    #[cfg(test)]
    pub(crate) fn output_db(&self, input_db: f64) -> f64 {
        let x = db_to_linear(input_db);
        linear_to_db(x * self.gain(x) * self.makeup)
    }
}

fn one_pole(secs: f64) -> f32 {
    let frames = secs.max(0.000_1) * f64::from(SAMPLE_RATE);
    (-1.0 / frames).exp() as f32
}

/// A feed-forward compressor on a stereo pair (linked by the peak of both).
#[derive(Clone, Debug)]
pub(crate) struct Compressor {
    curve: StaticCurve,
    makeup: f32,
    attack: f32,
    release: f32,
    /// Peak detector: jumps to peaks, decays with the release time.
    envelope: f32,
    /// The smoothed gain reduction (1 = none).
    gain: f32,
}

impl Compressor {
    pub(crate) fn new(params: &CompressorParams) -> Self {
        let mut compressor = Self {
            curve: StaticCurve::new(params),
            makeup: 1.0,
            attack: 0.0,
            release: 0.0,
            envelope: 0.0,
            gain: 1.0,
        };
        compressor.set(params);
        compressor
    }

    /// New parameters; the envelope carries on, so there is no jump.
    pub(crate) fn set(&mut self, params: &CompressorParams) {
        self.curve = StaticCurve::new(params);
        self.makeup = self.curve.makeup() as f32;
        self.attack = one_pole(params.attack_secs);
        self.release = one_pole(params.release_secs);
    }

    pub(crate) fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        // The detector holds peaks and lets go at the release rate; the
        // gain follows it down at the attack rate and back up at once (the
        // release already shaped the detector).
        let level = left.abs().max(right.abs());
        self.envelope = if level >= self.envelope {
            level
        } else {
            level + self.release * (self.envelope - level)
        };
        let target = self.curve.gain(f64::from(self.envelope)) as f32;
        self.gain = if target < self.gain {
            target + self.attack * (self.gain - target)
        } else {
            target
        };
        let gain = self.gain * self.makeup;
        (left * gain, right * gain)
    }
}

/// compressor -> makeup gain -> limiter, as `ListenTogetherAudioProcessor`
/// wires it. Off, every stage is set to identity, as `applySettings` does.
#[derive(Clone, Debug)]
pub(crate) struct LoudnessChain {
    compressor: Compressor,
    makeup: Ramp,
    limiter: Compressor,
}

impl LoudnessChain {
    pub(crate) fn new(loudness: Loudness) -> Self {
        let mut chain = Self {
            compressor: Compressor::new(&CompressorParams::IDENTITY),
            makeup: Ramp::new(1.0),
            limiter: Compressor::new(&CompressorParams::IDENTITY),
        };
        chain.set(loudness, 0);
        chain
    }

    pub(crate) fn set(&mut self, loudness: Loudness, ramp_frames: u32) {
        if loudness.enabled {
            let config = preset_config(loudness.preset);
            self.compressor.set(&config.compressor);
            self.makeup
                .set(db_to_linear(config.makeup_db) as f32, ramp_frames);
            self.limiter
                .set(&CompressorParams::limiter(config.limiter_ceiling_db));
        } else {
            self.compressor.set(&CompressorParams::IDENTITY);
            self.makeup.set(1.0, ramp_frames);
            self.limiter.set(&CompressorParams::IDENTITY);
        }
    }

    pub(crate) fn update(&mut self, loudness: Loudness) {
        self.set(loudness, GAIN_RAMP_FRAMES);
    }

    pub(crate) fn process(&mut self, left: f32, right: f32) -> (f32, f32) {
        let (left, right) = self.compressor.process(left, right);
        let makeup = self.makeup.next_value();
        self.limiter.process(left * makeup, right * makeup)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve(preset: LoudnessPreset) -> StaticCurve {
        StaticCurve::new(&preset_config(preset).compressor)
    }

    fn close(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    #[test]
    fn presets_parse_from_the_web_keys() {
        for preset in [
            LoudnessPreset::PeakProtection,
            LoudnessPreset::Balanced,
            LoudnessPreset::Night,
        ] {
            assert_eq!(LoudnessPreset::parse(preset.as_str()), Some(preset));
        }
        assert_eq!(LoudnessPreset::parse("loud"), None);
    }

    #[test]
    fn peak_protection_compressor_is_identity() {
        let curve = curve(LoudnessPreset::PeakProtection);
        for input in [-40.0, -12.0, -1.0, 0.0, 6.0] {
            assert!(close(curve.output_db(input), input, 1e-6), "{input}");
        }
    }

    #[test]
    fn below_threshold_only_the_automatic_makeup_applies() {
        for preset in [LoudnessPreset::Balanced, LoudnessPreset::Night] {
            let curve = curve(preset);
            let makeup_db = linear_to_db(curve.makeup());
            let input = preset_config(preset).compressor.threshold_db - 10.0;
            assert!(
                close(curve.output_db(input), input + makeup_db, 1e-9),
                "{preset:?}"
            );
            assert!(makeup_db > 0.0);
        }
    }

    #[test]
    fn above_the_knee_the_slope_is_one_over_ratio() {
        for preset in [LoudnessPreset::Balanced, LoudnessPreset::Night] {
            let params = preset_config(preset).compressor;
            let curve = curve(preset);
            let a = params.threshold_db + params.knee_db + 3.0;
            let b = a + 6.0;
            let slope = (curve.output_db(b) - curve.output_db(a)) / 6.0;
            assert!(
                close(slope, 1.0 / params.ratio, 1e-6),
                "{preset:?}: {slope}"
            );
        }
    }

    #[test]
    fn the_knee_is_soft_and_monotonic() {
        let params = preset_config(LoudnessPreset::Balanced).compressor;
        let curve = curve(LoudnessPreset::Balanced);
        let mut last = curve.output_db(params.threshold_db - 1.0);
        let mut input = params.threshold_db - 1.0;
        while input < params.threshold_db + params.knee_db + 1.0 {
            input += 0.25;
            let output = curve.output_db(input);
            let slope = (output - last) / 0.25;
            assert!(
                slope > 1.0 / params.ratio - 0.05 && slope <= 1.0 + 1e-6,
                "{input}: {slope}"
            );
            last = output;
        }
    }

    #[test]
    fn static_curve_points_per_preset() {
        // Full-scale input: saturate(1) * makeup = saturate(1)^0.4 (makeup is
        // (1/saturate(1))^0.6), so 0 dB in comes out at 0.4 x saturate(1) dB.
        for preset in [
            LoudnessPreset::PeakProtection,
            LoudnessPreset::Balanced,
            LoudnessPreset::Night,
        ] {
            let curve = curve(preset);
            let saturated_db = linear_to_db(curve.saturate(1.0));
            assert!(close(curve.output_db(0.0), 0.4 * saturated_db, 1e-9));
        }
        // Night (threshold -30, knee 12, ratio 6) squashes harder than balanced.
        let balanced = curve(LoudnessPreset::Balanced);
        let night = curve(LoudnessPreset::Night);
        let spread = |curve: &StaticCurve| curve.output_db(0.0) - curve.output_db(-30.0);
        assert!(spread(&night) < spread(&balanced));
        assert!(spread(&balanced) < 30.0);
    }

    #[test]
    fn limiter_curve_matches_ratio_twenty_with_makeup() {
        let curve = StaticCurve::new(&CompressorParams::limiter(-1.0));
        // saturate(1) = -1 + (0 - -1) / 20 = -0.95 dB; makeup = 0.95 x 0.6 dB.
        assert!(close(linear_to_db(curve.makeup()), 0.57, 1e-6));
        assert!(close(curve.output_db(0.0), -0.95 + 0.57, 1e-6));
        assert!(close(curve.output_db(-6.0), -6.0 + 0.57, 1e-6));
    }

    fn steady_peak(process: &mut dyn FnMut(f32) -> f32, amplitude: f32) -> f32 {
        let mut peak = 0f32;
        for frame in 0..48_000 {
            let x =
                amplitude * (2.0 * std::f32::consts::PI * 440.0 * frame as f32 / 48_000.0).sin();
            let y = process(x);
            if frame > 24_000 {
                peak = peak.max(y.abs());
            }
        }
        peak
    }

    #[test]
    fn limiter_holds_a_loud_sine_at_its_ceiling() {
        let mut limiter = Compressor::new(&CompressorParams::limiter(-1.0));
        // +6 dB in: -1 + (6 - -1) / 20 + 0.57 makeup = -0.08 dB out.
        let peak = steady_peak(&mut |x| limiter.process(x, x).0, 2.0);
        let peak_db = linear_to_db(f64::from(peak));
        assert!(close(peak_db, -0.08, 0.3), "{peak_db} dB");
    }

    #[test]
    fn loudness_chain_keeps_a_full_scale_sine_under_full_scale() {
        for preset in [
            LoudnessPreset::PeakProtection,
            LoudnessPreset::Balanced,
            LoudnessPreset::Night,
        ] {
            let mut chain = LoudnessChain::new(Loudness {
                enabled: true,
                preset,
            });
            let peak = steady_peak(&mut |x| chain.process(x, x).0, 1.0);
            assert!(peak < 1.0 && peak > 0.5, "{preset:?}: {peak}");
        }
    }

    #[test]
    fn disabled_chain_is_unity() {
        let mut chain = LoudnessChain::new(Loudness {
            enabled: false,
            preset: LoudnessPreset::Night,
        });
        for x in [0.0f32, 0.1, -0.5, 0.99, 1.0] {
            let (left, right) = chain.process(x, -x);
            assert!((left - x).abs() < 1e-6 && (right + x).abs() < 1e-6);
        }
    }
}
