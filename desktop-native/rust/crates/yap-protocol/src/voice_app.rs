//! Voice app events (op 106 on the voice socket): the soundboard, Listen
//! Together, activities and Wordle progress.
//!
//! - [`AppEvent`] is what the room broadcasts, read as the web client reads
//!   it (`VoiceSoundboardManager.tsx`, `VoiceListenTogetherManager.tsx`,
//!   `VoiceChannelView.tsx`, `WordleActivityStage.tsx`): fields of the
//!   wrong type are left out, and an event without what it needs is `None`.
//! - [`AppCommand`] is what we send, shaped as the web sends it.
//! - The `sanitize_*` functions are the room's own checks
//!   (`realtime/voice-room.ts` `sanitizeSoundboardEvent` and
//!   `asListenTogetherCommand`, `realtime/voice-room-events.ts`), minus the
//!   database lookups (membership, catalog). [`AppCommand::checked`] runs
//!   them before a command goes out, so we never send what the room drops.
//!
//! JavaScript's `slice(0, n)` counts UTF-16 units; the ports count chars,
//! which only differs for text outside the BMP.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::{Frame, GatewayOp};

/// The soundboard's built-in tones (`DEFAULT_SOUNDBOARD_SOUNDS`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefaultSound {
    pub id: &'static str,
    pub name: &'static str,
    /// Hz.
    pub tone: f32,
    /// Seconds.
    pub duration: f32,
}

pub const DEFAULT_SOUNDS: [DefaultSound; 4] = [
    DefaultSound {
        id: "ping",
        name: "Ping",
        tone: 880.0,
        duration: 0.18,
    },
    DefaultSound {
        id: "pop",
        name: "Pop",
        tone: 520.0,
        duration: 0.14,
    },
    DefaultSound {
        id: "chime",
        name: "Chime",
        tone: 660.0,
        duration: 0.32,
    },
    DefaultSound {
        id: "tada",
        name: "Ta-da",
        tone: 740.0,
        duration: 0.42,
    },
];

/// Whether `id` names a built-in tone.
pub fn is_default_sound(id: &str) -> bool {
    DEFAULT_SOUNDS.iter().any(|sound| sound.id == id)
}

/// Where a server sound's audio lives (`SOUNDBOARD_UPLOAD_PATH`).
pub const SOUNDBOARD_UPLOAD_PATH: &str = "/api/soundboard/uploads/";

/// The longest automatic (join/leave) clip, in seconds.
pub const MAX_AUTOMATIC_SECONDS: f64 = 3.0;

/// The latest `current_time` the room relays (`MAX_SOUNDBOARD_POSITION_SECONDS`).
pub const MAX_POSITION_SECONDS: f64 = 24.0 * 60.0 * 60.0;

/// How far a `sent_at` may be from our clock before it is ignored
/// (`MAX_SOUNDBOARD_EVENT_PAST_SKEW_MS` / `_FUTURE_SKEW_MS`).
pub const MAX_SENT_AT_SKEW_MS: i64 = 5_000;

/// The server key of a call's soundboard: the server id, or `dm-call`
/// (`getSoundboardServerKey`).
pub fn server_key(server_id: Option<&str>) -> String {
    match server_id {
        Some(id) if !id.is_empty() => id.to_owned(),
        _ => "dm-call".to_owned(),
    }
}

/// `encodeURIComponent`.
pub fn uri_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'!'
            | b'~'
            | b'*'
            | b'\''
            | b'('
            | b')' => out.push(char::from(byte)),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A server sound's audio (`getSoundboardUploadUrl`).
pub fn upload_url(sound_id: &str) -> String {
    format!("{SOUNDBOARD_UPLOAD_PATH}{}", uri_component(sound_id))
}

/// The deterministic playback id (`createSoundboardPlaybackId`): the same
/// sound by the same person replaces its last playback everywhere.
pub fn playback_id(owner_id: &str, sound_id: &str) -> String {
    format!(
        "sb1:{}:{owner_id}:{sound_id}",
        owner_id.encode_utf16().count()
    )
}

/// `parseSoundboardPlaybackId`: (owner, sound).
pub fn parse_playback_id(value: &str) -> Option<(String, String)> {
    let rest = value.strip_prefix("sb1:")?;
    let (length, rest) = rest.split_once(':')?;
    if length.is_empty()
        || !length.bytes().all(|b| b.is_ascii_digit())
        || (length.len() > 1 && length.starts_with('0'))
    {
        return None;
    }
    let length: usize = length.parse().ok()?;
    if length == 0 {
        return None;
    }
    // The length counts UTF-16 units, as JavaScript does.
    let mut units = 0;
    let mut end = None;
    for (index, ch) in rest.char_indices() {
        if units == length {
            end = Some(index);
            break;
        }
        units += ch.len_utf16();
    }
    let end = match end {
        Some(end) => end,
        None if units == length => rest.len(),
        None => return None,
    };
    let owner = &rest[..end];
    let sound = rest[end..].strip_prefix(':')?;
    (!owner.is_empty() && !sound.is_empty()).then(|| (owner.to_owned(), sound.to_owned()))
}

/// `isSoundboardPlaybackOwnedBy`.
pub fn is_playback_owned_by(playback_id: &str, owner_id: &str) -> bool {
    parse_playback_id(playback_id).is_some_and(|(owner, _)| owner == owner_id)
}

/// `getSoundboardEventReceivedAt`: when the clip started on the sender's
/// side, from the room's `sent_at`, or `now` when that is missing or more
/// than five seconds off.
pub fn received_at(sent_at: Option<i64>, now: i64) -> i64 {
    match sent_at {
        Some(sent)
            if sent > 0
                && sent >= now - MAX_SENT_AT_SKEW_MS
                && sent <= now + MAX_SENT_AT_SKEW_MS =>
        {
            sent.min(now)
        }
        _ => now,
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn num_field(value: &Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
}

fn bool_field(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// `value.trim().slice(0, max)`, or empty for a non-string.
fn trimmed(value: &Value, key: &str, max: usize) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().chars().take(max).collect())
        .unwrap_or_default()
}

fn clamp01(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

// ── Soundboard ────────────────────────────────────────────────────────────

/// Join or leave: an automatic clip (`auto-soundboard.ts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutomaticEvent {
    Join,
    Leave,
}

impl AutomaticEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Join => "join",
            Self::Leave => "leave",
        }
    }

    fn read(value: &Value) -> Option<Self> {
        match value.get("automatic_event").and_then(Value::as_str) {
            Some("join") => Some(Self::Join),
            Some("leave") => Some(Self::Leave),
            _ => None,
        }
    }
}

