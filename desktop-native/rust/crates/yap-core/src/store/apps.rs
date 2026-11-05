//! Voice apps in a call: the soundboard (SoundboardPicker.tsx,
//! VoiceSoundboardManager.tsx, ActiveSoundboardEffectList.tsx,
//! auto-soundboard.ts, settings/SoundboardPanel.tsx), Listen Together
//! (ListenTogetherPanel.tsx, VoiceListenTogetherManager.tsx,
//! `src/lib/voice/listen-together-*.ts`) and activities with the Wordle
//! stage (VoiceAppsModal.tsx, WordleActivityStage.tsx).
//!
//! What the UI asks for ([`AppsCommand`]), the REST calls ([`AppsRequest`]),
//! what the local player is told ([`AudioCommand`]) and tells back
//! ([`AudioEvent`]), the state the store keeps ([`AppsState`]) and what the
//! UI shows ([`AppsView`]). The op 106 wire shapes are
//! `yap_protocol::voice_app`.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{json, Value};
use yap_protocol::voice_app::{
    is_default_sound, LtSnapshot, LtTrack, WordleProgress, DEFAULT_SOUNDS,
};

use super::Method;
use crate::api::ApiError;
use crate::wordle::{self, Mark};

/// `LISTEN_TOGETHER_DRIFT_TOLERANCE_MS`: a player further than this from
/// the room's clock seeks.
pub const DRIFT_TOLERANCE_MS: f64 = 750.0;

/// The picker's send volume before the user moves it (`useState(0.8)`).
pub const DEFAULT_SEND_VOLUME: f64 = 0.8;

/// The soundboard upload limit (`MAX_SOUNDBOARD_UPLOAD_BYTES`).
pub const MAX_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;

/// The scope a join or leave sound applies to when no server overrides it
/// (`ALL_SERVERS_SOUND_SCOPE`).
pub const ALL_SERVERS: &str = "*";

/// The audio a clip plays.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipSource {
    /// A built-in tone (`ping`, `pop`, `chime`, `tada`).
    Tone(String),
    /// A path on our API (`/api/soundboard/uploads/…`, fetched with the
    /// session) or an absolute https URL (MyInstants).
    Media(String),
    /// A `data:audio/…;base64,…` URL.
    Data(String),
}

/// What the local player does. Clips are keyed by `<server key>::<playback
/// id>` (`getPlaybackKey`); there is one Listen Together stream.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioCommand {
    Play {
        key: String,
        source: ClipSource,
        /// 0..=1 before the soundboard volume setting (the player applies
        /// that, as the web's master volume).
        volume: f64,
        /// Seconds into the clip.
        start_at: f64,
        paused: bool,
        max_seconds: Option<f64>,
        /// Previews and settings tests: never part of our speaking flag.
        preview: bool,
    },
    Stop(String),
    Pause {
        key: String,
        paused: bool,
    },
    Volume {
        key: String,
        volume: f64,
    },
    StopAll,
    /// Load the stream for queue entry `key` (replacing any other).
    StreamLoad {
        key: String,
        /// `/api/listen-together/stream?…` on our API.
        path: String,
        position_ms: f64,
        paused: bool,
    },
    StreamSeek(f64),
    StreamPause(bool),
    StreamStop,
    /// The Listen Together output volume (0..=1) and loudness preset.
    StreamAudio {
        volume: f64,
        loudness: bool,
        preset: String,
    },
}

/// What the local player reports.
#[derive(Debug, Clone, PartialEq)]
pub enum AudioEvent {
    /// A clip's first audible frame.
    Started(String),
    /// A clip finished, was stopped or failed.
    Ended(String),
    Failed {
        key: String,
        error: String,
    },
    /// The stream's position, stamped with the wall clock.
    StreamPosition {
        key: String,
        position_ms: f64,
        now_ms: i64,
    },
    StreamRetrying {
        key: String,
        message: String,
    },
    StreamFailed {
        key: String,
        code: String,
        message: String,
    },
    StreamEnded(String),
}

// ── Commands ──────────────────────────────────────────────────────────────

/// A join or leave sound (`voiceJoinSoundboard` / `voiceLeaveSoundboard`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Trigger {
    Join,
    Leave,
}

impl Trigger {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "join" => Some(Self::Join),
            "leave" => Some(Self::Leave),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::Join => "voiceJoinSoundboard",
            Self::Leave => "voiceLeaveSoundboard",
        }
    }
}

