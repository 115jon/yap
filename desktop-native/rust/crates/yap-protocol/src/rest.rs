//! REST bodies the native client reads or sends. Responses are bare JSON (no
//! envelope). Loose like the payloads: unknown fields are ignored and only
//! fields the client cannot do without are required.

use serde::de::Deserializer;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// Socket tickets, call admission and media bootstrap

/// Which socket a `POST /api/voice/socket-ticket` ticket is for. Only the
/// chat gateway asks there now (`global`); the call sockets get theirs from
/// [`VoiceAdmissionRequest`] (`realtime/ADMISSION.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TicketAudience {
    Global,
}

/// Body of `POST /api/voice/socket-ticket` (the chat gateway's ticket).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SocketTicketRequest {
    pub audience: TicketAudience,
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

/// The participant's media credentials: an admission's `media` (the same
/// object the deprecated `media_bootstrap` was).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MediaBootstrap {
    pub participant_id: String,
    pub voice_token: String,
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
}

/// A call socket an admission issues a ticket for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CallSocket {
    /// `MeetingRoom`, `/api/channels/:slug/ws`.
    Room,
    /// `VoiceRoom`, `/api/channels/:slug/voice`.
    Voice,
}

/// Body of `POST /api/voice/admission` (`realtime/ADMISSION.md` "Request"):
/// the call sockets' tickets and, with `media`, the voice token and TURN,
/// from one Kova check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VoiceAdmissionRequest {
    pub channel_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    /// The server derives it for server channels; sent as the web sends it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room_slug: Option<String>,
    /// Which tickets to issue: non-empty, no repeats.
    pub sockets: Vec<CallSocket>,
    /// Mint the voice token and TURN for the room ticket's participant.
    /// Only with [`CallSocket::Room`].
    pub media: bool,
    /// Only when `media` is false: the participant the client already is,
    /// which binds the voice ticket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    /// The grant a `VoiceMove` frame carried: stands in for CONNECT in the
    /// channel it names. Only on the join after a move.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_grant: Option<String>,
}

impl VoiceAdmissionRequest {
    /// `server_id` empty: a DM call (`dm-call-<a>-<b>`), which names no server.
    fn new(server_id: &str, channel_id: &str, room_slug: &str, sockets: Vec<CallSocket>) -> Self {
        Self {
            channel_id: channel_id.to_owned(),
            server_id: (!server_id.is_empty()).then(|| server_id.to_owned()),
            room_slug: Some(room_slug.to_owned()),
            sockets,
            media: false,
            participant_id: None,
            move_grant: None,
        }
    }

    /// The same request carrying a move grant (none when empty).
    #[must_use]
    pub fn with_move_grant(mut self, grant: Option<&str>) -> Self {
        self.move_grant = grant.filter(|g| !g.is_empty()).map(str::to_owned);
        self
    }

    /// A join (Client flow 1): both tickets and the media credentials.
    pub fn join(server_id: &str, channel_id: &str, room_slug: &str) -> Self {
        Self {
            media: true,
            ..Self::new(
                server_id,
                channel_id,
                room_slug,
                vec![CallSocket::Room, CallSocket::Voice],
            )
        }
    }

    /// A room socket reconnect (Client flow 3): the room ticket alone,
    /// minting nothing.
    pub fn room_reconnect(server_id: &str, channel_id: &str, room_slug: &str) -> Self {
        Self::new(server_id, channel_id, room_slug, vec![CallSocket::Room])
    }

    /// A voice socket reconnect (Client flow 4): the voice ticket alone,
    /// bound to `participant_id`.
    pub fn voice_reconnect(
        server_id: &str,
        channel_id: &str,
        room_slug: &str,
        participant_id: &str,
    ) -> Self {
        Self {
            participant_id: Some(participant_id.to_owned()),
            ..Self::new(server_id, channel_id, room_slug, vec![CallSocket::Voice])
        }
    }
}

/// Response of `POST /api/voice/admission`. Each ticket is present when its
/// socket was asked for, is single use, and expires at `expires_at` (issue
/// time plus 60 s). Tickets are opaque: the client never reads their claims.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceAdmission {
    pub access_mode: Option<String>,
    pub room_slug: Option<String>,
    /// Epoch milliseconds.
    pub expires_at: Option<i64>,
    /// With `media`, the room ticket's nonce (the id the room's Ready
    /// carries); otherwise the `participantId` sent, if any.
    pub participant_id: Option<String>,
    pub room_ticket: Option<String>,
    pub voice_ticket: Option<String>,
    /// Present when `media` was asked for.
    pub media: Option<MediaBootstrap>,
}

