//! [`run_call`]: the async shell around a [`CallMachine`]. It turns socket,
//! media, pull and timer events into machine inputs and runs the effects
//! that come back: the two `yap_realtime` connections, the SFU, the media
//! engine and one worker that serializes pulls and closes.

use std::collections::VecDeque;
use std::future::Future;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;
use tokio::time::{Instant, Interval, MissedTickBehavior};
use yap_protocol::{GatewayCloseCode, GatewayOp};
use yap_realtime::{Config, Connection, Event, Policy, Reconnect, TicketError, TicketFuture};

use crate::engine::{MediaEngine, MediaSession};
use crate::machine::{
    CallMachine, CallTarget, CameraPublished, Effect, Input, MediaEvent, MediaSpec, Notice,
    Published, RoomEvent, Socket, SocketSpec, TicketReplies, Timer, UserCommand, VoiceEvent,
};
use crate::sfu::{LayerUpdate, Sfu, Signaling};
use crate::tracks::Remote;
use crate::vad::{EnergyMeter, VAD_TICK};
use crate::wire::{self, Participant, RoomState, Track};

/// A failed API request, as the ticket endpoint reports it.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct ApiFailure {
    /// 401: counts towards "session expired" on reconnects.
    pub unauthorized: bool,
    pub message: String,
}

/// `POST /api/voice/socket-ticket` with the signed-in session.
pub trait TicketApi: Clone + Send + Sync + 'static {
    fn socket_ticket(&self, body: Value) -> impl Future<Output = Result<Value, ApiFailure>> + Send;
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
pub struct CallEnv<E, T> {
    pub engine: E,
    pub tickets: T,
    pub http: reqwest::Client,
    /// The Yap API base, e.g. `https://yap.115jon.com`.
    pub api_base: String,
    /// The `Origin` for the call sockets.
    pub origin: String,
}

/// Runs one call until it is left or ends. Reports through `notify`;
/// returns why it failed (`None`: left). The engine is released at the end.
pub async fn run_call<E, T, C, N>(
    env: CallEnv<E, T>,
    target: CallTarget,
    mut commands: C,
    notify: N,
) -> Option<String>
where
    E: MediaEngine,
    T: TicketApi,
    C: Commands,
    N: Fn(Notice) + Sync,
{
    let mut io = Io::new(&env);
    let mut inbox = Inbox::default();
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
}

/// What the worker reports back.
#[derive(Debug, PartialEq)]
enum Done {
    /// (SFU track name, mid if the pull worked).
    Pull(String, Option<String>),
    Camera(Result<CameraPublished, String>),
}

/// Event sources that appear as the call opens.
#[derive(Default)]
struct Inbox {
    room: Option<UnboundedReceiver<Event>>,
    voice: Option<UnboundedReceiver<Event>>,
    media: Option<UnboundedReceiver<crate::machine::MediaState>>,
    pulls: Option<UnboundedReceiver<Done>>,
}

/// What the effects act on.
struct Io<'a, E: MediaEngine, T> {
    env: &'a CallEnv<E, T>,
    room: Option<Connection>,
    voice: Option<Connection>,
    session: Option<E::Session>,
    sfu: Option<Sfu>,
    jobs: Option<UnboundedSender<Job>>,
    worker: Option<JoinHandle<()>>,
    timers: Vec<(Instant, Timer)>,
    /// The mic level between two reads of the energy counters.
    meter: EnergyMeter,
    /// The VAD's clock.
    started: Instant,
}

enum Applied {
    Done,
    Input(Input),
    End(Option<String>),
}

