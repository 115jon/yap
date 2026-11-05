//! What changes the state: user intents, effect results, gateway and voice events.

use serde_json::Value;
use yap_protocol::payload::{
    GuildMemberAdd, GuildMemberRemove, GuildMemberUpdate, MessageDeleteEvent, NotificationsRead,
    PresenceList, PresenceUpdateEvent, ReactionEvent, TypingStartEvent, UserProfileUpdate,
    VoiceChannelStateUpdate, VoiceChannelStates,
};
use yap_protocol::rest::{DmChannel, Message, MessagePatch, Notification, ReadState};

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
    /// Home: the DM list instead of a server, no conversation open.
    SelectHome,
    /// A text channel, a voice channel's view, or a DM.
    SelectChannel(String),
    /// Opens the DM with a user, creating it when there is none.
    OpenDm(String),
    /// Opens a channel or DM by id from anywhere, switching servers first
    /// when it is in another one (a toast click). `server_id` is the toast's
    /// hint, used when no loaded tree has the channel.
    OpenChannel {
        channel_id: String,
        server_id: Option<String>,
    },
    /// The chosen status (`online`, `idle`, `dnd`, `offline` = invisible)
    /// and custom status (`None` clears it).
    SetStatus {
        status: String,
        custom_status: Option<String>,
    },
    /// Opens a user's popout; `None` closes it.
    ShowProfile(Option<String>),
    /// Closes the popout if it still shows this user (a closing card must
    /// not empty the one that opened after it).
    HideProfile(String),
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
    /// Adds the user's reaction, or removes it when it is there.
    ToggleReaction {
        message_id: String,
        emoji: String,
    },
    /// Files for the open channel's composer tray; each starts uploading.
    AddUploads(Vec<UploadFile>),
    /// Cancels an upload, or removes its tile.
    RemoveUpload(String),
    RetryUpload(String),
    /// Marks a finished upload as sensitive (or not).
    SetUploadSensitive {
        local_id: String,
        nsfw: bool,
    },
    /// A new GIF picker query (empty: trending) for a media type.
    GifSearch {
        query: String,
        media_type: String,
    },
    /// The next page of the GIF picker.
    GifMore,
    /// Sends a GIF picker tile (its row id) to the open channel at once.
    SendGif {
        gif_id: String,
        reply_to_id: Option<String>,
        nonce: String,
        sent_at: String,
    },
    /// Saves a file to `dest` (chosen by the UI); `open` it afterwards.
    Download {
        url: String,
        dest: String,
        open: bool,
    },
    /// The `@` query in the composer, or `None` when no suggestion list is open.
    MentionQuery(Option<String>),
    JoinVoice(String),
    LeaveVoice,
    SetVoiceMuted(bool),
    /// Deafen (or not). Like mute, it is kept between calls.
    SetVoiceDeafened(bool),
}

/// A file the user picked, pasted or dropped, as the UI read it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadFile {
    pub local_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub content_type: String,
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
    /// An upload's bytes sent so far.
    UploadProgress {
        id: RequestId,
        sent: u64,
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
    ReactionAdd(ReactionEvent),
    ReactionRemove(ReactionEvent),
    TypingStart(TypingStartEvent),
    ReadStateUpdate(ReadState),
    NotificationCreate(Box<Notification>),
    NotificationsRead(NotificationsRead),
    /// Every notification was cleared.
    NotificationsClear,
    /// The server list changed (`USER_SERVERS_UPDATE`, `GUILD_UPDATE`, `GUILD_DELETE`).
    ServersChanged,
    /// Who is in each voice channel (after a subscribe): replaces them all.
    VoiceChannelStates(VoiceChannelStates),
    /// One voice channel's roster changed.
    VoiceChannelState(VoiceChannelStateUpdate),
    PresenceUpdate(PresenceUpdateEvent),
    /// Everyone connected (after a channel subscribe).
    PresenceList(PresenceList),
    MemberAdd(Box<GuildMemberAdd>),
    MemberRemove(GuildMemberRemove),
    MemberUpdate(GuildMemberUpdate),
    DmCreate(Box<DmChannel>),
    ProfileUpdate(Box<UserProfileUpdate>),
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
            "REACTION_ADD" => Self::ReactionAdd(serde_json::from_value(data).ok()?),
            "REACTION_REMOVE" => Self::ReactionRemove(serde_json::from_value(data).ok()?),
            "TYPING_START" => Self::TypingStart(serde_json::from_value(data).ok()?),
            "READ_STATE_UPDATE" => Self::ReadStateUpdate(serde_json::from_value(data).ok()?),
            "NOTIFICATION_CREATE" => {
                Self::NotificationCreate(Box::new(serde_json::from_value(data).ok()?))
            }
            "NOTIFICATIONS_READ" => Self::NotificationsRead(serde_json::from_value(data).ok()?),
            "NOTIFICATIONS_CLEAR" => Self::NotificationsClear,
            "USER_SERVERS_UPDATE" | "GUILD_DELETE" | "GUILD_UPDATE" => Self::ServersChanged,
            "VOICE_CHANNEL_STATES" => Self::VoiceChannelStates(serde_json::from_value(data).ok()?),
            "VOICE_CHANNEL_STATE_UPDATE" => {
                Self::VoiceChannelState(serde_json::from_value(data).ok()?)
            }
            "PRESENCE_UPDATE" => Self::PresenceUpdate(serde_json::from_value(data).ok()?),
            "PRESENCE_LIST" => Self::PresenceList(serde_json::from_value(data).ok()?),
            "GUILD_MEMBER_ADD" => Self::MemberAdd(Box::new(serde_json::from_value(data).ok()?)),
            "GUILD_MEMBER_REMOVE" => Self::MemberRemove(serde_json::from_value(data).ok()?),
            "GUILD_MEMBER_UPDATE" => Self::MemberUpdate(serde_json::from_value(data).ok()?),
            "DM_CHANNEL_CREATE" => Self::DmCreate(Box::new(serde_json::from_value(data).ok()?)),
            "USER_PROFILE_UPDATE" => {
                Self::ProfileUpdate(Box::new(serde_json::from_value(data).ok()?))
            }
            _ => return None,
        })
    }
}
