//! Typed `d` payloads for the opcodes the native client sends or reads.
//!
//! Mirrors `packages/protocol/src/{client,server}-payloads.ts` and
//! `desktop-native/VOICE_PROTOCOL.md`. Like the TypeScript schemas these are
//! deliberately loose: unknown fields are ignored (never `deny_unknown_fields`)
//! and a field is required only when the client reads it without a fallback.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::game_activity::GameActivity;
use crate::opcode::GatewayCloseCode;
use crate::rest::IceServer;

// Both gateways

/// Hello (op 8): first frame after connect.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Hello {
    pub heartbeat_interval: Option<u64>,
    pub gateway_version: Option<Value>,
}

/// Error (op 18).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GatewayError {
    pub code: i64,
    pub message: String,
    /// Native screen-share errors only.
    pub request_id: Option<String>,
    pub operation: Option<String>,
    pub push_prefix: Option<String>,
}

impl GatewayError {
    /// The code as a known [`GatewayCloseCode`], if it is one.
    pub fn close_code(&self) -> Option<GatewayCloseCode> {
        GatewayCloseCode::try_from(self.code).ok()
    }
}

// Main gateway (chat socket and room socket)

/// Identify (op 0). One struct for both sockets: the global gateway sends
/// `clerk_user_id`, the room socket sends `username` and friends. Only `name`
/// is required. Do not set `standby` on a room socket that must get the voice
/// join grant.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Identify {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clerk_user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    /// A string or an avatar display object; the room stores what it gets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_display: Option<Value>,
    /// Unknown values fall back to `"web"` on the server.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_voice_state_deltas: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standby: Option<bool>,
}

/// Resume (op 7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resume {
    pub session_id: String,
    pub seq_ack: u64,
}

/// Ready (op 2) on the global (chat) gateway: only the session id is read.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChatReady {
    pub participant_id: Option<String>,
}

/// Dispatch (op 19): `event` routes `data`; `seq` is the replay cursor.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Dispatch {
    pub event: String,
    #[serde(default)]
    pub data: Value,
    pub seq: Option<u64>,
}

/// `MESSAGE_DELETE` dispatch data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MessageDeleteEvent {
    pub id: String,
    pub channel_id: Option<String>,
}

/// `MESSAGE_UNPIN` dispatch data (message.service.ts `unpinMessage`:
/// `{id, channel_id, is_pinned: false}`). `MESSAGE_PIN` carries the whole
/// message instead (pins.ts).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MessageUnpinEvent {
    pub id: String,
    pub channel_id: Option<String>,
}

/// `REACTION_ADD` / `REACTION_REMOVE` dispatch data (message.service.ts).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReactionEvent {
    pub message_id: String,
    pub channel_id: Option<String>,
    pub user_id: String,
    pub emoji: String,
}

/// `TYPING_START` dispatch data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TypingStartEvent {
    pub channel_id: String,
    pub user_id: String,
}

/// `NOTIFICATIONS_READ` dispatch data: these ids, or every one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct NotificationsRead {
    #[serde(default)]
    pub ids: Option<Vec<String>>,
    #[serde(default)]
    pub all: bool,
}

/// Subscribe (op 27) or unsubscribe (op 28) to a channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelRef {
    pub channel_id: String,
}

/// PresenceUpdate (op 26), client to server: the user's chosen status
/// (`online`, `idle`, `dnd`, or `offline` for invisible).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresenceStatus {
    pub status: String,
}

/// A field that may be absent (unchanged), `null` (cleared) or a value.
fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `PRESENCE_UPDATE` dispatch data. `custom_status` comes only with the
/// REST status change: absent leaves the known one, `null` clears it.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PresenceUpdateEvent {
    pub user_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, deserialize_with = "present")]
    pub custom_status: Option<Option<String>>,
    /// Where they are signed in now; absent leaves the known ones.
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
    /// What they are playing, as this viewer may see it: absent leaves it
    /// unchanged, `null` (or an invalid value) clears it. Boxed: most
    /// updates carry none, and the event rides in every dispatch enum.
    #[serde(default, deserialize_with = "crate::game_activity::present_activity")]
    pub game_activity: Option<Option<Box<GameActivity>>>,
}

/// One connected user in a [`PresenceList`].
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PresenceEntry {
    pub user_id: String,
    pub status: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub custom_status: Option<Option<String>>,
    #[serde(default)]
    pub platforms: Option<Vec<String>>,
    /// Absent from servers that predate game activity; `null` when there
    /// is none to show this viewer.
    #[serde(default, deserialize_with = "crate::game_activity::present_activity")]
    pub game_activity: Option<Option<GameActivity>>,
}