/// What the UI asks of the voice apps.
#[derive(Debug, Clone, PartialEq)]
pub enum AppsCommand {
    /// The device's sound settings (the store does not keep them):
    /// UI sounds on, their volume and the soundboard volume, 0..=100.
    LocalSettings {
        sounds_enabled: bool,
        sound_volume: u8,
        soundboard_volume: u8,
    },
    /// The picker opened: the call's server catalog loads.
    LoadSounds,
    /// A server's catalog (the settings chooser).
    LoadCatalog(String),
    /// Play a built-in tone or a catalog sound for everyone.
    Play(String),
    /// Hear a sound alone (`previewSound`).
    Preview(String),
    /// A playing row's controls: everyone's for our own playbacks, ours
    /// alone for the others'.
    StopPlayback(String),
    PausePlayback {
        playback_id: String,
        paused: bool,
    },
    PlaybackVolume {
        playback_id: String,
        volume: f64,
    },
    /// The picker's volume slider (0..=1).
    SendVolume(f64),
    /// Upload a sound to the call's server.
    Upload {
        path: String,
        file_name: String,
        size: u64,
        content_type: String,
        sound_name: String,
        emoji: String,
        volume: f64,
    },
    /// Delete a sound of the call's server.
    Delete(String),
    ClearError,
    /// Settings: a join or leave sound for a scope (`*` or a server id):
    /// on or off, a sound (`default:<id>` or `server:<server>:<id>`), or
    /// cleared.
    SetTrigger {
        trigger: Trigger,
        scope: String,
        enabled: Option<bool>,
        sound: Option<String>,
        clear: bool,
    },
    /// Settings: hear a join or leave sound.
    PreviewTrigger {
        trigger: Trigger,
        scope: String,
    },
    /// The activities dialog: start one for the call.
    StartActivity {
        activity: String,
        now_ms: i64,
    },
    LeaveActivity,
    /// The Wordle stage opened (or the day changed): `date` is today in New
    /// York (`getNewYorkDateKey`).
    WordleOpen {
        date: String,
    },
    WordleKey(char),
    WordleBackspace,
    WordleEnter,
    WordleHint,
    WordleReveal,
    WordleHardMode(bool),
    WordleClearNotice,
    /// Listen Together.
    LtSearch(String),
    LtResolve(String),
    /// Queue a search or resolve result by its index.
    LtEnqueue {
        index: usize,
        mode: String,
    },
    /// Play an entry (`None`: the current one, or the first).
    LtPlay(Option<String>),
    LtPause(bool),
    /// Seek the current entry; `now_ms` stamps it.
    LtSeek(f64),
    LtSkip,
    LtRemove(String),
    LtClear,
    LtVolume(f64),
    LtLoudness {
        enabled: bool,
        preset: String,
    },
    LtClearError,
}

// ── REST ──────────────────────────────────────────────────────────────────

/// The REST calls of the voice apps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppsRequest {
    /// GET `/api/servers/{id}/soundboard`.
    Catalog { server_id: String },
    /// DELETE `/api/servers/{id}/soundboard?soundId=`.
    DeleteSound { server_id: String, sound_id: String },
    /// PATCH `/api/users/me` `{sound_settings: {<key>: <map>}}`.
    SaveTrigger { key: &'static str, map: String },
    /// GET `/api/wordle/today?date=`.
    Wordle { date: String },
    /// GET `/api/listen-together/search`.
    Search { query: String, context: LtContext },
    /// POST `/api/listen-together/resolve`.
    Resolve { url: String, context: LtContext },
}

/// Where a Listen Together request comes from (the voice room it needs).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LtContext {
    pub room_slug: String,
    pub server_id: String,
    pub channel_id: String,
    /// Our voice participant (`X-Voice-Session-Id`, sent as `sessionId`).
    pub session_id: String,
}

impl LtContext {
    fn query(&self) -> String {
        let q = super::uri_component;
        format!(
            "roomSlug={}&serverId={}&channelId={}&sessionId={}",
            q(&self.room_slug),
            q(&self.server_id),
            q(&self.channel_id),
            q(&self.session_id)
        )
    }
}

/// One server sound (`ServerSoundboardItem`).
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize, Serialize)]
pub struct ServerSound {
    pub id: String,
    #[serde(default)]
    pub server_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub file_url: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default = "one")]
    pub volume: f64,
}

fn one() -> f64 {
    1.0
}

