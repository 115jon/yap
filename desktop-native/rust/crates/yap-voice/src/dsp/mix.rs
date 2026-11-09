//! The remote audio mix: one [`Stream`] per received audio track, each with
//! its gain and stereo pan, summed into 10 ms stereo frames, then through a
//! [`Limiter`], for the speaker (and, the same frame, for the echo
//! canceller's far end).
//!
//! The pan is Web Audio's `StereoPannerNode` (the web's per-peer panner,
//! `audio-pipeline.ts`), equal-power, with the law for what the track
//! really carries. `NativeAudioStream` hands every track over as two
//! channels whatever it decoded (its sink remixes to the count it was
//! opened with, and opening it mono would fold a stereo share to mono), so
//! a stream counts as mono until a frame's two channels differ, and stays
//! stereo from then on. Mono takes the mono law, `[cos x, sin x]` with
//! `x = (pan + 1) * PI / 4` (unity on one side at a hard pan); stereo the
//! two-channel law. At pan 0 a track plays as it is, as the web plays an
//! unpanned peer (a plain `<audio>` element, no panner).
//!
//! Each stream is a small queue between WebRTC's clock (decoded audio,
//! 10 ms at a time) and the speaker's. Playback starts once
//! [`PRIME_FRAMES`] are queued, and an empty queue plays silence until it
//! is primed again. The two clocks drift (a few tenths of a percent), so
//! the queue's smoothed depth is held between [`BAND_LOW_FRAMES`] and
//! [`BAND_HIGH_FRAMES`]: outside it, each frame is resampled from
//! [`SKEW`] input samples more or fewer (0.6 %) until the depth is back in
//! the middle. Only a burst far past the band ([`MAX_FRAMES`]) is cut. Gain
//! and pan changes ramp over one frame, so a slider does not click.
//!
//! A stream above unity gain (a person at more than 100 %, up to
//! [`MAX_GAIN`]) goes through its own [`BoostLimiter`] after its gain and
//! pan and before the sum, so a boosted loud talker is held under
//! [`BOOST_CEILING`] instead of squaring off, and nobody else is pumped by
//! it. Its look-ahead reads the samples already waiting in the stream's
//! queue, so it adds no latency. At or below unity a stream never enters
//! it and plays bit for bit as before.

use std::collections::VecDeque;
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, Weak};

use super::{FRAME, RATE};

/// The mix is stereo.
pub const CHANNELS: usize = 2;

/// One frame of the mix, interleaved.
pub const SAMPLES: usize = FRAME * CHANNELS;

/// A stream plays (again) once this many frames are queued: 40 ms, the
/// middle of the band.
pub const PRIME_FRAMES: usize = 4;

/// Below this smoothed depth (30 ms) frames are stretched ...
pub const BAND_LOW_FRAMES: usize = 3;

/// ... above this (60 ms) shortened, each until the depth is back at 45 ms.
pub const BAND_HIGH_FRAMES: usize = 6;

/// Input samples (per channel) a corrected frame takes more or fewer than
/// [`FRAME`]: 3 in 480, 0.625 %, past the 0.5 % two clocks may drift.
pub const SKEW: usize = 3;

/// More than this queued (120 ms, a burst) is cut ...
pub const MAX_FRAMES: usize = 12;

/// ... down to this.
pub const TRIM_TO_FRAMES: usize = PRIME_FRAMES;

/// The depth's smoothing per frame (about half a second).
const DEPTH_SMOOTHING: f32 = 1.0 / 50.0;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The pan law for a pan in -1..=1, as a matrix `[l<-l, l<-r, r<-l, r<-r]`.
/// `stereo`: `StereoPannerNode`'s two-channel law. Otherwise the track is
/// mono (both channels the same) and takes the mono law, except at 0,
/// where it plays as it is.
pub fn pan_matrix(pan: f32, stereo: bool) -> [f32; 4] {
    let pan = pan.clamp(-1.0, 1.0);
    if stereo {
        if pan <= 0.0 {
            let x = (pan + 1.0) * FRAC_PI_2;
            [1.0, x.cos(), 0.0, x.sin()]
        } else {
            let x = pan * FRAC_PI_2;
            [x.cos(), 0.0, x.sin(), 1.0]
        }
    } else if pan == 0.0 {
        [1.0, 0.0, 0.0, 1.0]
    } else {
        // Both channels carry the same samples: read the left one.
        let x = (pan + 1.0) * FRAC_PI_4;
        [x.cos(), 0.0, x.sin(), 0.0]
    }
}

#[derive(Debug, Default)]
struct Queue {
    samples: VecDeque<i16>,
    playing: bool,
    /// A frame came with two different channels.
    stereo: bool,
    /// The gain and pan matrix the last frame ended on (`None` before the
    /// first).
    applied: Option<(f32, [f32; 4])>,
    /// The smoothed depth before each frame, in samples per channel.
    depth: f32,
    /// The correction running: input samples per frame more (positive) or
    /// fewer than [`FRAME`].
    skew: isize,
    underruns: u64,
    trimmed_frames: u64,
    skewed_frames: u64,
    /// The boost limiter (idle while the gain is at or under unity).
    boost: BoostLimiter,
    /// Frames that went through the boost limiter.
    boosted_frames: u64,
}

impl Queue {
    /// Samples per channel queued.
    fn frames(&self) -> usize {
        self.samples.len() / CHANNELS
    }

    /// Restarts the depth's average at what is queued now.
    fn reset_depth(&mut self) {
        self.depth = self.frames() as f32;
        self.skew = 0;
    }

    /// The correction for the next frame, from the smoothed depth.
    fn next_skew(&mut self) -> isize {
        self.depth += (self.frames() as f32 - self.depth) * DEPTH_SMOOTHING;
        let low = (BAND_LOW_FRAMES * FRAME) as f32;
        let high = (BAND_HIGH_FRAMES * FRAME) as f32;
        let middle = (low + high) / 2.0;
        let skew = SKEW as isize;
        self.skew = match self.skew {
            0 if self.depth > high => skew,
            0 if self.depth < low => -skew,
            s if s > 0 && self.depth <= middle => 0,
            s if s < 0 && self.depth >= middle => 0,
            s => s,
        };
        self.skew
    }
}

/// One remote track's audio on its way to the mix.
#[derive(Debug)]
pub struct Stream {
    queue: Mutex<Queue>,
    gain: AtomicU64,
    pan: AtomicU64,
}

