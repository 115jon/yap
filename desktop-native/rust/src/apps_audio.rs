//! The local player for the voice apps: the store's [`AudioCommand`]s run on
//! a [`yap_audio::Engine`] (soundboard clips and tones, the Listen Together
//! stream), and its reports go back as [`AudioEvent`]s.
//!
//! As on the web, a clip's own volume is multiplied by the soundboard
//! volume setting (`setSoundboardMasterVolume`); changing the setting
//! applies to clips already playing. Clip audio on our API is fetched with
//! the session (bearer header, never a URL token) and kept in a small cache;
//! the session goes to our own origin only.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Instant;

use tokio::sync::mpsc;
use yap_audio::{Clip, ClipOptions, Engine, Event, Loudness, LoudnessPreset, StreamRequest};
use yap_core::store::apps::{AudioCommand, AudioEvent, ClipSource};
use yap_core::{config, AudioEvents, AudioHandle, Token};
use yap_protocol::voice_app as wire;

/// Clips whose bytes stay cached (the picker replays the same few).
const CACHE_CLIPS: usize = 24;
/// ...and at most this many bytes in all.
const CACHE_BYTES: usize = 64 * 1024 * 1024;
/// The largest clip fetched for playback. Uploads may be bigger (50 MB),
/// but a clip that size decodes to far more than the soundboard plays.
const MAX_FETCH_BYTES: usize = 16 * 1024 * 1024;

/// The output meter, as `f64` bits (`NaN`: nothing plays).
static LEVEL: AtomicU64 = AtomicU64::new(f64::NAN.to_bits());

/// The meter's last reading (0 when nothing plays).
pub fn level() -> f64 {
    let level = f64::from_bits(LEVEL.load(Ordering::Relaxed));
    if level.is_nan() {
        0.0
    } else {
        level
    }
}

#[derive(Default)]
struct Glue {
    engine: Option<Engine>,
    /// The soundboard volume setting, 0..=1.
    master: Option<f64>,
    output: Option<String>,
    /// Each clip's own volume (before the setting).
    raw: HashMap<String, f64>,
    /// Bumped per play; a fetch that finishes for an older one is dropped.
    /// An entry goes when its clip ends, fails or is stopped.
    generation: HashMap<String, u64>,
    /// The generation of the fetch in flight for a key (its entry stays
    /// until the fetch is over, so a replaced clip's end cannot drop it).
    fetching: HashMap<String, u64>,
    next: u64,
    cache: ClipCache,
}

impl Glue {
    /// A clip is over: its volume and generation are not needed.
    fn forget(&mut self, key: &str) {
        self.raw.remove(key);
        if !self.fetching.contains_key(key) {
            self.generation.remove(key);
        }
    }
}

/// A fetched clip: its bytes and `Content-Type`.
#[derive(Clone)]
struct Fetched {
    bytes: Arc<[u8]>,
    content_type: Option<String>,
}

/// Recently fetched clips, bounded by count and by total bytes.
#[derive(Default)]
struct ClipCache {
    entries: VecDeque<(String, Fetched)>,
    bytes: usize,
}

impl ClipCache {
    fn get(&self, url: &str) -> Option<Fetched> {
        self.entries
            .iter()
            .find(|(cached, _)| cached == url)
            .map(|(_, fetched)| fetched.clone())
    }

    fn insert(&mut self, url: &str, fetched: Fetched) {
        if fetched.bytes.len() > CACHE_BYTES {
            return;
        }
        if let Some(at) = self.entries.iter().position(|(cached, _)| cached == url) {
            if let Some((_, old)) = self.entries.remove(at) {
                self.bytes -= old.bytes.len();
            }
        }
        self.bytes += fetched.bytes.len();
        self.entries.push_back((url.to_owned(), fetched));
        while self.entries.len() > CACHE_CLIPS || self.bytes > CACHE_BYTES {
            let Some((_, old)) = self.entries.pop_front() else {
                break;
            };
            self.bytes -= old.bytes.len();
        }
    }
}

/// Releases a key's in-flight mark when its fetch task ends.
struct FetchMark {
    key: String,
    generation: u64,
}

