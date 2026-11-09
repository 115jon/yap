//! The call's wire shapes (VOICE_PROTOCOL.md): typed views of the frames
//! the machine reads, and the frames it sends. Pure; no I/O.

use serde_json::{json, Value};
use yap_protocol::rest::MediaBootstrap;
use yap_protocol::{Frame, GatewayOp};

use crate::spatial::SpatialState;

/// An ICE server from the admission's `media`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

/// What the admission's `media` grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bootstrap {
    pub participant_id: String,
    pub voice_token: String,
    pub ice_servers: Vec<IceServer>,
}

/// A participant in a room voice state (Ready, Resumed or op 15).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Participant {
    pub id: String,
    /// Full states carry one; deltas ("update", because we send
    /// `supports_voice_state_deltas`) leave the profile fields out.
    pub name: Option<String>,
    /// The Yap user behind the participant (full states only).
    pub user_id: Option<String>,
    pub self_mute: Option<bool>,
    pub self_deaf: Option<bool>,
    /// Camera on.
    pub self_video: Option<bool>,
    /// Sharing their screen.
    pub self_stream: Option<bool>,
    /// Their spatial audio switches (`spatial_audio_enabled`,
    /// `spatial_audio_high_fidelity`), for the spatial panel's notes.
    pub spatial_enabled: Option<bool>,
    pub spatial_high_fidelity: Option<bool>,
}

/// A room Ready or Resumed `d`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RoomState {
    /// The room session's participant (Ready only).
    pub participant_id: String,
    /// The others in the room (it excludes you); `None` when absent.
    pub participants: Option<Vec<Participant>>,
    /// A fresh voice token (Resumed only); empty when absent.
    pub voice_token: String,
}

/// One published track, as VoiceReady and op 12 list them. Fields the
/// frame leaves out are empty.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Track {
    pub participant_id: String,
    pub track_name: String,
    pub sfu_track_name: String,
    pub session_id: String,
    pub kind: String,
    /// A simulcast camera's layers, highest first (`["f","h","q"]`);
    /// empty for a single layer.
    pub simulcast_rids: Vec<String>,
}

pub(crate) fn str_of<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn member_name(value: &Value) -> Option<String> {
    ["display_name", "name", "username"]
        .into_iter()
        .map(|key| str_of(value, key))
        .find(|name| !name.is_empty())
        .map(str::to_owned)
}

impl Bootstrap {
    /// Reads the admission's `media`. Without ICE servers it falls back to
    /// Cloudflare's STUN.
    pub fn from_media(media: &MediaBootstrap) -> Self {
        let mut ice_servers: Vec<IceServer> = media
            .ice_servers
            .iter()
            .map(|s| IceServer {
                urls: s.urls.clone(),
                username: s.username.clone().unwrap_or_default(),
                password: s.credential.clone().unwrap_or_default(),
            })
            .collect();
        if ice_servers.is_empty() {
            ice_servers.push(IceServer {
                urls: vec!["stun:stun.cloudflare.com:3478".into()],
                username: String::new(),
                password: String::new(),
            });
        }
        Self {
            participant_id: media.participant_id.clone(),
            voice_token: media.voice_token.clone(),
            ice_servers,
        }
    }
}

impl Participant {
    pub fn from_value(p: &Value) -> Self {
        Self {
            id: str_of(p, "id").to_owned(),
            name: member_name(p),
            user_id: p
                .get("user_id")
                .and_then(Value::as_str)
                .filter(|id| !id.is_empty())
                .map(str::to_owned),
            self_mute: p.get("self_mute").and_then(Value::as_bool),
            self_deaf: p.get("self_deaf").and_then(Value::as_bool),
            self_video: p.get("self_video").and_then(Value::as_bool),
            self_stream: p.get("self_stream").and_then(Value::as_bool),
            spatial_enabled: p.get("spatial_audio_enabled").and_then(Value::as_bool),
            spatial_high_fidelity: p
                .get("spatial_audio_high_fidelity")
                .and_then(Value::as_bool),
        }
    }
}

