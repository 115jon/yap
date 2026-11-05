//! Plugs `yap_voice` calls into the store: the store's target and commands
//! in, its `VoiceUpdate`s out. The call itself (state machine, sockets,
//! SFU, libwebrtc media) lives in `yap_voice`. The voice settings
//! (volumes, sensitivity, processing) reach the call from [`crate::audio`]
//! through [`set_call_audio`]; the mic level goes back the same way.

use std::collections::VecDeque;
use std::sync::OnceLock;

use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use yap_core::VoiceShare;
use yap_core::{config, Api, ApiError, VoiceCommand, VoiceEvents, VoiceMember, VoiceUpdate};
use yap_voice::{
    run_call, ApiFailure, CallEnv, CallTarget, LibwebrtcEngine, Notice, Playback, Processing,
    Sensitivity, ShareEnd, ShareInfo, ShareNotice, ShareRequest, SourceKind, TicketApi,
    UserCommand, VideoIntent,
};

pub use yap_core::VoiceTarget as Target;

/// What a call takes from the voice settings.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallAudio {
    pub playback: Playback,
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

/// Socket tickets through the signed-in API.
#[derive(Clone)]
struct Tickets(Api);

impl TicketApi for Tickets {
    async fn socket_ticket(&self, body: Value) -> Result<Value, ApiFailure> {
        self.0
            .post("/api/voice/socket-ticket", &body)
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
                    return command.map(|command| match command {
                        VoiceCommand::SetMuted(muted) => UserCommand::SetMuted(muted),
                        VoiceCommand::SetDeafened(deafened) => UserCommand::SetDeafened(deafened),
                        VoiceCommand::SetWatched(watched) => {
                            UserCommand::SetWatched(watched.into_iter().collect())
                        }
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
                        VoiceCommand::SetShareQuality(quality) => {
                            UserCommand::SetShareQuality(quality)
                        }
                        VoiceCommand::Leave => UserCommand::Leave,
                    });
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

/// Runs one call until it is left or ends; reports through `events`.
pub async fn run(
    api: Api,
    http: reqwest::Client,
    target: Target,
    commands: UnboundedReceiver<VoiceCommand>,
    events: VoiceEvents,
) {
    static OUTPUT: std::sync::Once = std::sync::Once::new();
    OUTPUT.call_once(|| {
        LibwebrtcEngine::set_video_output(std::sync::Arc::new(crate::video::QtOutput))
    });
    let cfg = config::get();
    let env = CallEnv {
        engine: LibwebrtcEngine,
        tickets: Tickets(api),
        capturer: yap_capture::Capturer,
        http,
        api_base: cfg.api_base.clone(),
        origin: cfg.gateway_origin.clone(),
    };
    let target = CallTarget {
        server_id: target.server_id,
        channel_id: target.channel_id,
        me_name: target.me_name,
        me_username: target.me_username,
    };
    let notify = |notice: Notice| match notice {
        Notice::State(state) => eprintln!("[voice] {state}"),
        Notice::Status(status) => events.send(VoiceUpdate::Status(status.into())),
        Notice::Connected => events.send(VoiceUpdate::Connected),
        Notice::Rejoined => events.send(VoiceUpdate::Rejoined),
        Notice::MicLevel(level) => crate::audio::set_live_level(Some(level)),
        Notice::Camera { on, error } => events.send(VoiceUpdate::Camera { on, error }),
        Notice::Share(notice) => events.send(share_update(notice)),
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
                })
                .collect(),
        )),
    };
    let error = run_call(env, target, Commands::new(commands), notify).await;
    crate::audio::set_live_level(None);
    events.send(VoiceUpdate::Ended(error));
}
