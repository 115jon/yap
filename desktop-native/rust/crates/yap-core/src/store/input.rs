//! What changes the state: user intents, effect results, gateway and voice events.

use serde_json::Value;
use yap_protocol::payload::{
    CallRing, CallRingStop, CallRinging, ChannelDeleteEvent, ChannelUpdateEvent, GuildDelete,
    GuildMemberAdd, GuildMemberRemove, GuildMemberUpdate, MessageDeleteEvent, MessageUnpinEvent,
    NotificationsRead, PresenceList, PresenceUpdateEvent, ReactionEvent, RelationshipRemove,
    TypingStartEvent, UserProfileUpdate, VoiceChannelStateUpdate, VoiceChannelStates,
};
use yap_protocol::rest::{
    DmChannel, Message, MessagePatch, Notification, ReadState, Relationship, Server,
};

use yap_cache::Snapshot;

use super::{ApiResponse, CallId, RequestId, SessionId, SignInId, VoiceUpdate};
use crate::api::{ApiError, Token};

/// What the UI asks for.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Start,
    /// The window is on screen (`false`: minimised or hidden to the tray).
    WindowShown(bool),
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
    /// A toast's inline Reply (desktop-toasts.ts `notification-reply`):
    /// posts `content` to `channel_id` as a reply to `message_id`, whether
    /// the channel is open or not. Stamped like [`Command::Send`].
    ToastReply {
        channel_id: String,
        message_id: String,
        content: String,
        nonce: String,
        sent_at: String,
    },
    /// A toast's Mark as read (`notification-mark-read`): the channel and
    /// its unread notifications are read, open or not.
    ToastMarkRead(String),
    /// The server rail menu's Mark as read (ServerList.tsx, ChatPageClient
    /// `onMarkServerRead`): every channel of the server with messages after
    /// its read mark is read, and the server's unread notifications too.
    MarkServerRead(String),
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
    /// The top-most message on screen while the list is scrolled up: the
    /// channel reopens there (useChatArea.ts `SET_SCROLL_POSITION`).
    ScrollAt(String),
    /// The open voice channel's text chat beside its view
    /// (useChatPageLogic.ts `SET_VOICE_TEXT`).
    ShowVoiceChat(bool),
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
    /// A new GIF picker query (empty: trending) for a media type, from
    /// `klipy` or `tenor`.
    GifSearch {
        query: String,
        media_type: String,
        provider: String,
    },
    /// The picker opened: the feeds kept from its last open are dropped.
    GifReset,
    /// The picker's start screen: the categories and the Trending tile.
    GifStart {
        media_type: String,
        provider: String,
    },
    /// The picker shows the saved favourites of a media type.
    GifFavorites {
        media_type: String,
    },
    /// Search terms for what is typed (two characters or more).
    GifTerms {
        query: String,
        provider: String,
    },
    /// A tile's star (its row id): saves or removes that item.
    GifTileFavorite(String),
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
    /// A typed `@name` under the pointer in the composer (its tooltip), or
    /// `None` when the pointer left it.
    MentionHover(Option<String>),
    JoinVoice(String),
    LeaveVoice,
    SetVoiceMuted(bool),
    /// Deafen (or not). Like mute, it is kept between calls.
    SetVoiceDeafened(bool),
    /// Watch (or stop watching) a participant's share in the call.
    WatchStream {
        participant_id: String,
        watch: bool,
    },
    /// Our camera on or off in the call (useVoiceChannel.ts `toggleCamera`).
    SetVoiceCamera(bool),
    /// The camera capture failed before it could go out: off, with the
    /// reason (`missing`, `busy` or `unknown`).
    VoiceCameraFailed(String),
    /// Share a source in the call (the picker's Go live).
    StartVoiceShare(super::VoiceShare),
    /// Stop sharing.
    StopVoiceShare,
    /// The share's audio on or off.
    SetVoiceShareAudio(bool),
    /// The share's quality preset.
    SetVoiceShareQuality(String),
    /// The spatial panel's edit: the room's new state, as JSON.
    SetVoiceSpatial(String),
    /// Settings → Account: the email and sign-in methods from Kova.
    LoadAccount,
    /// Settings → Sessions.
    LoadSessions,
    /// Settings → Privacy: the shared message links.
    LoadShares,
    /// Revoke a shared message link.
    RevokeShare(String),
    /// Settings → Account → Display name (empty clears it).
    SaveDisplayName(String),
    /// The profile editor's Save.
    SaveProfile(Box<ProfileDraft>),
    /// The media filter level (`high`, `medium`, `low`, `off`).
    SetMediaFilter(String),
    /// Save the theme to the account (useAppearanceTheme `persistAppearance`):
    /// `sync` off clears the saved theme.
    SaveTheme {
        theme: Option<String>,
        sync: bool,
    },
    /// Server settings, channel settings and the tree's management.
    Admin(super::AdminCommand),
    /// Shows a message (useChatArea.ts `handleJumpToMessage`): its channel
    /// opens first when another one is open (as `OpenChannel`); a loaded
    /// message is highlighted, anything else replaces the list with the
    /// window around it (`around=`), detached from the newest end.
    JumpToMessage {
        channel_id: String,
        message_id: String,
    },
    /// The detached list is near its end: load the page after it
    /// (`handleLoadAfter`).
    LoadNewer,
    /// "Jump to present": the newest page again (`handleJumpToPresent`).
    JumpToPresent,
    /// The thread panel.
    Thread(super::ThreadCommand),
    /// The pins panel, and pinning.
    Pins(super::PinsCommand),
    /// Message search (the search panel).
    Search(super::SearchCommand),
    /// Friends and relationships.
    Friends(super::FriendsCommand),
    /// Create a server, the invite landing and links into the app.
    Invites(super::InviteCommand),
    /// The soundboard, Listen Together and activities.
    Apps(super::apps::AppsCommand),
    /// The collectibles shop and pickers.
    Shop(super::shop::ShopCommand),
    /// A TikTok or Instagram card asks for its media.
    Embeds(super::embeds::EmbedsCommand),
    /// The GIF and clip favourites' stars.
    Favorites(super::favorites::FavoritesCommand),
    /// The account's camera backgrounds.
    Backgrounds(super::backgrounds::BackgroundsCommand),
    /// Clear one of our messages' link previews (the embed's x).
    RemoveEmbeds(String),
    /// The inbox, who reacted, the details tabs and the other extras.
    Extras(super::extras::ExtrasCommand),
    /// DM calls, moves, stickers and the voice details.
    Calls(super::calls::CallsCommand),
    /// The app is quitting: leave the call (its sockets close cleanly).
    Quit,
}

