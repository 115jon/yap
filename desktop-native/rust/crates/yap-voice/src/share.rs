//! Our screen share (SCREEN_SHARE.md): what is asked for, the quality
//! presets, the capture the app provides, and the frames and audio on their
//! way into the call.
//!
//! The app captures (Windows Graphics Capture and WASAPI loopback in
//! `yap_native`) through a [`ShareCapturer`] the driver starts and releases
//! as the call machine asks. Every captured frame goes to
//! [`push_screen_frame`] on whatever thread it arrives: libyuv converts it
//! once to I420, scaled to fit the quality's box, and it goes to the share's
//! encoder while the share is live and to the local preview
//! ([`LOCAL_SCREEN_KEY`]) through the installed video output. Source audio
//! goes to [`push_screen_audio`] as 48 kHz 16-bit PCM.
//!
//! The share publishes through the SFU as the web's browser path does: a
//! `screen-video-<pid>` track (one layer, the preset's bitrate and frame
//! rate) and, when the capture carries audio, `screen-audio-<pid>`.

use std::fmt;

/// The video key our own share's preview shows under.
pub const LOCAL_SCREEN_KEY: &str = "local-screen";

/// The quality ids every share picker offers (`STREAMING_PROFILES`), in
/// order.
pub const SHARE_QUALITIES: [&str; 8] = [
    "720p30", "720p60", "1080p30", "1080p60", "1440p30", "1440p60", "4k30", "4k60",
];

/// The default share quality (`screenShareQuality`).
pub const DEFAULT_SHARE_QUALITY: &str = "720p30";

/// What is shared.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    Monitor,
    Window,
}

impl SourceKind {
    /// `monitor` or `window`, as the web names them.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Monitor => "monitor",
            Self::Window => "window",
        }
    }
}

/// One share request (`ScreenShareRequest`): the picked source, the
/// quality preset id and whether to send its audio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareRequest {
    /// `monitor-<index>` or `window-<hwnd>`, as the picker lists them.
    pub source_id: String,
    pub source_name: String,
    pub kind: SourceKind,
    /// One of [`SHARE_QUALITIES`].
    pub quality: String,
    pub audio: bool,
}

/// A preset as numbers (`getScreenQualitySettings`): the box the capture is
/// scaled to fit, the sender's max bitrate and the frame rate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareQuality {
    pub width: u32,
    pub height: u32,
    pub bitrate: u64,
    pub fps: u32,
}

impl ShareQuality {
    /// `SCREEN_QUALITY_MAP` and the fps suffix; an unknown id is the default.
    pub fn of(id: &str) -> Self {
        let fps = if id.ends_with("60") { 60 } else { 30 };
        let (width, height, bitrate) = match id.trim_end_matches(char::is_numeric) {
            "1080p" => (1920, 1080, 8_000_000),
            "1440p" => (2560, 1440, 14_000_000),
            "4k" => (3840, 2160, 24_000_000),
            "720p" => (1280, 720, 5_000_000),
            _ => return Self::of(DEFAULT_SHARE_QUALITY),
        };
        Self {
            width,
            height,
            bitrate,
            fps,
        }
    }

    /// The size a `width` x `height` capture is sent at: scaled down to fit
    /// the box with its aspect kept (a browser's `maxWidth`/`maxHeight`),
    /// never up, and even on both sides for the encoder.
    pub fn fit(&self, width: u32, height: u32) -> (u32, u32) {
        if width == 0 || height == 0 {
            return (0, 0);
        }
        let scale = (f64::from(self.width) / f64::from(width))
            .min(f64::from(self.height) / f64::from(height))
            .min(1.0);
        let even = |n: f64| ((n.round() as u32) & !1).max(2);
        (
            even(f64::from(width) * scale),
            even(f64::from(height) * scale),
        )
    }
}

/// How the share's sender encodes: the preset's caps and the content hint
/// (`window` shares are `motion`, the rest `detail`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShareEncoding {
    pub max_bitrate: u64,
    pub max_framerate: f64,
    /// `detail`: keep resolution under pressure (monitors). `motion` keeps
    /// the frame rate (windows).
    pub detail: bool,
}

