//! [`CallMachine`]: one call as a pure, synchronous state machine. See the
//! crate docs for the transition table.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::fmt;
use std::time::Duration;

use serde_json::Value;
use yap_protocol::payload::{RepublishRequest, VoiceMove};
use yap_protocol::rest::{VoiceAdmission, VoiceAdmissionRequest};
use yap_protocol::{GatewayOp, VOICE_MOVED_CLOSE_REASON};
use yap_realtime::StopReason;

use crate::audio::{Playback, Processing};
use crate::engine::MediaStats;
use crate::layers::{
    resolve_quality, select_rid, Quality, RenderSize, LAYER_DOWNGRADE_DELAY, LAYER_SWITCH_SETTLE,
    LAYER_UPGRADE_DELAY,
};
use crate::roster::{Member, Roster};
use crate::sfu::LayerUpdate;
use crate::share::{
    ShareCaptured, ShareEncoding, ShareNotice, SharePublished, ShareQuality, ShareRequest,
};
use crate::spatial::{self, SpatialLocal, SpatialState};
use crate::tracks::{Kind, Offer, Remote, TrackTable};
use crate::vad::{level_percent, Sensitivity, Vad};
use crate::wire::{
    self, Announced, Bootstrap, IceServer, Participant, RoomState, SelfState, Track,
};

mod health;
mod share;

use health::Health;
pub use health::{StatsSample, VoiceStats, STATS_SAMPLES};
use share::Share;

/// How many SFU session rebuilds in a row (without the new peer connection
/// ever connecting) before a failed connection ends the call.
pub const MAX_REBUILDS: u32 = 3;

/// The wait before the first SFU session rebuild; each one after doubles it
/// (1 s, 2 s, 4 s).
pub const REBUILD_BACKOFF: Duration = Duration::from_secs(1);

/// How long a connection must stay up before the rebuild count starts over:
/// a link that flaps keeps spending the same few rebuilds.
pub const REBUILD_STABLE_AFTER: Duration = Duration::from_secs(60);

/// How long a pulled video nobody shows any more keeps its pull before it
/// is released (a tile scrolled away for a moment keeps it).
pub const VIDEO_RELEASE_DELAY: Duration = Duration::from_secs(15);

/// A standby's mic track sends nothing; the SFU drops a track without media
/// after about 30 s, so a promotion after this long pushes it again
/// (voice-standby.ts `republishIfStale`).
pub const STANDBY_STALE_AFTER: Duration = Duration::from_secs(25);

/// A DM call's room: both user ids, sorted (`dmCallRoomSlug`).
pub fn dm_call_slug(me: &str, peer: &str) -> String {
    let (a, b) = if me <= peer { (me, peer) } else { (peer, me) };
    format!("dm-call-{a}-{b}")
}

/// How long a remote participant's pulls outlive a voice `ClientDisconnect`
/// for them (`REMOTE_RECONNECT_GRACE_MS`, STATE_MACHINE.md "Reconnect
/// grace"): VoiceRoom sends one whenever their voice socket reconnects,
/// before their tracks are offered again.
pub const REMOTE_RECONNECT_GRACE: Duration = Duration::from_millis(3000);

/// How long a joined leave keeps the sockets open before it disconnects
/// (`LEAVE_SEND_WINDOW_MS`, STATE_MACHINE.md "Teardown").
pub const LEAVE_SEND_WINDOW: Duration = Duration::from_millis(100);

/// How long after a voice token arrives the room is asked for a fresh one
/// (op 40): web and mobile refresh 45 minutes into the token's hour
/// (ADMISSION.md "Room socket, op 40").
pub const CREDENTIAL_REFRESH: Duration = Duration::from_secs(45 * 60);

/// The end reason when the room lost our session and identified a new
/// participant (a Resume refused with 4006): the SFU session and the voice
/// grant belong to the old one. The owner may join again at once.
pub const CALL_RESET: &str = "The call connection was reset. Join again.";

/// The call's phase, with the names and meaning of `packages/voice`
/// (STATE_MACHINE.md "Phases"; there is no standby here).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    /// Not started, or over.
    Idle,
    /// Admission, sockets, SFU session and push, until the room says Ready.
    Connecting,
    /// The room said Ready for us.
    Joined,
    /// Joined, and the room socket or the peer connection dropped. Roster
    /// and media are kept.
    Reconnecting,
    /// A joined leave: local media is released, the sockets stay open for
    /// the send window, then disconnect.
    Leaving,
}

impl fmt::Display for CallState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Idle => "idle",
            Self::Connecting => "connecting",
            Self::Joined => "joined",
            Self::Reconnecting => "reconnecting",
            Self::Leaving => "leaving",
        })
    }
}

/// What a call needs to join.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CallTarget {
    /// Empty for a DM call.
    pub server_id: String,
    /// The voice channel, or the DM channel of a call.
    pub channel_id: String,
    pub me_name: String,
    pub me_username: String,
    /// A DM call's room (`dm-call-<a>-<b>`, [`dm_call_slug`]); `None`: the
    /// server voice channel's.
    pub room_slug: Option<String>,
    /// The grant a moderator's move carried (VoiceMove): stands in for
    /// CONNECT on this join's admission.
    pub move_grant: Option<String>,
    /// Warm up hidden (`standby: true`) until [`UserCommand::Promote`].
    pub standby: bool,
}

impl CallTarget {
    /// The room both call sockets and the SFU proxy live under.
    pub fn slug(&self) -> String {
        self.room_slug
            .clone()
            .unwrap_or_else(|| format!("voice-{}-{}", self.server_id, self.channel_id))
    }
}

/// The two call sockets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Socket {
    /// `/api/channels/<slug>/ws`: join grant, roster, credentials.
    Room,
    /// `/api/channels/<slug>/voice`: track announcements.
    Voice,
}

/// Where the UI shows remote video, and what is on stage.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VideoIntent {
    /// Track alias (`cam-video-<pid>`, `screen-video-<pid>`) -> the largest
    /// size a visible tile renders it at. Absent: nothing shows it, and its
    /// frames are dropped.
    pub demand: BTreeMap<String, RenderSize>,
    /// The participant whose camera or share is focused on stage; their
    /// share audio is audible (SCREEN_SHARE.md "Watching").
    pub stage: Option<String>,
}

/// What the user asks of a running call.
#[derive(Debug, Clone, PartialEq)]
pub enum UserCommand {
    SetMuted(bool),
    /// Deafen: silence the others (remote audio off) and say so (op 15).
    SetDeafened(bool),
    /// The speaking threshold.
    SetSensitivity(Sensitivity),
    /// The output volume and per-user volumes.
    SetPlayback(Playback),
    /// The mic's echo cancellation, noise suppression, gain control and
    /// RNNoise.
    SetProcessing(Processing),
    /// Our spatial audio switches and who we are on the room's map. A
    /// change of either switch goes out in op 15 (`useSpatialAudioSync`).
    SetSpatial(SpatialLocal),
    /// A new placement for the room (the spatial panel's `commit`): kept
    /// and sent in op 15 for everyone.
    SetSpatialRoom(SpatialState),
    /// Rendered video and the stage.
    SetVideo(VideoIntent),
    /// The participants whose shares the user watches (pressed Watch).
    SetWatched(BTreeSet<String>),
    /// Our camera on or off. Off keeps its SFU track alive on the
    /// keepalive, so on again resumes the same track (SCREEN_SHARE.md
    /// "Keepalive park and reuse").
    SetCamera(bool),
    /// Share a source (SCREEN_SHARE.md `start`): always a new capture.
    StartShare(ShareRequest),
    /// Stop sharing (`stop`).
    StopShare,
    /// The share's audio on or off (`setAudio`).
    SetShareAudio(bool),
    /// The share's quality preset (`setQuality`).
    SetShareQuality(String),
    /// A voice app event (op 106) to send, as its frame text. It waits for
    /// the voice socket when that is reconnecting.
    SendApp(String),
    /// Our own soundboard clip is playing (or not): op 5's SOUNDSHARE flag
    /// (`vad.setSoundboardSpeaking`).
    SetSoundboardSpeaking(bool),
    /// A standby becomes the join (voice-standby.ts claim, `promote_standby`).
    Promote,
    Leave,
}

/// Our mic, published to the SFU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// Our SFU session capability (opaque; never shown).
    pub session_id: String,
    pub sfu_track_name: String,
}

/// Our camera, pushed to the SFU (or resumed on the track it kept).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CameraPublished {
    pub sfu_track_name: String,
    /// The layers the sender negotiated, highest first; empty for one.
    pub simulcast_rids: Vec<String>,
}

/// A room socket event.
#[derive(Debug, Clone, PartialEq)]
pub enum RoomEvent {
    /// Identified (op 2). A later one is a fresh session after a drop.
    Ready(RoomState),
    /// Resumed (op 9 answering a Resume).
    Resumed(RoomState),
    /// Op 15 S->C for a participant: `leave`, or a join/update.
    VoiceState {
        leave: bool,
        participant: Participant,
    },
    /// The room's spatial state, as a Ready, Resumed or op 15 carried it
    /// (just before that frame's own event).
    Spatial(SpatialState),
    /// Op 9 answering a credentials refresh (op 40).
    Credentials { voice_token: String },
    /// Op 42: a moderator moved this session; the 4010 close follows.
    Moved(VoiceMove),
    /// The socket dropped; it reconnects.
    Disconnected,
    /// The socket stopped for good.
    Stopped(StopReason),
    /// The connection task went away.
    Closed,
}

