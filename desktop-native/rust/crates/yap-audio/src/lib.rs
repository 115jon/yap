//! Local playback for the native Yap client: soundboard clips, the built-in
//! soundboard tones and the one "Listen Together" stream.
//!
//! The web client plays these on each client, outside the call: clips with
//! `new Audio(url)`, tones with a Web Audio oscillator
//! (`src/lib/voice/soundboard.ts`) and the stream with an audio element
//! behind a loudness chain (`src/lib/voice/listen-together-audio.ts`,
//! `listen-together-player.ts`). This crate does the same natively.
//!
//! # Threads
//!
//! - [`Engine`] is a cheap `Clone + Send + Sync` handle. Every method only
//!   sends a command over a channel and returns at once.
//! - The engine thread (`yap-audio`), spawned on the first play or stream
//!   load, owns the mixer and the output. It renders 20 ms blocks at 48 kHz
//!   stereo `f32` and writes them to `waveOut` (four buffers, 80 ms of
//!   latency). With nothing audible for 2 s it closes the device and sleeps
//!   on the command channel. It exits when the last `Engine` clone drops.
//! - Each encoded clip is decoded on a short-lived `yap-audio-decode`
//!   thread, then handed to the engine thread.
//! - The stream is fetched and decoded on its own `yap-audio-stream` thread,
//!   which fills a PCM queue (about 3 s ahead) that the mixer drains. HTTP
//!   runs on a one-worker tokio runtime the engine owns (`yap-audio-net`).
//! - The `events` callback runs on the engine, decode and stream threads.
//!   It must not block for long.
//!
//! # Pipeline
//!
//! decode (symphonia; Opus through libopus) → stereo → resample to 48 kHz
//! (rubato FFT) → per-voice gain ramps → the stream's loudness chain → sum →
//! soft clip → 16-bit PCM → `waveOut`.
//!
//! # Approximations
//!
//! - Tones are rendered exactly as the web schedules them: a sine (pop: a
//!   band-limited triangle, as Web Audio's `PeriodicWave` builds it) at the
//!   tone's frequency, gain from the volume to 0.001 along
//!   `exponentialRampToValueAtTime`, stopped at the duration.
//!   [`Engine::set_volume`] on a tone scales its envelope; the web instead
//!   cancels the ramp and holds the new volume.
//! - The loudness chain follows Chromium's `DynamicsCompressorKernel` for
//!   the static curve: the soft knee (an exponential curve whose slope meets
//!   `1/ratio` at `threshold + knee`), the constant ratio above it and the
//!   automatic makeup gain (`(1 / curve(1.0))^0.6`) that every
//!   `DynamicsCompressorNode` applies. The detector is the per-sample peak of
//!   both channels, held at peaks and decaying with the node's release
//!   time; the gain follows it down with the attack time constant.
//!   Chromium's 6 ms
//!   look-ahead, its adaptive release curve and its 32-frame block update
//!   are not reproduced, so fast transients can overshoot by a few samples;
//!   the final soft clip catches them. Parameter changes apply at once
//!   (the web glides them over 30 ms); the gain envelope smooths the result.
//! - `max_duration_secs` counts played frames; the web's timer counts wall
//!   time from the start, paused or not.
//! - Positions report what the mixer has consumed; the ~80 ms of output
//!   buffering is not subtracted.
//! - After a seek in an Opus stream there is no 80 ms pre-roll, so the
//!   first frames after the seek may be slightly off.
//! - Channels beyond the first two are dropped; mono is copied to both.

mod decode;
mod dsp;
mod dynamics;
mod engine;
mod guard;
mod mixer;
mod opus;
mod output;
mod resample;
mod stream;
mod tone;

use std::sync::Arc;

pub use engine::Engine;
pub use guard::public_http_client;

/// The mix rate, in frames per second.
pub const SAMPLE_RATE: u32 = 48_000;

/// Decodes a whole short file (a custom UI sound) to 48 kHz stereo `f32`
/// and its length in seconds. Blocks; run it off the GUI thread.
pub fn decode_sound(bytes: &[u8], hint: Option<&str>) -> Result<(Vec<f32>, f64), String> {
    let clip = decode::decode_clip(Arc::from(bytes), hint)?;
    let seconds = clip.samples.len() as f64 / 2.0 / f64::from(SAMPLE_RATE);
    Ok((clip.samples, seconds))
}

/// Callback for engine events, called from engine threads.
pub type EventSink = Arc<dyn Fn(Event) + Send + Sync>;

