//! What the reducer asks the runtime to do.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use yap_protocol::rest::{
    ChannelsResponse, DmChannel, GifCategories, GifCategory, GifPage, GifTerms, KovaAccount,
    KovaSession, Member, Message, MessagePatch, Notification, ProfileResponse, ReadStates, Server,
    SessionInfo, ShareItem, UploadedFile, User,
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
        /// Why it is asked (the `YAP_TRACE_HTTP` trace; a prefetch is
        /// answered with an error at once when the runner is busy).
        reason: HttpReason,
    },
    /// Read a channel's newest window kept on disk; answered by
    /// `Input::WindowLoaded` (empty when there is none).
    LoadWindow {
        user_id: String,
        channel_id: String,
    },
    /// Logs `[switch] <channel> <source> rows=N <ms>`, timed from the
    /// switch's start (`source` `None`) to the rows shown.
    SwitchMark {
        channel_id: String,
        source: Option<SwitchSource>,
        rows: usize,
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
    /// Read the collectibles catalog kept on disk; answered by
    /// `Input::CatalogLoaded`.
    LoadCatalog,
    /// Keep the catalog (as served) on disk. The runtime stamps it with
    /// the time it runs, which is when the response arrived.
    SaveCatalog {
        catalog: Value,
    },
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
    /// Poll the running sign-in's code now instead of at its next interval
    /// (a `signin` link: the browser just approved it).
    SignInPollNow,
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
    /// Upload one file as multipart `file`. Reports `Input::UploadProgress`
    /// while it goes, then `Input::Http` with `ApiResponse::Uploaded`.
    Upload {
        id: RequestId,
        token: Token,
        channel_id: String,
        path: String,
        name: String,
        content_type: String,
    },
    /// Stop an upload; it never answers.
    CancelUpload(RequestId),
    /// Save `url` to the file `dest`; the session goes along only to the
    /// API's own media. Answered by `Input::Http` (`Done`).
    Download {
        id: RequestId,
        token: Token,
        url: String,
        dest: String,
    },
    /// Upload a profile picture or banner as multipart `file`. Answered by
    /// `Input::Http` (`Done`).
    ProfileUpload {
        id: RequestId,
        token: Token,
        asset: ProfileAsset,
        path: String,
        content_type: String,
    },
    /// Upload a soundboard sound: multipart `file` plus `fields`
    /// (`purpose=soundboard`, name, emoji, volume) to the channel's upload
    /// route. Answered by `Input::Http` with `ApiResponse::Uploaded`.
    SoundUpload {
        id: RequestId,
        token: Token,
        channel_id: String,
        path: String,
        name: String,
        content_type: String,
        fields: Vec<(String, String)>,
    },
    /// Upload a camera background as multipart `file` to
    /// `/api/camera-backgrounds` (at most 25 MB). Answered by `Input::Http`
    /// with `ApiResponse::Backgrounds(Uploaded)`.
    BackgroundUpload {
        id: RequestId,
        token: Token,
        path: String,
        content_type: String,
    },
    /// Upload a voice status picture as multipart `file` with
    /// `preview_width` and `preview_height` to the channel's
    /// `voice-status-media` route, naming our voice session (at most
    /// 25 MB). Answered by `Input::Http` with
    /// `ApiResponse::Extras(VoiceMediaAdded)`.
    VoiceMediaUpload {
        id: RequestId,
        token: Token,
        channel_id: String,
        path: String,
        content_type: String,
        width: u32,
        height: u32,
        voice_session: String,
    },
    /// Tell the local player (soundboard clips, the Listen Together
    /// stream). `token` fetches our own media.
    Audio {
        token: Option<Token>,
        command: super::apps::AudioCommand,
    },
    /// Replace one of the voice apps' files in the app folder (custom
    /// sounds, Daily Word boards) with `contents` (JSON). No answer.
    SaveLocal {
        file: &'static str,
        contents: String,
    },
    /// A deleted camera background: remove its kept picture
    /// (`backgrounds::cached_path` in the app folder), if there is one.
    /// No answer.
    RemoveCachedBackground {
        id: String,
    },
}

/// Why a request is made, for the `[http]` trace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum HttpReason {
    #[default]
    Other,
    /// A channel opened with nothing to show yet.
    Open,
    /// A refetch of rows already on screen (warm, disk, a tree).
    Revalidate,
    /// A channel the pointer rests on, fetched into the warm cache only.
    Prefetch,
    /// The saved scroll position's window (`around=`).
    Window,
}