/// `soundboard.play` as the room relays it.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundboardPlay {
    pub server_key: String,
    /// `user_id`, else `participant_id`.
    pub owner_id: String,
    pub playback_id: String,
    pub name: String,
    pub sound_id: Option<String>,
    pub source_server_id: Option<String>,
    /// `/api/soundboard/uploads/<id>` (maybe with a `cap` query in DM
    /// calls) or a MyInstants URL.
    pub media_url: Option<String>,
    pub data_url: Option<String>,
    /// 0..=1; the web's default is 0.8 when it is missing.
    pub volume: Option<f64>,
    /// Seconds into the clip.
    pub current_time: Option<f64>,
    pub paused: Option<bool>,
    pub automatic_event: Option<AutomaticEvent>,
    pub max_duration_seconds: Option<f64>,
    /// Epoch ms the room stamped.
    pub sent_at: Option<i64>,
}

/// One server sound as `soundboard.catalog-updated` carries it.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CatalogSound {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
}

// ── Listen Together ───────────────────────────────────────────────────────

/// A queued track (`ListenTogetherTrack`). Music has `videoId` and
/// `durationMs`; radio has `streamUrl`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtTrack {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_id: Option<String>,
    #[serde(default)]
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artwork_url: Option<String>,
    #[serde(default)]
    pub canonical_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    #[serde(default)]
    pub source_label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_url: Option<String>,
    #[serde(
        default,
        rename = "station_uuid",
        skip_serializing_if = "Option::is_none"
    )]
    pub station_uuid: Option<String>,
}

impl LtTrack {
    pub fn is_radio(&self) -> bool {
        self.kind == "radio"
    }

    /// Music only (`getListenTogetherTrackDuration`).
    pub fn duration(&self) -> Option<f64> {
        (!self.is_radio()).then_some(self.duration_ms).flatten()
    }
}

/// Who queued an entry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtRequester {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub avatar_display: Option<String>,
}

/// One queue entry.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtEntry {
    #[serde(default)]
    pub entry_id: String,
    #[serde(default)]
    pub requested_at: f64,
    #[serde(default)]
    pub track: LtTrack,
    #[serde(default)]
    pub requester: LtRequester,
    #[serde(default)]
    pub import_batch_id: Option<String>,
    #[serde(default)]
    pub import_batch_label: Option<String>,
}

/// The room's Listen Together state (`ListenTogetherStateSnapshot`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtSnapshot {
    #[serde(default)]
    pub room_slug: String,
    #[serde(default)]
    pub revision: i64,
    #[serde(default = "default_true")]
    pub paused: bool,
    #[serde(default)]
    pub current_entry_id: Option<String>,
    #[serde(default)]
    pub anchor_position_ms: f64,
    #[serde(default)]
    pub anchor_updated_at: Option<f64>,
    #[serde(default)]
    pub last_updated_at: f64,
    #[serde(default)]
    pub queue: Vec<LtEntry>,
    #[serde(default)]
    pub current_entry: Option<LtEntry>,
    /// Finished entries, newest first (the room keeps
    /// [`RECENTLY_PLAYED_LIMIT`]).
    #[serde(default, deserialize_with = "recently_played")]
    pub recently_played: Vec<LtHistory>,
    #[serde(default)]
    pub position_ms: f64,
    #[serde(default)]
    pub duration_ms: Option<f64>,
}

fn default_true() -> bool {
    true
}

/// One finished entry (`ListenTogetherRecentlyPlayedEntry`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LtHistory {
    pub history_id: String,
    #[serde(default)]
    pub played_at: f64,
    pub entry: LtEntry,
}

/// The room's `recentlyPlayed`: entries that do not parse are dropped (as
/// `parseRecentlyPlayed` filters them), at most [`RECENTLY_PLAYED_LIMIT`].
fn recently_played<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<LtHistory>, D::Error> {
    let value = Value::deserialize(de)?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| serde_json::from_value::<LtHistory>(item.clone()).ok())
        .filter(|item| !item.history_id.is_empty())
        .take(RECENTLY_PLAYED_LIMIT)
        .collect())
}

/// `LISTEN_TOGETHER_RECENTLY_PLAYED_LIMIT`.
pub const RECENTLY_PLAYED_LIMIT: usize = 50;

/// `LISTEN_TOGETHER_IMPORT_LIMIT`: the most entries one enqueue (and one
/// collection import) carries.
pub const IMPORT_LIMIT: usize = 100;

/// `MAX_RADIO_STATION_LOOKUPS_PER_ENQUEUE`.
pub const MAX_RADIO_LOOKUPS_PER_ENQUEUE: usize = 5;

/// `RADIO_STATION_UUID_PATTERN`: a radio-browser station uuid (RFC 4122,
/// versions 1-5).
pub fn is_station_uuid(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (index, byte) in bytes.iter().enumerate() {
        let ok = match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            14 => (b'1'..=b'5').contains(byte),
            19 => b"89abAB".contains(byte),
            _ => byte.is_ascii_hexdigit(),
        };
        if !ok {
            return false;
        }
    }
    true
}

/// `sanitizePublicHttpsUrl` (`radio-station-resolver.ts`): https, no
/// credentials, not a local name or a private IPv4 literal, no IPv6
/// literal. The room only relays radio stream and artwork URLs that pass it,
/// and nothing else is fetched for a radio entry.
pub fn sanitize_public_https_url(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 2048 {
        return None;
    }
    let url = url::Url::parse(value).ok()?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return None;
    }
    let host = match url.host()? {
        url::Host::Domain(domain) => domain.to_ascii_lowercase(),
        url::Host::Ipv4(ip) => {
            let [first, second, ..] = ip.octets();
            let private = first == 0
                || first == 10
                || first == 127
                || (first == 169 && second == 254)
                || (first == 172 && (16..=31).contains(&second))
                || (first == 192 && second == 168);
            if private {
                return None;
            }
            return Some(url.to_string());
        }
        url::Host::Ipv6(_) => return None,
    };
    if host == "localhost" || host.ends_with(".localhost") || host.ends_with(".local") {
        return None;
    }
    Some(url.to_string())
}

