//! What the reducer asks the runtime to do.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use yap_protocol::rest::{
    ChannelsResponse, DmChannel, Member, Message, MessagePatch, Notification, ProfileResponse,
    ReadStates, Server, User,
};
use yap_protocol::Frame;

pub use yap_cache::Write as CacheWrite;

use super::{CallId, Input, RequestId, SessionId, SignInId, ViewUpdate, VoiceCommand, VoiceTarget};
use crate::api::{ApiError, Token};

/// One piece of I/O, or one update for the UI. The runner starts each in
/// order; I/O runs concurrently and reports back as an [`super::Input`].
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// A REST call; answered by `Input::Http` with the same `id`.
    Http {
        id: RequestId,
        token: Token,
        request: ApiRequest,
    },
    /// Open the gateway for a session (replacing any open one).
    GatewayConnect {
        session: SessionId,
        token: Token,
        user_id: String,
    },
    /// A frame for the open gateway. Only emitted while it is ready.
    GatewaySend(Frame),
    GatewayDisconnect,
    /// Read the stored session; answered by `Input::TokenLoaded`.
    LoadToken,
    /// Open the cache and read the last session's snapshot; answered by
    /// `Input::SnapshotLoaded`.
    LoadSnapshot,
    /// Replace part of a user's snapshot (write-behind, coalesced).
    CacheWrite {
        user_id: String,
        write: CacheWrite,
    },
    /// Wipe a user's snapshot (sign-out, expired session, another account).
    CacheClear {
        user_id: String,
    },
    StoreToken(Token),
    ClearToken,
    /// Revoke a session on Kova (best effort, no answer).
    Revoke(Token),
    /// Start a device sign-in (cancelling any running one).
    SignInStart(SignInId),
    SignInCancel,
    VoiceStart {
        call: CallId,
        token: Token,
        target: VoiceTarget,
    },
    /// A command for a running call. `Leave` also lets go of it.
    Voice {
        call: CallId,
        command: VoiceCommand,
    },
    /// Let go of a call that ended on its own.
    VoiceForget(CallId),
    /// An update for the UI. The runner hands each batch to the sink at once.
    View(ViewUpdate),
    /// Feed `input` back after `delay` (timeouts; the store has no clock).
    Later {
        delay: Duration,
        input: Box<Input>,
    },
    /// Log `[startup] <mark> after N ms` (once per mark; the reducer decides).
    StartupMark(&'static str),
}

/// An HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

/// The REST calls the store makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiRequest {
    Me,
    Servers,
    Channels {
        server_id: String,
    },
    /// A page of 50: the newest, or the one before `before` (a
    /// `created_at|id` cursor, as `useChatArea.ts` builds it).
    Messages {
        channel_id: String,
        before: Option<String>,
    },
    /// `nonce` comes back on the created message and its `MESSAGE_CREATE`.
    SendMessage {
        channel_id: String,
        content: String,
        reply_to_id: Option<String>,
        nonce: String,
    },
    EditMessage {
        channel_id: String,
        message_id: String,
        content: String,
    },
    DeleteMessage {
        channel_id: String,
        message_id: String,
    },
    Typing {
        channel_id: String,
    },
    ReadStates,
    /// The server sets `last_read_at` to now and broadcasts `READ_STATE_UPDATE`.
    MarkRead {
        channel_id: String,
    },
    /// Unread notifications (the mention counts).
    Notifications,
    MarkNotificationsRead {
        ids: Vec<String>,
    },
    Members {
        server_id: String,
    },
    /// Every DM channel, newest first.
    Dms,
    /// Opens (or creates) the DM with a user.
    OpenDm {
        user_id: String,
    },
    /// The chosen status and custom status (`None` clears it), stored and
    /// broadcast as `PRESENCE_UPDATE`.
    SetPresence {
        status: String,
        custom_status: Option<String>,
    },
    /// A user's full profile, for the popout.
    Profile {
        user_id: String,
    },
}

/// A page of messages, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Page {
    pub messages: Vec<Message>,
    pub has_more_before: bool,
}

/// A decoded REST response, one variant per kind of [`ApiRequest`].
#[derive(Debug, Clone, PartialEq)]
pub enum ApiResponse {
    Me(Box<User>),
    Servers(Vec<Server>),
    Channels(ChannelsResponse),
    Messages(Page),
    /// The created message, when the body decodes as one.
    Sent(Option<Box<Message>>),
    /// The edit as the server stored it.
    Edited(Option<MessagePatch>),
    ReadStates(ReadStates),
    Notifications(Vec<Notification>),
    Members(Vec<Member>),
    Dms(Vec<DmChannel>),
    Dm(Box<DmChannel>),
    Profile(Box<User>),
    /// A call whose body the store does not read.
    Done,
}

/// Decodes each element that fits and skips the rest, so one odd row does not
/// hide a whole list. Anything but an array is an empty list.
fn lenient_list<T: DeserializeOwned>(value: Value) -> Vec<T> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn take(value: &mut Value, key: &str) -> Value {
    value
        .as_object_mut()
        .and_then(|object| object.remove(key))
        .unwrap_or(Value::Null)
}

use super::view::query_value;

