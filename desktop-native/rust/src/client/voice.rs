//! Plugs `yap_voice` calls into the store: the store's target and commands
//! in, its `VoiceUpdate`s out. The call itself (state machine, sockets,
//! SFU, libwebrtc media) lives in `yap_voice`. The voice settings
//! (volumes, sensitivity, processing) reach the call from [`crate::audio`]
//! through [`set_call_audio`]; the mic level goes back the same way.

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use yap_core::store::calls::{CallIncomingVideoStat, CallStats, CallVideoStat};
use yap_core::VoiceShare;
use yap_core::{
    config, Api, ApiError, VoiceCommand, VoiceEvents, VoiceMember, VoiceMoved, VoiceUpdate,
};
use yap_voice::{
    run_call, AdmissionApi, ApiFailure, CallEnv, CallTarget, LibwebrtcEngine, Notice, Playback,
    Processing, Sensitivity, ShareEnd, ShareInfo, ShareNotice, ShareRequest, SourceKind,
    SpatialLocal, SpatialState, UserCommand, VideoIntent,
};

pub use yap_core::VoiceTarget as Target;

/// What a call takes from the voice settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallAudio {
    pub playback: Playback,
    /// Our spatial audio switches and user id.
    pub spatial: SpatialLocal,
    pub sensitivity: Sensitivity,
    pub processing: Processing,
}

fn call_audio() -> &'static watch::Sender<CallAudio> {
    static AUDIO: OnceLock<watch::Sender<CallAudio>> = OnceLock::new();
    AUDIO.get_or_init(|| watch::channel(CallAudio::default()).0)
}

/// The settings for this call and later ones.
pub fn set_call_audio(audio: CallAudio) {
    call_audio().send_if_modified(|current| {
        let changed = *current != audio;
        *current = audio;
        changed
    });
}

/// Call admission (`realtime/ADMISSION.md`) through the signed-in API.
#[derive(Clone)]
struct Admission(Api);

impl AdmissionApi for Admission {
    async fn admission(&self, body: Value) -> Result<Value, ApiFailure> {
        // `YAP_TEST_VOICE_FAIL=admission` (yap_core::faults): the join's
        // admission answers 500, as a broken voice backend would.
        if yap_core::faults::voice_fail() == Some(yap_core::faults::VoiceFail::Admission) {
            let error = yap_core::faults::status_error(500);
            eprintln!("[voice] test switch: admission fails ({error})");
            return Err(ApiFailure {
                unauthorized: false,
                message: error.to_string(),
            });
        }
        self.0
            .post("/api/voice/admission", &body)
            .await
            .map_err(|error| ApiFailure {
                unauthorized: matches!(error, ApiError::Unauthorized),
                message: error.to_string(),
            })
    }
}

/// The store's command channel and the voice settings, as call commands.
struct Commands {
    store: UnboundedReceiver<VoiceCommand>,
    audio: watch::Receiver<CallAudio>,
    video: watch::Receiver<VideoIntent>,
    /// Settings commands not handed out yet.
    pending: VecDeque<UserCommand>,
}

impl Commands {
    fn new(store: UnboundedReceiver<VoiceCommand>) -> Self {
        let mut audio = call_audio().subscribe();
        let first = audio.borrow_and_update().clone();
        let mut video = crate::video::subscribe();
        let shown = video.borrow_and_update().clone();
        let mut commands = Self {
            store,
            audio,
            video,
            pending: VecDeque::new(),
        };
        commands.queue(&first);
        commands.pending.push_back(UserCommand::SetVideo(shown));
        commands
    }

    fn queue(&mut self, audio: &CallAudio) {
        self.pending.extend([
            UserCommand::SetPlayback(audio.playback.clone()),
            UserCommand::SetSensitivity(audio.sensitivity),
            UserCommand::SetProcessing(audio.processing),
            UserCommand::SetSpatial(audio.spatial.clone()),
        ]);
    }
}

