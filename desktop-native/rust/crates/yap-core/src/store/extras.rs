//! The smaller chat, REST and settings surfaces the web has around the
//! message list: the notification inbox (NotificationBell.tsx), who reacted
//! (ReactionDetailsModal.tsx), the channel details' Media, Links and Files
//! tabs (MemberList.tsx), a voice channel's status
//! (VoiceChannelTextStatusModal.tsx, VoiceChannelMediaStatusModal.tsx), the
//! recent avatars (AvatarPickerModal.tsx), AI emoji (EmojiPicker.tsx), the
//! catalog admins' sync, Mark unread, shared message links
//! (MessageShareModal.tsx), the account's username, claims and sessions
//! (AccountPanel.tsx, SessionsPanel.tsx) and the channel's quick invite
//! (ChannelInviteModal.tsx).
//!
//! What the UI asks for is an [`ExtrasCommand`]; each REST call is an
//! [`ExtrasRequest`]; everything QML draws is one [`ExtrasView`], sent
//! whenever it changes.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use yap_protocol::rest::{Channel, Notification, ReadState, User};

use super::time::{local_date, parse_rfc3339, LocalOffset};
use super::view::query_value;
use super::{Method, RequestId};
use crate::api::ApiError;

/// The inbox's page (NotificationBell.tsx loads the 50 newest).
pub const INBOX_LIMIT: usize = 50;
/// The voice channel status text's limit (voice-channel-status.ts).
pub const MAX_VOICE_STATUS_TEXT: usize = 120;
/// The AI emoji prompt's limit (lib/emoji.ts `MAX_AI_EMOJI_PROMPT_LENGTH`).
pub const MAX_EMOJI_PROMPT: usize = 200;
/// Recent avatars shown (AvatarPickerModal.tsx: "Your last 6 uploads").
pub const RECENT_AVATARS: usize = 6;
/// The channel invite's lifetime (ChannelInviteModal.tsx: 7 days).
pub const CHANNEL_INVITE_MAX_AGE: i64 = 604_800;

/// A tab of the channel details (MemberList.tsx `TabId`, the media ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaTab {
    Media,
    Links,
    Files,
}

impl MediaTab {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "media" => Some(Self::Media),
            "links" => Some(Self::Links),
            "files" => Some(Self::Files),
            _ => None,
        }
    }

    /// The route's `type` (images covers pictures and videos).
    fn query(self) -> &'static str {
        match self {
            Self::Media => "images",
            Self::Links => "links",
            Self::Files => "files",
        }
    }
}

/// A share link's lifetime (MessageShareModal.tsx `ShareExpiry`).
pub const SHARE_EXPIRIES: [&str; 4] = ["7d", "30d", "90d", "never"];