impl VoiceAdmission {
    /// The ticket issued for `socket`, if it was asked for.
    pub fn ticket(&self, socket: CallSocket) -> Option<&str> {
        match socket {
            CallSocket::Room => self.room_ticket.as_deref(),
            CallSocket::Voice => self.voice_ticket.as_deref(),
        }
        .filter(|ticket| !ticket.is_empty())
    }
}

/// Response of `POST /api/voice/socket-ticket` (the chat gateway's ticket).
/// Single use with a 60 s TTL.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SocketTicket {
    pub ticket: String,
    pub access_mode: Option<String>,
    pub audience: Option<String>,
    /// Epoch milliseconds.
    pub expires_at: Option<i64>,
    pub room_slug: Option<String>,
}

// Users, servers, channels

/// A user (`GET /api/users/me` returns one for the signed-in account).
/// Member lists, DM recipients and `GET /api/users/:id/profile` carry the
/// profile fields too; elsewhere they are absent.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct User {
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub avatar_display: Option<Value>,
    pub status: Option<String>,
    pub custom_status: Option<String>,
    /// The display name's font, effect and colours: a JSON object, or the
    /// same object as a JSON string (profile-customization.ts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name_style: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bio: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pronouns: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_url: Option<String>,
    /// `#rrggbb`, when the user picked a banner colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_banner_color: Option<String>,
    /// The profile card's accent and background (`#RRGGBB`). The server
    /// stores its default pair for people who never picked colours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_accent_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_background_color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banner_content_type: Option<String>,
    /// The synced theme (`dark`, `miku-light`, …, `system`), if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_preference: Option<String>,
    /// `true`/`false` or `1`/`0`, as the endpoints send it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_sync_enabled: Option<Value>,
    /// `high`, `medium`, `low` or `off` (the GIF search filter).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_content_filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// The synced join and leave sounds and the soundboard volume
    /// (`GET /api/users/me` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound_settings: Option<Value>,
    /// The equipped nameplate: its animated webm, else its still PNG
    /// (collectibles/apply.ts), with the matching content type.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameplate_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameplate_content_type: Option<String>,
    /// Where they are signed in (`desktop`, `mobile`, `web`), as presence
    /// last said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_platforms: Option<Vec<String>>,
}

/// The signed-in user.
pub type Me = User;

impl User {
    /// Theme sync is on (`theme_sync_enabled` is `true` or `1`).
    pub fn theme_sync(&self) -> bool {
        match &self.theme_sync_enabled {
            Some(Value::Bool(on)) => *on,
            Some(Value::Number(n)) => n.as_i64() == Some(1),
            _ => false,
        }
    }

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
    /// Every invite link is off until a manager resumes them.
    #[serde(
        default,
        deserialize_with = "opt_bool_or_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub invites_paused: Option<bool>,
    /// The public message share switches (Server settings → Overview).
    #[serde(
        default,
        deserialize_with = "opt_bool_or_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_public_shares: Option<bool>,
    #[serde(
        default,
        deserialize_with = "opt_bool_or_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub show_source_in_shares: Option<bool>,
    #[serde(
        default,
        deserialize_with = "opt_bool_or_int",
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_share_indexing: Option<bool>,
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
    /// The channel's public share override; `None` follows the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_public_shares: Option<bool>,
    /// A voice channel's status (text and a picture), set by someone in
    /// the call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_status: Option<VoiceStatus>,
}

/// A voice channel's status (`voice_status`, lib/voice-channel-status.ts).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct VoiceStatus {
    #[serde(default)]
    pub text: Option<String>,
    /// The picture as the server stored it (`VoiceChannelStatusMedia`),
    /// sent back untouched when only the text changes.
    #[serde(default)]
    pub media: Option<Value>,
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

/// A message attachment. `url` is relative (`/api/attachments/...`, needs
/// the session) for uploads and absolute for GIF-picker picks.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Attachment {
    pub id: Option<String>,
    #[serde(default)]
    pub filename: String,
    pub content_type: Option<String>,
    pub url: Option<String>,
    pub size_bytes: Option<u64>,
    /// Used when `url` is absent (attachment-url.ts `getAttachmentUrl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_key: Option<String>,
    /// Marked sensitive by the sender (blurred under the high filter).
    #[serde(default, deserialize_with = "bool_or_int")]
    pub is_nsfw: bool,
}

/// One emoji's reactions on a message. `emoji` is the native character
/// (skin tone applied) or a custom `<:name:id>` token.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Reaction {
    pub emoji: String,
    #[serde(default)]
    pub count: i64,
    #[serde(default)]
    pub me: bool,
    /// Who reacted; the web highlights a chip when this has the viewer.
    #[serde(default)]
    pub users: Vec<String>,
}