/// The answers the store reads.
#[derive(Debug, Clone, PartialEq)]
pub enum AppsResponse {
    Catalog(Vec<ServerSound>),
    Wordle(Box<WordlePuzzle>),
    Search(Vec<LtTrack>),
    Resolve {
        tracks: Vec<LtTrack>,
        skipped: usize,
    },
    Done,
}

/// The day's puzzle (`WordlePuzzle`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Deserialize)]
pub struct WordlePuzzle {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub print_date: String,
    #[serde(default)]
    pub solution: String,
    #[serde(default)]
    pub editor: Option<String>,
}

impl AppsRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Catalog { .. } | Self::Wordle { .. } | Self::Search { .. } => Method::Get,
            Self::DeleteSound { .. } => Method::Delete,
            Self::SaveTrigger { .. } => Method::Patch,
            Self::Resolve { .. } => Method::Post,
        }
    }

    pub fn path(&self) -> String {
        let q = super::uri_component;
        match self {
            Self::Catalog { server_id } => format!("/api/servers/{}/soundboard", q(server_id)),
            Self::DeleteSound {
                server_id,
                sound_id,
            } => format!(
                "/api/servers/{}/soundboard?soundId={}",
                q(server_id),
                q(sound_id)
            ),
            Self::SaveTrigger { .. } => "/api/users/me".into(),
            Self::Wordle { date } => format!("/api/wordle/today?date={}", q(date)),
            Self::Search { query, context } => format!(
                "/api/listen-together/search?q={}&filter=track&{}",
                q(query),
                context.query()
            ),
            Self::Resolve { context, .. } => {
                format!("/api/listen-together/resolve?{}", context.query())
            }
        }
    }

    pub fn body(&self) -> Value {
        match self {
            Self::SaveTrigger { key, map } => {
                let map: Value = serde_json::from_str(map).unwrap_or(Value::Null);
                json!({ "sound_settings": { *key: map } })
            }
            Self::Resolve { url, context } => json!({
                "roomSlug": context.room_slug, "serverId": context.server_id,
                "channelId": context.channel_id, "url": url,
            }),
            _ => Value::Null,
        }
    }

    pub fn decode(&self, body: Value) -> Result<AppsResponse, ApiError> {
        let tracks = |list: Option<&Value>| -> Vec<LtTrack> {
            list.and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|item| serde_json::from_value::<LtTrack>(item.clone()).ok())
                .map(|mut track| {
                    // `convertSearchTrackToMusicTrack`: a search's `track`
                    // queues as `music`.
                    if track.kind.is_empty() || track.kind == "track" {
                        track.kind = "music".into();
                    }
                    track
                })
                .collect()
        };
        Ok(match self {
            Self::Catalog { .. } => AppsResponse::Catalog(match body {
                Value::Array(items) => items
                    .into_iter()
                    .filter_map(|item| serde_json::from_value(item).ok())
                    .collect(),
                _ => Vec::new(),
            }),
            Self::Wordle { .. } => AppsResponse::Wordle(Box::new(
                serde_json::from_value(body)
                    .map_err(|e| ApiError::Decode(format!("wordle: {e}")))?,
            )),
            // Collections need a resolve; the track tab lists tracks only.
            Self::Search { .. } => AppsResponse::Search(
                tracks(body.get("results"))
                    .into_iter()
                    .filter(|t| t.video_id.is_some())
                    .collect(),
            ),
            Self::Resolve { .. } => AppsResponse::Resolve {
                tracks: tracks(body.get("tracks")),
                skipped: body
                    .get("skippedCount")
                    .and_then(Value::as_u64)
                    .map_or(0, |n| usize::try_from(n).unwrap_or(usize::MAX)),
            },
            Self::DeleteSound { .. } | Self::SaveTrigger { .. } => AppsResponse::Done,
        })
    }
}

// ── State ─────────────────────────────────────────────────────────────────

/// What a pending request was for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AppsOp {
    Catalog(String),
    Delete {
        server_id: String,
        sound_id: String,
    },
    SaveTrigger,
    Wordle(String),
    Search,
    Resolve,
    Upload {
        server_id: String,
        sound: ServerSound,
    },
}

/// One playing clip (`activePlaybacks`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Playback {
    pub playback_id: String,
    pub owner_id: String,
    pub server_key: String,
    pub name: String,
    /// Ours (or a preview).
    pub local: bool,
    pub preview: bool,
    pub paused: bool,
    pub volume: f64,
    /// The player said it is audible.
    pub started: bool,
}