impl Drop for FetchMark {
    fn drop(&mut self) {
        let mut glue = glue();
        if glue.fetching.get(&self.key) == Some(&self.generation) {
            glue.fetching.remove(&self.key);
        }
    }
}

fn glue() -> MutexGuard<'static, Glue> {
    static GLUE: OnceLock<Mutex<Glue>> = OnceLock::new();
    GLUE.get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn master(glue: &Glue) -> f64 {
    glue.master.unwrap_or_else(|| {
        f64::from(crate::desktop::current().sound_settings.soundboard_volume) / 100.0
    })
}

#[expect(clippy::cast_possible_truncation, reason = "a gain in 0..=1")]
fn gain(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}

/// The voice settings' output device (its name), or the default.
pub fn set_output_device(name: Option<String>) {
    let mut glue = glue();
    if glue.output != name {
        glue.output.clone_from(&name);
        if let Some(engine) = &glue.engine {
            engine.set_output_device(name);
        }
    }
}

/// The soundboard volume setting (0..=100), for clips now and later.
pub fn set_soundboard_volume(volume: u8) {
    let mut glue = glue();
    let master = f64::from(volume.min(100)) / 100.0;
    if glue.master == Some(master) {
        return;
    }
    glue.master = Some(master);
    if let Some(engine) = &glue.engine {
        for (key, raw) in &glue.raw {
            engine.set_volume(key, gain(raw * master));
        }
    }
}

fn now_ms() -> i64 {
    crate::client::voice::now_ms()
}

/// Starts the player; the runtime calls this once.
pub fn spawn(http: reqwest::Client, events: AudioEvents) -> AudioHandle {
    let sink = events.clone();
    let engine = Engine::new(Arc::new(move |event: Event| {
        let event = match event {
            Event::Started { key } => AudioEvent::Started(key),
            Event::Ended { key } => {
                glue().forget(&key);
                AudioEvent::Ended(key)
            }
            Event::Failed { key, error } => {
                glue().forget(&key);
                AudioEvent::Failed { key, error }
            }
            #[expect(clippy::cast_precision_loss, reason = "ms positions")]
            Event::StreamPosition {
                key, position_ms, ..
            } => AudioEvent::StreamPosition {
                key,
                position_ms: position_ms as f64,
                now_ms: now_ms(),
            },
            Event::StreamRetrying { key, message, .. } => {
                AudioEvent::StreamRetrying { key, message }
            }
            Event::StreamFailed { key, failure } => AudioEvent::StreamFailed {
                key,
                code: failure.code,
                message: failure.message,
            },
            Event::StreamEnded { key } => AudioEvent::StreamEnded(key),
            Event::Level(level) => {
                LEVEL.store(f64::from(level).to_bits(), Ordering::Relaxed);
                return;
            }
        };
        sink.send(event);
    }));
    {
        let mut glue = glue();
        engine.set_output_device(glue.output.clone());
        glue.engine = Some(engine.clone());
    }
    let (tx, mut rx) = mpsc::unbounded_channel::<(Option<Token>, AudioCommand)>();
    tokio::spawn(async move {
        while let Some((token, command)) = rx.recv().await {
            run(&engine, &http, &events, token, command);
        }
    });
    AudioHandle { commands: Some(tx) }
}