/// A voice socket event.
#[derive(Debug, Clone, PartialEq)]
pub enum VoiceEvent {
    /// VoiceReady: every live track, and who is speaking.
    Ready {
        tracks: Vec<Track>,
        speaking: Vec<(String, bool)>,
    },
    /// Op 12: someone published.
    Video {
        participant_id: String,
        tracks: Vec<Track>,
    },
    /// Op 5.
    Speaking {
        participant_id: String,
        speaking: bool,
    },
    /// Op 13: these names stopped (empty: all of the sender's).
    StopTracks {
        participant_id: String,
        names: Vec<String>,
    },
    /// Op 11 S->C: their voice socket went away, or they re-identified.
    ClientDisconnect {
        participant_id: String,
    },
    /// Op 106: a voice app event (soundboard, Listen Together, activities).
    App(Value),
    /// Op 110: a listener cannot pull one of ours; push it again.
    Republish(RepublishRequest),
    /// An Error 4008 or 4010: this session must stop.
    Rejected {
        message: String,
    },
    /// The voice token was refused (4004 before ready).
    IdentifyRejected,
    Disconnected,
    Stopped(StopReason),
    Closed,
}

/// The peer connection's state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaState {
    New,
    Connecting,
    Connected,
    Disconnected,
    Failed,
    Closed,
}

/// A media engine event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaEvent {
    State(MediaState),
    /// The state stream ended.
    Closed,
}

/// A timer the machine armed, or a periodic one the driver runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Timer {
    /// `participant`'s reconnect grace ran out.
    Grace { participant: String, token: u64 },
    /// A joined leave's send window is over.
    Linger,
    /// A camera's rendered size held long enough: switch its layer.
    Layer { track: String, token: u64 },
    /// A layer switch had time to land: restore the SFU's policy.
    LayerSettle { track: String, token: u64 },
    /// Every 5 s: log media stats.
    Stats,
    /// A video pull nobody showed for [`VIDEO_RELEASE_DELAY`].
    ReleaseVideo { track: String, token: u64 },
    /// A republish of `track` may be asked for again.
    RepublishCooldown { track: String },
    /// A standby's idle mic is likely gone from the SFU.
    StandbyStale,
    /// [`CREDENTIAL_REFRESH`] after the voice token `token` counts arrived:
    /// ask the room for a fresh one (op 40).
    RefreshCredentials { token: u64 },
    /// The backoff before an SFU session rebuild is over.
    Rebuild { token: u64 },
    /// Media `token` stayed connected for [`REBUILD_STABLE_AFTER`].
    MediaStable { token: u64 },
}

/// Everything the machine consumes.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Start,
    /// The join's admission (both tickets and the media credentials), or
    /// why it could not be had.
    Admitted(Result<VoiceAdmission, String>),
    /// The mic is published, or why media could not start.
    Published(Result<Published, String>),
    Room(RoomEvent),
    Voice(VoiceEvent),
    Media(MediaEvent),
    /// The camera push (or resume) finished, or why it failed.
    CameraPublished(Result<CameraPublished, String>),
    /// The share capture `token` started, or why it could not.
    ShareCaptured {
        token: u64,
        result: Result<ShareCaptured, String>,
    },
    /// The share capture `token`'s source went away by itself.
    ShareCaptureEnded {
        token: u64,
    },
    /// The share's publish finished, or why it failed.
    SharePublished(Result<SharePublished, String>),
    /// The live share's hardware encoder failed: its frames go nowhere
    /// until the share is pushed again (on VP8).
    ShareEncoderFailed,
    /// A pull finished; `mid` if it worked.
    PullDone {
        sfu_track_name: String,
        mid: Option<String>,
    },
    /// The pull of `sfu_track_name` failed because the SFU kept answering
    /// `not_found_track_error` (sent just before its failed [`Input::PullDone`]).
    PullNotFound {
        sfu_track_name: String,
    },
    /// A republish ([`Effect::Republish`]) landed: the new SFU track name.
    Republished(Result<(Republish, String), String>),
    /// A stats snapshot of the call's peer connection, `at_ms` on the VAD's
    /// clock.
    Stats {
        stats: MediaStats,
        at_ms: u64,
    },
    Command(UserCommand),
    Timer(Timer),
    /// The mic level on the VAD's 0..100 scale, read every
    /// [`crate::VAD_TICK`] once the mic is published; `at_ms` is a
    /// monotonic clock.
    MicLevel {
        rms: f64,
        at_ms: u64,
    },
}

/// What the machine reports to its owner.
#[derive(Debug, Clone, PartialEq)]
pub enum Notice {
    /// The state changed.
    State(CallState),
    /// The status line ("Getting tickets", "Joining", "Connected",
    /// "Reconnecting").
    Status(&'static str),
    /// The room said Ready for the first time (`joined`): the connect sound
    /// and the presence join (op 33) go out.
    Connected,
    /// A second Ready while joined (the room could not resume): the room may
    /// have dropped our presence, so announce it again (STATE_MACHINE.md,
    /// transition note 1).
    Rejoined,
    /// The whole roster.
    Roster(Vec<Member>),
    /// The mic level as the meter shows it, 0..100 (the sensitivity
    /// slider's scale), with each reading.
    MicLevel(f64),
    /// Our camera is on (published and announced) or off; `error` says
    /// why turning it on failed.
    Camera { on: bool, error: Option<String> },
    /// Our share started, changed or ended.
    Share(ShareNotice),
    /// The voice socket is ready (VoiceReady), the first time or after a
    /// re-identify: app events can go out. `participant_id` is ours (the
    /// voice session id the Listen Together stream asks for).
    VoiceReady { participant_id: String },
    /// A voice app event (op 106) arrived.
    App(Value),
    /// A moderator moved us (op 42): the call ends; the owner joins there.
    Moved(VoiceMove),
    /// The voice details' numbers, every stats tick.
    Stats(VoiceStats),
    /// Nothing arrives from anyone while someone speaks (AudioSentinel).
    InboundAudioStalled(bool),
    /// Our unmuted mic sends nothing (OutboundAudioSentinel).
    MicStalled(bool),
}

/// Which of our publications a republish pushes again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Republish {
    Mic,
    Camera,
}

/// How to open one call socket.
#[derive(Debug, Clone, PartialEq)]
pub struct SocketSpec {
    /// The path under the API base, e.g. `/api/channels/<slug>/ws?v=1`.
    pub path: String,
    pub identify: Value,
    /// The join admission's ticket for this socket (single use).
    pub first_ticket: String,
    /// The admission that asks for this socket's ticket alone, minting
    /// nothing, on each reconnect (ADMISSION.md "Client flow" 3 and 4).
    pub reconnect: VoiceAdmissionRequest,
}

/// How to open media and publish the mic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaSpec {
    pub ice_servers: Vec<IceServer>,
    pub slug: String,
    pub participant_id: String,
    pub voice_token: String,
    /// The msid stream for our mic (`cam-<participant>`).
    pub stream_id: String,
    /// A standby: push the mic but do not capture until promoted.
    pub standby: bool,
}

/// Everything the machine asks for. The owner runs them in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Notify(Notice),
    /// `POST /api/voice/admission` for the join; answer with
    /// [`Input::Admitted`].
    RequestAdmission(VoiceAdmissionRequest),
    /// Open both sockets (boxed: the specs dwarf every other effect).
    OpenSockets {
        room: Box<SocketSpec>,
        voice: Box<SocketSpec>,
    },
    /// Open media, create the SFU session and push the mic; answer with
    /// [`Input::Published`] (socket events wait until then).
    OpenMedia(MediaSpec),
    /// The peer connection failed: drop it (and its pulls, video sessions
    /// and worker) and open media again on a new SFU session, as
    /// PartyTracks does; answer with [`Input::Published`].
    RebuildMedia(MediaSpec),
    /// Push one of our publications again under a new SFU track name
    /// (serialized with pulls); answer with [`Input::Republished`].
    Republish(Republish),
    /// A promoted standby: start capturing the mic.
    PromoteMedia,
    /// Send a text frame once that socket is ready.
    Send(Socket, String),
    /// The identify the voice socket sends next.
    SetVoiceIdentify(Value),
    /// The voice token for SFU calls.
    SetVoiceToken(String),
    /// Pull a remote track (serialized with closes); answer with
    /// [`Input::PullDone`].
    Pull(Remote),
    /// Push our camera, or resume the SFU track a camera that went off
    /// kept (serialized with pulls); answer with [`Input::CameraPublished`].
    PublishCamera {
        stream_id: String,
    },
    /// Send camera frames (`true`) or the keepalive's black frames.
    SetCameraLive(bool),
    /// Start capturing a share source; answer with [`Input::ShareCaptured`]
    /// (and [`Input::ShareCaptureEnded`] when its source goes away).
    StartShareCapture {
        token: u64,
        request: ShareRequest,
    },
    /// Release that capture (nothing if it is gone already).
    ReleaseShareCapture(u64),
    /// A new box and frame rate for that capture.
    SetShareCaptureQuality {
        token: u64,
        quality: ShareQuality,
    },
    /// Push our share's video, or resume the track it kept, and push its
    /// audio when `audio` (or retire audio a previous capture had);
    /// serialized with pulls. Answer with [`Input::SharePublished`].
    PublishShare {
        stream_id: String,
        encoding: ShareEncoding,
        audio: bool,
    },
    /// Park the share's video on the keepalive and retire its audio
    /// (serialized with pulls).
    UnpublishShare,
    /// Send captured share frames (`true`) or nothing new.
    SetShareLive(bool),
    /// The share sender's caps and content hint, in place.
    SetShareEncoding(ShareEncoding),
    /// The share audio track on or off, in place.
    SetShareAudioEnabled(bool),
    /// Stop these receive transceivers and close them on the SFU.
    Close(Vec<String>),
    /// Move a pulled camera to another simulcast layer (`tracks/update`).
    UpdateLayer(LayerUpdate),
    /// Route the frames of the video received on `mid` to the UI under
    /// `key` (its alias), or drop them (`None`).
    SetVideoSink {
        mid: String,
        key: Option<String>,
    },
    SetMicEnabled(bool),
    /// Play the others (`false` while deafened).
    SetPlayoutEnabled(bool),
    /// The gain of the remote track received on `mid` (1.0 until set).
    SetTrackGain {
        mid: String,
        gain: f64,
    },
    /// The stereo pan of the remote track received on `mid`, -1..=1 (0
    /// until set).
    SetTrackPan {
        mid: String,
        pan: f64,
    },
    /// The mic's capture processing.
    SetProcessing(Processing),
    /// Teardown step 4: stop the mic before any socket closes.
    StopLocalMedia,
    LogStats,
    /// A diagnostic line.
    Log(String),
    /// Fire [`Input::Timer`] with `timer` after `after`.
    ArmTimer {
        timer: Timer,
        after: Duration,
    },
    /// Close everything; the call is over (`Some` = why it failed).
    End(Option<String>),
}

