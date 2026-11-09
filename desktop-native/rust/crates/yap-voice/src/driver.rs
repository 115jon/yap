//! [`run_call`]: the async shell around a [`CallMachine`]. It turns socket,
//! media, pull and timer events into machine inputs and runs the effects
//! that come back: the two `yap_realtime` connections, the SFU, the media
//! engine and one worker that serializes pulls and closes. Share captures
//! run on the app's [`ShareCapturer`], started off the async runtime.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::{Instant, Interval, MissedTickBehavior};
use yap_protocol::rest::{CallSocket, VoiceAdmission, VoiceAdmissionRequest};
use yap_protocol::{GatewayCloseCode, GatewayOp};
use yap_realtime::{Config, Connection, Event, Policy, Reconnect, TicketError, TicketFuture};

use crate::engine::{MediaEngine, MediaSession};
use crate::machine::{
    CallMachine, CallTarget, CameraPublished, Effect, Input, MediaEvent, MediaSpec, MediaState,
    Notice, Published, Republish, RoomEvent, Socket, SocketSpec, Timer, UserCommand, VoiceEvent,
};
use crate::sfu::{LayerUpdate, Sfu, Signaling};
use crate::share::{ShareCaptured, ShareCapturer, ShareEncoding, SharePublished};
use crate::spatial::SpatialState;
use crate::tracks::Remote;
use crate::vad::{EnergyMeter, VAD_TICK};
use crate::wire::{self, Participant, RoomState, Track};

/// A failed API request, as the admission endpoint reports it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ApiFailure {
    /// 401: counts towards "session expired" on reconnects.
    pub unauthorized: bool,
    pub message: String,
}

/// `POST /api/voice/admission` with the signed-in session
/// (`realtime/ADMISSION.md`): the JSON body in, the JSON reply out.
pub trait AdmissionApi: Clone + Send + Sync + 'static {
    fn admission(&self, body: Value) -> impl Future<Output = Result<Value, ApiFailure>> + Send;
}

/// Where user commands come from. `None` (closed) counts as Leave.
pub trait Commands: Send {
    fn recv(&mut self) -> impl Future<Output = Option<UserCommand>> + Send;
}

impl Commands for UnboundedReceiver<UserCommand> {
    async fn recv(&mut self) -> Option<UserCommand> {
        UnboundedReceiver::recv(self).await
    }
}

/// What a call runs on.
pub struct CallEnv<E, T, S> {
    pub engine: E,
    /// The call sockets' tickets and the media credentials.
    pub admission: T,
    /// Screen share capture.
    pub capturer: S,
    pub http: reqwest::Client,
    /// The Yap API base, e.g. `https://yap.115jon.com`.
    pub api_base: String,
    /// The `Origin` for the call sockets.
    pub origin: String,
}

/// Runs one call until it is left or ends. Reports through `notify`;
/// returns why it failed (`None`: left). The engine is released at the end.
pub async fn run_call<E, T, S, C, N>(
    env: CallEnv<E, T, S>,
    target: CallTarget,
    mut commands: C,
    notify: N,
) -> Option<String>
where
    E: MediaEngine,
    T: AdmissionApi,
    S: ShareCapturer,
    C: Commands,
    N: Fn(Notice) + Sync,
{
    let (share_events, shares) = mpsc::unbounded_channel();
    let mut io = Io::new(&env, share_events);
    let mut inbox = Inbox::new(shares);
    let reason = io
        .run(CallMachine::new(target), &mut inbox, &mut commands, &notify)
        .await;
    io.teardown();
    drop(inbox);
    env.engine.release();
    reason
}

/// A job for the pull worker.
enum Job {
    Pull(Remote),
    /// Stop and close the receive transceivers with these mids.
    Close(Vec<String>),
    /// Switch a pulled camera's simulcast layer (no renegotiation).
    Update(LayerUpdate),
    /// Push our camera, or resume the track it kept.
    PublishCamera {
        stream_id: String,
    },
    /// Push (or resume) our share's video and push or drop its audio.
    PublishShare {
        stream_id: String,
        encoding: ShareEncoding,
        audio: bool,
    },
    /// Park the share's video and drop its audio.
    UnpublishShare,
    /// Push one of ours again under a new SFU track name.
    Republish(Republish),
}

/// A job for one video session (a fresh pull's own peer connection).
enum VideoJob {
    /// Close its pulls at the SFU, drain, then drop the connection.
    Close(Vec<String>),
    Update(LayerUpdate),
}

/// How long a video session with nothing pulled lives on, so the SFU can
/// take its `tracks/close` (`VIDEO_SESSION_DRAIN_MS`).
const VIDEO_SESSION_DRAIN: Duration = Duration::from_secs(3);

/// How often the share's hardware encoder is checked for a failure (a
/// failed one is republished on VP8 at once, so viewers see a short pause,
/// not a frozen share).
const SHARE_ENCODER_CHECK: Duration = Duration::from_millis(500);

/// The stats tick: the voice details' samples (the web's useVoiceStats.ts
/// 2 s, so its 24 samples span 48 s).
pub const STATS_EVERY: Duration = Duration::from_secs(2);

/// The stats line goes to the log at most this often.
const STATS_LOG_EVERY: Duration = Duration::from_secs(5);

/// Mids of a video session's pulls carry its number: `v<n>:<mid>`.
fn video_mid(mid: &str) -> Option<(u32, &str)> {
    let rest = mid.strip_prefix('v')?;
    let (n, mid) = rest.split_once(':')?;
    Some((n.parse().ok()?, mid))
}

/// `YAP_TEST_PC_FAIL_AFTER_SECS=<n>`: a test hook. `n` s after media first
/// connects, the call sees its peer connection fail once (the SFU session
/// rebuild path). Unset: off.
fn test_fail_after() -> Option<Duration> {
    std::env::var("YAP_TEST_PC_FAIL_AFTER_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .map(Duration::from_secs_f64)
}

/// `YAP_TEST_VOICE_FAIL=ice` (or `media`): a test hook, read once. The
/// peer connection never comes up: each time it reports connected, the
/// call sees it fail instead, so a join is rebuilt and finally given up
/// ("Media connection failed"), as a network that blocks ICE would. The
/// other stage, `admission`, is the app's (yap_core::faults). Unset: off.
fn test_ice_fail() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| is_ice_stage(&std::env::var("YAP_TEST_VOICE_FAIL").unwrap_or_default()))
}

fn is_ice_stage(value: &str) -> bool {
    matches!(value.trim().to_ascii_lowercase().as_str(), "ice" | "media")
}

/// What the worker reports back.
#[derive(Debug, PartialEq)]
enum Done {
    /// (SFU track name, mid if the pull worked).
    Pull(String, Option<String>),
    Camera(Result<CameraPublished, String>),
    Share(Result<SharePublished, String>),
    /// The pull of this SFU track kept getting `not_found_track_error`.
    NotFound(String),
    Republished(Result<(Republish, String), String>),
}

/// A share capture's news, from the blocking start or the capture itself.
enum ShareEvent<C> {
    Started {
        token: u64,
        result: Result<(C, ShareCaptured), String>,
    },
    Ended(u64),
}

/// Event sources that appear as the call opens.
struct Inbox<C> {
    shares: UnboundedReceiver<ShareEvent<C>>,
    room: Option<UnboundedReceiver<Event>>,
    voice: Option<UnboundedReceiver<Event>>,
    media: Option<UnboundedReceiver<crate::machine::MediaState>>,
    pulls: Option<UnboundedReceiver<Done>>,
}

impl<C> Inbox<C> {
    fn new(shares: UnboundedReceiver<ShareEvent<C>>) -> Self {
        Self {
            shares,
            room: None,
            voice: None,
            media: None,
            pulls: None,
        }
    }
}

/// What the effects act on.
struct Io<'a, E: MediaEngine, T, S: ShareCapturer> {
    env: &'a CallEnv<E, T, S>,
    /// Running share captures by token; dropping one releases it. Behind a
    /// lock only so `Io` stays `Sync` across awaits (captures need not be).
    captures: Mutex<HashMap<u64, S::Capture>>,
    share_events: UnboundedSender<ShareEvent<S::Capture>>,
    room: Option<Connection>,
    voice: Option<Connection>,
    session: Option<E::Session>,
    sfu: Option<Sfu>,
    jobs: Option<UnboundedSender<Job>>,
    worker: Option<JoinHandle<()>>,
    /// Where pull results go (video sessions report there too).
    done: Option<UnboundedSender<Done>>,
    /// The ICE servers media opened with (video sessions use them).
    ice_servers: Vec<crate::wire::IceServer>,
    /// Open video sessions by number: the session and its job queue.
    videos: HashMap<u32, (E::Session, UnboundedSender<VideoJob>)>,
    next_video: u32,
    /// The test hook's failure, once media connected.
    fail_at: Option<Instant>,
    fail_armed: bool,
    timers: Vec<(Instant, Timer)>,
    /// The mic level between two reads of the energy counters.
    meter: EnergyMeter,
    /// The VAD's clock.
    started: Instant,
    /// When the stats line last went to the log.
    stats_logged: Option<Instant>,
}