/// `listen_together.*` from the room.
#[derive(Debug, Clone, PartialEq)]
pub enum LtEvent {
    /// `snapshot`, `queue.updated` or `playback.updated`: each carries the
    /// whole state.
    Snapshot {
        room_slug: String,
        snapshot: Box<LtSnapshot>,
    },
    Error {
        room_slug: String,
        code: String,
        message: String,
    },
}

// ── Activities and Wordle ─────────────────────────────────────────────────

/// `wordle.progress`'s `progress`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordleProgress {
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub guesses: Vec<String>,
    #[serde(default)]
    pub streak: u32,
    #[serde(default)]
    pub finished: bool,
    #[serde(default)]
    pub missed: bool,
}

/// The activities a call can run (`ACTIVITY_TYPES`).
pub const ACTIVITIES: [&str; 2] = ["wordle", "warp-rush"];

// ── Inbound ───────────────────────────────────────────────────────────────

/// An op 106 the room broadcast.
#[derive(Debug, Clone, PartialEq)]
pub enum AppEvent {
    SoundboardPlay(Box<SoundboardPlay>),
    /// One playback, or every playback of `owner_id`.
    SoundboardStop {
        server_key: String,
        playback_id: Option<String>,
        owner_id: Option<String>,
    },
    SoundboardPauseSet {
        server_key: String,
        playback_id: String,
        paused: bool,
    },
    SoundboardVolumeSet {
        server_key: String,
        playback_id: String,
        volume: f64,
    },
    SoundboardCatalogUpdated {
        server_key: String,
        sound: CatalogSound,
    },
    SoundboardServerMuteSet {
        server_key: String,
        target_user_id: String,
        muted: bool,
    },
    ListenTogether(LtEvent),
    ActivityStart {
        user_id: String,
        channel_id: String,
        activity: String,
        started_at: Option<f64>,
    },
    ActivityLeave {
        user_id: String,
    },
    WordleProgress {
        channel_id: String,
        puzzle_date: String,
        progress: WordleProgress,
    },
    /// Who watches whose share (`stream.watch.snapshot`): streamer user id
    /// → viewer user ids, deduplicated, empty lists dropped
    /// (stream-watchers.ts `applyStreamWatcherSnapshot`).
    StreamWatchSnapshot(std::collections::BTreeMap<String, Vec<String>>),
    /// A floating sticker reaction (`reaction.sticker`, as the voice room
    /// re-broadcasts it sanitized).
    Sticker(Sticker),
}

/// A sticker reaction (StickerReactionsOverlay.tsx).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sticker {
    pub url: String,
    /// `image/gif`, `image/webp`, ... or `video/mp4`.
    pub content_type: String,
    /// `single`, `burst`, `rain` or `bounce`.
    pub display_mode: String,
    pub participant_id: String,
    /// The sender; absent when the room could not look it up.
    pub user_id: Option<String>,
}

/// The display modes the voice room relays (anything else is `single`).
pub const STICKER_MODES: [&str; 4] = ["single", "burst", "rain", "bounce"];

impl Sticker {
    /// The frame we send (GifPickerModal.tsx voice mode); the room adds the
    /// sender and the time.
    pub fn frame(url: &str, content_type: &str, display_mode: &str) -> String {
        let mode = if STICKER_MODES.contains(&display_mode) {
            display_mode
        } else {
            "single"
        };
        let content_type = if content_type.is_empty() {
            "image/gif"
        } else {
            content_type
        };
        serde_json::json!({ "op": crate::GatewayOp::VoiceAppEvent.code(), "d": {
            "type": "reaction.sticker", "url": url, "contentType": content_type,
            "displayMode": mode,
        }})
        .to_string()
    }

    fn decode(d: &Value) -> Option<Self> {
        let url = str_field(d, "url").filter(|u| !u.is_empty())?;
        // Older events carry no type: infer it from the URL as the web does.
        let content_type = str_field(d, "contentType")
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| {
                if url.contains(".mp4") || url.contains("/clips/") {
                    "video/mp4".to_owned()
                } else {
                    "image/gif".to_owned()
                }
            });
        let display_mode = str_field(d, "displayMode")
            .filter(|m| STICKER_MODES.contains(&m.as_str()))
            .unwrap_or_else(|| "single".to_owned());
        Some(Self {
            url,
            content_type,
            display_mode,
            participant_id: str_field(d, "participant_id").unwrap_or_default(),
            user_id: str_field(d, "user_id").filter(|u| !u.is_empty()),
        })
    }
}

impl AppEvent {
    /// Reads a broadcast `d`. `None` for other events (stream watching,
    /// demo chat) and for events missing what the web requires.
    pub fn decode(d: &Value) -> Option<Self> {
        let kind = d.get("type")?.as_str()?;
        if let Some(rest) = kind.strip_prefix("soundboard.") {
            return decode_soundboard(rest, d);
        }
        if kind.starts_with("listen_together.") {
            return decode_listen_together(kind, d).map(Self::ListenTogether);
        }
        match kind {
            "reaction.sticker" => Sticker::decode(d).map(Self::Sticker),
            "activity.start" => {
                let activity = str_field(d, "activity")?;
                let user_id = str_field(d, "userId")?;
                ACTIVITIES
                    .contains(&activity.as_str())
                    .then(|| Self::ActivityStart {
                        user_id,
                        channel_id: str_field(d, "channelId").unwrap_or_default(),
                        activity,
                        started_at: num_field(d, "startedAt"),
                    })
            }
            "activity.leave" => Some(Self::ActivityLeave {
                user_id: str_field(d, "userId")?,
            }),
            "stream.watch.snapshot" => {
                let map = d.get("watchers_by_streamer")?.as_object()?;
                let mut watchers = std::collections::BTreeMap::new();
                for (streamer, viewers) in map {
                    let mut ids: Vec<String> = Vec::new();
                    for id in viewers
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                    {
                        if !id.is_empty() && !ids.iter().any(|known| known == id) {
                            ids.push(id.to_owned());
                        }
                    }
                    if !ids.is_empty() {
                        watchers.insert(streamer.clone(), ids);
                    }
                }
                Some(Self::StreamWatchSnapshot(watchers))
            }
            "wordle.progress" => {
                let progress = d.get("progress")?;
                Some(Self::WordleProgress {
                    channel_id: str_field(d, "channel_id")?,
                    puzzle_date: str_field(d, "puzzle_date")?,
                    progress: read_progress(progress)?,
                })
            }
            _ => None,
        }
    }
}

