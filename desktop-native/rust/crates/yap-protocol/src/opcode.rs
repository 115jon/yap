//! Gateway opcodes and close codes, mirroring `packages/protocol/src/opcodes.ts`.
//!
//! The `opcode_parity` integration test reads the TypeScript file and fails
//! when a name or number differs, so edit both sides together.

use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A number that is not a known opcode or close code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownCode(pub i64);

impl fmt::Display for UnknownCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown code {}", self.0)
    }
}

impl std::error::Error for UnknownCode {}

/// Declares a fieldless enum with explicit `u16` discriminants that
/// serialises as a bare integer.
macro_rules! int_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $( $(#[$vmeta:meta])* $variant:ident = $value:literal, )+
        }
    ) => {
        $(#[$meta])*
        #[repr(u16)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant = $value, )+
        }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[ $(Self::$variant),+ ];

            /// The wire number.
            pub const fn code(self) -> u16 {
                self as u16
            }

            /// The variant name, as spelled in the TypeScript enum.
            pub const fn name(self) -> &'static str {
                match self {
                    $( Self::$variant => stringify!($variant), )+
                }
            }
        }

        impl TryFrom<i64> for $name {
            type Error = UnknownCode;

            fn try_from(value: i64) -> Result<Self, UnknownCode> {
                match value {
                    $( $value => Ok(Self::$variant), )+
                    other => Err(UnknownCode(other)),
                }
            }
        }

        impl From<$name> for u16 {
            fn from(value: $name) -> u16 {
                value.code()
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_u16(self.code())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = i64::deserialize(deserializer)?;
                Self::try_from(value).map_err(D::Error::custom)
            }
        }
    };
}

int_enum! {
    /// One numeric opcode space shared by the main gateway (MeetingRoom DO)
    /// and the voice gateway (VoiceRoom DO). A number never means two things.
    pub enum GatewayOp {
        /// C->S main: initial identification (join room).
        Identify = 0,
        /// C->S voice: native Rust screen-share SDP offer.
        SelectProtocol = 1,
        /// S->C main: confirm identification, send ICE/TURN config and roster.
        Ready = 2,
        /// C->S both: heartbeat keepalive. Sent raw as [`crate::HEARTBEAT_FRAME`].
        Heartbeat = 3,
        /// S->C voice: native screen-share SDP answer from the SFU.
        SessionDescription = 4,
        /// C<->S voice: speaking state with bitfield flags.
        Speaking = 5,
        /// S->C both: heartbeat acknowledgement.
        HeartbeatACK = 6,
        /// C->S main: resume a previous session after reconnect.
        Resume = 7,
        /// S->C both: first message after connect, carries `heartbeat_interval`.
        Hello = 8,
        /// S->C main: confirm session resume (also the voice-token refresh reply).
        Resumed = 9,
        /// S->C voice: native screen-share negotiation completed on the SFU.
        NegotiationDone = 10,
        /// C<->S both: graceful disconnect (S->C on voice: a participant left).
        ClientDisconnect = 11,
        /// C<->S voice: PartyTracks publication metadata.
        Video = 12,
        /// C<->S voice: stop/remove tracks.
        StopTracks = 13,
        /// C<->S main: voice state (C->S mute/camera, S->C join/leave/update).
        VoiceStateUpdate = 15,
        /// S->C main: profile info broadcast.
        ProfileUpdate = 16,
        /// C->S main: ask the server to re-fetch the verified profile.
        ProfileRefresh = 17,
        /// S->C both: error.
        Error = 18,

        // Chat opcodes (19-32).

        /// S->C: dispatch event (MESSAGE_CREATE, TYPING_START, ...).
        Dispatch = 19,
        /// C->S: send a chat message.
        MessageCreate = 20,
        /// C->S: edit a message.
        MessageUpdate = 21,
        /// C->S: delete a message.
        MessageDelete = 22,
        /// C->S: typing indicator.
        TypingStart = 23,
        /// C->S: add a reaction.
        ReactionAdd = 24,
        /// C->S: remove a reaction.
        ReactionRemove = 25,
        /// C->S: set own presence (S->C presence goes out as a Dispatch).
        PresenceUpdate = 26,
        /// C->S: subscribe to a channel's events.
        ChannelSubscribe = 27,
        /// C->S: unsubscribe from a channel.
        ChannelUnsubscribe = 28,
        /// S->C: channel updated.
        ChannelUpdate = 29,
        /// S->C: channel deleted.
        ChannelDelete = 30,
        /// S->C: server member add/remove/update.
        GuildMemberUpdate = 31,
        /// S->C: relationship update (friend request, ...).
        RelationshipUpdate = 32,

        // Voice channel sidebar opcodes (33-35).

        /// C->S: join a voice channel.
        VoiceChannelJoin = 33,
        /// C->S: leave a voice channel.
        VoiceChannelLeave = 34,
        /// C->S: subscribe to server events.
        ServerSubscribe = 35,

        // Call opcodes (36-42).

        /// C->S: initiate a 1:1 call.
        CallInitiate = 36,
        /// C->S: accept an incoming call.
        CallAccept = 37,
        /// C->S: decline an incoming call.
        CallDecline = 38,
        /// C->S: end an active call or cancel an outgoing ring.
        CallEnd = 39,
        /// C->S: refresh voice token and TURN credentials without reconnecting.
        RefreshVoiceCredentials = 40,
        /// S->C: media credentials before the full room/profile Ready.
        MediaReady = 41,
        /// S->C room: a moderator moved this voice session to another channel.
        VoiceMove = 42,

        // Voice gateway specific (100+).

        /// C->S: authenticate on the voice gateway with the main gateway's token.
        VoiceIdentify = 100,
        /// S->C: voice gateway confirms authentication.
        VoiceReady = 101,
        /// C->S: native publisher confirms screen tracks are flowing.
        TracksReady = 102,
        /// C<->S: lightweight voice-session app events (activities, soundboard).
        VoiceAppEvent = 106,
        /// C->S voice: pulls of another participant's track keep failing on the SFU.
        TrackUnavailable = 109,
        /// S->C voice: push this publication again under a new SFU track name.
        RepublishRequest = 110,
    }
}

