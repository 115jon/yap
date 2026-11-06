//! The call's own audio path, the parts that touch no device: RNNoise on
//! the mic ([`denoise`]), the stereo mixer the remote tracks play through
//! with their gain and pan ([`mix`]), and 16-bit WAV files for the test
//! hooks and fixtures ([`wav`]). Everything runs on 10 ms frames at 48 kHz,
//! which is WebRTC's frame and RNNoise's.

pub mod denoise;
pub mod mix;
pub mod wav;

/// The sample rate of everything here.
pub const RATE: u32 = 48_000;

/// One 10 ms frame, in samples per channel (RNNoise's `FRAME_SIZE`).
pub const FRAME: usize = 480;

/// The root mean square of 16-bit samples, on the 0..=1 scale.
pub fn rms(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .map(|s| {
            let x = f64::from(*s) / 32768.0;
            x * x
        })
        .sum();
    (sum / samples.len() as f64).sqrt()
}

/// One channel of interleaved samples.
pub fn channel(samples: &[i16], channels: usize, index: usize) -> Vec<i16> {
    samples
        .iter()
        .skip(index)
        .step_by(channels.max(1))
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_of_a_full_scale_square_should_be_one() {
        let square: Vec<i16> = (0..480)
            .map(|i| if i % 2 == 0 { 32767 } else { -32768 })
            .collect();
        assert!((rms(&square) - 1.0).abs() < 1e-3);
    }

    #[test]
    fn channel_should_take_every_other_sample() {
        assert_eq!(channel(&[1, 2, 3, 4, 5, 6], 2, 1), vec![2, 4, 6]);
    }
}