impl Stream {
    fn new() -> Self {
        Self {
            queue: Mutex::new(Queue::default()),
            gain: AtomicU64::new(1f64.to_bits()),
            pan: AtomicU64::new(0f64.to_bits()),
        }
    }

    /// Queues decoded audio: interleaved, one or two channels, 48 kHz.
    pub fn push(&self, samples: &[i16], channels: usize) {
        let mut q = lock(&self.queue);
        match channels {
            1 => {
                for s in samples {
                    q.samples.push_back(*s);
                    q.samples.push_back(*s);
                }
            }
            2 => {
                if !q.stereo && samples.chunks_exact(2).any(|p| p[0] != p[1]) {
                    q.stereo = true;
                }
                q.samples.extend(samples.iter().copied());
            }
            _ => return,
        }
        let queued = q.samples.len() / SAMPLES;
        if queued > MAX_FRAMES {
            let drop = (queued - TRIM_TO_FRAMES) * SAMPLES;
            q.samples.drain(..drop);
            q.trimmed_frames += (queued - TRIM_TO_FRAMES) as u64;
            q.reset_depth();
        }
        if !q.playing && q.samples.len() >= PRIME_FRAMES * SAMPLES {
            q.playing = true;
            q.reset_depth();
        }
    }

    /// The linear gain (0..=[`MAX_GAIN`]; 1 is as received).
    pub fn set_gain(&self, gain: f64) {
        let gain = if gain.is_finite() {
            gain.clamp(0.0, MAX_GAIN)
        } else {
            1.0
        };
        self.gain.store(gain.to_bits(), Ordering::Relaxed);
    }

    /// The pan, -1 (left) ..= 1 (right).
    pub fn set_pan(&self, pan: f64) {
        let pan = if pan.is_finite() {
            pan.clamp(-1.0, 1.0)
        } else {
            0.0
        };
        self.pan.store(pan.to_bits(), Ordering::Relaxed);
    }

    pub fn gain(&self) -> f64 {
        f64::from_bits(self.gain.load(Ordering::Relaxed))
    }

    pub fn pan(&self) -> f64 {
        f64::from_bits(self.pan.load(Ordering::Relaxed))
    }

    /// Frames queued now.
    pub fn queued_frames(&self) -> usize {
        lock(&self.queue).samples.len() / SAMPLES
    }

    /// Adds one frame into `out` (or nothing while it is not primed), at
    /// `pan` when one is forced.
    fn mix_into(&self, out: &mut [f32; SAMPLES], audible: bool, pan: Option<f32>) {
        let gain = self.gain() as f32;
        let pan = pan.unwrap_or(self.pan() as f32);
        let mut guard = lock(&self.queue);
        let q = &mut *guard;
        let to = pan_matrix(pan, q.stereo);
        let (from_gain, from) = q.applied.unwrap_or((gain, to));
        q.applied = Some((gain, to));
        if !q.playing {
            return;
        }
        // Output sample `i` reads input `i * taken / produced`, between two
        // input samples (linear) in a corrected frame, so the next frame
        // starts on a whole sample and the joins stay continuous.
        let have = q.frames();
        let (taken, produced) = if have < FRAME {
            // Ran dry: what is left plays, then silence until primed again.
            q.playing = false;
            q.underruns += 1;
            (have, have)
        } else {
            let skew = q.next_skew();
            let want = FRAME.saturating_add_signed(skew);
            if want <= have {
                if skew != 0 {
                    q.skewed_frames += 1;
                }
                (want, FRAME)
            } else {
                (FRAME, FRAME)
            }
        };
        if !audible {
            // Deafened: nothing to limit; start clean when it comes back.
            q.boost.reset();
        }
        // Above unity, or still releasing from it: through the limiter.
        // Otherwise straight into the sum, exactly as before boosting
        // existed.
        let boosted = audible && (gain > 1.0 || from_gain > 1.0 || !q.boost.idle());
        let mut own = [0f32; SAMPLES];
        let samples = &q.samples;
        let read = |j: usize, c: usize| f32::from(samples[CHANNELS * j + c]);
        {
            let dst: &mut [f32; SAMPLES] = if boosted { &mut own } else { out };
            for i in 0..produced {
                let at = i * taken;
                let (j, rest) = (at / produced, at % produced);
                let (l, r) = if rest == 0 {
                    (read(j, 0), read(j, 1))
                } else {
                    let next = (j + 1).min(have - 1);
                    let frac = rest as f32 / produced as f32;
                    (
                        read(j, 0) + (read(next, 0) - read(j, 0)) * frac,
                        read(j, 1) + (read(next, 1) - read(j, 1)) * frac,
                    )
                };
                if !audible {
                    continue;
                }
                let t = (i + 1) as f32 / FRAME as f32;
                let g = from_gain + (gain - from_gain) * t;
                let m: [f32; 4] = std::array::from_fn(|k| from[k] + (to[k] - from[k]) * t);
                dst[2 * i] += g * (m[0] * l + m[1] * r);
                dst[2 * i + 1] += g * (m[2] * l + m[3] * r);
            }
        }
        if boosted {
            // The look-ahead: what follows this frame in the queue, at the
            // gain and pan it is heading for.
            let mut ahead = [0f32; BOOST_LOOKAHEAD];
            let known = (have - taken).min(BOOST_LOOKAHEAD);
            for (k, peak) in ahead.iter_mut().enumerate().take(known) {
                let (l, r) = (read(taken + k, 0), read(taken + k, 1));
                let left = gain * (to[0] * l + to[1] * r);
                let right = gain * (to[2] * l + to[3] * r);
                *peak = left.abs().max(right.abs()) / 32768.0;
            }
            let n = produced * CHANNELS;
            // Back at or under unity: only let go of what is held, so the
            // stream returns to the plain path even if it is loud.
            if gain <= 1.0 && from_gain <= 1.0 {
                q.boost.release_only(&mut own[..n]);
            } else {
                q.boost.process(&mut own[..n], &ahead[..known], 32768.0);
            }
            q.boosted_frames += 1;
            for (o, s) in out.iter_mut().zip(&own[..n]) {
                *o += *s;
            }
        }
        q.samples.drain(..taken * CHANNELS);
    }
}

/// The highest stream gain: 400 % for the person times 200 % output.
pub const MAX_GAIN: f64 = crate::audio::MAX_GAIN;

/// The boost limiter's ceiling on the -1..=1 scale: the mix limiter's
/// threshold (-3 dBFS). One boosted person alone therefore reaches the mix
/// limiter under its threshold and passes it untouched (one limiter, the
/// one with look-ahead, does the work), and a sum of several still has the
/// mix limiter and the speaker's soft clip above it.
pub const BOOST_CEILING: f32 = LIMIT_THRESHOLD;