/// A join or leave sound setting for one scope
/// (`SoundboardTriggerConfig`).
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize, Serialize)]
pub struct TriggerConfig {
    pub enabled: bool,
    pub sound: Option<TriggerSound>,
}

/// The chosen sound (`SoundboardTriggerSelection`).
#[derive(Debug, Clone, PartialEq, Default, serde::Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerSound {
    /// `default`, `server` or `custom`.
    pub source: String,
    pub sound_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emoji: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_url: Option<String>,
    pub volume: f64,
}

/// `normalizeSoundboardTriggerMap`: a map of scope to config; a legacy
/// single config becomes the `*` scope; invalid entries are dropped.
pub fn trigger_map(value: Option<&Value>) -> BTreeMap<String, TriggerConfig> {
    let parse = |value: &Value| -> Option<TriggerConfig> {
        let enabled = value.get("enabled")?.as_bool()?;
        let sound = match value.get("sound")? {
            Value::Null => None,
            sound => {
                let parsed: TriggerSound = serde_json::from_value(sound.clone()).ok()?;
                let ok = matches!(parsed.source.as_str(), "default" | "server" | "custom")
                    && !parsed.sound_id.is_empty()
                    && parsed.sound_id.chars().count() <= 200
                    && !parsed.name.is_empty()
                    && parsed.name.chars().count() <= 200
                    && (0.0..=1.0).contains(&parsed.volume)
                    && (parsed.source != "server" || parsed.server_id.is_some());
                if !ok {
                    return None;
                }
                Some(parsed)
            }
        };
        Some(TriggerConfig { enabled, sound })
    };
    let Some(value) = value else {
        return BTreeMap::from([(ALL_SERVERS.into(), TriggerConfig::default())]);
    };
    if let Some(legacy) = parse(value) {
        return BTreeMap::from([(ALL_SERVERS.into(), legacy)]);
    }
    let Some(object) = value.as_object() else {
        return BTreeMap::from([(ALL_SERVERS.into(), TriggerConfig::default())]);
    };
    object
        .iter()
        .filter_map(|(scope, config)| Some((scope.clone(), parse(config)?)))
        .collect()
}

/// `resolveSoundboardTriggerConfig`: the server's own config, else `*`.
pub fn resolve_trigger<'a>(
    map: &'a BTreeMap<String, TriggerConfig>,
    server_id: &str,
) -> Option<&'a TriggerConfig> {
    map.get(server_id).or_else(|| map.get(ALL_SERVERS))
}

/// The soundboard.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SoundboardState {
    /// Catalogs by server, as last loaded.
    pub catalogs: HashMap<String, Vec<ServerSound>>,
    pub loading: Vec<String>,
    /// Playing clips by `<server key>::<playback id>`.
    pub playbacks: BTreeMap<String, Playback>,
    pub send_volume: f64,
    /// Owners the server muted (`soundboard.server-mute-set`).
    pub server_muted: BTreeMap<String, bool>,
    pub uploading: bool,
    pub error: String,
    /// The device's settings.
    pub sounds_enabled: bool,
    pub sound_volume: u8,
    pub soundboard_volume: u8,
    pub joins: BTreeMap<String, TriggerConfig>,
    pub leaves: BTreeMap<String, TriggerConfig>,
    /// This call's join sound went out (or was decided against).
    pub join_done: bool,
    /// The leave sound for this call, resolved at the join.
    pub leave_frame: Option<String>,
    /// Our soundboard speaking flag as last sent.
    pub speaking: bool,
    /// Playback sequence for automatic clips.
    pub sequence: u64,
}

/// One Listen Together search or resolve result.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LtResults {
    pub tracks: Vec<LtTrack>,
    pub skipped: usize,
}

/// Listen Together.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ListenState {
    pub snapshot: Option<LtSnapshot>,
    pub query: String,
    pub searching: bool,
    pub results: LtResults,
    pub error: String,
    /// The player's error or retry line (`PLAYBACK_*`).
    pub playback_error: String,
    /// What the player has loaded: (entry id, stream path).
    pub loaded: Option<(String, String)>,
    /// The player's last position, its wall-clock stamp.
    pub position: Option<(f64, i64)>,
    /// The entry whose stream failed for good (not loaded again).
    pub failed_entry: Option<String>,
    pub volume: f64,
    pub loudness: bool,
    pub preset: String,
    /// The last wall-clock time an input carried.
    pub now_ms: i64,
    /// The room position and the wall clock it was taken at, as the view
    /// reports them: set when a snapshot arrives, never by the player's
    /// 4 Hz reports (the card runs its own clock from it).
    pub clock: (f64, i64),
    /// No drift seek before this wall-clock time (a load or seek is still
    /// buffering; its position does not move yet).
    pub settle_until: i64,
}

