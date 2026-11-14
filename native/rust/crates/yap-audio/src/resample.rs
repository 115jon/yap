//! Resampling to the 48 kHz mix rate with rubato's synchronous FFT
//! resampler (fixed ratio, band-limited, no audible aliasing).

use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

use crate::SAMPLE_RATE;

/// Input frames per FFT chunk.
const CHUNK_FRAMES: usize = 1024;
const CHANNELS: usize = 2;

/// Source rates worth resampling (telephony to 192 kHz studio).
pub(crate) fn valid_rate(rate: u32) -> bool {
    (MIN_RATE..=MAX_RATE).contains(&rate)
}

const MIN_RATE: u32 = 8_000;
const MAX_RATE: u32 = 192_000;

fn make(rate: u32) -> Result<Fft<f32>, String> {
    if !valid_rate(rate) {
        return Err(format!("unsupported sample rate {rate} Hz"));
    }
    Fft::<f32>::new(
        rate as usize,
        SAMPLE_RATE as usize,
        CHUNK_FRAMES,
        CHANNELS,
        FixedSync::Input,
    )
    .map_err(|error| format!("resampler {rate} Hz: {error}"))
}

/// Resamples a whole stereo interleaved clip from `rate` to 48 kHz.
pub(crate) fn resample_clip(samples: Vec<f32>, rate: u32) -> Result<Vec<f32>, String> {
    if rate == SAMPLE_RATE || samples.is_empty() {
        return Ok(samples);
    }
    let frames = samples.len() / CHANNELS;
    let input = InterleavedSlice::new(&samples[..], CHANNELS, frames)
        .map_err(|error| format!("resampler input: {error}"))?;
    let mut resampler = make(rate)?;
    let output = resampler
        .process_all(&input, frames, None)
        .map_err(|error| format!("resample: {error}"))?;
    Ok(output.take_data())
}

/// Streaming resampler: push decoded stereo frames at the source rate, get
/// 48 kHz frames out, without the FFT's start-up delay.
pub(crate) struct StreamResampler {
    inner: Option<Inner>,
}

struct Inner {
    fft: Fft<f32>,
    ratio: f64,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    delay_left: usize,
    frames_in: u64,
    frames_out: u64,
}

impl StreamResampler {
    pub(crate) fn new(rate: u32) -> Result<Self, String> {
        if rate == SAMPLE_RATE {
            return Ok(Self { inner: None });
        }
        let fft = make(rate)?;
        let scratch = vec![0.0; fft.output_frames_max() * CHANNELS];
        Ok(Self {
            inner: Some(Inner {
                delay_left: fft.output_delay(),
                ratio: f64::from(SAMPLE_RATE) / f64::from(rate),
                fft,
                pending: Vec::new(),
                scratch,
                frames_in: 0,
                frames_out: 0,
            }),
        })
    }

    /// Clears all state (after a seek).
    pub(crate) fn reset(&mut self) {
        if let Some(inner) = &mut self.inner {
            inner.fft.reset();
            inner.pending.clear();
            inner.delay_left = inner.fft.output_delay();
            inner.frames_in = 0;
            inner.frames_out = 0;
        }
    }

    /// Appends the 48 kHz frames for `input` that are ready to `out`.
    pub(crate) fn push(&mut self, input: &[f32], out: &mut Vec<f32>) -> Result<(), String> {
        let Some(inner) = &mut self.inner else {
            out.extend_from_slice(input);
            return Ok(());
        };
        let mut pending = std::mem::take(&mut inner.pending);
        pending.extend_from_slice(input);
        inner.frames_in += (input.len() / CHANNELS) as u64;
        let chunk = CHUNK_FRAMES * CHANNELS;
        let mut consumed = 0;
        let mut result = Ok(());
        while pending.len() - consumed >= chunk {
            result = inner.run(&pending[consumed..consumed + chunk], None, out);
            if result.is_err() {
                break;
            }
            consumed += chunk;
        }
        pending.drain(..consumed);
        inner.pending = pending;
        result
    }

    /// At the end of the input: emits what is left, trimmed to the exact
    /// output length.
    pub(crate) fn flush(&mut self, out: &mut Vec<f32>) -> Result<(), String> {
        let Some(inner) = &mut self.inner else {
            return Ok(());
        };
        let expected = (inner.frames_in as f64 * inner.ratio).round() as u64;
        let pending = std::mem::take(&mut inner.pending);
        let mut padded = pending.clone();
        padded.resize(CHUNK_FRAMES * CHANNELS, 0.0);
        let partial = pending.len() / CHANNELS;
        let start = out.len();
        let mut produced = Vec::new();
        inner.run(&padded, Some(partial), &mut produced)?;
        let mut guard = 0;
        while inner.frames_out < expected + inner.delay_left as u64 && guard < 8 {
            inner.run(&padded, Some(0), &mut produced)?;
            guard += 1;
        }
        out.extend_from_slice(&produced);
        // `run` counts every frame it emits; cut back to the exact length.
        let emitted = inner.frames_out;
        if emitted > expected {
            let extra = (emitted - expected) as usize * CHANNELS;
            let keep = out.len().saturating_sub(extra).max(start);
            out.truncate(keep);
            inner.frames_out = expected;
        }
        Ok(())
    }
}