/// `normalizeProgress`: a record with a user id; guesses that are not
/// five letters are dropped.
fn read_progress(value: &Value) -> Option<WordleProgress> {
    let user_id = str_field(value, "userId").filter(|id| !id.is_empty())?;
    let guesses = value
        .get("guesses")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .filter(|guess| guess.len() == 5 && guess.bytes().all(|b| b.is_ascii_alphabetic()))
        .map(str::to_ascii_lowercase)
        .take(6)
        .collect();
    Some(WordleProgress {
        user_id,
        guesses,
        streak: value
            .get("streak")
            .and_then(Value::as_u64)
            .map_or(0, |s| u32::try_from(s).unwrap_or(u32::MAX)),
        finished: bool_field(value, "finished").unwrap_or(false),
        missed: bool_field(value, "missed").unwrap_or(false),
    })
}

fn owner_of(d: &Value) -> Option<String> {
    str_field(d, "user_id").or_else(|| str_field(d, "participant_id"))
}

fn decode_soundboard(kind: &str, d: &Value) -> Option<AppEvent> {
    let server_key = str_field(d, "server_key")?;
    Some(match kind {
        "play" => AppEvent::SoundboardPlay(Box::new(SoundboardPlay {
            owner_id: owner_of(d)?,
            playback_id: str_field(d, "playback_id")?,
            name: str_field(d, "name").unwrap_or_else(|| "Sound".into()),
            sound_id: str_field(d, "sound_id"),
            source_server_id: str_field(d, "source_server_id"),
            media_url: str_field(d, "media_url"),
            data_url: str_field(d, "data_url"),
            volume: num_field(d, "volume"),
            current_time: num_field(d, "current_time").filter(|t| *t >= 0.0),
            paused: bool_field(d, "paused"),
            automatic_event: AutomaticEvent::read(d),
            max_duration_seconds: num_field(d, "max_duration_seconds"),
            sent_at: d.get("sent_at").and_then(Value::as_i64),
            server_key,
        })),
        "stop" => {
            let playback_id = str_field(d, "playback_id");
            let owner_id = if playback_id.is_some() {
                None
            } else {
                Some(owner_of(d)?)
            };
            AppEvent::SoundboardStop {
                server_key,
                playback_id,
                owner_id,
            }
        }
        "pause-set" => AppEvent::SoundboardPauseSet {
            server_key,
            playback_id: str_field(d, "playback_id")?,
            paused: bool_field(d, "paused")?,
        },
        "volume-set" => AppEvent::SoundboardVolumeSet {
            server_key,
            playback_id: str_field(d, "playback_id")?,
            volume: clamp01(num_field(d, "volume")?),
        },
        "catalog-updated" => AppEvent::SoundboardCatalogUpdated {
            server_key,
            sound: serde_json::from_value(d.get("sound")?.clone()).ok()?,
        },
        "server-mute-set" => AppEvent::SoundboardServerMuteSet {
            server_key,
            target_user_id: str_field(d, "target_user_id")?,
            muted: bool_field(d, "muted")?,
        },
        _ => return None,
    })
}

fn decode_listen_together(kind: &str, d: &Value) -> Option<LtEvent> {
    let room_slug = str_field(d, "room_slug").unwrap_or_default();
    match kind {
        "listen_together.snapshot"
        | "listen_together.queue.updated"
        | "listen_together.playback.updated" => Some(LtEvent::Snapshot {
            room_slug,
            snapshot: Box::new(serde_json::from_value(d.get("snapshot")?.clone()).ok()?),
        }),
        "listen_together.error" => Some(LtEvent::Error {
            room_slug,
            code: str_field(d, "code").unwrap_or_default(),
            message: str_field(d, "message").unwrap_or_default(),
        }),
        _ => None,
    }
}

// ── Outbound ──────────────────────────────────────────────────────────────

/// Where an enqueue puts its entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnqueueMode {
    Append,
    PlayNext,
    PlayNow,
}

impl EnqueueMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Append => "append",
            Self::PlayNext => "play-next",
            Self::PlayNow => "play-now",
        }
    }
}

/// A queue entry as an enqueue sends it (`ListenTogetherQueueEntrySeed`).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LtSeed {
    pub track: LtTrack,
    pub requester: LtRequester,
    /// One id for every entry of a collection import, `None` otherwise.
    pub import_batch_id: Option<String>,
    /// The collection's title.
    pub import_batch_label: Option<String>,
}

impl LtSeed {
    /// A radio station as the soundboard queues it: the room looks the
    /// station up and fills in the rest (`broadcastSound`, `kind: "radio"`).
    pub fn radio(station_uuid: &str, title: &str, requester: LtRequester) -> Self {
        Self {
            track: LtTrack {
                kind: "radio".into(),
                id: station_uuid.to_owned(),
                provider: "radio".into(),
                title: title.to_owned(),
                station_uuid: Some(station_uuid.to_owned()),
                ..LtTrack::default()
            },
            requester,
            import_batch_id: None,
            import_batch_label: None,
        }
    }
}

/// What we send on op 106.
#[derive(Debug, Clone, PartialEq)]
pub enum AppCommand {
    SoundboardPlay(Box<SoundboardPlay>),
    SoundboardStop {
        server_key: String,
        playback_id: String,
    },
    SoundboardPauseSet {
        server_key: String,
        playback_id: String,
        paused: bool,
    },
    SoundboardVolumeSet {
        server_key: String,
        playback_id: String,
        volume: f64,
    },
    SoundboardCatalogUpdated {
        server_key: String,
        sound: CatalogSound,
    },
    LtStateRequest {
        room_slug: String,
    },
    LtEnqueue {
        room_slug: String,
        mode: EnqueueMode,
        entries: Vec<LtSeed>,
    },
    /// `entry_id` `None`: the current entry (or the first).
    LtPlay {
        room_slug: String,
        entry_id: Option<String>,
    },
    LtPause {
        room_slug: String,
        paused: bool,
        entry_id: String,
    },
    LtSeek {
        room_slug: String,
        position_ms: f64,
        entry_id: String,
    },
    LtSkip {
        room_slug: String,
        entry_id: String,
    },
    LtRemove {
        room_slug: String,
        entry_id: String,
    },
    LtClear {
        room_slug: String,
    },
    ActivityStart {
        user_id: String,
        channel_id: String,
        activity: String,
        started_at: i64,
    },
    ActivityLeave {
        user_id: String,
        channel_id: String,
    },
    WordleProgress {
        channel_id: String,
        puzzle_date: String,
        /// The display name the web sends along (the room drops it).
        name: String,
        progress: WordleProgress,
    },
}

