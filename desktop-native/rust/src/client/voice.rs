//! Plugs `yap_voice` calls into the store: the store's target and commands
//! in, its `VoiceUpdate`s out. The call itself (state machine, sockets,
//! SFU, libwebrtc media) lives in `yap_voice`.

use serde_json::Value;
use tokio::sync::mpsc::UnboundedReceiver;
use yap_core::{config, Api, ApiError, VoiceCommand, VoiceEvents, VoiceMember, VoiceUpdate};
use yap_voice::{
    run_call, ApiFailure, CallEnv, CallTarget, LibwebrtcEngine, Notice, TicketApi, UserCommand,
};

pub use yap_core::VoiceTarget as Target;

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

/// The store's command channel, as call commands.
struct Commands(UnboundedReceiver<VoiceCommand>);

impl yap_voice::Commands for Commands {
    async fn recv(&mut self) -> Option<UserCommand> {
        self.0.recv().await.map(|command| match command {
            VoiceCommand::SetMuted(muted) => UserCommand::SetMuted(muted),
            VoiceCommand::SetDeafened(deafened) => UserCommand::SetDeafened(deafened),
            VoiceCommand::Leave => UserCommand::Leave,
        })
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
    let cfg = config::get();
    let env = CallEnv {
        engine: LibwebrtcEngine,
        tickets: Tickets(api),
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
                })
                .collect(),
        )),
    };
    let error = run_call(env, target, Commands(commands), notify).await;
    events.send(VoiceUpdate::Ended(error));
}