fn run(
    engine: &Engine,
    http: &reqwest::Client,
    events: &AudioEvents,
    token: Option<Token>,
    command: AudioCommand,
) {
    match command {
        AudioCommand::Play {
            key,
            source,
            volume,
            start_at,
            paused,
            max_seconds,
            preview: _,
        } => {
            let generation = {
                let mut glue = glue();
                glue.next += 1;
                let generation = glue.next;
                glue.generation.insert(key.clone(), generation);
                glue.raw.insert(key.clone(), volume);
                generation
            };
            let opts = |extra: f64| {
                let glue = glue();
                ClipOptions {
                    volume: gain(glue.raw.get(&key).copied().unwrap_or(volume) * master(&glue)),
                    start_at_secs: start_at + if paused { 0.0 } else { extra },
                    paused,
                    max_duration_secs: max_seconds,
                }
            };
            match source {
                ClipSource::Tone(id) => engine.play(&key, Clip::Tone(id), opts(0.0)),
                ClipSource::Data(url) => match decode_data_url(&url) {
                    Some((bytes, hint)) => engine.play(
                        &key,
                        Clip::Encoded {
                            bytes: bytes.into(),
                            hint,
                        },
                        opts(0.0),
                    ),
                    None => {
                        glue().forget(&key);
                        events.send(AudioEvent::Failed {
                            key,
                            error: "bad data URL".into(),
                        });
                    }
                },
                ClipSource::Media(url) => {
                    let engine = engine.clone();
                    let http = http.clone();
                    let events = events.clone();
                    glue().fetching.insert(key.clone(), generation);
                    tokio::spawn(async move {
                        let _mark = FetchMark {
                            key: key.clone(),
                            generation,
                        };
                        let started = Instant::now();
                        let fetched = fetch(&http, token.as_ref(), &url).await;
                        if glue().generation.get(&key) != Some(&generation) {
                            return;
                        }
                        match fetched {
                            Ok(Fetched {
                                bytes,
                                content_type,
                            }) => {
                                let hint = extension_hint(&url, content_type.as_deref());
                                let waited = started.elapsed().as_secs_f64();
                                let options = {
                                    let glue = glue();
                                    if !glue.raw.contains_key(&key) {
                                        return; // stopped while it loaded
                                    }
                                    ClipOptions {
                                        volume: gain(
                                            glue.raw.get(&key).copied().unwrap_or(volume)
                                                * master(&glue),
                                        ),
                                        start_at_secs: start_at + if paused { 0.0 } else { waited },
                                        paused,
                                        max_duration_secs: max_seconds,
                                    }
                                };
                                engine.play(&key, Clip::Encoded { bytes, hint }, options);
                            }
                            Err(error) => {
                                eprintln!("[audio] fetch failed: {error}");
                                {
                                    let mut glue = glue();
                                    glue.raw.remove(&key);
                                    glue.generation.remove(&key);
                                }
                                events.send(AudioEvent::Failed { key, error });
                            }
                        }
                    });
                }
            }
        }
        AudioCommand::Stop(key) => {
            {
                let mut glue = glue();
                glue.raw.remove(&key);
                glue.generation.remove(&key);
            }
            engine.stop(&key);
        }
        AudioCommand::Pause { key, paused } => engine.set_paused(&key, paused),
        AudioCommand::Volume { key, volume } => {
            let master = {
                let mut glue = glue();
                glue.raw.insert(key.clone(), volume);
                master(&glue)
            };
            engine.set_volume(&key, gain(volume * master));
        }
        AudioCommand::StopAll => {
            {
                let mut glue = glue();
                glue.raw.clear();
                glue.generation.clear();
            }
            engine.stop_all();
            LEVEL.store(f64::NAN.to_bits(), Ordering::Relaxed);
        }
        AudioCommand::StreamLoad {
            key,
            path,
            position_ms,
            paused,
        } => {
            let mut headers = Vec::new();
            if let Some(token) = &token {
                headers.push((
                    "Authorization".to_owned(),
                    format!("Bearer {}", token.as_str()),
                ));
            }
            let (volume, loudness) = stream_audio();
            eprintln!("[listen] load {key} at {position_ms:.0} ms (paused {paused})");
            engine.stream_load(StreamRequest {
                key,
                url: format!("{}{path}", config::get().api_base),
                headers,
                position_ms: ms(position_ms),
                paused,
                volume,
                loudness,
            });
        }
        AudioCommand::StreamSeek(position_ms) => engine.stream_seek(ms(position_ms)),
        AudioCommand::StreamPause(paused) => engine.stream_set_paused(paused),
        AudioCommand::StreamStop => engine.stream_stop(),
        AudioCommand::StreamAudio {
            volume,
            loudness,
            preset,
        } => {
            let setting = Loudness {
                enabled: loudness,
                preset: LoudnessPreset::parse(&preset).unwrap_or(LoudnessPreset::Balanced),
            };
            *stream_settings() = (gain(volume), setting);
            engine.stream_set_volume(gain(volume));
            engine.stream_set_loudness(setting);
        }
    }
}