/// An embed's author (`EmbedAuthor`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedAuthor {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(rename = "iconURL", default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(
        rename = "isVerified",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub is_verified: Option<bool>,
}

/// An embed's site (`EmbedProvider`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedProvider {
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// An embed picture with its size when the server read it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedImage {
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

/// One picture or clip of an embed (`EmbedMedia`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedMedia {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default)]
    pub is_gif: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_seconds: Option<f64>,
}

/// An embed's player or file (`EmbedVideo`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedVideo {
    #[serde(default)]
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// `direct` (a file) or `player` (an iframe page).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(
        rename = "contentType",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub content_type: Option<String>,
    #[serde(
        rename = "durationSeconds",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub duration_seconds: Option<f64>,
}

/// Engagement counts of a social post (`EmbedMetrics`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedMetrics {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replies: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retweets: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub likes: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impressions: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comments: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub views: Option<f64>,
}

/// A post's sound (`EmbedAudio`): TikTok's music, Instagram's audio.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedAudio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
}

/// The link card an X post carries (`EmbedExternalCard`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedExternalCard {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
}

/// The post an X post quotes or reposts.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferencedTweet {
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub author: Option<EmbedAuthor>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient_list"
    )]
    pub media: Vec<EmbedMedia>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

/// An embed's footer line.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct EmbedFooter {
    #[serde(default)]
    pub text: String,
    #[serde(rename = "iconURL", default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

/// A link preview the server unfurled (`EmbedInfo`, src/lib/types.ts). Only
/// what the cards draw is kept; the rest is ignored.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Embed {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub url: String,
    /// `link`, `image`, `video` or `rich`.
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub unavailable: bool,
    #[serde(default)]
    pub restricted: bool,
    /// The link is the media itself: drawn bare, without a card.
    #[serde(default)]
    pub direct: bool,
    /// `large` (under the text) or `thumbnail` (beside it); absent is large.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_description: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub author: Option<EmbedAuthor>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub provider: Option<EmbedProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub thumbnail: Option<EmbedImage>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "lenient_list"
    )]
    pub media: Vec<EmbedMedia>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub video: Option<EmbedVideo>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub footer: Option<EmbedFooter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub metrics: Option<EmbedMetrics>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub audio: Option<EmbedAudio>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub external_card: Option<EmbedExternalCard>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "lenient"
    )]
    pub referenced_tweet: Option<ReferencedTweet>,
}

/// `POST /api/channels/:id/messages/upload` and `.../messages/gif` (201):
/// the attachment, not yet linked to a message.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct UploadedFile {
    pub id: String,
    #[serde(default)]
    pub file_url: String,
    #[serde(default)]
    pub file_name: String,
    #[serde(default)]
    pub file_size: u64,
    pub content_type: Option<String>,
}

/// One rendition of a picker GIF (`GifPickerAsset`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifAsset {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub width: f64,
    #[serde(default)]
    pub height: f64,
    #[serde(default)]
    pub size_bytes: u64,
    #[serde(default)]
    pub content_type: String,
}

/// One GIF picker result (`GifPickerItem`): `preview` for the grid, `send`
/// for the message.
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GifItem {
    pub id: String,
    #[serde(default)]
    pub title: String,
    /// `klipy`, `tenor` or `external`.
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub alt_text: Option<String>,
    #[serde(default)]
    pub preview: GifAsset,
    #[serde(default)]
    pub send: GifAsset,
    #[serde(default)]
    pub media_type: Option<String>,
}

/// `GET /api/gifs?mode=search…`: a page and the cursor of the next one.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct GifPage {
    #[serde(default, deserialize_with = "lenient_list")]
    pub results: Vec<GifItem>,
    pub next: Option<String>,
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

/// A chat message. Reactions and embeds that do not decode are skipped.
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
    #[serde(default, deserialize_with = "lenient_list")]
    pub reactions: Vec<Reaction>,
    #[serde(default, deserialize_with = "lenient_list")]
    pub embeds: Vec<Embed>,
    pub nonce: Option<String>,
    pub content_revision: Option<i64>,
    /// `"call"` for a DM call record.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Messages that reply to this one, its thread (message.service.ts
    /// `formatMessageRow`; 0 when absent).
    #[serde(default, deserialize_with = "count")]
    pub reply_count: i64,
}

