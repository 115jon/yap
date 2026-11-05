//! Creating and joining servers: the rail's Create a server dialog
//! (CreateServerModal.tsx), the invite landing (InviteClient.tsx) as a
//! dialog, and the links that lead into the app (deep links and links in
//! messages, [`crate::links`]). What the UI asks for ([`InviteCommand`]),
//! the REST calls as the web makes them ([`InviteRequest`]), their answers,
//! and what the store keeps ([`InviteState`], shown as [`InviteView`]).
//!
//! The web has no Leave server (no endpoint); neither does this.

use std::collections::HashMap;

use serde::Serialize;
use serde_json::{json, Value};
use yap_protocol::rest::{InvitePreview, JoinedInvite, Server};

use super::view::query_value;
use super::{Act, Method, RequestId};
use crate::api::ApiError;
use crate::links::Link;

/// A create or join action, or a link to follow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteCommand {
    /// The Create a server dialog opened: the last outcome is forgotten.
    StartCreate,
    /// Create server: the name and an optional icon file (path, content
    /// type), uploaded first.
    CreateServer {
        name: String,
        icon: Option<(String, String)>,
    },
    /// A deep link or a clicked link. Held until the shell is ready.
    OpenLink(Link),
    /// The landing's Accept invite.
    Accept,
    /// The landing's Try again: the join again, or the preview again when
    /// it never loaded.
    Retry,
    /// The landing closed (No thanks, Escape).
    CloseLanding,
    /// Open now / Open <server>: the landing closes on the server.
    OpenServer(String),
}

/// The REST calls of creating and joining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InviteRequest {
    /// `POST /api/servers` `{name, icon_url?}` (chat-actions.ts
    /// `createServer`).
    CreateServer {
        name: String,
        icon_url: Option<String>,
    },
    /// `GET /api/invites/:code`.
    Preview { code: String },
    /// `POST /api/invites/:code/join` with `{}`.
    Join { code: String },
}

/// The answers the store reads.
#[derive(Debug, Clone, PartialEq)]
pub enum InviteResponse {
    /// The created server, when the body is one.
    Created(Option<Box<Server>>),
    Preview(Box<InvitePreview>),
    Joined(Box<JoinedInvite>),
}

fn invite_path(code: &str) -> String {
    format!("/api/invites/{}", query_value(code))
}

impl InviteRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Preview { .. } => Method::Get,
            Self::CreateServer { .. } | Self::Join { .. } => Method::Post,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::CreateServer { .. } => "/api/servers".into(),
            Self::Preview { code } => invite_path(code),
            Self::Join { code } => format!("{}/join", invite_path(code)),
        }
    }

    /// The JSON body (`Null` for none).
    pub fn body(&self) -> Value {
        match self {
            // `icon_url: undefined` is left out by JSON.stringify.
            Self::CreateServer { name, icon_url } => match icon_url {
                Some(url) => json!({ "name": name, "icon_url": url }),
                None => json!({ "name": name }),
            },
            Self::Join { .. } => json!({}),
            Self::Preview { .. } => Value::Null,
        }
    }

    pub fn decode(&self, body: Value) -> Result<InviteResponse, ApiError> {
        Ok(match self {
            Self::CreateServer { .. } => {
                InviteResponse::Created(serde_json::from_value(body).ok().map(Box::new))
            }
            Self::Preview { .. } => InviteResponse::Preview(Box::new(
                serde_json::from_value(body)
                    .map_err(|e| ApiError::Decode(format!("invite: {e}")))?,
            )),
            Self::Join { .. } => InviteResponse::Joined(Box::new(
                serde_json::from_value(body).map_err(|e| ApiError::Decode(format!("join: {e}")))?,
            )),
        })
    }
}

/// Where the landing is (InviteClient.tsx `InviteState`). The web's
/// signed-out states (`signin`, "Sign in to join") have no place here: the
/// landing only opens signed in, and a 401 ends the session as everywhere.
#[derive(Debug, Clone, PartialEq)]
pub enum LandingStep {
    Loading,
    Preview,
    Joining,
    Joined {
        server_id: String,
        name: String,
    },
    Member {
        server_id: String,
        name: String,
    },
    /// 410 (expired or used up) or 404 (missing).
    Invalid {
        expired: bool,
    },
    /// Anything else; with the preview when the join failed.
    Error,
}

/// The open invite landing.
#[derive(Debug, Clone, PartialEq)]
pub struct Landing {
    pub code: String,
    pub step: LandingStep,
    pub invite: Option<InvitePreview>,
    /// The request whose answer it waits for; any other is stale.
    pub(super) request: Option<RequestId>,
}

/// What creating and joining keep.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InviteState {
    /// Create server: busy, the error under the icon (`uploadError`) and
    /// successes (the dialog closes).
    pub create: Act,
    /// "Give your server a name."
    pub create_name_error: String,
    pub(super) create_request: Option<RequestId>,
    pub landing: Option<Landing>,
    /// Bumped when a link opens a landing (the dialog opens).
    pub landing_seq: u64,
    /// A link that came before the shell was ready.
    pub pending_link: Option<Link>,
    /// The message a channel link points at, and a bump per link.
    pub jump: Option<(String, String)>,
    pub jump_seq: u64,
    pub(super) pending: HashMap<RequestId, InviteOp>,
    /// The view as last sent.
    pub(super) shown: InviteView,
}

/// What a pending request was for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum InviteOp {
    /// The icon upload; then the create with its URL.
    Icon {
        name: String,
    },
    Create,
    Preview,
    Join,
}

/// The landing as the dialog draws it.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LandingView {
    pub code: String,
    /// `loading`, `preview`, `joining`, `joined`, `member`, `invalid`,
    /// `error`.
    pub step: String,
    /// Invalid: 410 (expired or used up) rather than 404.
    pub expired: bool,
    /// The preview loaded (Error: the join failed, not the preview).
    pub has_invite: bool,
    pub server_id: String,
    pub server_name: String,
    /// Absolute, or empty for the initial.
    pub server_icon: String,
    pub server_initial: String,
    /// "1 member", "1,204 members".
    pub members: String,
    /// The inviter's display name, else username.
    pub inviter: String,
}

/// Creating and joining, as the UI shows them.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub create: Act,
    pub create_name_error: String,
    pub landing: Option<LandingView>,
    pub landing_seq: u64,
    pub jump_channel_id: String,
    pub jump_message_id: String,
    pub jump_seq: u64,
}

/// `Intl.NumberFormat("en-US")` with the plural (InviteClient.tsx
/// `memberCount`).
pub fn member_count(count: i64) -> String {
    let digits = count.unsigned_abs().to_string();
    let mut grouped = String::new();
    for (i, digit) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(',');
        }
        grouped.push(digit);
    }
    if count < 0 {
        grouped.insert(0, '-');
    }
    if count == 1 {
        format!("{grouped} member")
    } else {
        format!("{grouped} members")
    }
}

/// What a server icon file can be on Create a server (CreateServerModal.tsx
/// `handleIconSelect`): an image (by extension here), at most 8 MB. The
/// error copy, or `None`.
pub fn create_icon_problem(content_type: Option<&str>, size: Option<u64>) -> Option<&'static str> {
    if !content_type.is_some_and(|t| t.starts_with("image/")) {
        return Some("Only image files are allowed");
    }
    match size {
        Some(size) if size > crate::admin::MAX_ICON_BYTES => Some("Image too large (max 8MB)"),
        _ => None,
    }
}