enum Applied {
    Done,
    Input(Input),
    End(Option<String>),
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Waits on an event source that may not be open yet (then: forever).
async fn recv<T>(rx: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

impl<'a, E: MediaEngine, T: AdmissionApi, S: ShareCapturer> Io<'a, E, T, S> {
    fn new(
        env: &'a CallEnv<E, T, S>,
        share_events: UnboundedSender<ShareEvent<S::Capture>>,
    ) -> Self {
        Self {
            env,
            captures: Mutex::new(HashMap::new()),
            share_events,
            room: None,
            voice: None,
            session: None,
            sfu: None,
            jobs: None,
            worker: None,
            done: None,
            ice_servers: Vec::new(),
            videos: HashMap::new(),
            next_video: 0,
            fail_at: None,
            fail_armed: test_fail_after().is_some(),
            timers: Vec::new(),
            meter: EnergyMeter::default(),
            started: Instant::now(),
            stats_logged: None,
        }
    }

    /// The mic level since the last read, once the mic is published.
    async fn mic_level(&mut self) -> Option<f64> {
        let (energy, duration) = self.session.as_ref()?.mic_energy().await?;
        self.meter.read(energy, duration)
    }

    async fn run<C: Commands, N: Fn(Notice) + Sync>(
        &mut self,
        mut machine: CallMachine,
        inbox: &mut Inbox<S::Capture>,
        commands: &mut C,
        notify: &N,
    ) -> Option<String> {
        let mut queue: VecDeque<Effect> = machine.handle(Input::Start).into();
        let mut stats: Option<Interval> = None;
        let mut level = tokio::time::interval(VAD_TICK);
        level.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut encoder_check = tokio::time::interval(SHARE_ENCODER_CHECK);
        encoder_check.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut commands_open = true;
        loop {
            // Effects run in order. The admission and media are awaited
            // here, so socket events wait in their channels until the mic
            // is published (VOICE_PROTOCOL.md §6). The admission listens for
            // commands meanwhile: a Leave drops it before any socket opens.
            while let Some(effect) = queue.pop_front() {
                if let Effect::RequestAdmission(request) = effect {
                    let fetch = self.admit(request);
                    tokio::pin!(fetch);
                    let admitted = loop {
                        tokio::select! {
                            admission = &mut fetch => break Some(admission),
                            command = commands.recv(), if commands_open => {
                                let command = command.unwrap_or_else(|| {
                                    commands_open = false;
                                    UserCommand::Leave
                                });
                                let fx = machine.handle(Input::Command(command));
                                let ended = fx.iter().any(|e| matches!(e, Effect::End(_)));
                                queue.extend(fx);
                                if ended {
                                    break None;
                                }
                            }
                        }
                    };
                    if let Some(admission) = admitted {
                        queue.extend(machine.handle(Input::Admitted(admission)));
                    }
                    continue;
                }
                match self.apply(effect, inbox, notify).await {
                    Applied::Done => {}
                    Applied::Input(input) => queue.extend(machine.handle(input)),
                    Applied::End(reason) => return reason,
                }
            }
            let stats = stats.get_or_insert_with(|| tokio::time::interval(STATS_EVERY));
            let next_timer = self.timers.iter().map(|(at, _)| *at).min();
            let input = tokio::select! {
                event = recv(&mut inbox.room) => match event {
                    Some(event) => {
                        // The room's spatial state goes in first, so the
                        // frame's own event (a Ready sends ours) sees it.
                        if let Some(state) = spatial_of(&event) {
                            queue.extend(machine.handle(Input::Room(RoomEvent::Spatial(state))));
                        }
                        match room_event(event) {
                            Some(event) => Input::Room(event),
                            None => continue,
                        }
                    }
                    None => Input::Room(RoomEvent::Closed),
                },
                event = recv(&mut inbox.voice) => match event {
                    Some(event) => match voice_event(event) {
                        Some(event) => Input::Voice(event),
                        None => continue,
                    },
                    None => Input::Voice(VoiceEvent::Closed),
                },
                state = recv(&mut inbox.media) => Input::Media(match state {
                    Some(MediaState::Connected) if test_ice_fail() => {
                        eprintln!("[voice] test hook: ICE fails (YAP_TEST_VOICE_FAIL)");
                        MediaEvent::State(MediaState::Failed)
                    }
                    Some(state) => {
                        if state == MediaState::Connected && std::mem::take(&mut self.fail_armed) {
                            if let Some(after) = test_fail_after() {
                                eprintln!("[voice] test hook: failing the peer connection in {after:?}");
                                self.fail_at = Some(Instant::now() + after);
                            }
                        }
                        MediaEvent::State(state)
                    }
                    None => MediaEvent::Closed,
                }),
                () = tokio::time::sleep_until(self.fail_at.unwrap_or_else(Instant::now)), if self.fail_at.is_some() => {
                    self.fail_at = None;
                    eprintln!("[voice] test hook: peer connection failed");
                    Input::Media(MediaEvent::State(MediaState::Failed))
                },
                done = recv(&mut inbox.pulls) => match done {
                    Some(Done::Pull(sfu_track_name, mid)) => Input::PullDone { sfu_track_name, mid },
                    Some(Done::Camera(result)) => Input::CameraPublished(result),
                    Some(Done::Share(result)) => Input::SharePublished(result),
                    Some(Done::NotFound(sfu_track_name)) => Input::PullNotFound { sfu_track_name },
                    Some(Done::Republished(result)) => Input::Republished(result),
                    None => {
                        inbox.pulls = None;
                        continue;
                    }
                },
                Some(event) = inbox.shares.recv() => match event {
                    ShareEvent::Started { token, result } => Input::ShareCaptured {
                        token,
                        result: result.map(|(capture, captured)| {
                            lock(&self.captures).insert(token, capture);
                            captured
                        }),
                    },
                    ShareEvent::Ended(token) => Input::ShareCaptureEnded { token },
                },
                command = commands.recv(), if commands_open => match command {
                    Some(command) => Input::Command(command),
                    None => {
                        commands_open = false;
                        Input::Command(UserCommand::Leave)
                    }
                },
                () = tokio::time::sleep_until(next_timer.unwrap_or_else(Instant::now)), if next_timer.is_some() => {
                    let now = Instant::now();
                    let (due, later): (Vec<_>, Vec<_>) =
                        std::mem::take(&mut self.timers).into_iter().partition(|(at, _)| *at <= now);
                    self.timers = later;
                    for (_, timer) in due {
                        queue.extend(machine.handle(Input::Timer(timer)));
                    }
                    continue;
                },
                _ = level.tick(), if self.session.is_some() => match self.mic_level().await {
                    Some(rms) => Input::MicLevel {
                        rms,
                        at_ms: u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX),
                    },
                    None => continue,
                },
                _ = encoder_check.tick(), if self.session.is_some() => {
                    if self.session.as_ref().is_some_and(MediaSession::take_screen_failure) {
                        eprintln!("[share] hardware encoder failed mid-share");
                        Input::ShareEncoderFailed
                    } else {
                        continue;
                    }
                },
                _ = stats.tick() => Input::Timer(Timer::Stats),
            };
            queue.extend(machine.handle(input));
        }
    }

    async fn apply<N: Fn(Notice)>(
        &mut self,
        effect: Effect,
        inbox: &mut Inbox<S::Capture>,
        notify: &N,
    ) -> Applied {
        match effect {
            Effect::Notify(notice) => notify(notice),
            Effect::RequestAdmission(request) => {
                return Applied::Input(Input::Admitted(self.admit(request).await));
            }
            Effect::OpenSockets { room, voice } => self.open_sockets(*room, *voice, inbox),
            Effect::OpenMedia(spec) => {
                let result = self.open_media(spec, inbox).await;
                return Applied::Input(Input::Published(result.map_err(|e| format!("{e:#}"))));
            }
            Effect::RebuildMedia(spec) => {
                self.drop_media(inbox);
                let result = self.open_media(spec, inbox).await;
                return Applied::Input(Input::Published(result.map_err(|e| format!("{e:#}"))));
            }
            Effect::Republish(kind) => self.job(Job::Republish(kind)),
            Effect::PromoteMedia => {
                if let Some(session) = &self.session {
                    match session.promote_mic() {
                        Ok(()) => session.start_capture(),
                        Err(error) => eprintln!("[voice] standby promotion: {error}"),
                    }
                }
            }
            Effect::Send(socket, text) => {
                let connection = match socket {
                    Socket::Room => &self.room,
                    Socket::Voice => &self.voice,
                };
                if let Some(connection) = connection {
                    connection.send(text);
                }
            }
            Effect::SetVoiceIdentify(frame) => {
                if let Some(voice) = &self.voice {
                    voice.set_identify(frame);
                }
            }
            Effect::SetVoiceToken(token) => {
                if let Some(sfu) = &self.sfu {
                    sfu.set_voice_token(&token);
                }
            }
            Effect::Pull(remote) if remote.fresh => self.pull_fresh(remote),
            Effect::Pull(remote) => self.job(Job::Pull(remote)),
            Effect::Close(mids) => self.close_mids(mids),
            Effect::UpdateLayer(layer) => match video_mid(&layer.mid) {
                Some((n, mid)) => {
                    if let Some((_, jobs)) = self.videos.get(&n) {
                        let _ = jobs.send(VideoJob::Update(LayerUpdate {
                            mid: mid.to_owned(),
                            ..layer
                        }));
                    }
                }
                None => self.job(Job::Update(layer)),
            },
            Effect::PublishCamera { stream_id } => self.job(Job::PublishCamera { stream_id }),
            Effect::SetCameraLive(live) => {
                if let Some(session) = &self.session {
                    session.set_camera_live(live);
                }
            }
            Effect::StartShareCapture { token, request } => self.start_capture(token, request),
            Effect::ReleaseShareCapture(token) => {
                if lock(&self.captures).remove(&token).is_some() {
                    eprintln!("[share] released capture {token}");
                }
            }
            Effect::SetShareCaptureQuality { token, quality } => {
                if let Some(capture) = lock(&self.captures).get(&token) {
                    self.env.capturer.set_quality(capture, quality);
                }
            }
            Effect::PublishShare {
                stream_id,
                encoding,
                audio,
            } => self.job(Job::PublishShare {
                stream_id,
                encoding,
                audio,
            }),
            Effect::UnpublishShare => self.job(Job::UnpublishShare),
            Effect::SetShareLive(live) => {
                if let Some(session) = &self.session {
                    session.set_screen_live(live);
                }
            }
            Effect::SetShareEncoding(encoding) => {
                if let Some(session) = &self.session {
                    session.set_screen_encoding(&encoding);
                }
            }
            Effect::SetShareAudioEnabled(enabled) => {
                if let Some(session) = &self.session {
                    session.set_screen_audio_enabled(enabled);
                }
            }
            Effect::SetVideoSink { mid, key } => match video_mid(&mid) {
                Some((n, mid)) => {
                    if let Some((session, _)) = self.videos.get(&n) {
                        session.set_video_sink(mid, key.as_deref());
                    }
                }
                None => {
                    if let Some(session) = &self.session {
                        session.set_video_sink(&mid, key.as_deref());
                    }
                }
            },
            Effect::StopLocalMedia => {
                if let Some(session) = &self.session {
                    session.stop_local();
                }
            }
            Effect::SetMicEnabled(enabled) => {
                if let Some(session) = &self.session {
                    session.set_mic_enabled(enabled);
                }
            }
            Effect::SetPlayoutEnabled(enabled) => {
                if let Some(session) = &self.session {
                    session.set_playout_enabled(enabled);
                }
            }
            Effect::SetProcessing(processing) => {
                if let Some(session) = &self.session {
                    match session.set_processing(processing) {
                        Ok(()) => eprintln!("[voice] processing {processing:?}"),
                        Err(error) => eprintln!("[voice] processing refused: {error}"),
                    }
                }
            }
            Effect::SetTrackGain { mid, gain } => {
                if let Some(session) = &self.session {
                    session.set_track_gain(&mid, gain);
                }
            }
            Effect::SetTrackPan { mid, pan } => {
                if let Some(session) = &self.session {
                    session.set_track_pan(&mid, pan);
                }
            }
            Effect::LogStats => {
                if let Some(line) = self.env.engine.check_devices() {
                    eprintln!("{line}");
                }
                if let Some(session) = &self.session {
                    match session.stats().await {
                        Ok(stats) => {
                            if self.stats_logged.is_none_or(|at| at.elapsed() >= STATS_LOG_EVERY) {
                                self.stats_logged = Some(Instant::now());
                                eprintln!("{}", stats.line);
                            }
                            let at_ms = u64::try_from(self.started.elapsed().as_millis())
                                .unwrap_or(u64::MAX);
                            return Applied::Input(Input::Stats { stats, at_ms });
                        }
                        Err(error) => eprintln!("[voice] stats failed: {error}"),
                    }
                }
            }
            Effect::Log(line) => eprintln!("{line}"),
            Effect::ArmTimer { timer, after } => self.timers.push((Instant::now() + after, timer)),
            Effect::End(reason) => return Applied::End(reason),
        }
        Applied::Done
    }

    fn job(&self, job: Job) {
        if let Some(jobs) = &self.jobs {
            let _ = jobs.send(job);
        }
    }

    /// Starts a share capture off the runtime (window and device setup
    /// blocks); its result and a later end come back as share events.
    fn start_capture(&self, token: u64, request: crate::share::ShareRequest) {
        let capturer = self.env.capturer.clone();
        let events = self.share_events.clone();
        tokio::task::spawn_blocking(move || {
            let ended = {
                let events = events.clone();
                Box::new(move || {
                    let _ = events.send(ShareEvent::Ended(token));
                })
            };
            let result = capturer.start(&request, ended);
            match &result {
                Ok((_, captured)) => eprintln!(
                    "[share] capturing {} ({}){}",
                    request.source_id,
                    request.quality,
                    if captured.has_audio {
                        " with audio"
                    } else {
                        ""
                    }
                ),
                Err(error) => eprintln!("[share] capture of {} failed: {error}", request.source_id),
            }
            let _ = events.send(ShareEvent::Started { token, result });
        });
    }

    /// The join's admission: both tickets and the media credentials.
    async fn admit(&self, request: VoiceAdmissionRequest) -> Result<VoiceAdmission, String> {
        request_admission(&self.env.admission, "voice", &request)
            .await
            .map_err(|e| format!("call admission: {e}"))
    }

    /// Both sockets. They live in this call, so a new call never sees an
    /// old call's frames, and dropping them closes them.
    fn open_sockets(&mut self, room: SocketSpec, voice: SocketSpec, inbox: &mut Inbox<S::Capture>) {
        let env = self.env;
        let config = |label, spec: &SocketSpec, reconnect, ready_op| Config {
            label,
            url: yap_realtime::ws_url(&env.api_base, &spec.path),
            origin: env.origin.clone(),
            identify: spec.identify.clone(),
            reconnect,
            ready_op,
            policy: Policy::CALL,
            debug_drop_after: yap_realtime::debug_drop_after(),
        };
        let (room_tx, room_rx) = mpsc::unbounded_channel();
        let (voice_tx, voice_rx) = mpsc::unbounded_channel();
        self.room = Some(Connection::spawn(
            config("voice/room", &room, Reconnect::Resume, GatewayOp::Ready),
            call_tickets(
                env.admission.clone(),
                "voice/room",
                CallSocket::Room,
                room.first_ticket,
                room.reconnect,
            ),
            room_tx,
        ));
        self.voice = Some(Connection::spawn(
            config(
                "voice/voice",
                &voice,
                Reconnect::Reidentify,
                GatewayOp::VoiceReady,
            ),
            call_tickets(
                env.admission.clone(),
                "voice/voice",
                CallSocket::Voice,
                voice.first_ticket,
                voice.reconnect,
            ),
            voice_tx,
        ));
        inbox.room = Some(room_rx);
        inbox.voice = Some(voice_rx);
    }

    /// Opens media, creates our SFU session and pushes the mic (§3.3b).
    async fn open_media(
        &mut self,
        spec: MediaSpec,
        inbox: &mut Inbox<S::Capture>,
    ) -> Result<Published> {
        let (session, states) = if spec.standby {
            self.env.engine.open_standby(&spec.ice_servers)?
        } else {
            self.env.engine.open(&spec.ice_servers)?
        };
        self.ice_servers = spec.ice_servers.clone();
        inbox.media = Some(states);
        self.session = Some(session.clone());
        let sfu = Sfu::new(
            self.env.http.clone(),
            &self.env.api_base,
            &spec.slug,
            &spec.participant_id,
            &spec.voice_token,
        );
        self.sfu = Some(sfu.clone());
        let session_id = sfu.new_session().await?;
        let offer = if spec.standby {
            session.publish_standby(&spec.stream_id).await?
        } else {
            session.publish_mic(&spec.stream_id).await?
        };
        let sfu_track_name = uuid::Uuid::new_v4().to_string();
        let pushed = sfu
            .push(&session_id, &offer.sdp, &offer.mid, &sfu_track_name)
            .await?;
        session.apply_answer(&pushed.answer_sdp, "answer").await?;
        // A standby captures only once it is promoted (PromoteMedia).
        if !spec.standby {
            session.start_capture();
        }

        // Pulls run in their own task so a slow retry never blocks Leave
        // or frames.
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        self.done = Some(done_tx.clone());
        self.worker = Some(tokio::spawn(pull_worker(
            session,
            sfu,
            session_id.clone(),
            spec.stream_id.clone(),
            jobs_rx,
            done_tx,
        )));
        self.jobs = Some(jobs);
        inbox.pulls = Some(done_rx);
        Ok(Published {
            session_id,
            sfu_track_name: pushed.track_name,
        })
    }

    /// Closes the sockets, stops the worker, drops remote tracks and closes
    /// the peer connection.
    fn teardown(&mut self) {
        lock(&self.captures).clear();
        self.room = None;
        self.voice = None;
        self.jobs = None;
        self.close_videos();
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
        if let Some(session) = self.session.take() {
            session.close();
        }
    }

    /// Closes every video session at once (teardown or a rebuild).
    fn close_videos(&mut self) {
        for (_, (session, _)) in self.videos.drain() {
            session.close();
        }
    }

    /// Drops the failed peer connection with its worker, pulls and video
    /// sessions, and gives the devices back, before media opens again.
    fn drop_media(&mut self, inbox: &mut Inbox<S::Capture>) {
        self.jobs = None;
        self.done = None;
        inbox.pulls = None;
        inbox.media = None;
        self.close_videos();
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
        if let Some(session) = self.session.take() {
            session.close();
        }
        self.sfu = None;
        self.env.engine.release();
    }

    /// Closes pulls: main-session mids on the worker, a video session's on
    /// that session (which then drains and goes).
    fn close_mids(&mut self, mids: Vec<String>) {
        let mut main = Vec::new();
        let mut videos: HashMap<u32, Vec<String>> = HashMap::new();
        for mid in mids {
            match video_mid(&mid) {
                Some((n, bare)) => videos.entry(n).or_default().push(bare.to_owned()),
                None => main.push(mid),
            }
        }
        if !main.is_empty() {
            self.job(Job::Close(main));
        }
        for (n, mids) in videos {
            if let Some((_, jobs)) = self.videos.remove(&n) {
                let _ = jobs.send(VideoJob::Close(mids));
            }
        }
    }

    /// A pull on a receive-only peer connection of its own (a video alias
    /// pulled again after its pull closed): open it, make its SFU session,
    /// pull, and report the mid as `v<n>:<mid>`.
    fn pull_fresh(&mut self, remote: Remote) {
        let (Some(sfu), Some(done)) = (self.sfu.clone(), self.done.clone()) else {
            return;
        };
        let opened = self.env.engine.open_receiver(&self.ice_servers);
        let (session, states) = match opened {
            Ok(opened) => opened,
            Err(error) => {
                eprintln!("[voice] video session failed to open: {error}");
                let _ = done.send(Done::Pull(remote.sfu_track_name, None));
                return;
            }
        };
        self.next_video += 1;
        let n = self.next_video;
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        self.videos.insert(n, (session.clone(), jobs));
        eprintln!("[voice] video session {n} opened for {}", remote.track_name);
        tokio::spawn(video_session(
            n, session, states, sfu, remote, jobs_rx, done,
        ));
    }
}

/// The spatial state a room Ready, Resumed or op 15 carries.
fn spatial_of(event: &Event) -> Option<SpatialState> {
    match event {
        Event::Ready(d) | Event::Resumed(d) | Event::Frame(GatewayOp::VoiceStateUpdate, d) => {
            wire::spatial_of(d)
        }
        _ => None,
    }
}

/// A room socket event as a machine input (`None`: nothing for the call).
fn room_event(event: Event) -> Option<RoomEvent> {
    match event {
        Event::Ready(d) => Some(RoomEvent::Ready(RoomState::from_value(&d))),
        Event::Resumed(d) => {
            eprintln!("[voice/room] resumed");
            Some(RoomEvent::Resumed(RoomState::from_value(&d)))
        }
        Event::Frame(GatewayOp::VoiceStateUpdate, d) => {
            let p = d.get("participant").cloned().unwrap_or(Value::Null);
            Some(RoomEvent::VoiceState {
                leave: wire::str_of(&d, "action") == "leave",
                participant: Participant::from_value(&p),
            })
        }
        Event::Frame(GatewayOp::Resumed, d) => Some(RoomEvent::Credentials {
            voice_token: wire::str_of(&d, "voice_token").to_owned(),
        }),
        Event::Frame(GatewayOp::VoiceMove, d) => {
            serde_json::from_value(d).ok().map(RoomEvent::Moved)
        }
        Event::Frame(GatewayOp::Error, d) => {
            eprintln!("[voice] room error {d}");
            None
        }
        Event::Disconnected { .. } => Some(RoomEvent::Disconnected),
        Event::Stopped(reason) => Some(RoomEvent::Stopped(reason)),
        Event::Connected
        | Event::Frame(..)
        | Event::IdentifyRejected
        | Event::Reconnecting { .. } => None,
    }
}

/// A voice socket event as a machine input (`None`: nothing for the call).
fn voice_event(event: Event) -> Option<VoiceEvent> {
    let pid = |d: &Value| wire::str_of(d, "participant_id").to_owned();
    match event {
        Event::Ready(d) => Some(VoiceEvent::Ready {
            tracks: Track::list(&d),
            speaking: wire::speaking_map(&d),
        }),
        Event::Frame(GatewayOp::Video, d) => Some(VoiceEvent::Video {
            participant_id: pid(&d),
            tracks: Track::list(&d),
        }),
        Event::Frame(GatewayOp::Speaking, d) => Some(VoiceEvent::Speaking {
            participant_id: pid(&d),
            speaking: d.get("speaking").and_then(Value::as_i64).unwrap_or(0) != 0,
        }),
        Event::Frame(GatewayOp::StopTracks, d) => Some(VoiceEvent::StopTracks {
            participant_id: pid(&d),
            names: wire::stopped_names(&d),
        }),
        Event::Frame(GatewayOp::ClientDisconnect, d) => Some(VoiceEvent::ClientDisconnect {
            participant_id: pid(&d),
        }),
        Event::Frame(GatewayOp::VoiceAppEvent, d) => Some(VoiceEvent::App(d)),
        Event::Frame(GatewayOp::RepublishRequest, d) => {
            serde_json::from_value(d).ok().map(VoiceEvent::Republish)
        }
        Event::Frame(GatewayOp::Error, d) => {
            eprintln!("[voice] voice error {d}");
            let code = d.get("code").and_then(Value::as_i64).unwrap_or(0);
            matches!(
                GatewayCloseCode::try_from(code),
                Ok(GatewayCloseCode::AdmissionRejected | GatewayCloseCode::SessionReplaced)
            )
            .then(|| VoiceEvent::Rejected {
                message: wire::str_of(&d, "message").to_owned(),
            })
        }
        Event::IdentifyRejected => Some(VoiceEvent::IdentifyRejected),
        Event::Disconnected { .. } => Some(VoiceEvent::Disconnected),
        Event::Stopped(reason) => Some(VoiceEvent::Stopped(reason)),
        Event::Connected | Event::Resumed(_) | Event::Frame(..) | Event::Reconnecting { .. } => {
            None
        }
    }
}

/// One `POST /api/voice/admission`, logged by what it asks for (never what
/// it returns: tickets and tokens stay out of the logs).
async fn request_admission<T: AdmissionApi>(
    api: &T,
    label: &str,
    request: &VoiceAdmissionRequest,
) -> Result<VoiceAdmission, ApiFailure> {
    let sockets: Vec<&str> = request
        .sockets
        .iter()
        .map(|socket| match socket {
            CallSocket::Room => "room",
            CallSocket::Voice => "voice",
        })
        .collect();
    eprintln!(
        "[{label}] admission: sockets={}{}{}",
        sockets.join("+"),
        if request.media { " media" } else { "" },
        if request.participant_id.is_some() {
            " bound"
        } else {
            ""
        },
    );
    let body = serde_json::to_value(request).map_err(|e| ApiFailure {
        unauthorized: false,
        message: format!("admission body: {e}"),
    })?;
    let reply = api.admission(body).await?;
    // The serde error would quote the offending value; keep it out.
    serde_json::from_value(reply).map_err(|_| ApiFailure {
        unauthorized: false,
        message: "admission response unreadable".into(),
    })
}

/// Tickets for one call socket: the join admission's first, then one
/// admission per reconnect asking for this socket's ticket alone
/// (`reconnect`; tickets are single use).
fn call_tickets<T: AdmissionApi>(
    api: T,
    label: &'static str,
    socket: CallSocket,
    first: String,
    reconnect: VoiceAdmissionRequest,
) -> impl FnMut() -> TicketFuture + Send + 'static {
    let mut first = Some(first).filter(|t| !t.is_empty());
    move || {
        let first = first.take();
        let api = api.clone();
        let reconnect = reconnect.clone();
        Box::pin(async move {
            if let Some(ticket) = first {
                return Ok(ticket);
            }
            let admission = request_admission(&api, label, &reconnect)
                .await
                .map_err(|e| {
                    if e.unauthorized {
                        TicketError::Unauthorized
                    } else {
                        TicketError::Failed(e.message)
                    }
                })?;
            admission
                .ticket(socket)
                .map(str::to_owned)
                .ok_or_else(|| TicketError::Failed("admission without the socket's ticket".into()))
        })
    }
}

