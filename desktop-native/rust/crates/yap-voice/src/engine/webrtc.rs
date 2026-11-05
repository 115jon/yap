//! The production engine: libwebrtc's platform audio device module (real
//! mic and speakers, with WebRTC's APM: echo cancellation, noise
//! suppression, gain control) and one peer connection per call.
//!
//! Devices: the choice ([`LibwebrtcEngine::choose_devices`]) is kept for the
//! process and applied by endpoint id whenever the ADM is up; in a call it
//! switches at once (stop, select, init, start). Two track calls the crate
//! lacks (remote volume, live APM options) go through `cpp/track_audio.cpp`.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{
    AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState,
};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{
    IceServer as RtcIceServer, PeerConnectionFactory, RtcConfiguration,
};
use libwebrtc::prelude::RtcAudioTrack;
use libwebrtc::rtp_sender::RtpSender;
use libwebrtc::rtp_transceiver::{RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::stats::RtcStats;
use libwebrtc::MediaType;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::{LocalOffer, MediaEngine, MediaError, MediaSession};
use crate::audio::{resolve, AudioDevice, AudioDevices, Processing};
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

/// The remote tracks (the ADM mixes and plays them), by receive mid.
#[derive(Default)]
struct Remotes {
    tracks: Vec<(Option<String>, MediaStreamTrack)>,
    /// Gains set by mid, also for tracks that have not arrived yet.
    gains: HashMap<String, f64>,
    /// Deafened: remote tracks are disabled, new ones included.
    silenced: bool,
}

impl Remotes {
    fn add(&mut self, mid: Option<String>, track: MediaStreamTrack) {
        if self.silenced {
            track.set_enabled(false);
        }
        if let (Some(gain), MediaStreamTrack::Audio(audio)) =
            (mid.as_ref().and_then(|m| self.gains.get(m)), &track)
        {
            if !set_volume(audio, *gain) {
                eprintln!("[voice] track volume refused for a new track");
            }
        }
        self.tracks.push((mid, track));
    }
}

/// One peer connection, the mic track and its sender, and the remote
/// tracks it keeps alive.
#[derive(Clone)]
pub struct LibwebrtcSession {
    pc: PeerConnection,
    mic: RtcAudioTrack,
    sender: Arc<Mutex<Option<RtpSender>>>,
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
        let remotes = Arc::new(Mutex::new(Remotes::default()));
        let kept = Arc::clone(&remotes);
        pc.on_track(Some(Box::new(move |event| {
            lock(&kept).add(event.transceiver.mid(), event.track);
        })));
        Ok((
            LibwebrtcSession {
                pc,
                mic,
                sender: Arc::new(Mutex::new(None)),
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
        {
            let mut remotes = lock(&self.remotes);
            remotes.gains.retain(|mid, _| !mids.contains(mid));
            remotes
                .tracks
                .retain(|(mid, _)| !mid.as_ref().is_some_and(|m| mids.contains(m)));
        }
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

    fn set_mic_enabled(&self, enabled: bool) {
        self.mic.set_enabled(enabled);
        MIC_LIVE.store(enabled, Ordering::Relaxed);
    }

    fn set_playout_enabled(&self, enabled: bool) {
        let mut remotes = lock(&self.remotes);
        remotes.silenced = !enabled;
        for (_, track) in &remotes.tracks {
            track.set_enabled(enabled);
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
        lock(&self.remotes).tracks.clear();
        *lock(&self.sender) = None;
        self.pc.close();
    }
}