impl ShareEncoding {
    pub fn of(request: &ShareRequest) -> Self {
        let quality = ShareQuality::of(&request.quality);
        Self {
            max_bitrate: quality.bitrate,
            max_framerate: f64::from(quality.fps),
            detail: request.kind == SourceKind::Monitor,
        }
    }
}

/// What a capture that started carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShareCaptured {
    /// The source's audio is captured (asked for, and the loopback started).
    pub has_audio: bool,
}

/// Our share on the SFU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharePublished {
    pub video_sfu_track: String,
    /// While the share carries audio.
    pub audio_sfu_track: Option<String>,
}

/// Why a share ended (`ScreenShareEndReason`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShareEnd {
    /// `stop()`.
    Stopped,
    /// The source went away (window closed, monitor unplugged).
    SourceEnded,
    /// A capture or publish failed.
    Failed,
}

impl fmt::Display for ShareEnd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Stopped => "stopped",
            Self::SourceEnded => "source-ended",
            Self::Failed => "failed",
        })
    }
}

/// A live share as the UI shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShareInfo {
    pub request: ShareRequest,
    /// `self_stream_audio`: what the share reports.
    pub audio: bool,
}

/// The share's events (`ScreenShareEvent`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShareNotice {
    /// Live (`self_stream`); the start sound plays.
    Started(ShareInfo),
    /// Live on a new capture, or its audio or quality changed.
    Updated(ShareInfo),
    /// Over. `announced`: it had been live (the stop sound plays only for an
    /// announced `Stopped`).
    Ended {
        reason: ShareEnd,
        announced: bool,
        error: Option<String>,
    },
}

/// Called by the capture when its source goes away by itself.
pub type CaptureEnded = Box<dyn Fn() + Send + Sync>;

/// The app's capture (`ScreenShareSources.browser`): it starts a capture of
/// a picked source and hands its frames and audio to [`push_screen_frame`]
/// and [`push_screen_audio`]. Dropping the capture releases it.
pub trait ShareCapturer: Clone + Send + Sync + 'static {
    type Capture: Send + 'static;

    /// Starts capturing `request` (blocking; the driver runs it off the
    /// async runtime). `ended` is called once if the source goes away.
    fn start(
        &self,
        request: &ShareRequest,
        ended: CaptureEnded,
    ) -> Result<(Self::Capture, ShareCaptured), String>;

    /// Applies a new quality to a running capture (its box and pacing).
    fn set_quality(&self, capture: &Self::Capture, quality: ShareQuality);
}

/// A capturer with nothing to capture: every start fails.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoCapture;

impl ShareCapturer for NoCapture {
    type Capture = ();

    fn start(&self, _: &ShareRequest, _: CaptureEnded) -> Result<((), ShareCaptured), String> {
        Err("screen capture is not available".into())
    }

    fn set_quality(&self, _: &(), _: ShareQuality) {}
}

/// One captured frame in BGRA (memory order B, G, R, A), borrowed for the
/// length of [`push_screen_frame`], with the box it is sent within.
#[derive(Debug, Clone, Copy)]
pub struct ScreenFrame<'a> {
    pub bgra: &'a [u8],
    pub stride: u32,
    pub width: u32,
    pub height: u32,
    pub quality: ShareQuality,
}

impl ScreenFrame<'_> {
    /// The buffer holds every row the size and stride read.
    pub fn is_valid(&self) -> bool {
        let (w, h, stride) = (
            self.width as usize,
            self.height as usize,
            self.stride as usize,
        );
        (1..=16_384).contains(&w)
            && (1..=16_384).contains(&h)
            && stride >= w * 4
            && self.bgra.len() >= (h - 1) * stride + w * 4
    }
}

/// Hands a captured frame to the share. `true` if it went to the encoder.
/// Any thread; never blocks on the call.
pub fn push_screen_frame(frame: &ScreenFrame<'_>) -> bool {
    #[cfg(feature = "libwebrtc")]
    {
        live::push(frame)
    }
    #[cfg(not(feature = "libwebrtc"))]
    {
        let _ = frame;
        false
    }
}

/// The share audio's format: 48 kHz stereo, 16-bit.
pub const SHARE_AUDIO_RATE: u32 = 48_000;
pub const SHARE_AUDIO_CHANNELS: u32 = 2;

