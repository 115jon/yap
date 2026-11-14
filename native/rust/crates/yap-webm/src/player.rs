//! The player thread: fetch, demux, then decode and pace frames.
//!
//! One thread per player. It fetches the bytes (the caller's closure, which
//! caps the size), demuxes them, shows the first frame at once (the poster
//! a paused player holds), then, while playing, decodes each packet and
//! hands its picture to the [`Sink`] when it is due by the wall clock.
//! Late frames (more than [`LATE_MS`] behind) are decoded but not shown,
//! so a slow machine drops frames instead of drifting. A looping player
//! starts over after the last frame's duration.
//!
//! Pictures larger than the player's box are scaled down (never up) before
//! they leave the thread: an opaque stream as I420 (the call video path's
//! format), an alpha stream as premultiplied BGRA.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::demux::{demux, Video};
use crate::WebmError;

/// Frames this far behind their time are skipped.
pub const LATE_MS: u64 = 250;
/// Streams larger than this are refused (4K UHD).
pub const MAX_PIXELS: u64 = 3840 * 2160;
/// A looping clip takes at least this long a round, so a zero or tiny
/// duration cannot spin the decoder.
pub const MIN_LOOP_MS: u64 = 33;

/// Whether a picture of this size is within [`MAX_PIXELS`].
pub(crate) fn within_pixel_limit(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// How long a looping player waits, from the clock's base position, before
/// it starts over.
#[cfg_attr(not(feature = "libvpx"), allow(dead_code))]
fn loop_wait(duration_ms: u64, base_ms: u64) -> Duration {
    Duration::from_millis(duration_ms.saturating_sub(base_ms).max(MIN_LOOP_MS))
}

/// A picture for the sink; the slices are valid for the call only.
#[derive(Debug)]
pub enum Frame<'a> {
    I420 {
        width: u32,
        height: u32,
        y: &'a [u8],
        stride_y: u32,
        u: &'a [u8],
        stride_u: u32,
        v: &'a [u8],
        stride_v: u32,
    },
    /// Premultiplied, B G R A in memory.
    Bgra {
        width: u32,
        height: u32,
        data: &'a [u8],
        stride: u32,
    },
}

/// What a player reports besides frames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Demuxed and decoding: the first frame follows.
    Loaded {
        duration_ms: u64,
        width: u32,
        height: u32,
        alpha: bool,
    },
    /// The last frame showed and the player does not loop.
    Ended,
    /// Nothing (more) will play.
    Failed(String),
}

/// Where frames and events go. Called on the player thread.
pub trait Sink: Send + 'static {
    fn frame(&self, frame: &Frame<'_>, pts_ms: u64);
    fn event(&self, event: Event);
}

/// How a player starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// The largest picture to hand out (0: no bound).
    pub max_width: u32,
    pub max_height: u32,
    pub looping: bool,
    pub playing: bool,
    pub start_ms: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            max_width: 1920,
            max_height: 1080,
            looping: false,
            playing: false,
            start_ms: 0,
        }
    }
}

/// `width x height` scaled to fit `max_w x max_h` (0: unbounded), never
/// up, kept even (I420 chroma) and at least 2 x 2.
pub fn fit_within(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width, height);
    }
    let mut scale = 1.0_f64;
    if max_w > 0 {
        scale = scale.min(f64::from(max_w) / f64::from(width));
    }
    if max_h > 0 {
        scale = scale.min(f64::from(max_h) / f64::from(height));
    }
    if scale >= 1.0 {
        return (width, height);
    }
    let even = |v: f64| ((v.round() as u32) & !1).max(2);
    (
        even(f64::from(width) * scale),
        even(f64::from(height) * scale),
    )
}

enum Command {
    Play,
    Pause,
    Seek(u64),
    Looping(bool),
    MaxSize(u32, u32),
    Stop,
}

/// A running player; dropping it stops the thread (it exits at its next
/// wake-up, without being joined).
pub struct Player {
    commands: Sender<Command>,
}