/// What the room knows about the sender, for [`AppCommand::checked`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Sender {
    pub participant_id: String,
    pub user_id: Option<String>,
    /// The server the voice admission is for; `None` in a DM call.
    pub server_id: Option<String>,
    /// The room slug (`voice-<server>-<channel>` or `dm-call-…`).
    pub room_slug: String,
}

impl AppCommand {
    /// The `d` the web sends.
    pub fn payload(&self, user_id: &str) -> Value {
        match self {
            Self::SoundboardPlay(play) => {
                let mut d = Map::new();
                d.insert("type".into(), json!("soundboard.play"));
                d.insert("server_key".into(), json!(play.server_key));
                d.insert("user_id".into(), json!(user_id));
                d.insert("playback_id".into(), json!(play.playback_id));
                if let Some(sound_id) = &play.sound_id {
                    d.insert("sound_id".into(), json!(sound_id));
                }
                if let Some(event) = play.automatic_event {
                    d.insert("automatic_event".into(), json!(event.as_str()));
                }
                if let Some(max) = play.max_duration_seconds {
                    d.insert("max_duration_seconds".into(), json!(max));
                }
                d.insert("name".into(), json!(play.name));
                if let Some(source) = &play.source_server_id {
                    d.insert("source_server_id".into(), json!(source));
                }
                if let Some(url) = &play.media_url {
                    d.insert("media_url".into(), json!(url));
                } else if let Some(url) = &play.data_url {
                    d.insert("data_url".into(), json!(url));
                }
                if let Some(time) = play.current_time {
                    d.insert("current_time".into(), json!(time));
                }
                if let Some(paused) = play.paused {
                    d.insert("paused".into(), json!(paused));
                }
                if let Some(volume) = play.volume {
                    d.insert("volume".into(), json!(volume));
                }
                Value::Object(d)
            }
            Self::SoundboardStop {
                server_key,
                playback_id,
            } => json!({
                "type": "soundboard.stop", "server_key": server_key,
                "user_id": user_id, "playback_id": playback_id,
            }),
            Self::SoundboardPauseSet {
                server_key,
                playback_id,
                paused,
            } => json!({
                "type": "soundboard.pause-set", "server_key": server_key,
                "user_id": user_id, "playback_id": playback_id, "paused": paused,
            }),
            Self::SoundboardVolumeSet {
                server_key,
                playback_id,
                volume,
            } => json!({
                "type": "soundboard.volume-set", "server_key": server_key,
                "user_id": user_id, "playback_id": playback_id, "volume": volume,
            }),
            Self::SoundboardCatalogUpdated { server_key, sound } => json!({
                "type": "soundboard.catalog-updated", "server_key": server_key,
                "user_id": user_id, "sound": sound,
            }),
            Self::LtStateRequest { room_slug } => {
                json!({ "type": "listen_together.state.request", "room_slug": room_slug })
            }
            Self::LtEnqueue {
                room_slug,
                mode,
                entries,
            } => json!({
                "type": "listen_together.enqueue", "room_slug": room_slug,
                "mode": mode.as_str(), "entries": entries,
            }),
            Self::LtPlay {
                room_slug,
                entry_id,
            } => json!({
                "type": "listen_together.play", "room_slug": room_slug, "entryId": entry_id,
            }),
            Self::LtPause {
                room_slug,
                paused,
                entry_id,
            } => json!({
                "type": "listen_together.pause", "room_slug": room_slug,
                "paused": paused, "entryId": entry_id,
            }),
            Self::LtSeek {
                room_slug,
                position_ms,
                entry_id,
            } => json!({
                "type": "listen_together.seek", "room_slug": room_slug,
                "positionMs": position_ms.max(0.0).floor(), "entryId": entry_id,
            }),
            Self::LtSkip {
                room_slug,
                entry_id,
            } => json!({
                "type": "listen_together.skip", "room_slug": room_slug, "entryId": entry_id,
            }),
            Self::LtRemove {
                room_slug,
                entry_id,
            } => json!({
                "type": "listen_together.remove", "room_slug": room_slug, "entryId": entry_id,
            }),
            Self::LtClear { room_slug } => {
                json!({ "type": "listen_together.clear", "room_slug": room_slug })
            }
            Self::ActivityStart {
                user_id,
                channel_id,
                activity,
                started_at,
            } => json!({
                "type": "activity.start", "userId": user_id, "channelId": channel_id,
                "activity": activity, "startedAt": started_at,
            }),
            Self::ActivityLeave {
                user_id,
                channel_id,
            } => json!({
                "type": "activity.leave", "userId": user_id, "channelId": channel_id,
            }),
            Self::WordleProgress {
                channel_id,
                puzzle_date,
                name,
                progress,
            } => json!({
                "type": "wordle.progress", "channel_id": channel_id,
                "puzzle_date": puzzle_date,
                "progress": {
                    "userId": progress.user_id,
                    "name": name.chars().take(80).collect::<String>(),
                    "avatar": Value::Null,
                    "guesses": progress.guesses,
                    "streak": progress.streak,
                    "finished": progress.finished,
                    "missed": progress.missed,
                },
            }),
        }
    }

    /// The op 106 frame.
    pub fn frame(&self, user_id: &str) -> String {
        Frame::new(GatewayOp::VoiceAppEvent, self.payload(user_id)).encode()
    }