/// Our camera publication.
#[derive(Debug, Clone, Default)]
struct Camera {
    /// The user wants it on.
    wanted: bool,
    /// A push or resume is in flight.
    pushing: bool,
    /// The SFU track, once pushed. Kept while the camera is off.
    published: Option<CameraPublished>,
    /// Sending camera frames and announced. Off: parked on the keepalive,
    /// never announced.
    on: bool,
}

/// One pulled camera's simulcast layer.
#[derive(Debug, Clone, Default)]
struct Layer {
    /// The layer asked of the SFU.
    quality: Option<Quality>,
    /// A switch waiting for the size to settle, and its timer's token.
    pending: Option<(Quality, u64)>,
    /// The token of the settle timer of the last switch.
    settle: u64,
}

/// One call. Feed it [`Input`]s with [`CallMachine::handle`]; run the
/// [`Effect`]s it returns, in order.
#[derive(Debug)]
pub struct CallMachine {
    target: CallTarget,
    state: CallState,
    ended: bool,
    me: String,
    voice_token: String,
    /// The admission's ICE servers (a rebuild opens media with them again).
    ice_servers: Vec<IceServer>,
    /// The token of the armed credential refresh (a newer token re-arms it).
    refresh: u64,
    published: Option<Published>,
    sockets_open: bool,
    /// The room was identified once (a later Ready is a fresh session).
    room_identified: bool,
    room_up: bool,
    voice_ready: bool,
    /// VoiceReady was seen before (a later one follows a re-identify).
    reidentified: bool,
    /// Media connected at least once (pulls wait for it).
    connected: bool,
    /// The peer connection is connected now.
    media_up: bool,
    status: &'static str,
    muted: bool,
    deafened: bool,
    roster: Option<Roster>,
    /// The roster last reported.
    reported: Vec<Member>,
    tracks: TrackTable,
    next_token: u64,
    vad: Vad,
    sensitivity: Sensitivity,
    playback: Playback,
    processing: Processing,
    /// The gain set on each pulled audio mid (absent: 1.0).
    gains: HashMap<String, f64>,
    /// The pan set on each pulled audio mid (absent: 0).
    pans: HashMap<String, f64>,
    /// The room's shared spatial state, and our side of it.
    spatial_room: SpatialState,
    spatial_local: SpatialLocal,
    video: VideoIntent,
    watched: BTreeSet<String>,
    /// Watch announcements sent (op 106), by streamer user id.
    announced_watch: BTreeSet<String>,
    /// The frame route set on each pulled video mid (absent: none).
    routes: HashMap<String, String>,
    layers: HashMap<String, Layer>,
    camera: Camera,
    share: Share,
    /// Our own soundboard clip is audible (op 5 SOUNDSHARE).
    soundboard_speaking: bool,
    /// Op 106 frames sent while the voice socket was not ready (joining or
    /// reconnecting), flushed on the next VoiceReady; dropped on leave.
    app_queue: VecDeque<String>,
    /// A moderator moved us: the stop that follows is not an error.
    moved: bool,
    /// SFU session rebuilds since media last connected.
    rebuilds: u32,
    /// A rebuild is waiting out its backoff or for its new session: a
    /// failure reported by the old connection meanwhile is stale.
    rebuilding: bool,
    /// The armed rebuild's token (an older timer fires as a no-op).
    rebuild_token: u64,
    /// The connected media's stability timer (0: none armed).
    stable_token: u64,
    /// Still a hidden standby (not promoted).
    standby: bool,
    /// The standby's mic has sat long enough to be gone from the SFU.
    standby_stale: bool,
    /// Video aliases whose pull was closed: later pulls of them are fresh.
    closed_video: HashSet<String>,
    /// Release timers of unwanted video pulls, by alias.
    releasing: HashMap<String, u64>,
    /// Stats, the sentinels, op 109 reports and republish cooldowns.
    health: Health,
}

/// Frames held for a voice socket that is not ready (the oldest go first
/// when it stays down).
const APP_QUEUE_MAX: usize = 32;

impl CallMachine {
    pub fn new(target: CallTarget) -> Self {
        Self {
            target,
            state: CallState::Idle,
            ended: false,
            me: String::new(),
            voice_token: String::new(),
            ice_servers: Vec::new(),
            refresh: 0,
            published: None,
            sockets_open: false,
            room_identified: false,
            room_up: false,
            voice_ready: false,
            reidentified: false,
            connected: false,
            media_up: false,
            status: "",
            muted: false,
            deafened: false,
            roster: None,
            reported: Vec::new(),
            tracks: TrackTable::default(),
            next_token: 0,
            vad: Vad::default(),
            sensitivity: Sensitivity::default(),
            playback: Playback::default(),
            processing: Processing::default(),
            gains: HashMap::new(),
            pans: HashMap::new(),
            spatial_room: SpatialState::default(),
            spatial_local: SpatialLocal::default(),
            video: VideoIntent::default(),
            watched: BTreeSet::new(),
            announced_watch: BTreeSet::new(),
            routes: HashMap::new(),
            layers: HashMap::new(),
            camera: Camera::default(),
            share: Share::default(),
            soundboard_speaking: false,
            app_queue: VecDeque::new(),
            moved: false,
            rebuilds: 0,
            rebuilding: false,
            rebuild_token: 0,
            stable_token: 0,
            standby: false,
            standby_stale: false,
            closed_video: HashSet::new(),
            releasing: HashMap::new(),
            health: Health::default(),
        }
    }

    pub fn state(&self) -> CallState {
        self.state
    }

    /// The remote tracks (for inspection).
    pub fn tracks(&self) -> &TrackTable {
        &self.tracks
    }

    /// The roster so far (empty before the admission), with whose camera and
    /// share are live.
    pub fn roster(&self) -> Vec<Member> {
        let mut members = self
            .roster
            .as_ref()
            .map(Roster::snapshot)
            .unwrap_or_default();
        for m in &mut members {
            m.camera_live = self.tracks.is_live(&camera_alias(&m.id));
            m.screen_live = self.tracks.is_live(&screen_alias(&m.id));
        }
        members
    }

    /// Consumes one input; returns the effects to run, in order.
    pub fn handle(&mut self, input: Input) -> Vec<Effect> {
        let mut fx = Vec::new();
        if self.ended {
            return fx;
        }
        match (self.state, input) {
            (CallState::Idle, Input::Start) => self.start(&mut fx),
            (CallState::Idle, _) => {}
            (CallState::Leaving, Input::Timer(Timer::Linger)) => {
                self.disconnect(&mut fx);
                self.end(&mut fx, None);
            }
            (CallState::Leaving, _) => {}
            (_, Input::Start) => {}
            (_, Input::Admitted(result)) => self.on_admitted(&mut fx, result),
            (_, Input::Published(result)) => self.on_published(&mut fx, result),
            (_, Input::CameraPublished(result)) => self.on_camera_published(&mut fx, result),
            (_, Input::ShareCaptured { token, result }) => {
                self.on_share_captured(&mut fx, token, result);
            }
            (_, Input::ShareCaptureEnded { token }) => self.on_share_capture_ended(&mut fx, token),
            (_, Input::SharePublished(result)) => self.on_share_published(&mut fx, result),
            (_, Input::ShareEncoderFailed) => self.on_share_encoder_failed(&mut fx),
            (_, Input::Room(event)) => self.on_room(&mut fx, event),
            (_, Input::Voice(event)) => self.on_voice(&mut fx, event),
            (_, Input::Media(event)) => self.on_media(&mut fx, event),
            (
                _,
                Input::PullDone {
                    sfu_track_name,
                    mid,
                },
            ) => {
                if mid.is_some() {
                    if let Some(alias) = self.tracks.pending_alias(&sfu_track_name) {
                        self.health.pulled(alias);
                    }
                }
                let closing = self.tracks.pull_done(&sfu_track_name, mid);
                close(&mut fx, closing);
            }
            (_, Input::PullNotFound { sfu_track_name }) => {
                self.on_pull_not_found(&mut fx, &sfu_track_name);
            }
            (_, Input::Republished(result)) => self.on_republished(&mut fx, result),
            (_, Input::Stats { stats, at_ms }) => self.on_stats(&mut fx, &stats, at_ms),
            (_, Input::Command(command)) => self.on_command(&mut fx, command),
            (_, Input::Timer(timer)) => self.on_timer(&mut fx, timer),
            (_, Input::MicLevel { rms, at_ms }) => self.on_mic_level(&mut fx, rms, at_ms),
        }
        if !self.ended && self.state != CallState::Leaving {
            self.sync_media(&mut fx);
            self.sync_watch(&mut fx);
            if self.roster.is_some() && self.roster() != self.reported {
                self.emit_roster(&mut fx);
            }
        }
        fx
    }