/// Runs pulls, closes and the camera push one at a time (each renegotiates
/// the one peer connection).
async fn pull_worker<S: MediaSession, G: Signaling>(
    session: S,
    sfu: G,
    session_id: String,
    mic_stream: String,
    mut jobs: UnboundedReceiver<Job>,
    done: UnboundedSender<Done>,
) {
    // Our camera's SFU track, kept while the camera is off.
    let mut camera: Option<CameraPublished> = None;
    // Our share's video SFU track (kept between shares) and audio track.
    let mut share = ShareTracks::default();
    while let Some(job) = jobs.recv().await {
        match job {
            Job::Pull(remote) => {
                let mid = match pull(&session, &sfu, &session_id, &remote).await {
                    Ok(mid) => {
                        eprintln!(
                            "[voice] pulled {} from {}{}",
                            remote.track_name,
                            remote.participant_id,
                            remote
                                .preferred_rid
                                .as_deref()
                                .map(|rid| format!(" (rid {rid})"))
                                .unwrap_or_default()
                        );
                        Some(mid)
                    }
                    Err(error) => {
                        eprintln!("[voice] pull {} failed: {error:#}", remote.track_name);
                        if is_not_found(&error) {
                            let _ = done.send(Done::NotFound(remote.sfu_track_name.clone()));
                        }
                        None
                    }
                };
                let _ = done.send(Done::Pull(remote.sfu_track_name, mid));
            }
            Job::Republish(kind) => {
                let result = republish(&session, &sfu, &session_id, &mic_stream, kind, &mut camera)
                    .await
                    .map(|name| (kind, name))
                    .map_err(|error| format!("{error:#}"));
                match &result {
                    Ok((kind, name)) => eprintln!("[voice] republished {kind:?} as {name}"),
                    Err(error) => eprintln!("[voice] republish failed: {error}"),
                }
                let _ = done.send(Done::Republished(result));
            }
            Job::PublishCamera { stream_id } => {
                let result = publish_camera(&session, &sfu, &session_id, &stream_id, &mut camera)
                    .await
                    .map_err(|error| format!("{error:#}"));
                match &result {
                    Ok(p) => eprintln!(
                        "[camera] published {} (layers {})",
                        p.sfu_track_name,
                        if p.simulcast_rids.is_empty() {
                            "single".to_owned()
                        } else {
                            p.simulcast_rids.join(",")
                        }
                    ),
                    Err(error) => eprintln!("[camera] publish failed: {error}"),
                }
                let _ = done.send(Done::Camera(result));
            }
            Job::PublishShare {
                stream_id,
                encoding,
                audio,
            } => {
                let result = publish_share(
                    &session,
                    &sfu,
                    &session_id,
                    &stream_id,
                    &encoding,
                    audio,
                    &mut share,
                )
                .await
                .map_err(|error| format!("{error:#}"));
                match &result {
                    Ok(p) => eprintln!(
                        "[share] published video {}{}",
                        p.video_sfu_track,
                        p.audio_sfu_track
                            .as_deref()
                            .map(|a| format!(", audio {a}"))
                            .unwrap_or_default()
                    ),
                    Err(error) => eprintln!("[share] publish failed: {error}"),
                }
                let _ = done.send(Done::Share(result));
            }
            Job::UnpublishShare => {
                session.set_screen_live(false);
                if std::mem::take(&mut share.audio_live) {
                    session.park_screen_audio();
                }
                eprintln!("[share] withdrawn; video and audio parked");
            }
            Job::Close(mids) => match close(&session, &sfu, &session_id, &mids).await {
                Ok(()) => eprintln!("[voice] closed {} transceiver(s)", mids.len()),
                Err(error) => eprintln!("[voice] close failed: {error:#}"),
            },
            Job::Update(layer) => match sfu.update_layer(&session_id, &layer).await {
                Ok(()) => eprintln!(
                    "[voice] layer {} on mid {}{}",
                    layer.rid,
                    layer.mid,
                    if layer.switch_now { "" } else { " (settled)" }
                ),
                Err(error) => eprintln!("[voice] layer update failed: {error:#}"),
            },
        }
    }
}

