//! The production engine: one peer connection per call, with the call's
//! own audio path (`pipeline.rs`: WASAPI mic and speaker, WebRTC's APM,
//! RNNoise, a per-track stereo mix the echo canceller hears as its far
//! end). libwebrtc's platform audio device module lists the devices; with
//! `YAP_ADM_AUDIO=1` it also records and plays, as it did before the own
//! path (mixed playout: no panning, no RNNoise).
//!
//! Devices: the choice ([`LibwebrtcEngine::choose_devices`]) is kept for the
//! process and resolved by endpoint id against the ADM's lists whenever it
//! is up; in a call it switches at once (the own path reopens that side;
//! the ADM stops, selects, inits and starts). Two track calls the crate
//! lacks (remote volume, live APM options) go through `cpp/track_audio.cpp`
//! for the ADM path.
//!
//! Camera: one send-only VP8 transceiver with the web's three simulcast
//! encodings, fed by `crate::camera` (captured frames while live, the
//! keepalive's black frames while parked). A stale park pushes a fresh
//! transceiver; the old one is retired, not closed: it keeps its track but
//! nothing feeds it any more. (Not `set_track(None)`: webrtc-sys 0.3.52's
//! `RtpSender::set_track` dereferences the null track and crashes.)
//!
//! Share: one send-only video transceiver with a single encoding at the
//! preset's caps, fed by `crate::share` (captured frames while live, the
//! last one repeated while the source is still, the keepalive's black
//! frames while parked), and an Opus transceiver for the source's audio fed
//! from a `NativeAudioSource`. Stopping parks the video for the next share
//! and retires the audio. With a hardware H.264 encoder (`mf_h264.rs`, as
//! the desktop app's native share) the video is H.264 only: the frames go
//! to the encoder, its access units to an encoded source, and the sender
//! runs libwebrtc's pre-encoded pass-through. Without one (or with
//! `YAP_SHARE_ENCODER=software`) it is libwebrtc's VP8, as the browser path
//! sends.
//!
//! Video: each routed remote video track gets a `NativeVideoStream` (a
//! WebRTC sink with a one-frame queue: a slow consumer sees the newest frame,
//! never a backlog) pumped by a tokio task that hands I420 planes to the
//! installed [`VideoOutput`]. A decoded I420 buffer is passed as is; other
//! buffer kinds are converted once. An unrouted track has no sink at all, so
//! its frames cost nothing past the decoder.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use libwebrtc::video_frame::{VideoBuffer, VideoRotation};
use libwebrtc::video_stream::native::NativeVideoStream;
use libwebrtc::video_track::RtcVideoTrack;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::audio_source::AudioSourceOptions;
use libwebrtc::audio_stream::native::{NativeAudioStream, NativeAudioStreamOptions};
use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{
    AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState,
};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{
    IceServer as RtcIceServer, PeerConnectionFactory, RtcConfiguration,
};
use libwebrtc::prelude::RtcAudioTrack;
use libwebrtc::rtp_parameters::{DegradationPreference, RtpCodecCapability, RtpEncodingParameters};
use libwebrtc::rtp_sender::{RtpSender, VideoEncoderBackend};
use libwebrtc::rtp_transceiver::{RtpTransceiver, RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::stats::RtcStats;
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_source::VideoResolution;
use libwebrtc::MediaType;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::pipeline::{self, pipeline};
#[cfg(windows)]
use super::{HwConfig, HwEncoder};
use super::{
    I420Frame, InboundVideo, LocalOffer, MediaEngine, MediaError, MediaSession, MediaStats,
    OutboundVideo, VideoOutput,
};
use crate::audio::{resolve, AudioDevice, AudioDevices, Processing};
use crate::camera::live::{set_sink, Keepalive};
use crate::dsp::mix::Stream;
use crate::dsp::RATE;
use crate::layers::CAMERA_SIMULCAST;
use crate::machine::MediaState;
use crate::share::live::{
    set_audio as set_share_audio, set_sink as set_share_sink, AudioKeepalive, Refresher,
    Sink as ShareSink,
};
use crate::share::{ShareEncoding, SHARE_AUDIO_CHANNELS, SHARE_AUDIO_RATE};
use crate::wire::IceServer;

/// The process-wide factory, made by the first call (never before: its
/// synthetic audio device wakes every 10 ms for as long as it lives) and let
/// go [`FACTORY_IDLE`] after the last call ends.
static FACTORY: Mutex<Option<PeerConnectionFactory>> = Mutex::new(None);

/// Counts uses of the factory; a pending release made before the newest use
/// does nothing.
static FACTORY_USES: AtomicU64 = AtomicU64::new(0);

/// How long the factory outlives the last call: a quick rejoin keeps it.
const FACTORY_IDLE: Duration = Duration::from_secs(30);

/// One process-wide factory: it owns the platform ADM. Made if there is
/// none.
fn factory() -> PeerConnectionFactory {
    let mut held = lock(&FACTORY);
    FACTORY_USES.fetch_add(1, Ordering::Relaxed);
    held.get_or_insert_with(|| {
        eprintln!("[voice] webrtc factory created");
        PeerConnectionFactory::default()
    })
    .clone()
}

/// The factory if one is up (none is made for asking).
fn existing_factory() -> Option<PeerConnectionFactory> {
    lock(&FACTORY).clone()
}

/// Keeps WebRTC's voice engine, and with it the platform ADM, up while the
/// factory lives. The voice engine runs only while a peer connection exists:
/// when the last one goes, `WebRtcVoiceEngine::Terminate` terminates the
/// ADM, and webrtc-sys's `AdmProxy` never initialises its platform ADM again
/// (its `Init` is a no-op), so every later call on that factory would list
/// -1 devices and record nothing. Every call still opens the ADM (its
/// device choice is applied there, and `YAP_ADM_AUDIO=1` records and plays
/// through it), so the keeper stays: one idle connection (no transceivers,
/// no ICE), made just before the first call's, closed only with the
/// factory. A new factory brings a new ADM.
static KEEPER: Mutex<Option<PeerConnection>> = Mutex::new(None);

fn keep_voice_engine(f: &PeerConnectionFactory) {
    let mut keeper = lock(&KEEPER);
    if keeper.is_none() {
        *keeper = f
            .create_peer_connection(RtcConfiguration::default())
            .map_err(|e| eprintln!("[voice] voice engine keeper refused: {e:?}"))
            .ok();
    }
}

/// The last call ended: after [`FACTORY_IDLE`] with no call opened and no
/// other use since, the keeper and the factory go, and with them its
/// threads and the synthetic audio device's 100 Hz wake-ups. The next call
/// makes a new one.
fn release_factory_later() {
    let uses = FACTORY_USES.load(Ordering::Relaxed);
    let spawned = std::thread::Builder::new()
        .name("yap-voice-idle".into())
        .spawn(move || {
            std::thread::sleep(FACTORY_IDLE);
            let mut held = lock(&FACTORY);
            if FACTORY_USES.load(Ordering::Relaxed) != uses || CALL_OPEN.load(Ordering::Relaxed) {
                return;
            }
            if let Some(keeper) = lock(&KEEPER).take() {
                keeper.close();
            }
            if held.take().is_some() {
                eprintln!("[voice] webrtc factory released");
            }
        });
    if let Err(e) = spawned {
        eprintln!("[voice] factory release: {e}");
    }
}

fn err(what: &str) -> impl FnOnce(libwebrtc::RtcError) -> MediaError + '_ {
    move |e| MediaError(format!("{what}: {e:?}"))
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

extern "C" {
    fn yap_audio_track_set_volume(
        track: *const c_void,
        id: *const c_char,
        id_len: usize,
        volume: f64,
    ) -> i32;
    fn yap_audio_track_set_processing(
        track: *const c_void,
        id: *const c_char,
        id_len: usize,
        echo_cancellation: bool,
        noise_suppression: bool,
        auto_gain_control: bool,
    ) -> i32;
}

/// The address of the `livekit_ffi::AudioTrack` behind `track`, for
/// `cpp/track_audio.cpp`.
fn livekit_track(track: &RtcAudioTrack) -> *const c_void {
    const {
        assert!(size_of::<RtcAudioTrack>() == 2 * size_of::<*const c_void>());
    }
    // SAFETY: in libwebrtc 0.3.55 (pinned in Cargo.toml) RtcAudioTrack is a
    // newtype over the native track, itself a newtype over cxx's
    // `SharedPtr<livekit_ffi::AudioTrack>`, whose two words are a
    // `std::shared_ptr`: element pointer first. The size check rules out
    // any other field. This copies the two pointers; the track and its
    // reference count are untouched.
    let words: [*const c_void; 2] = unsafe { std::mem::transmute_copy(track) };
    words[0]
}

/// Sets a remote track's playout gain (0..=10); `false` if refused.
fn set_volume(track: &RtcAudioTrack, gain: f64) -> bool {
    let id = track.id();
    // SAFETY: the pointer stays valid while `track` is borrowed; the C++
    // side checks the kind and this id before it touches the track.
    unsafe {
        yap_audio_track_set_volume(
            livekit_track(track),
            id.as_ptr().cast(),
            id.len(),
            gain.clamp(0.0, 10.0),
        ) == 1
    }
}

/// The libwebrtc engine. Stateless: the factory, its ADM and the device
/// choice are global.
#[derive(Debug, Clone, Copy, Default)]
pub struct LibwebrtcEngine;

/// A call holds the ADM, and its mic is unmuted. While both hold, capture
/// runs; the ADM does not always report it (`RecordingIsInitialized` reads
/// false during a call here), and WebRTC stops recording while every send
/// stream is muted, so a device switch decides from these.
static CALL_OPEN: AtomicBool = AtomicBool::new(false);
static MIC_LIVE: AtomicBool = AtomicBool::new(false);

/// The chosen devices (`None`: the system default) and the endpoints in
/// use while the ADM is up.
#[derive(Debug, Default)]
struct Selection {
    input: Option<String>,
    output: Option<String>,
    applied_input: Option<String>,
    applied_output: Option<String>,
}

fn selection() -> &'static Mutex<Selection> {
    static SELECTION: Mutex<Selection> = Mutex::new(Selection {
        input: None,
        output: None,
        applied_input: None,
        applied_output: None,
    });
    &SELECTION
}