impl RoomState {
    pub fn from_value(d: &Value) -> Self {
        Self {
            participant_id: str_of(d, "participant_id").to_owned(),
            participants: d
                .get("participants")
                .and_then(Value::as_array)
                .map(|list| list.iter().map(Participant::from_value).collect()),
            voice_token: str_of(d, "voice_token").to_owned(),
        }
    }
}

impl Track {
    pub fn from_value(t: &Value) -> Self {
        Self {
            participant_id: str_of(t, "participant_id").to_owned(),
            track_name: str_of(t, "track_name").to_owned(),
            sfu_track_name: str_of(t, "sfu_track_name").to_owned(),
            session_id: str_of(t, "session_id").to_owned(),
            kind: str_of(t, "kind").to_owned(),
            simulcast_rids: t
                .get("simulcast_rids")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        }
    }

    /// `d.tracks` of a VoiceReady or op 12.
    pub fn list(d: &Value) -> Vec<Self> {
        d.get("tracks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(Self::from_value)
            .collect()
    }
}

/// VoiceReady's `speaking` map: participant -> speaking (any flag, as the
/// web reads it: the microphone or the soundboard).
pub fn speaking_map(d: &Value) -> Vec<(String, bool)> {
    d.get("speaking")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(pid, bits)| (pid.clone(), bits.as_i64().unwrap_or(0) != 0))
        .collect()
}

/// The names an op 13 stops: `track_names`, plus each `tracks[]` entry's
/// `track_name` and `sfu_track_name`. Empty means every track of the sender.
pub fn stopped_names(d: &Value) -> Vec<String> {
    let mut names: Vec<String> = d
        .get("track_names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    for t in d
        .get("tracks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        for key in ["track_name", "sfu_track_name"] {
            let name = str_of(t, key);
            if !name.is_empty() {
                names.push(name.to_owned());
            }
        }
    }
    names
}

/// `{"op":<op>,"d":{}}`.
pub fn empty_frame(op: GatewayOp) -> String {
    Frame::new(op, json!({})).encode()
}

/// The room Identify (op 0); it, not standby, is what grants the join.
pub fn room_identify(name: &str, username: &str) -> Value {
    room_identify_as(name, username, false)
}

/// The room Identify, as a hidden standby (`standby: true`: no join grant,
/// nobody sees us) when `standby`.
pub fn room_identify_as(name: &str, username: &str, standby: bool) -> Value {
    let mut frame = json!({ "op": GatewayOp::Identify.code(), "d": {
        "name": name, "username": username,
        "platform": "desktop", "supports_voice_state_deltas": true,
    }});
    if standby {
        frame["d"]["standby"] = json!(true);
    }
    frame
}

/// Promotes a standby room session to a join (op 15 `promote_standby`,
/// room-gateway.ts `promoteStandby`), with the mic and deafen state.
pub fn promote_standby(muted: bool, deafened: bool) -> String {
    json!({ "op": GatewayOp::VoiceStateUpdate as u16, "d": {
        "promote_standby": true, "self_mute": muted, "self_deaf": deafened,
    }})
    .to_string()
}

/// TrackUnavailable (op 109): the SFU keeps not finding a track we pull.
pub fn track_unavailable(
    participant_id: &str,
    track_name: &str,
    sfu_track_name: &str,
    session_id: &str,
) -> String {
    json!({ "op": GatewayOp::TrackUnavailable.code(), "d": {
        "participant_id": participant_id, "track_name": track_name,
        "sfu_track_name": sfu_track_name, "session_id": session_id,
    }})
    .to_string()
}

/// The voice Identify (op 100).
pub fn voice_identify(me: &str, voice_token: &str) -> Value {
    json!({ "op": GatewayOp::VoiceIdentify.code(), "d": { "participant_id": me, "voice_token": voice_token } })
}