/// What to play.
#[derive(Clone, Debug)]
pub enum Clip {
    /// Built-in tone by id: "ping" | "pop" | "chime" | "tada" (unknown id -> Failed event).
    Tone(String),
    /// Encoded bytes (mp3, aac/m4a, ogg vorbis, ogg/webm opus, wav, flac, webm); `hint` is an
    /// extension or MIME type when known.
    Encoded {
        bytes: Arc<[u8]>,
        hint: Option<String>,
    },
}

/// How to play a clip.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipOptions {
    /// Linear 0..=1, already composed by the caller (event volume x setting).
    pub volume: f32,
    /// Seek into the clip (web: currentTime + delivery latency). At or past the
    /// end, the clip ends at once (an `Ended` event, no `Started`).
    pub start_at_secs: f64,
    /// Load paused; `Started` fires on the first audible frame after resuming.
    pub paused: bool,
    /// Automatic join/leave clips stop after this many seconds of playback.
    /// The caller caps it (the web caps it at 3 s).
    pub max_duration_secs: Option<f64>,
}

impl Default for ClipOptions {
    fn default() -> Self {
        Self {
            volume: 1.0,
            start_at_secs: 0.0,
            paused: false,
            max_duration_secs: None,
        }
    }
}

/// Load the Listen Together stream.
#[derive(Clone)]
pub struct StreamRequest {
    /// The queue entry id; echoed in events.
    pub key: String,
    /// Absolute https URL of `/api/listen-together/stream?...`. The caller picks
    /// `format=` (prefer `mp4`); the engine switches container once on its own.
    pub url: String,
    /// Sent with every request (bearer, `X-Voice-Session-Id`). Never logged.
    pub headers: Vec<(String, String)>,
    pub position_ms: u64,
    pub paused: bool,
    pub volume: f32,
    pub loudness: Loudness,
    /// A live radio stream (`url` is the station's own): one plain GET,
    /// no Range, no seeking, `position_ms` ignored.
    pub live: bool,
}

impl std::fmt::Debug for StreamRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let header_names: Vec<&str> = self.headers.iter().map(|(name, _)| name.as_str()).collect();
        f.debug_struct("StreamRequest")
            .field("key", &self.key)
            .field("url", &stream::describe_url(&self.url))
            .field("headers", &header_names)
            .field("position_ms", &self.position_ms)
            .field("paused", &self.paused)
            .field("volume", &self.volume)
            .field("loudness", &self.loudness)
            .field("live", &self.live)
            .finish()
    }
}

/// The Listen Together loudness setting (web: `ListenTogetherAudioSettings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loudness {
    pub enabled: bool,
    pub preset: LoudnessPreset,
}

impl Default for Loudness {
    /// The web's default: on, balanced.
    fn default() -> Self {
        Self {
            enabled: true,
            preset: LoudnessPreset::Balanced,
        }
    }
}

/// The web's `LISTEN_TOGETHER_LOUDNESS_PRESETS` keys.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LoudnessPreset {
    PeakProtection,
    #[default]
    Balanced,
    Night,
}

impl LoudnessPreset {
    /// Parses the web's key: "peak-protection" | "balanced" | "night".
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "peak-protection" => Some(Self::PeakProtection),
            "balanced" => Some(Self::Balanced),
            "night" => Some(Self::Night),
            _ => None,
        }
    }

    /// The web's key.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PeakProtection => "peak-protection",
            Self::Balanced => "balanced",
            Self::Night => "night",
        }
    }
}

impl std::str::FromStr for LoudnessPreset {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value).ok_or_else(|| format!("unknown loudness preset {value:?}"))
    }
}

/// What the engine reports through the `events` callback.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// First audible frame of a clip / stream.
    Started { key: String },
    /// Clip finished, stopped, or hit max duration. Not sent when a new
    /// `play` with the same key replaces the voice.
    Ended { key: String },
    /// Clip decode/unsupported.
    Failed { key: String, error: String },
    /// ~every 250 ms while loaded.
    StreamPosition {
        key: String,
        position_ms: u64,
        paused: bool,
    },
    /// After the engine's own retries are exhausted or for non-retryable failures.
    StreamFailed { key: String, failure: StreamFailure },
    StreamRetrying {
        key: String,
        delay_ms: u64,
        message: String,
    },
    /// The stream played to its end. Not sent for `stream_stop` or a replacing load.
    StreamEnded { key: String },
    /// Output peak level 0..1 of the mixed output, at most ~10 per second, only
    /// while something plays (one final 0 when it goes quiet).
    Level(f32),
}

/// A final stream failure, with the web's code and copy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamFailure {
    /// The stream route's HTTP status, when the failure was one.
    pub status: Option<u16>,
    /// `PLAYBACK_SESSION` | `PLAYBACK_UNAVAILABLE` | `PLAYBACK_FAILED` | `PLAYBACK_SRC_NOT_SUPPORTED`.
    pub code: String,
    pub message: String,
}