int_enum! {
    /// Codes carried in Error frames (`d.code`) and, for some, as WebSocket
    /// close codes. Clients compare against these, never against message text.
    pub enum GatewayCloseCode {
        /// Bad request: a payload failed validation or a business rule.
        InvalidPayload = 4000,
        /// Missing, non-numeric or unknown opcode.
        UnknownOpcode = 4001,
        /// The frame is not JSON.
        InvalidJson = 4002,
        /// Not identified, not allowed, or unavailable for this access mode.
        NotAuthenticated = 4003,
        /// Voice token rejected; also "not found" replies on the main gateway.
        AuthenticationFailed = 4004,
        /// Identify sent twice.
        AlreadyAuthenticated = 4005,
        /// Resume impossible (unknown session or continuity lost): Identify again.
        SessionInvalid = 4006,
        /// Realtime admission rejected: the client must not reconnect.
        AdmissionRejected = 4008,
        /// No heartbeat in time.
        SessionTimeout = 4009,
        /// The user's voice session in this room moved to another device. The
        /// client must stop: no reconnect, resume or rejoin until the user acts.
        SessionReplaced = 4010,
        /// The server was not found.
        ServerNotFound = 4014,
        /// Unknown protocol.
        UnknownProtocol = 4015,
        /// Per-opcode rate limit exceeded.
        RateLimited = 4290,
    }
}

/// Opcodes of the retired browser SFU negotiation (14, 103-105, 107, 108).
/// The voice gateway answers each with "Browser SFU negotiation is
/// unsupported" instead of "Unknown opcode".
pub const RETIRED_BROWSER_SFU_OPS: [u16; 6] = [14, 103, 104, 105, 107, 108];

/// The heartbeat exactly as clients send it: no `d`, no extra fields, no
/// whitespace. Keep it byte-stable; heartbeats are never schema-decoded.
pub const HEARTBEAT_FRAME: &str = r#"{"op":3}"#;

/// Close reason (with `SessionReplaced`) when a moderator moved the session
/// to another channel, on both call sockets.
pub const VOICE_MOVED_CLOSE_REASON: &str = "Moved to another voice channel";

/// WebSocket close code ("Try Again Later") the voice room closes with when
/// a VoiceIdentify's join grant could not be verified, or names another
/// participant than the voice ticket is bound to (`realtime/voice-room.ts`
/// `VOICE_JOIN_UNVERIFIED_CLOSE_CODE`, ADMISSION.md). Not a gateway code: no
/// Error frame comes first. Retryable with a fresh voice ticket bound to the
/// participant the client is now.
pub const VOICE_JOIN_UNVERIFIED_CLOSE_CODE: u16 = 1013;

/// Used when a Hello carries no interval.
pub const DEFAULT_HEARTBEAT_INTERVAL_MS: u64 = 45_000;