/// `PRESENCE_LIST` dispatch data (after every channel subscribe): everyone
/// connected. Older servers send only `user_ids`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct PresenceList {
    pub users: Option<Vec<PresenceEntry>>,
    #[serde(default)]
    pub user_ids: Vec<String>,
}

/// `GUILD_MEMBER_ADD` dispatch data.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GuildMemberAdd {
    pub server_id: String,
    pub user: crate::rest::User,
    #[serde(default)]
    pub roles: Vec<crate::rest::Role>,
}

/// `GUILD_MEMBER_REMOVE` dispatch data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GuildMemberRemove {
    pub server_id: String,
    pub user_id: String,
}

/// `RELATIONSHIP_REMOVE` dispatch data (social.service.ts
/// `removeRelationship`, `blockUser`): the other person is gone from the list.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RelationshipRemove {
    pub user_id: String,
}

/// `GUILD_MEMBER_UPDATE` dispatch data: the member's roles now.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GuildMemberUpdate {
    pub server_id: String,
    pub user_id: String,
    pub roles: Option<Vec<crate::rest::Role>>,
}

/// `CHANNEL_UPDATE` dispatch data: the channel as stored (without the
/// viewer's `permissions`), or only the server (and sometimes the channel
/// id) when the tree or an override changed and the list must be fetched.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ChannelUpdateEvent {
    pub server_id: Option<String>,
    pub channel: Option<crate::rest::Channel>,
    pub id: Option<String>,
}

/// `CHANNEL_DELETE` dispatch data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChannelDeleteEvent {
    pub id: String,
    pub server_id: Option<String>,
}

/// `GUILD_DELETE` dispatch data.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct GuildDelete {
    pub id: String,
}

/// `USER_PROFILE_UPDATE` dispatch data: the fields present changed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct UserProfileUpdate {
    pub user_id: String,
    pub username: Option<String>,
    #[serde(default, deserialize_with = "present")]
    pub display_name: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub avatar_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub banner_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub profile_banner_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub bio: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub pronouns: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub profile_accent_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub profile_background_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub banner_content_type: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub theme_preference: Option<Option<String>>,
    /// Sent to every member as `true`/`false`.
    pub theme_sync_enabled: Option<Value>,
    /// Sent only to the user's own sessions.
    #[serde(default, deserialize_with = "present")]
    pub media_content_filter: Option<Option<String>>,
    /// "Share my activity", sent only to the user's own sessions.
    #[serde(default)]
    pub share_activity: Option<bool>,
    /// The serialized avatar display (crop and collectibles), as stored.
    #[serde(default, deserialize_with = "present")]
    pub avatar_display: Option<Option<Value>>,
    #[serde(default, deserialize_with = "present")]
    pub nameplate_url: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub nameplate_content_type: Option<Option<String>>,
}

impl UserProfileUpdate {
    /// Applies the fields this update carries to `user`.
    pub fn apply(&self, user: &mut crate::rest::User) {
        if let Some(username) = &self.username {
            user.username.clone_from(username);
        }
        let fields = [
            (&self.display_name, &mut user.display_name),
            (&self.avatar_url, &mut user.avatar_url),
            (&self.banner_url, &mut user.banner_url),
            (&self.profile_banner_color, &mut user.profile_banner_color),
            (&self.bio, &mut user.bio),
            (&self.pronouns, &mut user.pronouns),
            (&self.profile_accent_color, &mut user.profile_accent_color),
            (
                &self.profile_background_color,
                &mut user.profile_background_color,
            ),
            (&self.banner_content_type, &mut user.banner_content_type),
            (&self.theme_preference, &mut user.theme_preference),
            (&self.media_content_filter, &mut user.media_content_filter),
            (&self.nameplate_url, &mut user.nameplate_url),
            (
                &self.nameplate_content_type,
                &mut user.nameplate_content_type,
            ),
        ];
        for (update, field) in fields {
            if let Some(value) = update {
                field.clone_from(value);
            }
        }
        if let Some(sync) = &self.theme_sync_enabled {
            user.theme_sync_enabled = Some(sync.clone());
        }
        if let Some(share) = self.share_activity {
            user.share_activity = Some(share);
        }
        if let Some(display) = &self.avatar_display {
            user.avatar_display.clone_from(display);
        }
    }
}

/// ServerSubscribe (op 35).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerSubscribe {
    pub server_id: String,
}

