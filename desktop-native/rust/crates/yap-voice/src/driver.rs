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
use tokio::time::{Instant, Interval};
use yap_protocol::{GatewayCloseCode, GatewayOp};
use yap_realtime::{Config, Connection, Event, Policy, Reconnect, TicketError, TicketFuture};

use crate::engine::{MediaEngine, MediaSession};
use crate::machine::{
    CallMachine, CallTarget, Effect, Input, MediaEvent, MediaSpec, Notice, Published, RoomEvent,
    Socket, SocketSpec, TicketReplies, Timer, UserCommand, VoiceEvent,
};
use crate::sfu::{Sfu, Signaling};
use crate::tracks::Remote;
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
}

/// (SFU track name, mid if the pull worked).
type PullDone = (String, Option<String>);

/// Event sources that appear as the call opens.
#[derive(Default)]
struct Inbox {
    room: Option<UnboundedReceiver<Event>>,
    voice: Option<UnboundedReceiver<Event>>,
    media: Option<UnboundedReceiver<crate::machine::MediaState>>,
    pulls: Option<UnboundedReceiver<PullDone>>,
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
        }
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
        let mut commands_open = true;
        loop {
            // Effects run in order. Tickets and media are awaited here, so
            // socket events wait in their channels until the mic is
            // published (VOICE_PROTOCOL.md §6).
            while let Some(effect) = queue.pop_front() {
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
                    Some((sfu_track_name, mid)) => Input::PullDone { sfu_track_name, mid },
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
            Effect::SetMicEnabled(enabled) => {
                if let Some(session) = &self.session {
                    session.set_mic_enabled(enabled);
                }
            }
            Effect::LogStats => {
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

/// Runs pulls and closes one at a time (each renegotiates the one peer
/// connection).
async fn pull_worker<S: MediaSession, G: Signaling>(
    session: S,
    sfu: G,
    session_id: String,
    mut jobs: UnboundedReceiver<Job>,
    done: UnboundedSender<PullDone>,
) {
    while let Some(job) = jobs.recv().await {
        match job {
            Job::Pull(remote) => {
                let mid = match pull(&session, &sfu, &session_id, &remote).await {
                    Ok(mid) => {
                        eprintln!(
                            "[voice] pulled {} from {}",
                            remote.track_name, remote.participant_id
                        );
                        Some(mid)
                    }
                    Err(error) => {
                        eprintln!("[voice] pull {} failed: {error:#}", remote.track_name);
                        None
                    }
                };
                let _ = done.send((remote.sfu_track_name, mid));
            }
            Job::Close(mids) => match close(&session, &sfu, &session_id, &mids).await {
                Ok(()) => eprintln!("[voice] closed {} transceiver(s)", mids.len()),
                Err(error) => eprintln!("[voice] close failed: {error:#}"),
            },
        }
    }
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
            .pull(session_id, &remote.sfu_track_name, &remote.session_id)
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
        async fn pull(&self, _: &str, track_name: &str, _: &str) -> Result<Pulled> {
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
        }
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
                ("a".to_owned(), Some("mid-a".to_owned())),
                ("b".to_owned(), Some("mid-b".to_owned())),
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
}