    /// The frame, if the room would accept it (its checks without the
    /// database); `Err` says why not.
    pub fn checked(&self, sender: &Sender, now: i64) -> Result<String, &'static str> {
        let user_id = sender.user_id.as_deref().unwrap_or_default();
        let payload = self.payload(user_id);
        let kind = payload.get("type").and_then(Value::as_str).unwrap_or("");
        if let Some(result) = sanitize_activity(&payload, user_id, &sender.participant_id, now) {
            return result.map(|_| self.frame(user_id));
        }
        if kind.starts_with("listen_together.") {
            return lt_command(&payload, &sender.room_slug)
                .map(|_| self.frame(user_id))
                .ok_or("Listen Together command is invalid");
        }
        sanitize_soundboard(kind, &payload, sender)
            .map(|_| self.frame(user_id))
            .ok_or("Soundboard event was rejected")
    }
}

// ── The room's checks ─────────────────────────────────────────────────────

/// `sanitizeSoundboardDataUrl`: `data:audio/<type>;base64,<canonical base64>`
/// of at most `max_bytes` decoded bytes.
pub fn sanitize_data_url(value: &str, max_bytes: usize) -> bool {
    let Some(rest) = value.strip_prefix("data:audio/") else {
        return false;
    };
    let Some((subtype, encoded)) = rest.split_once(";base64,") else {
        return false;
    };
    if subtype.is_empty()
        || !subtype
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"+.-".contains(&b))
    {
        return false;
    }
    if encoded.is_empty() || encoded.len() % 4 != 0 {
        return false;
    }
    let content = encoded.trim_end_matches('=');
    let padding = encoded.len() - content.len();
    if padding > 2
        || !content
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'+' || b == b'/')
    {
        return false;
    }
    let expected = (4 - content.len() % 4) % 4;
    if content.len() % 4 == 1 || padding != expected {
        return false;
    }
    // Canonical: the unused low bits of the last symbol are zero.
    let last = content.bytes().last().map_or(0, base64_value);
    let canonical = match padding {
        1 => last & 0b11 == 0,
        2 => last & 0b1111 == 0,
        _ => true,
    };
    canonical && content.len() * 3 / 4 <= max_bytes
}

fn base64_value(b: u8) -> u8 {
    match b {
        b'A'..=b'Z' => b - b'A',
        b'a'..=b'z' => b - b'a' + 26,
        b'0'..=b'9' => b - b'0' + 52,
        b'+' => 62,
        _ => 63,
    }
}

/// The largest `data_url` clip the room relays (`MAX_SOUNDBOARD_DATA_URL_BYTES`).
pub const MAX_DATA_URL_BYTES: usize = 128 * 1024;