fn stream_settings() -> MutexGuard<'static, (f32, Loudness)> {
    static SETTINGS: OnceLock<Mutex<(f32, Loudness)>> = OnceLock::new();
    SETTINGS
        .get_or_init(|| {
            Mutex::new((
                1.0,
                Loudness {
                    enabled: true,
                    preset: LoudnessPreset::Balanced,
                },
            ))
        })
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

fn stream_audio() -> (f32, Loudness) {
    *stream_settings()
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a non-negative ms position"
)]
fn ms(value: f64) -> u64 {
    value.max(0.0).round() as u64
}

/// Where a clip comes from.
#[derive(Debug, PartialEq, Eq)]
struct Target {
    url: String,
    /// Our API: the session goes with the request. Nobody else gets it.
    ours: bool,
}

/// The URL to fetch for a clip's `media_url`, or why not. Only what the
/// room itself relays is fetched: an upload path on `api_base` (maybe with a
/// DM call's capability query) or a MyInstants sound.
fn target(api_base: &str, url: &str) -> Result<Target, String> {
    let url = wire::sanitize_inbound_media_url(url).ok_or("unsupported clip URL")?;
    if url.starts_with('/') {
        return Ok(Target {
            url: format!("{}{url}", api_base.trim_end_matches('/')),
            ours: true,
        });
    }
    Ok(Target { url, ours: false })
}

/// The origin (`scheme://host:port`) of `url`, if it parses.
fn origin(url: &str) -> Option<String> {
    reqwest::Url::parse(url)
        .ok()
        .map(|url| url.origin().ascii_serialization())
}

/// A clip's bytes: our API with the session, else a plain https GET
/// (MyInstants). At most [`MAX_FETCH_BYTES`].
async fn fetch(
    http: &reqwest::Client,
    token: Option<&Token>,
    url: &str,
) -> Result<Fetched, String> {
    if let Some(cached) = glue().cache.get(url) {
        return Ok(cached);
    }
    let api_base = config::get().api_base.clone();
    let target = target(&api_base, url)?;
    let mut request = http.get(&target.url);
    if target.ours {
        if origin(&target.url) != origin(&api_base) {
            return Err("clip URL is not on our API".into());
        }
        if let Some(token) = token {
            request = request.bearer_auth(token.as_str());
        }
    }
    let mut response = request.send().await.map_err(|e| e.to_string())?;
    // A redirect must not carry us (or the body we read) to another origin.
    if origin(response.url().as_str()) != origin(&target.url) {
        return Err("clip redirected to another host".into());
    }
    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_FETCH_BYTES as u64)
    {
        return Err("clip is too large".into());
    }
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > MAX_FETCH_BYTES {
            return Err("clip is too large".into());
        }
        body.extend_from_slice(&chunk);
    }
    let fetched = Fetched {
        bytes: body.into(),
        content_type,
    };
    glue().cache.insert(url, fetched.clone());
    Ok(fetched)
}

/// A decoder hint: the extension of the URL's last path segment (not of a
/// host or a query), else the `Content-Type`.
fn extension_hint(url: &str, content_type: Option<&str>) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let segment = path.rsplit('/').next().unwrap_or("");
    segment
        .rsplit_once('.')
        .map(|(_, extension)| extension)
        .filter(|extension| {
            (1..=5).contains(&extension.len())
                && extension.bytes().all(|b| b.is_ascii_alphanumeric())
        })
        .map(str::to_ascii_lowercase)
        .or_else(|| {
            content_type
                .map(str::trim)
                .filter(|content_type| !content_type.is_empty())
                .map(str::to_owned)
        })
}

