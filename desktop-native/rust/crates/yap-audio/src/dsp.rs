//! Small DSP pieces: linear gain ramps and the master soft clip.

/// Gain changes glide over 20 ms (the brief: 10-30 ms; the web glides the
/// stream's volume with a 30 ms time constant).
pub(crate) const GAIN_RAMP_FRAMES: u32 = 960;
/// Pause, resume and stop fade over 10 ms.
pub(crate) const FADE_FRAMES: u32 = 480;

/// A linear ramp from the current value to a target over a number of frames.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ramp {
    value: f32,
    target: f32,
    step: f32,
    left: u32,
}

impl Ramp {
    pub(crate) fn new(value: f32) -> Self {
        Self {
            value,
            target: value,
            step: 0.0,
            left: 0,
        }
    }

    /// Glides to `target` over `frames` (at once when 0).
    pub(crate) fn set(&mut self, target: f32, frames: u32) {
        self.target = target;
        if frames == 0 || self.value == target {
            self.value = target;
            self.left = 0;
            self.step = 0.0;
        } else {
            self.left = frames;
            self.step = (target - self.value) / frames as f32;
        }
    }

    /// The value for the next frame.
    pub(crate) fn next_value(&mut self) -> f32 {
        if self.left > 0 {
            self.left -= 1;
            self.value = if self.left == 0 {
                self.target
            } else {
                self.value + self.step
            };
        }
        self.value
    }

    #[cfg(test)]
    pub(crate) fn value(&self) -> f32 {
        self.value
    }

    /// Frames until the target is reached.
    pub(crate) fn frames_left(&self) -> u32 {
        self.left
    }

    pub(crate) fn is_settled_at(&self, value: f32) -> bool {
        self.left == 0 && self.value == value
    }
}

/// Where the soft clip starts bending.
const SOFT_CLIP_KNEE: f32 = 0.8;
/// What it approaches (-0.18 dBFS), never reached even in `f32`.
pub(crate) const SOFT_CLIP_CEILING: f32 = 0.98;

/// Identity up to 0.8, then a tanh shoulder that approaches 0.98, with a
/// continuous slope at the knee. Overlapping voices can
/// sum past full scale; this keeps them from wrapping or hard clipping.
pub(crate) fn soft_clip(x: f32) -> f32 {
    let magnitude = x.abs();
    if magnitude <= SOFT_CLIP_KNEE || !x.is_finite() {
        return if x.is_finite() { x } else { 0.0 };
    }
    let headroom = SOFT_CLIP_CEILING - SOFT_CLIP_KNEE;
    let shaped = SOFT_CLIP_KNEE + headroom * ((magnitude - SOFT_CLIP_KNEE) / headroom).tanh();
    shaped.copysign(x)
}

/// Appends `channels`-interleaved `input` to `out` as stereo: mono is copied
/// to both sides, channels past the first two are dropped.
pub(crate) fn push_stereo(input: &[f32], channels: usize, out: &mut Vec<f32>) {
    match channels {
        0 => {}
        1 => {
            out.reserve(input.len() * 2);
            for &sample in input {
                out.push(sample);
                out.push(sample);
            }
        }
        2 => out.extend_from_slice(input),
        _ => {
            out.reserve(input.len() / channels * 2);
            for frame in input.chunks_exact(channels) {
                out.push(frame[0]);
                out.push(frame[1]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ramp_reaches_its_target_after_the_given_frames() {
        let mut ramp = Ramp::new(0.0);
        ramp.set(1.0, 4);
        let values: Vec<f32> = (0..5).map(|_| ramp.next_value()).collect();
        assert_eq!(values, [0.25, 0.5, 0.75, 1.0, 1.0]);
        assert!(ramp.is_settled_at(1.0));
    }

    #[test]
    fn ramp_with_zero_frames_jumps() {
        let mut ramp = Ramp::new(0.3);
        ramp.set(0.7, 0);
        assert_eq!(ramp.value(), 0.7);
        assert_eq!(ramp.frames_left(), 0);
    }

    #[test]
    fn soft_clip_is_identity_below_the_knee() {
        for x in [-0.8, -0.5, 0.0, 0.25, 0.8] {
            assert_eq!(soft_clip(x), x);
        }
    }

    #[test]
    fn soft_clip_never_reaches_full_scale() {
        for x in [0.9f32, 1.0, 1.5, 4.0, 100.0] {
            let y = soft_clip(x);
            assert!(y <= SOFT_CLIP_CEILING && y > 0.8, "{x} -> {y}");
            assert_eq!(soft_clip(-x), -y);
        }
        assert_eq!(soft_clip(f32::NAN), 0.0);
    }

    #[test]
    fn soft_clip_is_monotonic() {
        let mut last = soft_clip(0.0);
        for i in 1..400 {
            let y = soft_clip(i as f32 / 100.0);
            assert!(y >= last);
            last = y;
        }
    }

    #[test]
    fn stereo_conversion_copies_mono_and_drops_extra_channels() {
        let mut out = Vec::new();
        push_stereo(&[0.1, 0.2], 1, &mut out);
        assert_eq!(out, [0.1, 0.1, 0.2, 0.2]);
        out.clear();
        push_stereo(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3, &mut out);
        assert_eq!(out, [1.0, 2.0, 4.0, 5.0]);
    }
}