impl yap_voice::Commands for Commands {
    async fn recv(&mut self) -> Option<UserCommand> {
        loop {
            if let Some(command) = self.pending.pop_front() {
                return Some(command);
            }
            tokio::select! {
                command = self.store.recv() => {
                    if let Some(command) = user_command(command?) {
                        return Some(command);
                    }
                }
                Ok(()) = self.video.changed() => {
                    return Some(UserCommand::SetVideo(self.video.borrow_and_update().clone()));
                }
                // The sender is a static: this never ends.
                Ok(()) = self.audio.changed() => {
                    let audio = self.audio.borrow_and_update().clone();
                    self.queue(&audio);
                }
            }
        }
    }
}

/// A store command as the call takes it; `None` drops it (a spatial edit
/// that is not a state: dropped, not sent as the defaults).
fn user_command(command: VoiceCommand) -> Option<UserCommand> {
    Some(match command {
        VoiceCommand::SetMuted(muted) => UserCommand::SetMuted(muted),
        VoiceCommand::SetDeafened(deafened) => UserCommand::SetDeafened(deafened),
        VoiceCommand::SetWatched(watched) => UserCommand::SetWatched(watched.into_iter().collect()),
        VoiceCommand::SetCamera(on) => UserCommand::SetCamera(on),
        VoiceCommand::StartShare(share) => UserCommand::StartShare(ShareRequest {
            source_id: share.source_id,
            source_name: share.source_name,
            kind: if share.window {
                SourceKind::Window
            } else {
                SourceKind::Monitor
            },
            quality: share.quality,
            audio: share.audio,
        }),
        VoiceCommand::StopShare => UserCommand::StopShare,
        VoiceCommand::SetShareAudio(on) => UserCommand::SetShareAudio(on),
        VoiceCommand::SetShareQuality(quality) => UserCommand::SetShareQuality(quality),
        VoiceCommand::SendApp(frame) => UserCommand::SendApp(frame),
        VoiceCommand::SetSoundboardSpeaking(on) => UserCommand::SetSoundboardSpeaking(on),
        VoiceCommand::SetSpatialRoom(json) => {
            let state = serde_json::from_str::<Value>(&json)
                .ok()
                .as_ref()
                .and_then(SpatialState::from_value);
            if state.is_none() {
                eprintln!("[voice] spatial edit dropped: not a state");
            }
            UserCommand::SetSpatialRoom(state?)
        }
        VoiceCommand::Promote => UserCommand::Promote,
        VoiceCommand::Leave => UserCommand::Leave,
    })
}

/// A live share as the store keeps it.
fn store_share(info: ShareInfo) -> VoiceShare {
    VoiceShare {
        source_id: info.request.source_id,
        source_name: info.request.source_name,
        window: info.request.kind == SourceKind::Window,
        quality: info.request.quality,
        audio: info.audio,
    }
}

/// The share's notices as store updates (the stop sound only for a live
/// share the user stopped, as the web plays it).
fn share_update(notice: ShareNotice) -> VoiceUpdate {
    match notice {
        ShareNotice::Started(info) => VoiceUpdate::ShareStarted(store_share(info)),
        ShareNotice::Updated(info) => VoiceUpdate::ShareUpdated(store_share(info)),
        ShareNotice::Ended {
            reason,
            announced,
            error,
        } => {
            eprintln!(
                "[share] ended ({reason}{})",
                if announced { "" } else { ", unannounced" }
            );
            VoiceUpdate::ShareEnded {
                stop_sound: announced && reason == ShareEnd::Stopped,
                error,
            }
        }
    }
}

/// Our live share's stills for everyone's roster (`yap_voice::still`, the
/// web's `STREAM_PREVIEW_CAPTURE_*`): the first shortly after it goes live,
/// then one every 8 s, each the last frame it sent at most 224 wide as a
/// JPEG `data:` URL the gateway takes. The store sends it on (op 15
/// `stream_preview_url`) when it changed. Aborted when the share ends.
fn share_stills(events: VoiceEvents) -> tokio::task::JoinHandle<()> {
    use yap_voice::still::{fit_still_url, STILL_EVERY, STILL_FIRST_AFTER};
    tokio::spawn(async move {
        tokio::time::sleep(STILL_FIRST_AFTER).await;
        loop {
            let url = tokio::task::spawn_blocking(|| fit_still_url(yap_voice::share_still, still_jpeg))
                .await
                .ok()
                .flatten();
            if let Some(url) = url {
                events.send(VoiceUpdate::SharePreview(url));
            }
            tokio::time::sleep(STILL_EVERY).await;
        }
    })
}