/// The boost limiter's look-ahead, in samples: 1.5 ms at 48 kHz, which is
/// also its attack. Read from the stream's queue, so it adds no delay.
pub const BOOST_LOOKAHEAD: usize = 72;

/// How long the boost limiter holds a reduction before it lets go: 5 ms,
/// half a period at 100 Hz, so it does not ride the waveform of a low
/// voice.
pub const BOOST_HOLD: usize = 240;

/// The boost limiter's release time constant, in seconds.
pub const BOOST_RELEASE_SECONDS: f32 = 0.06;

/// The attack's window: the look-ahead and the sample itself.
const BOOST_WINDOW: usize = BOOST_LOOKAHEAD + 1;

/// The most samples per channel the limiter looks at in one frame.
const BOOST_SPAN: usize = FRAME + BOOST_LOOKAHEAD;

/// A look-ahead peak limiter for one boosted stream, stereo linked so its
/// pan survives it.
///
/// Each sample needs the gain that brings its louder channel down to
/// [`BOOST_CEILING`]. The gain held at a sample is the smallest need over
/// it and the [`BOOST_LOOKAHEAD`] samples after it (a sliding minimum, van
/// Herk / Gil-Werman, a few operations per sample), kept for
/// [`BOOST_HOLD`] samples and then let back up with a one-pole release
/// ([`BOOST_RELEASE_SECONDS`], run on the reduction's depth so it gets all
/// the way back to unity in `f32`). The gain applied is the
/// mean of the last [`BOOST_LOOKAHEAD`] + 1 held gains: each of them
/// already covers the sample's own need, so the mean does too, and the gain
/// glides down over 1.5 ms into a peak instead of stepping (no click, no
/// square edge). A last clamp at the ceiling only acts when the look-ahead
/// guessed wrong (the gain or pan moved between frames).
#[derive(Debug, Clone)]
pub struct BoostLimiter {
    /// The last [`BOOST_WINDOW`] held gains, a ring.
    ring: [f32; BOOST_WINDOW],
    at: usize,
    /// The held reduction, 1 - gain, at the last sample.
    dip: f32,
    /// Samples left before the release starts.
    wait: usize,
    release: f32,
    /// Samples the last clamp had to cut (the look-ahead guessed wrong).
    clamped: u64,
}

impl Default for BoostLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl BoostLimiter {
    pub fn new() -> Self {
        Self {
            ring: [1.0; BOOST_WINDOW],
            at: 0,
            dip: 0.0,
            wait: 0,
            release: (-1.0 / (BOOST_RELEASE_SECONDS * RATE as f32)).exp(),
            clamped: 0,
        }
    }

    /// Samples the last clamp cut so far: 0 while the look-ahead was right.
    pub fn clamped(&self) -> u64 {
        self.clamped
    }

    /// Back to no reduction at once.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Nothing held back: until a peak needs it, it would change nothing.
    pub fn idle(&self) -> bool {
        self.dip <= 0.0 && self.ring.iter().all(|g| *g >= 1.0)
    }

    /// The gain it applies now (1: none).
    pub fn reduction(&self) -> f32 {
        (self.ring.iter().sum::<f32>() / BOOST_WINDOW as f32).min(1.0)
    }

    /// Lets go of the reduction it holds (on its hold and release) without
    /// taking any new one: the stream is back at or under unity, where it
    /// plays as it is. Idle again within about a second.
    pub fn release_only(&mut self, frame: &mut [f32]) {
        let mut sum: f32 = self.ring.iter().sum();
        for pair in frame.chunks_exact_mut(CHANNELS).take(FRAME) {
            let g = self.step(0.0, &mut sum);
            for s in pair {
                let y = *s * g;
                *s = if y.is_finite() { y } else { 0.0 };
            }
        }
    }

    /// One sample: holds `target` (a depth, 1 - gain) or releases towards
    /// it, and returns the gain to apply, the mean of the window.
    fn step(&mut self, target: f32, sum: &mut f32) -> f32 {
        if target >= self.dip {
            self.dip = target;
            self.wait = BOOST_HOLD;
        } else if self.wait > 0 {
            self.wait -= 1;
        } else {
            // Back towards `target`, never past it.
            self.dip = target + (self.dip - target) * self.release;
            if target <= 0.0 && self.dip < 1e-6 {
                self.dip = 0.0;
            }
        }
        let held = 1.0 - self.dip;
        *sum += held - self.ring[self.at];
        self.ring[self.at] = held;
        self.at = (self.at + 1) % BOOST_WINDOW;
        (*sum / BOOST_WINDOW as f32).min(1.0)
    }

    /// The gain a sample of this peak (-1..=1 scale) needs.
    fn need(peak: f32) -> f32 {
        if peak > BOOST_CEILING {
            BOOST_CEILING / peak
        } else {
            // NaN too: nothing to hold for it; the last clamp drops it.
            1.0
        }
    }

    /// Limits interleaved stereo (at most a frame) in place, on a scale
    /// where `full` is full scale (`1.0` for -1..=1, `32768.0` for the
    /// mix's 16-bit sums). `ahead`: the peaks (on the -1..=1 scale) of what
    /// comes next, as far as it is known (at most [`BOOST_LOOKAHEAD`]);
    /// past it, silence is assumed.
    pub fn process(&mut self, frame: &mut [f32], ahead: &[f32], full: f32) {
        let n = (frame.len() / CHANNELS).min(FRAME);
        let known = ahead.len().min(BOOST_LOOKAHEAD);
        let span = n + BOOST_LOOKAHEAD;
        let scale = 1.0 / full;
        let mut need = [1f32; BOOST_SPAN];
        for (k, pair) in frame.chunks_exact(CHANNELS).take(n).enumerate() {
            need[k] = Self::need(pair[0].abs().max(pair[1].abs()) * scale);
        }
        for (k, peak) in ahead[..known].iter().enumerate() {
            need[n + k] = Self::need(*peak);
        }
        // The minimum over need[k..k + BOOST_WINDOW], from running
        // minimums forwards and backwards within blocks of the window.
        let mut forward = [1f32; BOOST_SPAN];
        let mut backward = [1f32; BOOST_SPAN];
        for k in 0..span {
            forward[k] = if k.is_multiple_of(BOOST_WINDOW) {
                need[k]
            } else {
                forward[k - 1].min(need[k])
            };
        }
        for k in (0..span).rev() {
            backward[k] = if k % BOOST_WINDOW == BOOST_WINDOW - 1 || k + 1 == span {
                need[k]
            } else {
                backward[k + 1].min(need[k])
            };
        }
        let ceiling = BOOST_CEILING * full;
        // Summed afresh each frame, so rounding never builds up.
        let mut sum: f32 = self.ring.iter().sum();
        for (k, pair) in frame.chunks_exact_mut(CHANNELS).take(n).enumerate() {
            // The depth the window ahead needs held from here.
            let target = 1.0 - backward[k].min(forward[k + BOOST_LOOKAHEAD]);
            let g = self.step(target, &mut sum);
            for s in pair {
                let y = *s * g;
                *s = if !y.is_finite() {
                    self.clamped += 1;
                    0.0
                } else if y.abs() > ceiling {
                    // Rounding alone stays within a few ulps; count more.
                    if y.abs() > ceiling * 1.000_01 {
                        self.clamped += 1;
                    }
                    y.clamp(-ceiling, ceiling)
                } else {
                    y
                };
            }
        }
    }
}

