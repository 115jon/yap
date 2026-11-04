//! What the reducer asks the runtime to do.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use yap_protocol::rest::{ChannelsResponse, Message, Server, User};
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

/// The REST calls the store makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiRequest {
    Me,
    Servers,
    Channels {
        server_id: String,
    },
    /// The newest page of a channel.
    Messages {
        channel_id: String,
    },
    SendMessage {
        channel_id: String,
        content: String,
    },
}

/// A decoded REST response, one variant per [`ApiRequest`].
#[derive(Debug, Clone, PartialEq)]
pub enum ApiResponse {
    Me(Box<User>),
    Servers(Vec<Server>),
    Channels(ChannelsResponse),
    Messages(Vec<Message>),
    /// The created message, when the body decodes as one.
    Sent(Option<Box<Message>>),
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

impl ApiRequest {
    /// `true` for a POST.
    pub fn is_post(&self) -> bool {
        matches!(self, Self::SendMessage { .. })
    }

    pub fn path(&self) -> String {
        match self {
            Self::Me => "/api/users/me".into(),
            Self::Servers => "/api/servers".into(),
            Self::Channels { server_id } => format!("/api/servers/{server_id}/channels"),
            Self::Messages { channel_id } => {
                format!("/api/channels/{channel_id}/messages?limit=50")
            }
            Self::SendMessage { channel_id, .. } => format!("/api/channels/{channel_id}/messages"),
        }
    }

    /// The JSON body of a POST.
    pub fn body(&self) -> Value {
        match self {
            Self::SendMessage { content, .. } => json!({ "content": content }),
            _ => Value::Null,
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
            // A page object, or (older servers) a bare array.
            Self::Messages { .. } => ApiResponse::Messages(if body.is_array() {
                lenient_list(body)
            } else {
                lenient_list(take(&mut body, "messages"))
            }),
            Self::SendMessage { .. } => {
                ApiResponse::Sent(serde_json::from_value(body).ok().map(Box::new))
            }
        })
    }
}