/// `data:audio/<type>;base64,<data>` as bytes and a type hint.
fn decode_data_url(url: &str) -> Option<(Vec<u8>, Option<String>)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    let mime = meta.strip_suffix(";base64")?;
    Some((base64(data)?, Some(mime.to_owned())))
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |b: u8| -> Option<u32> {
        Some(u32::from(match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        }))
    };
    let clean: Vec<u8> = text.bytes().filter(|b| *b != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut acc = 0u32;
        for (i, b) in chunk.iter().enumerate() {
            acc |= value(*b)? << (18 - 6 * i);
        }
        let bytes = acc.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_urls_should_decode() {
        assert_eq!(
            decode_data_url("data:audio/wav;base64,AAE="),
            Some((vec![0, 1], Some("audio/wav".into())))
        );
        assert_eq!(base64("aGVsbG8="), Some(b"hello".to_vec()));
    }

    const API: &str = "https://yap.example";

    #[test]
    fn only_our_uploads_get_the_session() {
        assert_eq!(
            target(API, "/api/soundboard/uploads/snd_9"),
            Ok(Target {
                url: "https://yap.example/api/soundboard/uploads/snd_9".into(),
                ours: true
            })
        );
        let capability = "/api/soundboard/uploads/snd_9?cap=a.b.c&room_slug=r&playback_id=p";
        assert_eq!(
            target("https://yap.example/", capability).map(|target| target.ours),
            Ok(true)
        );
        assert_eq!(
            target(API, "https://www.myinstants.com/media/sounds/x.mp3"),
            Ok(Target {
                url: "https://www.myinstants.com/media/sounds/x.mp3".into(),
                ours: false
            })
        );
        for bad in [
            "https://evil.example/api/soundboard/uploads/snd_9",
            "//evil.example/api/soundboard/uploads/snd_9",
            "http://www.myinstants.com/media/sounds/x.mp3",
            "https://www.myinstants.com@evil.example/media/sounds/x.mp3",
            "/api/attachments/x",
            "data:audio/wav;base64,AAAA",
        ] {
            assert!(target(API, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_cache_is_bounded_by_bytes_and_count() {
        let clip = |len: usize| Fetched {
            bytes: vec![0u8; len].into(),
            content_type: None,
        };
        let mut cache = ClipCache::default();
        // Three 30 MB clips: only the last two fit in 64 MB.
        for name in ["a", "b", "c"] {
            cache.insert(name, clip(30 * 1024 * 1024));
        }
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some() && cache.get("c").is_some());
        assert_eq!(cache.bytes, 60 * 1024 * 1024);
        // The same key again replaces, not adds.
        cache.insert("c", clip(1024));
        assert_eq!(cache.bytes, 30 * 1024 * 1024 + 1024);
        // Many small clips: the count cap.
        let mut cache = ClipCache::default();
        for index in 0..(CACHE_CLIPS + 10) {
            cache.insert(&format!("k{index}"), clip(10));
        }
        assert_eq!(cache.entries.len(), CACHE_CLIPS);
        assert_eq!(cache.bytes, CACHE_CLIPS * 10);
    }

    #[test]
    fn the_extension_comes_from_the_last_path_segment_or_the_content_type() {
        assert_eq!(
            extension_hint("https://www.myinstants.com/media/sounds/x.MP3", None).as_deref(),
            Some("mp3")
        );
        // No extension in the segment: the host's dot must not leak in.
        assert_eq!(
            extension_hint(
                "https://yap.example/api/soundboard/uploads/snd_9",
                Some("audio/mpeg")
            )
            .as_deref(),
            Some("audio/mpeg")
        );
        assert_eq!(
            extension_hint(
                "/api/soundboard/uploads/snd_9?cap=a.b.c&room_slug=dm.call",
                None
            ),
            None
        );
    }

    #[test]
    fn a_finished_clip_forgets_its_generation_unless_a_fetch_is_in_flight() {
        let mut glue = Glue::default();
        glue.generation.insert("k".into(), 1);
        glue.raw.insert("k".into(), 0.5);
        glue.forget("k");
        assert!(glue.generation.is_empty() && glue.raw.is_empty());
        glue.generation.insert("k".into(), 2);
        glue.fetching.insert("k".into(), 2);
        glue.forget("k");
        assert_eq!(glue.generation.get("k"), Some(&2));
    }
}