/// Whether a pull failure was the SFU not finding the track.
fn is_not_found(error: &anyhow::Error) -> bool {
    format!("{error:#}").contains("not_found")
}

/// Pushes the mic or the camera again under a new SFU track name
/// (PartyTracks `republishTrack`).
async fn republish<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    mic_stream: &str,
    kind: Republish,
    camera: &mut Option<CameraPublished>,
) -> Result<String> {
    match kind {
        Republish::Mic => {
            let offer = session.republish_mic(mic_stream).await?;
            push_local(session, sfu, session_id, offer, "republish answer").await
        }
        Republish::Camera => {
            // Whatever the keepalive says: the SFU lost this one.
            *camera = None;
            let published = async {
                let offer = session.publish_camera(mic_stream).await?;
                push_local(session, sfu, session_id, offer, "camera answer").await
            }
            .await;
            match published {
                Ok(name) => {
                    *camera = Some(CameraPublished {
                        sfu_track_name: name.clone(),
                        simulcast_rids: session.camera_rids(),
                    });
                    session.set_camera_live(true);
                    Ok(name)
                }
                Err(error) => {
                    session.abandon_camera();
                    Err(error)
                }
            }
        }
    }
}

/// One video session: a receive-only peer connection with its own SFU
/// session for one fresh pull. It reports the pull (mid `v<n>:<mid>`),
/// then serves layer switches until its pull closes, drains
/// [`VIDEO_SESSION_DRAIN`] and closes.
async fn video_session<S: MediaSession, G: Signaling>(
    n: u32,
    session: S,
    mut states: UnboundedReceiver<MediaState>,
    sfu: G,
    remote: Remote,
    mut jobs: UnboundedReceiver<VideoJob>,
    done: UnboundedSender<Done>,
) {
    let pulled = async {
        let capability = sfu.new_session().await?;
        let mid = pull(&session, &sfu, &capability, &remote).await?;
        anyhow::Ok((capability, mid))
    }
    .await;
    let capability = match pulled {
        Ok((capability, mid)) => {
            eprintln!(
                "[voice] pulled {} from {} on video session {n}",
                remote.track_name, remote.participant_id
            );
            let _ = done.send(Done::Pull(
                remote.sfu_track_name.clone(),
                Some(format!("v{n}:{mid}")),
            ));
            capability
        }
        Err(error) => {
            eprintln!(
                "[voice] pull {} failed on video session {n}: {error:#}",
                remote.track_name
            );
            if is_not_found(&error) {
                let _ = done.send(Done::NotFound(remote.sfu_track_name.clone()));
            }
            let _ = done.send(Done::Pull(remote.sfu_track_name, None));
            session.close();
            return;
        }
    };
    loop {
        tokio::select! {
            job = jobs.recv() => match job {
                Some(VideoJob::Update(layer)) => {
                    if let Err(error) = sfu.update_layer(&capability, &layer).await {
                        eprintln!("[voice] layer update failed on video session {n}: {error:#}");
                    }
                }
                Some(VideoJob::Close(mids)) => {
                    if let Err(error) = close(&session, &sfu, &capability, &mids).await {
                        eprintln!("[voice] close failed on video session {n}: {error:#}");
                    }
                    tokio::time::sleep(VIDEO_SESSION_DRAIN).await;
                    break;
                }
                None => break,
            },
            Some(state) = states.recv() => {
                if matches!(state, MediaState::Failed | MediaState::Disconnected) {
                    eprintln!("[voice] video session {n}: {state:?}");
                }
            }
        }
    }
    session.close();
    eprintln!("[voice] video session {n} closed");
}

