//! Yap's realtime sockets: one [`Connection`] type for the main gateway and
//! both call sockets. It owns tickets, Hello and heartbeats (with ACK
//! liveness), Resume with the replay cursor, backoff and the close-code
//! policy; the owner keeps the protocol (what Ready and the other ops mean).
//! Tokio and tungstenite only; no Qt.

pub mod connection;
pub mod policy;

pub use connection::{
    debug_drop_after, debug_offline, debug_offline_for, Config, Connection, Event, Reconnect,
    StopReason, TicketError, TicketFuture, TicketSource, SUBPROTOCOL,
};
pub use policy::{close_action, Backoff, CloseAction, Policy};

/// `{api_base}{path}` with the scheme swapped to `ws`/`wss`.
pub fn ws_url(api_base: &str, path: &str) -> String {
    let base = api_base
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    format!("{base}{path}")
}