/// `data:<mime>;base64,<bytes>` with canonical padded base64, as
/// `FileReader.readAsDataURL` writes it.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(mime.len() + 13 + bytes.len().div_ceil(3) * 4);
    out.push_str("data:");
    out.push_str(mime);
    out.push_str(";base64,");
    for chunk in bytes.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for (index, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if index <= chunk.len() {
                out.push(char::from(ALPHABET[((n >> shift) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `normalizeSoundboardUploadUrl` and the MyInstants rule of
/// `sanitizeSoundboardMediaUrl`.
pub fn sanitize_media_url(value: &str) -> Option<String> {
    if value.is_empty() || value.len() > 2048 {
        return None;
    }
    if value.starts_with('/') {
        if value.starts_with("//") || !value.starts_with(SOUNDBOARD_UPLOAD_PATH) {
            return None;
        }
        if value.contains('?') || value.contains('#') {
            return None;
        }
        let id = &value[SOUNDBOARD_UPLOAD_PATH.len()..];
        if id.is_empty() || id.contains('/') || id.contains('%') {
            return None;
        }
        return Some(value.to_owned());
    }
    let rest = value.strip_prefix("https://www.myinstants.com/media/sounds/")?;
    if rest.contains('?') || rest.contains('#') || rest.contains('@') {
        return None;
    }
    Some(value.to_owned())
}

/// A `media_url` as it arrives from the room: [`sanitize_media_url`], except
/// that a DM call's upload path carries the room's capability query
/// (`?cap=...&room_slug=...&playback_id=...`, `getSoundboardMediaCapabilityUrl`).
/// Nothing else is allowed after the path.
pub fn sanitize_inbound_media_url(value: &str) -> Option<String> {
    let Some((path, query)) = value.split_once('?') else {
        return sanitize_media_url(value);
    };
    if value.len() > 2048 || query.is_empty() || query.contains('#') {
        return None;
    }
    // Only upload paths take a capability (MyInstants never does).
    if !path.starts_with(SOUNDBOARD_UPLOAD_PATH) {
        return None;
    }
    sanitize_media_url(path)?;
    let mut seen = [false; 3];
    for pair in query.split('&') {
        let (key, val) = pair.split_once('=')?;
        let slot = ["cap", "room_slug", "playback_id"]
            .iter()
            .position(|name| *name == key)?;
        if std::mem::replace(&mut seen[slot], true) || val.is_empty() {
            return None;
        }
        // URLSearchParams output: unreserved characters, `+` and `%XX`.
        let clean = val
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-._~*+%".contains(&b));
        if !clean {
            return None;
        }
    }
    Some(value.to_owned())
}

/// `sanitizeSoundboardEvent` without its database checks (membership, the
/// catalog, Manage server): the event the room would broadcast, or `None`.
pub fn sanitize_soundboard(kind: &str, payload: &Value, sender: &Sender) -> Option<Value> {
    let server_key = trimmed(payload, "server_key", 160);
    let dm = sender.server_id.is_none() && sender.room_slug.starts_with("dm-call-");
    let key_ok = if dm {
        server_key == "dm-call"
    } else {
        sender.server_id.as_deref() == Some(server_key.as_str())
    };
    if server_key.is_empty() || !key_ok {
        return None;
    }
    let mut base = Map::new();
    base.insert("type".into(), json!(kind));
    base.insert("participant_id".into(), json!(sender.participant_id));
    if let Some(user) = &sender.user_id {
        base.insert("user_id".into(), json!(user));
    }
    base.insert("server_key".into(), json!(server_key));
    let user_id = sender.user_id.as_deref();
    match kind {
        "soundboard.play" => {
            let playback_id = trimmed(payload, "playback_id", 160);
            let sound_id = trimmed(payload, "sound_id", 160);
            let name = trimmed(payload, "name", 160);
            let source_server_id = trimmed(payload, "source_server_id", 160);
            let data_url = payload
                .get("data_url")
                .and_then(Value::as_str)
                .filter(|url| sanitize_data_url(url, MAX_DATA_URL_BYTES));
            let media_url = payload
                .get("media_url")
                .and_then(Value::as_str)
                .and_then(sanitize_media_url);
            let has_data_input = payload.get("data_url").is_some();
            let user_id = user_id?;
            if playback_id.is_empty() || name.is_empty() {
                return None;
            }
            if dm {
                let default_ok = !sound_id.is_empty()
                    && source_server_id.is_empty()
                    && data_url.is_none()
                    && media_url.is_none()
                    && is_default_sound(&sound_id);
                let upload_ok = !sound_id.is_empty()
                    && !source_server_id.is_empty()
                    && data_url.is_none()
                    && media_url
                        .as_deref()
                        .is_some_and(|url| url.starts_with(SOUNDBOARD_UPLOAD_PATH));
                if !default_ok && !upload_ok {
                    return None;
                }
            }
            if (has_data_input && data_url.is_none() && media_url.is_none())
                || (sound_id.is_empty() && data_url.is_none() && media_url.is_none())
                || (!sound_id.is_empty()
                    && data_url.is_none()
                    && media_url.is_none()
                    && !is_default_sound(&sound_id))
            {
                return None;
            }
            if !is_playback_owned_by(&playback_id, user_id) {
                return None;
            }
            if let Some(url) = media_url
                .as_deref()
                .filter(|url| url.starts_with(SOUNDBOARD_UPLOAD_PATH))
            {
                if sound_id.is_empty() || url != upload_url(&sound_id) {
                    return None;
                }
            }
            base.insert("playback_id".into(), json!(playback_id));
            if !sound_id.is_empty() {
                base.insert("sound_id".into(), json!(sound_id));
            }
            if dm && !source_server_id.is_empty() {
                base.insert("source_server_id".into(), json!(source_server_id));
            }
            if let Some(event) = AutomaticEvent::read(payload) {
                base.insert("automatic_event".into(), json!(event.as_str()));
            }
            if let Some(max) = num_field(payload, "max_duration_seconds") {
                base.insert(
                    "max_duration_seconds".into(),
                    json!(max.clamp(0.0, MAX_AUTOMATIC_SECONDS)),
                );
            }
            base.insert("name".into(), json!(name));
            if let Some(url) = media_url {
                base.insert("media_url".into(), json!(url));
            } else if let Some(url) = data_url {
                base.insert("data_url".into(), json!(url));
            }
            if let Some(time) = num_field(payload, "current_time")
                .filter(|t| (0.0..=MAX_POSITION_SECONDS).contains(t))
            {
                base.insert("current_time".into(), json!(time));
            }
            if let Some(paused) = bool_field(payload, "paused") {
                base.insert("paused".into(), json!(paused));
            }
            let volume = num_field(payload, "volume").map_or(1.0, clamp01);
            base.insert("volume".into(), json!(volume));
            Some(Value::Object(base))
        }
        "soundboard.catalog-updated" => {
            if dm || user_id.is_none() {
                return None;
            }
            let sound = payload.get("sound").filter(|s| s.is_object())?;
            let id = trimmed(sound, "id", 160);
            if id.is_empty() {
                return None;
            }
            // The room fills name, emoji and volume from its catalog.
            base.insert(
                "sound".into(),
                json!({ "id": id, "file_url": upload_url(&id) }),
            );
            Some(Value::Object(base))
        }
        "soundboard.stop" | "soundboard.pause-set" | "soundboard.volume-set" => {
            let playback_id = trimmed(payload, "playback_id", 160);
            let owns = user_id.is_some_and(|user| is_playback_owned_by(&playback_id, user));
            if !owns || playback_id.is_empty() {
                return None;
            }
            base.insert("playback_id".into(), json!(playback_id));
            match kind {
                "soundboard.pause-set" => {
                    base.insert("paused".into(), json!(bool_field(payload, "paused")?));
                }
                "soundboard.volume-set" => {
                    if dm {
                        return None;
                    }
                    base.insert(
                        "volume".into(),
                        json!(clamp01(num_field(payload, "volume")?)),
                    );
                }
                _ => {}
            }
            Some(Value::Object(base))
        }
        _ => None,
    }
}

const MAX_ACTIVITY_CHANNEL_ID_LENGTH: usize = 200;
const MAX_ACTIVITY_TIME_SKEW_MS: i64 = 5 * 60 * 1000;

/// `isValidDateKey`: a real `YYYY-MM-DD` day.
pub fn is_date_key(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| -> Option<u32> {
        let part = value.get(range)?;
        part.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| part.parse().ok())
            .flatten()
    };
    let (Some(year), Some(month), Some(day)) = (digits(0..4), digits(5..7), digits(8..10)) else {
        return false;
    };
    let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}

/// `sanitizeVoiceAppEvent` (activities and Wordle): `None` for other
/// events, `Err` with the room's message for a malformed one.
pub fn sanitize_activity(
    input: &Value,
    sender_user_id: &str,
    participant_id: &str,
    now: i64,
) -> Option<Result<Value, &'static str>> {
    let kind = input.get("type").and_then(Value::as_str)?;
    let channel = |key: &str| -> Option<String> {
        let id = input.get(key)?.as_str()?;
        (!id.trim().is_empty() && id.chars().count() <= MAX_ACTIVITY_CHANNEL_ID_LENGTH)
            .then(|| id.trim().to_owned())
    };
    match kind {
        "activity.start" => {
            let activity = input.get("activity").and_then(Value::as_str);
            let started = num_field(input, "startedAt");
            Some(match (channel("channelId"), activity, started) {
                (Some(channel_id), Some(activity), Some(started))
                    if ACTIVITIES.contains(&activity) =>
                {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "Math.trunc of a finite timestamp"
                    )]
                    let started = (started.trunc() as i64)
                        .max(now - MAX_ACTIVITY_TIME_SKEW_MS)
                        .min(now + MAX_ACTIVITY_TIME_SKEW_MS);
                    Ok(json!({
                        "type": "activity.start", "userId": sender_user_id,
                        "participant_id": participant_id, "channelId": channel_id,
                        "activity": activity, "startedAt": started,
                    }))
                }
                _ => Err("Invalid activity.start payload"),
            })
        }
        "activity.leave" => Some(match channel("channelId") {
            Some(channel_id) => Ok(json!({
                "type": "activity.leave", "userId": sender_user_id,
                "participant_id": participant_id, "channelId": channel_id,
            })),
            None => Err("Invalid activity.leave payload"),
        }),
        "wordle.progress" => Some(sanitize_wordle(input, sender_user_id, participant_id)),
        _ => None,
    }
}

fn sanitize_wordle(
    input: &Value,
    sender_user_id: &str,
    participant_id: &str,
) -> Result<Value, &'static str> {
    const INVALID: &str = "Invalid wordle.progress payload";
    let channel_id = input
        .get("channel_id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty() && id.chars().count() <= MAX_ACTIVITY_CHANNEL_ID_LENGTH)
        .ok_or(INVALID)?;
    let date = input
        .get("puzzle_date")
        .and_then(Value::as_str)
        .filter(|d| is_date_key(d))
        .ok_or(INVALID)?;
    let progress = input
        .get("progress")
        .and_then(Value::as_object)
        .ok_or(INVALID)?;
    let guesses = progress
        .get("guesses")
        .and_then(Value::as_array)
        .ok_or(INVALID)?;
    let words: Option<Vec<String>> = guesses
        .iter()
        .map(|g| {
            g.as_str()
                .filter(|g| g.len() == 5 && g.bytes().all(|b| b.is_ascii_alphabetic()))
                .map(str::to_ascii_lowercase)
        })
        .collect();
    let words = words.filter(|w| w.len() <= 6).ok_or(INVALID)?;
    let name_ok = match progress.get("name") {
        None => true,
        Some(Value::String(name)) => name.encode_utf16().count() <= 80,
        Some(_) => false,
    };
    let avatar_ok = match progress.get("avatar") {
        None | Some(Value::Null) => true,
        Some(Value::String(avatar)) => avatar.encode_utf16().count() <= 2048,
        Some(_) => false,
    };
    let streak = progress
        .get("streak")
        .and_then(Value::as_f64)
        .filter(|s| s.fract() == 0.0 && (0.0..=10_000.0).contains(s))
        .ok_or(INVALID)?;
    let finished = progress.get("finished").and_then(Value::as_bool);
    let missed = progress.get("missed").and_then(Value::as_bool);
    let (true, true, Some(finished), Some(missed)) = (name_ok, avatar_ok, finished, missed) else {
        return Err(INVALID);
    };
    Ok(json!({
        "type": "wordle.progress", "channel_id": channel_id.trim(), "puzzle_date": date,
        "progress": {
            "userId": sender_user_id, "participant_id": participant_id,
            "name": Value::Null, "avatar": Value::Null, "guesses": words,
            "streak": streak, "finished": finished, "missed": missed,
        },
    }))
}

/// `asListenTogetherCommand`: the command the room would run, with the
/// room slug filled in, or `None` when it answers `INVALID_COMMAND`.
pub fn lt_command(value: &Value, room_slug: &str) -> Option<Value> {
    let kind = value.get("type")?.as_str()?;
    let slug = str_field(value, "room_slug").unwrap_or_else(|| room_slug.to_owned());
    if !kind.starts_with("listen_together.") || slug.is_empty() {
        return None;
    }
    let entry = || value.get("entryId").and_then(Value::as_str);
    Some(match kind {
        "listen_together.state.request" | "listen_together.clear" => {
            json!({ "type": kind, "room_slug": slug })
        }
        "listen_together.enqueue" => {
            let mode = value
                .get("mode")
                .and_then(Value::as_str)
                .filter(|mode| matches!(*mode, "play-next" | "play-now"))
                .unwrap_or("append");
            let entries = value
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            json!({ "type": kind, "room_slug": slug, "mode": mode, "entries": entries })
        }
        "listen_together.play" => match value.get("entryId") {
            None => json!({ "type": kind, "room_slug": slug }),
            Some(Value::Null) => json!({ "type": kind, "room_slug": slug, "entryId": null }),
            Some(Value::String(id)) => json!({ "type": kind, "room_slug": slug, "entryId": id }),
            Some(_) => json!({ "type": kind, "room_slug": slug }),
        },
        "listen_together.pause" => json!({
            "type": kind, "room_slug": slug,
            "paused": bool_field(value, "paused")?, "entryId": entry()?,
        }),
        "listen_together.seek" => json!({
            "type": kind, "room_slug": slug,
            "positionMs": num_field(value, "positionMs")?, "entryId": entry()?,
        }),
        "listen_together.skip" | "listen_together.remove" => {
            json!({ "type": kind, "room_slug": slug, "entryId": entry()? })
        }
        _ => return None,
    })
}

/// `isValidListenTogetherVideoId`.
pub fn is_video_id(id: &str) -> bool {
    (6..=20).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// The music half of `sanitizeListenTogetherEnqueueEntries` for one track:
/// whether the room keeps it (radio needs the room's station lookup).
pub fn is_queueable_music(track: &LtTrack) -> bool {
    matches!(
        track.provider.as_str(),
        "youtube" | "youtube_music" | "spotify"
    ) && !track.is_radio()
        && track.video_id.as_deref().is_some_and(is_video_id)
        && !track.title.trim().is_empty()
        && track.duration_ms.is_some_and(|d| d.is_finite() && d > 0.0)
}

/// `isListenTogetherResolvableUrl`: a YouTube or Spotify link (resolve), not
/// a search.
pub fn is_resolvable_url(input: &str) -> bool {
    let input = input.trim();
    let Some((scheme, rest)) = input.split_once("://") else {
        return false;
    };
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return false;
    }
    let (host, path) = match rest.find(['/', '?', '#']) {
        Some(index) => (&rest[..index], &rest[index..]),
        None => (rest, ""),
    };
    let host = host
        .rsplit_once('@')
        .map_or(host, |(_, h)| h)
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    if path.is_empty() || path == "/" {
        return false;
    }
    matches!(
        host.as_str(),
        "youtube.com"
            | "www.youtube.com"
            | "m.youtube.com"
            | "music.youtube.com"
            | "youtu.be"
            | "open.spotify.com"
            | "play.spotify.com"
    )
}
