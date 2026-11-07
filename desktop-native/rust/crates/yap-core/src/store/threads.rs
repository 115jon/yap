//! Threads, pins and jumps: the thread panel (ThreadSidebar.tsx), the pins
//! panel (PinnedMessagesSidebar.tsx, PinModal.tsx), jump to message with
//! `around=` windows and the detached list (useChatArea.ts
//! `handleJumpToMessage`, `handleLoadAfter`, `handleJumpToPresent`), and
//! the message link (MessageItem.tsx `buildMessageLink`). The request
//! shapes, the sub-store's state and its view; the reducer is
//! `reduce/threads.rs`.
//!
//! A thread is the reply chain of one message (`reply_to_id`), not a
//! channel: `GET /api/channels/{id}/thread?message_id=` answers the root
//! and every reply to it (thread.ts).

use std::collections::HashMap;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use yap_protocol::rest::{Message, ThreadSummary};

use super::state::MessageLog;
use super::view::{Load, MessageRow};
use super::{Method, RequestId};
use crate::api::ApiError;

/// The thread panel's intents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadCommand {
    /// Shows the thread of a message in the open channel (ChatArea.tsx
    /// `handleThread`): the root and its replies load afresh.
    Open(String),
    /// The panel closed.
    Close,
    /// Replies in the thread (ThreadSidebar.tsx `handleSendReply`): a
    /// pending row at once, then a post with `reply_to_id` set to the
    /// root. The UI stamps the nonce and the local time, as for `Send`.
    Send {
        content: String,
        nonce: String,
        sent_at: String,
    },
    /// The member list's Threads tab was shown (MemberList.tsx fetches
    /// `/threads` every time the tab opens): the open channel's threads
    /// load afresh. `now_ms` dates their last replies ("5m ago").
    List { now_ms: i64 },
}

/// The pins panel's intents.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PinsCommand {
    /// The panel opened, or the channel under it changed: load the open
    /// channel's pins (useChatArea.ts `handleTogglePins` → `loadPins`),
    /// once per channel; live pins and unpins keep the list current after.
    Open,
    /// Pin or unpin a message, at once and then on the server
    /// (chat-actions.ts `pinMessage` / `unpinMessage`; a refusal puts it
    /// back, silently, as the web does).
    SetPinned { message_id: String, pinned: bool },
}

/// The REST calls of threads, pins and jumps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadRequest {
    /// `GET /api/channels/{id}/thread?message_id=` (thread.ts).
    Thread {
        channel_id: String,
        message_id: String,
    },
    /// `GET /api/channels/{id}/pins`, newest first (pins.ts).
    Pins { channel_id: String },
    /// `GET /api/channels/{id}/threads`: the channel's thread starters by
    /// latest reply (threads.ts; the web sends no `limit`, so 30).
    ChannelThreads { channel_id: String },
    /// `PUT /api/channels/{id}/pins` with `{message_id, pinned}` (pins.ts).
    SetPin {
        channel_id: String,
        message_id: String,
        pinned: bool,
    },
    /// The window around a message: 25 before it (itself included) and 24
    /// after (chat-actions.ts `loadMessagesAround`; the server ignores a
    /// `limit` in this mode, message.service.ts `fetchMessageRows`).
    Around {
        channel_id: String,
        message_id: String,
    },
    /// The 50 after a `created_at|id` cursor (chat-actions.ts
    /// `loadMessagesAfter`).
    After { channel_id: String, after: String },
}

/// A decoded answer to a [`ThreadRequest`].
#[derive(Debug, Clone, PartialEq)]
pub enum ThreadResponse {
    Thread {
        root: Box<Message>,
        replies: Vec<Message>,
    },
    Pins(Vec<Message>),
    ChannelThreads(Vec<ThreadSummary>),
    /// An `around` or `after` page, oldest first. `after` pages carry no
    /// `hasMoreBefore` (messages.ts), which reads as `false`.
    Window {
        messages: Vec<Message>,
        has_more_before: bool,
        has_more_after: bool,
    },
    Done,
}