/// A count that may be absent or `null`.
fn count<'de, D: Deserializer<'de>>(deserializer: D) -> Result<i64, D::Error> {
    Ok(Option::<i64>::deserialize(deserializer)?.unwrap_or(0))
}

/// A thread starter in `GET /api/channels/{id}/threads` (`{threads}`,
/// message.service.ts `fetchChannelThreads`): a message with replies, its
/// content cut to 200 characters, newest reply first.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ThreadSummary {
    pub id: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub author: User,
    #[serde(default, deserialize_with = "count")]
    pub reply_count: i64,
    #[serde(default)]
    pub last_reply_at: String,
    #[serde(default)]
    pub created_at: String,
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
    /// `Some([])` clears them; `None` leaves them.
    #[serde(default, deserialize_with = "lenient_some_list")]
    pub embeds: Option<Vec<Embed>>,
    pub content_revision: Option<i64>,
}

/// Like [`lenient_list`], but a present field is `Some` (even when empty).
fn lenient_some_list<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Array(items) => Some(
            items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
        ),
        _ => None,
    })
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
    /// Who mentioned, replied or wrote (the toast's title).
    #[serde(default)]
    pub from_user: Option<NotificationUser>,
    /// The message text (the toast's body).
    #[serde(default)]
    pub content: Option<String>,
    /// Denormalized for display (the toast's summary).
    #[serde(default)]
    pub server_name: Option<String>,
    #[serde(default)]
    pub channel_name: Option<String>,
    /// When it was made (the inbox's "5m").
    #[serde(default)]
    pub created_at: Option<String>,
}

/// The author a notification carries.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct NotificationUser {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

impl Notification {
    /// Counts toward a channel's and a server's mention badge.
    pub fn is_mention(&self) -> bool {
        !self.is_read && matches!(self.kind.as_str(), "mention" | "reply")
    }
}

/// A server role. Member lists send `is_default` as a bool, some
/// dispatches as SQLite's 0 or 1.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct Role {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// `#rrggbb`; absent or empty when the role has no colour.
    pub color: Option<String>,
    #[serde(default)]
    pub position: i64,
    #[serde(default, deserialize_with = "bool_or_int")]
    pub is_default: bool,
    /// The bitmask (src/lib/permissions.ts); 0 when a payload leaves it out.
    #[serde(default)]
    pub permissions: i64,
}

/// `true`/`false`, SQLite's 0/1, or absent/`null` (`None`).
fn opt_bool_or_int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<bool>, D::Error> {
    Ok(match Option::<Value>::deserialize(deserializer)? {
        Some(Value::Bool(value)) => Some(value),
        Some(Value::Number(n)) => Some(n.as_i64().is_some_and(|n| n != 0)),
        _ => None,
    })
}

fn bool_or_int<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Bool(value) => value,
        Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
        _ => false,
    })
}

/// Decodes each list element that fits and skips the rest.
fn lenient_list<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    Ok(match Value::deserialize(deserializer)? {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    })
}

/// [`lenient_list`] for sibling modules.
pub(crate) fn lenient_vec<'de, D: Deserializer<'de>, T: serde::de::DeserializeOwned>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    lenient_list(deserializer)
}

/// One server member (`GET /api/servers/:id/members` is a bare array).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Member {
    pub user: User,
    /// Highest position first, as the server sorts them.
    #[serde(default, deserialize_with = "lenient_list")]
    pub roles: Vec<Role>,
}

impl Member {
    /// The role with the highest position (MemberList.tsx `getHighestRole`).
    pub fn highest_role(&self) -> Option<&Role> {
        self.roles.iter().reduce(|best, role| {
            if role.position > best.position {
                role
            } else {
                best
            }
        })
    }
}

/// A direct message channel (`GET /api/dms`, `POST /api/dms`,
/// `DM_CHANNEL_CREATE`): the other person is `recipient`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct DmChannel {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub created_at: Option<String>,
    pub recipient: User,
}

/// One relationship (`GET /api/friends`, `RELATIONSHIP_ADD`;
/// social.service.ts `listRelationships`): the other person and what they
/// are to the signed-in user.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Relationship {
    pub user: User,
    /// 0 friend, 1 blocked (by us), 2 incoming request, 3 outgoing request.
    #[serde(rename = "type")]
    pub kind: i64,
}

impl Relationship {
    pub const FRIEND: i64 = 0;
    pub const BLOCKED: i64 = 1;
    pub const INCOMING: i64 = 2;
    pub const OUTGOING: i64 = 3;
}

/// `POST /api/friends`: `{user, type}`; `type` 0 when it accepted their
/// request (friends.ts:63), 3 when it sent one.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FriendRequestSent {
    #[serde(rename = "type", default)]
    pub kind: Option<i64>,
}