/// The system default, from the ADM's index -1 (the default communication
/// endpoint on Windows, which WebRTC opens unless told otherwise).
fn default_device(name: String, id: String) -> Option<AudioDevice> {
    (!name.is_empty() && !id.is_empty() && !id.starts_with("index:"))
        .then_some(AudioDevice { id, name })
}

/// Both lists, with the ADM up.
fn list_devices(f: &PeerConnectionFactory) -> AudioDevices {
    let count = |n: i16| u16::try_from(n).unwrap_or(0);
    AudioDevices {
        inputs: (0..count(f.recording_devices()))
            .map(|i| AudioDevice {
                id: f.recording_device_guid(i),
                name: f.recording_device_name(i),
            })
            .collect(),
        outputs: (0..count(f.playout_devices()))
            .map(|i| AudioDevice {
                id: f.playout_device_guid(i),
                name: f.playout_device_name(i),
            })
            .collect(),
        default_input: default_device(
            f.recording_device_name(u16::MAX),
            f.recording_device_guid(u16::MAX),
        ),
        default_output: default_device(
            f.playout_device_name(u16::MAX),
            f.playout_device_guid(u16::MAX),
        ),
    }
}

/// Puts the ADM on the devices the choice resolves to, restarting capture
/// or playout that was running. A line per switch.
fn apply(f: &PeerConnectionFactory, sel: &mut Selection) -> Option<String> {
    let list = list_devices(f);
    if list.inputs.is_empty() || list.outputs.is_empty() {
        // Diagnostic: a call that can select no device sends no audio.
        eprintln!(
            "[voice] devices: none to apply (recording_devices={} playout_devices={} adm_active={})",
            f.recording_devices(),
            f.playout_devices(),
            f.is_platform_adm_active()
        );
    }
    let name = |devices: &[AudioDevice], id: &str| {
        devices
            .iter()
            .find(|d| d.id == id)
            .map_or_else(|| id.to_owned(), |d| d.name.clone())
    };
    let mut lines = Vec::new();
    let input = resolve(
        sel.input.as_deref(),
        &list.inputs,
        list.default_input.as_ref(),
    );
    if pipeline::enabled() {
        // The own path opens the endpoints itself; the ADM records and
        // plays nothing.
        let output = resolve(
            sel.output.as_deref(),
            &list.outputs,
            list.default_output.as_ref(),
        );
        let changed =
            (input, output) != (sel.applied_input.as_deref(), sel.applied_output.as_deref());
        if !changed {
            return None;
        }
        let (input, output) = (input.map(str::to_owned), output.map(str::to_owned));
        let line = format!(
            "[voice] devices: input {}; output {}",
            input
                .as_deref()
                .map_or_else(|| "default".to_owned(), |id| name(&list.inputs, id)),
            output
                .as_deref()
                .map_or_else(|| "default".to_owned(), |id| name(&list.outputs, id)),
        );
        pipeline().set_devices(input.clone(), output.clone());
        sel.applied_input = input;
        sel.applied_output = output;
        return Some(line);
    }
    if let Some(id) = input.filter(|id| sel.applied_input.as_deref() != Some(*id)) {
        let running = f.recording_is_initialized()
            || (CALL_OPEN.load(Ordering::Relaxed) && MIC_LIVE.load(Ordering::Relaxed));
        if running {
            f.stop_recording();
        }
        let selected = f.set_recording_device_by_guid(id);
        let restarted = running && f.init_recording() && f.start_recording();
        lines.push(format!(
            "input {} (selected {selected}, restarted {running}: {restarted})",
            name(&list.inputs, id)
        ));
        sel.applied_input = Some(id.to_owned());
    }
    let output = resolve(
        sel.output.as_deref(),
        &list.outputs,
        list.default_output.as_ref(),
    );
    if let Some(id) = output.filter(|id| sel.applied_output.as_deref() != Some(*id)) {
        let running = f.playout_is_initialized() || CALL_OPEN.load(Ordering::Relaxed);
        if running {
            f.stop_playout();
        }
        let selected = f.set_playout_device_by_guid(id);
        let restarted = running && f.init_playout() && f.start_playout();
        lines.push(format!(
            "output {} (selected {selected}, restarted {running}: {restarted})",
            name(&list.outputs, id)
        ));
        sel.applied_output = Some(id.to_owned());
    }
    (!lines.is_empty()).then(|| format!("[voice] devices: {}", lines.join("; ")))
}

impl LibwebrtcEngine {
    /// Installs where remote video frames go, for this and later calls.
    pub fn set_video_output(output: Arc<dyn VideoOutput>) {
        *video_output()
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(output);
    }

    /// The platform's input and output devices. Listed through WASAPI on
    /// Windows (the ids and names the ADM lists), so no factory is made
    /// for it; otherwise this opens the ADM briefly (no capture or playout
    /// starts). Blocks: call it off the GUI thread.
    pub fn devices(&self) -> AudioDevices {
        #[cfg(windows)]
        if let Some(list) = super::wasapi::list_devices() {
            return list;
        }
        let f = &factory();
        if !f.acquire_platform_adm() {
            return AudioDevices::default();
        }
        let list = list_devices(f);
        f.release_platform_adm();
        if !f.is_platform_adm_active() {
            Self::forget_applied();
        }
        list
    }

    /// Chooses the devices (`None`: the system default) for this and later
    /// calls; in a call it switches at once. A log line if it switched.
    /// Blocks like [`Self::devices`].
    pub fn choose_devices(&self, input: Option<String>, output: Option<String>) -> Option<String> {
        let mut sel = lock(selection());
        sel.input = input;
        sel.output = output;
        // Before the first call there is no factory to tell: the call
        // applies the choice when it opens the ADM.
        match existing_factory() {
            Some(f) if f.is_platform_adm_active() => apply(&f, &mut sel),
            _ => None,
        }
    }

    /// A fresh ADM uses the default devices until told otherwise.
    fn forget_applied() {
        let mut sel = lock(selection());
        sel.applied_input = None;
        sel.applied_output = None;
    }
}

fn video_output() -> &'static RwLock<Option<Arc<dyn VideoOutput>>> {
    static OUTPUT: RwLock<Option<Arc<dyn VideoOutput>>> = RwLock::new(None);
    &OUTPUT
}

