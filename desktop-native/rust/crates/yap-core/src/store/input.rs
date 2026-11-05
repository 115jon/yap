//! What changes the state: user intents, effect results, gateway and voice events.

use serde_json::Value;
use yap_protocol::payload::{
    MessageDeleteEvent, NotificationsRead, TypingStartEvent, VoiceChannelStateUpdate,
    VoiceChannelStates,
};
use yap_protocol::rest::{Message, MessagePatch, Notification, ReadState};

use yap_cache::Snapshot;

use super::{ApiResponse, CallId, RequestId, SessionId, SignInId, VoiceUpdate};
use crate::api::{ApiError, Token};

/// What the UI asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Start,
    StartSignIn,
    CancelSignIn,
    UseToken(String),
    SignOut,
    SelectServer(String),
    SelectChannel(String),
    /// Post to the open channel. The UI stamps the nonce and the local send
    /// time (the store has no clock or randomness), so the pending row can
    /// show at once and the echo can find it.
    Send {
        content: String,
        reply_to_id: Option<String>,
        nonce: String,
        sent_at: String,
    },
    /// Post a failed send again, with the same nonce.
    RetrySend(String),
    /// Drop a failed send's row.
    DiscardSend(String),
    EditMessage {
        message_id: String,
        content: String,
    },
    DeleteMessage(String),
    /// The composer has text (the UI throttles this to every 3 s).
    Typing,
    /// The message list is scrolled near its top: load the page before.
    LoadOlder,
    /// Whether the message list is at its newest end.
    SetAtBottom(bool),
    /// The top-most message on screen (reported with the end): its
    /// mentions and everything below it have been seen.
    SeenFrom(String),
    /// Whether the window has focus.
    SetFocused(bool),
    /// Mark the open channel read and drop its "New" divider.
    MarkRead,
    /// The `@` query in the composer, or `None` when no suggestion list is open.
    MentionQuery(Option<String>),
    JoinVoice(String),
    LeaveVoice,
    SetVoiceMuted(bool),
    /// Deafen (or not). Like mute, it is kept between calls.
    SetVoiceDeafened(bool),
}

/// Everything [`super::reduce`] consumes.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    User(Command),
    /// The stored session, if there is one.
    TokenLoaded(Option<Token>),
    /// The last session's snapshot, if the cache has one.
    SnapshotLoaded(Option<Box<Snapshot>>),
    Http {
        id: RequestId,
        result: Result<ApiResponse, ApiError>,
    },
    SignIn {
        attempt: SignInId,
        event: SignInEvent,
    },
    Gateway {
        session: SessionId,
        event: GatewayEvent,
    },
    Voice {
        call: CallId,
        update: VoiceUpdate,
    },
    /// One typer's timeout; only the newest event of theirs clears them.
    TypingExpired {
        user_id: String,
        generation: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignInEvent {
    DeviceCode { user_code: String, url: String },
    Approved(Token),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GatewayEvent {
    /// Ready (op 2): a fresh session. After a drop, events in the gap are
    /// lost, so what is on screen is refetched.
    Ready,
    /// Resumed (op 9): the server replayed what was missed and kept the
    /// subscriptions, so nothing is refetched.
    Resumed,
    /// The socket dropped; the gateway reconnects on its own.
    Closed,
    /// A ticket fetch got a 401.
    SessionExpired,
    Dispatch(Dispatch),
}

/// The gateway dispatches the store handles, decoded.
#[derive(Debug, Clone, PartialEq)]
pub enum Dispatch {
    MessageCreate(Box<Message>),
    /// A full update (it carries `author`).
    MessageUpdate(Box<Message>),
    /// A partial update: only the fields present changed.
    MessagePatch(MessagePatch),
    MessageDelete(MessageDeleteEvent),
    TypingStart(TypingStartEvent),
    ReadStateUpdate(ReadState),
    NotificationCreate(Notification),
    NotificationsRead(NotificationsRead),
    /// Every notification was cleared.
    NotificationsClear,
    /// The server list changed (`USER_SERVERS_UPDATE`, `GUILD_UPDATE`, `GUILD_DELETE`).
    ServersChanged,
    /// Who is in each voice channel (after a subscribe): replaces them all.
    VoiceChannelStates(VoiceChannelStates),
    /// One voice channel's roster changed.
    VoiceChannelState(VoiceChannelStateUpdate),
}

impl Dispatch {
    /// Decodes one dispatch. `None` for events the store ignores or bodies
    /// that do not decode.
    pub fn decode(event: &str, data: Value) -> Option<Self> {
        Some(match event {
            "MESSAGE_CREATE" => Self::MessageCreate(Box::new(serde_json::from_value(data).ok()?)),
            // Like the web reducer: an update with `author` is a whole message.
            "MESSAGE_UPDATE" if data.get("author").is_some() => {
                match serde_json::from_value::<Message>(data.clone()) {
                    Ok(message) => Self::MessageUpdate(Box::new(message)),
                    Err(_) => Self::MessagePatch(serde_json::from_value(data).ok()?),
                }
            }
            "MESSAGE_UPDATE" => Self::MessagePatch(serde_json::from_value(data).ok()?),
            "MESSAGE_DELETE" => Self::MessageDelete(serde_json::from_value(data).ok()?),
            "TYPING_START" => Self::TypingStart(serde_json::from_value(data).ok()?),
            "READ_STATE_UPDATE" => Self::ReadStateUpdate(serde_json::from_value(data).ok()?),
            "NOTIFICATION_CREATE" => Self::NotificationCreate(serde_json::from_value(data).ok()?),
            "NOTIFICATIONS_READ" => Self::NotificationsRead(serde_json::from_value(data).ok()?),
            "NOTIFICATIONS_CLEAR" => Self::NotificationsClear,
            "USER_SERVERS_UPDATE" | "GUILD_DELETE" | "GUILD_UPDATE" => Self::ServersChanged,
            "VOICE_CHANNEL_STATES" => Self::VoiceChannelStates(serde_json::from_value(data).ok()?),
            "VOICE_CHANNEL_STATE_UPDATE" => {
                Self::VoiceChannelState(serde_json::from_value(data).ok()?)
            }
            _ => return None,
        })
    }
}
