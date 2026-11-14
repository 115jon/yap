//! Share audio arithmetic: padding silence so the track keeps ticking, and
//! the synthetic sine of `YAP_FAKE_SHARE_AUDIO=1`.

use std::time::Duration;

use yap_voice::{SHARE_AUDIO_CHANNELS, SHARE_AUDIO_RATE};

/// Sample frames (one sample per channel) per second.
const RATE: u64 = SHARE_AUDIO_RATE as u64;
/// 20 ms of sample frames: how close to real time padding catches up.
const PAD_STEP: u64 = RATE / 50;
/// How far behind real time the stream may fall before padding starts:
/// 100 ms.
const PAD_SLACK: u64 = RATE / 10;

/// How many silent sample frames to push so the track's clock keeps pace
/// with wall-clock time. Loopback delivers *nothing* while nothing renders,
/// so without padding the track would freeze during silence and the next
/// real audio would land early on the receiver's clock. Padding starts only
/// once the stream is more than 100 ms behind and catches up to 20 ms short
/// of real time, so a device that delivers normally (even in bursts) is
/// never padded.
pub(crate) fn silence_frames_to_pad(elapsed: Duration, frames_emitted: u64) -> u64 {
    let expected =
        u64::try_from(elapsed.as_nanos() * u128::from(RATE) / 1_000_000_000).unwrap_or(u64::MAX);
    if expected > frames_emitted.saturating_add(PAD_SLACK) {
        expected - frames_emitted - PAD_STEP
    } else {
        0
    }
}

/// The synthetic tone's frequency.
const SINE_HZ: f64 = 440.0;
/// About -12 dBFS: 10^(-12/20) of full scale.
const SINE_PEAK: f64 = 0.251_188_643_150_958 * 32_767.0;

/// Appends `frames` interleaved stereo sample frames of the 440 Hz tone to
/// `out`, starting at sample frame `first` of the tone (so consecutive
/// chunks join without a click).
pub(crate) fn push_sine(out: &mut Vec<i16>, first: u64, frames: usize) {
    let channels = SHARE_AUDIO_CHANNELS as usize;
    out.reserve(frames * channels);
    for n in first..first + frames as u64 {
        // The phase from `n mod rate` keeps f64 exact over a long share.
        let cycle = (n % RATE) as f64 / RATE as f64;
        let sample = (cycle * SINE_HZ * std::f64::consts::TAU).sin() * SINE_PEAK;
        let sample = sample.round() as i16;
        out.extend(std::iter::repeat_n(sample, channels));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_padding_while_the_device_keeps_up() {
        assert_eq!(silence_frames_to_pad(Duration::ZERO, 0), 0);
        assert_eq!(silence_frames_to_pad(Duration::from_secs(1), 48_000), 0);
    }

    #[test]
    fn bursty_delivery_inside_the_slack_is_not_padded() {
        // 80 ms behind.
        assert_eq!(silence_frames_to_pad(Duration::from_secs(1), 44_160), 0);
    }

    #[test]
    fn pads_to_20_ms_short_of_real_time_after_silence() {
        // 1 s with nothing emitted: 48000 due, pad all but 960.
        assert_eq!(silence_frames_to_pad(Duration::from_secs(1), 0), 47_040);
    }

    #[test]
    fn once_caught_up_nothing_more_is_padded() {
        assert_eq!(silence_frames_to_pad(Duration::from_secs(1), 47_040), 0);
    }

    #[test]
    fn a_device_running_ahead_is_never_padded() {
        assert_eq!(silence_frames_to_pad(Duration::from_millis(100), 40_000), 0);
    }

    #[test]
    fn the_sine_is_stereo() {
        let mut out = Vec::new();
        push_sine(&mut out, 0, 480);
        assert_eq!(out.len(), 960);
    }

    #[test]
    fn both_channels_carry_the_same_sample() {
        let mut out = Vec::new();
        push_sine(&mut out, 0, 480);
        assert!(out.chunks_exact(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn the_sine_peaks_near_minus_12_dbfs() {
        let mut out = Vec::new();
        push_sine(&mut out, 0, 48_000);
        let peak = out.iter().map(|s| i32::from(*s).abs()).max().unwrap_or(0);
        assert!((8_200..=8_260).contains(&peak), "peak {peak}");
    }

    #[test]
    fn the_sine_starts_at_zero() {
        let mut out = Vec::new();
        push_sine(&mut out, 0, 4);
        assert_eq!(out[0], 0);
    }

    #[test]
    fn the_sine_repeats_every_second() {
        let (mut a, mut b) = (Vec::new(), Vec::new());
        push_sine(&mut a, 0, 100);
        push_sine(&mut b, 48_000, 100);
        assert_eq!(a, b);
    }

    #[test]
    fn chunks_join_into_the_same_wave() {
        let (mut whole, mut parts) = (Vec::new(), Vec::new());
        push_sine(&mut whole, 0, 960);
        push_sine(&mut parts, 0, 480);
        push_sine(&mut parts, 480, 480);
        assert_eq!(whole, parts);
    }

    #[test]
    fn the_sine_completes_440_cycles_a_second() {
        let mut out = Vec::new();
        push_sine(&mut out, 0, 48_000);
        let crossings = out
            .chunks_exact(2)
            .map(|pair| pair[0])
            .collect::<Vec<_>>()
            .windows(2)
            .filter(|w| w[0] < 0 && w[1] >= 0)
            .count();
        assert!((439..=441).contains(&crossings), "{crossings} crossings");
    }
}