#[derive(Debug, Clone, PartialEq)]
pub enum ExtrasCommand {
    /// The bell opened: the inbox loads the first time.
    OpenInbox,
    /// A row was opened (it is marked read first when unread).
    ReadNotification(String),
    /// Mark all read.
    ReadAllNotifications,
    /// Every notification goes (`DELETE /api/notifications`).
    ClearNotifications,
    /// Who reacted to a message of the open list, on one emoji's tab
    /// (`None`: the first).
    ShowReactions {
        message_id: String,
        emoji: Option<String>,
    },
    SelectReaction(String),
    CloseReactions,
    /// A moderator takes someone's reaction off (Manage messages).
    RemoveReaction {
        emoji: String,
        user_id: String,
    },
    /// A channel details tab showed: its list loads afresh.
    LoadMedia(MediaTab),
    /// The voice channel status: the text (empty clears it), keeping the
    /// picture.
    SetVoiceStatus {
        channel_id: String,
        text: String,
    },
    /// The status picture dialog opened: the channel's recent pictures.
    LoadVoiceMedia(String),
    /// A GIF picker tile (its row id) becomes the status picture.
    SetVoiceMediaGif {
        channel_id: String,
        gif_id: String,
    },
    /// A recent picture (its asset id) becomes the status picture.
    UseVoiceMedia {
        channel_id: String,
        asset_id: String,
    },
    RemoveVoiceMedia(String),
    CloseVoiceStatus,
    /// The avatar picker opened.
    LoadAvatars,
    DeleteAvatar(String),
    /// The emoji picker opened (`silent`: the poll while some generate).
    LoadEmojis {
        silent: bool,
    },
    GenerateEmoji {
        prompt: String,
        shortcode: String,
    },
    ClearEmojiNotes,
    /// A catalog admin forces an upstream sync.
    SyncCatalog,
    /// The sync's note has shown long enough (fed back after 6 s).
    ClearSyncNote(u32),
    /// Mark unread from this message on (the message menu).
    MarkUnread(String),
    /// A public link to a message of the open list.
    ShareMessage {
        message_id: String,
        expires: String,
    },
    CloseShare,
    /// The username field changed (the UI waits 400 ms first).
    CheckUsername(String),
    SaveUsername(String),
    CancelUsername,
    /// Settings → Account: legacy accounts that may be this person's.
    LoadClaims,
    Claim(String),
    /// Settings → Sessions: revoke another device's session.
    RevokeSession(String),
    /// The channel's quick invite: a fresh 7-day link to it.
    ChannelInvite {
        server_id: String,
        channel_id: String,
    },
    CloseInvite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtrasRequest {
    Inbox,
    ReadNotifications {
        /// `None`: all of them.
        ids: Option<Vec<String>>,
    },
    ClearNotifications,
    RemoveReaction {
        channel_id: String,
        message_id: String,
        emoji: String,
        target_user_id: String,
    },
    ChannelMedia {
        channel_id: String,
        tab: MediaTab,
    },
    /// `PATCH /api/channels/{id}/voice-status`; the header names our voice
    /// session (the route needs us connected there).
    VoiceStatus {
        channel_id: String,
        status: Value,
        voice_session: String,
    },
    VoiceMedia {
        channel_id: String,
        voice_session: String,
    },
    /// Registers a GIF as a recent status picture.
    AddVoiceMedia {
        channel_id: String,
        media: Value,
        voice_session: String,
    },
    AvatarHistory,
    DeleteAvatar {
        id: String,
    },
    Emojis,
    GenerateEmoji {
        prompt: String,
        shortcode: String,
    },
    SyncCatalog,
    MarkUnread {
        channel_id: String,
        message_id: String,
    },
    Share {
        message_id: String,
        expires: String,
    },
    CheckUsername {
        username: String,
    },
    SaveUsername {
        username: String,
    },
    Claims,
    Claim {
        legacy_user_id: String,
    },
    RevokeSession {
        session_id: String,
    },
    ChannelInvite {
        server_id: String,
        channel_id: String,
    },
}

impl ExtrasRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Inbox
            | Self::ChannelMedia { .. }
            | Self::VoiceMedia { .. }
            | Self::AvatarHistory
            | Self::Emojis
            | Self::CheckUsername { .. }
            | Self::Claims => Method::Get,
            Self::ReadNotifications { .. }
            | Self::VoiceStatus { .. }
            | Self::MarkUnread { .. }
            | Self::SaveUsername { .. } => Method::Patch,
            Self::ClearNotifications
            | Self::RemoveReaction { .. }
            | Self::DeleteAvatar { .. }
            | Self::RevokeSession { .. } => Method::Delete,
            Self::AddVoiceMedia { .. }
            | Self::GenerateEmoji { .. }
            | Self::SyncCatalog
            | Self::Share { .. }
            | Self::Claim { .. }
            | Self::ChannelInvite { .. } => Method::Post,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Inbox => format!("/api/notifications?limit={INBOX_LIMIT}"),
            Self::ReadNotifications { .. } | Self::ClearNotifications => {
                "/api/notifications".into()
            }
            Self::RemoveReaction { channel_id, .. } => {
                format!("/api/channels/{channel_id}/reactions")
            }
            Self::ChannelMedia { channel_id, tab } => {
                format!("/api/channels/{channel_id}/media?type={}", tab.query())
            }
            Self::VoiceStatus { channel_id, .. } => {
                format!("/api/channels/{channel_id}/voice-status")
            }
            Self::VoiceMedia { channel_id, .. } | Self::AddVoiceMedia { channel_id, .. } => {
                format!("/api/channels/{channel_id}/voice-status-media")
            }
            Self::AvatarHistory | Self::DeleteAvatar { .. } => "/api/avatars/history".into(),
            Self::Emojis | Self::GenerateEmoji { .. } => "/api/emojis".into(),
            Self::SyncCatalog => "/api/collectibles/sync".into(),
            Self::MarkUnread { channel_id, .. } => format!("/api/channels/{channel_id}/read-state"),
            Self::Share { message_id, .. } => format!("/api/messages/{message_id}/share"),
            Self::CheckUsername { username } => {
                format!("/api/check-username?username={}", query_value(username))
            }
            Self::SaveUsername { .. } => "/api/update-profile".into(),
            Self::Claims | Self::Claim { .. } => "/api/account-claims".into(),
            Self::RevokeSession { .. } => "/api/sessions".into(),
            Self::ChannelInvite { server_id, .. } => format!("/api/servers/{server_id}/invites"),
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::ReadNotifications { ids: Some(ids) } => json!({ "ids": ids }),
            Self::ReadNotifications { ids: None } => json!({ "all": true }),
            Self::RemoveReaction {
                message_id,
                emoji,
                target_user_id,
                ..
            } => json!({
                "message_id": message_id,
                "emoji": emoji,
                "target_user_id": target_user_id,
            }),
            Self::VoiceStatus { status, .. } => json!({ "voice_status": status }),
            Self::AddVoiceMedia { media, .. } => json!({ "media": media }),
            Self::DeleteAvatar { id } => json!({ "id": id }),
            Self::GenerateEmoji { prompt, shortcode } if shortcode.is_empty() => {
                json!({ "prompt": prompt })
            }
            Self::GenerateEmoji { prompt, shortcode } => {
                json!({ "prompt": prompt, "shortcode": shortcode })
            }
            Self::SyncCatalog => json!({}),
            Self::MarkUnread { message_id, .. } => json!({ "message_id": message_id }),
            Self::Share { expires, .. } => json!({ "expires": expires }),
            Self::SaveUsername { username } => json!({ "username": username }),
            Self::Claim { legacy_user_id } => json!({ "legacyUserId": legacy_user_id }),
            Self::RevokeSession { session_id } => json!({ "sessionId": session_id }),
            Self::ChannelInvite { channel_id, .. } => {
                json!({ "channel_id": channel_id, "max_age": CHANNEL_INVITE_MAX_AGE })
            }
            Self::Inbox
            | Self::ClearNotifications
            | Self::ChannelMedia { .. }
            | Self::VoiceMedia { .. }
            | Self::AvatarHistory
            | Self::Emojis
            | Self::CheckUsername { .. }
            | Self::Claims => Value::Null,
        }
    }

    /// Extra headers: the voice session the status routes check.
    pub fn headers(&self) -> Vec<(&'static str, String)> {
        match self {
            Self::VoiceStatus { voice_session, .. }
            | Self::VoiceMedia { voice_session, .. }
            | Self::AddVoiceMedia { voice_session, .. }
                if !voice_session.is_empty() =>
            {
                vec![("X-Voice-Session-Id", voice_session.clone())]
            }
            _ => Vec::new(),
        }
    }

    pub fn decode(&self, mut body: Value) -> Result<ExtrasResponse, ApiError> {
        let decode = |what: &str, e: serde_json::Error| ApiError::Decode(format!("{what}: {e}"));
        Ok(match self {
            Self::Inbox => ExtrasResponse::Inbox(lenient(take(&mut body, "notifications"))),
            Self::ChannelMedia { .. } => ExtrasResponse::Media(lenient(take(&mut body, "items"))),
            Self::VoiceStatus { .. } => ExtrasResponse::Channel(Box::new(
                serde_json::from_value(body).map_err(|e| decode("channel", e))?,
            )),
            Self::VoiceMedia { .. } => {
                ExtrasResponse::VoiceMedia(lenient(take(&mut body, "items")))
            }
            Self::AddVoiceMedia { .. } => ExtrasResponse::VoiceMediaAdded(
                serde_json::from_value(take(&mut body, "item")).map_err(|e| decode("media", e))?,
            ),
            Self::AvatarHistory => ExtrasResponse::Avatars(lenient(take(&mut body, "items"))),
            Self::Emojis => ExtrasResponse::Emojis(lenient(take(&mut body, "items"))),
            Self::GenerateEmoji { .. } => ExtrasResponse::Emoji(
                serde_json::from_value(take(&mut body, "item")).map_err(|e| decode("emoji", e))?,
            ),
            Self::SyncCatalog => ExtrasResponse::Catalog(take(&mut body, "catalog")),
            Self::MarkUnread { .. } => ExtrasResponse::ReadState(
                serde_json::from_value(body).map_err(|e| decode("read", e))?,
            ),
            Self::Share { .. } => ExtrasResponse::Shared(
                body.get("share_url")
                    .and_then(Value::as_str)
                    .filter(|url| !url.is_empty())
                    .ok_or_else(|| ApiError::Decode("share: no link".into()))?
                    .to_owned(),
            ),
            Self::CheckUsername { .. } => ExtrasResponse::UsernameAvailable(
                body.get("available")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            ),
            Self::Claims => {
                let claimed = body
                    .get("claimed")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                ExtrasResponse::Claims(if claimed {
                    Vec::new()
                } else {
                    lenient(take(&mut body, "candidates"))
                })
            }
            Self::ChannelInvite { .. } => ExtrasResponse::Invite(
                body.get("code")
                    .and_then(Value::as_str)
                    .filter(|code| !code.is_empty())
                    .ok_or_else(|| ApiError::Decode("invite: no code".into()))?
                    .to_owned(),
            ),
            Self::ReadNotifications { .. }
            | Self::ClearNotifications
            | Self::RemoveReaction { .. }
            | Self::DeleteAvatar { .. }
            | Self::SaveUsername { .. }
            | Self::Claim { .. }
            | Self::RevokeSession { .. } => ExtrasResponse::Done,
        })
    }
}

