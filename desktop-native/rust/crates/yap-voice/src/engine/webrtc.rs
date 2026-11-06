//! The production engine: libwebrtc's platform audio device module (real
//! mic and speakers, with WebRTC's APM: echo cancellation, noise
//! suppression, gain control) and one peer connection per call.
//!
//! Devices: the choice ([`LibwebrtcEngine::choose_devices`]) is kept for the
//! process and applied by endpoint id whenever the ADM is up; in a call it
//! switches at once (stop, select, init, start). Two track calls the crate
//! lacks (remote volume, live APM options) go through `cpp/track_audio.cpp`.
//!
//! Camera: one send-only VP8 transceiver with the web's three simulcast
//! encodings, fed by `crate::camera` (captured frames while live, the
//! keepalive's black frames while parked). A stale park pushes a fresh
//! transceiver; the old one is retired, not closed: it keeps its track but
//! nothing feeds it any more. (Not `set_track(None)`: webrtc-sys 0.3.52's
//! `RtpSender::set_track` dereferences the null track and crashes.)
//!
//! Share: one send-only VP8 transceiver with a single encoding at the
//! preset's caps, fed by `crate::share` (captured frames while live, the
//! last one repeated while the source is still, the keepalive's black
//! frames while parked), and an Opus transceiver for the source's audio fed
//! from a `NativeAudioSource`. Stopping parks the video for the next share
//! and retires the audio.
//!
//! Video: each routed remote video track gets a `NativeVideoStream` (a
//! WebRTC sink with a one-frame queue: a slow consumer sees the newest frame,
//! never a backlog) pumped by a tokio task that hands I420 planes to the
//! installed [`VideoOutput`]. A decoded I420 buffer is passed as is; other
//! buffer kinds are converted once. An unrouted track has no sink at all, so
//! its frames cost nothing past the decoder.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, RwLock};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use libwebrtc::video_frame::{VideoBuffer, VideoRotation};
use libwebrtc::video_stream::native::NativeVideoStream;
use libwebrtc::video_track::RtcVideoTrack;
use tokio::runtime::Handle;
use tokio::task::JoinHandle;