/// The installed video output (our share's preview goes there too).
pub(crate) fn installed_video_output() -> Option<Arc<dyn VideoOutput>> {
    video_output()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// How often a routed track logs its frame rate and size.
const VIDEO_STATS_EVERY: Duration = Duration::from_secs(5);

/// Feeds one remote video track's frames to the output under `key` until
/// aborted (dropping the stream removes its sink from the track).
async fn pump_video(track: RtcVideoTrack, key: String) {
    let mut stream = NativeVideoStream::new(track);
    let output = video_output()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone();
    let Some(output) = output else {
        eprintln!("[video] no output installed; {key} dropped");
        return;
    };
    let (mut frames, mut since) = (0u32, Instant::now());
    while let Some(frame) = stream.next().await {
        let buffer = frame.buffer.as_ref();
        let rotation = match frame.rotation {
            VideoRotation::VideoRotation0 => 0,
            VideoRotation::VideoRotation90 => 90,
            VideoRotation::VideoRotation180 => 180,
            VideoRotation::VideoRotation270 => 270,
        };
        let size = (buffer.width(), buffer.height());
        // A hardware-decoded frame (the patched webrtc-sys's D3D11 texture)
        // goes to the output as it is; only an output that cannot take it
        // gets it read back.
        #[cfg(windows)]
        let native = buffer
            .as_native()
            .is_some_and(|native| output.native_frame(&key, native.sys_ptr(), rotation));
        #[cfg(not(windows))]
        let native = false;
        if !native {
            let converted;
            let i420 = match buffer.as_i420() {
                Some(i420) => i420,
                None => {
                    converted = buffer.to_i420();
                    &converted
                }
            };
            let (y, u, v) = i420.data();
            let (stride_y, stride_u, stride_v) = i420.strides();
            output.frame(
                &key,
                &I420Frame {
                    width: i420.width(),
                    height: i420.height(),
                    rotation,
                    y,
                    u,
                    v,
                    stride_y,
                    stride_u,
                    stride_v,
                },
            );
        }
        frames += 1;
        let elapsed = since.elapsed();
        if elapsed >= VIDEO_STATS_EVERY {
            eprintln!(
                "[video] {key} {}x{} {:.1} fps",
                size.0,
                size.1,
                f64::from(frames) / elapsed.as_secs_f64()
            );
            (frames, since) = (0, Instant::now());
        }
    }
}

/// One remote audio track in the own path's mix: its stream there, and the
/// task feeding it the track's decoded audio.
struct Playing {
    stream: Arc<Stream>,
    pump: JoinHandle<()>,
}

impl Drop for Playing {
    fn drop(&mut self) {
        self.pump.abort();
    }
}

/// Feeds one remote audio track's decoded audio (48 kHz stereo) to its
/// stream in the mix until aborted (dropping the `NativeAudioStream`
/// removes its sink from the track).
async fn pump_audio(track: RtcAudioTrack, stream: Arc<Stream>) {
    // At most 200 ms waiting here; the mix trims its own queue further.
    // Two channels whatever the track carries (the sink remixes to what it
    // was opened with): the mix tells a mono track by its equal channels.
    let mut frames = NativeAudioStream::with_options(
        track,
        RATE as i32,
        2,
        NativeAudioStreamOptions {
            queue_size_frames: Some(20),
        },
    );
    while let Some(frame) = frames.next().await {
        stream.push(&frame.data, frame.num_channels as usize);
    }
}

/// The remote tracks (the own path mixes the audio ones, or the ADM does),
/// by receive mid, and the video routes.
struct Remotes {
    tracks: Vec<(Option<String>, MediaStreamTrack)>,
    /// Gains set by mid, also for tracks that have not arrived yet.
    gains: HashMap<String, f64>,
    /// Pans set by mid (own path), also for tracks that have not arrived.
    pans: HashMap<String, f64>,
    /// Audio mid -> its stream in the own path's mix.
    playing: HashMap<String, Playing>,
    /// Deafened: remote audio tracks are disabled, new ones included.
    silenced: bool,
    /// Video mid -> the output key its frames go to, also for tracks that
    /// have not arrived yet.
    routes: HashMap<String, String>,
    /// Video mid -> the task pumping its frames.
    pumps: HashMap<String, JoinHandle<()>>,
    /// The call's runtime (WebRTC calls `on_track` on its own thread).
    runtime: Handle,
}

impl Remotes {
    fn new(runtime: Handle) -> Self {
        Self {
            tracks: Vec::new(),
            gains: HashMap::new(),
            pans: HashMap::new(),
            playing: HashMap::new(),
            silenced: false,
            routes: HashMap::new(),
            pumps: HashMap::new(),
            runtime,
        }
    }

    fn add(&mut self, mid: Option<String>, track: MediaStreamTrack) {
        match &track {
            MediaStreamTrack::Audio(audio) => {
                if self.silenced {
                    track.set_enabled(false);
                }
                let gain = mid.as_ref().and_then(|m| self.gains.get(m)).copied();
                if pipeline::enabled() {
                    let stream = pipeline().add_stream();
                    stream.set_gain(gain.unwrap_or(1.0));
                    stream.set_pan(
                        mid.as_ref()
                            .and_then(|m| self.pans.get(m))
                            .copied()
                            .unwrap_or(0.0),
                    );
                    let pump = self
                        .runtime
                        .spawn(pump_audio(audio.clone(), Arc::clone(&stream)));
                    // A track without a mid cannot be addressed; it still
                    // plays, at its first gain.
                    let key = mid
                        .clone()
                        .unwrap_or_else(|| format!("no-mid-{}", audio.id()));
                    self.playing.insert(key, Playing { stream, pump });
                } else if let Some(gain) = gain {
                    if !set_volume(audio, gain) {
                        eprintln!("[voice] track volume refused for a new track");
                    }
                }
            }
            MediaStreamTrack::Video(video) => {
                if let Some((mid, key)) = mid
                    .as_ref()
                    .and_then(|m| Some((m.clone(), self.routes.get(m)?.clone())))
                {
                    self.start_pump(mid, video.clone(), key);
                }
            }
        }
        self.tracks.push((mid, track));
    }

    fn start_pump(&mut self, mid: String, track: RtcVideoTrack, key: String) {
        let pump = self.runtime.spawn(pump_video(track, key));
        if let Some(old) = self.pumps.insert(mid, pump) {
            old.abort();
        }
    }

    fn video_track(&self, mid: &str) -> Option<RtcVideoTrack> {
        self.tracks.iter().find_map(|(m, track)| match track {
            MediaStreamTrack::Video(video) if m.as_deref() == Some(mid) => Some(video.clone()),
            _ => None,
        })
    }

    fn route(&mut self, mid: &str, key: Option<&str>) {
        match key {
            Some(key) => {
                self.routes.insert(mid.to_owned(), key.to_owned());
                if let Some(track) = self.video_track(mid) {
                    self.start_pump(mid.to_owned(), track, key.to_owned());
                }
            }
            None => {
                self.routes.remove(mid);
                if let Some(pump) = self.pumps.remove(mid) {
                    pump.abort();
                }
            }
        }
    }

    /// Forgets these mids: their gains, pans, routes, pumps and tracks.
    fn forget(&mut self, mids: &[String]) {
        for mid in mids {
            self.route(mid, None);
            self.gains.remove(mid);
            self.pans.remove(mid);
            self.playing.remove(mid);
        }
        self.tracks
            .retain(|(mid, _)| !mid.as_ref().is_some_and(|m| mids.contains(m)));
    }

    fn clear(&mut self) {
        for (_, pump) in self.pumps.drain() {
            pump.abort();
        }
        self.playing.clear();
        self.routes.clear();
        self.tracks.clear();
    }
}

/// Our camera's send side: its source, transceiver and keepalive.
struct CameraSend {
    source: NativeVideoSource,
    transceiver: RtpTransceiver,
    keepalive: Keepalive,
}

/// Our share's video send side.
struct ScreenSend {
    sink: ShareSink,
    transceiver: RtpTransceiver,
    refresher: Refresher,
    /// VP8 because the hardware encoder failed earlier in this same share:
    /// the next share pushes afresh to try the hardware again.
    fallback: bool,
    /// Its encoder's failure was reported (`take_screen_failure`).
    failure_reported: AtomicBool,
}

impl ScreenSend {
    #[cfg(windows)]
    fn encoder(&self) -> Option<&HwEncoder> {
        match &self.sink {
            ShareSink::Hardware(encoder) => Some(encoder),
            ShareSink::Raw(_) => None,
        }
    }

    #[cfg(not(windows))]
    fn encoder(&self) -> Option<&std::convert::Infallible> {
        None
    }

    /// Its hardware encoder failed: frames go nowhere.
    fn encoder_failed(&self) -> bool {
        #[cfg(windows)]
        {
            self.encoder().is_some_and(HwEncoder::failed)
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

#[cfg(windows)]
fn hw_config(encoding: &ShareEncoding) -> HwConfig {
    HwConfig {
        max_bitrate: encoding.max_bitrate,
        // Presets are 30 or 60 fps; the cap is a whole number.
        fps: encoding.max_framerate.round().clamp(1.0, 240.0) as u32,
        detail: encoding.detail,
    }
}

/// Our share's audio send side, while the share carries audio.
struct ScreenAudioSend {
    source: NativeAudioSource,
    track: RtcAudioTrack,
    transceiver: RtpTransceiver,
    keepalive: AudioKeepalive,
    /// Between shares: disabled, fed silence by the keepalive.
    parked: bool,
}

/// The share's one encoding, and its degradation preference: `detail`
/// keeps the resolution, `motion` the frame rate (what Chrome derives from
/// the track's `contentHint`).
///
/// A hardware share's encoder owns the frame rate and the size (frames the
/// pass-through never sees cannot be dropped or scaled without breaking the
/// H.264 reference chain), so its sender adapts nothing and has no frame
/// rate cap; the bitrate cap still bounds the estimate it reports.
fn apply_screen_encoding(sender: &RtpSender, encoding: &ShareEncoding, hardware: bool) {
    let mut parameters = sender.parameters();
    if parameters.encodings.is_empty() {
        parameters.encodings.push(RtpEncodingParameters::default());
    }
    for e in &mut parameters.encodings {
        e.active = true;
        e.max_bitrate = Some(encoding.max_bitrate);
        e.max_framerate = (!hardware).then_some(encoding.max_framerate);
        e.scale_resolution_down_by = Some(1.0);
    }
    parameters.set_degradation_preference(if hardware {
        DegradationPreference::MaintainFramerateAndResolution
    } else if encoding.detail {
        DegradationPreference::MaintainResolution
    } else {
        DegradationPreference::MaintainFramerate
    });
    if let Err(error) = sender.set_parameters(parameters) {
        eprintln!("[share] encoding refused: {error:?}");
    }
}

/// The highest rate any capture profile runs at.
const CAMERA_MAX_FPS: f64 = 60.0;

/// The camera simulcast encodings in libwebrtc's terms. A layer the web
/// leaves uncapped gets the highest profile rate: the capture is paced to
/// the chosen profile already, and libwebrtc otherwise holds the source to
/// the lowest cap it is given (every layer at `q`'s 15 fps).
fn camera_encodings() -> Vec<RtpEncodingParameters> {
    CAMERA_SIMULCAST
        .iter()
        .map(|layer| RtpEncodingParameters {
            rid: layer.rid.to_owned(),
            active: true,
            scale_resolution_down_by: Some(layer.scale_down_by),
            max_bitrate: layer.max_bitrate,
            max_framerate: layer.max_framerate.or(Some(CAMERA_MAX_FPS)),
            ..RtpEncodingParameters::default()
        })
        .collect()
}

/// The video codecs with VP8 first, as Chrome offers them (the web sets no
/// preference; the SFU answers with the first it takes). The rest keep
/// their order, RTX and FEC included.
fn vp8_first() -> Vec<RtpCodecCapability> {
    let mut codecs = factory()
        .get_rtp_sender_capabilities(MediaType::Video)
        .codecs;
    codecs.sort_by_key(|c| !c.mime_type.eq_ignore_ascii_case("video/vp8"));
    codecs
}

/// A software share's codecs: VP8 first, or VP9 first with
/// `YAP_DEBUG_SHARE_CODEC=vp9` (a test switch: a VP9 stream to watch, for
/// the Media Foundation VP9 decoder; no Yap client sends VP9 today).
fn share_codecs() -> Vec<RtpCodecCapability> {
    let vp9 = std::env::var("YAP_DEBUG_SHARE_CODEC").is_ok_and(|v| v.eq_ignore_ascii_case("vp9"));
    let mut codecs = vp8_first();
    if vp9 {
        codecs.sort_by_key(|c| !c.mime_type.eq_ignore_ascii_case("video/vp9"));
    }
    codecs
}

/// H.264 alone (constrained baseline, mode 1 first), with RTX, for a
/// hardware share: the SFU must not pick another codec, since the
/// pass-through forwards H.264 only.
fn h264_only() -> Vec<RtpCodecCapability> {
    let mut codecs: Vec<_> = factory()
        .get_rtp_sender_capabilities(MediaType::Video)
        .codecs
        .into_iter()
        .filter(|c| {
            c.mime_type.eq_ignore_ascii_case("video/h264")
                || c.mime_type.eq_ignore_ascii_case("video/rtx")
        })
        .collect();
    codecs.sort_by_key(|c| {
        if c.mime_type.eq_ignore_ascii_case("video/rtx") {
            u8::MAX
        } else {
            crate::h264::h264_rank(c.sdp_fmtp_line.as_deref().unwrap_or_default())
        }
    });
    codecs
}

/// The own path's mic: an unbuffered 48 kHz mono source the capture thread
/// feeds, already processed (the source's own options stay off), and its
/// track.
fn own_mic() -> RtcAudioTrack {
    let source = NativeAudioSource::new(AudioSourceOptions::default(), RATE, 1, 0);
    pipeline().set_mic(Some(source.clone()));
    factory().create_audio_track("mic", source)
}

/// One peer connection, the mic track and its sender, the camera, and the
/// remote tracks it keeps alive.
#[derive(Clone)]
pub struct LibwebrtcSession {
    pc: PeerConnection,
    /// The device mic track: `None` on a receive-only video session, and
    /// on a standby until its promotion creates it.
    mic: Arc<Mutex<Option<RtcAudioTrack>>>,
    /// Owns the call's senders (not a receive-only video session).
    sender_session: bool,
    /// A standby's mic: the device track, disabled, with ADM recording off
    /// (the device stays closed) until the promotion enables both.
    standby: Arc<Mutex<Option<RtcAudioTrack>>>,
    /// The mic's send transceiver (stopped by a republish).
    mic_transceiver: Arc<Mutex<Option<RtpTransceiver>>>,
    sender: Arc<Mutex<Option<RtpSender>>>,
    camera: Arc<Mutex<Option<CameraSend>>>,
    screen: Arc<Mutex<Option<ScreenSend>>>,
    screen_audio: Arc<Mutex<Option<ScreenAudioSend>>>,
    /// Mic, camera, share and share audio transceivers a fresh push replaced,
    /// kept, unfed, until the call ends (closing one renegotiates; see
    /// PartyTracks `republishTrack`).
    retired: Arc<Mutex<Vec<RtpTransceiver>>>,
    remotes: Arc<Mutex<Remotes>>,
}

impl MediaEngine for LibwebrtcEngine {
    type Session = LibwebrtcSession;

    fn open(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        let f = &factory();
        if !CALL_OPEN.load(Ordering::Relaxed) {
            // No call runs: a fallback to the ADM takes effect here, never
            // midway through one.
            pipeline::settle_path();
        }
        if !f.acquire_platform_adm() {
            return Err(MediaError("no audio device".into()));
        }
        {
            // Nothing runs yet: select only.
            let mut sel = lock(selection());
            sel.applied_input = None;
            sel.applied_output = None;
            if let Some(line) = apply(f, &mut sel) {
                eprintln!("{line}");
            }
        }
        MIC_LIVE.store(false, Ordering::Relaxed);
        CALL_OPEN.store(true, Ordering::Relaxed);
        if pipeline::enabled() {
            // The ADM only lists; the own path records and plays.
            f.set_adm_recording_enabled(false);
            f.set_adm_playout_enabled(false);
            let mic = own_mic();
            pipeline().start_render();
            pipeline().start_capture();
            return Self::connect(f, ice_servers, Some(mic), true);
        }
        // Both directions through the real devices. Recording is off by
        // default (synthetic mode), and while it is off WebRTC's own
        // Init/StartRecording calls do nothing. It must be on before the
        // device track is created.
        f.set_adm_recording_enabled(true);
        f.set_adm_playout_enabled(true);
        let mic: RtcAudioTrack = f.create_device_audio_track("mic");
        let (session, states) = Self::connect(f, ice_servers, Some(mic), true)?;
        Ok((session, states))
    }

    fn open_standby(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        let f = &factory();
        if !CALL_OPEN.load(Ordering::Relaxed) {
            // No call runs: a fallback to the ADM takes effect here, never
            // midway through one.
            pipeline::settle_path();
        }
        if !f.acquire_platform_adm() {
            return Err(MediaError("no audio device".into()));
        }
        {
            let mut sel = lock(selection());
            sel.applied_input = None;
            sel.applied_output = None;
            if let Some(line) = apply(f, &mut sel) {
                eprintln!("{line}");
            }
        }
        MIC_LIVE.store(false, Ordering::Relaxed);
        CALL_OPEN.store(true, Ordering::Relaxed);
        // Hidden: no recording (the mic stays closed) until the promotion
        // enables the device track; playout is ready.
        f.set_adm_recording_enabled(false);
        if pipeline::enabled() {
            f.set_adm_playout_enabled(false);
            pipeline().start_render();
        } else {
            f.set_adm_playout_enabled(true);
        }
        Self::connect(f, ice_servers, None, true)
    }

    fn open_receiver(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        Self::connect(&factory(), ice_servers, None, false)
    }

    fn release(&self) {
        CALL_OPEN.store(false, Ordering::Relaxed);
        MIC_LIVE.store(false, Ordering::Relaxed);
        if pipeline::enabled() {
            pipeline().stop();
        }
        let f = &factory();
        f.release_platform_adm();
        if !f.is_platform_adm_active() {
            Self::forget_applied();
        }
        release_factory_later();
    }

    fn check_devices(&self) -> Option<String> {
        let f = existing_factory()?;
        if !f.is_platform_adm_active() {
            return None;
        }
        apply(&f, &mut lock(selection()))
    }
}

impl LibwebrtcEngine {
    /// One peer connection with these ICE servers; `mic` is the call's mic
    /// track (none for a receive-only video session).
    fn connect(
        f: &PeerConnectionFactory,
        ice_servers: &[IceServer],
        mic: Option<RtcAudioTrack>,
        sender_session: bool,
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        keep_voice_engine(f);
        let pc = f
            .create_peer_connection({
                let mut config = RtcConfiguration::default();
                config.ice_servers = ice_servers
                    .iter()
                    .map(|s| RtcIceServer {
                        urls: s.urls.clone(),
                        username: s.username.clone(),
                        password: s.password.clone(),
                    })
                    .collect();
                config
            })
            .map_err(err("peer connection"))?;
        let (state_tx, states) = mpsc::unbounded_channel();
        pc.on_connection_state_change(Some(Box::new(move |state| {
            let _ = state_tx.send(match state {
                PeerConnectionState::New => MediaState::New,
                PeerConnectionState::Connecting => MediaState::Connecting,
                PeerConnectionState::Connected => MediaState::Connected,
                PeerConnectionState::Disconnected => MediaState::Disconnected,
                PeerConnectionState::Failed => MediaState::Failed,
                PeerConnectionState::Closed => MediaState::Closed,
            });
        })));
        let remotes = Arc::new(Mutex::new(Remotes::new(Handle::current())));
        let kept = Arc::clone(&remotes);
        pc.on_track(Some(Box::new(move |event| {
            lock(&kept).add(event.transceiver.mid(), event.track);
        })));
        Ok((
            LibwebrtcSession {
                pc,
                mic: Arc::new(Mutex::new(mic)),
                sender_session,
                standby: Arc::new(Mutex::new(None)),
                mic_transceiver: Arc::new(Mutex::new(None)),
                sender: Arc::new(Mutex::new(None)),
                camera: Arc::new(Mutex::new(None)),
                screen: Arc::new(Mutex::new(None)),
                screen_audio: Arc::new(Mutex::new(None)),
                retired: Arc::new(Mutex::new(Vec::new())),
                remotes,
            },
            states,
        ))
    }
}

impl LibwebrtcSession {
    /// Adds the mic as a send-only Opus transceiver and offers it.
    async fn push_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        let mic = lock(&self.mic)
            .clone()
            .or_else(|| lock(&self.standby).clone())
            .ok_or_else(|| MediaError("no mic on a receive-only session".into()))?;
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Audio(mic.clone()),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: vec![],
                },
            )
            .map_err(err("add transceiver"))?;
        *lock(&self.sender) = Some(transceiver.sender());
        *lock(&self.mic_transceiver) = Some(transceiver.clone());
        MIC_LIVE.store(mic.enabled(), Ordering::Relaxed);
        let opus: Vec<_> = factory()
            .get_rtp_sender_capabilities(MediaType::Audio)
            .codecs
            .into_iter()
            .filter(|c| c.mime_type.eq_ignore_ascii_case("audio/opus"))
            .collect();
        let _ = transceiver.set_codec_preferences(opus);
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local offer"))?;
        Ok(LocalOffer {
            sdp: offer.to_string(),
            mid: transceiver.mid().unwrap_or_else(|| "0".into()),
        })
    }
}

impl MediaSession for LibwebrtcSession {
    async fn publish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        self.push_mic(stream_id).await
    }

    async fn publish_standby(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        // The device mic, disabled, while ADM recording is off: nothing opens
        // the device and nothing is sent (the SFU keeps the track about 30 s
        // without media; a later promotion republishes it if it went stale).
        // Not a silent `NativeAudioSource`: LiveKit's WebRTC marks a send
        // stream made for one as external for good, and an external stream
        // never gets device audio, not even after `set_track` to the mic.
        let track = if pipeline::enabled() {
            // The own path's source: nothing feeds it until the promotion
            // starts the mic.
            own_mic()
        } else {
            factory().create_device_audio_track("mic")
        };
        track.set_enabled(false);
        *lock(&self.standby) = Some(track);
        self.push_mic(stream_id).await
    }

    fn promote_mic(&self) -> Result<(), MediaError> {
        if lock(&self.mic).is_some() {
            return Ok(());
        }
        let mic = lock(&self.standby)
            .take()
            .ok_or_else(|| MediaError("the standby pushed no mic".into()))?;
        if pipeline::enabled() {
            mic.set_enabled(true);
            MIC_LIVE.store(true, Ordering::Relaxed);
            *lock(&self.mic) = Some(mic);
            pipeline().start_capture();
            eprintln!("[voice] standby mic promoted (own audio path)");
            return Ok(());
        }
        let f = &factory();
        // Windows' built-in AEC refuses to record before playout runs, and a
        // standby has pulled nothing yet, so nothing has started playout.
        if !f.playout_is_initialized() {
            let _ = f.init_playout() && f.start_playout();
        }
        f.set_adm_recording_enabled(true);
        mic.set_enabled(true);
        MIC_LIVE.store(true, Ordering::Relaxed);
        *lock(&self.mic) = Some(mic);
        // The send stream was already running with recording off, so WebRTC
        // does not start recording by itself; whatever it set up while
        // recording was off is stopped first.
        f.stop_recording();
        let init = f.init_recording();
        let started = init && f.start_recording();
        eprintln!("[voice] standby mic promoted (init {init}, recording {started})");
        Ok(())
    }

    async fn republish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        // The old send is retired, not stopped: a stopped one's m-line is
        // rejected, and the mic's is the BUNDLE tag (the session's first),
        // so the new send would lose the transport and never connect. Its
        // encoding goes inactive, so it sends nothing, and it stays until
        // the call ends, like a replaced camera.
        if let Some(old) = lock(&self.mic_transceiver).take() {
            let sender = old.sender();
            let mut parameters = sender.parameters();
            for encoding in &mut parameters.encodings {
                encoding.active = false;
            }
            if let Err(error) = sender.set_parameters(parameters) {
                eprintln!("[voice] old mic send not deactivated: {error:?}");
            }
            lock(&self.retired).push(old);
        }
        self.push_mic(stream_id).await
    }

    async fn apply_answer(&self, sdp: &str, what: &'static str) -> Result<(), MediaError> {
        let answer = SessionDescription::parse(sdp, SdpType::Answer)
            .map_err(|e| MediaError(format!("{what} sdp: {e:?}")))?;
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(|e| MediaError(format!("remote {what}: {e:?}")))
    }

    fn start_capture(&self) {
        if pipeline::enabled() {
            return;
        }
        let f = &factory();
        if !f.recording_is_initialized() {
            let ok = f.init_recording() && f.start_recording();
            eprintln!("[voice] started recording explicitly: {ok}");
        }
    }

    async fn answer_offer(&self, sdp: &str) -> Result<String, MediaError> {
        let offer = SessionDescription::parse(sdp, SdpType::Offer)
            .map_err(|e| MediaError(format!("offer sdp: {e:?}")))?;
        self.pc
            .set_remote_description(offer)
            .await
            .map_err(err("remote offer"))?;
        let answer = self
            .pc
            .create_answer(AnswerOptions::default())
            .await
            .map_err(err("answer"))?;
        self.pc
            .set_local_description(answer.clone())
            .await
            .map_err(err("local answer"))?;
        Ok(answer.to_string())
    }

    async fn close_mids(&self, mids: &[String]) -> Result<String, MediaError> {
        for transceiver in self.pc.transceivers() {
            if transceiver.mid().is_some_and(|mid| mids.contains(&mid)) {
                let _ = transceiver.stop();
            }
        }
        lock(&self.remotes).forget(mids);
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("close offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local close offer"))?;
        Ok(offer.to_string())
    }

    async fn publish_camera(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        if let Some(old) = lock(&self.camera).take() {
            set_sink(None);
            // Its keepalive goes with it: the old sender gets no frames.
            lock(&self.retired).push(old.transceiver);
        }
        // The size is a hint for the adapter; frames bring their own.
        let source = NativeVideoSource::new(
            VideoResolution {
                width: 1280,
                height: 720,
            },
            false,
        );
        let track = factory().create_video_track("cam-video", source.clone());
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Video(track),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: camera_encodings(),
                },
            )
            .map_err(err("add camera transceiver"))?;
        if transceiver.set_codec_preferences(vp8_first()).is_err() {
            eprintln!("[camera] codec preferences refused; libwebrtc's order stands");
        }
        *lock(&self.camera) = Some(CameraSend {
            keepalive: Keepalive::start(source.clone()),
            source,
            transceiver: transceiver.clone(),
        });
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("camera offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local camera offer"))?;
        let mid = transceiver
            .mid()
            .ok_or_else(|| MediaError("camera transceiver without a mid".into()))?;
        Ok(LocalOffer {
            sdp: offer.to_string(),
            mid,
        })
    }

    fn camera_rids(&self) -> Vec<String> {
        let camera = lock(&self.camera);
        let Some(camera) = camera.as_ref() else {
            return Vec::new();
        };
        let rids: Vec<String> = camera
            .transceiver
            .sender()
            .parameters()
            .encodings
            .into_iter()
            .map(|e| e.rid)
            .filter(|rid| !rid.is_empty())
            .collect();
        if rids.len() > 1 {
            rids
        } else {
            Vec::new()
        }
    }

    fn set_camera_live(&self, live: bool) {
        let camera = lock(&self.camera);
        if let Some(camera) = camera.as_ref() {
            camera.keepalive.park(!live);
            set_sink(live.then(|| camera.source.clone()));
        }
    }

    fn camera_stale(&self) -> bool {
        lock(&self.camera)
            .as_ref()
            .is_none_or(|camera| camera.keepalive.stale())
    }

    fn abandon_camera(&self) {
        if let Some(old) = lock(&self.camera).take() {
            set_sink(None);
            lock(&self.retired).push(old.transceiver);
        }
    }

    fn abandon_screen(&self) {
        if let Some(old) = lock(&self.screen).take() {
            set_share_sink(None);
            lock(&self.retired).push(old.transceiver);
        }
    }

    async fn publish_screen(
        &self,
        stream_id: &str,
        encoding: &ShareEncoding,
    ) -> Result<LocalOffer, MediaError> {
        let old = lock(&self.screen).take();
        // A live share whose hardware encoder failed is being pushed again:
        // it stays on VP8 (a parked one is a new share, which may try).
        let fallback = old
            .as_ref()
            .is_some_and(|o| o.encoder_failed() && !o.refresher.parked());
        if let Some(old) = old {
            set_share_sink(None);
            // Its refresher goes with it: the old sender gets no frames.
            lock(&self.retired).push(old.transceiver);
        }
        // The size is a hint, frames bring their own.
        let hint = VideoResolution {
            width: 1920,
            height: 1080,
        };
        // A hardware encoder when there is one: an encoded source (no raw
        // keepalive), fed the H.264 access units of the encoder thread.
        // The first probe opens an MFT (about 100 ms): off the async worker.
        #[cfg(windows)]
        let probed = if fallback {
            eprintln!("[share] this share's hardware encoder failed; it stays on VP8");
            None
        } else {
            tokio::task::spawn_blocking(super::mf_h264::probe)
                .await
                .ok()
                .flatten()
        };
        #[cfg(windows)]
        let hardware = probed.and_then(|_| {
            let source = NativeVideoSource::new_encoded(hint.clone());
            match HwEncoder::start(source.clone(), hw_config(encoding)) {
                Ok(encoder) => Some((source, ShareSink::Hardware(Arc::new(encoder)))),
                Err(why) => {
                    eprintln!("[share] hardware encoder refused ({why}); VP8");
                    None
                }
            }
        });
        #[cfg(not(windows))]
        let hardware: Option<(NativeVideoSource, ShareSink)> = None;
        let is_hardware = hardware.is_some();
        let (source, sink) = hardware.unwrap_or_else(|| {
            // A screencast source (`getDisplayMedia` tracks are).
            let source = NativeVideoSource::new(hint, true);
            (source.clone(), ShareSink::Raw(source))
        });
        let track = factory().create_video_track("screen-video", source.clone());
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Video(track),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: vec![RtpEncodingParameters {
                        active: true,
                        max_bitrate: Some(encoding.max_bitrate),
                        max_framerate: (!is_hardware).then_some(encoding.max_framerate),
                        scale_resolution_down_by: Some(1.0),
                        ..RtpEncodingParameters::default()
                    }],
                },
            )
            .map_err(err("add share transceiver"))?;
        let codecs = if is_hardware {
            h264_only()
        } else {
            share_codecs()
        };
        if is_hardware {
            transceiver
                .sender()
                .set_video_encoder_backend(VideoEncoderBackend::PreEncoded);
        }
        if transceiver.set_codec_preferences(codecs).is_err() {
            eprintln!("[share] codec preferences refused; libwebrtc's order stands");
        }
        apply_screen_encoding(&transceiver.sender(), encoding, is_hardware);
        eprintln!(
            "[share] publishing {}",
            if is_hardware { "hardware H.264" } else { "VP8" }
        );
        *lock(&self.screen) = Some(ScreenSend {
            refresher: Refresher::start(sink.clone()),
            sink,
            transceiver: transceiver.clone(),
            fallback,
            failure_reported: AtomicBool::new(false),
        });
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("share offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local share offer"))?;
        let mid = transceiver
            .mid()
            .ok_or_else(|| MediaError("share transceiver without a mid".into()))?;
        Ok(LocalOffer {
            sdp: offer.to_string(),
            mid,
        })
    }

    fn set_screen_encoding(&self, encoding: &ShareEncoding) {
        if let Some(screen) = lock(&self.screen).as_ref() {
            #[cfg(windows)]
            if let Some(encoder) = screen.encoder() {
                encoder.set_config(hw_config(encoding));
            }
            apply_screen_encoding(
                &screen.transceiver.sender(),
                encoding,
                screen.encoder().is_some(),
            );
        }
    }

    fn set_screen_live(&self, live: bool) {
        let screen = lock(&self.screen);
        if let Some(screen) = screen.as_ref() {
            screen.refresher.park(!live);
            set_share_sink(live.then(|| screen.sink.clone()));
        } else if !live {
            set_share_sink(None);
        }
    }

    fn screen_stale(&self) -> bool {
        // A hardware encoder that failed cannot be revived: the share pushes
        // afresh, on VP8. A parked VP8 fallback is pushed afresh too, so the
        // next share tries the hardware again (unless the probe remembers a
        // failure that had produced output).
        lock(&self.screen).as_ref().is_none_or(|screen| {
            screen.refresher.stale()
                || screen.encoder_failed()
                || (screen.fallback && screen.refresher.parked())
        })
    }

    fn take_screen_failure(&self) -> bool {
        lock(&self.screen).as_ref().is_some_and(|screen| {
            screen.encoder_failed() && !screen.failure_reported.swap(true, Ordering::Relaxed)
        })
    }

    async fn publish_screen_audio(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        self.abandon_screen_audio();
        // No processing: the source's audio as it plays. Unbuffered, so
        // `push_screen_audio` hands 10 ms frames straight to the track.
        let source = NativeAudioSource::new(
            AudioSourceOptions::default(),
            SHARE_AUDIO_RATE,
            SHARE_AUDIO_CHANNELS,
            0,
        );
        let track = factory().create_audio_track("screen-audio", source.clone());
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Audio(track.clone()),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: vec![],
                },
            )
            .map_err(err("add share audio transceiver"))?;
        let opus: Vec<_> = factory()
            .get_rtp_sender_capabilities(MediaType::Audio)
            .codecs
            .into_iter()
            .filter(|c| c.mime_type.eq_ignore_ascii_case("audio/opus"))
            .collect();
        let _ = transceiver.set_codec_preferences(opus);
        set_share_audio(Some(source.clone()));
        *lock(&self.screen_audio) = Some(ScreenAudioSend {
            keepalive: AudioKeepalive::start(source.clone()),
            source,
            track,
            transceiver: transceiver.clone(),
            parked: false,
        });
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("share audio offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local share audio offer"))?;
        let mid = transceiver
            .mid()
            .ok_or_else(|| MediaError("share audio transceiver without a mid".into()))?;
        Ok(LocalOffer {
            sdp: offer.to_string(),
            mid,
        })
    }

    fn park_screen_audio(&self) {
        set_share_audio(None);
        if let Some(audio) = lock(&self.screen_audio).as_mut() {
            audio.track.set_enabled(false);
            audio.keepalive.park(true);
            audio.parked = true;
        }
    }

    fn resume_screen_audio(&self) -> bool {
        let mut audio = lock(&self.screen_audio);
        let Some(send) = audio.as_mut().filter(|a| a.parked && !a.keepalive.stale()) else {
            return false;
        };
        send.keepalive.park(false);
        send.parked = false;
        send.track.set_enabled(true);
        set_share_audio(Some(send.source.clone()));
        true
    }

    fn abandon_screen_audio(&self) {
        // Nothing feeds a retired send (its keepalive goes with it), so it
        // sends nothing; disabled too.
        set_share_audio(None);
        if let Some(old) = lock(&self.screen_audio).take() {
            old.track.set_enabled(false);
            lock(&self.retired).push(old.transceiver);
        }
    }

    fn set_screen_audio_enabled(&self, enabled: bool) {
        if let Some(audio) = lock(&self.screen_audio).as_ref() {
            audio.track.set_enabled(enabled);
        }
    }

    fn set_mic_enabled(&self, enabled: bool) {
        if let Some(mic) = lock(&self.mic).as_ref() {
            mic.set_enabled(enabled);
            MIC_LIVE.store(enabled, Ordering::Relaxed);
        }
    }

    fn set_playout_enabled(&self, enabled: bool) {
        if pipeline::enabled() && self.sender_session {
            pipeline().set_playout(enabled);
        }
        let mut remotes = lock(&self.remotes);
        remotes.silenced = !enabled;
        for (_, track) in &remotes.tracks {
            if matches!(track, MediaStreamTrack::Audio(_)) {
                track.set_enabled(enabled);
            }
        }
    }

    fn set_processing(&self, processing: Processing) -> Result<(), MediaError> {
        if pipeline::enabled() {
            pipeline().set_processing(processing);
            return Ok(());
        }
        if processing.noise_reduction {
            eprintln!("[voice] RNNoise needs the own audio path (YAP_ADM_AUDIO is set)");
        }
        let Some(mic) = lock(&self.mic).clone() else {
            return Ok(());
        };
        let mic = &mic;
        let id = mic.id();
        // SAFETY: as in `set_volume`; the mic track lives as long as `self`.
        let code = unsafe {
            yap_audio_track_set_processing(
                livekit_track(mic),
                id.as_ptr().cast(),
                id.len(),
                processing.echo_cancellation,
                processing.noise_suppression,
                processing.auto_gain_control,
            )
        };
        match code {
            0 | 1 => Ok(()),
            -1 => Err(MediaError("the mic track did not match".into())),
            code => Err(MediaError(format!("refused (code {code})"))),
        }
    }

    fn set_track_gain(&self, mid: &str, gain: f64) {
        let mut remotes = lock(&self.remotes);
        remotes.gains.insert(mid.to_owned(), gain);
        if pipeline::enabled() {
            if let Some(playing) = remotes.playing.get(mid) {
                playing.stream.set_gain(gain);
            }
            return;
        }
        for (track_mid, track) in &remotes.tracks {
            if let (Some(_), MediaStreamTrack::Audio(audio)) =
                (track_mid.as_deref().filter(|m| *m == mid), track)
            {
                if !set_volume(audio, gain) {
                    eprintln!("[voice] track volume refused on {mid}");
                }
            }
        }
    }

    fn set_track_pan(&self, mid: &str, pan: f64) {
        let mut remotes = lock(&self.remotes);
        remotes.pans.insert(mid.to_owned(), pan);
        if let Some(playing) = remotes.playing.get(mid) {
            playing.stream.set_pan(pan);
        }
    }

    fn set_video_sink(&self, mid: &str, key: Option<&str>) {
        lock(&self.remotes).route(mid, key);
    }

    fn stop_local(&self) {
        if !self.sender_session {
            return;
        }
        set_sink(None);
        set_share_sink(None);
        set_share_audio(None);
        lock(&self.camera).take();
        lock(&self.screen).take();
        lock(&self.screen_audio).take();
        lock(&self.standby).take();
        let Some(mic) = lock(&self.mic).clone() else {
            return;
        };
        mic.set_enabled(false);
        MIC_LIVE.store(false, Ordering::Relaxed);
        if pipeline::enabled() {
            pipeline().stop_capture();
            return;
        }
        let f = &factory();
        if f.recording_is_initialized() {
            f.stop_recording();
        }
    }

    async fn mic_energy(&self) -> Option<(f64, f64)> {
        let sender = lock(&self.sender).clone()?;
        let stats = sender.get_stats().await.ok()?;
        stats.into_iter().find_map(|stat| match stat {
            RtcStats::MediaSource(source) if source.source.kind == "audio" => Some((
                source.audio.total_audio_energy,
                source.audio.total_samples_duration,
            )),
            _ => None,
        })
    }

    async fn stats(&self) -> Result<MediaStats, MediaError> {
        let stats = self
            .pc
            .get_stats()
            .await
            .map_err(|e| MediaError(format!("{e:?}")))?;
        let (gains, routes) = {
            let remotes = lock(&self.remotes);
            (remotes.gains.clone(), remotes.routes.clone())
        };
        let mic_mid = lock(&self.mic_transceiver)
            .as_ref()
            .and_then(RtpTransceiver::mid);
        // The hardware share's own numbers, by its mid.
        #[cfg(windows)]
        let hardware = lock(&self.screen)
            .as_ref()
            .and_then(|screen| Some((screen.transceiver.mid()?, screen.encoder()?.stats())));
        // Codec ids to names (`VP8`, `H264`).
        let codecs: HashMap<String, String> = stats
            .iter()
            .filter_map(|stat| match stat {
                RtcStats::Codec(c) => Some((
                    c.rtc.id.clone(),
                    c.codec
                        .mime_type
                        .split('/')
                        .next_back()
                        .unwrap_or_default()
                        .to_owned(),
                )),
                _ => None,
            })
            .collect();
        let codec_of = |id: &str| codecs.get(id).cloned().unwrap_or_default();
        let mut out = MediaStats::default();
        let mut line = String::from("[voice] stats");
        let mut pair: Option<(f64, String)> = None;
        let mut remote_candidates: HashMap<String, String> = HashMap::new();
        for stat in stats {
            match stat {
                RtcStats::OutboundRtp(o) if o.stream.kind == "video" => {
                    let v = &o.outbound;
                    let mut video = OutboundVideo {
                        mid: v.mid.clone(),
                        rid: v.rid.clone(),
                        bytes_sent: o.sent.bytes_sent,
                        frames_encoded: u64::from(v.frames_encoded),
                        width: v.frame_width,
                        height: v.frame_height,
                        fps: v.frames_per_second,
                        encoder: v.encoder_implementation.clone(),
                        limitation: format!("{:?}", v.quality_limitation_reason).to_lowercase(),
                        active: v.active,
                        codec: codec_of(&o.stream.codec_id),
                        target_bitrate: v.target_bitrate,
                        dropped: 0,
                        encode_ms: if v.frames_encoded > 0 {
                            v.total_encode_time * 1000.0 / f64::from(v.frames_encoded)
                        } else {
                            0.0
                        },
                        hardware: false,
                    };
                    #[cfg(windows)]
                    if let Some((_, hw)) = hardware.as_ref().filter(|(mid, _)| *mid == v.mid) {
                        // The pass-through reports itself; the encoder is ours.
                        video.encoder.clone_from(&hw.name);
                        video.hardware = true;
                        video.dropped = hw.replaced;
                        video.encode_ms = hw.latency_us as f64 / 1000.0;
                        line += &format!(
                            " hw[{} in={} out={} key={} replaced={} rate={}k {:.1}ms]",
                            hw.name,
                            hw.frames_in,
                            hw.frames_out,
                            hw.keyframes,
                            hw.replaced,
                            hw.bitrate / 1000,
                            video.encode_ms
                        );
                    }
                    line += &format!(
                        " out[{}{}]={}B {} {}x{}@{:.0} {} limit={:?}{}",
                        v.mid,
                        if v.rid.is_empty() {
                            String::new()
                        } else {
                            format!("/{}", v.rid)
                        },
                        o.sent.bytes_sent,
                        video.codec,
                        v.frame_width,
                        v.frame_height,
                        v.frames_per_second,
                        video.encoder,
                        v.quality_limitation_reason,
                        if v.active { "" } else { " off" },
                    );
                    out.outbound_video.push(video);
                }
                RtcStats::OutboundRtp(o) => {
                    line += &format!(" out[{}]={}B", o.outbound.mid, o.sent.bytes_sent);
                    if mic_mid.as_deref() == Some(o.outbound.mid.as_str()) {
                        out.mic_bytes_sent = o.sent.bytes_sent;
                        out.mic_packets_sent = o.sent.packets_sent;
                    }
                }
                RtcStats::InboundRtp(inbound) if inbound.stream.kind == "video" => {
                    let v = &inbound.inbound;
                    out.inbound_streams += 1;
                    let video = InboundVideo {
                        mid: v.mid.clone(),
                        key: routes.get(&v.mid).cloned().unwrap_or_default(),
                        width: v.frame_width,
                        height: v.frame_height,
                        fps: v.frames_per_second,
                        codec: codec_of(&inbound.stream.codec_id),
                        decoder: v.decoder_implementation.clone(),
                        frames_decoded: u64::from(v.frames_decoded),
                        frames_dropped: u64::from(v.frames_dropped),
                        decode_ms: if v.frames_decoded > 0 {
                            v.total_decode_time * 1000.0 / f64::from(v.frames_decoded)
                        } else {
                            0.0
                        },
                    };
                    line += &format!(
                        " in[{}]={}B {} {}x{}@{:.0} {} {:.1}ms",
                        v.mid,
                        v.bytes_received,
                        video.codec,
                        v.frame_width,
                        v.frame_height,
                        v.frames_per_second,
                        video.decoder,
                        video.decode_ms
                    );
                    out.inbound_video.push(video);
                }
                RtcStats::InboundRtp(inbound) => {
                    let mid = &inbound.inbound.mid;
                    out.inbound_streams += 1;
                    if inbound.stream.kind == "audio" {
                        out.audio_bytes_received += inbound.inbound.bytes_received;
                        out.audio_packets_received += inbound.received.packets_received;
                        out.audio_packets_lost += inbound.received.packets_lost;
                        out.audio_jitter = out.audio_jitter.max(inbound.received.jitter);
                    }
                    line += &format!(
                        " in[{mid}]={}B level={:.3} gain={:.2}",
                        inbound.inbound.bytes_received,
                        inbound.inbound.audio_level,
                        gains.get(mid).copied().unwrap_or(1.0)
                    );
                }
                RtcStats::MediaSource(source) if source.source.kind == "audio" => {
                    line += &format!(" mic={:.3}", source.audio.audio_level);
                }
                RtcStats::CandidatePair(p) if p.candidate_pair.nominated => {
                    let c = &p.candidate_pair;
                    if pair.is_none() || c.current_round_trip_time > 0.0 {
                        pair = Some((c.current_round_trip_time, c.remote_candidate_id.clone()));
                    }
                }
                RtcStats::RemoteCandidate(c) => {
                    let r = &c.remote_candidate;
                    remote_candidates.insert(
                        c.rtc.id.clone(),
                        format!("{}:{} {}", r.address, r.port, r.protocol),
                    );
                }
                _ => {}
            }
        }
        if pipeline::enabled() && self.sender_session {
            line += " ";
            line += &pipeline().stats();
            (out.mic_device_failed, out.speaker_device_failed) = pipeline().failing();
        }
        if let Some((rtt, remote)) = pair {
            out.round_trip = (rtt > 0.0).then_some(rtt);
            out.server = remote_candidates.remove(&remote).unwrap_or_default();
            if let Some(rtt) = out.round_trip {
                line += &format!(" rtt={:.0}ms", rtt * 1000.0);
            }
        }
        out.line = line;
        Ok(out)
    }

    fn close(&self) {
        // A receive-only session owns none of the call's senders.
        if self.sender_session {
            set_sink(None);
            set_share_sink(None);
            set_share_audio(None);
        }
        lock(&self.camera).take();
        lock(&self.screen).take();
        lock(&self.screen_audio).take();
        lock(&self.retired).clear();
        lock(&self.remotes).clear();
        *lock(&self.sender) = None;
        self.pc.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A call's peer connection going must leave the ADM usable for the
    /// next call. Without the keeper, WebRTC terminates the ADM with the last
    /// peer connection and it lists -1 devices from then on. Needs the
    /// platform's audio stack; skipped where the ADM cannot start.
    #[test]
    fn adm_should_outlive_a_calls_peer_connection() {
        let f = &factory();
        if !f.acquire_platform_adm() {
            return;
        }
        let before = (f.recording_devices(), f.playout_devices());
        keep_voice_engine(f);
        let pc = f
            .create_peer_connection(RtcConfiguration::default())
            .expect("peer connection");
        pc.close();
        drop(pc);
        f.release_platform_adm();
        assert!(f.acquire_platform_adm());
        let after = (f.recording_devices(), f.playout_devices());
        f.release_platform_adm();
        assert_eq!(
            before, after,
            "devices before and after a call's peer connection"
        );
    }

    /// A hardware H.264 share is only watchable by other native clients if
    /// this build decodes H.264 (webrtc-sys drops it from the receive
    /// formats when the prebuilt's FFmpeg lacks the codec).
    #[test]
    fn receiver_should_decode_h264() {
        let caps = factory().get_rtp_receiver_capabilities(MediaType::Video);
        let names: Vec<String> = caps.codecs.iter().map(|c| c.mime_type.clone()).collect();
        eprintln!("receive video codecs: {names:?}");
        assert!(
            names.iter().any(|n| n.eq_ignore_ascii_case("video/h264")),
            "no H.264 decoder in {names:?}"
        );
    }

    /// The device menus list through WASAPI (no factory for that), so its
    /// ids and names must be the ADM's: a saved choice is applied to the
    /// ADM by id. Needs the platform's audio stack; skipped without it.
    #[cfg(windows)]
    #[test]
    fn wasapi_listing_should_match_the_adms() {
        let Some(ours) = super::super::wasapi::list_devices() else {
            return;
        };
        let f = &factory();
        if !f.acquire_platform_adm() {
            return;
        }
        let adm = list_devices(f);
        f.release_platform_adm();
        assert_eq!(
            (ours.inputs, ours.outputs),
            (adm.inputs, adm.outputs),
            "WASAPI and ADM device lists"
        );
    }
}