/// A share still as a JPEG at `quality` (1-100).
fn still_jpeg(still: &yap_voice::Still, quality: u8) -> Option<Vec<u8>> {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
        .encode(
            &still.rgb,
            still.width,
            still.height,
            image::ExtendedColorType::Rgb8,
        )
        .ok()?;
    Some(jpeg)
}

/// The wall clock in epoch ms (app events are stamped with it; the store
/// has no clock).
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// What the disconnect beacon needs (voice-disconnect-beacon.ts): the
/// call's channel and server, our voice session, and the signed-in API.
#[derive(Clone)]
struct Beacon {
    api: Api,
    channel_id: String,
    server_id: String,
    participant_id: String,
}

/// The joined (not standby) call's beacon, if there is one.
fn beacon() -> &'static Mutex<Option<Beacon>> {
    static BEACON: Mutex<Option<Beacon>> = Mutex::new(None);
    &BEACON
}

/// The beacon as kept on disk while a call is up, so a process that dies
/// mid-call (a crash, a kill, a power cut) can still say it left: the next
/// start sends it ([`send_stale_beacon`]). Without it the room holds the
/// dead session for a resume, and the user shows as reconnecting in the
/// channel for its 120 s grace (90 s more if the close never arrived).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct StoredBeacon {
    user_id: String,
    server_id: String,
    channel_id: String,
    participant_id: String,
}

/// `voice-beacon.json` in this profile's folder.
fn beacon_file() -> Option<std::path::PathBuf> {
    yap_cache::profile::data_root().map(|root| root.join("voice-beacon.json"))
}

fn store_beacon(stored: &StoredBeacon) {
    let Some(path) = beacon_file() else {
        return;
    };
    let written = std::fs::create_dir_all(path.parent().unwrap_or(&path))
        .and_then(|()| std::fs::write(&path, serde_json::to_vec(stored).unwrap_or_default()));
    if let Err(error) = written {
        eprintln!("[voice] beacon not kept: {error}");
    }
}

/// Forgets the kept beacon if it is `participant_id`'s (any, for `None`).
fn forget_stored_beacon(participant_id: Option<&str>) {
    let Some(path) = beacon_file() else {
        return;
    };
    let ours = match participant_id {
        None => true,
        Some(id) => read_stored_beacon(&path).is_some_and(|b| b.participant_id == id),
    };
    if ours {
        let _ = std::fs::remove_file(&path);
    }
}

fn read_stored_beacon(path: &std::path::Path) -> Option<StoredBeacon> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// At sign-in (once per process): a beacon kept by an earlier process
/// means it died in a call. Sends it, so the room drops that session now
/// rather than after its resume grace; a join that already replaced it
/// makes this a no-op. Another account's beacon is dropped unsent.
pub fn send_stale_beacon(api: &Api, user_id: &str) {
    static ONCE: std::sync::Once = std::sync::Once::new();
    let mut first = false;
    ONCE.call_once(|| first = true);
    if !first {
        return;
    }
    let Some(path) = beacon_file() else {
        return;
    };
    let Some(stale) = read_stored_beacon(&path) else {
        let _ = std::fs::remove_file(&path);
        return;
    };
    let live = beacon()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .as_ref()
        .is_some_and(|b| b.participant_id == stale.participant_id);
    if live {
        return;
    }
    let _ = std::fs::remove_file(&path);
    if stale.user_id != user_id || stale.server_id.is_empty() {
        return;
    }
    let api = api.clone();
    tokio::spawn(async move {
        let body = serde_json::json!({
            "server_id": stale.server_id,
            "gateway_session_id": null,
            "voice_session_id": stale.participant_id,
        });
        let path = format!("/api/channels/{}/voice-disconnect", stale.channel_id);
        let result = api.post(&path, &body).await;
        eprintln!(
            "[voice] stale call beacon: {}",
            match result {
                Ok(_) => "sent".to_owned(),
                Err(error) => error.to_string(),
            }
        );
    });
}