/// The limiter's threshold, -3 dBFS on the -1..=1 scale.
pub const LIMIT_THRESHOLD: f32 = 0.707_946;

/// The soft clip's ceiling, -0.2 dBFS: the mix never reaches the rails.
pub const LIMIT_CEILING: f32 = 0.977_237;

const LIMIT_RATIO: f32 = 20.0;

/// A soft limiter on the summed mix, the web's `DynamicsCompressorNode`
/// before its master gain (`audio-pipeline.ts`: -3 dBFS, ratio 20, 1 ms
/// attack, 100 ms release) without its look-ahead: a gain follows the
/// peaks, and what its 1 ms attack lets past is bent by a soft clip under
/// full scale, so loud peers together never square off at the rails.
#[derive(Debug)]
pub struct Limiter {
    gain: f32,
    attack: f32,
    release: f32,
}

impl Default for Limiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Limiter {
    pub fn new() -> Self {
        // One-pole coefficients per stereo sample frame.
        let per = |seconds: f32| (-1.0 / (seconds * RATE as f32)).exp();
        Self {
            gain: 1.0,
            attack: per(0.001),
            release: per(0.1),
        }
    }

    fn soft_clip(x: f32) -> f32 {
        let a = x.abs();
        if a <= LIMIT_THRESHOLD {
            return x;
        }
        let room = LIMIT_CEILING - LIMIT_THRESHOLD;
        (LIMIT_THRESHOLD + room * ((a - LIMIT_THRESHOLD) / room).tanh()).copysign(x)
    }

    /// Limits interleaved stereo on the -1..=1 scale, in place.
    pub fn process(&mut self, frame: &mut [f32]) {
        for pair in frame.chunks_exact_mut(CHANNELS) {
            let peak = pair.iter().fold(0f32, |p, s| p.max(s.abs()));
            let target = if peak > LIMIT_THRESHOLD {
                LIMIT_THRESHOLD * (peak / LIMIT_THRESHOLD).powf(1.0 / LIMIT_RATIO) / peak
            } else {
                1.0
            };
            let coefficient = if target < self.gain {
                self.attack
            } else {
                self.release
            };
            self.gain = target + (self.gain - target) * coefficient;
            for s in pair {
                *s = Self::soft_clip(*s * self.gain);
            }
        }
    }
}

/// What the mix has done so far, for the stats line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MixStats {
    pub streams: usize,
    /// The fullest queue now, in frames.
    pub most_queued: usize,
    pub underruns: u64,
    pub trimmed_frames: u64,
    /// Frames resampled to hold a queue in its band (clock drift).
    pub skewed_frames: u64,
    /// Frames of boosted streams (above unity gain) through their limiter.
    pub boosted_frames: u64,
    /// Samples the boost limiters' last clamp cut (0 while their
    /// look-ahead holds).
    pub boost_clamped: u64,
}

/// Every remote audio stream of the call.
#[derive(Debug)]
pub struct Mixer {
    streams: Mutex<Vec<Weak<Stream>>>,
    enabled: AtomicBool,
    /// A pan for every stream (`NaN`: none), the `YAP_TEST_PAN` hook.
    forced_pan: AtomicU64,
    limiter: Mutex<Limiter>,
}

impl Default for Mixer {
    fn default() -> Self {
        Self::new()
    }
}

impl Mixer {
    pub fn new() -> Self {
        Self {
            streams: Mutex::new(Vec::new()),
            enabled: AtomicBool::new(true),
            forced_pan: AtomicU64::new(f64::NAN.to_bits()),
            limiter: Mutex::new(Limiter::new()),
        }
    }

    /// A new stream in the mix; it leaves the mix when the last `Arc` goes.
    pub fn add(&self) -> Arc<Stream> {
        let stream = Arc::new(Stream::new());
        lock(&self.streams).push(Arc::downgrade(&stream));
        stream
    }

    /// Off (deafened): the queues still drain, so nothing stale plays when
    /// it comes back on, but the mix is silence.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    pub fn enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Every stream at `pan` whatever it is set to (a test hook), or each
    /// at its own (`None`).
    pub fn force_pan(&self, pan: Option<f64>) {
        let pan = pan.filter(|p| p.is_finite()).map(|p| p.clamp(-1.0, 1.0));
        self.forced_pan
            .store(pan.unwrap_or(f64::NAN).to_bits(), Ordering::Relaxed);
    }

    /// The next 10 ms of the mix into `out` (interleaved stereo, at least
    /// [`SAMPLES`] long; the rest is left alone).
    pub fn render(&self, out: &mut [i16]) {
        let mut sum = [0f32; SAMPLES];
        let audible = self.enabled();
        let forced = f64::from_bits(self.forced_pan.load(Ordering::Relaxed));
        let forced = (!forced.is_nan()).then_some(forced as f32);
        {
            // Mixed under the list's lock, so a frame allocates nothing; a
            // stream being added waits one frame's mix at most.
            let mut streams = lock(&self.streams);
            streams.retain(|s| s.strong_count() > 0);
            for stream in streams.iter().filter_map(Weak::upgrade) {
                stream.mix_into(&mut sum, audible, forced);
            }
        }
        for s in &mut sum {
            *s /= 32768.0;
        }
        lock(&self.limiter).process(&mut sum);
        for (o, s) in out.iter_mut().zip(sum.iter()) {
            // In range after the clamp (and under it after the limiter).
            *o = (s * 32768.0).round().clamp(-32768.0, 32767.0) as i16;
        }
    }

