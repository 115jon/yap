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
//! transceiver; the old one is detached, not closed.
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

use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{
    AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState,
};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{
    IceServer as RtcIceServer, PeerConnectionFactory, RtcConfiguration,
};
use libwebrtc::prelude::RtcAudioTrack;
use libwebrtc::rtp_parameters::{RtpCodecCapability, RtpEncodingParameters};
use libwebrtc::rtp_sender::RtpSender;
use libwebrtc::rtp_transceiver::{RtpTransceiver, RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::stats::RtcStats;
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_source::VideoResolution;
use libwebrtc::MediaType;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::{I420Frame, LocalOffer, MediaEngine, MediaError, MediaSession, VideoOutput};
use crate::audio::{resolve, AudioDevice, AudioDevices, Processing};
use crate::camera::live::{set_sink, Keepalive};
use crate::layers::CAMERA_SIMULCAST;
use crate::machine::MediaState;
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
    mic: RtcAudioTrack,
    sender: Arc<Mutex<Option<RtpSender>>>,
    camera: Arc<Mutex<Option<CameraSend>>>,
    /// Camera transceivers a fresh push replaced, detached until the call
    /// ends (closing one renegotiates; see PartyTracks `republishTrack`).
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
                mic,
                sender: Arc::new(Mutex::new(None)),
                camera: Arc::new(Mutex::new(None)),
                retired: Arc::new(Mutex::new(Vec::new())),
                remotes,
            },
            states,
        ))
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

impl MediaSession for LibwebrtcSession {
    async fn publish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Audio(self.mic.clone()),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: vec![],
                },
            )
            .map_err(err("add transceiver"))?;
        *lock(&self.sender) = Some(transceiver.sender());
        MIC_LIVE.store(self.mic.enabled(), Ordering::Relaxed);
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
            if old.transceiver.sender().set_track(None).is_err() {
                eprintln!("[camera] could not detach the old transceiver");
            }
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
            .is_some_and(|camera| camera.keepalive.stale())
    }

    fn set_mic_enabled(&self, enabled: bool) {
        self.mic.set_enabled(enabled);
        MIC_LIVE.store(enabled, Ordering::Relaxed);
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
        let id = self.mic.id();
        // SAFETY: as in `set_volume`; the mic track lives as long as `self`.
        let code = unsafe {
            yap_audio_track_set_processing(
                livekit_track(&self.mic),
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
        set_sink(None);
        lock(&self.camera).take();
        self.mic.set_enabled(false);
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

    async fn stats(&self) -> Result<String, MediaError> {
        let stats = self
            .pc
            .get_stats()
            .await
            .map_err(|e| MediaError(format!("{e:?}")))?;
        let gains = lock(&self.remotes).gains.clone();
        let mut line = String::from("[voice] stats");
        for stat in stats {
            match stat {
                RtcStats::OutboundRtp(out) if !out.outbound.rid.is_empty() => {
                    let o = &out.outbound;
                    line += &format!(
                        " out[{}/{}]={}B {}x{}@{:.0} {} limit={:?}{}",
                        o.mid,
                        o.rid,
                        out.sent.bytes_sent,
                        o.frame_width,
                        o.frame_height,
                        o.frames_per_second,
                        o.encoder_implementation,
                        o.quality_limitation_reason,
                        if o.active { "" } else { " off" },
                    );
                }
                RtcStats::OutboundRtp(out) => {
                    line += &format!(" out[{}]={}B", out.outbound.mid, out.sent.bytes_sent);
                }
                RtcStats::InboundRtp(inbound) => {
                    let mid = &inbound.inbound.mid;
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
                _ => {}
            }
        }
        Ok(line)
    }

    fn close(&self) {
        set_sink(None);
        lock(&self.camera).take();
        lock(&self.retired).clear();
        lock(&self.remotes).clear();
        *lock(&self.sender) = None;
        self.pc.close();
    }
}