/// `GET /api/users/:id/profile`: the full user and the servers you share
/// (user.service.ts `getUserProfileMutuals`; mutual friends are not read).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ProfileResponse {
    pub user: User,
    #[serde(default, rename = "mutualServers")]
    pub mutual_servers: MutualServers,
}

/// `mutualServers`: how many, and the first few.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct MutualServers {
    pub count: i64,
    pub items: Vec<MutualServer>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct MutualServer {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
}

// Server administration (server settings, channel settings)

/// One invite link (`GET /api/servers/:id/invites`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Invite {
    pub code: String,
    pub channel_id: Option<String>,
    pub channel_name: Option<String>,
    #[serde(default)]
    pub inviter_id: String,
    pub inviter: Option<User>,
    /// `None` or 0: no limit.
    pub max_uses: Option<i64>,
    #[serde(default)]
    pub uses: i64,
    #[serde(default, deserialize_with = "bool_or_int")]
    pub temporary: bool,
    pub expires_at: Option<String>,
    pub created_at: Option<String>,
}

/// What `POST /api/servers/:id/invites` answers.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct CreatedInvite {
    pub code: String,
}

/// `GET /api/invites/:code` (social.service.ts `getInviteInfo`,
/// InviteClient.tsx `InvitePreview`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct InvitePreview {
    #[serde(default)]
    pub code: String,
    pub server: InviteServer,
    #[serde(default)]
    pub inviter: InviteInviter,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct InviteServer {
    pub id: String,
    pub name: String,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub member_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct InviteInviter {
    #[serde(default)]
    pub username: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}

/// `POST /api/invites/:code/join`: `{joined: true, server}` or
/// `{already_member: true, server}`; `server` is the stored row.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct JoinedInvite {
    #[serde(default)]
    pub joined: bool,
    #[serde(default)]
    pub already_member: bool,
    /// `None` when missing or not a server row.
    #[serde(default, deserialize_with = "lenient_server")]
    pub server: Option<Server>,
}

fn lenient_server<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Server>, D::Error> {
    Ok(Option::<Value>::deserialize(deserializer)?.and_then(|v| serde_json::from_value(v).ok()))
}

/// One ban (`GET /api/servers/:id/bans`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct Ban {
    pub user_id: String,
    pub username: Option<String>,
    pub avatar_url: Option<String>,
    pub reason: Option<String>,
    pub banned_by_username: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

/// One audit log entry (`GET /api/servers/:id/audit-logs`, newest first).
/// `created_at` may be SQLite's `YYYY-MM-DD HH:MM:SS` (UTC).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, Serialize)]
pub struct AuditLog {
    pub id: String,
    #[serde(default)]
    pub actor_id: String,
    pub actor: Option<User>,
    #[serde(default)]
    pub action_type: String,
    pub target_id: Option<String>,
    pub changes: Option<Value>,
    pub reason: Option<String>,
    #[serde(default)]
    pub created_at: String,
}

/// A channel permission override (`GET /api/channels/:id/permissions`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct PermissionOverride {
    pub target_id: String,
    /// `role` or `user`.
    pub target_type: String,
    #[serde(default)]
    pub allow: i64,
    #[serde(default)]
    pub deny: i64,
}

// Settings

/// One row of `GET /api/sessions` (`{ sessions: [...] }`). The server only
/// knows the session making the call today, but the shape allows more.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    /// Epoch ms, or an ISO string.
    pub last_active_at: Option<Value>,
    pub is_current: bool,
    pub activity: Option<SessionActivity>,
}

/// What the session's client reported about itself, when anything.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionActivity {
    pub is_mobile: bool,
    pub browser_name: Option<String>,
    pub device_type: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
}

/// One shared message link from `GET /api/shares` (`{ shares: [...] }`).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ShareItem {
    pub id: String,
    pub token: String,
    pub content: String,
    pub author: ShareAuthor,
    pub created_at: String,
    pub expires_at: Option<String>,
    pub revoked_at: Option<String>,
    /// `active`, `revoked`, `deleted` or `expired`.
    pub status: String,
    pub view_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct ShareAuthor {
    pub username: String,
    pub display_name: Option<String>,
}

/// Kova's `GET /api/auth/get-session`: only the user's email is read.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct KovaSession {
    pub user: KovaUser,
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct KovaUser {
    pub email: Option<String>,
}

/// One row of Kova's `GET /api/auth/list-accounts`: a linked provider
/// (`credential` for email and password).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KovaAccount {
    pub provider_id: String,
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