    pub fn stats(&self) -> MixStats {
        let mut streams = lock(&self.streams);
        streams.retain(|s| s.strong_count() > 0);
        let mut stats = MixStats::default();
        for stream in streams.iter().filter_map(Weak::upgrade) {
            let q = lock(&stream.queue);
            stats.streams += 1;
            stats.most_queued = stats.most_queued.max(q.samples.len() / SAMPLES);
            stats.underruns += q.underruns;
            stats.trimmed_frames += q.trimmed_frames;
            stats.skewed_frames += q.skewed_frames;
            stats.boosted_frames += q.boosted_frames;
            stats.boost_clamped += q.boost.clamped();
        }
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::{channel, rms};

    /// A tone at `hz` and `scale` (0..=1), `n` samples from sample `from`.
    fn sine(hz: f32, scale: f32, from: usize, n: usize) -> Vec<i16> {
        (from..from + n)
            .map(|n| {
                let t = n as f32 / 48_000.0;
                (scale * 32767.0 * (2.0 * std::f32::consts::PI * hz * t).sin()) as i16
            })
            .collect()
    }

    /// `frames` 10 ms frames of a mono 1 kHz tone at half scale.
    fn tone(frames: usize) -> Vec<i16> {
        sine(1000.0, 0.5, 0, frames * FRAME)
    }

    /// Interleaves `left` and `right`.
    fn stereo(left: &[i16], right: &[i16]) -> Vec<i16> {
        left.iter().zip(right).flat_map(|(l, r)| [*l, *r]).collect()
    }

    /// The left and right RMS of the last of three frames mixed from
    /// `samples` (`channels` interleaved) at `pan` and `gain`.
    fn mixed(samples: &[i16], channels: usize, pan: f64, gain: f64) -> (f64, f64) {
        let mixer = Mixer::new();
        let stream = mixer.add();
        stream.set_pan(pan);
        stream.set_gain(gain);
        stream.push(samples, channels);
        let mut out = vec![0i16; SAMPLES];
        for _ in 0..3 {
            mixer.render(&mut out);
        }
        (rms(&channel(&out, 2, 0)), rms(&channel(&out, 2, 1)))
    }

    fn panned(pan: f64, gain: f64) -> (f64, f64) {
        mixed(&tone(6), 1, pan, gain)
    }

    /// The tone's RMS: half scale.
    const TONE_RMS: f64 = 0.3535;

    #[test]
    fn centre_should_play_the_track_as_it_is() {
        let (l, r) = panned(0.0, 1.0);
        assert!(
            (l - r).abs() < 1e-6 && (l - TONE_RMS).abs() < 0.01,
            "{l} {r}"
        );
    }

    #[test]
    fn a_hard_right_mono_track_should_play_at_unity_on_the_right_only() {
        let (l, r) = panned(1.0, 1.0);
        assert!(l < 1e-4 && (r - TONE_RMS).abs() < 0.005, "{l} {r}");
    }

    #[test]
    fn a_hard_left_mono_track_should_play_at_unity_on_the_left_only() {
        let (l, r) = panned(-1.0, 1.0);
        assert!(r < 1e-4 && (l - TONE_RMS).abs() < 0.005, "{l} {r}");
    }

    #[test]
    fn a_half_right_mono_track_should_follow_the_mono_law() {
        // x = 3/8 pi: cos 0.383, sin 0.924.
        let (l, r) = panned(0.5, 1.0);
        assert!(
            (l - TONE_RMS * 0.3827).abs() < 0.005 && (r - TONE_RMS * 0.9239).abs() < 0.005,
            "{l} {r}"
        );
    }

    #[test]
    fn a_mono_track_sent_as_two_equal_channels_should_take_the_mono_law() {
        // What NativeAudioStream hands over for a mono track: not +6 dB.
        let mono = tone(6);
        let (l, r) = mixed(&stereo(&mono, &mono), 2, -1.0, 1.0);
        assert!(r < 1e-4 && (l - TONE_RMS).abs() < 0.005, "{l} {r}");
    }

    #[test]
    fn a_hard_right_stereo_track_should_fold_its_left_into_the_right() {
        let left = tone(6);
        let (l, r) = mixed(&stereo(&left, &vec![0; left.len()]), 2, 1.0, 1.0);
        assert!(l < 1e-4 && (r - TONE_RMS).abs() < 0.005, "{l} {r}");
    }

    #[test]
    fn a_centred_stereo_track_should_keep_its_channels() {
        let left = tone(6);
        let (l, r) = mixed(&stereo(&left, &vec![0; left.len()]), 2, 0.0, 1.0);
        assert!(r < 1e-4 && (l - TONE_RMS).abs() < 0.005, "{l} {r}");
    }

    #[test]
    fn a_left_pan_should_favour_the_left() {
        let (l, r) = panned(-0.5, 1.0);
        assert!(l > 1.5 * r, "{l} {r}");
    }

    #[test]
    fn the_pan_laws_should_keep_unity_at_the_centre() {
        assert_eq!(pan_matrix(0.0, false), [1.0, 0.0, 0.0, 1.0]);
        let m = pan_matrix(0.0, true);
        assert!(m[0] == 1.0 && m[1].abs() < 1e-6 && (m[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn gain_should_scale_the_output() {
        let (full, _) = panned(0.0, 1.0);
        let (half, _) = panned(0.0, 0.5);
        assert!((half / full - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_disabled_mix_should_be_silent_but_drain() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        stream.push(&tone(PRIME_FRAMES + 1), 1);
        mixer.set_enabled(false);
        let mut out = vec![0i16; SAMPLES];
        mixer.render(&mut out);
        assert!(out.iter().all(|s| *s == 0) && stream.queued_frames() == PRIME_FRAMES);
    }

    #[test]
    fn a_stream_should_wait_until_primed() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        stream.push(&tone(PRIME_FRAMES - 1), 1);
        let mut out = vec![0i16; SAMPLES];
        mixer.render(&mut out);
        assert!(out.iter().all(|s| *s == 0));
    }

    #[test]
    fn a_queue_far_ahead_should_be_trimmed() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        stream.push(&tone(MAX_FRAMES + 1), 1);
        assert_eq!(stream.queued_frames(), TRIM_TO_FRAMES);
        assert_eq!(
            mixer.stats().trimmed_frames,
            (MAX_FRAMES + 1 - TRIM_TO_FRAMES) as u64
        );
    }

    #[test]
    fn a_dropped_stream_should_leave_the_mix() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        drop(stream);
        assert_eq!(mixer.stats().streams, 0);
    }

    #[test]
    fn two_hard_panned_mono_streams_should_add_at_unity() {
        let mixer = Mixer::new();
        let (a, b) = (mixer.add(), mixer.add());
        a.set_pan(-1.0);
        b.set_pan(1.0);
        a.push(&tone(PRIME_FRAMES), 1);
        b.push(&tone(PRIME_FRAMES), 1);
        let mut out = vec![0i16; SAMPLES];
        mixer.render(&mut out);
        mixer.render(&mut out);
        let (l, r) = (rms(&channel(&out, 2, 0)), rms(&channel(&out, 2, 1)));
        assert!(
            (l - TONE_RMS).abs() < 0.005 && (r - TONE_RMS).abs() < 0.005,
            "{l} {r}"
        );
    }

    #[test]
    fn a_forced_pan_should_override_every_stream() {
        let mixer = Mixer::new();
        mixer.force_pan(Some(1.0));
        let stream = mixer.add();
        stream.set_pan(-1.0);
        stream.push(&tone(PRIME_FRAMES), 1);
        let mut out = vec![0i16; SAMPLES];
        mixer.render(&mut out);
        mixer.render(&mut out);
        assert!(rms(&channel(&out, 2, 0)) < 1e-4);
    }

    #[test]
    fn loud_peers_at_gain_four_should_never_hit_the_rails() {
        let mixer = Mixer::new();
        let streams: Vec<_> = (0..4).map(|_| mixer.add()).collect();
        for (k, stream) in streams.iter().enumerate() {
            stream.set_gain(4.0);
            stream.set_pan([-1.0, -0.3, 0.3, 1.0][k]);
            stream.push(&sine(300.0 + 170.0 * k as f32, 0.9, 0, 10 * FRAME), 1);
        }
        let mut out = vec![0i16; SAMPLES];
        let mut played = Vec::new();
        for _ in 0..10 {
            mixer.render(&mut out);
            played.extend_from_slice(&out);
        }
        let peak = played
            .iter()
            .map(|s| i32::from(*s).abs())
            .max()
            .unwrap_or(0);
        let ceiling = (LIMIT_CEILING * 32768.0) as i32 + 1;
        assert!(peak <= ceiling && rms(&played) > 0.3, "peak {peak}");
    }

    #[test]
    fn the_limiter_should_leave_a_quiet_mix_alone() {
        let mut limiter = Limiter::new();
        let mut frame: Vec<f32> = tone(1).iter().map(|s| f32::from(*s) / 32768.0).collect();
        let before = frame.clone();
        limiter.process(&mut frame);
        assert_eq!(frame, before);
    }

    #[test]
    fn matched_clocks_should_pass_every_sample_unchanged() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        let input = sine(440.0, 0.5, 0, 200 * FRAME);
        let mut out = vec![0i16; SAMPLES];
        let mut played = Vec::new();
        for (n, frame) in input.chunks_exact(FRAME).enumerate() {
            stream.push(frame, 1);
            if n + 1 >= PRIME_FRAMES {
                mixer.render(&mut out);
                played.extend(channel(&out, 2, 0));
            }
        }
        assert!(played == input[..played.len()] && mixer.stats().skewed_frames == 0);
    }

    /// What a 10 minute run with the speaker's period `ratio` times the
    /// decoder's showed.
    #[derive(Debug)]
    struct Drift {
        /// The longest run of silence once playback began, in samples.
        longest_gap: usize,
        most_queued: usize,
        stats: MixStats,
    }

    /// The decoder delivers 10 ms frames two at a time every 20 ms (as the
    /// receive pump may); the speaker takes 10 ms every `10 * ratio` ms.
    /// The tone rides on an offset, so silence is a gap, never a zero
    /// crossing.
    fn drift_run(ratio: f64) -> Drift {
        let mixer = Mixer::new();
        let stream = mixer.add();
        let end_us = 10.0 * 60.0 * 1e6;
        let source: Vec<i16> = sine(250.0, 0.2, 0, 2 * FRAME)
            .iter()
            .map(|s| s + 8000)
            .collect();
        let (mut pushed, mut rendered) = (0u32, 0u32);
        let mut out = vec![0i16; SAMPLES];
        let (mut started, mut gap, mut longest_gap, mut most_queued) = (false, 0, 0, 0);
        loop {
            let push_at = f64::from(pushed) * 20_000.0;
            let render_at = 5_000.0 + f64::from(rendered) * 10_000.0 * ratio;
            if push_at.min(render_at) > end_us {
                break;
            }
            if push_at <= render_at {
                stream.push(&source, 1);
                most_queued = most_queued.max(stream.queued_frames());
                pushed += 1;
            } else {
                mixer.render(&mut out);
                for s in out.iter().step_by(2) {
                    if *s == 0 {
                        gap += 1;
                    } else {
                        started = true;
                        gap = 0;
                    }
                    if started {
                        longest_gap = longest_gap.max(gap);
                    }
                }
                rendered += 1;
            }
        }
        Drift {
            longest_gap,
            most_queued,
            stats: mixer.stats(),
        }
    }

    fn drift_held(drift: &Drift) -> bool {
        drift.longest_gap <= FRAME
            && drift.stats.underruns == 0
            && drift.stats.trimmed_frames == 0
            && drift.most_queued <= BAND_HIGH_FRAMES + 3
            && drift.stats.skewed_frames > 0
    }

    #[test]
    fn a_speaker_half_a_percent_slow_should_neither_gap_nor_grow_the_queue() {
        let drift = drift_run(1.005);
        eprintln!("{drift:?}");
        assert!(drift_held(&drift), "{drift:?}");
    }

    #[test]
    fn a_speaker_half_a_percent_fast_should_neither_gap_nor_drain_the_queue() {
        let drift = drift_run(0.995);
        eprintln!("{drift:?}");
        assert!(drift_held(&drift), "{drift:?}");
    }

    // ── The boost limiter ──────────────────────────────────────────────────

    /// After 30 ms of silence (someone starts talking), full-scale noise
    /// bursts with hard onsets (10 ms on, 20 ms off) and a 220 Hz tone at
    /// -3 dBFS between them: a loud talker at its worst.
    fn loud_talker(frames: usize, seed: u64) -> Vec<i16> {
        let mut noise = crate::dsp::denoise::tests::Noise::new(seed);
        let tone = sine(220.0, 0.707, 0, frames * FRAME);
        (0..frames * FRAME)
            .map(|n| {
                if n < 3 * FRAME {
                    return 0;
                }
                let burst = (n / FRAME).is_multiple_of(3);
                let s = if burst {
                    noise.next() * 32767.0
                } else {
                    f64::from(tone[n])
                };
                s.clamp(-32768.0, 32767.0) as i16
            })
            .collect()
    }

    /// Mixes `input` (`channels` interleaved) at `gain` and `pan`, a frame at
    /// a time, the queue topped up as the decoder would; every frame's
    /// contribution of this stream alone (before the mix limiter).
    fn own_frames(input: &[i16], channels: usize, gain: f64, pan: f64) -> (Vec<f32>, MixStats) {
        let mixer = Mixer::new();
        let stream = mixer.add();
        stream.set_gain(gain);
        stream.set_pan(pan);
        let step = FRAME * channels;
        let mut played = Vec::new();
        for (n, frame) in input.chunks_exact(step).enumerate() {
            stream.push(frame, channels);
            if n + 1 >= PRIME_FRAMES {
                let mut out = [0f32; SAMPLES];
                stream.mix_into(&mut out, true, None);
                played.extend_from_slice(&out);
            }
        }
        (played, mixer.stats())
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0f32, |p, s| p.max(s.abs())) / 32768.0
    }

    fn rms_f32(samples: &[f32]) -> f64 {
        let sum: f64 = samples
            .iter()
            .map(|s| (f64::from(*s) / 32768.0).powi(2))
            .sum();
        (sum / samples.len().max(1) as f64).sqrt()
    }

    #[test]
    fn a_stream_at_or_under_unity_should_play_bit_for_bit_as_before() {
        let input = loud_talker(30, 3);
        for gain in [1.0f64, 0.999, 0.75, 0.3, 0.0] {
            let (played, stats) = own_frames(&input, 1, gain, 0.0);
            // What the mix did before the boost existed: g * (1 l + 0 r).
            let g = gain as f32;
            let before: Vec<f32> = input
                .iter()
                .take(played.len() / 2)
                .flat_map(|s| {
                    let l = f32::from(*s);
                    [
                        0.0 + g * (1.0 * l + 0.0 * l),
                        0.0 + g * (0.0 * l + 1.0 * l),
                    ]
                })
                .collect();
            assert_eq!(stats.boosted_frames, 0, "gain {gain}");
            assert!(
                played.iter().zip(&before).all(|(a, b)| a.to_bits() == b.to_bits()),
                "gain {gain} changed the samples"
            );
        }
    }

    #[test]
    fn a_ramp_that_stays_at_or_under_unity_should_never_enter_the_limiter() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        let input = loud_talker(40, 5);
        let mut out = [0f32; SAMPLES];
        for (n, frame) in input.chunks_exact(FRAME).enumerate() {
            stream.push(frame, 1);
            stream.set_gain([1.0, 0.4, 1.0, 0.8][n % 4]);
            stream.set_pan([0.0, -0.6, 0.6, 1.0][n % 4]);
            stream.mix_into(&mut out, true, None);
        }
        assert_eq!(mixer.stats().boosted_frames, 0);
    }

    #[test]
    fn a_boosted_loud_talker_should_never_pass_the_ceiling() {
        let mono = loud_talker(60, 9);
        let other = loud_talker(60, 21);
        let stereo_input = stereo(&mono, &other);
        for gain in [1.01, 1.5, 2.0, 4.0, MAX_GAIN] {
            for pan in [-1.0, -0.4, 0.0, 0.7, 1.0] {
                for (input, channels) in [(&mono, 1), (&stereo_input, 2)] {
                    let (played, stats) = own_frames(input, channels, gain, pan);
                    let p = peak(&played);
                    assert!(
                        p <= BOOST_CEILING * 1.000_001,
                        "gain {gain} pan {pan} ch {channels}: peak {p}"
                    );
                    assert!(played.iter().all(|s| s.is_finite()));
                    assert!(stats.boosted_frames > 0);
                    // The limiter's own gain did it, not the last clamp.
                    assert_eq!(stats.boost_clamped, 0, "gain {gain} pan {pan}");
                }
            }
        }
    }

    #[test]
    fn four_times_should_be_much_louder_than_unity_yet_clean() {
        // A quiet talker: a -18 dBFS tone with a -2.5 dBFS shout every
        // half second.
        let quiet: Vec<i16> = sine(330.0, 0.125, 0, 200 * FRAME)
            .iter()
            .enumerate()
            .map(|(n, s)| if (n / FRAME) % 50 == 10 { s * 6 } else { *s })
            .collect();
        let (unity, _) = own_frames(&quiet, 1, 1.0, 0.0);
        let (boosted, _) = own_frames(&quiet, 1, 4.0, 0.0);
        // The talker between shouts (frames 40-49 of each 50 played, the
        // limiter released).
        let body = |played: &[f32]| -> Vec<f32> {
            played
                .chunks_exact(SAMPLES)
                .enumerate()
                .filter(|(n, _)| (n + PRIME_FRAMES - 1) % 50 >= 40)
                .flat_map(|(_, f)| f.iter().copied())
                .collect()
        };
        let all_db = 20.0 * (rms_f32(&boosted) / rms_f32(&unity)).log10();
        let body_db = 20.0 * (rms_f32(&body(&boosted)) / rms_f32(&body(&unity))).log10();
        eprintln!(
            "boost x4: {body_db:.2} dB louder between shouts, {all_db:.1} dB overall, peak {:.3}",
            peak(&boosted)
        );
        // The -18 dBFS talker takes the full 12 dB; only the shouts are held.
        assert!((body_db - 12.04).abs() < 0.1, "{body_db:.2} dB");
        assert!(all_db > 8.0, "{all_db:.1} dB");
        assert!(peak(&boosted) <= BOOST_CEILING * 1.000_001);
    }

    #[test]
    fn a_boosted_stream_should_keep_its_pan() {
        let input = loud_talker(30, 4);
        let (right, _) = own_frames(&input, 1, 4.0, 1.0);
        let left: Vec<f32> = right.iter().step_by(2).copied().collect();
        assert!(rms_f32(&left) < 1e-4, "a hard right leaked left");
        // A half pan keeps the mono law's ratio while it limits.
        let (half, _) = own_frames(&input, 1, 4.0, 0.5);
        let (l, r): (Vec<f32>, Vec<f32>) = half.chunks_exact(2).map(|p| (p[0], p[1])).unzip();
        let ratio = rms_f32(&l) / rms_f32(&r);
        assert!((ratio - 0.3827 / 0.9239).abs() < 0.01, "{ratio}");
    }

    #[test]
    fn the_boost_limiter_should_never_produce_nan_or_pass_the_ceiling() {
        let mut limiter = BoostLimiter::new();
        let mut frame: Vec<f32> = (0..SAMPLES)
            .map(|i| match i % 7 {
                0 => f32::NAN,
                1 => f32::INFINITY,
                2 => f32::NEG_INFINITY,
                3 => 1e30,
                4 => -3.5,
                5 => 0.0,
                _ => 0.25,
            })
            .collect();
        limiter.process(&mut frame, &[f32::NAN, f32::INFINITY, 9.0], 1.0);
        assert!(frame.iter().all(|s| s.is_finite() && s.abs() <= BOOST_CEILING));
        assert!(limiter.reduction().is_finite());
        // Silence and an empty look-ahead are fine too.
        let mut silence = [0f32; SAMPLES];
        BoostLimiter::new().process(&mut silence, &[], 1.0);
        assert!(silence.iter().all(|s| *s == 0.0));
    }

    /// The gain the limiter applies sample by sample to a constant `level`
    /// that steps to `loud` for `loud_frames` frames, then back.
    fn gain_curve(level: f32, loud: f32, quiet_before: usize, loud_frames: usize, after: usize) -> Vec<f32> {
        let mut limiter = BoostLimiter::new();
        let total = quiet_before + loud_frames + after;
        let at = |frame: usize| {
            if (quiet_before..quiet_before + loud_frames).contains(&frame) {
                loud
            } else {
                level
            }
        };
        let mut gains = Vec::new();
        for f in 0..total {
            let x = at(f);
            let mut frame = [x; SAMPLES];
            let ahead = [at(f + 1); BOOST_LOOKAHEAD];
            limiter.process(&mut frame, &ahead, 1.0);
            gains.extend(frame.iter().step_by(2).map(|y| y / x));
        }
        gains
    }

    #[test]
    fn the_boost_limiter_should_glide_into_a_peak_ahead_of_it() {
        let gains = gain_curve(0.5, 2.0, 5, 5, 0);
        let onset = 5 * FRAME;
        // At the peak the gain is already down to what it needs ...
        let needed = BOOST_CEILING / 2.0;
        assert!(gains[onset] <= needed * 1.000_01, "{}", gains[onset]);
        // ... and it got there over the look-ahead, not in a step.
        let steepest = gains
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0f32, f32::max);
        assert!(
            steepest <= (1.0 - needed) / BOOST_WINDOW as f32 * 1.01,
            "a step of {steepest}"
        );
        assert!(gains[onset - BOOST_WINDOW - 1] == 1.0);
    }

