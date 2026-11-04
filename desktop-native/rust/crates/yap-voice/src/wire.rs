//! The call's wire shapes (VOICE_PROTOCOL.md): typed views of the frames
//! the machine reads, and the frames it sends. Pure; no I/O.

use serde_json::{json, Value};
use yap_protocol::{Frame, GatewayOp};

/// An ICE server from the room ticket's `media_bootstrap`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IceServer {
    pub urls: Vec<String>,
    pub username: String,
    pub password: String,
}

/// What the room ticket's `media_bootstrap` grants.
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
    pub self_mute: Option<bool>,
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
    /// Reads `media_bootstrap` from the room ticket reply. Without ICE
    /// servers it falls back to Cloudflare's STUN.
    pub fn from_room_ticket(room: &Value) -> Option<Self> {
        let bootstrap = room.get("media_bootstrap")?;
        let ice_servers: Vec<IceServer> = bootstrap
            .get("ice_servers")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .map(|s| IceServer {
                        urls: s
                            .get("urls")
                            .and_then(Value::as_array)
                            .map(|u| {
                                u.iter()
                                    .filter_map(Value::as_str)
                                    .map(str::to_owned)
                                    .collect()
                            })
                            .unwrap_or_default(),
                        username: str_of(s, "username").to_owned(),
                        password: str_of(s, "credential").to_owned(),
                    })
                    .collect()
            })
            .filter(|list: &Vec<IceServer>| !list.is_empty())
            .unwrap_or_else(|| {
                vec![IceServer {
                    urls: vec!["stun:stun.cloudflare.com:3478".into()],
                    username: String::new(),
                    password: String::new(),
                }]
            });
        Some(Self {
            participant_id: str_of(bootstrap, "participant_id").to_owned(),
            voice_token: str_of(bootstrap, "voice_token").to_owned(),
            ice_servers,
        })
    }
}

impl Participant {
    pub fn from_value(p: &Value) -> Self {
        Self {
            id: str_of(p, "id").to_owned(),
            name: member_name(p),
            self_mute: p.get("self_mute").and_then(Value::as_bool),
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

/// VoiceReady's `speaking` map: participant -> speaking (bit 0).
pub fn speaking_map(d: &Value) -> Vec<(String, bool)> {
    d.get("speaking")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(pid, bits)| (pid.clone(), bits.as_i64().unwrap_or(0) & 1 == 1))
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
    json!({ "op": GatewayOp::Identify.code(), "d": {
        "name": name, "username": username,
        "platform": "desktop", "supports_voice_state_deltas": true,
    }})
}

/// The voice Identify (op 100).
pub fn voice_identify(me: &str, voice_token: &str) -> Value {
    json!({ "op": GatewayOp::VoiceIdentify.code(), "d": { "participant_id": me, "voice_token": voice_token } })
}

/// Our voice state (op 15 C->S).
pub fn voice_state(muted: bool) -> String {
    json!({ "op": GatewayOp::VoiceStateUpdate as u16, "d": {
        "self_mute": muted, "self_deaf": false, "self_video": false,
        "self_stream": false, "self_stream_audio": false,
    }})
    .to_string()
}

/// Our mic publication (op 12).
pub fn announce(me: &str, track_name: &str, sfu_track_name: &str, session_id: &str) -> String {
    json!({ "op": GatewayOp::Video as u16, "d": { "tracks": [{
        "participant_id": me, "track_name": track_name, "sfu_track_name": sfu_track_name,
        "session_id": session_id, "kind": "audio",
    }]}})
    .to_string()
}

/// Stops our named track (op 13).
pub fn stop_tracks(track_name: &str) -> String {
    json!({ "op": GatewayOp::StopTracks.code(), "d": { "track_names": [track_name] } }).to_string()
}

/// The body for `POST /api/voice/socket-ticket`.
pub fn ticket_body(audience: &str, server_id: &str, channel_id: &str, slug: &str) -> Value {
    json!({ "audience": audience, "channelId": channel_id, "serverId": server_id, "roomSlug": slug })
}