/// Our voice state (op 15 C->S).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SelfState {
    pub muted: bool,
    pub deafened: bool,
    /// The camera is on.
    pub video: bool,
    /// Sharing (`self_stream`), and the share carries audio.
    pub stream: bool,
    pub stream_audio: bool,
}

/// Our voice state as op 15 sends it.
pub fn voice_state(state: SelfState) -> String {
    json!({ "op": GatewayOp::VoiceStateUpdate as u16, "d": {
        "self_mute": state.muted, "self_deaf": state.deafened, "self_video": state.video,
        "self_stream": state.stream, "self_stream_audio": state.stream_audio,
    }})
    .to_string()
}

/// The room's spatial state in a Ready, Resumed or op 15 `d`, if it carries
/// one (`spatial_audio_state`).
pub fn spatial_of(d: &Value) -> Option<SpatialState> {
    d.get("spatial_audio_state")
        .filter(|v| !v.is_null())
        .and_then(SpatialState::from_value)
}

/// Our spatial switches and the room's state (op 15 C->S,
/// `useSpatialAudioSync`).
pub fn spatial_state(enabled: bool, high_fidelity: bool, state: &SpatialState) -> String {
    json!({ "op": GatewayOp::VoiceStateUpdate as u16, "d": {
        "spatial_audio_enabled": enabled,
        "spatial_audio_high_fidelity": high_fidelity,
        "spatial_audio_state": state.to_value(),
    }})
    .to_string()
}

/// Now, in epoch milliseconds (`Date.now()`; the server stamps its own).
pub fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_millis() as f64)
}

/// One of our publications as op 12 announces it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Announced<'a> {
    /// The alias (`cam-audio-<pid>`, `cam-video-<pid>`, `screen-video-<pid>`).
    pub track_name: &'a str,
    pub sfu_track_name: &'a str,
    /// Our session capability.
    pub session_id: &'a str,
    /// `audio` or `video`.
    pub kind: &'a str,
    /// The layers a simulcast camera negotiated, highest first; empty for
    /// one layer (left out of the frame, as the web leaves it out).
    pub simulcast_rids: &'a [String],
}

/// One publication (op 12), one per frame as the web sends them.
pub fn announce(me: &str, track: &Announced<'_>) -> String {
    let mut info = json!({
        "participant_id": me, "track_name": track.track_name,
        "sfu_track_name": track.sfu_track_name, "session_id": track.session_id,
        "kind": track.kind,
    });
    if !track.simulcast_rids.is_empty() {
        info["simulcast_rids"] = json!(track.simulcast_rids);
    }
    json!({ "op": GatewayOp::Video as u16, "d": { "tracks": [info] } }).to_string()
}

/// We stopped offering these aliases (op 13 C->S), as the web's
/// `onPublicationStopped` says it.
pub fn stop_tracks(names: &[&str]) -> String {
    json!({ "op": GatewayOp::StopTracks.code(), "d": { "track_names": names } }).to_string()
}

/// Our speaking flags (op 5): bit 0 is MICROPHONE (`src/lib/types.ts`).
pub fn speaking(on: bool) -> String {
    speaking_flags(u8::from(on))
}

/// `SpeakingFlags.SOUNDSHARE`: our own soundboard clip is playing.
pub const SPEAKING_SOUNDSHARE: u8 = 1 << 1;

/// Op 5 with every flag (MICROPHONE, SOUNDSHARE).
pub fn speaking_flags(flags: u8) -> String {
    json!({ "op": GatewayOp::Speaking.code(), "d": { "speaking": flags } }).to_string()
}

/// Watching a share starts or stops (op 106, SCREEN_SHARE.md "Watching");
/// the server's watcher snapshots follow.
pub fn watch_event(streamer_user_id: &str, watching: bool) -> String {
    let kind = if watching {
        "stream.watch.start"
    } else {
        "stream.watch.stop"
    };
    json!({ "op": GatewayOp::VoiceAppEvent.code(), "d": {
        "type": kind, "streamer_user_id": streamer_user_id,
    }})
    .to_string()
}