/// The Wordle stage.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WordleState {
    pub date: String,
    pub puzzle: Option<WordlePuzzle>,
    pub loading: bool,
    pub error: String,
    pub guesses: Vec<String>,
    pub draft: String,
    pub notice: String,
    /// Bumped on a word the list does not have (the row shakes).
    pub invalid: u64,
    pub hints: Vec<u8>,
    pub hard_mode: bool,
    /// Gave up (Reveal answer).
    pub revealed: bool,
    pub streak: u32,
    /// Everyone's progress by user id, this channel and day.
    pub progress: BTreeMap<String, WordleProgress>,
}

/// Activities in the call (`useActivityStore`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ActivityState {
    /// user id -> (channel id, activity).
    pub by_user: BTreeMap<String, (String, String)>,
    pub wordle: WordleState,
}

/// Everything the voice apps keep.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AppsState {
    pub soundboard: SoundboardState,
    pub listen: ListenState,
    pub activity: ActivityState,
    /// Our voice participant once the voice socket is ready.
    pub participant_id: String,
}

impl AppsState {
    pub fn new() -> Self {
        Self {
            soundboard: SoundboardState {
                send_volume: DEFAULT_SEND_VOLUME,
                sounds_enabled: true,
                sound_volume: 100,
                soundboard_volume: 100,
                ..SoundboardState::default()
            },
            listen: ListenState {
                volume: 1.0,
                loudness: true,
                preset: "balanced".into(),
                ..ListenState::default()
            },
            ..Self::default()
        }
    }
}

// ── Listen Together sync ──────────────────────────────────────────────────

/// `getListenTogetherPositionMs`: the anchor plus the time since, clamped to
/// the track.
pub fn lt_position(snapshot: &LtSnapshot, now_ms: i64) -> f64 {
    let duration = snapshot
        .current_entry
        .as_ref()
        .and_then(|entry| entry.track.duration());
    #[expect(clippy::cast_precision_loss, reason = "epoch ms fit f64 exactly")]
    let now = now_ms as f64;
    let raw = match snapshot.anchor_updated_at {
        Some(at) if !snapshot.paused && at > 0.0 => {
            snapshot.anchor_position_ms + (now - at).max(0.0)
        }
        _ => snapshot.anchor_position_ms,
    };
    let floor = raw.max(0.0).floor();
    match duration {
        Some(d) if d.is_finite() => floor.min(d.max(0.0).floor()),
        _ => floor,
    }
}

/// `getListenTogetherPlaybackPlan`, as commands for the player.
pub fn lt_plan(
    snapshot: Option<&LtSnapshot>,
    loaded: Option<&(String, String)>,
    player_position: Option<f64>,
    stream_path: Option<&str>,
    now_ms: i64,
) -> Vec<AudioCommand> {
    let current = snapshot.and_then(|s| s.current_entry.as_ref());
    let (Some(snapshot), Some(entry), Some(path)) = (snapshot, current, stream_path) else {
        return if loaded.is_some() {
            vec![AudioCommand::StreamStop]
        } else {
            Vec::new()
        };
    };
    let position = lt_position(snapshot, now_ms);
    let load = loaded.is_none_or(|(id, loaded_path)| *id != entry.entry_id || loaded_path != path);
    if load {
        return vec![AudioCommand::StreamLoad {
            key: entry.entry_id.clone(),
            path: path.to_owned(),
            position_ms: if entry.track.is_radio() {
                0.0
            } else {
                position
            },
            paused: snapshot.paused,
        }];
    }
    let mut out = vec![AudioCommand::StreamPause(snapshot.paused)];
    if let Some(at) = player_position {
        if !entry.track.is_radio() && (at - position).abs() > DRIFT_TOLERANCE_MS {
            out.push(AudioCommand::StreamSeek(position));
        }
    }
    out
}

/// How long after a load or a seek the player gets before its position is
/// compared with the room's (a fragmented m4a needs a few requests first).
pub const SETTLE_MS: i64 = 3_000;