    /// Brings pulls, audio gains, frame routes and camera layers in line
    /// with the offers and the user's intent.
    fn sync_media(&mut self, fx: &mut Vec<Effect>) {
        if self.connected {
            // Pull what is wanted once media is up (VOICE_PROTOCOL.md §3.3c).
            let (video, watched) = (&self.video, &self.watched);
            let mut pulls = self
                .tracks
                .take_pulls(|offer| wanted(offer, video, watched));
            for remote in &mut pulls {
                remote.fresh = self.closed_video.contains(&remote.track_name);
            }
            for remote in &pulls {
                if remote.kind == Kind::Video && is_camera(&remote.track_name) {
                    let quality = resolve_quality(video.demand.get(&remote.track_name).copied());
                    self.layers.insert(
                        remote.track_name.clone(),
                        Layer {
                            quality: Some(quality),
                            ..Layer::default()
                        },
                    );
                }
            }
            fx.extend(pulls.into_iter().map(Effect::Pull));
            self.sync_releases(fx);
        }
        self.sync_gains(fx);
        self.sync_routes(fx);
        self.sync_layers(fx);
    }

    /// Sets each pulled audio track's gain where it differs from what was
    /// set (a new mid starts at 1.0): its owner's volume; share audio takes
    /// their stream volume instead, and plays only while their share or
    /// camera is on stage or we always hear them. While we hear the room
    /// spatially, a voice track also takes its owner's distance gain and
    /// pan (`applySpatialAudio`: `cam-audio` only); otherwise pans are 0.
    fn sync_gains(&mut self, fx: &mut Vec<Effect>) {
        let mixes = self.spatial_mixes();
        let (roster, playback) = (&self.roster, &self.playback);
        let (gains, pans) = (&mut self.gains, &mut self.pans);
        let mut live = HashSet::new();
        for (alias, pull) in self.tracks.pulls() {
            let Some(mid) = pull.mid.as_deref().filter(|_| pull.kind == Kind::Audio) else {
                continue;
            };
            let owner = pull.participant_id.as_str();
            let user_id = roster
                .as_ref()
                .and_then(|r| r.user_id_of(owner))
                .unwrap_or("");
            let share = is_share(alias);
            let audible = !share
                || self.video.stage.as_deref() == Some(owner)
                || playback.always_hears(user_id);
            let place = if share {
                spatial::FLAT
            } else {
                mixes.get(user_id).copied().unwrap_or(spatial::FLAT)
            };
            let gain = match (audible, share) {
                (false, _) => 0.0,
                (true, true) => playback.stream_gain_for(user_id),
                (true, false) => playback.gain_for(user_id) * place.gain,
            };
            live.insert(mid);
            let set = gains.get(mid).copied().unwrap_or(1.0);
            if (set - gain).abs() > f64::EPSILON {
                gains.insert(mid.to_owned(), gain);
                fx.push(Effect::SetTrackGain {
                    mid: mid.to_owned(),
                    gain,
                });
            }
            let set = pans.get(mid).copied().unwrap_or(0.0);
            if (set - place.pan).abs() > f64::EPSILON {
                pans.insert(mid.to_owned(), place.pan);
                fx.push(Effect::SetTrackPan {
                    mid: mid.to_owned(),
                    pan: place.pan,
                });
            }
        }
        gains.retain(|mid, _| live.contains(mid.as_str()));
        pans.retain(|mid, _| live.contains(mid.as_str()));
    }

    /// Each other person's spatial mix by user id (empty while we do not
    /// hear the room spatially). The map lists us first, then the others in
    /// roster order, once each.
    fn spatial_mixes(&self) -> HashMap<String, spatial::Mix> {
        let local = &self.spatial_local;
        if !local.hears(&self.spatial_room, self.deafened) {
            return HashMap::new();
        }
        let Some(roster) = &self.roster else {
            return HashMap::new();
        };
        let me = if local.user_id.is_empty() {
            self.me.as_str()
        } else {
            local.user_id.as_str()
        };
        let mut seen = HashSet::new();
        let peers: Vec<String> = roster
            .snapshot()
            .into_iter()
            .filter(|m| !m.me && !m.user_id.is_empty() && m.user_id != me)
            .filter(|m| seen.insert(m.user_id.clone()))
            .map(|m| m.user_id)
            .collect();
        let peers: Vec<&str> = peers.iter().map(String::as_str).collect();
        spatial::mixes(me, &peers, &self.spatial_room, true)
    }

    /// Op 15 with our spatial switches and the room's state (with our
    /// `enabled` when `ours`, as `useSpatialAudioSync` sends it), once the
    /// room knows us.
    fn send_spatial(&mut self, fx: &mut Vec<Effect>, ours: bool) {
        if !self.sockets_open || !self.room_identified || self.standby {
            return;
        }
        if ours {
            self.spatial_room.enabled = self.spatial_local.enabled;
        }
        self.spatial_room.updated_at = wire::now_ms();
        fx.push(Effect::Send(
            Socket::Room,
            wire::spatial_state(
                self.spatial_local.enabled,
                self.spatial_local.high_fidelity,
                &self.spatial_room,
            ),
        ));
    }

    /// Routes a pulled video's frames to the UI while a visible tile shows
    /// it and it is not parked; otherwise they are dropped.
    fn sync_routes(&mut self, fx: &mut Vec<Effect>) {
        let mut live = HashSet::new();
        for (alias, pull) in self.tracks.pulls() {
            let Some(mid) = pull.mid.as_deref().filter(|_| pull.kind == Kind::Video) else {
                continue;
            };
            live.insert(mid.to_owned());
            let owner = pull.participant_id.as_str();
            let allowed = is_camera(alias)
                || self.watched.contains(owner)
                || self.video.stage.as_deref() == Some(owner);
            let shown = !pull.parked && allowed && self.video.demand.contains_key(alias);
            let current = self.routes.get(mid).map(String::as_str);
            let wanted = shown.then_some(alias);
            if current != wanted {
                match wanted {
                    Some(key) => self.routes.insert(mid.to_owned(), key.to_owned()),
                    None => self.routes.remove(mid),
                };
                fx.push(Effect::SetVideoSink {
                    mid: mid.to_owned(),
                    key: wanted.map(str::to_owned),
                });
            }
        }
        self.routes.retain(|mid, _| live.contains(mid));
    }

    /// Moves each pulled simulcast camera to the layer its largest render
    /// needs once that need has held for the upgrade or downgrade delay.
    fn sync_layers(&mut self, fx: &mut Vec<Effect>) {
        let pulled: HashSet<&str> = self.tracks.pulled_names().into_iter().collect();
        self.layers
            .retain(|alias, _| pulled.contains(alias.as_str()));
        let aliases: Vec<String> = self.layers.keys().cloned().collect();
        for alias in aliases {
            let has_layers = self
                .tracks
                .offer_of(&alias)
                .is_some_and(|o| o.simulcast_rids.len() >= 2);
            if !has_layers || !self.tracks.is_live(&alias) {
                continue;
            }
            let wanted = resolve_quality(self.video.demand.get(&alias).copied());
            let token = self.next_token + 1;
            let Some(layer) = self.layers.get_mut(&alias) else {
                continue;
            };
            if layer.quality == Some(wanted) {
                layer.pending = None;
                continue;
            }
            if layer.pending.is_some_and(|(q, _)| q == wanted) {
                continue;
            }
            let after = if layer.quality.is_some_and(|q| wanted > q) {
                LAYER_UPGRADE_DELAY
            } else {
                LAYER_DOWNGRADE_DELAY
            };
            self.next_token = token;
            layer.pending = Some((wanted, token));
            fx.push(Effect::ArmTimer {
                timer: Timer::Layer {
                    track: alias,
                    token,
                },
                after,
            });
        }
    }

    /// The `tracks/update` for `alias` at its current layer.
    fn layer_update(&self, alias: &str, switch_now: bool) -> Option<LayerUpdate> {
        let pull = self.tracks.pull_of(alias)?;
        let offer = self.tracks.offer_of(alias)?;
        let quality = self.layers.get(alias)?.quality?;
        Some(LayerUpdate {
            mid: pull.mid.clone()?,
            sfu_track_name: pull.sfu_track_name.clone(),
            session_id: pull.session_id.clone(),
            rid: select_rid(quality, &offer.simulcast_rids)?,
            switch_now,
        })
    }