fn take(value: &mut Value, key: &str) -> Value {
    value
        .as_object_mut()
        .and_then(|object| object.remove(key))
        .unwrap_or(Value::Null)
}

/// Every element that decodes; one odd row never hides a list.
fn lenient<T: for<'de> Deserialize<'de>>(value: Value) -> Vec<T> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExtrasResponse {
    Inbox(Vec<Notification>),
    Media(Vec<MediaItem>),
    Channel(Box<Channel>),
    VoiceMedia(Vec<VoiceMediaAsset>),
    VoiceMediaAdded(VoiceMediaAsset),
    Avatars(Vec<AvatarUpload>),
    Emojis(Vec<GeneratedEmoji>),
    Emoji(GeneratedEmoji),
    /// The catalog as served after a sync.
    Catalog(Value),
    ReadState(ReadState),
    /// The public link.
    Shared(String),
    UsernameAvailable(bool),
    Claims(Vec<ClaimCandidate>),
    /// The invite's code.
    Invite(String),
    Done,
}

/// Who posted a media, link or file item.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ItemAuthor {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl ItemAuthor {
    pub fn label(&self) -> &str {
        match self.display_name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => name,
            _ => &self.username,
        }
    }
}

/// One row of `GET /api/channels/{id}/media` (any `type`): files and
/// pictures carry the file fields, links the message `content`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct MediaItem {
    pub id: String,
    pub message_id: String,
    pub filename: String,
    pub file_key: String,
    pub url: String,
    pub source_url: Option<String>,
    pub content_type: String,
    pub size_bytes: u64,
    /// `attachment` or `embed`.
    pub source_kind: String,
    pub thumbnail_url: Option<String>,
    pub is_gif: bool,
    pub content: String,
    pub author: ItemAuthor,
    pub created_at: String,
}