/// What the profile editor saves. Colours are `#RRGGBB` or `None` (Auto);
/// a picked image is a local file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProfileDraft {
    pub pronouns: String,
    pub bio: String,
    pub custom_status: String,
    pub accent_color: Option<String>,
    pub background_color: Option<String>,
    pub banner_color: Option<String>,
    /// A new avatar (path, content type).
    pub avatar: Option<(String, String)>,
    pub remove_avatar: bool,
    /// A recent avatar to use again (its stored URL), when no new one
    /// was picked.
    pub avatar_url: Option<String>,
    /// A new banner (path, content type).
    pub banner: Option<(String, String)>,
    pub remove_banner: bool,
    /// The frame editor's crop (AvatarFrameEditor.tsx): `None` leaves the
    /// stored one, `Some(None)` saves none.
    pub avatar_crop: Option<Option<crate::settings::AvatarCrop>>,
    /// The display-name style as the views carry it
    /// (`view::name_style`): `None` leaves it, `Some("")` clears it.
    pub name_style: Option<String>,
    /// A new nameplate (path, content type): a picture or a video.
    pub nameplate: Option<(String, String)>,
    /// Delete the uploaded nameplate.
    pub remove_nameplate: bool,
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
    /// The catalog kept on disk (as served) and when it was fetched.
    CatalogLoaded(Option<(serde_json::Value, i64)>),
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
    /// The local player's report.
    Audio(super::apps::AudioEvent),
    /// One typer's timeout; only the newest event of theirs clears them.
    TypingExpired {
        user_id: String,
        generation: u64,
    },
    /// A while after a gateway Ready or Resumed: rosters nobody resent are
    /// empty now. Only the newest check counts.
    VoiceRosterCheck {
        generation: u64,
    },
    /// The window has been hidden for [`super::reduce::STANDBY_HIDDEN_GRACE`].
    /// Only the newest hide counts.
    StandbyHidden {
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
    /// The server list changed (`USER_SERVERS_UPDATE`, or a `GUILD_UPDATE`
    /// whose body does not decode).
    ServersChanged,
    /// A server's settings changed: the server as stored now.
    GuildUpdate(Box<Server>),
    GuildDelete(GuildDelete),
    /// A channel was created or changed, or the tree must be refetched.
    ChannelUpdate(Box<ChannelUpdateEvent>),
    ChannelDelete(ChannelDeleteEvent),
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
    /// A message was pinned: the whole message (pins.ts).
    MessagePin(Box<Message>),
    MessageUnpin(MessageUnpinEvent),
    /// A relationship was made or changed (`RELATIONSHIP_ADD`).
    RelationshipAdd(Box<Relationship>),
    RelationshipRemove(RelationshipRemove),
    /// Someone calls us (`CALL_RING`).
    CallRing(CallRing),
    /// Our call rings on their side (`CALL_RINGING`).
    CallRinging(CallRinging),
    /// A ring is over (`CALL_RING_STOP`).
    CallRingStop(CallRingStop),
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
            "USER_SERVERS_UPDATE" => Self::ServersChanged,
            "GUILD_UPDATE" => match serde_json::from_value::<Server>(data) {
                Ok(server) => Self::GuildUpdate(Box::new(server)),
                Err(_) => Self::ServersChanged,
            },
            "GUILD_DELETE" => Self::GuildDelete(serde_json::from_value(data).ok()?),
            "CHANNEL_UPDATE" => Self::ChannelUpdate(Box::new(serde_json::from_value(data).ok()?)),
            "CHANNEL_DELETE" => Self::ChannelDelete(serde_json::from_value(data).ok()?),
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
            "MESSAGE_PIN" => Self::MessagePin(Box::new(serde_json::from_value(data).ok()?)),
            "MESSAGE_UNPIN" => Self::MessageUnpin(serde_json::from_value(data).ok()?),
            "RELATIONSHIP_ADD" => {
                Self::RelationshipAdd(Box::new(serde_json::from_value(data).ok()?))
            }
            "RELATIONSHIP_REMOVE" => Self::RelationshipRemove(serde_json::from_value(data).ok()?),
            "CALL_RING" => Self::CallRing(serde_json::from_value(data).ok()?),
            "CALL_RINGING" => Self::CallRinging(serde_json::from_value(data).ok()?),
            "CALL_RING_STOP" => Self::CallRingStop(serde_json::from_value(data).ok()?),
            _ => return None,
        })
    }
}
