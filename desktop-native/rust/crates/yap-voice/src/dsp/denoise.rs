//! RNNoise on the mic, as the web's `RnnoiseWorkletNode` runs it
//! (`@sapphi-red/web-noise-suppressor`): Xiph's recurrent network on 10 ms
//! mono frames at 48 kHz, samples on the 16-bit scale. The port is
//! `nnnoiseless`, with RNNoise's original model.
//!
//! It runs after WebRTC's processing (echo cancellation and gain control),
//! where the web's sits after the browser's; WebRTC's own noise suppression
//! is off while it runs (`captureProcessingFor`), so the voice is not
//! suppressed twice.

use nnnoiseless::DenoiseState;

use super::FRAME;

const _: () = assert!(DenoiseState::FRAME_SIZE == FRAME);

/// One mic's RNNoise state. It carries the network's memory from frame to
/// frame, so one stream keeps one.
pub struct Denoiser {
    state: Box<DenoiseState<'static>>,
    input: [f32; FRAME],
    output: [f32; FRAME],
}

impl Default for Denoiser {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for Denoiser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Denoiser")
    }
}

impl Denoiser {
    pub fn new() -> Self {
        Self {
            state: DenoiseState::new(),
            input: [0.0; FRAME],
            output: [0.0; FRAME],
        }
    }