/// A recent status picture (`VoiceChannelStatusMediaAsset`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct VoiceMediaAsset {
    pub id: String,
    pub media: Value,
}

/// One recent avatar (`GET /api/avatars/history`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct AvatarUpload {
    pub id: String,
    pub avatar_url: String,
    pub content_type: String,
    pub created_at: String,
}

/// One AI emoji (`GeneratedEmoji`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct GeneratedEmoji {
    pub id: String,
    pub shortcode: String,
    pub prompt: String,
    /// `pending`, `ready` or `failed`.
    pub status: String,
    pub image_url: Option<String>,
    pub created_at: String,
}

/// A legacy account that may be this person's (`GET /api/account-claims`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ClaimCandidate {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// What a request was for.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtrasOp {
    Inbox,
    /// A read or clear; on a refusal the inbox loads again.
    InboxChange,
    Media(MediaTab),
    VoiceStatus,
    VoiceMedia,
    /// A GIF registered; then the status takes it.
    VoiceMediaAdded {
        channel_id: String,
    },
    Avatars,
    DeleteAvatar(String),
    Emojis,
    Generate,
    Sync,
    /// The read mark before, to put back on a refusal.
    MarkUnread {
        channel_id: String,
        optimistic: String,
        previous: Option<String>,
    },
    Share,
    CheckUsername(String),
    SaveUsername(String),
    Claims,
    Claim,
    RevokeSession(String),
    Invite,
}

/// A load's state, as the view says it.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Fetch {
    #[default]
    Idle,
    Loading(RequestId),
    Ready,
    Failed(String),
}

impl Fetch {
    pub fn is_loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }

    pub fn error(&self) -> String {
        match self {
            Self::Failed(error) => error.clone(),
            _ => String::new(),
        }
    }
}

/// The username field's availability (AccountPanel.tsx `UsernameCheckState`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UsernameCheck {
    #[default]
    Idle,
    Checking,
    Available,
    Unavailable,
    Invalid,
    Error,
}

/// What the store keeps.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExtrasState {
    /// Every notification of the inbox, newest first (read ones too).
    pub(super) inbox: Vec<Notification>,
    pub(super) inbox_fetch: Fetch,
    /// The message and the selected emoji of the open reactions dialog.
    pub(super) reactions: Option<(String, Option<String>)>,
    /// The details tab's channel, list and load.
    pub(super) media_channel: String,
    pub(super) media_tab: Option<MediaTab>,
    pub(super) media: Vec<MediaItem>,
    pub(super) media_fetch: Fetch,
    pub(super) status_saving: Option<RequestId>,
    pub(super) status_error: String,
    /// The status that saved, for the dialog to close on.
    pub(super) status_saved: u32,
    pub(super) voice_media: Vec<VoiceMediaAsset>,
    pub(super) voice_media_fetch: Fetch,
    pub(super) avatars: Vec<AvatarUpload>,
    pub(super) avatars_fetch: Fetch,
    pub(super) avatar_busy: Option<String>,
    pub(super) avatar_error: String,
    pub(super) emojis: Vec<GeneratedEmoji>,
    pub(super) emojis_fetch: Fetch,
    pub(super) generating: Option<RequestId>,
    pub(super) generate_error: String,
    pub(super) generate_success: String,
    pub(super) sync: Option<RequestId>,
    /// The sync's outcome: (ok, sentence).
    pub(super) sync_note: Option<(bool, String)>,
    /// Counts the notes, so only the newest one's timeout clears it.
    pub(super) sync_serial: u32,
    /// A channel whose automatic read mark waits for the user (Mark unread).
    pub(super) unread_hold: Option<String>,
    pub(super) share_message: String,
    pub(super) share_busy: Option<RequestId>,
    pub(super) share_url: String,
    pub(super) share_error: String,
    pub(super) username_draft: String,
    pub(super) username_check: UsernameCheck,
    pub(super) username_checked: String,
    pub(super) username_saving: Option<RequestId>,
    pub(super) username_error: String,
    pub(super) username_saved: u32,
    pub(super) claims: Vec<ClaimCandidate>,
    pub(super) claiming: Option<RequestId>,
    pub(super) claim_error: String,
    pub(super) revoking: Option<String>,
    pub(super) revoke_error: Option<(String, String)>,
    pub(super) invite_channel: String,
    pub(super) invite_fetch: Fetch,
    pub(super) invite_code: String,
}