/// VoiceChannelJoin (op 33).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceChannelJoin {
    pub channel_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_mute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<Value>,
    /// A voice move grant: stands in for CONNECT on the channel it names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub move_grant: Option<String>,
}

/// VoiceChannelLeave (op 34). Without a channel it applies to the current one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct VoiceChannelLeave {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel_id: Option<String>,
}

/// Someone in a server's voice channel, as the sidebar shows it
/// (`VOICE_CHANNEL_STATES`, `VOICE_CHANNEL_STATE_UPDATE`). With
/// `supports_voice_state_deltas` an update leaves the profile fields out;
/// keep them from the last full member with the same `user_id`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct VoiceChannelMember {
    #[serde(default)]
    pub user_id: String,
    pub name: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub self_mute: Option<bool>,
    pub self_deaf: Option<bool>,
    pub self_video: Option<bool>,
    pub self_stream: Option<bool>,
    /// The still of their live share (a `data:` URL their client refreshes
    /// every 8 s while sharing), shown behind an unwatched share's Watch
    /// prompt. Not a profile field: deltas carry it too.
    #[serde(default)]
    pub stream_preview_url: Option<String>,
    /// When they joined (ms), for the channel's call clock.
    #[serde(default)]
    pub joined_at: Option<Value>,
    /// `reconnecting` while their voice session is coming back; with
    /// `connected: false` the row dims (voice-presence.ts).
    #[serde(default)]
    pub connection_state: Option<String>,
    #[serde(default)]
    pub connected: Option<bool>,
}

/// `VOICE_CHANNEL_STATES` dispatch data: every visible voice channel that
/// has someone in it, sent after a subscribe, and when each call started
/// (ms, for the sidebar's clock).
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct VoiceChannelStates {
    #[serde(default)]
    pub voice_states: BTreeMap<String, Vec<VoiceChannelMember>>,
    #[serde(default)]
    pub voice_started_at: BTreeMap<String, Value>,
}

/// `VOICE_CHANNEL_STATE_UPDATE` dispatch data: one channel's whole roster
/// (empty when the last person left).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoiceChannelStateUpdate {
    pub channel_id: String,
    #[serde(default)]
    pub members: Vec<VoiceChannelMember>,
    #[serde(default)]
    pub started_at: Option<Value>,
}

// Room socket: roster and voice state

/// A room participant. Join and leave carry the full state; an update may be
/// a delta without the profile fields, so only `id` is required.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoiceState {
    pub id: String,
    pub user_id: Option<String>,
    pub name: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub self_mute: Option<bool>,
    pub self_deaf: Option<bool>,
    pub self_video: Option<bool>,
    pub self_stream: Option<bool>,
}

/// What a [`VoiceStateUpdate`] did to the participant. Anything this build
/// does not know is [`VoiceAction::Unknown`] and should be treated as an update.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VoiceAction {
    Join,
    Leave,
    Update,
    #[serde(other)]
    Unknown,
}

/// VoiceStateUpdate (op 15), server to client.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoiceStateUpdate {
    pub participant: VoiceState,
    pub action: VoiceAction,
}

/// VoiceStateUpdate (op 15), client to server: the sender's own flags.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientVoiceState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_mute: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_deaf: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_video: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_stream: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub self_stream_audio: Option<bool>,
}

/// VoiceStateUpdate (op 15) on the global gateway: our live share's still
/// for the sidebar roster (chat-gateway.ts `sendVoiceStateUpdate({
/// stream_preview_url })`); `None` sends `null`, which clears it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StreamPreviewUpdate {
    pub stream_preview_url: Option<String>,
}

/// MediaReady (op 41): media credentials before the full Ready.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MediaReady {
    pub participant_id: String,
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
    pub voice_token: String,
}

/// Ready (op 2) on a room socket. On a room ticket whose admission minted
/// the credentials (`media: "issued"`) it carries no `voice_token` and no
/// `ice_servers`; an Identify on a `held` ticket carries both.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RoomReady {
    pub participant_id: String,
    #[serde(default)]
    pub ice_servers: Vec<IceServer>,
    pub voice_token: Option<String>,
    /// The roster, excluding the receiver.
    #[serde(default)]
    pub participants: Vec<VoiceState>,
    pub heartbeat_interval: Option<u64>,
    pub spatial_audio_state: Option<Value>,
}

/// Resumed (op 9) on a room socket. The reply to a credentials refresh
/// (op 40) carries a fresh `voice_token` and `ice_servers`; the reply to a
/// Resume (op 7) on an admission ticket carries neither (keep the held ones).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Resumed {
    pub voice_token: Option<String>,
    pub ice_servers: Option<Vec<IceServer>>,
    pub participants: Option<Vec<VoiceState>>,
}

