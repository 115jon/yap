//! The global gateway: `wss://{api}/api/gateway`, the MeetingRoom DO.
//!
//! The socket lifecycle (tickets, heartbeat and ACK liveness, Resume with
//! the replay cursor, backoff, close codes) lives in `yap_realtime`. This
//! module only turns its events into store events: Ready is a fresh
//! session (the store refetches after a drop), Resumed is a resumed one
//! (the server replayed the gap, nothing is refetched).

use serde_json::{json, Value};
use tokio::sync::mpsc::{self, UnboundedReceiver};

use yap_core::{config, Api, ApiError, GatewayEvents};
use yap_protocol::GatewayOp;
use yap_realtime::{Config, Connection, Event, Policy, Reconnect, StopReason, TicketError};

/// Maps a ticket request failure; a 401 counts towards "session expired".
pub fn ticket_error(error: ApiError) -> TicketError {
    match error {
        ApiError::Unauthorized => TicketError::Unauthorized,
        other => TicketError::Failed(other.to_string()),
    }
}

/// Runs until `outgoing` closes or the task is aborted.
pub async fn run(
    api: Api,
    user_id: String,
    mut outgoing: UnboundedReceiver<String>,
    events: GatewayEvents,
) {
    let cfg = config::get();
    let identify = json!({
        "op": GatewayOp::Identify.code(),
        "d": {
            "name": "ChatClient",
            "clerk_user_id": user_id,
            "platform": "desktop",
            "supports_voice_state_deltas": true,
        }
    });
    let config = Config {
        label: "gateway",
        url: yap_realtime::ws_url(&cfg.api_base, "/api/gateway"),
        origin: cfg.gateway_origin.clone(),
        identify,
        reconnect: Reconnect::Resume,
        ready_op: GatewayOp::Ready,
        policy: Policy::CHAT,
        debug_drop_after: yap_realtime::debug_drop_after(),
    };
    let tickets = move || -> yap_realtime::TicketFuture {
        let api = api.clone();
        Box::pin(async move { api.socket_ticket("global").await.map_err(ticket_error) })
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Dropped on return, which closes the socket.
    let connection = Connection::spawn(config, tickets, tx);
    let mut was_ready = false;

    loop {
        tokio::select! {
            out = outgoing.recv() => match out {
                Some(text) => connection.send(text),
                None => return,
            },
            event = rx.recv() => {
                let Some(event) = event else { return };
                if !handle(&events, event, &mut was_ready) {
                    return;
                }
            }
        }
    }
}

/// Forwards one connection event; `false` once the connection stopped.
fn handle(events: &GatewayEvents, event: Event, was_ready: &mut bool) -> bool {
    match event {
        Event::Ready(_) => {
            if std::mem::replace(was_ready, true) {
                eprintln!("[gateway] identified fresh after a drop; refetching");
            } else {
                let ms = yap_core::started_at().elapsed().as_millis();
                eprintln!("[startup] gateway ready after {ms} ms");
            }
            events.ready();
        }
        Event::Resumed(_) => {
            eprintln!("[gateway] resumed");
            events.resumed();
        }
        Event::Frame(GatewayOp::Dispatch, d) => {
            let event = d.get("event").and_then(Value::as_str).unwrap_or_default();
            let payload = d.get("data").cloned().unwrap_or(Value::Null);
            events.dispatch(event, payload);
        }
        Event::Frame(GatewayOp::Error, d) => eprintln!("[gateway] error frame: {d}"),
        Event::Disconnected { .. } => events.closed(),
        Event::Stopped(StopReason::SessionExpired) => {
            events.session_expired();
            return false;
        }
        Event::Stopped(reason) => {
            eprintln!("[gateway] stopped: {reason:?}");
            return false;
        }
        Event::Connected
        | Event::Frame(..)
        | Event::IdentifyRejected
        | Event::Reconnecting { .. } => {}
    }
    true
}
