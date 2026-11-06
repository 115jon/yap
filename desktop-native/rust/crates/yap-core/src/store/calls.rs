//! Batch N4's call surfaces: 1:1 DM calls (useCallStore.ts and the
//! gateway's `CALL_*` events in chat-gateway.ts: ringing both ways, accept,
//! decline, cancel, the busy auto-decline, the ring sounds and the toast),
//! moderator voice moves (voice-move-client.ts), sticker reactions
//! (StickerReactionsOverlay.tsx), the voice details' numbers
//! (VoiceDetailsPanel.tsx, StreamingStatsPanel.tsx) and the audio sentinels'
//! banners.
//!
//! What the UI asks for is a [`CallsCommand`]; everything QML draws is one
//! [`CallsView`], sent whenever it changes.

use serde::Serialize;

/// How many sticker reactions the view keeps (the overlay animates each
/// once, by `seq`).
pub const STICKERS_KEPT: usize = 16;

/// `useCallStore` `status`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RingStatus {
    #[default]
    Idle,
    RingingOutgoing,
    RingingIncoming,
    Active,
}

/// The other person in a DM call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CallUser {
    pub id: String,
    pub name: String,
    pub username: String,
    /// Absolute, or empty.
    pub avatar: String,
}

/// The DM call, as `useCallStore` keeps it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Ring {
    pub status: RingStatus,
    /// `None` for a cold rejoin (no ring happened on this client).
    pub call_id: Option<String>,
    pub remote: CallUser,
    /// The DM channel.
    pub channel_id: String,
    /// `dm-call-<a>-<b>`.
    pub room: String,
    /// The user opted into the call's media (`hasJoinedSFU`).
    pub joined_sfu: bool,
    /// Turn the camera on once the call's room joins ("Join with video").
    pub with_video: bool,
}

/// What the UI asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallsCommand {
    /// Ring the open DM's recipient (op 36), or rejoin its active call.
    Start { channel_id: String },
    /// Answer the incoming ring (op 37); `video` turns the camera on.
    Accept { video: bool },
    /// Decline the incoming ring (op 38).
    Decline,
    /// Cancel our outgoing ring (op 39).
    Cancel,
    /// Join a DM's call that is already going (the lobby's Join voice).
    Join { channel_id: String, video: bool },
    /// Hang up: leave the call's room and end the call ("left").
    Leave,
    /// A moderator moves someone (`POST /api/voice/move`).
    Move {
        user_id: String,
        from_channel_id: String,
        to_channel_id: String,
    },
    /// A sticker reaction into the call (op 106 `reaction.sticker`).
    Sticker {
        url: String,
        content_type: String,
        display_mode: String,
    },
    /// The voice details opened or closed (stats are kept either way).
    Details(bool),
}

/// The numbers of the voice details (`VoiceConnectionStats`), as the call
/// reports them every 5 s.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallStats {
    /// `good`, `warn` or `bad`.
    pub quality: String,
    /// `None` until the SFU answers a STUN check.
    pub rtt_ms: Option<f64>,
    pub packet_loss_rate: f64,
    pub jitter_ms: f64,
    pub bitrate_kbps: f64,
    pub server: String,
    pub remote_tracks: u32,
    /// Our outbound video layers (camera and share).
    pub video: Vec<CallVideoStat>,
    /// The last 24 ticks, oldest first: `[bitrate, loss, jitter, rtt]`.
    pub samples: Vec<[f64; 4]>,
}

/// One outbound video encoding.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallVideoStat {
    pub mid: String,
    pub rid: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub encoder: String,
    pub limitation: String,
    pub frames: u64,
    pub active: bool,
}

/// A sticker that landed (the overlay plays each `seq` once).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StickerView {
    pub seq: u64,
    pub url: String,
    pub content_type: String,
    pub display_mode: String,
    /// The sender's Yap user id, when the room knew it.
    pub user_id: String,
}

/// Someone in the open DM's call, for its lobby tiles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LobbyPerson {
    pub user_id: String,
    pub name: String,
    pub avatar: String,
    pub initials: String,
    pub me: bool,
    pub muted: bool,
    pub deafened: bool,
    /// In the call's room right now.
    pub in_voice: bool,
}

/// A voice channel of the open server, for "Move to".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveChannel {
    pub id: String,
    pub name: String,
    /// The viewer may move members out of it (`canMoveMembersIn`).
    pub move_out: bool,
    /// The viewer may move members into it (`canMoveMembersInto`).
    pub move_in: bool,
}

/// Everything the call surfaces draw.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallsView {
    pub status: RingStatus,
    pub call_id: String,
    pub channel_id: String,
    pub remote: CallUser,
    /// The user is in the call's room (or joining it).
    pub joined: bool,
    /// The call we are in (or joining) is a DM call.
    pub in_dm_call: bool,
    /// Why the last DM call ended (`endReason`), until the next one.
    pub end_reason: String,
    /// The open DM, when it has a call going or ringing: its region shows.
    pub region: bool,
    /// The open DM's channel and recipient (for the header's call key).
    pub dm_channel_id: String,
    pub dm_user_id: String,
    pub dm_name: String,
    /// Who the open DM's call has (voice members of the DM channel, and
    /// the ringing pair), as the lobby draws them.
    pub lobby: Vec<LobbyPerson>,
    pub details_open: bool,
    pub stats: Option<CallStats>,
    pub inbound_stalled: bool,
    pub mic_stalled: bool,
    pub stickers: Vec<StickerView>,
    /// The open server's voice channels, for "Move to".
    pub voice_channels: Vec<MoveChannel>,
    /// Why the last move was refused, until the next one.
    pub move_error: String,
}

/// The calls' state in the store.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallsState {
    pub ring: Option<Ring>,
    pub end_reason: String,
    pub details_open: bool,
    pub stats: Option<CallStats>,
    pub inbound_stalled: bool,
    pub mic_stalled: bool,
    pub stickers: Vec<StickerView>,
    pub sticker_seq: u64,
    pub move_error: String,
}

/// `describeMoveError`, by the route's error code (or its sentence).
pub fn move_error_copy(message: &str) -> Option<&'static str> {
    let codes = [
        ("NOT_IN_VOICE", "They already left that voice channel."),
        (
            "ROLE_HIERARCHY",
            "You can't move someone with an equal or higher role.",
        ),
        ("TARGET_CANNOT_VIEW", "They can't see that channel."),
        (
            "MISSING_PERMISSIONS",
            "You don't have permission to move members there.",
        ),
    ];
    codes
        .into_iter()
        .find(|(code, _)| message.contains(code))
        .map(|(_, copy)| copy)
}

/// The incoming-call toast's title (chat-gateway.ts CALL_RING).
pub fn ring_toast_title(name: &str) -> String {
    format!("Incoming call from {name}")
}

/// A ring stop that ends a call plays the end tone, except for these
/// (chat-gateway.ts CALL_RING_STOP).
pub fn silent_stop(reason: &str) -> bool {
    matches!(reason, "busy" | "unavailable" | "invalid" | "declined")
}