impl Player {
    /// Starts a player thread. `source` runs on it and returns the whole
    /// file (or why it could not).
    pub fn spawn<F, S>(source: F, sink: S, options: Options) -> std::io::Result<Self>
    where
        F: FnOnce() -> Result<Arc<[u8]>, String> + Send + 'static,
        S: Sink,
    {
        let (commands, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("yap-webm".into())
            .spawn(move || {
                // A bug in the decoder path ends this player, not the app:
                // the UI falls back to "cannot be played".
                let guarded = catch_unwind(AssertUnwindSafe(|| run(source, &sink, options, &rx)));
                if guarded.is_err() {
                    sink.event(Event::Failed(WebmError::Crashed.to_string()));
                }
            })?;
        Ok(Self { commands })
    }

    pub fn play(&self) {
        let _ = self.commands.send(Command::Play);
    }

    pub fn pause(&self) {
        let _ = self.commands.send(Command::Pause);
    }

    pub fn seek(&self, position_ms: u64) {
        let _ = self.commands.send(Command::Seek(position_ms));
    }

    pub fn set_looping(&self, looping: bool) {
        let _ = self.commands.send(Command::Looping(looping));
    }

    pub fn set_max_size(&self, width: u32, height: u32) {
        let _ = self.commands.send(Command::MaxSize(width, height));
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Stop);
    }
}

/// Applies the commands that came while fetching; `false` on stop.
fn drain(rx: &Receiver<Command>, options: &mut Options, seek: &mut Option<u64>) -> bool {
    while let Ok(command) = rx.try_recv() {
        match command {
            Command::Stop => return false,
            Command::Play => options.playing = true,
            Command::Pause => options.playing = false,
            Command::Seek(ms) => *seek = Some(ms),
            Command::Looping(looping) => options.looping = looping,
            Command::MaxSize(w, h) => {
                options.max_width = w;
                options.max_height = h;
            }
        }
    }
    true
}

fn run<F, S>(source: F, sink: &S, mut options: Options, rx: &Receiver<Command>)
where
    F: FnOnce() -> Result<Arc<[u8]>, String>,
    S: Sink,
{
    let bytes = match source() {
        Ok(bytes) => bytes,
        Err(error) => {
            sink.event(Event::Failed(WebmError::Fetch(error).to_string()));
            return;
        }
    };
    let mut seek = (options.start_ms > 0).then_some(options.start_ms);
    if !drain(rx, &mut options, &mut seek) {
        return;
    }
    let video = match demux(&bytes) {
        Ok(video) => video,
        Err(error) => {
            sink.event(Event::Failed(error.to_string()));
            return;
        }
    };
    drop(bytes);
    if !within_pixel_limit(video.width, video.height) {
        sink.event(Event::Failed(
            WebmError::Decode("picture too large".into()).to_string(),
        ));
        return;
    }
    play(&video, sink, options, seek, rx);
}

#[cfg(not(feature = "libvpx"))]
fn play<S: Sink>(_: &Video, sink: &S, _: Options, _: Option<u64>, _: &Receiver<Command>) {
    sink.event(Event::Failed(WebmError::Unsupported.to_string()));
}