    /// One level reading: the meter, then speaking (held off while muted).
    fn on_mic_level(&mut self, fx: &mut Vec<Effect>, rms: f64, at_ms: u64) {
        fx.push(Effect::Notify(Notice::MicLevel(level_percent(rms))));
        if self.published.is_none() || self.muted {
            return;
        }
        if self
            .vad
            .update(rms, self.sensitivity.threshold(), at_ms)
            .is_some()
        {
            self.send_speaking(fx);
        }
    }

    /// Op 5's flags as they are now: the microphone (the VAD) and our own
    /// soundboard clip (`refreshSpeakingState`).
    fn speaking_flags(&self) -> u8 {
        u8::from(self.vad.speaking())
            | if self.soundboard_speaking {
                wire::SPEAKING_SOUNDSHARE
            } else {
                0
            }
    }

    /// Our row and op 5 from the current flags.
    fn send_speaking(&mut self, fx: &mut Vec<Effect>) {
        let flags = self.speaking_flags();
        if let Some(roster) = &mut self.roster {
            roster.set_speaking(&self.me, flags != 0);
        }
        if self.voice_ready {
            fx.push(Effect::Send(Socket::Voice, wire::speaking_flags(flags)));
        }
        self.emit_roster(fx);
    }

    fn set_state(&mut self, fx: &mut Vec<Effect>, state: CallState) {
        if self.state != state {
            self.state = state;
            fx.push(Effect::Notify(Notice::State(state)));
        }
    }

    /// Ends the call for a reason of our own while the room may still hold
    /// our session: `ClientDisconnect` first, as a leave does, so the room
    /// drops the seat at once. A close alone reads as a lost connection,
    /// and the room keeps the seat for a resume that never comes (a
    /// reconnecting ghost for its 120 s grace). Ends the room or a socket
    /// decided (4008, 4010, a move) need no leave.
    fn fail(&mut self, fx: &mut Vec<Effect>, reason: String) {
        if self.sockets_open {
            self.disconnect(fx);
        }
        self.end(fx, Some(reason));
    }

    fn end(&mut self, fx: &mut Vec<Effect>, reason: Option<String>) {
        self.app_queue.clear();
        self.set_state(fx, CallState::Idle);
        self.ended = true;
        fx.push(Effect::End(reason));
    }

    fn set_status(&mut self, fx: &mut Vec<Effect>, status: &'static str) {
        if self.status != status {
            self.status = status;
            fx.push(Effect::Notify(Notice::Status(status)));
        }
    }

    /// The status line from what is up: "Connected" once room, voice and
    /// media are all up, "Reconnecting" when one dropped after that.
    fn refresh_status(&mut self, fx: &mut Vec<Effect>) {
        let status = if self.room_up && self.voice_ready && self.media_up {
            "Connected"
        } else if self.status == "Connected" || self.status == "Reconnecting" {
            "Reconnecting"
        } else {
            "Joining"
        };
        self.set_status(fx, status);
    }

    fn emit_roster(&mut self, fx: &mut Vec<Effect>) {
        self.reported = self.roster();
        fx.push(Effect::Notify(Notice::Roster(self.reported.clone())));
    }

    fn my_track(&self) -> String {
        format!("cam-audio-{}", self.me)
    }

    fn start(&mut self, fx: &mut Vec<Effect>) {
        self.set_state(fx, CallState::Connecting);
        self.set_status(fx, "Getting tickets");
        let (t, slug) = (&self.target, self.target.slug());
        fx.push(Effect::RequestAdmission(
            VoiceAdmissionRequest::join(&t.server_id, &t.channel_id, &slug)
                .with_move_grant(t.move_grant.as_deref()),
        ));
    }

    /// The join's admission: the credentials come from its `media`, not
    /// from the room (its Ready carries none, and no MediaReady comes).
    fn on_admitted(&mut self, fx: &mut Vec<Effect>, result: Result<VoiceAdmission, String>) {
        if self.sockets_open {
            return;
        }
        let admission = match result {
            Ok(admission) => admission,
            Err(error) => return self.end(fx, Some(error)),
        };
        let (Some(room_ticket), Some(voice_ticket), Some(media)) = (
            admission.room_ticket.filter(|t| !t.is_empty()),
            admission.voice_ticket.filter(|t| !t.is_empty()),
            admission.media.as_ref(),
        ) else {
            return self.end(fx, Some("The call admission was incomplete.".into()));
        };
        let Bootstrap {
            participant_id: me,
            voice_token,
            ice_servers,
        } = Bootstrap::from_media(media);
        self.me = me;
        self.voice_token = voice_token;
        self.roster = Some(Roster::new(&self.me, &self.target.me_name));
        self.sockets_open = true;
        self.standby = self.target.standby;

        let t = &self.target;
        let slug = t.slug();
        // A standby's voice socket connects but does not identify until the
        // promotion (the room never offers it anyone's tracks before).
        let voice_identify = if self.standby {
            Value::Null
        } else {
            wire::voice_identify(&self.me, &self.voice_token)
        };
        let sockets = Effect::OpenSockets {
            room: Box::new(SocketSpec {
                path: format!("/api/channels/{slug}/ws?v=1"),
                identify: wire::room_identify_as(&t.me_name, &t.me_username, self.standby),
                first_ticket: room_ticket,
                reconnect: VoiceAdmissionRequest::room_reconnect(
                    &t.server_id,
                    &t.channel_id,
                    &slug,
                ),
            }),
            voice: Box::new(SocketSpec {
                path: format!("/api/channels/{slug}/voice?v=1"),
                identify: voice_identify,
                first_ticket: voice_ticket,
                // The participant never changes in a call (a fresh one ends
                // it), so the binding is fixed.
                reconnect: VoiceAdmissionRequest::voice_reconnect(
                    &t.server_id,
                    &t.channel_id,
                    &slug,
                    &self.me,
                ),
            }),
        };
        self.ice_servers = ice_servers.clone();
        let media = Effect::OpenMedia(MediaSpec {
            ice_servers,
            stream_id: format!("cam-{}", self.me),
            slug,
            participant_id: self.me.clone(),
            voice_token: self.voice_token.clone(),
            standby: self.standby,
        });
        self.set_status(fx, "Joining");
        fx.push(sockets);
        fx.push(media);
        // The token arrived just now.
        self.arm_refresh(fx);
    }

    fn on_published(&mut self, fx: &mut Vec<Effect>, result: Result<Published, String>) {
        if self.published.is_some() || !self.sockets_open {
            return;
        }
        self.rebuilding = false;
        match result {
            Ok(published) => {
                self.published = Some(published);
                if self.muted || self.standby {
                    fx.push(Effect::SetMicEnabled(false));
                }
                if self.standby {
                    fx.push(Effect::ArmTimer {
                        timer: Timer::StandbyStale,
                        after: STANDBY_STALE_AFTER,
                    });
                }
                if self.deafened {
                    fx.push(Effect::SetPlayoutEnabled(false));
                }
                if self.processing != Processing::default() {
                    fx.push(Effect::SetProcessing(self.processing));
                }
                self.emit_roster(fx);
                // Only if the voice socket was ready first (the driver holds
                // socket events until now, so normally it is not).
                if self.voice_ready {
                    self.announce(fx);
                }
                self.publish_camera(fx);
                self.drive_share(fx);
            }
            Err(error) => self.fail(fx, error),
        }
    }

    /// Announces our publications (op 12): the mic, and the camera while it
    /// is on (a parked camera is never announced).
    fn announce(&self, fx: &mut Vec<Effect>) {
        let Some(p) = &self.published else {
            return;
        };
        let mic = self.my_track();
        fx.push(Effect::Send(
            Socket::Voice,
            wire::announce(
                &self.me,
                &Announced {
                    track_name: &mic,
                    sfu_track_name: &p.sfu_track_name,
                    session_id: &p.session_id,
                    kind: "audio",
                    simulcast_rids: &[],
                },
            ),
        ));
        self.announce_camera(fx);
        self.announce_share(fx, None);
    }

    /// Our camera's op 12, while it is on.
    fn announce_camera(&self, fx: &mut Vec<Effect>) {
        let Some(p) = &self.published else {
            return;
        };
        if let Some(camera) = self.camera.published.as_ref().filter(|_| self.camera.on) {
            let alias = camera_alias(&self.me);
            fx.push(Effect::Send(
                Socket::Voice,
                wire::announce(
                    &self.me,
                    &Announced {
                        track_name: &alias,
                        sfu_track_name: &camera.sfu_track_name,
                        session_id: &p.session_id,
                        kind: "video",
                        simulcast_rids: &camera.simulcast_rids,
                    },
                ),
            ));
        }
    }

    /// `connection-restored`: back to joined.
    fn restored(&mut self, fx: &mut Vec<Effect>) {
        if self.state == CallState::Reconnecting {
            self.set_state(fx, CallState::Joined);
        }
        self.refresh_status(fx);
    }

    /// `connection-lost`: the room socket or the peer connection dropped.
    fn lost(&mut self, fx: &mut Vec<Effect>) {
        if self.state == CallState::Joined {
            self.set_state(fx, CallState::Reconnecting);
        }
        self.refresh_status(fx);
    }