/// `buildListenTogetherStreamUrl` (mp4, the format the player decodes).
pub fn lt_stream_path(video_id: &str, context: &LtContext) -> String {
    let q = super::uri_component;
    format!(
        "/api/listen-together/stream?videoId={}&{}&format=mp4",
        q(video_id),
        context.query()
    )
}

// ── Views ─────────────────────────────────────────────────────────────────

/// A sound tile.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SoundTile {
    pub id: String,
    pub name: String,
    pub emoji: String,
    /// `default` or `server`.
    pub kind: &'static str,
    pub playing: bool,
    pub can_delete: bool,
}

/// A playing row.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackRow {
    pub playback_id: String,
    pub name: String,
    pub owner: String,
    pub mine: bool,
    pub paused: bool,
    pub volume: f64,
}

/// A trigger row in settings.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRow {
    pub enabled: bool,
    pub name: String,
    pub emoji: String,
    pub has_sound: bool,
    /// The scope uses the All servers setting.
    pub inherited: bool,
}

/// A queue row.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRow {
    pub entry_id: String,
    pub title: String,
    pub artist: String,
    pub artwork: String,
    pub duration_ms: f64,
    pub requester: String,
    pub current: bool,
}

/// A search or resolve row.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub index: usize,
    pub title: String,
    pub artist: String,
    pub artwork: String,
    pub duration_ms: f64,
}

/// A Wordle tile.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct Tile {
    pub letter: String,
    /// `correct`, `present`, `absent`, `typed` or empty.
    pub mark: &'static str,
}

/// Another player's row.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerRow {
    pub user_id: String,
    pub name: String,
    /// Each guess as marks only (others' letters stay hidden).
    pub rows: Vec<Vec<&'static str>>,
    pub streak: u32,
    pub finished: bool,
    pub missed: bool,
}

/// What the UI shows of the voice apps.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsView {
    pub in_call: bool,
    pub server_id: String,
    pub defaults: Vec<SoundTile>,
    pub sounds: Vec<SoundTile>,
    pub loading_sounds: bool,
    pub playing: Vec<PlaybackRow>,
    pub send_volume: f64,
    pub can_upload: bool,
    pub uploading: bool,
    pub sound_error: String,
    /// Settings: catalogs for the trigger chooser, by server.
    pub catalogs: BTreeMap<String, Vec<SoundTile>>,
    pub joins: BTreeMap<String, TriggerRow>,
    pub leaves: BTreeMap<String, TriggerRow>,
    /// The activity we are in (`wordle`), or empty.
    pub activity: String,
    /// Who else plays what: user id -> activity.
    pub others: BTreeMap<String, String>,
    pub wordle: WordleView,
    pub listen: ListenView,
}

/// The Wordle stage.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WordleView {
    pub date: String,
    pub loading: bool,
    pub error: String,
    /// Six rows of five.
    pub board: Vec<Vec<Tile>>,
    /// Letter -> best mark.
    pub keys: BTreeMap<String, &'static str>,
    pub notice: String,
    pub invalid: u64,
    /// `playing`, `solved` or `missed`.
    pub status: &'static str,
    pub answer: String,
    pub hard_mode: bool,
    pub hints: Vec<HintRow>,
    pub streak: u32,
    pub players: Vec<PlayerRow>,
}

/// An unlocked hint.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct HintRow {
    pub level: u8,
    pub title: String,
    pub value: String,
}

/// Listen Together.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenView {
    pub queue: Vec<QueueRow>,
    pub now: Option<QueueRow>,
    pub paused: bool,
    /// The room position when the view was made, and the wall clock then
    /// (the card runs its own clock between updates).
    pub position_ms: f64,
    pub position_at: i64,
    pub duration_ms: f64,
    pub query: String,
    pub searching: bool,
    pub results: Vec<ResultRow>,
    pub skipped: usize,
    pub error: String,
    pub playback_error: String,
    pub volume: f64,
    pub loudness: bool,
    pub preset: String,
}

/// Marks as the UI names them.
pub fn mark_name(mark: Mark) -> &'static str {
    match mark {
        Mark::Correct => "correct",
        Mark::Present => "present",
        Mark::Absent => "absent",
    }
}

