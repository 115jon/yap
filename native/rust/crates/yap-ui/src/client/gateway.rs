//! The global gateway: `wss://{api}/api/gateway`, the MeetingRoom DO.
//!
//! The socket lifecycle (tickets, heartbeat and ACK liveness, Resume with
//! the replay cursor, backoff, close codes) lives in `yap_realtime`. This
//! module only turns its events into store events: Ready is a fresh
//! session (the store refetches after a drop), Resumed is a resumed one
//! (the server replayed the gap, nothing is refetched).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
    // Whether the last ticket request found no network (the strip's
    // "Offline"; the web reads `navigator.onLine`). The ticket is the
    // first request of every try, on the same host as the socket. A name
    // that did not resolve while the system says its network is up is not
    // that ([`no_network`]).
    let offline = Arc::new(AtomicBool::new(false));
    let last_offline = Arc::clone(&offline);
    let ticket_events = events.clone();
    let tickets = move || -> yap_realtime::TicketFuture {
        let api = api.clone();
        let last_offline = Arc::clone(&last_offline);
        let events = ticket_events.clone();
        Box::pin(async move {
            let ticket = api.gateway_ticket().await;
            // Refused while the account waits for deletion: the store
            // closes this connection (no retries) and shows the restore
            // screen.
            if ticket.as_ref().is_err_and(ApiError::is_pending_deletion) {
                events.pending_deletion();
            }
            let no_network = no_network(ticket.as_ref().err(), || {
                crate::platform::installed().and_then(|p| p.network_up())
            });
            last_offline.store(no_network, Ordering::Relaxed);
            ticket.map_err(ticket_error)
        })
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    // Dropped on return, which closes the socket.
    let connection = Connection::spawn(config, tickets, tx);
    let mut was_ready = false;

    loop {
        tokio::select! {
            out = outgoing.recv() => match out {
                Some(text) => {
                    // Our game activity (op 43) is logged when it leaves.
                    if text.starts_with(r#"{"op":43,"#) {
                        eprintln!("[games] op 43 sent");
                    }
                    connection.send(text)
                }
                None => return,
            },
            event = rx.recv() => {
                let Some(event) = event else { return };
                if !handle(&events, event, &mut was_ready, &offline) {
                    return;
                }
            }
        }
    }
}

/// A failed ticket request found no network: the name did not resolve,
/// and the system does not say its network is up (`network_up`, asked only
/// then). A phone app's own network is blocked in the background while the
/// device's stays up; its lookups fail the same way.
fn no_network(error: Option<&ApiError>, network_up: impl FnOnce() -> Option<bool>) -> bool {
    error.is_some_and(ApiError::is_offline) && network_up() != Some(true)
}

/// Forwards one connection event; `false` once the connection stopped.
fn handle(
    events: &GatewayEvents,
    event: Event,
    was_ready: &mut bool,
    offline: &AtomicBool,
) -> bool {
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
        Event::Reconnecting { attempt, .. } => {
            // The debug window (YAP_DEBUG_OFFLINE_SECS) fails a try before
            // it asks for a ticket.
            let offline = offline.load(Ordering::Relaxed) || yap_realtime::debug_offline();
            events.reconnecting(attempt, offline);
        }
        Event::Stopped(StopReason::SessionExpired) => {
            events.session_expired();
            return false;
        }
        Event::Stopped(reason) => {
            eprintln!("[gateway] stopped: {reason:?}");
            return false;
        }
        Event::Connected | Event::Frame(..) | Event::IdentifyRejected => {}
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    const DNS: &str = "error sending request: client error (Connect): dns error: no network";

    #[test]
    fn a_failed_lookup_is_no_network_unless_the_system_says_it_is_up() {
        let dns = ApiError::Network(DNS.into());
        // The desktop, which cannot ask: as before.
        assert!(no_network(Some(&dns), || None));
        assert!(no_network(Some(&dns), || Some(false)));
        // A phone whose app was blocked in the background.
        assert!(!no_network(Some(&dns), || Some(true)));
    }

    #[test]
    fn other_failures_and_success_never_ask() {
        let refused = ApiError::Network("connection refused".into());
        let ask = || -> Option<bool> { panic!("asked") };
        assert!(!no_network(Some(&refused), ask));
        assert!(!no_network(Some(&ApiError::Unauthorized), ask));
        assert!(!no_network(None, ask));
    }
}