    /// Takes a fresh voice token from the room, for the SFU and the voice
    /// socket's next identify, and refreshes it again in
    /// [`CREDENTIAL_REFRESH`].
    fn update_token(&mut self, fx: &mut Vec<Effect>, token: &str) {
        if !token.is_empty() && token != self.voice_token {
            token.clone_into(&mut self.voice_token);
            fx.push(Effect::SetVoiceToken(self.voice_token.clone()));
            fx.push(Effect::SetVoiceIdentify(wire::voice_identify(
                &self.me,
                &self.voice_token,
            )));
            self.arm_refresh(fx);
        }
    }

    /// (Re)arms the credential refresh; an older one fires as a no-op.
    fn arm_refresh(&mut self, fx: &mut Vec<Effect>) {
        self.next_token += 1;
        self.refresh = self.next_token;
        fx.push(Effect::ArmTimer {
            timer: Timer::RefreshCredentials {
                token: self.refresh,
            },
            after: CREDENTIAL_REFRESH,
        });
    }

    fn on_room(&mut self, fx: &mut Vec<Effect>, event: RoomEvent) {
        match event {
            RoomEvent::Ready(d) => {
                if self.room_identified
                    && !d.participant_id.is_empty()
                    && d.participant_id != self.me
                {
                    // Resume failed and the room made a new participant: the
                    // voice grant and SFU session belong to the old one.
                    return self.fail(fx, CALL_RESET.into());
                }
                let again = std::mem::replace(&mut self.room_identified, true);
                self.room_up = true;
                self.replace_roster(&d);
                if self.standby {
                    // Hidden: no voice state, no join until the promotion.
                    self.emit_roster(fx);
                    return self.refresh_status(fx);
                }
                fx.push(Effect::Send(Socket::Room, self.voice_state()));
                self.send_spatial(fx, true);
                self.emit_roster(fx);
                match self.state {
                    CallState::Connecting => {
                        self.set_state(fx, CallState::Joined);
                        fx.push(Effect::Notify(Notice::Connected));
                    }
                    CallState::Joined | CallState::Reconnecting if again => {
                        fx.push(Effect::Log(
                            "[voice/room] identified fresh after a drop".into(),
                        ));
                        self.set_state(fx, CallState::Joined);
                        fx.push(Effect::Notify(Notice::Rejoined));
                    }
                    _ => {}
                }
                self.refresh_status(fx);
            }
            RoomEvent::Resumed(d) => {
                self.room_up = true;
                self.update_token(fx, &d.voice_token);
                self.replace_roster(&d);
                self.emit_roster(fx);
                self.restored(fx);
            }
            RoomEvent::VoiceState { leave, participant } => {
                let id = participant.id.as_str();
                if id.is_empty() || id == self.me {
                    return;
                }
                if leave {
                    if let Some(roster) = &mut self.roster {
                        roster.remove(id);
                    }
                    self.tracks.cancel_grace(id);
                    let closing = self.tracks.drop_participant(id, &HashSet::new());
                    close(fx, closing);
                } else if let Some(roster) = &mut self.roster {
                    roster.upsert(&participant);
                }
                self.emit_roster(fx);
            }
            RoomEvent::Spatial(state) => self.spatial_room = state,
            RoomEvent::Credentials { voice_token } => self.update_token(fx, &voice_token),
            RoomEvent::Moved(to) => {
                fx.push(Effect::Log(format!(
                    "[voice/room] moved to channel {}",
                    to.channel_id
                )));
                self.moved = true;
                fx.push(Effect::Notify(Notice::Moved(to)));
            }
            RoomEvent::Disconnected => {
                self.room_up = false;
                self.lost(fx);
            }
            RoomEvent::Stopped(reason) => self.stopped(fx, reason),
            RoomEvent::Closed => self.end(fx, Some("Room connection closed".into())),
        }
    }

    /// Who left the room is decided by the room alone: a Ready or Resumed
    /// roster drops the ones it does not list, with their media.
    fn replace_roster(&mut self, d: &RoomState) {
        let (Some(roster), Some(list)) = (&mut self.roster, &d.participants) else {
            return;
        };
        roster.replace_others(list);
    }

    fn on_voice(&mut self, fx: &mut Vec<Effect>, event: VoiceEvent) {
        match event {
            VoiceEvent::Ready { tracks, speaking } => {
                self.voice_ready = true;
                if std::mem::replace(&mut self.reidentified, true) {
                    fx.push(Effect::Log(
                        "[voice/voice] re-identified; re-announcing".into(),
                    ));
                    // VoiceReady lists every live track: drop pulls of tracks
                    // that went away during the blip, keep the rest.
                    let closing = self.tracks.keep_live(&tracks);
                    close(fx, closing);
                }
                // Announce (again): a re-identified socket is a new voice
                // session to everyone else, and so is our speaking bit. The
                // peer connection kept its publication, so this is all the
                // republish there is (STATE_MACHINE.md "Reconnect republish").
                self.announce(fx);
                let flags = self.speaking_flags();
                if flags != 0 {
                    fx.push(Effect::Send(Socket::Voice, wire::speaking_flags(flags)));
                }
                // Op 106 frames held while the socket was down go out now,
                // after the announce, before the apps react to the notice.
                for frame in std::mem::take(&mut self.app_queue) {
                    fx.push(Effect::Send(Socket::Voice, frame));
                }
                fx.push(Effect::Notify(Notice::VoiceReady {
                    participant_id: self.me.clone(),
                }));
                let closing = self.tracks.offer(&tracks, "", &self.me);
                close(fx, closing);
                if let Some(roster) = &mut self.roster {
                    for (pid, bit) in &speaking {
                        roster.set_speaking(pid, *bit);
                    }
                }
                self.emit_roster(fx);
                // A watch the voice room forgot with our old socket.
                self.announced_watch.clear();
                self.sync_watch(fx);
                self.restored(fx);
            }
            VoiceEvent::Video {
                participant_id,
                tracks,
            } => {
                let closing = self.tracks.offer(&tracks, &participant_id, &self.me);
                close(fx, closing);
            }
            VoiceEvent::Speaking {
                participant_id,
                speaking,
            } => {
                if speaking && participant_id != self.me {
                    self.health.heard_speaking();
                }
                let changed = self
                    .roster
                    .as_mut()
                    .is_some_and(|r| r.set_speaking(&participant_id, speaking));
                if changed {
                    self.emit_roster(fx);
                }
            }
            VoiceEvent::StopTracks {
                participant_id,
                names,
            } => {
                let closing = self.tracks.withdraw(&participant_id, &names);
                close(fx, closing);
            }
            VoiceEvent::ClientDisconnect { participant_id } => {
                // Their voice socket reconnects (or they are leaving; the
                // room says which). Keep their row and pulls for the grace;
                // speaking clears at once.
                if participant_id.is_empty() || participant_id == self.me {
                    return;
                }
                self.next_token += 1;
                self.tracks.start_grace(&participant_id, self.next_token);
                if let Some(roster) = &mut self.roster {
                    if roster.set_speaking(&participant_id, false) {
                        self.emit_roster(fx);
                    }
                }
                fx.push(Effect::ArmTimer {
                    timer: Timer::Grace {
                        participant: participant_id,
                        token: self.next_token,
                    },
                    after: REMOTE_RECONNECT_GRACE,
                });
            }
            VoiceEvent::App(d) => fx.push(Effect::Notify(Notice::App(d))),
            VoiceEvent::Republish(request) => self.on_republish_request(fx, &request),
            VoiceEvent::Rejected { message } => self.end(fx, Some(message)),
            VoiceEvent::IdentifyRejected => {
                // Expired voice token: the room's Resumed reply carries a
                // fresh one, which becomes the next identify.
                fx.push(Effect::Log(
                    "[voice] voice token refused; refreshing".into(),
                ));
                fx.push(Effect::Send(
                    Socket::Room,
                    wire::empty_frame(GatewayOp::RefreshVoiceCredentials),
                ));
            }
            VoiceEvent::Disconnected => {
                // Not a `connection-lost`: the phase stays.
                self.voice_ready = false;
                self.refresh_status(fx);
            }
            VoiceEvent::Stopped(reason) => self.stopped(fx, reason),
            VoiceEvent::Closed => self.end(fx, Some("Voice connection closed".into())),
        }
    }

    fn on_media(&mut self, fx: &mut Vec<Effect>, event: MediaEvent) {
        match event {
            MediaEvent::State(MediaState::Connected) => {
                self.connected = true;
                self.media_up = true;
                // The count starts over only once the link has held for a
                // while, not on every connect.
                self.next_token += 1;
                self.stable_token = self.next_token;
                fx.push(Effect::ArmTimer {
                    timer: Timer::MediaStable {
                        token: self.stable_token,
                    },
                    after: REBUILD_STABLE_AFTER,
                });
                self.restored(fx);
            }
            MediaEvent::State(MediaState::Disconnected) => {
                self.media_up = false;
                self.stable_token = 0;
                self.lost(fx);
            }
            // PartyTracks discards a failed session and starts again: a new
            // capability, our tracks pushed and announced again, the pulls
            // made again. A connection that keeps failing ends the call.
            // The rebuild waits out a backoff first.
            MediaEvent::State(MediaState::Failed) => {
                self.stable_token = 0;
                if self.rebuilding {
                    // The connection being replaced reporting again.
                } else if self.published.is_some() && self.rebuilds < MAX_REBUILDS {
                    self.schedule_rebuild(fx);
                } else {
                    self.fail(fx, "Media connection failed".into());
                }
            }
            MediaEvent::State(_) => {}
            MediaEvent::Closed => self.fail(fx, "Media connection ended".into()),
        }
    }