/// Our camera on the SFU (PartyTracks `publishStream` for `cam` video):
/// the SFU track a camera that went off kept, unless its keepalive stalled
/// long enough that the SFU may have dropped it; otherwise a fresh push
/// (we offer, a new UUID track name, the SFU answers).
async fn publish_camera<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    stream_id: &str,
    kept: &mut Option<CameraPublished>,
) -> Result<CameraPublished> {
    if let Some(kept) = kept.as_ref().filter(|_| !session.camera_stale()) {
        return Ok(kept.clone());
    }
    // The old track goes either way: a failed push must not leave it to be
    // "resumed" next time, nor a half-made send sending keepalives.
    *kept = None;
    let pushed = async {
        let offer = session.publish_camera(stream_id).await?;
        push_local(session, sfu, session_id, offer, "camera answer").await
    }
    .await;
    let track_name = match pushed {
        Ok(name) => name,
        Err(error) => {
            session.abandon_camera();
            return Err(error);
        }
    };
    let published = CameraPublished {
        sfu_track_name: track_name,
        simulcast_rids: session.camera_rids(),
    };
    *kept = Some(published.clone());
    Ok(published)
}

/// Our share's SFU tracks in the worker.
#[derive(Default)]
struct ShareTracks {
    /// The video track, kept (parked) between shares.
    video: Option<String>,
    /// The audio track, kept (parked) between shares, and whether the live
    /// share carries it.
    audio: Option<String>,
    audio_live: bool,
}

