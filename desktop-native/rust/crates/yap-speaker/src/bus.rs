//! The mix bus: every input summed into one 10 ms stereo frame, bent under
//! full scale, as 16-bit PCM. Pure; the render thread drives it.

use std::sync::{Arc, Mutex, PoisonError, Weak};

use crate::SAMPLES;

/// A source of sound on the speaker. Called on the render thread every
/// 10 ms; it must not block for long.
pub trait Input: Send + Sync {
    /// Adds the next 10 ms (48 kHz interleaved stereo, -1..=1) to `out`.
    /// Returns `true` while it has something to play, which keeps the
    /// device open.
    fn mix_into(&self, out: &mut [f32; SAMPLES]) -> bool;
}

/// Where the final soft clip starts bending: the call mix's own limiter
/// ceiling (-0.2 dBFS), so a single input passes through untouched (the
/// call's limiter and the player's soft clip both stay under it).
pub const CLIP_KNEE: f32 = 0.977_237;

/// What the final soft clip approaches, never reaches.
pub const CLIP_CEILING: f32 = 0.999;

/// Identity up to [`CLIP_KNEE`], then a tanh shoulder towards
/// [`CLIP_CEILING`] with a continuous slope: inputs that sum past full
/// scale (a soundboard clip over a loud call) never wrap or square off.
pub fn soft_clip(x: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let a = x.abs();
    if a <= CLIP_KNEE {
        return x;
    }
    let room = CLIP_CEILING - CLIP_KNEE;
    (CLIP_KNEE + room * ((a - CLIP_KNEE) / room).tanh()).copysign(x)
}

/// The inputs, held weakly: an input leaves the mix when its owner drops
/// the last `Arc`.
#[derive(Default)]
pub struct Bus {
    inputs: Mutex<Vec<Weak<dyn Input>>>,
}

impl std::fmt::Debug for Bus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bus").field("inputs", &self.len()).finish()
    }
}

impl Bus {
    pub fn new() -> Self {
        Self::default()
    }

    fn inputs(&self) -> std::sync::MutexGuard<'_, Vec<Weak<dyn Input>>> {
        self.inputs.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds `input` (once; adding it again does nothing).
    pub fn add(&self, input: &Arc<dyn Input>) {
        let mut inputs = self.inputs();
        let weak = Arc::downgrade(input);
        if !inputs.iter().any(|w| w.ptr_eq(&weak)) {
            inputs.push(weak);
        }
    }

    /// Takes `input` out of the mix now (rather than when it is dropped).
    pub fn remove(&self, input: &Arc<dyn Input>) {
        let weak = Arc::downgrade(input);
        self.inputs()
            .retain(|w| !w.ptr_eq(&weak) && w.strong_count() > 0);
    }

    /// Inputs still alive.
    pub fn len(&self) -> usize {
        let mut inputs = self.inputs();
        inputs.retain(|w| w.strong_count() > 0);
        inputs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The next 10 ms of every input into `out`; `true` if any of them has
    /// something to play.
    pub fn render(&self, out: &mut [i16; SAMPLES]) -> bool {
        let mut sum = [0f32; SAMPLES];
        let mut active = false;
        {
            // Mixed under the list's lock, so a frame allocates nothing; an
            // input being added waits one frame's mix at most.
            let mut inputs = self.inputs();
            inputs.retain(|w| w.strong_count() > 0);
            for input in inputs.iter().filter_map(Weak::upgrade) {
                active |= input.mix_into(&mut sum);
            }
        }
        for (o, s) in out.iter_mut().zip(sum.iter()) {
            // In range: the clip stays under full scale.
            *o = (soft_clip(*s) * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// A constant on both channels.
    struct Dc {
        value: f32,
        active: AtomicBool,
    }

    impl Dc {
        fn input(value: f32) -> Arc<dyn Input> {
            Arc::new(Self {
                value,
                active: AtomicBool::new(true),
            })
        }
    }

    impl Input for Dc {
        fn mix_into(&self, out: &mut [f32; SAMPLES]) -> bool {
            for s in out.iter_mut() {
                *s += self.value;
            }
            self.active.load(Ordering::Relaxed)
        }
    }

    fn render(bus: &Bus) -> ([i16; SAMPLES], bool) {
        let mut out = [0i16; SAMPLES];
        let active = bus.render(&mut out);
        (out, active)
    }

    #[test]
    fn an_empty_bus_should_play_silence_and_be_idle() {
        let (out, active) = render(&Bus::new());
        assert!(out.iter().all(|s| *s == 0) && !active);
    }

    #[test]
    fn inputs_should_sum() {
        let bus = Bus::new();
        let (a, b) = (Dc::input(0.25), Dc::input(0.125));
        bus.add(&a);
        bus.add(&b);
        let (out, active) = render(&bus);
        assert!(active);
        assert_eq!(out[0], (0.375f32 * 32768.0) as i16);
        assert_eq!(out[SAMPLES - 1], out[0]);
    }

    #[test]
    fn a_single_input_under_the_knee_should_pass_unchanged() {
        let bus = Bus::new();
        let a = Dc::input(0.95);
        bus.add(&a);
        let (out, _) = render(&bus);
        assert_eq!(out[0], (0.95f32 * 32768.0).round() as i16);
    }

    #[test]
    fn inputs_past_full_scale_should_bend_under_it() {
        let bus = Bus::new();
        let (a, b) = (Dc::input(0.9), Dc::input(0.9));
        bus.add(&a);
        bus.add(&b);
        let (out, _) = render(&bus);
        let peak = f32::from(out[0]) / 32768.0;
        assert!(peak > CLIP_KNEE && peak < CLIP_CEILING, "{peak}");
        assert!((soft_clip(-1.8) + soft_clip(1.8)).abs() < 1e-6);
    }

    #[test]
    fn the_soft_clip_should_be_continuous_at_the_knee() {
        let below = soft_clip(CLIP_KNEE - 1e-4);
        let above = soft_clip(CLIP_KNEE + 1e-4);
        assert!((above - below - 2e-4).abs() < 1e-5, "{below} {above}");
        assert_eq!(soft_clip(f32::NAN), 0.0);
    }

    #[test]
    fn a_dropped_input_should_leave_the_mix() {
        let bus = Bus::new();
        let (a, b) = (Dc::input(0.25), Dc::input(0.25));
        bus.add(&a);
        bus.add(&b);
        drop(b);
        let (out, _) = render(&bus);
        assert_eq!(out[0], 8192);
        assert_eq!(bus.len(), 1);
    }

    #[test]
    fn a_removed_input_should_leave_at_once() {
        let bus = Bus::new();
        let a = Dc::input(0.25);
        bus.add(&a);
        bus.add(&a);
        assert_eq!(bus.len(), 1, "added twice, listed once");
        bus.remove(&a);
        assert!(bus.is_empty());
        assert!(render(&bus).0.iter().all(|s| *s == 0));
    }

    #[test]
    fn the_bus_should_be_active_while_any_input_is() {
        let bus = Bus::new();
        let quiet = Arc::new(Dc {
            value: 0.0,
            active: AtomicBool::new(false),
        });
        let quiet_input: Arc<dyn Input> = quiet.clone();
        bus.add(&quiet_input);
        assert!(!render(&bus).1);
        quiet.active.store(true, Ordering::Relaxed);
        assert!(render(&bus).1);
    }
}