/// On quit (voice-disconnect-beacon.ts on `pagehide`): `POST
/// /api/channels/{id}/voice-disconnect` with our voice session, so the room
/// drops us at once rather than after the heartbeat timeout. Waits at most
/// `wait`; a server voice channel only (the route ignores DM calls).
pub fn send_disconnect_beacon(wait: Duration) {
    let Some(beacon) = beacon()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take()
    else {
        return;
    };
    // Sent now (or the call's own leave goes out on quit): nothing is left
    // for the next start to send.
    forget_stored_beacon(Some(&beacon.participant_id));
    if beacon.server_id.is_empty() {
        return;
    }
    let (done, finished) = std::sync::mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("yap-beacon".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            let body = serde_json::json!({
                "server_id": beacon.server_id,
                "gateway_session_id": null,
                "voice_session_id": beacon.participant_id,
            });
            let path = format!("/api/channels/{}/voice-disconnect", beacon.channel_id);
            let result = runtime.block_on(beacon.api.post(&path, &body));
            eprintln!(
                "[voice] disconnect beacon: {}",
                match result {
                    Ok(_) => "sent".to_owned(),
                    Err(error) => error.to_string(),
                }
            );
            let _ = done.send(());
        });
    if spawned.is_ok() {
        let _ = finished.recv_timeout(wait);
    }
}

fn call_stats(stats: yap_voice::VoiceStats) -> CallStats {
    let ticks: Vec<(u64, [f64; 4])> = stats
        .samples
        .iter()
        .map(|s| (s.at_ms, [s.bitrate, s.loss, s.jitter, s.rtt]))
        .collect();
    let (samples, window_s) = yap_core::store::calls::chart_samples(&ticks);
    CallStats {
        quality: stats.quality.to_owned(),
        rtt_ms: stats.rtt_ms,
        packet_loss_rate: stats.packet_loss_rate,
        jitter_ms: stats.jitter_ms,
        bitrate_kbps: stats.bitrate_kbps,
        server: stats.server,
        room: stats.room,
        remote_tracks: stats.remote_tracks,
        video: stats
            .outbound_video
            .into_iter()
            .map(|v| CallVideoStat {
                mid: v.mid,
                rid: v.rid,
                width: v.width,
                height: v.height,
                fps: v.fps,
                encoder: v.encoder,
                limitation: v.limitation,
                frames: v.frames_encoded,
                active: v.active,
                codec: v.codec,
                bitrate_kbps: v.target_bitrate / 1000.0,
                dropped: v.dropped,
                encode_ms: v.encode_ms,
                hardware: v.hardware,
            })
            .collect(),
        incoming: stats
            .inbound_video
            .into_iter()
            .map(|v| CallIncomingVideoStat {
                key: v.key,
                width: v.width,
                height: v.height,
                fps: v.fps,
                codec: v.codec,
                decoder: v.decoder,
                frames: v.frames_decoded,
                dropped: v.frames_dropped,
                decode_ms: v.decode_ms,
            })
            .collect(),
        samples,
        window_s,
    }
}