/// The built-in tiles.
pub fn default_tiles(playing: &[String]) -> Vec<SoundTile> {
    DEFAULT_SOUNDS
        .iter()
        .map(|sound| SoundTile {
            id: sound.id.into(),
            name: sound.name.into(),
            emoji: String::new(),
            kind: "default",
            playing: playing.iter().any(|id| id == sound.id),
            can_delete: false,
        })
        .collect()
}

/// The Wordle view from its state (`me` is the local user id).
pub fn wordle_view(state: &WordleState, me: &str, names: &HashMap<String, String>) -> WordleView {
    let answer = state
        .puzzle
        .as_ref()
        .map(|p| p.solution.to_ascii_lowercase())
        .unwrap_or_default();
    let status = if answer.is_empty() {
        "playing"
    } else if state.revealed {
        "missed"
    } else {
        match wordle::completion(&state.guesses, &answer) {
            wordle::Completion::Playing => "playing",
            wordle::Completion::Solved(_) => "solved",
            wordle::Completion::Missed => "missed",
        }
    };
    let mut board = Vec::with_capacity(6);
    for guess in &state.guesses {
        let marks = wordle::evaluate(guess, &answer);
        board.push(
            guess
                .chars()
                .zip(marks)
                .map(|(letter, mark)| Tile {
                    letter: letter.to_ascii_uppercase().to_string(),
                    mark: mark_name(mark),
                })
                .collect(),
        );
    }
    if board.len() < 6 && status == "playing" {
        let mut row: Vec<Tile> = state
            .draft
            .chars()
            .map(|letter| Tile {
                letter: letter.to_ascii_uppercase().to_string(),
                mark: "typed",
            })
            .collect();
        row.resize(5, Tile::default());
        board.push(row);
    }
    while board.len() < 6 {
        board.push(vec![Tile::default(); 5]);
    }
    let keys = wordle::key_marks(&state.guesses, &answer)
        .into_iter()
        .map(|(letter, mark)| (letter.to_string(), mark_name(mark)))
        .collect();
    let hints = state
        .hints
        .iter()
        .filter_map(|level| {
            let hint = wordle::hint(&answer, *level)?;
            Some(HintRow {
                level: *level,
                title: hint.title.into(),
                value: match hint.position {
                    Some(position) => format!("{} (letter {position})", hint.value),
                    None => hint.value,
                },
            })
        })
        .collect();
    let players = state
        .progress
        .values()
        .filter(|p| p.user_id != me)
        .map(|p| PlayerRow {
            user_id: p.user_id.clone(),
            name: names
                .get(&p.user_id)
                .cloned()
                .unwrap_or_else(|| "Someone".into()),
            rows: p
                .guesses
                .iter()
                .map(|g| {
                    wordle::evaluate(g, &answer)
                        .into_iter()
                        .map(mark_name)
                        .collect()
                })
                .collect(),
            streak: p.streak,
            finished: p.finished,
            missed: p.missed,
        })
        .collect();
    WordleView {
        date: state.date.clone(),
        loading: state.loading,
        error: state.error.clone(),
        board,
        keys,
        notice: state.notice.clone(),
        invalid: state.invalid,
        status,
        answer: if status == "playing" {
            String::new()
        } else {
            answer.to_ascii_uppercase()
        },
        hard_mode: state.hard_mode,
        hints,
        streak: state.streak,
        players,
    }
}

/// `default:<id>` or `server:<server>:<id>` as a trigger selection.
pub fn trigger_sound(
    pick: &str,
    catalogs: &HashMap<String, Vec<ServerSound>>,
) -> Option<TriggerSound> {
    if let Some(id) = pick.strip_prefix("default:") {
        let sound = DEFAULT_SOUNDS.iter().find(|s| s.id == id)?;
        return Some(TriggerSound {
            source: "default".into(),
            sound_id: sound.id.into(),
            name: sound.name.into(),
            volume: 1.0,
            ..TriggerSound::default()
        });
    }
    let (server_id, sound_id) = pick.strip_prefix("server:")?.split_once(':')?;
    let sound = catalogs.get(server_id)?.iter().find(|s| s.id == sound_id)?;
    Some(TriggerSound {
        source: "server".into(),
        sound_id: sound.id.clone(),
        name: sound.name.clone(),
        server_id: Some(server_id.to_owned()),
        emoji: sound.emoji.clone().filter(|e| !e.is_empty()),
        media_url: None,
        volume: sound.volume.clamp(0.0, 1.0),
    })
}

/// Whether a sound id names a built-in tone.
pub fn is_tone(id: &str) -> bool {
    is_default_sound(id)
}