impl ApiRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::SendMessage { .. }
            | Self::Typing { .. }
            | Self::OpenDm { .. }
            | Self::SetPresence { .. } => Method::Post,
            Self::MarkRead { .. } => Method::Put,
            Self::EditMessage { .. } | Self::MarkNotificationsRead { .. } => Method::Patch,
            Self::DeleteMessage { .. } => Method::Delete,
            Self::Me
            | Self::Servers
            | Self::Channels { .. }
            | Self::Messages { .. }
            | Self::ReadStates
            | Self::Notifications
            | Self::Members { .. }
            | Self::Dms
            | Self::Profile { .. } => Method::Get,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Me => "/api/users/me".into(),
            Self::Servers => "/api/servers".into(),
            Self::Channels { server_id } => format!("/api/servers/{server_id}/channels"),
            Self::Messages { channel_id, before } => match before {
                Some(cursor) => format!(
                    "/api/channels/{channel_id}/messages?limit=50&before={}",
                    query_value(cursor)
                ),
                None => format!("/api/channels/{channel_id}/messages?limit=50"),
            },
            Self::SendMessage { channel_id, .. }
            | Self::EditMessage { channel_id, .. }
            | Self::DeleteMessage { channel_id, .. } => {
                format!("/api/channels/{channel_id}/messages")
            }
            Self::Typing { channel_id } => format!("/api/channels/{channel_id}/typing"),
            Self::ReadStates => "/api/read-states".into(),
            Self::MarkRead { channel_id } => format!("/api/channels/{channel_id}/read-state"),
            Self::Notifications => "/api/notifications?limit=100&unread=true".into(),
            Self::MarkNotificationsRead { .. } => "/api/notifications".into(),
            Self::Members { server_id } => format!("/api/servers/{server_id}/members"),
            Self::Dms | Self::OpenDm { .. } => "/api/dms".into(),
            Self::SetPresence { .. } => "/api/presence".into(),
            Self::Profile { user_id } => format!("/api/users/{user_id}/profile"),
        }
    }

    /// The JSON body (`Null` for a GET).
    pub fn body(&self) -> Value {
        match self {
            Self::SendMessage {
                content,
                reply_to_id,
                nonce,
                ..
            } => {
                let mut body = json!({ "content": content, "nonce": nonce });
                if let Some(reply) = reply_to_id {
                    body["reply_to_id"] = json!(reply);
                }
                body
            }
            Self::EditMessage {
                message_id,
                content,
                ..
            } => json!({ "message_id": message_id, "content": content }),
            Self::DeleteMessage { message_id, .. } => json!({ "message_id": message_id }),
            Self::MarkNotificationsRead { ids } => json!({ "ids": ids }),
            Self::OpenDm { user_id } => json!({ "target_user_id": user_id }),
            Self::SetPresence {
                status,
                custom_status,
            } => json!({ "status": status, "custom_status": custom_status }),
            Self::Typing { .. } | Self::MarkRead { .. } => json!({}),
            Self::Me
            | Self::Servers
            | Self::Channels { .. }
            | Self::Messages { .. }
            | Self::ReadStates
            | Self::Notifications
            | Self::Members { .. }
            | Self::Dms
            | Self::Profile { .. } => Value::Null,
        }
    }

    /// Decodes a success body into the response for this request.
    pub fn decode(&self, mut body: Value) -> Result<ApiResponse, ApiError> {
        Ok(match self {
            Self::Me => ApiResponse::Me(Box::new(
                serde_json::from_value(body).map_err(|e| ApiError::Decode(format!("me: {e}")))?,
            )),
            Self::Servers => ApiResponse::Servers(lenient_list(body)),
            Self::Channels { .. } => ApiResponse::Channels(ChannelsResponse {
                channels: lenient_list(take(&mut body, "channels")),
                categories: lenient_list(take(&mut body, "categories")),
            }),
            // A page object, or (older servers) a bare array, where a full
            // page means there may be more (chat-actions.ts).
            Self::Messages { .. } if body.is_array() => {
                let messages: Vec<Message> = lenient_list(body);
                ApiResponse::Messages(Page {
                    has_more_before: messages.len() >= 50,
                    messages,
                })
            }
            Self::Messages { .. } => ApiResponse::Messages(Page {
                has_more_before: body
                    .get("hasMoreBefore")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                messages: lenient_list(take(&mut body, "messages")),
            }),
            Self::SendMessage { .. } => {
                ApiResponse::Sent(serde_json::from_value(body).ok().map(Box::new))
            }
            Self::EditMessage { .. } => ApiResponse::Edited(serde_json::from_value(body).ok()),
            Self::ReadStates => ApiResponse::ReadStates(ReadStates {
                read_states: lenient_list(take(&mut body, "read_states")),
                last_messages: lenient_list(take(&mut body, "last_messages")),
            }),
            Self::Notifications => {
                ApiResponse::Notifications(lenient_list(take(&mut body, "notifications")))
            }
            Self::Members { .. } => ApiResponse::Members(lenient_list(body)),
            Self::Dms => ApiResponse::Dms(lenient_list(body)),
            Self::OpenDm { .. } => ApiResponse::Dm(Box::new(
                serde_json::from_value(body).map_err(|e| ApiError::Decode(format!("dm: {e}")))?,
            )),
            Self::Profile { .. } => {
                let profile: ProfileResponse = serde_json::from_value(body)
                    .map_err(|e| ApiError::Decode(format!("profile: {e}")))?;
                ApiResponse::Profile(Box::new(profile.user))
            }
            Self::DeleteMessage { .. }
            | Self::SetPresence { .. }
            | Self::Typing { .. }
            | Self::MarkRead { .. }
            | Self::MarkNotificationsRead { .. } => ApiResponse::Done,
        })
    }
}