// The view

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxRow {
    pub id: String,
    /// `mention`, `reply` or `dm`.
    pub kind: String,
    pub name: String,
    pub avatar: String,
    pub initials: String,
    /// "mentioned you", "replied to you", "sent a message".
    pub action: String,
    /// When it was made (epoch ms; the UI says "5m").
    pub created_ms: i64,
    /// "general · Native Spike Lab"; empty for a DM.
    pub context: String,
    pub content: String,
    pub unread: bool,
    pub channel_id: String,
    pub message_id: String,
    pub server_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxView {
    pub loaded: bool,
    pub loading: bool,
    pub error: String,
    /// Unread notifications (the bell's badge).
    pub unread: i64,
    pub rows: Vec<InboxRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionTab {
    pub emoji: String,
    pub url: String,
    pub label: String,
    pub count: i64,
    pub selected: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionPerson {
    pub user_id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub initials: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionsView {
    pub open: bool,
    pub message_id: String,
    pub can_manage: bool,
    pub tabs: Vec<ReactionTab>,
    pub people: Vec<ReactionPerson>,
    /// "and 3 others" / "2 people reacted" (empty when none).
    pub others: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaRow {
    pub id: String,
    pub message_id: String,
    pub name: String,
    /// What the viewer opens (absolute; embed videos through the proxy).
    pub url: String,
    pub poster: String,
    pub video: bool,
    pub gif: bool,
    pub author: String,
    pub avatar: String,
    pub initials: String,
    pub created_ms: i64,
    /// Links: the URLs in the message.
    pub urls: Vec<String>,
    /// Files: `1.2 MB`, `Nov 5, 2026` and the glyph.
    pub size: String,
    pub date: String,
    pub icon: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaView {
    pub channel_id: String,
    /// `media`, `links`, `files`, or empty.
    pub tab: String,
    pub loading: bool,
    pub error: String,
    pub rows: Vec<MediaRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceMediaRow {
    pub id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceStatusView {
    pub saving: bool,
    pub error: String,
    pub saved: u32,
    pub media_loading: bool,
    pub media_error: String,
    pub recents: Vec<VoiceMediaRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarRow {
    pub id: String,
    /// As stored (what the profile save sends back).
    pub avatar_url: String,
    /// Absolute, for the picture.
    pub url: String,
    pub current: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvatarsView {
    pub loading: bool,
    pub error: String,
    pub busy_id: String,
    pub rows: Vec<AvatarRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiRow {
    pub id: String,
    pub shortcode: String,
    pub url: String,
    pub status: String,
    /// What goes into the composer (`<:code:id>`), empty until ready.
    pub token: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojisView {
    pub loading: bool,
    pub error: String,
    /// Some are still generating (the picker polls every 4 s).
    pub pending: bool,
    pub generating: bool,
    pub generate_error: String,
    pub generate_success: String,
    pub rows: Vec<EmojiRow>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncView {
    pub busy: bool,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareView {
    pub message_id: String,
    pub busy: bool,
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimRow {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub initials: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub username_draft: String,
    /// `idle`, `checking`, `available`, `unavailable`, `invalid`, `error`.
    pub username_check: String,
    pub username_message: String,
    pub username_saving: bool,
    pub username_error: String,
    pub username_saved: u32,
    pub claims: Vec<ClaimRow>,
    pub claiming: bool,
    pub claim_error: String,
    pub revoking: String,
    pub revoke_error_id: String,
    pub revoke_error: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub channel_id: String,
    pub loading: bool,
    pub code: String,
    pub url: String,
    pub error: String,
}

/// Everything the extras' QML draws.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtrasView {
    pub inbox: InboxView,
    pub reactions: ReactionsView,
    pub media: MediaView,
    pub voice_status: VoiceStatusView,
    pub avatars: AvatarsView,
    pub emojis: EmojisView,
    pub sync: SyncView,
    pub share: ShareView,
    pub account: AccountView,
    pub invite: InviteView,
}

// Rules

/// NotificationRow's sentence for a kind.
pub fn notification_action(kind: &str) -> &'static str {
    match kind {
        "mention" => "mentioned you",
        "reply" => "replied to you",
        _ => "sent a message",
    }
}

/// NotificationBell.tsx `getTimeAgo`, as the row shows it: `now`, `5m`,
/// `3h`, `2d`, then `Oct 22` (with the year when it is not this one).
pub fn time_ago(created_ms: i64, now_ms: i64, offset: LocalOffset) -> String {
    let minutes = (now_ms - created_ms).div_euclid(60_000);
    if minutes < 1 {
        return "now".into();
    }
    if minutes < 60 {
        return format!("{minutes}m");
    }
    let hours = minutes / 60;
    if hours < 24 {
        return format!("{hours}h");
    }
    let days = hours / 24;
    if days < 7 {
        return format!("{days}d");
    }
    let date = local_date(created_ms, offset, false);
    let this_year = local_date(now_ms, offset, false);
    let year_of = |d: &str| d.rsplit(' ').next().map(str::to_owned);
    if year_of(&date) == year_of(&this_year) {
        date.rsplit_once(',')
            .map_or(date.clone(), |(md, _)| md.to_owned())
    } else {
        date
    }
}

/// `sanitizeGeneratedEmojiShortcode`: lower case, every run of other
/// characters one `_`, no `_` at the ends, at most 32, `emoji` when empty.
pub fn sanitize_shortcode(value: &str) -> String {
    let mut out = String::new();
    for c in value.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    let cut: String = out.trim_matches('_').chars().take(32).collect();
    let cut = cut.trim_matches('_');
    if cut.is_empty() {
        "emoji".into()
    } else {
        cut.to_owned()
    }
}

/// `buildCustomEmojiToken`: what the composer takes for an AI emoji.
pub fn custom_emoji_token(shortcode: &str, id: &str) -> String {
    format!("<:{}:{id}>", sanitize_shortcode(shortcode))
}

/// The URLs in a message (MemberList.tsx `extractUrls`).
pub fn extract_urls(text: &str) -> Vec<String> {
    let mut urls = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("http") {
        let tail = &rest[start..];
        let scheme = if tail.starts_with("https://") {
            8
        } else if tail.starts_with("http://") {
            7
        } else {
            rest = &rest[start + 4..];
            continue;
        };
        let end = tail[scheme..]
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | ')' | '"' | '\'' | ']'))
            .map_or(tail.len(), |at| at + scheme);
        if end > scheme {
            urls.push(tail[..end].to_owned());
        }
        rest = &tail[end..];
    }
    urls
}

/// `normalizeVoiceChannelStatusText`: runs of space collapse, trimmed, at
/// most 120 characters; `None` when nothing is left.
pub fn normalize_voice_status(text: &str) -> Option<String> {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        return None;
    }
    Some(collapsed.chars().take(MAX_VOICE_STATUS_TEXT).collect())
}

/// The status body a save sends (VoiceChannelTextStatusModal.tsx
/// `payload`): the text and the kept picture, or `null` for neither.
pub fn voice_status_body(text: Option<&str>, media: Option<&Value>) -> Value {
    let media = media.filter(|m| !m.is_null());
    if text.is_none() && media.is_none() {
        return Value::Null;
    }
    json!({ "text": text, "media": media })
}

/// `voiceChannelStatusMediaFromGifItem`.
pub fn gif_status_media(item: &yap_protocol::rest::GifItem) -> Value {
    let asset = if item.send.url.is_empty() {
        &item.preview
    } else {
        &item.send
    };
    json!({
        "id": item.id,
        "provider": item.provider,
        "media_type": item.media_type,
        "title": (!item.title.is_empty()).then_some(&item.title),
        "alt_text": item.alt_text,
        "source_url": Value::Null,
        "preview_url": asset.url,
        "preview_width": asset.width.round().max(1.0),
        "preview_height": asset.height.round().max(1.0),
        "preview_content_type": asset.content_type,
    })
}

/// The picture a status media shows.
pub fn status_media_url(media: &Value) -> String {
    media
        .get("preview_url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

/// `normalizeUsername` then `UsernameSchema`: lower case, 2-32 of
/// letters, digits, `_`, `.` and `-`, starting with a letter or digit.
pub fn normalize_username(text: &str) -> Option<String> {
    let name = text.trim().to_lowercase();
    let len = name.chars().count();
    let mut chars = name.chars();
    let first_ok = chars
        .next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit());
    let rest_ok = chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.-".contains(c));
    ((2..=32).contains(&len) && first_ok && rest_ok).then_some(name)
}

/// "and 3 others" under a tab's people, "2 people reacted" when the
/// server named nobody (ReactionDetailsModal.tsx).
pub fn others_line(named: usize, count: i64) -> String {
    let others = usize::try_from(count).unwrap_or(0).saturating_sub(named);
    match (others, named > 0) {
        (0, _) => String::new(),
        (1, true) => "and 1 other".into(),
        (n, true) => format!("and {n} others"),
        (1, false) => "1 person reacted".into(),
        (n, false) => format!("{n} people reacted"),
    }
}

/// `getNextReactionEmoji`: the selected emoji while it still has
/// reactions, else the first one left.
pub fn reaction_tab<'a>(emojis: &[&'a str], selected: Option<&str>) -> Option<&'a str> {
    selected
        .and_then(|s| emojis.iter().copied().find(|e| *e == s))
        .or_else(|| emojis.first().copied())
}

/// The file card's glyph (`getFileIcon`, the kinds the kit draws).
pub fn file_icon(name: &str, content_type: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let mime = content_type.to_ascii_lowercase();
    match ext.as_deref() {
        _ if mime.starts_with("image/") => "fileImage",
        _ if mime.starts_with("video/") => "fileVideo",
        _ if mime.starts_with("audio/") => "fileAudio",
        Some("zip" | "rar" | "7z" | "tar" | "gz") => "fileArchive",
        Some("pdf" | "doc" | "docx" | "txt" | "md" | "rtf") => "fileText",
        Some("js" | "ts" | "rs" | "py" | "json" | "html" | "css" | "c" | "cpp" | "go") => {
            "fileCode"
        }
        _ => "file",
    }
}

/// When an item was posted, in epoch ms (0 when unreadable).
pub(super) fn created_ms(at: &str) -> i64 {
    parse_rfc3339(at).unwrap_or(0)
}

/// The user a claim offers, as the row shows them.
pub(super) fn claim_row(candidate: &ClaimCandidate, api_base: &str) -> ClaimRow {
    let name = candidate
        .display_name
        .as_deref()
        .filter(|n| !n.trim().is_empty())
        .unwrap_or(&candidate.username);
    ClaimRow {
        id: candidate.id.clone(),
        name: name.to_owned(),
        username: candidate.username.clone(),
        avatar: super::view::absolute_url(api_base, candidate.avatar_url.as_deref()),
        initials: super::view::initials(name),
    }
}

/// A reactor as the dialog shows them; unknown people show their id.
pub(super) fn person(user_id: &str, user: Option<&User>, api_base: &str) -> ReactionPerson {
    let name = user.map_or(user_id, User::label);
    ReactionPerson {
        user_id: user_id.to_owned(),
        name: name.to_owned(),
        username: user.map(|u| u.username.clone()).unwrap_or_default(),
        avatar: super::view::absolute_url(api_base, user.and_then(|u| u.avatar_url.as_deref())),
        initials: super::view::initials(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400_000;

    #[test]
    fn time_ago_should_say_now_under_a_minute() {
        assert_eq!(time_ago(1_000, 30_000, LocalOffset::UTC), "now");
    }

    #[test]
    fn time_ago_should_count_minutes_hours_and_days() {
        let now = 10 * DAY;
        let got = [5 * 60_000, 3 * 3_600_000, 2 * DAY]
            .map(|ago| time_ago(now - ago, now, LocalOffset::UTC));
        assert_eq!(got, ["5m", "3h", "2d"]);
    }

    #[test]
    fn time_ago_should_drop_the_year_when_it_is_this_one() {
        // 2026-11-06 and 2026-10-22, both UTC.
        let now = 1_793_923_200_000;
        let then = now - 15 * DAY;
        assert_eq!(time_ago(then, now, LocalOffset::UTC), "Oct 22");
    }

    #[test]
    fn time_ago_should_keep_another_year() {
        let now = 1_793_923_200_000;
        let then = now - 400 * DAY;
        assert!(time_ago(then, now, LocalOffset::UTC).ends_with(", 2025"));
    }

    #[test]
    fn shortcodes_should_sanitize_as_the_web_does() {
        let got = [
            sanitize_shortcode("Party Parrot!!"),
            sanitize_shortcode("__"),
            sanitize_shortcode(&"a".repeat(40)),
        ];
        assert_eq!(
            got,
            [
                "party_parrot".to_owned(),
                "emoji".to_owned(),
                "a".repeat(32)
            ]
        );
    }

    #[test]
    fn a_custom_token_should_wrap_the_code_and_id() {
        assert_eq!(custom_emoji_token("Cat Hat", "e1"), "<:cat_hat:e1>");
    }

    #[test]
    fn urls_should_stop_at_spaces_and_brackets() {
        let urls = extract_urls("see https://a.example/x?y=1) and (http://b.example] ok");
        assert_eq!(urls, ["https://a.example/x?y=1", "http://b.example"]);
    }

    #[test]
    fn text_without_a_scheme_should_have_no_urls() {
        assert!(extract_urls("httpd is not a link, www.x.com either").is_empty());
    }

    #[test]
    fn voice_status_should_collapse_space_and_cap_the_length() {
        let long = format!("  a   b {}", "x".repeat(200));
        let got = normalize_voice_status(&long).unwrap_or_default();
        assert_eq!(
            (got.starts_with("a b x"), got.chars().count()),
            (true, MAX_VOICE_STATUS_TEXT)
        );
    }

    #[test]
    fn a_blank_voice_status_should_be_none() {
        assert_eq!(normalize_voice_status(" \n\t "), None);
    }

    #[test]
    fn a_status_with_neither_text_nor_picture_should_be_null() {
        assert_eq!(voice_status_body(None, None), Value::Null);
    }

    #[test]
    fn a_text_status_should_keep_the_picture() {
        let media = json!({ "preview_url": "https://x/y.gif" });
        assert_eq!(
            voice_status_body(Some("hi"), Some(&media)),
            json!({ "text": "hi", "media": media })
        );
    }

    #[test]
    fn usernames_should_lowercase_and_follow_the_schema() {
        let got = ["  Jon.Doe ", "a", "-jon", "jo n", "ok_1"].map(normalize_username);
        assert_eq!(
            got,
            [
                Some("jon.doe".into()),
                None,
                None,
                None,
                Some("ok_1".into())
            ]
        );
    }

    #[test]
    fn others_should_read_as_the_web_does() {
        let got = [
            others_line(2, 3),
            others_line(1, 4),
            others_line(0, 1),
            others_line(0, 2),
            others_line(3, 3),
        ];
        assert_eq!(
            got,
            [
                "and 1 other",
                "and 3 others",
                "1 person reacted",
                "2 people reacted",
                ""
            ]
        );
    }

    #[test]
    fn the_tab_should_move_to_the_first_when_its_emoji_goes() {
        assert_eq!(reaction_tab(&["a", "b"], Some("c")), Some("a"));
    }

    #[test]
    fn the_tab_should_stay_while_its_emoji_has_reactions() {
        assert_eq!(reaction_tab(&["a", "b"], Some("b")), Some("b"));
    }

    #[test]
    fn read_all_should_send_all() {
        let request = ExtrasRequest::ReadNotifications { ids: None };
        assert_eq!(request.body(), json!({ "all": true }));
    }

    #[test]
    fn the_status_routes_should_name_the_voice_session() {
        let request = ExtrasRequest::VoiceMedia {
            channel_id: "c".into(),
            voice_session: "p1".into(),
        };
        assert_eq!(
            request.headers(),
            vec![("X-Voice-Session-Id", "p1".to_owned())]
        );
    }

    #[test]
    fn a_channel_invite_should_last_seven_days() {
        let request = ExtrasRequest::ChannelInvite {
            server_id: "s".into(),
            channel_id: "c".into(),
        };
        assert_eq!(
            request.body(),
            json!({ "channel_id": "c", "max_age": 604_800 })
        );
    }

    #[test]
    fn a_claimed_account_should_offer_no_candidates() {
        let body = json!({ "claimed": true, "candidates": [{ "id": "x", "username": "y" }] });
        let got = ExtrasRequest::Claims.decode(body);
        assert_eq!(got, Ok(ExtrasResponse::Claims(Vec::new())));
    }

    #[test]
    fn a_share_should_decode_its_link() {
        let body = json!({ "share": {}, "share_url": "https://yap.example/share/t" });
        let request = ExtrasRequest::Share {
            message_id: "m".into(),
            expires: "30d".into(),
        };
        assert_eq!(
            request.decode(body),
            Ok(ExtrasResponse::Shared("https://yap.example/share/t".into()))
        );
    }

    #[test]
    fn file_icons_should_follow_the_type_then_the_extension() {
        let got = [
            file_icon("a.zip", ""),
            file_icon("a.bin", "video/mp4"),
            file_icon("a.rs", ""),
            file_icon("a", ""),
        ];
        assert_eq!(got, ["fileArchive", "fileVideo", "fileCode", "file"]);
    }

    #[test]
    fn a_gif_should_become_status_media_with_its_send_asset() {
        let item = yap_protocol::rest::GifItem {
            id: "g".into(),
            provider: "klipy".into(),
            send: yap_protocol::rest::GifAsset {
                url: "https://cdn/x.gif".into(),
                width: 100.4,
                height: 0.2,
                size_bytes: 1,
                content_type: "image/gif".into(),
            },
            ..Default::default()
        };
        let media = gif_status_media(&item);
        assert_eq!(
            (
                media["preview_url"].clone(),
                media["preview_width"].clone(),
                media["preview_height"].clone()
            ),
            (json!("https://cdn/x.gif"), json!(100.0), json!(1.0))
        );
    }
}