#[cfg(feature = "libvpx")]
fn play<S: Sink>(
    video: &Video,
    sink: &S,
    options: Options,
    seek: Option<u64>,
    rx: &Receiver<Command>,
) {
    let mut engine = match engine::Engine::new(video, options.max_width, options.max_height) {
        Ok(engine) => engine,
        Err(error) => {
            sink.event(Event::Failed(error.to_string()));
            return;
        }
    };
    sink.event(Event::Loaded {
        duration_ms: video.duration_ms,
        width: video.width,
        height: video.height,
        alpha: video.alpha,
    });
    let count = video.packets.len();
    let mut playing = options.playing;
    let mut looping = options.looping;
    // The next packet to decode, and the wall time it maps to.
    let mut next = engine.seek(sink, seek.unwrap_or(0));
    if engine.failing() {
        sink.event(Event::Failed(engine.failure(None)));
        return;
    }
    let mut clock: Option<(Instant, u64)> = None;
    loop {
        if playing && next >= count {
            if looping {
                // Start over once the last frame has had its time.
                let end = clock.map_or_else(Instant::now, |(wall, base)| {
                    wall + loop_wait(video.duration_ms, base)
                });
                clock = Some((end, 0));
                next = 0;
            } else {
                playing = false;
                sink.event(Event::Ended);
            }
        }
        let wait = if playing {
            let pts = video.packets[next].pts_ms;
            let (wall, base) = *clock.get_or_insert((Instant::now(), pts));
            let due = wall + Duration::from_millis(pts.saturating_sub(base));
            Some(due.saturating_duration_since(Instant::now()))
        } else {
            None
        };
        let command = match wait {
            Some(wait) if wait.is_zero() => None,
            Some(wait) => match rx.recv_timeout(wait) {
                Ok(command) => Some(command),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
            None => match rx.recv() {
                Ok(command) => Some(command),
                Err(_) => return,
            },
        };
        if let Some(command) = command {
            match command {
                Command::Stop => return,
                Command::Play if !playing => {
                    playing = true;
                    clock = None;
                    if next >= count {
                        next = engine.seek(sink, 0);
                    }
                }
                Command::Play => {}
                Command::Pause => playing = false,
                Command::Seek(ms) => {
                    next = engine.seek(sink, ms);
                    clock = None;
                }
                Command::Looping(on) => looping = on,
                Command::MaxSize(w, h) => engine.set_max_size(w, h),
            }
            continue;
        }
        let packet = &video.packets[next];
        let late = clock.is_some_and(|(wall, base)| {
            let at = wall + Duration::from_millis(packet.pts_ms.saturating_sub(base));
            Instant::now().saturating_duration_since(at) > Duration::from_millis(LATE_MS)
        });
        match engine.decode(next) {
            Ok(()) if !late => engine.present(sink, packet.pts_ms),
            Ok(()) => {}
            Err(error) => {
                if engine.failing() {
                    sink.event(Event::Failed(engine.failure(Some(error))));
                    return;
                }
            }
        }
        // An inter frame can resize past the limit.
        if engine.oversized() {
            sink.event(Event::Failed(engine.failure(None)));
            return;
        }
        next += 1;
    }
}

#[cfg(feature = "libvpx")]
mod engine {
    use super::{fit_within, Frame, Sink};
    use crate::demux::Video;
    use crate::vpx::{
        alpha_to_bgra, gbr_to_bgra, scale_bgra, scale_i420, scale_plane, to_i420, Decoder,
        OwnedI420,
    };
    use crate::WebmError;

    /// Packets in a row that may fail before the player gives up.
    const MAX_ERRORS: u32 = 8;

    pub(super) struct Engine<'v> {
        video: &'v Video,
        main: Decoder,
        alpha: Option<Decoder>,
        max: (u32, u32),
        errors: u32,
        scaled: OwnedI420,
        converted: OwnedI420,
        scaled_alpha: Vec<u8>,
        opaque: Vec<u8>,
        bgra: Vec<u8>,
        rgb: Vec<u8>,
    }

    impl<'v> Engine<'v> {
        pub(super) fn new(video: &'v Video, max_w: u32, max_h: u32) -> Result<Self, WebmError> {
            let pixels = u64::from(video.width) * u64::from(video.height);
            let threads = if pixels > 1280 * 720 {
                4
            } else if pixels > 640 * 360 {
                2
            } else {
                1
            };
            Ok(Self {
                video,
                main: Decoder::new(video.codec, threads)?,
                alpha: if video.alpha {
                    Some(Decoder::new(video.codec, 1)?)
                } else {
                    None
                },
                max: (max_w, max_h),
                errors: 0,
                scaled: OwnedI420::default(),
                converted: OwnedI420::default(),
                scaled_alpha: Vec::new(),
                opaque: Vec::new(),
                bgra: Vec::new(),
                rgb: Vec::new(),
            })
        }

        pub(super) fn set_max_size(&mut self, width: u32, height: u32) {
            self.max = (width, height);
        }

        pub(super) fn failing(&self) -> bool {
            self.errors >= MAX_ERRORS || self.main.unsupported || self.oversized()
        }

        /// A stream went past the pixel limit.
        pub(super) fn oversized(&self) -> bool {
            self.main.oversized || self.alpha.as_ref().is_some_and(|a| a.oversized)
        }

        /// Why the player gives up: the last error, unless the picture was
        /// too large.
        pub(super) fn failure(&self, error: Option<WebmError>) -> String {
            if self.oversized() {
                WebmError::Decode("picture too large".into()).to_string()
            } else {
                error
                    .unwrap_or_else(|| WebmError::Decode("unsupported picture format".into()))
                    .to_string()
            }
        }

        pub(super) fn decode(&mut self, index: usize) -> Result<(), WebmError> {
            let video = self.video;
            let packet = &video.packets[index];
            let result = self.main.decode(&packet.data).and_then(|()| {
                match (&mut self.alpha, &packet.alpha) {
                    (Some(decoder), Some(alpha)) => decoder.decode(alpha),
                    _ => Ok(()),
                }
            });
            match &result {
                Ok(()) => self.errors = 0,
                Err(_) => self.errors += 1,
            }
            result
        }

        /// Decodes from the keyframe before `ms` up to it, shows that
        /// frame and returns the next packet to decode.
        pub(super) fn seek<S: Sink>(&mut self, sink: &S, ms: u64) -> usize {
            let video = self.video;
            let packets = &video.packets;
            let mut index = video.keyframe_before(ms);
            let mut shown = None;
            while index < packets.len() && (shown.is_none() || packets[index].pts_ms <= ms) {
                if self.decode(index).is_ok() {
                    shown = Some(packets[index].pts_ms);
                }
                index += 1;
            }
            if let Some(pts) = shown {
                self.present(sink, pts);
            }
            index
        }

        /// Hands the last decoded picture to the sink, scaled to the box.
        pub(super) fn present<S: Sink>(&mut self, sink: &S, pts_ms: u64) {
            let Some(raw) = self.main.frame() else {
                return;
            };
            if raw.srgb {
                // VP9 RGB: straight to BGRA (opaque), then to the box.
                let (w, h) = fit_within(raw.width, raw.height, self.max.0, self.max.1);
                gbr_to_bgra(raw, &mut self.rgb);
                let data = if (w, h) == (raw.width, raw.height) {
                    &self.rgb
                } else {
                    scale_bgra(&self.rgb, raw.width, raw.height, w, h, &mut self.bgra);
                    &self.bgra
                };
                sink.frame(
                    &Frame::Bgra {
                        width: w,
                        height: h,
                        data,
                        stride: w * 4,
                    },
                    pts_ms,
                );
                return;
            }
            // 4:2:2 / 4:4:4 / 4:4:0: to 4:2:0 first.
            let planes = if raw.is_420() {
                raw
            } else {
                to_i420(raw, &mut self.converted);
                self.converted.planes()
            };
            let (w, h) = fit_within(planes.width, planes.height, self.max.0, self.max.1);
            let scaled = (w, h) != (planes.width, planes.height);
            let Some(alpha_decoder) = self.alpha.as_mut() else {
                if scaled {
                    scale_i420(planes, w, h, &mut self.scaled);
                }
                let p = if scaled { self.scaled.planes() } else { planes };
                sink.frame(
                    &Frame::I420 {
                        width: p.width,
                        height: p.height,
                        y: p.y,
                        stride_y: p.stride_y,
                        u: p.u,
                        stride_u: p.stride_u,
                        v: p.v,
                        stride_v: p.stride_v,
                    },
                    pts_ms,
                );
                return;
            };
            // Alpha: the side frame's luma, at the picture's size.
            let (aw, ah) = (planes.width, planes.height);
            let (alpha, alpha_stride) = match alpha_decoder.frame() {
                Some(a) if (a.width, a.height) == (aw, ah) && !scaled => (a.y, a.stride_y),
                Some(a) => {
                    scale_plane(
                        a.y,
                        a.stride_y,
                        a.width,
                        a.height,
                        w,
                        h,
                        &mut self.scaled_alpha,
                    );
                    (&self.scaled_alpha[..], w)
                }
                None => {
                    self.opaque.clear();
                    self.opaque.resize((w * h) as usize, 255);
                    (&self.opaque[..], w)
                }
            };
            if scaled {
                scale_i420(planes, w, h, &mut self.scaled);
                alpha_to_bgra(self.scaled.planes(), alpha, alpha_stride, &mut self.bgra);
            } else {
                alpha_to_bgra(planes, alpha, alpha_stride, &mut self.bgra);
            }
            sink.frame(
                &Frame::Bgra {
                    width: w,
                    height: h,
                    data: &self.bgra,
                    stride: w * 4,
                },
                pts_ms,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn a_small_picture_should_keep_its_size() {
        assert_eq!(fit_within(448, 84, 1920, 1080), (448, 84));
    }

    #[test]
    fn a_large_picture_should_fit_its_box() {
        assert_eq!(fit_within(3840, 2160, 1280, 720), (1280, 720));
    }

    #[test]
    fn a_scaled_size_should_stay_even() {
        assert_eq!(fit_within(1001, 501, 500, 500), (500, 250));
    }

    #[test]
    fn no_bound_should_never_scale() {
        assert_eq!(fit_within(5000, 100, 0, 0), (5000, 100));
    }

    #[test]
    fn a_pixel_count_over_4k_should_be_refused() {
        assert!(within_pixel_limit(3840, 2160) && !within_pixel_limit(3841, 2160));
        // The product of two large sides does not wrap.
        assert!(!within_pixel_limit(u32::MAX, u32::MAX));
    }

    #[test]
    fn a_zero_duration_loop_should_still_wait() {
        assert_eq!(loop_wait(0, 0), Duration::from_millis(MIN_LOOP_MS));
        assert_eq!(loop_wait(1, 0), Duration::from_millis(MIN_LOOP_MS));
        assert_eq!(loop_wait(500, 600), Duration::from_millis(MIN_LOOP_MS));
    }

    #[test]
    fn a_normal_loop_should_wait_out_the_clip() {
        assert_eq!(loop_wait(600, 100), Duration::from_millis(500));
    }

    #[derive(Default)]
    struct Events(Arc<Mutex<Vec<Event>>>);

    impl Sink for Events {
        fn frame(&self, _: &Frame<'_>, _: u64) {}
        fn event(&self, event: Event) {
            if let Ok(mut events) = self.0.lock() {
                events.push(event);
            }
        }
    }

    fn first_event(source: Result<Arc<[u8]>, String>) -> Event {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Events(Arc::clone(&events));
        let _player = Player::spawn(move || source, sink, Options::default()).expect("spawn");
        for _ in 0..200 {
            if let Some(event) = events.lock().ok().and_then(|e| e.first().cloned()) {
                return event;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no event");
    }

    #[test]
    fn a_failed_fetch_should_say_why() {
        assert_eq!(
            first_event(Err("HTTP 404".into())),
            Event::Failed("HTTP 404".into())
        );
    }

    #[test]
    fn a_panicking_player_should_say_it_failed() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Events(Arc::clone(&events));
        let source = || -> Result<Arc<[u8]>, String> { panic!("boom") };
        let _player = Player::spawn(source, sink, Options::default()).expect("spawn");
        for _ in 0..200 {
            if let Some(event) = events.lock().ok().and_then(|e| e.first().cloned()) {
                assert_eq!(event, Event::Failed(WebmError::Crashed.to_string()));
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no event");
    }

    #[test]
    fn bytes_that_are_not_webm_should_fail() {
        let event = first_event(Ok(Arc::from(&b"<html>nope</html>"[..])));
        assert!(matches!(event, Event::Failed(text) if text.starts_with("not a WebM")));
    }

    #[cfg(not(feature = "libvpx"))]
    #[test]
    fn without_libvpx_a_webm_should_fail_as_unsupported() {
        let bytes: Arc<[u8]> = Arc::from(&include_bytes!("../tests/fixtures/opaque-vp8.webm")[..]);
        assert_eq!(
            first_event(Ok(bytes)),
            Event::Failed(WebmError::Unsupported.to_string())
        );
    }
}
