//! Friends and relationships (FriendsView.tsx, ProfileCard.tsx
//! `useProfileActions`, src/routes/api/friends.ts): what the UI asks for
//! ([`FriendsCommand`]), the REST calls shaped as the web sends them, their
//! answers, what the store keeps ([`FriendsState`]) and what the UI shows
//! ([`FriendsView`]).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Value};
use yap_protocol::rest::{FriendRequestSent, Relationship};

use super::state::Presence;
use super::view::{avatar_url, initials};
use super::{Method, RequestId};
use crate::api::ApiError;
use crate::settings::describe_api_error;

/// A friends action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendsCommand {
    /// The Friends view opened (FriendsView.tsx `fetchFriends` on mount):
    /// the list loads, with the loading rows while it is empty.
    Load,
    /// A profile card opened (UserProfilePopover.tsx `loadRelationships`):
    /// the list loads quietly.
    Refresh,
    /// Friends → Add friend → Send request, with what the field holds.
    SendRequest(String),
    /// The field changed: an error under it goes (FriendsView.tsx `onChange`).
    ClearAdd,
    /// A row's or a profile card's action on one person. `username` is read
    /// by `Add` only; `name` completes the card's error copy.
    Act {
        action: FriendAction,
        user_id: String,
        username: String,
        name: String,
    },
    /// Forget a person's last outcome (a card opening again starts clean).
    Clear(String),
}

/// What an action does to a relationship (ProfileCard.tsx `Busy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FriendAction {
    /// Send a request by their username (the card's Add friend).
    Add,
    Accept,
    /// An incoming request: Ignore (FriendsView) or Decline (the card).
    Decline,
    /// Our outgoing request.
    Cancel,
    /// A friend.
    Remove,
    Block,
    Unblock,
}

impl FriendAction {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "add" => Self::Add,
            "accept" => Self::Accept,
            "decline" => Self::Decline,
            "cancel" => Self::Cancel,
            "remove" => Self::Remove,
            "block" => Self::Block,
            "unblock" => Self::Unblock,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Accept => "accept",
            Self::Decline => "decline",
            Self::Cancel => "cancel",
            Self::Remove => "remove",
            Self::Block => "block",
            Self::Unblock => "unblock",
        }
    }

    /// What follows "Unable to" (ProfileCard.tsx `ACTION_COPY`).
    pub fn copy(self, name: &str) -> String {
        match self {
            Self::Add => format!("send {name} a friend request"),
            Self::Accept => format!("accept {name}'s friend request"),
            Self::Decline => format!("decline {name}'s friend request"),
            Self::Cancel => format!("cancel your friend request to {name}"),
            Self::Remove => format!("update your friendship with {name}"),
            Self::Block => format!("block {name}"),
            Self::Unblock => format!("unblock {name}"),
        }
    }
}

/// The REST calls of `/api/friends`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FriendsRequest {
    /// Every relationship, newest first.
    List,
    /// `POST {username}`.
    Send { username: String },
    /// `PUT {target_user_id, action: "accept"}`.
    Accept { user_id: String },
    /// `PUT {target_user_id, action: "block"}`.
    Block { user_id: String },
    /// `DELETE {target_user_id}`: remove, ignore, cancel or unblock.
    Remove { user_id: String },
}

/// The answers the store reads.
#[derive(Debug, Clone, PartialEq)]
pub enum FriendsResponse {
    List(Vec<Relationship>),
    /// A request went out: the `type` it made (0 when it accepted theirs).
    Sent(Option<i64>),
    Done,
}

