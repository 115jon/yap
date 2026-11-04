//! What changes the state: user intents, effect results, gateway and voice events.

use serde_json::Value;
use yap_protocol::payload::{MessageDeleteEvent, TypingStartEvent};
use yap_protocol::rest::{Message, MessagePatch};

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
    Send(String),
    JoinVoice(String),
    LeaveVoice,
    SetVoiceMuted(bool),
}

/// Everything [`super::reduce`] consumes.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    User(Command),
    /// The stored session, if there is one.
    TokenLoaded(Option<Token>),
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum SignInEvent {
    DeviceCode { user_code: String, url: String },
    Approved(Token),
    Failed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum GatewayEvent {
    /// Ready (op 2) or Resumed (op 9).
    Ready,
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
    /// The server list changed (`USER_SERVERS_UPDATE`, `GUILD_UPDATE`, `GUILD_DELETE`).
    ServersChanged,
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
            "USER_SERVERS_UPDATE" | "GUILD_DELETE" | "GUILD_UPDATE" => Self::ServersChanged,
            _ => return None,
        })
    }
}