/// Waits on an event source that may not be open yet (then: forever).
async fn recv<T>(rx: &mut Option<UnboundedReceiver<T>>) -> Option<T> {
    match rx {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

impl<'a, E: MediaEngine, T: TicketApi> Io<'a, E, T> {
    fn new(env: &'a CallEnv<E, T>) -> Self {
        Self {
            env,
            room: None,
            voice: None,
            session: None,
            sfu: None,
            jobs: None,
            worker: None,
            timers: Vec::new(),
            meter: EnergyMeter::default(),
            started: Instant::now(),
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
        inbox: &mut Inbox,
        commands: &mut C,
        notify: &N,
    ) -> Option<String> {
        let mut queue: VecDeque<Effect> = machine.handle(Input::Start).into();
        let mut periodic: Option<(Interval, Interval)> = None;
        let mut level = tokio::time::interval(VAD_TICK);
        level.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut commands_open = true;
        loop {
            // Effects run in order. Tickets and media are awaited here, so
            // socket events wait in their channels until the mic is
            // published (VOICE_PROTOCOL.md §6). The ticket fetch listens
            // for commands meanwhile: a Leave drops it before any socket opens.
            while let Some(effect) = queue.pop_front() {
                if let Effect::FetchTickets { room, voice } = effect {
                    let fetch = self.fetch_tickets(room, voice);
                    tokio::pin!(fetch);
                    let tickets = loop {
                        tokio::select! {
                            tickets = &mut fetch => break Some(tickets),
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
                    if let Some(tickets) = tickets {
                        queue.extend(machine.handle(Input::Tickets(tickets)));
                    }
                    continue;
                }
                match self.apply(effect, inbox, notify).await {
                    Applied::Done => {}
                    Applied::Input(input) => queue.extend(machine.handle(input)),
                    Applied::End(reason) => return reason,
                }
            }
            let (stats, refresh) = periodic.get_or_insert_with(|| {
                let refresh = Duration::from_secs(45 * 60);
                (
                    tokio::time::interval(Duration::from_secs(5)),
                    tokio::time::interval_at(Instant::now() + refresh, refresh),
                )
            });
            let next_timer = self.timers.iter().map(|(at, _)| *at).min();
            let input = tokio::select! {
                event = recv(&mut inbox.room) => match event {
                    Some(event) => match room_event(event) {
                        Some(event) => Input::Room(event),
                        None => continue,
                    },
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
                    Some(state) => MediaEvent::State(state),
                    None => MediaEvent::Closed,
                }),
                done = recv(&mut inbox.pulls) => match done {
                    Some(Done::Pull(sfu_track_name, mid)) => Input::PullDone { sfu_track_name, mid },
                    Some(Done::Camera(result)) => Input::CameraPublished(result),
                    None => {
                        inbox.pulls = None;
                        continue;
                    }
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
                _ = stats.tick() => Input::Timer(Timer::Stats),
                _ = refresh.tick() => Input::Timer(Timer::RefreshCredentials),
            };
            queue.extend(machine.handle(input));
        }
    }

    async fn apply<N: Fn(Notice)>(
        &mut self,
        effect: Effect,
        inbox: &mut Inbox,
        notify: &N,
    ) -> Applied {
        match effect {
            Effect::Notify(notice) => notify(notice),
            Effect::FetchTickets { room, voice } => {
                return Applied::Input(Input::Tickets(self.fetch_tickets(room, voice).await));
            }
            Effect::OpenSockets { room, voice } => self.open_sockets(room, voice, inbox),
            Effect::OpenMedia(spec) => {
                let result = self.open_media(spec, inbox).await;
                return Applied::Input(Input::Published(result.map_err(|e| format!("{e:#}"))));
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
            Effect::Pull(remote) => self.job(Job::Pull(remote)),
            Effect::Close(mids) => self.job(Job::Close(mids)),
            Effect::UpdateLayer(layer) => self.job(Job::Update(layer)),
            Effect::PublishCamera { stream_id } => self.job(Job::PublishCamera { stream_id }),
            Effect::SetCameraLive(live) => {
                if let Some(session) = &self.session {
                    session.set_camera_live(live);
                }
            }
            Effect::SetVideoSink { mid, key } => {
                if let Some(session) = &self.session {
                    session.set_video_sink(&mid, key.as_deref());
                }
            }
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
            Effect::LogStats => {
                if let Some(line) = self.env.engine.check_devices() {
                    eprintln!("{line}");
                }
                if let Some(session) = &self.session {
                    match session.stats().await {
                        Ok(line) => eprintln!("{line}"),
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

    async fn fetch_tickets(&self, room: Value, voice: Value) -> Result<TicketReplies, String> {
        let api = &self.env.tickets;
        let (room, voice) = tokio::join!(api.socket_ticket(room), api.socket_ticket(voice));
        let room = room.map_err(|e| format!("room ticket: {e}"))?;
        let voice = voice.map_err(|e| format!("voice ticket: {e}"))?;
        Ok(TicketReplies { room, voice })
    }

    /// Both sockets. They live in this call, so a new call never sees an
    /// old call's frames, and dropping them closes them.
    fn open_sockets(&mut self, room: SocketSpec, voice: SocketSpec, inbox: &mut Inbox) {
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
            call_tickets(env.tickets.clone(), room.first_ticket, room.ticket_body),
            room_tx,
        ));
        self.voice = Some(Connection::spawn(
            config(
                "voice/voice",
                &voice,
                Reconnect::Reidentify,
                GatewayOp::VoiceReady,
            ),
            call_tickets(env.tickets.clone(), voice.first_ticket, voice.ticket_body),
            voice_tx,
        ));
        inbox.room = Some(room_rx);
        inbox.voice = Some(voice_rx);
    }

    /// Opens media, creates our SFU session and pushes the mic (§3.3b).
    async fn open_media(&mut self, spec: MediaSpec, inbox: &mut Inbox) -> Result<Published> {
        let (session, states) = self.env.engine.open(&spec.ice_servers)?;
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
        let offer = session.publish_mic(&spec.stream_id).await?;
        let sfu_track_name = uuid::Uuid::new_v4().to_string();
        let pushed = sfu
            .push(&session_id, &offer.sdp, &offer.mid, &sfu_track_name)
            .await?;
        session.apply_answer(&pushed.answer_sdp, "answer").await?;
        session.start_capture();

        // Pulls run in their own task so a slow retry never blocks Leave
        // or frames.
        let (jobs, jobs_rx) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        self.worker = Some(tokio::spawn(pull_worker(
            session,
            sfu,
            session_id.clone(),
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
        self.room = None;
        self.voice = None;
        self.jobs = None;
        if let Some(worker) = self.worker.take() {
            worker.abort();
        }
        if let Some(session) = self.session.take() {
            session.close();
        }
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
            speaking: d.get("speaking").and_then(Value::as_i64).unwrap_or(0) & 1 == 1,
        }),
        Event::Frame(GatewayOp::StopTracks, d) => Some(VoiceEvent::StopTracks {
            participant_id: pid(&d),
            names: wire::stopped_names(&d),
        }),
        Event::Frame(GatewayOp::ClientDisconnect, d) => Some(VoiceEvent::ClientDisconnect {
            participant_id: pid(&d),
        }),
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

/// Tickets for one call socket: the one fetched at join first, then a fresh
/// one per reconnect (tickets are single use).
fn call_tickets<T: TicketApi>(
    api: T,
    first: String,
    body: Value,
) -> impl FnMut() -> TicketFuture + Send + 'static {
    let mut first = Some(first).filter(|t| !t.is_empty());
    move || {
        let first = first.take();
        let api = api.clone();
        let body = body.clone();
        Box::pin(async move {
            if let Some(ticket) = first {
                return Ok(ticket);
            }
            let reply = api.socket_ticket(body).await.map_err(|e| {
                if e.unauthorized {
                    TicketError::Unauthorized
                } else {
                    TicketError::Failed(e.message)
                }
            })?;
            reply
                .get("ticket")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .ok_or_else(|| TicketError::Failed("socket ticket missing".into()))
        })
    }
}

/// Runs pulls, closes and the camera push one at a time (each renegotiates
/// the one peer connection).
async fn pull_worker<S: MediaSession, G: Signaling>(
    session: S,
    sfu: G,
    session_id: String,
    mut jobs: UnboundedReceiver<Job>,
    done: UnboundedSender<Done>,
) {
    // Our camera's SFU track, kept while the camera is off.
    let mut camera: Option<CameraPublished> = None;
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
                        None
                    }
                };
                let _ = done.send(Done::Pull(remote.sfu_track_name, mid));
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
    let offer = session.publish_camera(stream_id).await?;
    let track_name = uuid::Uuid::new_v4().to_string();
    let answer = sfu
        .push(session_id, &offer.sdp, &offer.mid, &track_name)
        .await?;
    session.apply_answer(&answer, "camera answer").await?;
    let published = CameraPublished {
        sfu_track_name: track_name,
        simulcast_rids: session.camera_rids(),
    };
    *kept = Some(published.clone());
    Ok(published)
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

    use super::*;
    use crate::engine::{FakeEngine, FakeOp};
    use crate::sfu::Pulled;

    /// Records SFU calls; the first pull of `flaky` fails once.
    #[derive(Clone, Default)]
    struct FakeSfu {
        calls: Arc<Mutex<Vec<String>>>,
        flaky: Option<String>,
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
        async fn push(&self, _: &str, offer_sdp: &str, mid: &str, _: &str) -> Result<String> {
            self.log(format!("push {mid} {offer_sdp}"));
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
        }
    }

    /// Ticket replies that never come.
    #[derive(Clone)]
    struct StuckTickets;

    impl TicketApi for StuckTickets {
        async fn socket_ticket(&self, _: Value) -> Result<Value, ApiFailure> {
            std::future::pending().await
        }
    }

    #[tokio::test]
    async fn leave_while_getting_tickets_should_end_before_any_socket_opens() {
        let engine = FakeEngine::new();
        let env = CallEnv {
            engine: engine.clone(),
            tickets: StuckTickets,
            http: reqwest::Client::new(),
            api_base: "https://api.invalid".into(),
            origin: "https://api.invalid".into(),
        };
        let target = CallTarget {
            server_id: "s1".into(),
            channel_id: "c1".into(),
            me_name: "Jon".into(),
            me_username: "jon".into(),
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
}