    #[test]
    fn the_boost_limiter_should_release_smoothly_and_go_idle() {
        let mut limiter = BoostLimiter::new();
        let loud = [2.0f32; SAMPLES];
        for _ in 0..20 {
            let mut frame = loud;
            limiter.process(&mut frame, &[2.0; BOOST_LOOKAHEAD], 1.0);
        }
        assert!((limiter.reduction() - BOOST_CEILING / 2.0).abs() < 1e-3);
        // Then a quiet signal: the gain comes back on the release, not at
        // once, and never overshoots unity.
        let mut seen = vec![limiter.reduction()];
        for _ in 0..200 {
            let mut frame = [0.1f32; SAMPLES];
            limiter.process(&mut frame, &[0.1; BOOST_LOOKAHEAD], 1.0);
            seen.push(limiter.reduction());
        }
        assert!(seen.windows(2).all(|w| w[1] >= w[0] && w[1] <= 1.0));
        // One frame in: barely moved (10 ms of an 80 ms release).
        assert!(seen[1] < 0.5 && seen[1] > seen[0], "{:?}", &seen[..3]);
        // 100 ms in: most of the way back.
        assert!(seen[10] > 0.75 && seen[10] < 1.0, "{}", seen[10]);
        // Two seconds in: idle, back on the plain path.
        assert!(limiter.idle(), "{}", limiter.reduction());
    }