use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::audio_source::AudioSourceOptions;
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
use libwebrtc::rtp_sender::RtpSender;
use libwebrtc::rtp_transceiver::{RtpTransceiver, RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::stats::RtcStats;
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_source::VideoResolution;
use libwebrtc::MediaType;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::{
    I420Frame, LocalOffer, MediaEngine, MediaError, MediaSession, MediaStats, OutboundVideo,
    VideoOutput,
};
use crate::audio::{resolve, AudioDevice, AudioDevices, Processing};
use crate::camera::live::{set_sink, Keepalive};
use crate::layers::CAMERA_SIMULCAST;
use crate::machine::MediaState;
use crate::share::live::{
    set_audio as set_share_audio, set_sink as set_share_sink, AudioKeepalive, Refresher,
};
use crate::share::{ShareEncoding, SHARE_AUDIO_CHANNELS, SHARE_AUDIO_RATE};
use crate::wire::IceServer;

/// One process-wide factory: it owns the platform ADM.
fn factory() -> &'static PeerConnectionFactory {
    static FACTORY: OnceLock<PeerConnectionFactory> = OnceLock::new();
    FACTORY.get_or_init(PeerConnectionFactory::default)
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

    /// The platform's input and output devices. Outside a call this opens
    /// the ADM briefly (no capture or playout starts). Blocks on WebRTC's
    /// worker thread: call it off the GUI thread.
    pub fn devices(&self) -> AudioDevices {
        let f = factory();
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
        let f = factory();
        let mut sel = lock(selection());
        sel.input = input;
        sel.output = output;
        if f.is_platform_adm_active() {
            apply(f, &mut sel)
        } else {
            None
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
        let rotation = match frame.rotation {
            VideoRotation::VideoRotation0 => 0,
            VideoRotation::VideoRotation90 => 90,
            VideoRotation::VideoRotation180 => 180,
            VideoRotation::VideoRotation270 => 270,
        };
        let size = (i420.width(), i420.height());
        output.frame(
            &key,
            &I420Frame {
                width: size.0,
                height: size.1,
                rotation,
                y,
                u,
                v,
                stride_y,
                stride_u,
                stride_v,
            },
        );
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

/// The remote tracks (the ADM mixes and plays the audio ones), by receive
/// mid, and the video routes.
struct Remotes {
    tracks: Vec<(Option<String>, MediaStreamTrack)>,
    /// Gains set by mid, also for tracks that have not arrived yet.
    gains: HashMap<String, f64>,
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
                if let Some(gain) = mid.as_ref().and_then(|m| self.gains.get(m)) {
                    if !set_volume(audio, *gain) {
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

    /// Forgets these mids: their gains, routes, pumps and tracks.
    fn forget(&mut self, mids: &[String]) {
        for mid in mids {
            self.route(mid, None);
            self.gains.remove(mid);
        }
        self.tracks
            .retain(|(mid, _)| !mid.as_ref().is_some_and(|m| mids.contains(m)));
    }

    fn clear(&mut self) {
        for (_, pump) in self.pumps.drain() {
            pump.abort();
        }
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
    source: NativeVideoSource,
    transceiver: RtpTransceiver,
    refresher: Refresher,
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
fn apply_screen_encoding(sender: &RtpSender, encoding: &ShareEncoding) {
    let mut parameters = sender.parameters();
    if parameters.encodings.is_empty() {
        parameters.encodings.push(RtpEncodingParameters::default());
    }
    for e in &mut parameters.encodings {
        e.active = true;
        e.max_bitrate = Some(encoding.max_bitrate);
        e.max_framerate = Some(encoding.max_framerate);
        e.scale_resolution_down_by = Some(1.0);
    }
    parameters.set_degradation_preference(if encoding.detail {
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
    /// A standby's silent stand-in for the mic (no device is touched).
    standby: Arc<Mutex<Option<RtcAudioTrack>>>,
    /// The mic's send transceiver (stopped by a republish).
    mic_transceiver: Arc<Mutex<Option<RtpTransceiver>>>,
    sender: Arc<Mutex<Option<RtpSender>>>,
    camera: Arc<Mutex<Option<CameraSend>>>,
    screen: Arc<Mutex<Option<ScreenSend>>>,
    screen_audio: Arc<Mutex<Option<ScreenAudioSend>>>,
    /// Camera, share and share audio transceivers a fresh push replaced,
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
        let f = factory();
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
        let f = factory();
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
        // creates the device track; playout is ready.
        f.set_adm_recording_enabled(false);
        f.set_adm_playout_enabled(true);
        Self::connect(f, ice_servers, None, true)
    }

    fn open_receiver(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        Self::connect(factory(), ice_servers, None, false)
    }

    fn release(&self) {
        CALL_OPEN.store(false, Ordering::Relaxed);
        MIC_LIVE.store(false, Ordering::Relaxed);
        let f = factory();
        f.release_platform_adm();
        if !f.is_platform_adm_active() {
            Self::forget_applied();
        }
    }

    fn check_devices(&self) -> Option<String> {
        let f = factory();
        if !f.is_platform_adm_active() {
            return None;
        }
        apply(f, &mut lock(selection()))
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
        // Silence nobody feeds: the SFU keeps the track about 30 s without
        // media; a later promotion republishes it if it went stale.
        let source = NativeAudioSource::new(AudioSourceOptions::default(), 48_000, 1, 0);
        let track = factory().create_audio_track("standby", source);
        track.set_enabled(false);
        *lock(&self.standby) = Some(track);
        self.push_mic(stream_id).await
    }

    fn promote_mic(&self) -> Result<(), MediaError> {
        if lock(&self.mic).is_some() {
            return Ok(());
        }
        let f = factory();
        // Recording must be on before the device track exists.
        f.set_adm_recording_enabled(true);
        let mic: RtcAudioTrack = f.create_device_audio_track("mic");
        let sender = lock(&self.sender)
            .clone()
            .ok_or_else(|| MediaError("the standby pushed no mic".into()))?;
        sender
            .set_track(Some(MediaStreamTrack::Audio(mic.clone())))
            .map_err(|e| MediaError(format!("promote mic: {e:?}")))?;
        MIC_LIVE.store(mic.enabled(), Ordering::Relaxed);
        *lock(&self.mic) = Some(mic);
        lock(&self.standby).take();
        // The send stream was already running on the silence, so WebRTC
        // does not start recording for the new track by itself; whatever
        // it set up while recording was off is stopped first.
        f.stop_recording();
        let init = f.init_recording();
        let started = init && f.start_recording();
        eprintln!("[voice] standby mic promoted (init {init}, recording {started})");
        Ok(())
    }

    async fn republish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        // The old send goes with the new offer (its m-line is rejected).
        if let Some(old) = lock(&self.mic_transceiver).take() {
            let _ = old.stop();
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
        let f = factory();
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
        if let Some(old) = lock(&self.screen).take() {
            set_share_sink(None);
            // Its refresher goes with it: the old sender gets no frames.
            lock(&self.retired).push(old.transceiver);
        }
        // A screencast source (`getDisplayMedia` tracks are); the size is a
        // hint, frames bring their own.
        let source = NativeVideoSource::new(
            VideoResolution {
                width: 1920,
                height: 1080,
            },
            true,
        );
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
                        max_framerate: Some(encoding.max_framerate),
                        scale_resolution_down_by: Some(1.0),
                        ..RtpEncodingParameters::default()
                    }],
                },
            )
            .map_err(err("add share transceiver"))?;
        if transceiver.set_codec_preferences(vp8_first()).is_err() {
            eprintln!("[share] codec preferences refused; libwebrtc's order stands");
        }
        apply_screen_encoding(&transceiver.sender(), encoding);
        *lock(&self.screen) = Some(ScreenSend {
            refresher: Refresher::start(source.clone()),
            source,
            transceiver: transceiver.clone(),
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
            apply_screen_encoding(&screen.transceiver.sender(), encoding);
        }
    }

    fn set_screen_live(&self, live: bool) {
        let screen = lock(&self.screen);
        if let Some(screen) = screen.as_ref() {
            screen.refresher.park(!live);
            set_share_sink(live.then(|| screen.source.clone()));
        } else if !live {
            set_share_sink(None);
        }
    }

    fn screen_stale(&self) -> bool {
        lock(&self.screen)
            .as_ref()
            .is_none_or(|screen| screen.refresher.stale())
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
        let mut remotes = lock(&self.remotes);
        remotes.silenced = !enabled;
        for (_, track) in &remotes.tracks {
            if matches!(track, MediaStreamTrack::Audio(_)) {
                track.set_enabled(enabled);
            }
        }
    }

    fn set_processing(&self, processing: Processing) -> Result<(), MediaError> {
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
        let f = factory();
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
        let gains = lock(&self.remotes).gains.clone();
        let mic_mid = lock(&self.mic_transceiver)
            .as_ref()
            .and_then(RtpTransceiver::mid);
        let mut out = MediaStats::default();
        let mut line = String::from("[voice] stats");
        let mut pair: Option<(f64, String)> = None;
        let mut remote_candidates: HashMap<String, String> = HashMap::new();
        for stat in stats {
            match stat {
                RtcStats::OutboundRtp(o) if o.stream.kind == "video" => {
                    let v = &o.outbound;
                    line += &format!(
                        " out[{}{}]={}B {}x{}@{:.0} {} limit={:?}{}",
                        v.mid,
                        if v.rid.is_empty() {
                            String::new()
                        } else {
                            format!("/{}", v.rid)
                        },
                        o.sent.bytes_sent,
                        v.frame_width,
                        v.frame_height,
                        v.frames_per_second,
                        v.encoder_implementation,
                        v.quality_limitation_reason,
                        if v.active { "" } else { " off" },
                    );
                    out.outbound_video.push(OutboundVideo {
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
                    });
                }
                RtcStats::OutboundRtp(o) => {
                    line += &format!(" out[{}]={}B", o.outbound.mid, o.sent.bytes_sent);
                    if mic_mid.as_deref() == Some(o.outbound.mid.as_str()) {
                        out.mic_bytes_sent = o.sent.bytes_sent;
                        out.mic_packets_sent = o.sent.packets_sent;
                    }
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