impl Inner {
    fn run(
        &mut self,
        chunk: &[f32],
        partial: Option<usize>,
        out: &mut Vec<f32>,
    ) -> Result<(), String> {
        let input = InterleavedSlice::new(chunk, CHANNELS, CHUNK_FRAMES)
            .map_err(|error| format!("resampler input: {error}"))?;
        let frames_max = self.scratch.len() / CHANNELS;
        let mut output = InterleavedSlice::new_mut(&mut self.scratch[..], CHANNELS, frames_max)
            .map_err(|error| format!("resampler output: {error}"))?;
        let indexing = partial.map(|frames| Indexing::new().partial_len(frames));
        let (_, produced) = self
            .fft
            .process_into_buffer(&input, &mut output, indexing.as_ref())
            .map_err(|error| format!("resample: {error}"))?;
        let skip = self.delay_left.min(produced);
        self.delay_left -= skip;
        let frames = produced - skip;
        out.extend_from_slice(&self.scratch[skip * CHANNELS..produced * CHANNELS]);
        self.frames_out += frames as u64;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(rate: u32, hz: f32, seconds: f32) -> Vec<f32> {
        let frames = (rate as f32 * seconds) as usize;
        let mut out = Vec::with_capacity(frames * 2);
        for frame in 0..frames {
            let x = (2.0 * std::f32::consts::PI * hz * frame as f32 / rate as f32).sin() * 0.5;
            out.push(x);
            out.push(x);
        }
        out
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        let left: Vec<f32> = samples.iter().step_by(2).copied().collect();
        left.windows(2)
            .filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0))
            .count()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn clip_resampling_keeps_length_frequency_and_level() {
        let input = sine(44_100, 1000.0, 1.0);
        let output = resample_clip(input, 44_100).unwrap_or_default();
        let frames = output.len() / 2;
        assert!((47_990..=48_010).contains(&frames), "{frames} frames");
        // Skip the edges, where the filter rings in and out.
        let middle = &output[4800 * 2..43_200 * 2];
        let hz = zero_crossings(middle) as f32 / 2.0 / 0.8;
        assert!((hz - 1000.0).abs() < 5.0, "{hz} Hz");
        assert!((peak(middle) - 0.5).abs() < 0.02, "peak {}", peak(middle));
    }

    #[test]
    fn same_rate_passes_through() {
        let input = sine(48_000, 440.0, 0.1);
        assert_eq!(resample_clip(input.clone(), 48_000).ok(), Some(input));
    }

    #[test]
    fn streaming_matches_the_expected_length_and_frequency() {
        let input = sine(22_050, 500.0, 1.0);
        let mut resampler = match StreamResampler::new(22_050) {
            Ok(resampler) => resampler,
            Err(error) => panic!("{error}"),
        };
        let mut out = Vec::new();
        // Odd-sized pushes, like decoded packets.
        for piece in input.chunks(2 * 1153) {
            assert!(resampler.push(piece, &mut out).is_ok());
        }
        assert!(resampler.flush(&mut out).is_ok());
        assert_eq!(out.len() / 2, 48_000);
        let middle = &out[4800 * 2..43_200 * 2];
        let hz = zero_crossings(middle) as f32 / 2.0 / 0.8;
        assert!((hz - 500.0).abs() < 3.0, "{hz} Hz");
        assert!((peak(middle) - 0.5).abs() < 0.02);
        // No start-up delay: the first frames already carry the sine.
        assert!(peak(&out[..200]) > 0.1);
    }

    #[test]
    fn streaming_reset_starts_clean() {
        let mut resampler = match StreamResampler::new(44_100) {
            Ok(resampler) => resampler,
            Err(error) => panic!("{error}"),
        };
        let mut out = Vec::new();
        assert!(resampler.push(&sine(44_100, 440.0, 0.2), &mut out).is_ok());
        resampler.reset();
        let mut after = Vec::new();
        assert!(resampler.push(&vec![0.0; 2048 * 2], &mut after).is_ok());
        assert!(peak(&after) < 1e-4);
    }
}