// Voice socket

/// VoiceIdentify (op 100).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VoiceIdentify {
    pub participant_id: String,
    pub voice_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports_listen_together_snapshot_events: Option<bool>,
}

/// Kind of a published track. Anything else decodes as [`TrackKind::Unknown`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Audio,
    Video,
    #[serde(other)]
    Unknown,
}

/// A published track as the voice room describes it (Video, op 12, and
/// VoiceReady `tracks`). Skip tracks without `sfu_track_name`: they are
/// legacy or native screen tracks and cannot be pulled.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrackInfo {
    /// Absent on a broadcast track; use the outer `participant_id` then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    /// Yap's alias, e.g. `cam-audio-<participant id>`.
    pub track_name: String,
    /// The SFU's track name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sfu_track_name: Option<String>,
    /// The PartyTracks session capability.
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mid: Option<String>,
    pub kind: TrackKind,
}

/// Video (op 12): tracks announced or changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    /// Set by the server on the broadcast to listeners.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    pub tracks: Vec<TrackInfo>,
}

/// StopTracks (op 13). Clients send only `track_names`; the server's
/// broadcast adds `participant_id` and `tracks`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StopTracks {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    #[serde(default)]
    pub track_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<TrackInfo>,
}

/// ClientDisconnect (op 11). Clients send `{}`; on the voice socket the
/// server sends the departing `participant_id`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClientDisconnect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
}

/// Speaking (op 5). Clients send `speaking` only; the server relays it with
/// the speaker's `participant_id`. Bit 0 is the microphone.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Speaking {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub participant_id: Option<String>,
    pub speaking: i64,
}

/// VoiceReady (op 101).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct VoiceReady {
    pub participant_id: String,
    /// Every live track except the receiver's own.
    #[serde(default)]
    pub tracks: Vec<TrackInfo>,
    /// Speaking flags by participant id.
    #[serde(default)]
    pub speaking: BTreeMap<String, i64>,
    pub join_wait_ms: Option<u64>,
}

// DM calls (chat gateway ops 36-39, `chat-gateway.ts` CALL_*)

/// CallInitiate (op 36): ring the other recipient of a DM.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallInitiate {
    pub target_user_id: String,
    pub channel_id: String,
}

/// CallAccept (37), CallDecline (38) and CallEnd (39).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRef {
    pub call_id: String,
}

/// `CALL_RING`: someone calls us.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CallRing {
    pub call_id: String,
    pub caller_id: String,
    pub caller_name: Option<String>,
    pub caller_username: Option<String>,
    pub caller_display_name: Option<String>,
    pub caller_avatar: Option<String>,
    pub channel_id: String,
}

/// `CALL_RINGING`: our call rings on their side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CallRinging {
    pub call_id: String,
    pub callee_id: String,
    pub callee_name: Option<String>,
    pub callee_username: Option<String>,
    pub callee_display_name: Option<String>,
    pub callee_avatar: Option<String>,
    pub channel_id: String,
}

/// `CALL_RING_STOP`: the ring is over (`accepted`, `timeout`, `declined`,
/// `cancelled`, `busy`, `unavailable`, `invalid`, `expired`, `abandoned`,
/// ...). `call_id` is null for a call that never rang.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct CallRingStop {
    pub call_id: Option<String>,
    pub reason: String,
}

/// VoiceMove (op 42, room socket): a moderator moved this session.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct VoiceMove {
    pub channel_id: String,
    pub server_id: String,
    pub channel_name: Option<String>,
    pub mover_name: Option<String>,
    /// Signed for this user and channel; stands in for CONNECT there.
    pub grant: Option<String>,
    /// Epoch ms.
    pub grant_expires_at: Option<f64>,
}

/// `POST /api/voice/move` (voice-move-client.ts).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VoiceMoveRequest {
    pub server_id: String,
    pub user_id: String,
    pub from_channel_id: String,
    pub to_channel_id: String,
}

/// TrackUnavailable (op 109, voice socket): our pulls of this track keep
/// failing with `not_found_track_error`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TrackUnavailable {
    pub participant_id: String,
    pub track_name: String,
    pub sfu_track_name: String,
    pub session_id: String,
}

/// RepublishRequest (op 110, voice socket): push this publication again.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct RepublishRequest {
    pub track_name: String,
    pub sfu_track_name: Option<String>,
    pub session_id: Option<String>,
}
