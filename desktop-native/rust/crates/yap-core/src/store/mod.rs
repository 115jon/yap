//! The client's state store: typed [`State`], the [`Input`]s that change it,
//! the [`Effect`]s it asks for, and [`reduce`], which is pure and synchronous
//! (no I/O, no clock, no `await`). The runtime executes the effects and feeds
//! their results back as inputs.
//!
//! Staleness is one rule: every request carries a [`RequestId`], the state
//! keeps the id it is waiting for per slot (servers, a server's channels, the
//! open channel's page, each send), and a result for any other id is dropped.
//! Gateway, voice and sign-in inputs carry the session, call or attempt they
//! belong to, with the same rule.

mod effect;
mod input;
mod reduce;
mod state;
pub mod time;
pub mod view;

pub use effect::{ApiRequest, ApiResponse, CacheWrite, Effect};
pub use input::{Command, Dispatch, GatewayEvent, Input, SignInEvent};
pub use reduce::reduce;
pub use state::{
    GatewayStatus, MessageLog, Phase, ServerChannels, Session, State, VoiceCall, VoicePrefs,
};
pub use time::LocalOffset;
pub use view::{Row, ViewDiff, ViewUpdate};
pub use yap_cache::Snapshot;

/// Which HTTP request a result answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

/// One signed-in session (a token from `UseToken`, the store or sign-in).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(pub u64);

/// One voice call. A new join is a new id; updates from older calls are dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CallId(pub u64);

/// One device sign-in attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SignInId(pub u64);

/// What a voice session needs to join.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceTarget {
    pub server_id: String,
    pub channel_id: String,
    pub me_name: String,
    pub me_username: String,
}

/// Commands to a running voice session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoiceCommand {
    SetMuted(bool),
    SetDeafened(bool),
    Leave,
}

/// One participant in a call, as the voice session reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceMember {
    /// The participant id.
    pub id: String,
    /// The Yap user id when the room named it (empty otherwise).
    pub user_id: String,
    pub name: String,
    pub muted: bool,
    pub deafened: bool,
    pub speaking: bool,
    /// This client's own participant.
    pub me: bool,
}

/// Someone in a server voice channel, as the gateway reports it for the
/// sidebar (`VOICE_CHANNEL_STATES`); profile fields carry over from the
/// last full state of the same user.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Occupant {
    pub user_id: String,
    pub name: String,
    /// As sent (possibly relative); empty when there is none.
    pub avatar_url: String,
    pub muted: bool,
    pub deafened: bool,
    pub video: bool,
    pub stream: bool,
}

/// What a voice session reports back.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceUpdate {
    Status(String),
    Connected,
    /// The whole roster, in a stable order.
    Roster(Vec<VoiceMember>),
    /// The call is over; the error text if it failed.
    Ended(Option<String>),
}