/// Hands interleaved stereo 48 kHz samples to the share's audio track,
/// while it carries one. Any thread; never blocks on the call.
pub fn push_screen_audio(samples: &[i16]) {
    #[cfg(feature = "libwebrtc")]
    live::push_audio(samples);
    #[cfg(not(feature = "libwebrtc"))]
    let _ = samples;
}

/// How often a live share repeats its last frame when the source sent none
/// (Windows Graphics Capture only delivers frames that changed; a still
/// screen would otherwise send nothing and stall viewers who join).
#[cfg_attr(not(feature = "libwebrtc"), allow(dead_code))]
pub(crate) const REPEAT_AFTER: std::time::Duration = std::time::Duration::from_millis(1000);

#[cfg(feature = "libwebrtc")]
pub(crate) mod live {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, Mutex, PoisonError, RwLock};
    use std::time::Instant;

    use futures_util::FutureExt;
    use libwebrtc::audio_frame::AudioFrame;
    use libwebrtc::audio_source::native::NativeAudioSource;
    use libwebrtc::video_frame::{I420Buffer, VideoBuffer, VideoFrame, VideoRotation};
    use libwebrtc::video_source::native::NativeVideoSource;
    use tokio::task::JoinHandle;

    #[cfg(windows)]
    use crate::engine::HwEncoder;

    use super::{
        ScreenFrame, LOCAL_SCREEN_KEY, REPEAT_AFTER, SHARE_AUDIO_CHANNELS, SHARE_AUDIO_RATE,
    };
    use crate::camera::{keepalive_luma, KEEPALIVE_EVERY, KEEPALIVE_SIZE, STALE_AFTER};
    use crate::engine::I420Frame;

    extern "C" {
        fn yap_screen_to_i420(
            bgra: *const u8,
            stride: i32,
            width: i32,
            height: i32,
            dst_y: *mut u8,
            stride_y: i32,
            dst_u: *mut u8,
            stride_u: i32,
            dst_v: *mut u8,
            stride_v: i32,
            dst_width: i32,
            dst_height: i32,
        ) -> i32;
    }

    /// Where the share's frames go: libwebrtc's own encoder (VP8) through
    /// the source, or the hardware H.264 encoder, which hands its access
    /// units to an encoded source itself.
    #[derive(Clone)]
    pub(crate) enum Sink {
        Raw(NativeVideoSource),
        #[cfg(windows)]
        Hardware(Arc<HwEncoder>),
    }

    /// A shared frame as a buffer libwebrtc takes (the same frame goes to
    /// the encoder, and later to the repeater).
    struct Shared(Arc<I420Buffer>);

    impl AsRef<dyn VideoBuffer> for Shared {
        fn as_ref(&self) -> &(dyn VideoBuffer + 'static) {
            &*self.0
        }
    }

    impl Sink {
        /// Hands a frame on. `true` if it was taken.
        pub(crate) fn send(&self, frame: &Arc<I420Buffer>) -> bool {
            match self {
                Self::Raw(source) => source.capture_frame(&VideoFrame {
                    rotation: VideoRotation::VideoRotation0,
                    timestamp_us: 0,
                    frame_metadata: None,
                    buffer: Shared(Arc::clone(frame)),
                }),
                #[cfg(windows)]
                Self::Hardware(encoder) => encoder.submit(Arc::clone(frame)),
            }
        }

        /// The parked share's black frame: libwebrtc's encoders take any
        /// size; a hardware one is kept at a size every vendor's MFT opens.
        fn keepalive_size(&self) -> (u32, u32) {
            match self {
                Self::Raw(_) => KEEPALIVE_SIZE,
                #[cfg(windows)]
                Self::Hardware(_) => HARDWARE_KEEPALIVE_SIZE,
            }
        }
    }

    /// The parked hardware share's black frame size.
    #[cfg(windows)]
    const HARDWARE_KEEPALIVE_SIZE: (u32, u32) = (320, 180);

    /// The live share's sink; `None` while no share is live.
    static SINK: RwLock<Option<Sink>> = RwLock::new(None);
    static LIVE: AtomicBool = AtomicBool::new(false);
    /// The last frame sent, for the repeater.
    static LAST: Mutex<Option<(Instant, Arc<I420Buffer>)>> = Mutex::new(None);
    /// The share's audio source while it carries audio, and the samples not
    /// yet a whole 10 ms frame.
    static AUDIO: Mutex<Option<(NativeAudioSource, Vec<i16>)>> = Mutex::new(None);

    pub(crate) fn set_sink(source: Option<Sink>) {
        let mut sink = SINK.write().unwrap_or_else(PoisonError::into_inner);
        LIVE.store(source.is_some(), Ordering::Release);
        *sink = source;
        if sink.is_none() {
            LAST.lock().unwrap_or_else(PoisonError::into_inner).take();
        }
    }

    pub(crate) fn set_audio(source: Option<NativeAudioSource>) {
        *AUDIO.lock().unwrap_or_else(PoisonError::into_inner) =
            source.map(|s| (s, Vec::with_capacity(SAMPLES_10MS * 2)));
    }

    fn int(n: u32) -> i32 {
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    pub(crate) fn push(frame: &ScreenFrame<'_>) -> bool {
        if !frame.is_valid() {
            return false;
        }
        let (w, h) = frame.quality.fit(frame.width, frame.height);
        let mut buffer = I420Buffer::new(w, h);
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data_mut();
        // SAFETY: `is_valid` checked the source rows against its size and
        // stride; the destination planes are the buffer's own, sized for
        // `w` x `h`. libyuv reads and writes nothing else and keeps no
        // pointer.
        let converted = unsafe {
            yap_screen_to_i420(
                frame.bgra.as_ptr(),
                int(frame.stride),
                int(frame.width),
                int(frame.height),
                y.as_mut_ptr(),
                int(sy),
                u.as_mut_ptr(),
                int(su),
                v.as_mut_ptr(),
                int(sv),
                int(w),
                int(h),
            )
        };
        if converted != 0 {
            return false;
        }
        preview(&buffer);
        if !LIVE.load(Ordering::Acquire) {
            return false;
        }
        // The frame is kept for the repeater under the sink's read guard:
        // `set_sink` clears it under the write guard, so a push racing a
        // stop can never leave the stopped share's frame for the next one.
        let sink = SINK.read().unwrap_or_else(PoisonError::into_inner);
        let Some(sink) = sink.as_ref() else {
            return false;
        };
        let frame = Arc::new(buffer);
        let sent = sink.send(&frame);
        *LAST.lock().unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), frame));
        sent
    }

    /// Our own share's tile.
    fn preview(buffer: &I420Buffer) {
        let Some(output) = crate::engine::video_output() else {
            return;
        };
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data();
        output.frame(
            LOCAL_SCREEN_KEY,
            &I420Frame {
                width: buffer.width(),
                height: buffer.height(),
                rotation: 0,
                y,
                u,
                v,
                stride_y: sy,
                stride_u: su,
                stride_v: sv,
            },
        );
    }

    const SAMPLES_10MS: usize = (SHARE_AUDIO_RATE / 100 * SHARE_AUDIO_CHANNELS) as usize;

    pub(crate) fn push_audio(samples: &[i16]) {
        let mut audio = AUDIO.lock().unwrap_or_else(PoisonError::into_inner);
        let Some((source, pending)) = audio.as_mut() else {
            return;
        };
        pending.extend_from_slice(samples);
        let whole = pending.len() / SAMPLES_10MS * SAMPLES_10MS;
        for chunk in pending[..whole].chunks_exact(SAMPLES_10MS) {
            let frame = AudioFrame {
                data: chunk.into(),
                sample_rate: SHARE_AUDIO_RATE,
                num_channels: SHARE_AUDIO_CHANNELS,
                samples_per_channel: SHARE_AUDIO_RATE / 100,
            };
            // Unbuffered: a 10 ms frame goes straight to the track's sinks
            // and the future is ready at once.
            if let Some(Err(error)) = source.capture_frame(&frame).now_or_never() {
                eprintln!("[share] audio frame refused: {}", error.message);
            }
        }
        pending.drain(..whole);
    }

    /// The share audio's publication between shares: parked, its source
    /// gets 10 ms of silence every 10 ms (the track is disabled, so it
    /// sends muted frames), so the SFU keeps the track and the next share
    /// with audio resumes it on the same transceiver.
    pub(crate) struct AudioKeepalive {
        parked: Arc<AtomicBool>,
        last_ms: Arc<AtomicU64>,
        longest_gap_ms: Arc<AtomicU64>,
        epoch: Instant,
        task: JoinHandle<()>,
    }

    impl AudioKeepalive {
        /// Starts live (the capture feeds the source).
        pub(crate) fn start(source: NativeAudioSource) -> Self {
            let parked = Arc::new(AtomicBool::new(false));
            let last_ms = Arc::new(AtomicU64::new(0));
            let longest_gap_ms = Arc::new(AtomicU64::new(0));
            let epoch = Instant::now();
            let task = tokio::spawn(run_audio(
                source,
                Arc::clone(&parked),
                Arc::clone(&last_ms),
                Arc::clone(&longest_gap_ms),
                epoch,
            ));
            Self {
                parked,
                last_ms,
                longest_gap_ms,
                epoch,
                task,
            }
        }

        pub(crate) fn park(&self, parked: bool) {
            if parked {
                self.longest_gap_ms.store(0, Ordering::Relaxed);
                self.last_ms.store(millis(self.epoch), Ordering::Relaxed);
            }
            self.parked.store(parked, Ordering::Relaxed);
        }

        /// Parked, and silent long enough that the SFU may have dropped it.
        pub(crate) fn stale(&self) -> bool {
            let open = millis(self.epoch).saturating_sub(self.last_ms.load(Ordering::Relaxed));
            let gap = self.longest_gap_ms.load(Ordering::Relaxed).max(open);
            self.parked.load(Ordering::Relaxed) && u128::from(gap) >= STALE_AFTER.as_millis()
        }
    }

    impl Drop for AudioKeepalive {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn run_audio(
        source: NativeAudioSource,
        parked: Arc<AtomicBool>,
        last_ms: Arc<AtomicU64>,
        longest_gap_ms: Arc<AtomicU64>,
        epoch: Instant,
    ) {
        let silence = vec![0i16; SAMPLES_10MS];
        let mut ticks = tokio::time::interval(std::time::Duration::from_millis(10));
        // The timer is coarser than 10 ms on Windows: catch up in bursts so
        // the encoder sees real time.
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        loop {
            ticks.tick().await;
            if !parked.load(Ordering::Relaxed) {
                continue;
            }
            let frame = AudioFrame {
                data: silence.as_slice().into(),
                sample_rate: SHARE_AUDIO_RATE,
                num_channels: SHARE_AUDIO_CHANNELS,
                samples_per_channel: SHARE_AUDIO_RATE / 100,
            };
            // Unbuffered: ready at once (see `push_audio`).
            let _ = source.capture_frame(&frame).now_or_never();
            let now = millis(epoch);
            let last = last_ms.swap(now, Ordering::Relaxed);
            longest_gap_ms.fetch_max(now.saturating_sub(last), Ordering::Relaxed);
        }
    }

    /// The share's publication between shares (SCREEN_SHARE.md "Keepalive
    /// park and reuse"): parked, it sends a 64x36 black frame each second;
    /// live, it repeats the last frame once the source has been still for
    /// [`REPEAT_AFTER`].
    pub(crate) struct Refresher {
        parked: Arc<AtomicBool>,
        longest_gap_ms: Arc<AtomicU64>,
        last_ms: Arc<AtomicU64>,
        epoch: Instant,
        task: JoinHandle<()>,
    }

    fn millis(since: Instant) -> u64 {
        u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    impl Refresher {
        /// Starts parked.
        pub(crate) fn start(sink: Sink) -> Self {
            let parked = Arc::new(AtomicBool::new(true));
            let longest_gap_ms = Arc::new(AtomicU64::new(0));
            let last_ms = Arc::new(AtomicU64::new(0));
            let epoch = Instant::now();
            let task = tokio::spawn(run(
                sink,
                Arc::clone(&parked),
                Arc::clone(&longest_gap_ms),
                Arc::clone(&last_ms),
                epoch,
            ));
            Self {
                parked,
                longest_gap_ms,
                last_ms,
                epoch,
                task,
            }
        }

        pub(crate) fn park(&self, parked: bool) {
            if parked {
                self.longest_gap_ms.store(0, Ordering::Relaxed);
                self.last_ms.store(millis(self.epoch), Ordering::Relaxed);
            }
            self.parked.store(parked, Ordering::Relaxed);
        }

        /// Parked, and a gap long enough that the SFU may have dropped the
        /// track.
        pub(crate) fn stale(&self) -> bool {
            let open = millis(self.epoch).saturating_sub(self.last_ms.load(Ordering::Relaxed));
            let gap = self.longest_gap_ms.load(Ordering::Relaxed).max(open);
            self.parked.load(Ordering::Relaxed) && u128::from(gap) >= STALE_AFTER.as_millis()
        }
    }

    impl Drop for Refresher {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn run(
        sink: Sink,
        parked: Arc<AtomicBool>,
        longest_gap_ms: Arc<AtomicU64>,
        last_ms: Arc<AtomicU64>,
        epoch: Instant,
    ) {
        let mut ticks = tokio::time::interval(KEEPALIVE_EVERY.min(REPEAT_AFTER) / 2);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut odd = false;
        let mut last_black = Instant::now();
        loop {
            ticks.tick().await;
            if !parked.load(Ordering::Relaxed) {
                last_ms.store(millis(epoch), Ordering::Relaxed);
                let mut last = LAST.lock().unwrap_or_else(PoisonError::into_inner);
                if let Some((at, frame)) = last.as_mut() {
                    if at.elapsed() >= REPEAT_AFTER {
                        sink.send(frame);
                        *at = Instant::now();
                    }
                }
                continue;
            }
            if last_black.elapsed() < KEEPALIVE_EVERY {
                continue;
            }
            last_black = Instant::now();
            let now = millis(epoch);
            let last = last_ms.swap(now, Ordering::Relaxed);
            longest_gap_ms.fetch_max(now.saturating_sub(last), Ordering::Relaxed);
            odd = !odd;
            let (w, h) = sink.keepalive_size();
            let mut buffer = I420Buffer::new(w, h);
            let (y, u, v) = buffer.data_mut();
            y.fill(keepalive_luma(odd));
            u.fill(128);
            v.fill(128);
            sink.send(&Arc::new(buffer));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_should_match_the_web_quality_map() {
        assert_eq!(
            ShareQuality::of("1080p60"),
            ShareQuality {
                width: 1920,
                height: 1080,
                bitrate: 8_000_000,
                fps: 60
            }
        );
        assert_eq!(ShareQuality::of("4k30").bitrate, 24_000_000);
    }

    #[test]
    fn an_unknown_preset_should_be_720p30() {
        assert_eq!(ShareQuality::of("8k120"), ShareQuality::of("720p30"));
    }

    #[test]
    fn a_larger_capture_should_fit_the_box_with_its_aspect() {
        let q = ShareQuality::of("1080p30");
        assert_eq!(q.fit(2560, 1440), (1920, 1080));
        assert_eq!(q.fit(3440, 1440), (1920, 804));
    }

    #[test]
    fn a_smaller_capture_should_never_be_scaled_up() {
        assert_eq!(ShareQuality::of("4k60").fit(1281, 721), (1280, 720));
    }

    #[test]
    fn window_shares_should_hint_motion_and_monitors_detail() {
        let mut request = ShareRequest {
            source_id: "window-1".into(),
            source_name: "Doc".into(),
            kind: SourceKind::Window,
            quality: "1080p30".into(),
            audio: false,
        };
        assert!(!ShareEncoding::of(&request).detail);
        request.kind = SourceKind::Monitor;
        assert!(ShareEncoding::of(&request).detail);
    }

    #[test]
    fn a_frame_should_need_every_row() {
        let data = [0u8; 16 * 2];
        let frame = |len: usize| ScreenFrame {
            bgra: &data[..len],
            stride: 16,
            width: 4,
            height: 2,
            quality: ShareQuality::of("720p30"),
        };
        assert!(frame(32).is_valid());
        assert!(!frame(31).is_valid());
    }
}