/// Pushes one local track the session just offered (we offer, a new UUID
/// track name, the SFU answers).
async fn push_local<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    offer: crate::engine::LocalOffer,
    what: &'static str,
) -> Result<String> {
    let track_name = uuid::Uuid::new_v4().to_string();
    let answer = sfu
        .push(session_id, &offer.sdp, &offer.mid, &track_name)
        .await?;
    session.apply_answer(&answer, what).await?;
    Ok(track_name)
}

/// Our share on the SFU (PartyTracks `publishStream` under `screen`): the
/// video resumes the SFU track a previous share kept unless its keepalive
/// stalled (then a fresh push), with this share's caps; audio is pushed when
/// the capture carries it, and dropped when a new capture does not.
async fn publish_share<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    stream_id: &str,
    encoding: &ShareEncoding,
    audio: bool,
    tracks: &mut ShareTracks,
) -> Result<SharePublished> {
    let video = match tracks.video.clone().filter(|_| !session.screen_stale()) {
        Some(kept) => {
            session.set_screen_encoding(encoding);
            kept
        }
        None => {
            // As the camera: a failed push leaves nothing to resume.
            tracks.video = None;
            let pushed = async {
                let offer = session.publish_screen(stream_id, encoding).await?;
                push_local(session, sfu, session_id, offer, "share answer").await
            }
            .await;
            let name = match pushed {
                Ok(name) => name,
                Err(error) => {
                    session.abandon_screen();
                    return Err(error);
                }
            };
            tracks.video = Some(name.clone());
            name
        }
    };
    match (audio, tracks.audio_live) {
        // The parked track comes back on the same transceiver unless its
        // keepalive stalled; otherwise a fresh push.
        (true, false) if tracks.audio.is_some() && session.resume_screen_audio() => {
            tracks.audio_live = true;
        }
        (true, false) => {
            tracks.audio = None;
            let pushed = async {
                let offer = session.publish_screen_audio(stream_id).await?;
                push_local(session, sfu, session_id, offer, "share audio answer").await
            }
            .await;
            match pushed {
                Ok(name) => {
                    tracks.audio = Some(name);
                    tracks.audio_live = true;
                }
                Err(error) => {
                    session.abandon_screen_audio();
                    return Err(error);
                }
            }
        }
        (false, true) => {
            session.park_screen_audio();
            tracks.audio_live = false;
        }
        _ => {}
    }
    Ok(SharePublished {
        video_sfu_track: video,
        audio_sfu_track: tracks.audio.clone().filter(|_| tracks.audio_live),
    })
}

/// Stops receive transceivers and tells the SFU (§3.3d: we offer).
async fn close<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    mids: &[String],
) -> Result<()> {
    let offer = session.close_mids(mids).await?;
    let answer = sfu.close(session_id, mids, &offer).await?;
    session.apply_answer(&answer, "close answer").await?;
    Ok(())
}