    /// Announces watch starts and stops (op 106) by the streamer's user id.
    fn sync_watch(&mut self, fx: &mut Vec<Effect>) {
        if !self.sockets_open {
            return;
        }
        let Some(roster) = &self.roster else {
            return;
        };
        let now: BTreeSet<String> = self
            .watched
            .iter()
            .filter_map(|pid| roster.user_id_of(pid))
            .map(str::to_owned)
            .collect();
        for user in now.difference(&self.announced_watch) {
            fx.push(Effect::Send(Socket::Voice, wire::watch_event(user, true)));
        }
        for user in self.announced_watch.difference(&now) {
            fx.push(Effect::Send(Socket::Voice, wire::watch_event(user, false)));
        }
        self.announced_watch = now;
    }

    fn on_command(&mut self, fx: &mut Vec<Effect>, command: UserCommand) {
        match command {
            UserCommand::SetMuted(muted) => {
                self.muted = muted;
                if self.published.is_some() {
                    fx.push(Effect::SetMicEnabled(!muted));
                }
                if let Some(roster) = &mut self.roster {
                    roster.set_my_mute(muted);
                }
                if muted && self.vad.reset() {
                    let flags = self.speaking_flags();
                    if let Some(roster) = &mut self.roster {
                        roster.set_speaking(&self.me, flags != 0);
                    }
                    if self.voice_ready {
                        fx.push(Effect::Send(Socket::Voice, wire::speaking_flags(flags)));
                    }
                }
                if self.sockets_open {
                    fx.push(Effect::Send(Socket::Room, self.voice_state()));
                }
                self.emit_roster(fx);
            }
            UserCommand::SetDeafened(deafened) => {
                self.deafened = deafened;
                if self.published.is_some() {
                    fx.push(Effect::SetPlayoutEnabled(!deafened));
                }
                if let Some(roster) = &mut self.roster {
                    roster.set_my_deaf(deafened);
                }
                if self.sockets_open {
                    fx.push(Effect::Send(Socket::Room, self.voice_state()));
                }
                self.emit_roster(fx);
            }
            UserCommand::SetSensitivity(sensitivity) => self.sensitivity = sensitivity,
            UserCommand::SetPlayback(playback) => self.playback = playback,
            UserCommand::SetProcessing(processing) => {
                if processing != self.processing {
                    self.processing = processing;
                    if self.published.is_some() {
                        fx.push(Effect::SetProcessing(processing));
                    }
                }
            }
            UserCommand::SetSpatial(local) => {
                let announce = local.enabled != self.spatial_local.enabled
                    || local.high_fidelity != self.spatial_local.high_fidelity;
                self.spatial_local = local;
                if announce {
                    self.send_spatial(fx, true);
                }
            }
            UserCommand::SetSpatialRoom(state) => {
                self.spatial_room = SpatialState {
                    enabled: self.spatial_room.enabled,
                    ..state
                };
                self.send_spatial(fx, false);
            }
            UserCommand::SetVideo(video) => self.video = video,
            UserCommand::SetWatched(watched) => {
                self.watched = watched;
                self.sync_watch(fx);
            }
            UserCommand::SetCamera(on) => self.set_camera(fx, on),
            UserCommand::StartShare(request) => self.start_share(fx, request),
            UserCommand::StopShare => self.stop_share(fx),
            UserCommand::SetShareAudio(on) => self.set_share_audio(fx, on),
            UserCommand::SetShareQuality(quality) => self.set_share_quality(fx, quality),
            UserCommand::SendApp(frame) => {
                if self.voice_ready {
                    fx.push(Effect::Send(Socket::Voice, frame));
                } else if self.sockets_open {
                    // The voice socket is joining or reconnecting: hold the
                    // frame for its VoiceReady instead of losing it.
                    if self.app_queue.len() >= APP_QUEUE_MAX {
                        self.app_queue.pop_front();
                    }
                    self.app_queue.push_back(frame);
                }
            }
            UserCommand::SetSoundboardSpeaking(on) => {
                if self.soundboard_speaking != on {
                    self.soundboard_speaking = on;
                    self.send_speaking(fx);
                }
            }
            UserCommand::Promote => self.promote(fx),
            UserCommand::Leave => self.leave(fx),
        }
    }

    /// Our voice state (op 15) as it is now.
    fn voice_state(&self) -> String {
        let share = self.share.live();
        wire::voice_state(SelfState {
            muted: self.muted,
            deafened: self.deafened,
            video: self.camera.on,
            stream: share.is_some(),
            stream_audio: share.is_some_and(|s| s.audio),
        })
    }

    /// The camera toggle (useVoiceChannel.ts `toggleCamera`). On pushes it,
    /// or resumes the track it kept; it counts as on once that lands. Off
    /// parks it on the keepalive at once: StopTracks (op 13) for its alias
    /// and `self_video: false`.
    fn set_camera(&mut self, fx: &mut Vec<Effect>, on: bool) {
        self.camera.wanted = on;
        if on {
            self.publish_camera(fx);
        } else if self.camera.on {
            self.camera.on = false;
            fx.push(Effect::SetCameraLive(false));
            if self.voice_ready {
                let alias = camera_alias(&self.me);
                fx.push(Effect::Send(Socket::Voice, wire::stop_tracks(&[&alias])));
            }
            self.camera_changed(fx, None);
        }
    }

    /// Starts the push or resume a wanted camera needs, once the mic is
    /// published (its push opened the peer connection).
    fn publish_camera(&mut self, fx: &mut Vec<Effect>) {
        let camera = &mut self.camera;
        if !camera.wanted || camera.on || camera.pushing || self.published.is_none() {
            return;
        }
        camera.pushing = true;
        fx.push(Effect::PublishCamera {
            stream_id: format!("cam-{}", self.me),
        });
    }

    fn on_camera_published(
        &mut self,
        fx: &mut Vec<Effect>,
        result: Result<CameraPublished, String>,
    ) {
        if !std::mem::replace(&mut self.camera.pushing, false) {
            return;
        }
        match result {
            Ok(published) => {
                self.camera.published = Some(published);
                if !self.camera.wanted {
                    // Turned off while it was pushed: it stays parked.
                    return;
                }
                self.camera.on = true;
                fx.push(Effect::SetCameraLive(true));
                // One op 12 for this publication; a voice socket that is
                // not ready announces everything on its VoiceReady.
                if self.voice_ready {
                    self.announce_camera(fx);
                }
                self.camera_changed(fx, None);
            }
            Err(error) => {
                self.camera.wanted = false;
                fx.push(Effect::Log(format!(
                    "[voice] camera publish failed: {error}"
                )));
                self.camera_changed(fx, Some(error));
            }
        }
    }

    /// Our camera state, out to the room (op 15), our row and the UI.
    fn camera_changed(&mut self, fx: &mut Vec<Effect>, error: Option<String>) {
        if let Some(roster) = &mut self.roster {
            roster.set_my_video(self.camera.on);
        }
        if self.sockets_open {
            fx.push(Effect::Send(Socket::Room, self.voice_state()));
        }
        fx.push(Effect::Notify(Notice::Camera {
            on: self.camera.on,
            error,
        }));
        self.emit_roster(fx);
    }

    /// The one teardown (STATE_MACHINE.md "Teardown"). Local media first,
    /// then, after the send window for a joined call (none otherwise), op 11
    /// on the room socket, then on the voice socket, and the end, which
    /// closes the room socket, the voice socket and the peer connection in
    /// that order.
    fn leave(&mut self, fx: &mut Vec<Effect>) {
        self.app_queue.clear();
        if !self.sockets_open {
            return self.end(fx, None);
        }
        self.release_shares(fx);
        fx.push(Effect::StopLocalMedia);
        if matches!(self.state, CallState::Joined | CallState::Reconnecting) {
            fx.push(Effect::ArmTimer {
                timer: Timer::Linger,
                after: LEAVE_SEND_WINDOW,
            });
            self.set_state(fx, CallState::Leaving);
        } else {
            self.disconnect(fx);
            self.end(fx, None);
        }
    }

    /// `ClientDisconnect` (op 11, `d: {}`) on the room socket, then on the
    /// voice socket. A socket that is not ready drops it when it closes.
    fn disconnect(&self, fx: &mut Vec<Effect>) {
        for socket in [Socket::Room, Socket::Voice] {
            fx.push(Effect::Send(
                socket,
                wire::empty_frame(GatewayOp::ClientDisconnect),
            ));
        }
    }