    /// Denoises one 10 ms mono frame in place and returns RNNoise's voice
    /// probability (0..=1). A frame of any other length is left as it is.
    pub fn process(&mut self, frame: &mut [i16]) -> f32 {
        if frame.len() != FRAME {
            return 0.0;
        }
        for (x, s) in self.input.iter_mut().zip(frame.iter()) {
            *x = f32::from(*s);
        }
        let voice = self.state.process_frame(&mut self.output, &self.input);
        for (s, y) in frame.iter_mut().zip(self.output.iter()) {
            // In range after the clamp, so the cast cannot wrap.
            *s = y.round().clamp(-32768.0, 32767.0) as i16;
        }
        voice
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::dsp::{wav, RATE};

    /// A deterministic white noise source (xorshift), uniform in -1..1.
    pub(crate) struct Noise(u64);

    impl Noise {
        pub(crate) fn new(seed: u64) -> Self {
            Self(seed.max(1))
        }

        pub(crate) fn next(&mut self) -> f64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 11) as f64 / (1u64 << 52) as f64 - 1.0
        }
    }

    /// The power of `samples` at `hz` (Goertzel), per sample.
    pub(crate) fn power_at(samples: &[i16], hz: f64) -> f64 {
        let w = 2.0 * std::f64::consts::PI * hz / f64::from(RATE);
        let coeff = 2.0 * w.cos();
        let (mut s1, mut s2) = (0.0, 0.0);
        for x in samples {
            let s = f64::from(*x) / 32768.0 + coeff * s1 - s2;
            s2 = s1;
            s1 = s;
        }
        (s1 * s1 + s2 * s2 - coeff * s1 * s2) / (samples.len() as f64).powi(2)
    }

    /// The mean power over bands above the tone's harmonics: the noise
    /// floor.
    fn floor(samples: &[i16]) -> f64 {
        let bands = [3700.0, 5100.0, 7300.0, 9900.0, 13100.0, 17300.0];
        bands.iter().map(|hz| power_at(samples, *hz)).sum::<f64>() / bands.len() as f64
    }

    fn db(ratio: f64) -> f64 {
        10.0 * ratio.log10()
    }

    /// The fixture's fundamental.
    const PITCH_HZ: f64 = 140.0;

    /// Four seconds of `tone(t)` (-1..1) under white noise at -26 dBFS RMS,
    /// as a WAV file.
    fn noisy_fixture(tone: impl Fn(f64) -> f64) -> Vec<u8> {
        let mut noise = Noise::new(0x5eed);
        let samples: Vec<i16> = (0..RATE as usize * 4)
            .map(|n| {
                let t = n as f64 / f64::from(RATE);
                let hiss = 0.087 * noise.next();
                ((tone(t) + hiss) * 32767.0).round() as i16
            })
            .collect();
        wav::encode(RATE, 1, &samples)
    }

    /// A voiced tone: a 140 Hz fundamental with its harmonics to 2.8 kHz
    /// (falling 1/k), in syllables of four a second. RNNoise keeps what
    /// sounds like a voice and removes steady sounds, so this, not a plain
    /// sine, is the tone it should keep.
    fn voiced(t: f64) -> f64 {
        let syllable = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * 4.0 * t).cos();
        let wave: f64 = (1..=20)
            .map(|k| {
                (2.0 * std::f64::consts::PI * PITCH_HZ * f64::from(k) * t).sin() / f64::from(k)
            })
            .sum();
        0.18 * syllable * wave
    }

    fn sine(t: f64) -> f64 {
        0.25 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()
    }

    /// Writes the fixture, reads it back and denoises it frame by frame;
    /// input and output after the first second (the network's warm-up).
    fn run_fixture(tone: impl Fn(f64) -> f64) -> (Vec<i16>, Vec<i16>) {
        let path = std::env::temp_dir().join(format!(
            "yap-noisy-{}-{:?}.wav",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::write(&path, noisy_fixture(tone)).expect("write fixture");
        let input = wav::read(&path).expect("read fixture");
        let _ = std::fs::remove_file(&path);
        assert_eq!((input.rate, input.channels), (RATE, 1));
        let mut output = input.samples.clone();
        let mut denoiser = Denoiser::new();
        for frame in output.chunks_exact_mut(FRAME) {
            denoiser.process(frame);
        }
        let skip = RATE as usize;
        (input.samples[skip..].to_vec(), output[skip..].to_vec())
    }

    /// The voiced tone's power at its first three harmonics.
    fn harmonics(samples: &[i16]) -> f64 {
        (1..=3)
            .map(|k| power_at(samples, PITCH_HZ * f64::from(k)))
            .sum()
    }

    #[test]
    fn rnnoise_should_lower_the_noise_floor_of_a_noisy_tone() {
        let (input, output) = run_fixture(voiced);
        let drop = db(floor(&output) / floor(&input));
        eprintln!("noise floor change: {drop:.1} dB");
        assert!(drop < -10.0, "noise floor only changed by {drop:.1} dB");
    }

    #[test]
    fn rnnoise_should_keep_the_tone() {
        let (input, output) = run_fixture(voiced);
        let kept = db(harmonics(&output) / harmonics(&input));
        eprintln!("tone change: {kept:.1} dB");
        assert!(kept > -6.0, "the tone dropped by {kept:.1} dB");
    }

    /// What the model does to a plain sine (the web's RNNoise does the
    /// same): a steady tone is noise to it.
    #[test]
    fn a_steady_sine_should_count_as_noise() {
        let (input, output) = run_fixture(sine);
        let kept = db(power_at(&output, 440.0) / power_at(&input, 440.0));
        eprintln!("sine change: {kept:.1} dB");
        assert!(kept < -20.0, "the sine only dropped by {kept:.1} dB");
    }

    #[test]
    fn a_short_frame_should_be_left_alone() {
        let mut frame = vec![100i16; FRAME - 1];
        Denoiser::new().process(&mut frame);
        assert!(frame.iter().all(|s| *s == 100));
    }

    /// The CPU cost per 10 ms frame (printed; `--release --nocapture`).
    #[test]
    fn rnnoise_cost_per_frame_should_be_well_under_its_ten_milliseconds() {
        let (input, _) = run_fixture(voiced);
        let mut denoiser = Denoiser::new();
        let mut frames = input.clone();
        let started = std::time::Instant::now();
        let mut count = 0u32;
        for frame in frames.chunks_exact_mut(FRAME) {
            denoiser.process(frame);
            count += 1;
        }
        let per_frame = started.elapsed() / count;
        eprintln!("rnnoise: {per_frame:?} per 10 ms frame ({count} frames)");
        // Even a debug build stays far below real time.
        assert!(per_frame < std::time::Duration::from_millis(5));
    }
}
