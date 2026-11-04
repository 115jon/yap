//! The client core lives in `yap-core` (typed store, reducer, effect runner,
//! REST and auth). This module plugs the realtime pieces that still live in
//! this crate (the gateway socket and voice calls) into its runtime, and
//! keeps the `spawn` / [`Command`] / [`Ui`] / [`Sink`] interface the bridge uses.

pub mod gateway;
pub mod sfu;
pub mod socket;
pub mod voice;

use std::sync::Arc;

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_core::{Api, GatewayEvents, GatewayHandle, Realtime, VoiceCommand, VoiceEvents};

pub use yap_core::{Command, Sink, Ui};

struct NativeRealtime;

impl Realtime for NativeRealtime {
    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle {
        let (outgoing, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(api, user_id, rx, events));
        GatewayHandle { outgoing, task }
    }

    fn spawn_voice(
        &self,
        api: Api,
        http: reqwest::Client,
        target: voice::Target,
        commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        tokio::spawn(voice::run(api, http, target, commands, events));
    }
}

/// Starts the core and returns its command channel.
pub fn spawn(sink: Sink) -> UnboundedSender<Command> {
    // Fails only if the TLS backend cannot start; there is no client then.
    let http = yap_core::api::http_client().expect("HTTP client");
    yap_core::spawn(
        sink,
        http,
        Box::new(NativeRealtime),
        Arc::new(yap_core::auth::Keyring),
    )
}