    fn on_timer(&mut self, fx: &mut Vec<Effect>, timer: Timer) {
        match timer {
            Timer::Grace { participant, token } => {
                let Some(reoffered) = self.tracks.expire_grace(&participant, token) else {
                    return;
                };
                // Their row stays: only the room says who left. What they
                // did not offer again goes.
                let closing = self.tracks.drop_participant(&participant, &reoffered);
                close(fx, closing);
            }
            Timer::Linger => {}
            Timer::Layer { track, token } => {
                let Some(layer) = self.layers.get_mut(&track) else {
                    return;
                };
                let Some((quality, _)) = layer.pending.filter(|(_, t)| *t == token) else {
                    return;
                };
                layer.pending = None;
                layer.quality = Some(quality);
                self.next_token += 1;
                layer.settle = self.next_token;
                if let Some(update) = self.layer_update(&track, true) {
                    fx.push(Effect::UpdateLayer(update));
                    fx.push(Effect::ArmTimer {
                        timer: Timer::LayerSettle {
                            track,
                            token: self.next_token,
                        },
                        after: LAYER_SWITCH_SETTLE,
                    });
                }
            }
            Timer::LayerSettle { track, token } => {
                let current = self.layers.get(&track).is_some_and(|l| l.settle == token);
                if let Some(update) = self.layer_update(&track, false).filter(|_| current) {
                    fx.push(Effect::UpdateLayer(update));
                }
            }
            Timer::Stats => {
                if self.connected {
                    fx.push(Effect::LogStats);
                }
            }
            Timer::ReleaseVideo { track, token } => self.release_video(fx, &track, token),
            Timer::RepublishCooldown { track } => self.health.cooled(&track),
            Timer::StandbyStale => {
                if self.standby {
                    self.standby_stale = true;
                }
            }
            Timer::Rebuild { token } => {
                if self.rebuilding && token == self.rebuild_token && self.published.is_some() {
                    self.rebuild(fx);
                }
            }
            Timer::MediaStable { token } => {
                if token == self.stable_token && self.media_up {
                    self.rebuilds = 0;
                }
            }
            Timer::RefreshCredentials { token } => {
                if self.sockets_open && token == self.refresh {
                    fx.push(Effect::Send(
                        Socket::Room,
                        wire::empty_frame(GatewayOp::RefreshVoiceCredentials),
                    ));
                    // Asked again later if this reply never brings a token
                    // (as mobile does); a token that arrives re-arms it.
                    self.arm_refresh(fx);
                }
            }
        }
    }
}

impl CallMachine {
    /// A socket stopped for good. After a move the 4010 is expected: the
    /// call just ends (the owner joins the target).
    fn stopped(&mut self, fx: &mut Vec<Effect>, reason: StopReason) {
        let moved_close = matches!(
            &reason,
            StopReason::Replaced { reason, .. } if reason == VOICE_MOVED_CLOSE_REASON
        );
        if self.moved || moved_close {
            self.end(fx, None);
        } else {
            self.end(fx, Some(stop_message(reason)));
        }
    }

    /// The peer connection failed: a new SFU session (PartyTracks' recovery,
    /// VOICE_PROTOCOL.md §3.3 "Recovery"). Pulls start again once it
    /// connects; our mic (and a camera that was on) is pushed again and
    /// announced under its new capability. A share in flight ends.
    fn schedule_rebuild(&mut self, fx: &mut Vec<Effect>) {
        self.rebuilding = true;
        self.next_token += 1;
        self.rebuild_token = self.next_token;
        // 1 s, 2 s, 4 s: doubling per rebuild already spent.
        let after = REBUILD_BACKOFF * 2u32.saturating_pow(self.rebuilds);
        fx.push(Effect::Log(format!(
            "[voice] peer connection failed; rebuilding in {} ms",
            after.as_millis()
        )));
        fx.push(Effect::ArmTimer {
            timer: Timer::Rebuild {
                token: self.rebuild_token,
            },
            after,
        });
        self.media_up = false;
        self.lost(fx);
    }

    fn rebuild(&mut self, fx: &mut Vec<Effect>) {
        self.rebuilds += 1;
        fx.push(Effect::Log(format!(
            "[voice] peer connection failed; rebuilding the SFU session ({}/{MAX_REBUILDS})",
            self.rebuilds
        )));
        self.published = None;
        self.connected = false;
        self.media_up = false;
        self.tracks.reset_pulls();
        self.gains.clear();
        self.routes.clear();
        self.layers.clear();
        self.releasing.clear();
        // Every closed slot belonged to the old connection.
        self.closed_video.clear();
        self.health.reset_media();
        self.camera.pushing = false;
        self.camera.published = None;
        self.camera.on = false;
        self.drop_shares_for_rebuild(fx);
        fx.push(Effect::RebuildMedia(MediaSpec {
            ice_servers: self.ice_servers.clone(),
            slug: self.target.slug(),
            participant_id: self.me.clone(),
            voice_token: self.voice_token.clone(),
            stream_id: format!("cam-{}", self.me),
            // A standby stays hidden on its new session too.
            standby: self.standby,
        }));
        self.lost(fx);
    }

    /// A standby becomes the join: the room hears `promote_standby` (with
    /// mute and deafen), the voice socket identifies, capture starts.
    fn promote(&mut self, fx: &mut Vec<Effect>) {
        if !std::mem::replace(&mut self.standby, false) {
            return;
        }
        fx.push(Effect::Log("[voice] standby promoted".into()));
        fx.push(Effect::Send(
            Socket::Room,
            wire::promote_standby(self.muted, self.deafened),
        ));
        fx.push(Effect::SetVoiceIdentify(wire::voice_identify(
            &self.me,
            &self.voice_token,
        )));
        self.send_spatial(fx, true);
        if self.published.is_some() {
            fx.push(Effect::PromoteMedia);
            fx.push(Effect::SetMicEnabled(!self.muted));
            if std::mem::take(&mut self.standby_stale) {
                fx.push(Effect::Log(
                    "[voice] standby mic idle too long; republishing".into(),
                ));
                fx.push(Effect::Republish(Republish::Mic));
            }
        }
        if self.room_identified && self.state == CallState::Connecting {
            self.set_state(fx, CallState::Joined);
            fx.push(Effect::Notify(Notice::Connected));
        }
        self.refresh_status(fx);
    }

    /// Arms a release for each live video pull nothing wants any more, and
    /// cancels the ones wanted again.
    fn sync_releases(&mut self, fx: &mut Vec<Effect>) {
        let (video, watched) = (&self.video, &self.watched);
        let mut unwanted = HashSet::new();
        for (alias, pull) in self.tracks.pulls() {
            if pull.kind != Kind::Video || pull.mid.is_none() || pull.parked {
                continue;
            }
            let Some(offer) = self.tracks.offer_of(alias) else {
                continue;
            };
            if wanted(offer, video, watched).is_none() {
                unwanted.insert(alias.to_owned());
            }
        }
        self.releasing.retain(|alias, _| unwanted.contains(alias));
        for alias in unwanted {
            if self.releasing.contains_key(&alias) {
                continue;
            }
            self.next_token += 1;
            self.releasing.insert(alias.clone(), self.next_token);
            fx.push(Effect::ArmTimer {
                timer: Timer::ReleaseVideo {
                    track: alias,
                    token: self.next_token,
                },
                after: VIDEO_RELEASE_DELAY,
            });
        }
    }

    /// Closes a video pull that stayed unwanted; a later pull of it runs on
    /// a fresh receive session.
    fn release_video(&mut self, fx: &mut Vec<Effect>, alias: &str, token: u64) {
        if self.releasing.get(alias) != Some(&token) {
            return;
        }
        self.releasing.remove(alias);
        if let Some(mid) = self.tracks.release(alias) {
            fx.push(Effect::Log(format!(
                "[voice] releasing {alias}: nobody shows it"
            )));
            self.layers.remove(alias);
            self.closed_video.insert(alias.to_owned());
            close(fx, vec![mid]);
        }
    }
}

/// The alias of `pid`'s camera.
pub fn camera_alias(pid: &str) -> String {
    format!("cam-video-{pid}")
}

/// The alias of `pid`'s screen share video.
pub fn screen_alias(pid: &str) -> String {
    format!("screen-video-{pid}")
}

/// The alias of `pid`'s screen share audio.
pub fn screen_audio_alias(pid: &str) -> String {
    format!("screen-audio-{pid}")
}

fn is_camera(alias: &str) -> bool {
    alias.starts_with("cam-")
}

fn is_share(alias: &str) -> bool {
    alias.starts_with("screen-")
}

/// Whether to pull `offer`, and at which layer: audio always (share audio
/// too: audibility is only its gain); a camera once a visible tile renders
/// it, at the layer for that size; a share once watched or on stage.
fn wanted(
    offer: &Offer,
    video: &VideoIntent,
    watched: &BTreeSet<String>,
) -> Option<Option<String>> {
    match offer.kind {
        Kind::Audio => Some(None),
        Kind::Video if is_camera(&offer.track_name) => {
            let size = video.demand.get(&offer.track_name)?;
            Some(select_rid(
                resolve_quality(Some(*size)),
                &offer.simulcast_rids,
            ))
        }
        Kind::Video => {
            let pid = offer.participant_id.as_str();
            // Watched, or its own tile shows it (on stage). Their camera on
            // stage pulls it only as watched (SCREEN_SHARE.md "Watching").
            (watched.contains(pid) || video.demand.contains_key(&offer.track_name))
                .then(|| select_rid(Quality::High, &offer.simulcast_rids))
        }
    }
}

fn close(fx: &mut Vec<Effect>, mids: Vec<String>) {
    if !mids.is_empty() {
        fx.push(Effect::Close(mids));
    }
}

/// What the call shows when a socket stops for good.
pub fn stop_message(reason: StopReason) -> String {
    match reason {
        StopReason::Replaced { reason, .. } if reason == VOICE_MOVED_CLOSE_REASON => {
            "You were moved to another voice channel.".into()
        }
        StopReason::Replaced { .. } => "You joined this call somewhere else.".into(),
        StopReason::AdmissionRejected { .. } => "The call refused this join.".into(),
        StopReason::SessionExpired => "Your session expired. Sign in again.".into(),
    }
}
