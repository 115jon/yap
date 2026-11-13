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
    /// This device answered or started the call. The server sends the ring
    /// and its stop to every session of both users, so a device that only
    /// saw the call ring must not join when it is accepted elsewhere.
    pub answered_here: bool,
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
    /// The SFU edge we talk to (remote candidate, `IP:port proto`).
    pub server: String,
    /// The call's room slug, which the web shows as "Server"
    /// (`serverIdentifier`, stats-monitor.ts).
    pub room: String,
    pub remote_tracks: u32,
    /// Our outbound video layers (camera and share).
    pub video: Vec<CallVideoStat>,
    /// The video we receive.
    pub incoming: Vec<CallIncomingVideoStat>,
    /// The last 24 ticks, oldest first: `[bitrate, loss, jitter, rtt,
    /// age]`, the age in whole seconds before the latest tick (0 for it,
    /// negative before), as the samples table reads it (`formatAge`).
    pub samples: Vec<[f64; 5]>,
    /// Seconds the samples span, counting the latest one's interval: the
    /// caption's "last N s" (dither-chart-model.ts `windowS`).
    pub window_s: u32,
}

/// The stats tick, seconds (yap-voice `STATS_EVERY`, the web's 2 s): what
/// one sample counts for before a second one shows the real gap.
pub const STATS_TICK_S: u32 = 2;

/// The chart's samples and window from raw ticks, oldest first: each tick
/// is when it was taken (ms) and `[bitrate, loss, jitter, rtt]`
/// (dither-chart-model.ts `windowS`, `sampleIntervalS`, `formatAge`).
pub fn chart_samples(ticks: &[(u64, [f64; 4])]) -> (Vec<[f64; 5]>, u32) {
    let Some(&(last, _)) = ticks.last() else {
        return (Vec::new(), 0);
    };
    #[allow(clippy::cast_precision_loss)]
    let age = |at: u64| -((last.saturating_sub(at) as f64 / 1000.0).round());
    let samples = ticks
        .iter()
        .map(|&(at, [b, l, j, r])| [b, l, j, r, age(at) + 0.0])
        .collect();
    // The median gap, in whole seconds (at least 1).
    let mut gaps: Vec<u64> = ticks
        .windows(2)
        .map(|w| w[1].0.saturating_sub(w[0].0))
        .collect();
    gaps.sort_unstable();
    let interval_s = gaps.get(gaps.len() / 2).map_or(STATS_TICK_S, |&g| {
        u32::try_from((g + 500) / 1000).unwrap_or(u32::MAX).max(1)
    });
    let span_s = (last.saturating_sub(ticks[0].0) + 500) / 1000;
    let window_s = u32::try_from(span_s)
        .unwrap_or(u32::MAX)
        .saturating_add(interval_s);
    (samples, window_s)
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
    pub codec: String,
    pub bitrate_kbps: f64,
    pub dropped: u64,
    pub encode_ms: f64,
    pub hardware: bool,
}

/// One inbound video stream.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallIncomingVideoStat {
    pub key: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub codec: String,
    pub decoder: String,
    pub frames: u64,
    pub dropped: u64,
    pub decode_ms: f64,
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
    /// The DM whose call this device asked for (op 36), until its
    /// `CALL_RINGING`, a refusal or a gateway drop.
    pub initiating: Option<String>,
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

/// The notice for a call started while the gateway is down: op 36 cannot
/// be sent (the mobile app's `startDmCall`).
pub const CALL_OFFLINE: &str = "You're offline. Try again in a moment.";

/// Why a ring ended on this device when another device of ours answered
/// it (the server's stop only says `accepted`).
pub const ANSWERED_ELSEWHERE: &str = "answered_elsewhere";

/// A ring stop that ends a call plays the end tone, except for these
/// (chat-gateway.ts CALL_RING_STOP).
pub fn silent_stop(reason: &str) -> bool {
    matches!(
        reason,
        "busy" | "unavailable" | "invalid" | "declined" | ANSWERED_ELSEWHERE
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chart_samples_should_age_each_tick_and_span_the_window() {
        let ticks = [
            (10_000, [40.0, 0.0, 4.0, 15.0]),
            (15_000, [42.0, 0.5, 5.0, 16.0]),
            (20_000, [44.0, 1.0, 6.0, 17.0]),
        ];
        let (samples, window_s) = chart_samples(&ticks);
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[0], [40.0, 0.0, 4.0, 15.0, -10.0]);
        assert_eq!(samples[2][4], 0.0);
        assert!(samples[2][4].is_sign_positive());
        // 10 s between the first and the latest, plus one 5 s interval.
        assert_eq!(window_s, 15);
    }

    #[test]
    fn chart_samples_should_count_one_tick_as_the_stats_tick() {
        let (samples, window_s) = chart_samples(&[(7_000, [1.0, 2.0, 3.0, 4.0])]);
        assert_eq!(samples, vec![[1.0, 2.0, 3.0, 4.0, 0.0]]);
        assert_eq!(window_s, STATS_TICK_S);
        assert_eq!(chart_samples(&[]), (Vec::new(), 0));
    }

    #[test]
    fn chart_samples_should_use_the_median_gap() {
        let ticks = [
            (0, [0.0; 4]),
            (5_000, [0.0; 4]),
            (10_000, [0.0; 4]),
            (25_000, [0.0; 4]),
        ];
        // Gaps 5, 5, 15: the median is 5; span 25 s.
        assert_eq!(chart_samples(&ticks).1, 30);
    }

    #[test]
    fn call_stats_should_serialize_camel_case() {
        let stats = CallStats {
            room: "voice-s1-c1".into(),
            window_s: 15,
            samples: vec![[1.0, 0.0, 2.0, 3.0, 0.0]],
            ..CallStats::default()
        };
        let json = serde_json::to_value(&stats).expect("serializes");
        assert_eq!(json["room"], "voice-s1-c1");
        assert_eq!(json["windowS"], 15);
        assert_eq!(json["remoteTracks"], 0);
        assert_eq!(json["samples"][0].as_array().map(Vec::len), Some(5));
    }
}