/// SFU offers, we answer (§3.3c). Retries "track not ready yet" briefly.
async fn pull<S: MediaSession, G: Signaling>(
    session: &S,
    sfu: &G,
    session_id: &str,
    remote: &Remote,
) -> Result<String> {
    let mut delay = Duration::from_millis(100);
    let pulled = loop {
        match sfu
            .pull(
                session_id,
                &remote.sfu_track_name,
                &remote.session_id,
                remote.preferred_rid.as_deref(),
            )
            .await
        {
            Ok(pulled) => break pulled,
            Err(error) if delay < Duration::from_secs(5) => {
                eprintln!("[voice] pull retry: {error:#}");
                tokio::time::sleep(delay).await;
                delay *= 2;
            }
            Err(error) => return Err(error),
        }
    };
    let Some(offer) = pulled.offer_sdp else {
        return Ok(pulled.mid);
    };
    let answer = session.answer_offer(&offer).await?;
    sfu.renegotiate(session_id, &answer).await?;
    Ok(pulled.mid)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex, PoisonError};

    use anyhow::anyhow;
    use serde_json::json;

    use super::*;
    use crate::engine::{FakeEngine, FakeOp};
    use crate::sfu::Pulled;

    #[test]
    fn the_ice_stage_should_parse() {
        assert!(is_ice_stage("ice"));
        assert!(is_ice_stage(" ICE "));
        assert!(is_ice_stage("media"));
        assert!(!is_ice_stage("admission"));
        assert!(!is_ice_stage(""));
    }

    /// Records SFU calls; the first pull of `flaky` fails once, and the
    /// next `failing_pushes` pushes fail.
    #[derive(Clone, Default)]
    struct FakeSfu {
        calls: Arc<Mutex<Vec<String>>>,
        flaky: Option<String>,
        failing_pushes: Arc<std::sync::atomic::AtomicUsize>,
    }

    impl FakeSfu {
        fn log(&self, call: String) {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(call);
        }

        fn calls(&self) -> Vec<String> {
            self.calls
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl Signaling for FakeSfu {
        async fn new_session(&self) -> Result<String> {
            self.log("new session".into());
            Ok("video-cap".into())
        }

        async fn push(&self, _: &str, offer_sdp: &str, mid: &str, _: &str) -> Result<String> {
            self.log(format!("push {mid} {offer_sdp}"));
            let failing = &self.failing_pushes;
            let fail = failing
                .fetch_update(
                    std::sync::atomic::Ordering::Relaxed,
                    std::sync::atomic::Ordering::Relaxed,
                    |n| n.checked_sub(1),
                )
                .is_ok();
            if fail {
                return Err(anyhow!("push refused"));
            }
            Ok("sfu-camera-answer".into())
        }

        async fn update_layer(&self, _: &str, layer: &LayerUpdate) -> Result<()> {
            self.log(format!("update {} {}", layer.mid, layer.rid));
            Ok(())
        }

        async fn pull(
            &self,
            _: &str,
            track_name: &str,
            _: &str,
            rid: Option<&str>,
        ) -> Result<Pulled> {
            if let Some(rid) = rid {
                self.log(format!("rid {rid}"));
            }
            let first = !self
                .calls()
                .iter()
                .any(|c| c == &format!("pull {track_name}"));
            self.log(format!("pull {track_name}"));
            if first && self.flaky.as_deref() == Some(track_name) {
                return Err(anyhow!("track not ready"));
            }
            Ok(Pulled {
                mid: format!("mid-{track_name}"),
                offer_sdp: Some("sfu-offer".into()),
            })
        }

        async fn renegotiate(&self, _: &str, answer_sdp: &str) -> Result<()> {
            self.log(format!("renegotiate {answer_sdp}"));
            Ok(())
        }

        async fn close(&self, _: &str, mids: &[String], offer_sdp: &str) -> Result<String> {
            self.log(format!("close {} {offer_sdp}", mids.join(",")));
            Ok("sfu-close-answer".into())
        }
    }

    fn remote(name: &str) -> Remote {
        Remote {
            participant_id: "peer".into(),
            track_name: format!("alias-{name}"),
            sfu_track_name: name.into(),
            session_id: "their-session".into(),
            kind: crate::tracks::Kind::Audio,
            preferred_rid: None,
            fresh: false,
        }
    }

    /// An admission reply that never comes.
    #[derive(Clone)]
    struct StuckAdmission;

    impl AdmissionApi for StuckAdmission {
        async fn admission(&self, _: Value) -> Result<Value, ApiFailure> {
            std::future::pending().await
        }
    }

    /// Records each admission body and answers with the next reply (the
    /// last one again once they run out).
    #[derive(Clone, Default)]
    struct RecordedAdmission {
        bodies: Arc<Mutex<Vec<Value>>>,
        replies: Arc<Mutex<VecDeque<Result<Value, ApiFailure>>>>,
    }

    impl RecordedAdmission {
        fn answering(replies: Vec<Result<Value, ApiFailure>>) -> Self {
            Self {
                bodies: Arc::default(),
                replies: Arc::new(Mutex::new(replies.into())),
            }
        }

        fn bodies(&self) -> Vec<Value> {
            self.bodies
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone()
        }
    }

    impl AdmissionApi for RecordedAdmission {
        async fn admission(&self, body: Value) -> Result<Value, ApiFailure> {
            self.bodies
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(body);
            let mut replies = self.replies.lock().unwrap_or_else(PoisonError::into_inner);
            match replies.len() {
                0 => Err(ApiFailure {
                    unauthorized: false,
                    message: "no reply".into(),
                }),
                1 => replies[0].clone(),
                _ => replies.pop_front().expect("non-empty"),
            }
        }
    }

    fn target() -> CallTarget {
        CallTarget {
            server_id: "s1".into(),
            channel_id: "c1".into(),
            me_name: "Jon".into(),
            me_username: "jon".into(),
            ..CallTarget::default()
        }
    }

    fn env<T: AdmissionApi>(
        engine: &FakeEngine,
        admission: T,
    ) -> CallEnv<FakeEngine, T, crate::share::NoCapture> {
        CallEnv {
            engine: engine.clone(),
            admission,
            capturer: crate::share::NoCapture,
            http: reqwest::Client::new(),
            api_base: "https://api.invalid".into(),
            origin: "https://api.invalid".into(),
        }
    }

    #[tokio::test]
    async fn the_join_should_make_one_admission_request_for_both_sockets_and_media() {
        let engine = FakeEngine::new();
        let api = RecordedAdmission::answering(vec![Err(ApiFailure {
            unauthorized: false,
            message: "refused".into(),
        })]);
        let (_commands, rx) = mpsc::unbounded_channel();
        let ended = tokio::time::timeout(
            Duration::from_secs(5),
            run_call(env(&engine, api.clone()), target(), rx, |_| {}),
        )
        .await
        .expect("ends");
        assert_eq!(ended, Some("call admission: refused".into()));
        assert_eq!(
            api.bodies(),
            vec![json!({
                "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
                "sockets": ["room", "voice"], "media": true,
            })]
        );
        assert_eq!(engine.ops(), vec![FakeOp::Release]);
    }

    #[tokio::test]
    async fn an_admission_without_media_should_end_before_any_socket_opens() {
        let engine = FakeEngine::new();
        let api = RecordedAdmission::answering(vec![Ok(json!({
            "room_ticket": "room-t", "voice_ticket": "voice-t", "participant_id": "me-1",
        }))]);
        let (_commands, rx) = mpsc::unbounded_channel();
        let ended = tokio::time::timeout(
            Duration::from_secs(5),
            run_call(env(&engine, api.clone()), target(), rx, |_| {}),
        )
        .await
        .expect("ends");
        assert_eq!(ended, Some("The call admission was incomplete.".into()));
        assert_eq!(api.bodies().len(), 1);
        assert_eq!(engine.ops(), vec![FakeOp::Release]);
    }

    async fn ticket(
        source: &mut (impl FnMut() -> TicketFuture + Send),
    ) -> Result<String, TicketError> {
        source().await
    }

    #[tokio::test]
    async fn the_room_socket_should_take_the_join_ticket_then_one_room_admission_per_reconnect() {
        let api = RecordedAdmission::answering(vec![Ok(json!({ "room_ticket": "room-2" }))]);
        let reconnect = VoiceAdmissionRequest::room_reconnect("s1", "c1", "voice-s1-c1");
        let mut source = call_tickets(
            api.clone(),
            "voice/room",
            CallSocket::Room,
            "room-1".into(),
            reconnect,
        );
        assert_eq!(ticket(&mut source).await, Ok("room-1".into()));
        assert!(
            api.bodies().is_empty(),
            "the join's ticket needs no request"
        );
        assert_eq!(ticket(&mut source).await, Ok("room-2".into()));
        assert_eq!(ticket(&mut source).await, Ok("room-2".into()));
        let room_only = json!({
            "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
            "sockets": ["room"], "media": false,
        });
        assert_eq!(api.bodies(), vec![room_only.clone(), room_only]);
    }

    #[tokio::test]
    async fn the_voice_socket_should_reconnect_on_a_ticket_bound_to_its_participant() {
        // A 1013 (or any drop) on the voice socket comes back here.
        let api = RecordedAdmission::answering(vec![Ok(json!({
            "voice_ticket": "voice-2", "participant_id": "me-1",
        }))]);
        let reconnect = VoiceAdmissionRequest::voice_reconnect("s1", "c1", "voice-s1-c1", "me-1");
        let mut source = call_tickets(
            api.clone(),
            "voice/voice",
            CallSocket::Voice,
            "voice-1".into(),
            reconnect,
        );
        assert_eq!(ticket(&mut source).await, Ok("voice-1".into()));
        assert_eq!(ticket(&mut source).await, Ok("voice-2".into()));
        assert_eq!(
            api.bodies(),
            vec![json!({
                "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
                "sockets": ["voice"], "media": false, "participantId": "me-1",
            })]
        );
    }

    #[tokio::test]
    async fn a_reconnect_admission_without_the_ticket_should_fail_the_attempt() {
        let api = RecordedAdmission::answering(vec![Ok(json!({ "voice_ticket": "voice-2" }))]);
        let reconnect = VoiceAdmissionRequest::room_reconnect("s1", "c1", "voice-s1-c1");
        let mut source = call_tickets(
            api,
            "voice/room",
            CallSocket::Room,
            String::new(),
            reconnect,
        );
        assert!(matches!(
            ticket(&mut source).await,
            Err(TicketError::Failed(_))
        ));
    }

    #[tokio::test]
    async fn a_refused_reconnect_admission_should_count_as_unauthorized() {
        let api = RecordedAdmission::answering(vec![Err(ApiFailure {
            unauthorized: true,
            message: "401".into(),
        })]);
        let reconnect = VoiceAdmissionRequest::room_reconnect("s1", "c1", "voice-s1-c1");
        let mut source = call_tickets(
            api,
            "voice/room",
            CallSocket::Room,
            String::new(),
            reconnect,
        );
        assert_eq!(ticket(&mut source).await, Err(TicketError::Unauthorized));
    }

    #[tokio::test]
    async fn leave_while_getting_tickets_should_end_before_any_socket_opens() {
        let engine = FakeEngine::new();
        let env = CallEnv {
            engine: engine.clone(),
            admission: StuckAdmission,
            capturer: crate::share::NoCapture,
            http: reqwest::Client::new(),
            api_base: "https://api.invalid".into(),
            origin: "https://api.invalid".into(),
        };
        let target = CallTarget {
            server_id: "s1".into(),
            channel_id: "c1".into(),
            me_name: "Jon".into(),
            me_username: "jon".into(),
            ..CallTarget::default()
        };
        let (commands, rx) = mpsc::unbounded_channel();
        commands.send(UserCommand::Leave).expect("open");
        let ended = tokio::time::timeout(Duration::from_secs(5), run_call(env, target, rx, |_| {}))
            .await
            .expect("ends without waiting for the tickets");
        assert_eq!(ended, None);
        // No media was opened, only released.
        assert_eq!(engine.ops(), vec![FakeOp::Release]);
    }

    #[tokio::test]
    async fn pull_worker_should_serialize_pulls_and_closes_in_order() {
        let engine = FakeEngine::new();
        let (session, _states) = engine.open(&[]).expect("fake open");
        let sfu = FakeSfu {
            flaky: Some("a".into()),
            ..FakeSfu::default()
        };
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let worker = tokio::spawn(pull_worker(
            session,
            sfu.clone(),
            "cap".into(),
            "cam-me".into(),
            jobs_rx,
            done_tx,
        ));
        jobs.send(Job::Pull(remote("a"))).expect("worker alive");
        jobs.send(Job::Close(vec!["mid-x".into()]))
            .expect("worker alive");
        jobs.send(Job::Pull(remote("b"))).expect("worker alive");
        drop(jobs);
        worker.await.expect("worker ran");

        let mut finished = Vec::new();
        while let Some(item) = done.recv().await {
            finished.push(item);
        }
        assert_eq!(
            finished,
            vec![
                Done::Pull("a".to_owned(), Some("mid-a".to_owned())),
                Done::Pull("b".to_owned(), Some("mid-b".to_owned())),
            ]
        );
        assert_eq!(
            sfu.calls(),
            vec![
                "pull a",
                "pull a",
                "renegotiate fake-answer",
                "close mid-x fake-close-offer",
                "pull b",
                "renegotiate fake-answer",
            ]
        );
        assert!(engine.ops().contains(&FakeOp::ApplyAnswer {
            sdp: "sfu-close-answer".into(),
            what: "close answer",
        }));
    }
    #[tokio::test]
    async fn camera_should_push_once_then_resume_its_kept_track() {
        let engine = FakeEngine::new();
        let (session, _states) = engine.open(&[]).expect("fake open");
        let sfu = FakeSfu::default();
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        let (done_tx, mut done) = mpsc::unbounded_channel();
        let worker = tokio::spawn(pull_worker(
            session,
            sfu.clone(),
            "cap".into(),
            "cam-me".into(),
            jobs_rx,
            done_tx,
        ));
        let publish = || Job::PublishCamera {
            stream_id: "cam-me".into(),
        };
        jobs.send(publish()).expect("worker alive");
        jobs.send(publish()).expect("worker alive");
        // A stalled keepalive: the SFU may have dropped it, so push anew.
        let first = done.recv().await.expect("first");
        let second = done.recv().await.expect("second");
        engine.set_camera_stale(true);
        jobs.send(publish()).expect("worker alive");
        let third = done.recv().await.expect("third");
        drop(jobs);
        worker.await.expect("worker ran");

        let name = |d: &Done| match d {
            Done::Camera(Ok(p)) => p.sfu_track_name.clone(),
            other => panic!("not a camera result: {other:?}"),
        };
        assert_eq!(name(&first), name(&second));
        assert_ne!(name(&first), name(&third));
        assert_eq!(
            sfu.calls(),
            vec!["push 1 fake-camera-offer", "push 1 fake-camera-offer"]
        );
    }

    /// The worker, and a way to run one job and wait for its result.
    fn worker_on(
        engine: &FakeEngine,
        sfu: &FakeSfu,
    ) -> (UnboundedSender<Job>, UnboundedReceiver<Done>) {
        let (session, _states) = engine.open(&[]).expect("fake open");
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        let (done_tx, done) = mpsc::unbounded_channel();
        tokio::spawn(pull_worker(
            session,
            sfu.clone(),
            "cap".into(),
            "cam-me".into(),
            jobs_rx,
            done_tx,
        ));
        (jobs, done)
    }

    #[tokio::test]
    async fn a_failed_camera_push_should_push_afresh_next_time() {
        let engine = FakeEngine::new();
        let sfu = FakeSfu::default();
        let (jobs, mut done) = worker_on(&engine, &sfu);
        let publish = || Job::PublishCamera {
            stream_id: "cam-me".into(),
        };
        jobs.send(publish()).expect("worker alive");
        let first = done.recv().await.expect("first");
        sfu.failing_pushes
            .store(1, std::sync::atomic::Ordering::Relaxed);
        // Stale: a fresh push, which fails; the old track must not come back.
        engine.set_camera_stale(true);
        jobs.send(publish()).expect("worker alive");
        let failed = done.recv().await.expect("failed");
        engine.set_camera_stale(false);
        jobs.send(publish()).expect("worker alive");
        let third = done.recv().await.expect("third");

        assert!(matches!(failed, Done::Camera(Err(_))));
        assert!(engine.ops().contains(&FakeOp::AbandonCamera));
        let (Done::Camera(Ok(a)), Done::Camera(Ok(c))) = (first, third) else {
            panic!("camera results");
        };
        assert_ne!(a.sfu_track_name, c.sfu_track_name);
        assert_eq!(sfu.calls().len(), 3, "the third publish pushed again");
    }

    #[tokio::test]
    async fn a_failed_share_push_should_push_afresh_next_time() {
        let engine = FakeEngine::new();
        let sfu = FakeSfu::default();
        sfu.failing_pushes
            .store(1, std::sync::atomic::Ordering::Relaxed);
        let (jobs, mut done) = worker_on(&engine, &sfu);
        let publish = || Job::PublishShare {
            stream_id: "screen-me".into(),
            encoding: crate::share::ShareEncoding {
                max_bitrate: 8_000_000,
                max_framerate: 30.0,
                detail: true,
            },
            audio: false,
        };
        jobs.send(publish()).expect("worker alive");
        let failed = done.recv().await.expect("failed");
        jobs.send(publish()).expect("worker alive");
        let second = done.recv().await.expect("second");

        assert!(matches!(failed, Done::Share(Err(_))));
        assert!(engine.ops().contains(&FakeOp::AbandonScreen));
        assert!(matches!(second, Done::Share(Ok(_))));
        assert_eq!(sfu.calls().len(), 2, "pushed again, not resumed");
    }

    #[tokio::test]
    async fn a_failed_share_encoder_should_push_a_fresh_video_track() {
        let engine = FakeEngine::new();
        let sfu = FakeSfu::default();
        let (jobs, mut done) = worker_on(&engine, &sfu);
        let publish = || Job::PublishShare {
            stream_id: "screen-me".into(),
            encoding: crate::share::ShareEncoding {
                max_bitrate: 8_000_000,
                max_framerate: 30.0,
                detail: true,
            },
            audio: false,
        };
        let video_of = |d: Done| match d {
            Done::Share(Ok(p)) => p.video_sfu_track,
            other => panic!("not a share result: {other:?}"),
        };
        jobs.send(publish()).expect("worker alive");
        let first = video_of(done.recv().await.expect("first"));
        engine.fail_screen_encoder();
        jobs.send(publish()).expect("worker alive");
        let second = video_of(done.recv().await.expect("second"));

        assert_ne!(first, second, "a new SFU track, not the frozen one");
    }

    #[tokio::test]
    async fn share_audio_should_park_and_resume_its_one_transceiver() {
        let engine = FakeEngine::new();
        let sfu = FakeSfu::default();
        let (jobs, mut done) = worker_on(&engine, &sfu);
        let publish = || Job::PublishShare {
            stream_id: "screen-me".into(),
            encoding: crate::share::ShareEncoding {
                max_bitrate: 5_000_000,
                max_framerate: 30.0,
                detail: false,
            },
            audio: true,
        };
        let audio_of = |d: Done| match d {
            Done::Share(Ok(p)) => p.audio_sfu_track,
            other => panic!("not a share result: {other:?}"),
        };
        jobs.send(publish()).expect("worker alive");
        let first = audio_of(done.recv().await.expect("first"));
        jobs.send(Job::UnpublishShare).expect("worker alive");
        jobs.send(publish()).expect("worker alive");
        let second = audio_of(done.recv().await.expect("second"));

        assert!(first.is_some());
        assert_eq!(first, second, "the same SFU audio track");
        assert_eq!(sfu.calls().len(), 2, "video and audio pushed once each");
        let ops = engine.ops();
        assert!(ops.contains(&FakeOp::ParkScreenAudio));
        assert!(ops.contains(&FakeOp::ResumeScreenAudio));
        assert!(!ops.contains(&FakeOp::AbandonScreenAudio));
    }
}