impl FriendsRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::List => Method::Get,
            Self::Send { .. } => Method::Post,
            Self::Accept { .. } | Self::Block { .. } => Method::Put,
            Self::Remove { .. } => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        "/api/friends".into()
    }

    pub fn body(&self) -> Value {
        match self {
            Self::List => Value::Null,
            Self::Send { username } => json!({ "username": username }),
            Self::Accept { user_id } => json!({ "target_user_id": user_id, "action": "accept" }),
            Self::Block { user_id } => json!({ "target_user_id": user_id, "action": "block" }),
            Self::Remove { user_id } => json!({ "target_user_id": user_id }),
        }
    }

    pub fn decode(&self, body: Value) -> Result<FriendsResponse, ApiError> {
        Ok(match self {
            // One odd row does not hide the list.
            Self::List => FriendsResponse::List(match body {
                Value::Array(items) => items
                    .into_iter()
                    .filter_map(|item| serde_json::from_value(item).ok())
                    .collect(),
                _ => Vec::new(),
            }),
            Self::Send { .. } => FriendsResponse::Sent(
                serde_json::from_value::<FriendRequestSent>(body)
                    .ok()
                    .and_then(|sent| sent.kind),
            ),
            Self::Accept { .. } | Self::Block { .. } | Self::Remove { .. } => FriendsResponse::Done,
        })
    }
}

/// Add-friend failures in the person's terms (FriendsView.tsx
/// `addFriendError`): 404 unknown username, 400 yourself, 409 already
/// friends, pending or blocked.
pub fn add_friend_error(error: &ApiError) -> String {
    match error {
        ApiError::Http { status: 404, .. } => {
            "No one has that username. Check the spelling and try again.".into()
        }
        ApiError::Http { status: 400, .. } => "You can't add yourself.".into(),
        ApiError::Http { status: 409, .. } => {
            "You're already friends, have a request pending, or blocked this person.".into()
        }
        _ => describe_api_error(error, "send that friend request"),
    }
}

/// The line under the Add friend field (FriendsView.tsx `AddStatus`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AddStatus {
    /// `success`, `error`, or empty for none.
    pub kind: &'static str,
    pub message: String,
    /// Counts requests that went out (the field empties).
    pub sent: u64,
}

/// One person's action as a card or row shows it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct PersonAct {
    /// The running action (`accept`, `block`, …), or empty.
    pub busy: &'static str,
    /// The last failure's copy, and which action failed (the card shows a
    /// failed block in its confirm strip, the rest under the keys).
    pub error: String,
    pub failed: &'static str,
    /// Counts finished actions (the remove dialog closes).
    pub done: u64,
}

/// What the store keeps about relationships.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FriendsState {
    /// Newest first, as `/api/friends` sorts them; adds go on top.
    pub relationships: Vec<Relationship>,
    /// The list request whose answer counts (a newer one replaces it).
    pub list: Option<RequestId>,
    /// The Friends view asked (its loading rows show while it is empty).
    pub loading: bool,
    /// The newest Send request; older answers are dropped.
    pub add_request: Option<RequestId>,
    pub add: AddStatus,
    /// By user id.
    pub acts: BTreeMap<String, PersonAct>,
}

impl FriendsState {
    pub fn get(&self, user_id: &str) -> Option<&Relationship> {
        self.relationships.iter().find(|r| r.user.id == user_id)
    }

    /// Incoming requests: what needs you (the Pending badge, the Home key).
    pub fn incoming(&self) -> i64 {
        let n = self
            .relationships
            .iter()
            .filter(|r| r.kind == Relationship::INCOMING)
            .count();
        i64::try_from(n).unwrap_or(i64::MAX)
    }
}

/// What a pending friends request was for.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum FriendsOp {
    List,
    Add,
    Act {
        user_id: String,
        action: FriendAction,
        name: String,
    },
}

/// One person in the Friends view (FriendsView.tsx `renderRow`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRow {
    pub user_id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub initials: String,
    /// `online`, `idle`, `dnd` or `offline` (blocked rows show no dot).
    pub presence: String,
    /// The second line: a friend's custom status, else empty
    /// (FriendsView.tsx `statusLine`).
    pub status_line: String,
    /// `friend`, `blocked`, `incoming` or `sent`.
    pub kind: &'static str,
}

/// The open profile card's relationship (ProfileCard.tsx `relationFor`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRelation {
    pub user_id: String,
    /// `self`, `none`, `friend`, `blocked`, `incoming` or `outgoing`.
    pub relation: &'static str,
}