/// Decodes each element that fits and skips the rest.
fn lenient<T: DeserializeOwned>(value: Value) -> Vec<T> {
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

fn flag(value: &Value, key: &str) -> bool {
    value.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )`
/// is percent-encoded as UTF-8.
pub fn uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let keep = byte.is_ascii_alphanumeric()
            || matches!(
                byte,
                b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')'
            );
        if keep {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

impl ThreadRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::SetPin { .. } => Method::Put,
            Self::Thread { .. }
            | Self::Pins { .. }
            | Self::ChannelThreads { .. }
            | Self::Around { .. }
            | Self::After { .. } => Method::Get,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Thread {
                channel_id,
                message_id,
            } => format!(
                "/api/channels/{channel_id}/thread?message_id={}",
                uri_component(message_id)
            ),
            Self::Pins { channel_id } | Self::SetPin { channel_id, .. } => {
                format!("/api/channels/{channel_id}/pins")
            }
            Self::ChannelThreads { channel_id } => format!("/api/channels/{channel_id}/threads"),
            Self::Around {
                channel_id,
                message_id,
            } => format!(
                "/api/channels/{channel_id}/messages?around={}",
                uri_component(message_id)
            ),
            Self::After { channel_id, after } => format!(
                "/api/channels/{channel_id}/messages?after={}&limit=50",
                uri_component(after)
            ),
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::SetPin {
                message_id, pinned, ..
            } => json!({ "message_id": message_id, "pinned": pinned }),
            Self::Thread { .. }
            | Self::Pins { .. }
            | Self::ChannelThreads { .. }
            | Self::Around { .. }
            | Self::After { .. } => Value::Null,
        }
    }

    pub fn decode(&self, mut body: Value) -> Result<ThreadResponse, ApiError> {
        Ok(match self {
            Self::Thread { .. } => {
                let root = serde_json::from_value(take(&mut body, "root"))
                    .map_err(|e| ApiError::Decode(format!("thread: {e}")))?;
                ThreadResponse::Thread {
                    root: Box::new(root),
                    replies: lenient(take(&mut body, "replies")),
                }
            }
            Self::Pins { .. } => ThreadResponse::Pins(lenient(body)),
            Self::ChannelThreads { .. } => {
                ThreadResponse::ChannelThreads(lenient(take(&mut body, "threads")))
            }
            Self::SetPin { .. } => ThreadResponse::Done,
            // A bare array (older servers) is a window with nothing more.
            Self::Around { .. } | Self::After { .. } if body.is_array() => ThreadResponse::Window {
                messages: lenient(body),
                has_more_before: false,
                has_more_after: false,
            },
            Self::Around { .. } | Self::After { .. } => ThreadResponse::Window {
                has_more_before: flag(&body, "hasMoreBefore"),
                has_more_after: flag(&body, "hasMoreAfter"),
                messages: lenient(take(&mut body, "messages")),
            },
        })
    }
}

/// What MessageItem.tsx's "Copy message link" puts on the clipboard
/// (`buildMessageLink`): `{origin}/chat/{server or @me}/{channel}?message={id}`,
/// each segment `encodeURIComponent`-ed.
pub fn message_link(
    web_origin: &str,
    server_id: Option<&str>,
    channel_id: &str,
    message_id: &str,
) -> String {
    let server = server_id.filter(|id| !id.is_empty()).unwrap_or("@me");
    format!(
        "{}/chat/{}/{}?message={}",
        web_origin.trim_end_matches('/'),
        uri_component(server),
        uri_component(channel_id),
        uri_component(message_id)
    )
}

/// A request of this sub-store in flight, and what its answer is for.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum PanelOp {
    Thread {
        channel_id: String,
        root_id: String,
    },
    ThreadSend {
        nonce: String,
    },
    Pins {
        channel_id: String,
    },
    ChannelThreads {
        channel_id: String,
    },
    /// The message as it was, to put back if the server refuses.
    SetPin {
        message: Box<Message>,
        pinned: bool,
    },
    Around {
        channel_id: String,
        message_id: String,
        /// Reopening where the channel was left: no highlight.
        restore: bool,
    },
    Newer {
        channel_id: String,
    },
}

/// The open thread: its root first, then the replies by time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadState {
    /// Empty while no thread is open.
    pub root_id: String,
    pub channel_id: String,
    pub log: MessageLog,
    /// `Ready(())` once the root and replies are in `log`.
    pub load: Load<()>,
}

/// A channel's pinned messages, newest first (`pinsLoadedFor`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PinsState {
    pub channel_id: String,
    pub load: Load<Vec<Message>>,
}

/// The member list's Threads tab: the open channel's thread starters.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadListState {
    pub channel_id: String,
    /// When the tab asked (for "5m ago").
    pub now_ms: i64,
    pub load: Load<Vec<ThreadSummary>>,
}

/// A jump waiting for its channel to open: (channel, message).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JumpState {
    pub wanted: Option<(String, String)>,
    /// A page after the window is loading.
    pub newer_loading: bool,
}

/// Everything this sub-store keeps, and what it last emitted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PanelsState {
    pub thread: ThreadState,
    pub pins: PinsState,
    pub threads: ThreadListState,
    pub jump: JumpState,
    pub(super) pending: HashMap<RequestId, PanelOp>,
    /// The thread's or the pins' messages changed since their rows were
    /// last emitted.
    pub(super) thread_dirty: bool,
    pub(super) pins_dirty: bool,
    pub(super) thread_rows: Vec<MessageRow>,
    pub(super) pin_rows: Vec<MessageRow>,
    pub(super) view: PanelsView,
    /// (loading, more) of newer pages, as last emitted.
    pub(super) newer: (bool, bool),
}

/// The panels' scalars, as the UI binds them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PanelsView {
    /// Pin and unpin are offered: in a DM always (pins.ts checks only
    /// server channels), in a server with Manage messages, Manage
    /// channels, Manage server or Administrator from the user's roles
    /// (useChatArea.ts `canPin`).
    pub can_pin: bool,
    /// The open thread's root, empty when none is open.
    pub thread_root_id: String,
    pub thread_loading: bool,
    /// The error copy ("Unable to load this thread. …"), empty otherwise.
    pub thread_error: String,
    /// Replies shown (ThreadSidebar.tsx counts `replies.length`).
    pub thread_replies: i64,
    /// A failed load shows the empty state, as on the web (`loadPins`).
    pub pins_loading: bool,
    pub pins_count: i64,
    /// The Threads tab (MemberList.tsx `ThreadsTabContent`): loading, the
    /// error copy (empty otherwise) and its rows.
    pub threads_loading: bool,
    pub threads_error: String,
    pub threads: Vec<ThreadListRow>,
}

/// One thread on the Threads tab: the starter's author, up to 200
/// characters of it, its reply count and when the last reply came.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadListRow {
    pub id: String,
    pub author: String,
    pub initials: String,
    /// Absolute, or empty.
    pub avatar: String,
    pub content: String,
    pub replies: i64,
    /// "Just now", "5m ago", … or "Nov 5".
    pub last_reply: String,
}
