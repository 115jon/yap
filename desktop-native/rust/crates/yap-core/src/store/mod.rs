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

mod admin;
mod effect;
mod input;
mod reduce;
mod state;
pub mod time;
pub mod view;

pub use admin::{
    Act, AdminCommand, AdminList, AdminRequest, AdminResponse, AdminState, OverrideDraft,
    ServerDraft,
};
pub use effect::{ApiRequest, ApiResponse, CacheWrite, Effect, Method, Page, ProfileAsset};
pub use input::{Command, Dispatch, GatewayEvent, Input, ProfileDraft, SignInEvent, UploadFile};
pub use reduce::reduce;
pub use state::{
    live_status, pending_id, Divider, GatewayStatus, GifBrowse, MessageLog, Outgoing, Phase,
    Presence, ProfileStep, ReactionChange, ServerChannels, Session, SettingsState, State, Typer,
    Upload, UploadStatus, VoiceCall, VoicePrefs,
};
pub use time::LocalOffset;
pub use view::{
    DmRow, DmView, EmojiRow, GifRow, GifStatusView, Load, MemberListRow, MentionRow, ProfileView,
    Row, SessionRow, SettingsView, ShareRow, UnreadView, UploadRow, ViewDiff, ViewUpdate,
};
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCommand {
    SetMuted(bool),
    SetDeafened(bool),
    /// The participants whose shares the user watches.
    SetWatched(Vec<String>),
    /// Our camera on or off.
    SetCamera(bool),
    /// Share a source (always a new capture; the latest request wins).
    StartShare(VoiceShare),
    StopShare,
    /// The share's audio on or off.
    SetShareAudio(bool),
    /// The share's quality preset id.
    SetShareQuality(String),
    Leave,
}

/// A screen share as asked for, or as it is live.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VoiceShare {
    /// `monitor-<index>` or `window-<hwnd>`.
    pub source_id: String,
    pub source_name: String,
    /// The source is a window (else a monitor).
    pub window: bool,
    /// A preset id (`1080p30`).
    pub quality: String,
    /// Asked: send the source's audio. Live: the share carries it
    /// (`self_stream_audio`).
    pub audio: bool,
}

/// One participant in a call, as the voice session reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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
    /// Camera on (`self_video`).
    pub video: bool,
    /// Sharing their screen (`self_stream`).
    pub stream: bool,
    /// Their camera's frames arrive.
    pub camera_live: bool,
    /// Their share's frames arrive.
    pub screen_live: bool,
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
    /// The room said Ready for the first time.
    Connected,
    /// The room identified us afresh mid-call: presence goes out again.
    Rejoined,
    /// The whole roster, in a stable order.
    Roster(Vec<VoiceMember>),
    /// Our camera is on or off now; `error` says why turning it on failed.
    Camera {
        on: bool,
        error: Option<String>,
    },
    /// Our share went live (the start sound plays).
    ShareStarted(VoiceShare),
    /// Our live share moved to a new capture, or its audio or quality
    /// changed.
    ShareUpdated(VoiceShare),
    /// Our share is over. `stop_sound`: a live share the user stopped;
    /// `error` when it failed.
    ShareEnded {
        stop_sound: bool,
        error: Option<String>,
    },
    /// The call is over; the error text if it failed.
    Ended(Option<String>),
}
