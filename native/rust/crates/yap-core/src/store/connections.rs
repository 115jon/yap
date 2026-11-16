//! Settings › Connections in the store ([`crate::connections`],
//! ConnectionsPanel.tsx): the list, Connect (in the browser), the claim
//! when the browser hands the outcome back, Unlink and Show on profile.

use serde::Serialize;

use super::RequestId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionsCommand {
    /// The panel showed: load the list (again).
    Load,
    /// Connect: ask for the begin URL, which the UI opens in the browser.
    /// `app` is this platform (`windows`, `android`).
    Connect {
        provider: String,
        app: String,
    },
    Unlink(String),
    SetShown {
        provider: String,
        show: bool,
    },
    /// The outcome toast was read.
    DismissNotice,
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionsOp {
    List,
    Start(String),
    Complete(String),
    Unlink(String),
    /// The value before the switch, to put back on a refusal.
    SetShown {
        provider: String,
        previous: bool,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConnectionsState {
    pub(super) list: Option<Vec<crate::connections::OwnConnection>>,
    pub(super) loading: Option<RequestId>,
    pub(super) load_error: String,
    pub(super) pending: std::collections::HashMap<RequestId, ConnectionsOp>,
    /// The provider and action in flight (`connect`, `complete`,
    /// `unlink`, `show`), at most one at a time.
    pub(super) busy: Option<(String, &'static str)>,
    pub(super) action_error: String,
    /// The last outcome: its tone and words.
    pub(super) notice: Option<(&'static str, String)>,
    /// The begin URL to open, and a count that moves with each.
    pub(super) open_url: String,
    pub(super) open_seq: u64,
    /// Moves when a returning link asks the UI to show Connections.
    pub(super) show_seq: u64,
    pub(super) shown: ConnectionsView,
}

/// One provider's card.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionCard {
    pub provider: String,
    pub label: String,
    pub blurb: String,
    pub linked: bool,
    pub display_name: String,
    pub profile_url: String,
    pub avatar_url: String,
    pub show_on_profile: bool,
    /// When it was verified, as a local date ("Nov 14, 2026").
    pub verified: String,
}

/// What QML draws (`Session.connections`, JSON).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionsView {
    pub loaded: bool,
    pub loading: bool,
    pub load_error: String,
    pub cards: Vec<ConnectionCard>,
    /// `<action>:<provider>` in flight, or empty.
    pub busy: String,
    pub action_error: String,
    /// `success`, `info` or `error`; empty without a notice.
    pub notice_tone: String,
    pub notice: String,
    pub open_url: String,
    pub open_seq: u64,
    pub show_seq: u64,
}