impl HttpReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Other => "other",
            Self::Open => "open",
            Self::Revalidate => "revalidate",
            Self::Prefetch => "prefetch",
            Self::Window => "window",
        }
    }
}

/// Where a switched-to channel's first rows came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwitchSource {
    /// The warm cache: shown in the same reduce.
    Warm,
    /// The window kept on disk.
    Disk,
    /// The network page.
    Cold,
}

impl SwitchSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warm => "warm",
            Self::Disk => "disk",
            Self::Cold => "cold",
        }
    }
}

/// Which profile image an upload replaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileAsset {
    /// `POST /api/avatar-upload`, keeping the avatar's collectibles
    /// (`avatar_display`, serialized) when there are any.
    Avatar { display: Option<String> },
    /// `POST /api/profile-assets/manage` with `kind=banner`.
    Banner,
    /// The same route with `kind=nameplate`: a picture or a video.
    Nameplate,
    /// `POST /api/servers/icon-upload`; answers with the icon's URL.
    ServerIcon,
}

impl ProfileAsset {
    pub fn path(&self) -> &'static str {
        match self {
            Self::Avatar { .. } => "/api/avatar-upload",
            Self::Banner | Self::Nameplate => "/api/profile-assets/manage",
            Self::ServerIcon => "/api/servers/icon-upload",
        }
    }
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
        /// Uploaded files to link, and which of them are sensitive.
        attachment_ids: Vec<String>,
        nsfw_attachment_ids: Vec<String>,
    },
    /// Adds (PUT) or removes (DELETE) the user's reaction.
    React {
        channel_id: String,
        message_id: String,
        emoji: String,
        add: bool,
    },
    /// A page of the GIF picker: trending (empty query) or a search, the
    /// next page after `next`. `limit` 1 is the Trending tile's picture.
    Gifs {
        query: String,
        media_type: String,
        /// `klipy` or `tenor`.
        provider: String,
        limit: u32,
        next: Option<String>,
        customer_id: String,
        /// The account's media filter (`high` unless chosen otherwise).
        content_filter: String,
    },
    /// Registers a picked GIF as an attachment (the server keeps the URL).
    GifUpload {
        channel_id: String,
        source_url: String,
        filename: String,
        content_type: String,
        provider: String,
        size_bytes: u64,
    },
    /// The picker's start screen: the provider's categories for a media type.
    GifCategories {
        media_type: String,
        provider: String,
        customer_id: String,
        content_filter: String,
    },
    /// Search terms for a partial query: `autocomplete` or `suggestions`.
    GifTerms {
        mode: &'static str,
        query: String,
        provider: String,
        customer_id: String,
        content_filter: String,
    },
    /// Tells Klipy a GIF was shared (best effort, as the web does).
    GifShare {
        id: String,
        query: String,
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
    /// Changes to the signed-in profile (`PATCH /api/update-profile`); the
    /// body is the patch as the web sends it.
    UpdateProfile(Value),
    /// Drop the banner (`DELETE /api/profile-assets/manage`).
    DeleteBanner,
    /// Drop the uploaded nameplate (the same route, `kind=nameplate`).
    DeleteNameplate,
    Sessions,
    Shares,
    RevokeShare {
        share_id: String,
    },
    /// Kova's own session (on the auth server): the account's email.
    KovaSession,
    /// Kova's linked sign-in methods (on the auth server).
    KovaAccounts,
    /// Server and channel administration.
    Admin(super::AdminRequest),
    /// Threads, pins and jump windows.
    Threads(super::ThreadRequest),
    /// Message search in a server or a DM.
    Search(super::SearchRequest),
    /// `/api/friends`.
    Friends(super::FriendsRequest),
    /// Creating and joining servers.
    Invites(super::InviteRequest),
    /// The soundboard, Listen Together and the Wordle puzzle.
    Apps(super::apps::AppsRequest),
    /// The collectibles shop.
    Shop(super::shop::ShopRequest),
    /// A TikTok or Instagram lookup.
    Embeds(super::embeds::EmbedsRequest),
    /// The GIF and clip favourites.
    Favorites(super::favorites::FavoritesRequest),
    /// The account's camera backgrounds.
    Backgrounds(super::backgrounds::BackgroundsRequest),
    /// `PATCH /api/channels/{id}/messages {message_id, embeds: []}`.
    RemoveEmbeds {
        channel_id: String,
        message_id: String,
    },
    /// The inbox, the details tabs, voice status and the other extras.
    Extras(super::extras::ExtrasRequest),
    /// `POST /api/voice/move`: a moderator moves someone.
    VoiceMove(yap_protocol::payload::VoiceMoveRequest),
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
    /// The full user and the servers you share.
    Profile(
        Box<User>,
        yap_protocol::rest::MutualServers,
        yap_protocol::rest::MutualFriends,
    ),
    /// A file (or a GIF pick) the next message can link.
    Uploaded(UploadedFile),
    Gifs(GifPage),
    GifCategories(Vec<GifCategory>),
    GifTerms(Vec<String>),
    Sessions(Vec<SessionInfo>),
    Shares(Vec<ShareItem>),
    /// The account's email, when Kova has one.
    KovaEmail(Option<String>),
    /// Linked provider ids.
    KovaAccounts(Vec<String>),
    Admin(super::AdminResponse),
    Threads(super::ThreadResponse),
    Search(Box<crate::search::SearchResponse>),
    Friends(super::FriendsResponse),
    Invites(super::InviteResponse),
    Apps(super::apps::AppsResponse),
    Shop(super::shop::ShopResponse),
    /// A lookup's payload, as served.
    Embeds(Value),
    /// The whole favourites list.
    Favorites(Vec<Value>),
    Backgrounds(super::backgrounds::BackgroundsResponse),
    Extras(super::extras::ExtrasResponse),
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
    /// Kova's auth server answers this, not the Yap API.
    pub fn on_kova(&self) -> bool {
        matches!(self, Self::KovaSession | Self::KovaAccounts)
    }

    /// A GET of this whole URL on another host, without the session.
    pub fn public_url(&self) -> Option<String> {
        match self {
            Self::Apps(request) => request.public_url(),
            _ => None,
        }
    }

    /// Headers beyond the session's (the voice status routes').
    pub fn headers(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::Extras(request) => request.headers(),
            _ => Vec::new(),
        }
    }

    pub fn method(&self) -> Method {
        match self {
            Self::Extras(request) => request.method(),
            Self::Admin(request) => request.method(),
            Self::Threads(request) => request.method(),
            Self::Search(request) => request.method(),
            Self::Friends(request) => request.method(),
            Self::Invites(request) => request.method(),
            Self::Apps(request) => request.method(),
            Self::Shop(request) => request.method(),
            Self::Embeds(request) => request.method(),
            Self::Favorites(request) => request.method(),
            Self::Backgrounds(request) => request.method(),
            Self::RemoveEmbeds { .. } => Method::Patch,
            Self::VoiceMove(_) => Method::Post,
            Self::SendMessage { .. }
            | Self::Typing { .. }
            | Self::OpenDm { .. }
            | Self::SetPresence { .. }
            | Self::GifUpload { .. } => Method::Post,
            Self::UpdateProfile(_) => Method::Patch,
            Self::DeleteBanner | Self::DeleteNameplate | Self::RevokeShare { .. } => Method::Delete,
            Self::Sessions | Self::Shares | Self::KovaSession | Self::KovaAccounts => Method::Get,
            Self::MarkRead { .. } => Method::Put,
            Self::React { add: true, .. } => Method::Put,
            Self::React { add: false, .. } => Method::Delete,
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
            | Self::Profile { .. }
            | Self::Gifs { .. }
            | Self::GifCategories { .. }
            | Self::GifTerms { .. }
            | Self::GifShare { .. } => Method::Get,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Extras(request) => request.path(),
            Self::Admin(request) => request.path(),
            Self::Threads(request) => request.path(),
            Self::Search(request) => request.path(),
            Self::Friends(request) => request.path(),
            Self::Invites(request) => request.path(),
            Self::Apps(request) => request.path(),
            Self::Shop(request) => request.path(),
            Self::Embeds(request) => request.path(),
            Self::Favorites(request) => request.path(),
            Self::Backgrounds(request) => request.path(),
            Self::RemoveEmbeds { channel_id, .. } => format!("/api/channels/{channel_id}/messages"),
            Self::VoiceMove(_) => "/api/voice/move".into(),
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
            Self::UpdateProfile(_) => "/api/update-profile".into(),
            Self::DeleteBanner | Self::DeleteNameplate => "/api/profile-assets/manage".into(),
            Self::Sessions => "/api/sessions".into(),
            Self::Shares => "/api/shares".into(),
            Self::RevokeShare { share_id } => format!("/api/shares/{}", query_value(share_id)),
            Self::KovaSession => "/api/auth/get-session".into(),
            Self::KovaAccounts => "/api/auth/list-accounts".into(),
            Self::React { channel_id, .. } => format!("/api/channels/{channel_id}/reactions"),
            Self::Gifs {
                query,
                media_type,
                provider,
                limit,
                next,
                customer_id,
                content_filter,
            } => {
                let mut path = format!(
                    "/api/gifs?mode=search&provider={}&limit={limit}&mediaType={}&contentFilter={}",
                    query_value(provider),
                    query_value(media_type),
                    query_value(content_filter)
                );
                if !customer_id.is_empty() {
                    path.push_str(&format!("&customerId={}", query_value(customer_id)));
                }
                if !query.is_empty() {
                    path.push_str(&format!("&q={}", query_value(query)));
                }
                if let Some(next) = next {
                    path.push_str(&format!("&next={}", query_value(next)));
                }
                path
            }
            Self::GifCategories {
                media_type,
                provider,
                customer_id,
                content_filter,
            } => {
                let mut path = format!(
                    "/api/gifs?mode=categories&provider={}&mediaType={}&contentFilter={}",
                    query_value(provider),
                    query_value(media_type),
                    query_value(content_filter)
                );
                if !customer_id.is_empty() {
                    path.push_str(&format!("&customerId={}", query_value(customer_id)));
                }
                path
            }
            Self::GifTerms {
                mode,
                query,
                provider,
                customer_id,
                content_filter,
            } => {
                let mut path = format!(
                    "/api/gifs?mode={mode}&q={}&provider={}&contentFilter={}",
                    query_value(query),
                    query_value(provider),
                    query_value(content_filter)
                );
                if !customer_id.is_empty() {
                    path.push_str(&format!("&customerId={}", query_value(customer_id)));
                }
                path
            }
            Self::GifUpload { channel_id, .. } => {
                format!("/api/channels/{channel_id}/messages/gif")
            }
            Self::GifShare { id, query } => {
                let mut path = format!(
                    "/api/gifs?mode=register-share&provider=klipy&id={}",
                    query_value(id)
                );
                if !query.is_empty() {
                    path.push_str(&format!("&q={}", query_value(query)));
                }
                path
            }
        }
    }

    /// The JSON body (`Null` for a GET).
    pub fn body(&self) -> Value {
        match self {
            Self::Extras(request) => request.body(),
            Self::Admin(request) => request.body(),
            Self::Threads(request) => request.body(),
            Self::Search(_) => Value::Null,
            Self::Friends(request) => request.body(),
            Self::Invites(request) => request.body(),
            Self::Apps(request) => request.body(),
            Self::Shop(request) => request.body(),
            Self::Embeds(_) => Value::Null,
            Self::Favorites(request) => request.body(),
            Self::Backgrounds(request) => request.body(),
            Self::VoiceMove(request) => serde_json::to_value(request).unwrap_or(Value::Null),
            Self::RemoveEmbeds { message_id, .. } => {
                json!({ "message_id": message_id, "embeds": [] })
            }
            Self::SendMessage {
                content,
                reply_to_id,
                nonce,
                attachment_ids,
                nsfw_attachment_ids,
                ..
            } => {
                let mut body = json!({ "content": content, "nonce": nonce });
                if let Some(reply) = reply_to_id {
                    body["reply_to_id"] = json!(reply);
                }
                if !attachment_ids.is_empty() {
                    body["attachment_ids"] = json!(attachment_ids);
                }
                if !nsfw_attachment_ids.is_empty() {
                    body["nsfw_attachment_ids"] = json!(nsfw_attachment_ids);
                }
                body
            }
            Self::React {
                message_id, emoji, ..
            } => json!({ "message_id": message_id, "emoji": emoji }),
            Self::GifUpload {
                source_url,
                filename,
                content_type,
                provider,
                size_bytes,
                ..
            } => json!({
                "source_url": source_url,
                "filename": filename,
                "content_type": content_type,
                "provider": provider,
                "size_bytes": size_bytes,
            }),
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
            Self::UpdateProfile(patch) => patch.clone(),
            Self::DeleteBanner => json!({ "kind": "banner" }),
            Self::DeleteNameplate => json!({ "kind": "nameplate" }),
            Self::Sessions
            | Self::Shares
            | Self::RevokeShare { .. }
            | Self::KovaSession
            | Self::KovaAccounts => Value::Null,
            Self::Me
            | Self::Servers
            | Self::Channels { .. }
            | Self::Messages { .. }
            | Self::ReadStates
            | Self::Notifications
            | Self::Members { .. }
            | Self::Dms
            | Self::Profile { .. }
            | Self::Gifs { .. }
            | Self::GifCategories { .. }
            | Self::GifTerms { .. }
            | Self::GifShare { .. } => Value::Null,
        }
    }

    /// Decodes a success body into the response for this request.
    pub fn decode(&self, mut body: Value) -> Result<ApiResponse, ApiError> {
        Ok(match self {
            Self::Extras(request) => ApiResponse::Extras(request.decode(body)?),
            Self::Admin(request) => ApiResponse::Admin(request.decode(body)?),
            Self::Threads(request) => ApiResponse::Threads(request.decode(body)?),
            Self::Search(request) => ApiResponse::Search(Box::new(request.decode(body)?)),
            Self::Friends(request) => ApiResponse::Friends(request.decode(body)?),
            Self::Invites(request) => ApiResponse::Invites(request.decode(body)?),
            Self::Apps(request) => ApiResponse::Apps(request.decode(body)?),
            Self::Shop(request) => ApiResponse::Shop(request.decode(body)?),
            Self::Embeds(_) => ApiResponse::Embeds(body),
            Self::Favorites(request) => ApiResponse::Favorites(request.decode(body)?),
            Self::Backgrounds(request) => ApiResponse::Backgrounds(request.decode(body)?),
            Self::RemoveEmbeds { .. } | Self::VoiceMove(_) => ApiResponse::Done,
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
                ApiResponse::Profile(
                    Box::new(profile.user),
                    profile.mutual_servers,
                    profile.mutual_friends,
                )
            }
            Self::GifUpload { .. } => ApiResponse::Uploaded(
                serde_json::from_value(body)
                    .map_err(|e| ApiError::Decode(format!("gif upload: {e}")))?,
            ),
            Self::Gifs { .. } => ApiResponse::Gifs(
                serde_json::from_value(body).map_err(|e| ApiError::Decode(format!("gifs: {e}")))?,
            ),
            Self::GifCategories { .. } => ApiResponse::GifCategories(
                serde_json::from_value::<GifCategories>(body)
                    .map_err(|e| ApiError::Decode(format!("gif categories: {e}")))?
                    .categories,
            ),
            Self::GifTerms { .. } => ApiResponse::GifTerms(
                serde_json::from_value::<GifTerms>(body)
                    .map(|terms| terms.results)
                    .unwrap_or_default(),
            ),
            Self::Sessions => ApiResponse::Sessions(lenient_list(take(&mut body, "sessions"))),
            Self::Shares => ApiResponse::Shares(lenient_list(take(&mut body, "shares"))),
            // `null` when Kova has no session for the token.
            Self::KovaSession => ApiResponse::KovaEmail(
                serde_json::from_value::<KovaSession>(body)
                    .ok()
                    .and_then(|session| session.user.email)
                    .filter(|email| !email.is_empty()),
            ),
            Self::KovaAccounts => ApiResponse::KovaAccounts(
                lenient_list::<KovaAccount>(body)
                    .into_iter()
                    .map(|account| account.provider_id)
                    .collect(),
            ),
            Self::UpdateProfile(_)
            | Self::DeleteBanner
            | Self::DeleteNameplate
            | Self::RevokeShare { .. } => ApiResponse::Done,
            Self::DeleteMessage { .. }
            | Self::React { .. }
            | Self::GifShare { .. }
            | Self::SetPresence { .. }
            | Self::Typing { .. }
            | Self::MarkRead { .. }
            | Self::MarkNotificationsRead { .. } => ApiResponse::Done,
        })
    }
}