/// The Friends view and every surface's relationship state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsView {
    /// The loading rows (a view-asked load in flight).
    pub loading: bool,
    /// The tabs' rows (FriendsView.tsx `filteredFriends`): Online is friends
    /// whose presence is exactly `online`; Pending splits into Incoming and
    /// Sent.
    pub online: Vec<FriendRow>,
    pub all: Vec<FriendRow>,
    pub incoming: Vec<FriendRow>,
    pub sent: Vec<FriendRow>,
    pub blocked: Vec<FriendRow>,
    /// Incoming requests (the Pending tab's and the Friends row's badge).
    pub pending: i64,
    /// The Home key's count (ChatPageClient.tsx `homeBadgeCount`): unread
    /// DMs beyond the three the rail shows, plus incoming requests.
    pub home_badge: i64,
    pub add: AddStatus,
    pub acts: BTreeMap<String, PersonAct>,
    pub card: CardRelation,
}

/// Where the profile card stands with someone.
pub fn relation_name(kind: Option<i64>) -> &'static str {
    match kind {
        Some(Relationship::FRIEND) => "friend",
        Some(Relationship::BLOCKED) => "blocked",
        Some(Relationship::INCOMING) => "incoming",
        Some(Relationship::OUTGOING) => "outgoing",
        _ => "none",
    }
}

/// A friend's presence (UserStatusDot.tsx `livePresenceStatus`): their
/// status while the gateway has them connected (a list or an update), else
/// offline.
fn presence_of(presence: &Presence, user_id: &str) -> &'static str {
    presence.live.get(user_id).copied().unwrap_or("offline")
}

fn row(rel: &Relationship, presence: &Presence, api_base: &str) -> FriendRow {
    let user = &rel.user;
    let name = user.label().to_owned();
    let kind = match rel.kind {
        Relationship::FRIEND => "friend",
        Relationship::BLOCKED => "blocked",
        Relationship::INCOMING => "incoming",
        _ => "sent",
    };
    let status_line = if rel.kind == Relationship::FRIEND {
        presence.custom_status(&user.id, user.custom_status.as_deref())
    } else {
        String::new()
    };
    FriendRow {
        user_id: user.id.clone(),
        initials: initials(&name),
        name,
        username: user.username.clone(),
        avatar: avatar_url(
            api_base,
            user.avatar_url.as_deref(),
            user.avatar_display.as_ref(),
        ),
        presence: presence_of(presence, &user.id).to_owned(),
        status_line,
        kind,
    }
}

/// What the store passes to [`friends_view`] besides the relationships.
pub struct FriendsContext<'a> {
    pub presence: &'a Presence,
    pub api_base: &'a str,
    pub me_id: &'a str,
    /// The user whose profile card is open.
    pub card_user: Option<&'a str>,
    pub dm_unread: i64,
}

/// Unread DMs the rail cannot show (ServerList.tsx `MAX_VISIBLE_DMS`).
const RAIL_DMS: i64 = 3;

pub fn friends_view(state: &FriendsState, ctx: &FriendsContext<'_>) -> FriendsView {
    let mut view = FriendsView {
        loading: state.loading,
        pending: state.incoming(),
        add: state.add.clone(),
        acts: state.acts.clone(),
        ..FriendsView::default()
    };
    view.home_badge = (ctx.dm_unread - RAIL_DMS).max(0) + view.pending;
    for rel in &state.relationships {
        let row = row(rel, ctx.presence, ctx.api_base);
        match rel.kind {
            Relationship::FRIEND => {
                if row.presence == "online" {
                    view.online.push(row.clone());
                }
                view.all.push(row);
            }
            Relationship::BLOCKED => view.blocked.push(row),
            Relationship::INCOMING => view.incoming.push(row),
            Relationship::OUTGOING => view.sent.push(row),
            _ => {}
        }
    }
    if let Some(user_id) = ctx.card_user.filter(|id| !id.is_empty()) {
        view.card = CardRelation {
            user_id: user_id.to_owned(),
            relation: if user_id == ctx.me_id {
                "self"
            } else {
                relation_name(state.get(user_id).map(|r| r.kind))
            },
        };
    }
    view
}
