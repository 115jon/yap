//! REST bodies the native client reads or sends. Responses are bare JSON (no
//! envelope). Loose like the payloads: unknown fields are ignored and only
//! fields the client cannot do without are required.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Socket tickets and media bootstrap

/// Which socket a ticket is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketAudience {
    Global,
    Room,
    Voice,
}

/// Body of `POST /api/voice/socket-ticket`. A server voice channel needs
/// `channelId` (the server derives the room slug itself).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocketTicketRequest {
    pub audience: TicketAudience,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_slug: Option<String>,
}

/// One ICE (STUN/TURN) server. The wire allows `urls` as a string or a list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IceServer {
    #[serde(deserialize_with = "string_or_list")]
    pub urls: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
}

fn string_or_list<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(String),
        Many(Vec<String>),
    }
    match OneOrMany::deserialize(deserializer)? {
        OneOrMany::One(url) => Ok(vec![url]),
        OneOrMany::Many(urls) => Ok(urls),
    }
}

/// Room-ticket credentials for joining media.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MediaBootstrap {
    pub participant_id: String,
    pub voice_token: String,
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
}

/// Response of `POST /api/voice/socket-ticket`. A ticket is single use with a
/// 60 s TTL; `media_bootstrap` is present only for the `room` audience.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SocketTicket {
    pub ticket: String,
    pub access_mode: Option<String>,
    pub audience: Option<String>,
    /// Epoch milliseconds.
    pub expires_at: Option<i64>,
    pub room_slug: Option<String>,
    pub media_bootstrap: Option<MediaBootstrap>,
}

// Users, servers, channels

/// A user (`GET /api/users/me` returns one for the signed-in account).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_display: Option<Value>,
    pub status: Option<String>,
    pub custom_status: Option<String>,
}

/// The signed-in user.
pub type Me = User;

impl User {
    /// `display_name`, else `username` (what the UI shows).
    pub fn label(&self) -> &str {
        match self.display_name.as_deref() {
            Some(name) if !name.is_empty() => name,
            _ => &self.username,
        }
    }
}

/// A server (guild) from `GET /api/servers`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub owner_id: Option<String>,
    pub icon_url: Option<String>,
    pub created_at: Option<String>,
}

/// Channel kind. Anything else decodes as [`ChannelType::Other`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChannelType {
    Text,
    Voice,
    Dm,
    #[serde(other)]
    Other,
}

/// A channel.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Channel {
    pub id: String,
    pub server_id: Option<String>,
    #[serde(default)]
    pub name: String,
    pub description: Option<String>,
    pub channel_type: ChannelType,
    pub category_id: Option<String>,
    #[serde(default)]
    pub position: i64,
    pub created_at: Option<String>,
    pub permissions: Option<i64>,
}

/// A channel category.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Category {
    pub id: String,
    pub server_id: Option<String>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub rank: i64,
}

/// `GET /api/servers/:id/channels`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ChannelsResponse {
    #[serde(default)]
    pub channels: Vec<Channel>,
    #[serde(default)]
    pub categories: Vec<Category>,
}

// Messages

/// A message attachment.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Attachment {
    pub id: Option<String>,
    #[serde(default)]
    pub filename: String,
    pub content_type: Option<String>,
    pub url: Option<String>,
    pub size_bytes: Option<u64>,
}

/// The message a reply quotes (`reply_to`): the server sends the first 200
/// characters of its content. Absent when the parent was deleted.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ReplyTo {
    pub id: String,
    #[serde(default)]
    pub content: String,
    pub author_id: Option<String>,
    pub author: Option<User>,
    #[serde(default)]
    pub attachment_count: i64,
}

/// Decodes an optional field that may have any shape; one that does not fit
/// is `None` rather than failing the whole body.
fn lenient<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// A chat message. `embeds` and `reactions` stay untyped until a client
/// renders them.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Message {
    pub id: String,
    pub channel_id: String,
    #[serde(default)]
    pub author_id: String,
    pub author: Option<User>,
    #[serde(default)]
    pub content: String,
    pub reply_to_id: Option<String>,
    #[serde(default, deserialize_with = "lenient")]
    pub reply_to: Option<ReplyTo>,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub created_at: String,
    /// Set once the message has been edited.
    pub updated_at: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub reactions: Vec<Value>,
    #[serde(default)]
    pub embeds: Vec<Value>,
    pub nonce: Option<String>,
    pub content_revision: Option<i64>,
    /// `"call"` for a DM call record.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// `MESSAGE_UPDATE` dispatch data when it is partial (an edit, or embeds that
/// arrived after a link unfurl): only `id` is guaranteed, and a field that is
/// absent did not change. A full update carries `author` and decodes as
/// [`Message`] instead.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MessagePatch {
    pub id: String,
    pub channel_id: Option<String>,
    pub content: Option<String>,
    pub updated_at: Option<String>,
    pub embeds: Option<Vec<Value>>,
    pub content_revision: Option<i64>,
}

/// `GET /api/channels/:id/messages`. `hasMoreBefore` is absent in `after` mode.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagePage {
    #[serde(default)]
    pub messages: Vec<Message>,
    pub has_more_before: Option<bool>,
    pub has_more_after: Option<bool>,
}

/// One channel's read mark (`GET /api/read-states`, `READ_STATE_UPDATE`).
/// Timestamps are ISO 8601 and compare as strings, as on the web.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReadState {
    pub channel_id: String,
    pub last_read_at: String,
}

/// The newest message time of one channel.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct LastMessage {
    pub channel_id: String,
    pub last_message_at: String,
}

/// `GET /api/read-states`: every read mark and every channel's newest
/// message time.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReadStates {
    #[serde(default)]
    pub read_states: Vec<ReadState>,
    #[serde(default)]
    pub last_messages: Vec<LastMessage>,
}

/// One notification (`GET /api/notifications`, `NOTIFICATION_CREATE`).
/// Unread `mention` and `reply` notifications are the mention counts.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Notification {
    pub id: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    pub channel_id: Option<String>,
    pub server_id: Option<String>,
    pub message_id: Option<String>,
    #[serde(default)]
    pub is_read: bool,
}

impl Notification {
    /// Counts toward a channel's and a server's mention badge.
    pub fn is_mention(&self) -> bool {
        !self.is_read && matches!(self.kind.as_str(), "mention" | "reply")
    }
}

/// One server member (`GET /api/servers/:id/members` is a bare array).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Member {
    pub user: User,
}

// Device sign-in (Kova)

/// Body of `POST .../device/start`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceStartRequest {
    pub platform: String,
    pub label: String,
    pub hostname: Option<String>,
}

/// Response of `POST .../device/start`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri_complete: String,
    pub interval: u64,
    pub expires_in: u64,
}

/// Body of `POST .../device/poll` (and `.../device/cancel`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePollRequest {
    pub device_code: String,
}

/// Where a device sign-in stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DevicePollStatus {
    Pending,
    Approved,
    Denied,
    #[serde(other)]
    Other,
}

/// Response of `POST .../device/poll`. Errors (`expired_token`, `slow_down`)
/// arrive as `error` with a non-2xx status and no `status`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePollResponse {
    pub status: Option<DevicePollStatus>,
    pub session_token: Option<String>,
    pub error: Option<String>,
}
