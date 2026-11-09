//! The built-in soundboard tones, ported from the web's oscillator code
//! (`DEFAULT_SOUNDBOARD_SOUNDS` and the end of `playSoundboardPlayback` in
//! `src/lib/voice/soundboard.ts`).

use std::f64::consts::PI;

use crate::SAMPLE_RATE;

/// One entry of `DEFAULT_SOUNDBOARD_SOUNDS`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Tone {
    pub id: &'static str,
    pub hz: f64,
    pub duration_secs: f64,
    /// `osc.type = "triangle"` (pop); otherwise `"sine"`.
    pub triangle: bool,
}

pub(crate) const TONES: [Tone; 4] = [
    Tone {
        id: "ping",
        hz: 880.0,
        duration_secs: 0.18,
        triangle: false,
    },
    Tone {
        id: "pop",
        hz: 520.0,
        duration_secs: 0.14,
        triangle: true,
    },
    Tone {
        id: "chime",
        hz: 660.0,
        duration_secs: 0.32,
        triangle: false,
    },
    Tone {
        id: "tada",
        hz: 740.0,
        duration_secs: 0.42,
        triangle: false,
    },
];

/// The gain the web's ramp ends on.
const RAMP_END_GAIN: f64 = 0.001;

pub(crate) fn find(id: &str) -> Option<Tone> {
    TONES.iter().copied().find(|tone| tone.id == id)
}

/// Renders `tone` as 48 kHz stereo interleaved samples: the oscillator times
/// `gain.setValueAtTime(start_gain, 0)` then
/// `exponentialRampToValueAtTime(0.001, duration)`, stopped at the duration.
pub(crate) fn render(tone: Tone, start_gain: f32) -> Vec<f32> {
    let rate = f64::from(SAMPLE_RATE);
    let frames = (tone.duration_secs * rate).round() as usize;
    let start_gain = f64::from(start_gain);
    let harmonics = if tone.triangle {
        triangle_harmonics(tone.hz, rate)
    } else {
        Vec::new()
    };
    let mut out = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let t = frame as f64 / rate;
        let wave = if tone.triangle {
            harmonics
                .iter()
                .map(|&(k, amp)| amp * (2.0 * PI * tone.hz * k * t).sin())
                .sum::<f64>()
        } else {
            (2.0 * PI * tone.hz * t).sin()
        };
        // Web Audio's exponential ramp: v0 * (v1 / v0)^((t - t0) / (t1 - t0)).
        let gain = if start_gain > 0.0 {
            start_gain * (RAMP_END_GAIN / start_gain).powf(t / tone.duration_secs)
        } else {
            0.0
        };
        let sample = (wave * gain) as f32;
        out.push(sample);
        out.push(sample);
    }
    out
}

/// Web Audio builds "triangle" from its Fourier series (odd harmonics,
/// `8 / (pi^2 k^2)`, alternating sign) up to Nyquist, normalised to a peak
/// of 1. Returns `(k, amplitude)` pairs.
fn triangle_harmonics(hz: f64, rate: f64) -> Vec<(f64, f64)> {
    let nyquist = rate / 2.0;
    let mut harmonics = Vec::new();
    let mut k = 1u32;
    while f64::from(k) * hz < nyquist {
        let kf = f64::from(k);
        let sign = if (k / 2).is_multiple_of(2) { 1.0 } else { -1.0 };
        harmonics.push((kf, sign * 8.0 / (PI * PI * kf * kf)));
        k += 2;
    }
    // The series peaks at a quarter period, where every term is positive.
    let peak: f64 = harmonics.iter().map(|&(_, amp)| amp.abs()).sum();
    if peak > 0.0 {
        for (_, amp) in &mut harmonics {
            *amp /= peak;
        }
    }
    harmonics
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left(samples: &[f32]) -> Vec<f32> {
        samples.iter().step_by(2).copied().collect()
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count()
    }

    #[test]
    fn every_web_tone_is_found_by_id() {
        let ids: Vec<&str> = TONES.iter().map(|tone| tone.id).collect();
        assert_eq!(ids, ["ping", "pop", "chime", "tada"]);
        assert!(find("airhorn").is_none());
    }

    #[test]
    fn tone_length_matches_its_duration() {
        for tone in TONES {
            let samples = render(tone, 0.8);
            let expected = (tone.duration_secs * 48_000.0).round() as usize;
            assert_eq!(samples.len(), expected * 2, "{}", tone.id);
        }
    }

    #[test]
    fn tone_frequency_matches_by_zero_crossings() {
        for tone in TONES {
            let mono = left(&render(tone, 1.0));
            // Only the first half: the tail is too quiet for sign changes to be clean.
            let half = &mono[..mono.len() / 2];
            let seconds = half.len() as f64 / 48_000.0;
            let measured = zero_crossings(half) as f64 / 2.0 / seconds;
            assert!(
                (measured - tone.hz).abs() / tone.hz < 0.03,
                "{}: measured {measured} Hz",
                tone.id
            );
        }
    }

    #[test]
    fn tone_starts_at_the_volume_and_decays_to_a_thousandth() {
        let tone = find("chime").unwrap_or(TONES[0]);
        let mono = left(&render(tone, 0.5));
        let first_period = (48_000.0 / tone.hz).ceil() as usize;
        let head = mono[..first_period]
            .iter()
            .fold(0f32, |m, s| m.max(s.abs()));
        let tail = mono[mono.len() - first_period..]
            .iter()
            .fold(0f32, |m, s| m.max(s.abs()));
        assert!((head - 0.5).abs() < 0.02, "head peak {head}");
        assert!(tail > 0.0008 && tail < 0.0012, "tail peak {tail}");
    }

    #[test]
    fn pop_is_a_triangle_with_a_unit_peak() {
        let tone = find("pop").unwrap_or(TONES[1]);
        let harmonics = triangle_harmonics(tone.hz, 48_000.0);
        assert!(harmonics.len() > 10);
        let peak: f64 = (0..1000)
            .map(|i| {
                let t = f64::from(i) / 1000.0 / tone.hz;
                harmonics
                    .iter()
                    .map(|&(k, amp)| amp * (2.0 * PI * tone.hz * k * t).sin())
                    .sum::<f64>()
                    .abs()
            })
            .fold(0.0, f64::max);
        assert!((peak - 1.0).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn zero_volume_renders_silence() {
        let samples = render(TONES[0], 0.0);
        assert!(samples.iter().all(|&s| s == 0.0));
    }
}