    #[test]
    fn a_boost_turned_off_should_return_to_the_plain_path() {
        let mixer = Mixer::new();
        let stream = mixer.add();
        let input = loud_talker(300, 8);
        let mut out = [0f32; SAMPLES];
        let mut last = Vec::new();
        for (n, frame) in input.chunks_exact(FRAME).enumerate() {
            stream.set_gain(if n < 50 { 4.0 } else { 1.0 });
            stream.push(frame, 1);
            out.fill(0.0);
            stream.mix_into(&mut out, true, None);
            if n == 299 {
                last = out.to_vec();
            }
        }
        let boosted = mixer.stats().boosted_frames;
        // The boost, plus its release (well under the 250 frames after).
        assert!((50..250).contains(&boosted), "{boosted}");
        // The last frame is the plain path again, bit for bit: 1 * (1 l + 0 r)
        // of the input frame it plays (three behind, the primed queue).
        let first = (299 + 1 - PRIME_FRAMES) * FRAME;
        let plain: Vec<f32> = input[first..first + FRAME]
            .iter()
            .flat_map(|s| {
                let l = f32::from(*s);
                [
                    0.0 + 1.0 * (1.0 * l + 0.0 * l),
                    0.0 + 1.0 * (0.0 * l + 1.0 * l),
                ]
            })
            .collect();
        assert!(last.iter().zip(&plain).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    /// The limiter's CPU per stream per 10 ms frame (printed; run with
    /// `--release --nocapture`).
    #[test]
    fn boost_limiter_cost_per_frame_should_be_tiny() {
        let input = loud_talker(500, 13);
        let mixer = Mixer::new();
        let stream = mixer.add();
        let mut out = [0f32; SAMPLES];
        let mut time = |gain: f64| {
            stream.set_gain(gain);
            let started = std::time::Instant::now();
            let mut count = 0u32;
            for frame in input.chunks_exact(FRAME) {
                stream.push(frame, 1);
                stream.mix_into(&mut out, true, None);
                count += 1;
            }
            started.elapsed() / count
        };
        let plain = time(1.0);
        let boosted = time(4.0);
        eprintln!("mix per stream per 10 ms frame: {plain:?} at 1.0, {boosted:?} at 4.0 (limiter on)");
        assert!(boosted < std::time::Duration::from_millis(2));
    }
}