/// Runs one call until it is left or ends; reports through `events`.
pub async fn run(
    api: Api,
    http: reqwest::Client,
    target: Target,
    commands: UnboundedReceiver<VoiceCommand>,
    events: VoiceEvents,
) {
    crate::video::install_output();
    let cfg = config::get();
    let beacon_api = api.clone();
    let env = CallEnv {
        engine: LibwebrtcEngine,
        admission: Admission(api),
        capturer: yap_capture::Capturer,
        http,
        api_base: cfg.api_base.clone(),
        origin: cfg.gateway_origin.clone(),
    };
    // Our voice session in this call, once the voice socket is ready.
    let my_participant: Mutex<Option<String>> = Mutex::new(None);
    // Our live share's still task (`share_stills`).
    let stills: Mutex<Option<tokio::task::JoinHandle<()>>> = Mutex::new(None);
    let (beacon_channel, beacon_server) = (target.channel_id.clone(), target.server_id.clone());
    let beacon_user = target.me_id.clone();
    let target = CallTarget {
        server_id: target.server_id,
        channel_id: target.channel_id,
        me_name: target.me_name,
        me_username: target.me_username,
        room_slug: target.room_slug,
        move_grant: target.move_grant,
        standby: target.standby,
    };
    let notify = |notice: Notice| match notice {
        Notice::State(state) => eprintln!("[voice] {state}"),
        Notice::Status(status) => events.send(VoiceUpdate::Status(status.into())),
        Notice::Connected => events.send(VoiceUpdate::Connected),
        Notice::Rejoined => events.send(VoiceUpdate::Rejoined),
        Notice::MicLevel(level) => crate::audio::set_live_level(Some(level)),
        Notice::Camera { on, error } => events.send(VoiceUpdate::Camera { on, error }),
        Notice::Share(notice) => {
            let mut task = stills.lock().unwrap_or_else(PoisonError::into_inner);
            match &notice {
                ShareNotice::Started(_) => {
                    if let Some(old) = task.replace(share_stills(events.clone())) {
                        old.abort();
                    }
                }
                ShareNotice::Ended { .. } => {
                    if let Some(old) = task.take() {
                        old.abort();
                    }
                }
                ShareNotice::Updated(_) => {}
            }
            drop(task);
            events.send(share_update(notice));
        }
        Notice::VoiceReady { participant_id } => {
            *my_participant
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(participant_id.clone());
            *beacon().lock().unwrap_or_else(PoisonError::into_inner) = Some(Beacon {
                api: beacon_api.clone(),
                channel_id: beacon_channel.clone(),
                server_id: beacon_server.clone(),
                participant_id: participant_id.clone(),
            });
            if !beacon_server.is_empty() {
                store_beacon(&StoredBeacon {
                    user_id: beacon_user.clone(),
                    server_id: beacon_server.clone(),
                    channel_id: beacon_channel.clone(),
                    participant_id: participant_id.clone(),
                });
            }
            events.send(VoiceUpdate::VoiceReady { participant_id });
        }
        Notice::Moved(to) => events.send(VoiceUpdate::Moved(VoiceMoved {
            server_id: to.server_id,
            channel_id: to.channel_id,
            channel_name: to.channel_name.unwrap_or_default(),
            mover_name: to.mover_name.unwrap_or_default(),
            grant: to.grant,
        })),
        Notice::Stats(stats) => events.send(VoiceUpdate::Stats(Box::new(call_stats(stats)))),
        Notice::InboundAudioStalled(stalled) => events.send(VoiceUpdate::InboundStalled(stalled)),
        Notice::MicStalled(stalled) => events.send(VoiceUpdate::MicStalled(stalled)),
        Notice::Spatial(map) => events.send(VoiceUpdate::Spatial(map.to_value().to_string())),
        Notice::App(d) => {
            if let Some(event) = yap_protocol::voice_app::AppEvent::decode(&d) {
                events.send(VoiceUpdate::App {
                    event: Box::new(event),
                    now_ms: now_ms(),
                });
            }
        }
        Notice::Roster(members) => events.send(VoiceUpdate::Roster(
            members
                .into_iter()
                .map(|m| VoiceMember {
                    id: m.id,
                    user_id: m.user_id,
                    name: m.name,
                    muted: m.muted,
                    deafened: m.deafened,
                    speaking: m.speaking,
                    me: m.me,
                    video: m.video,
                    stream: m.stream,
                    camera_live: m.camera_live,
                    screen_live: m.screen_live,
                    screen_audio: m.screen_audio,
                })
                .collect(),
        )),
    };
    let error = run_call(env, target, Commands::new(commands), notify).await;
    if let Some(task) = stills.lock().unwrap_or_else(PoisonError::into_inner).take() {
        task.abort();
    }
    // Only our own beacon: a newer call (a move, a standby promotion) may
    // already have replaced it.
    let mine = my_participant
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    if let Some(mine) = mine {
        beacon()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take_if(|b| b.participant_id == mine);
        // The call ended in this process: its leave (or the room's own
        // end) already went out.
        forget_stored_beacon(Some(&mine));
    }
    crate::audio::set_live_level(None);
    events.send(if error.as_deref() == Some(yap_voice::CALL_RESET) {
        VoiceUpdate::Reset
    } else {
        VoiceUpdate::Ended(error)
    });
}
