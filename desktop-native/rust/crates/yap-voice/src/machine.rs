//! [`CallMachine`]: one call as a pure, synchronous state machine. See the
//! crate docs for the transition table.

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use serde_json::Value;
use yap_protocol::{GatewayOp, VOICE_MOVED_CLOSE_REASON};
use yap_realtime::StopReason;

use crate::roster::{Member, Roster};
use crate::tracks::{Remote, TrackTable};
use crate::wire::{self, Bootstrap, IceServer, Participant, RoomState, Track};

/// How long a participant whose voice socket went away has to re-announce
/// (a voice reconnect: call-socket backoff tops out at 10 s, plus a ticket).
pub const REANNOUNCE_GRACE: Duration = Duration::from_secs(15);

/// How long Leave waits for its goodbye frames to go out before closing.
pub const LEAVE_LINGER: Duration = Duration::from_millis(200);

/// The call's lifecycle, with the same names and meaning as the web
/// client's `packages/voice` machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallState {
    /// Not started, or over.
    Idle,
    /// Tickets, sockets, SFU session and push, until all three of room,
    /// voice socket and media are up for the first time.
    Connecting,
    /// Room and voice sockets ready and media connected.
    Joined,
    /// Joined before; a call socket dropped and is coming back.
    Reconnecting,
    /// Leave was asked: goodbye frames are out, closing shortly.
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallTarget {
    pub server_id: String,
    pub channel_id: String,
    pub me_name: String,
    pub me_username: String,
}

impl CallTarget {
    /// The room both call sockets and the SFU proxy live under.
    pub fn slug(&self) -> String {
        format!("voice-{}-{}", self.server_id, self.channel_id)
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

/// What the user asks of a running call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UserCommand {
    SetMuted(bool),
    Leave,
}

/// Both ticket replies (`POST /api/voice/socket-ticket`).
#[derive(Debug, Clone, PartialEq)]
pub struct TicketReplies {
    pub room: Value,
    pub voice: Value,
}

/// Our mic, published to the SFU.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// Our SFU session capability (opaque; never shown).
    pub session_id: String,
    pub sfu_track_name: String,
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
    /// Op 9 answering a credentials refresh (op 40).
    Credentials { voice_token: String },
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
    /// `participant`'s re-announce grace ran out.
    Grace { participant: String, token: u64 },
    /// Leave's goodbye frames had their time.
    Linger,
    /// Every 5 s: log media stats.
    Stats,
    /// Every 45 min: refresh voice credentials.
    RefreshCredentials,
}

/// Everything the machine consumes.
#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    Start,
    /// The two tickets, or why they could not be had.
    Tickets(Result<TicketReplies, String>),
    /// The mic is published, or why media could not start.
    Published(Result<Published, String>),
    Room(RoomEvent),
    Voice(VoiceEvent),
    Media(MediaEvent),
    /// A pull finished; `mid` if it worked.
    PullDone {
        sfu_track_name: String,
        mid: Option<String>,
    },
    Command(UserCommand),
    Timer(Timer),
}

/// What the machine reports to its owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    /// The state changed.
    State(CallState),
    /// The status line ("Getting tickets", "Joining", "Connected",
    /// "Reconnecting").
    Status(&'static str),
    /// Media connected for the first time (the store announces sidebar
    /// presence on it).
    Connected,
    /// The whole roster.
    Roster(Vec<Member>),
}

/// How to open one call socket.
#[derive(Debug, Clone, PartialEq)]
pub struct SocketSpec {
    /// The path under the API base, e.g. `/api/channels/<slug>/ws?v=1`.
    pub path: String,
    pub identify: Value,
    /// The ticket fetched at join (single use).
    pub first_ticket: String,
    /// The body for a fresh ticket on each reconnect.
    pub ticket_body: Value,
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
}

/// Everything the machine asks for. The owner runs them in order.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Notify(Notice),
    /// Fetch both tickets, answer with [`Input::Tickets`].
    FetchTickets {
        room: Value,
        voice: Value,
    },
    /// Open both sockets.
    OpenSockets {
        room: SocketSpec,
        voice: SocketSpec,
    },
    /// Open media, create the SFU session and push the mic; answer with
    /// [`Input::Published`] (socket events wait until then).
    OpenMedia(MediaSpec),
    /// Send a text frame once that socket is ready.
    Send(Socket, String),
    /// The identify the voice socket sends next.
    SetVoiceIdentify(Value),
    /// The voice token for SFU calls.
    SetVoiceToken(String),
    /// Pull a remote track (serialized with closes); answer with
    /// [`Input::PullDone`].
    Pull(Remote),
    /// Stop these receive transceivers and close them on the SFU.
    Close(Vec<String>),
    SetMicEnabled(bool),
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

/// One call. Feed it [`Input`]s with [`CallMachine::handle`]; run the
/// [`Effect`]s it returns, in order.
#[derive(Debug)]
pub struct CallMachine {
    target: CallTarget,
    state: CallState,
    ended: bool,
    me: String,
    voice_token: String,
    published: Option<Published>,
    sockets_open: bool,
    /// The room was identified once (a later Ready is a fresh session).
    room_identified: bool,
    room_up: bool,
    voice_ready: bool,
    /// VoiceReady was seen before (a later one follows a re-identify).
    reidentified: bool,
    /// Media connected at least once.
    connected: bool,
    muted: bool,
    roster: Option<Roster>,
    tracks: TrackTable,
    next_token: u64,
}

impl CallMachine {
    pub fn new(target: CallTarget) -> Self {
        Self {
            target,
            state: CallState::Idle,
            ended: false,
            me: String::new(),
            voice_token: String::new(),
            published: None,
            sockets_open: false,
            room_identified: false,
            room_up: false,
            voice_ready: false,
            reidentified: false,
            connected: false,
            muted: false,
            roster: None,
            tracks: TrackTable::default(),
            next_token: 0,
        }
    }

    pub fn state(&self) -> CallState {
        self.state
    }

    /// The remote tracks (for inspection).
    pub fn tracks(&self) -> &TrackTable {
        &self.tracks
    }

    /// The roster so far (empty before the tickets).
    pub fn roster(&self) -> Vec<Member> {
        self.roster
            .as_ref()
            .map(Roster::snapshot)
            .unwrap_or_default()
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
            (CallState::Leaving, Input::Timer(Timer::Linger)) => self.end(&mut fx, None),
            (CallState::Leaving, _) => {}
            (_, Input::Start) => {}
            (_, Input::Tickets(result)) => self.on_tickets(&mut fx, result),
            (_, Input::Published(result)) => self.on_published(&mut fx, result),
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
                let closing = self.tracks.pull_done(&sfu_track_name, mid);
                close(&mut fx, closing);
            }
            (_, Input::Command(command)) => self.on_command(&mut fx, command),
            (_, Input::Timer(timer)) => self.on_timer(&mut fx, timer),
        }
        // Pull what is waiting once media is up (VOICE_PROTOCOL.md §3.3c).
        if self.connected
            && matches!(
                self.state,
                CallState::Connecting | CallState::Joined | CallState::Reconnecting
            )
        {
            fx.extend(self.tracks.take_pulls().into_iter().map(Effect::Pull));
        }
        fx
    }

    fn set_state(&mut self, fx: &mut Vec<Effect>, state: CallState) {
        if self.state != state {
            self.state = state;
            fx.push(Effect::Notify(Notice::State(state)));
        }
    }

    fn end(&mut self, fx: &mut Vec<Effect>, reason: Option<String>) {
        self.set_state(fx, CallState::Idle);
        self.ended = true;
        fx.push(Effect::End(reason));
    }

    fn status(fx: &mut Vec<Effect>, status: &'static str) {
        fx.push(Effect::Notify(Notice::Status(status)));
    }

    fn emit_roster(&self, fx: &mut Vec<Effect>) {
        fx.push(Effect::Notify(Notice::Roster(self.roster())));
    }

    fn my_track(&self) -> String {
        format!("cam-audio-{}", self.me)
    }

    fn start(&mut self, fx: &mut Vec<Effect>) {
        self.set_state(fx, CallState::Connecting);
        Self::status(fx, "Getting tickets");
        let (t, slug) = (&self.target, self.target.slug());
        fx.push(Effect::FetchTickets {
            room: wire::ticket_body("room", &t.server_id, &t.channel_id, &slug),
            voice: wire::ticket_body("voice", &t.server_id, &t.channel_id, &slug),
        });
    }

    fn on_tickets(&mut self, fx: &mut Vec<Effect>, result: Result<TicketReplies, String>) {
        if self.sockets_open {
            return;
        }
        let replies = match result {
            Ok(replies) => replies,
            Err(error) => return self.end(fx, Some(error)),
        };
        let Some(bootstrap) = Bootstrap::from_room_ticket(&replies.room) else {
            return self.end(fx, Some("room ticket without media bootstrap".into()));
        };
        let Bootstrap {
            participant_id: me,
            voice_token,
            ice_servers,
        } = bootstrap;
        self.me = me;
        self.voice_token = voice_token;
        self.roster = Some(Roster::new(&self.me, &self.target.me_name));
        self.sockets_open = true;

        let t = &self.target;
        let slug = t.slug();
        Self::status(fx, "Joining");
        fx.push(Effect::OpenSockets {
            room: SocketSpec {
                path: format!("/api/channels/{slug}/ws?v=1"),
                identify: wire::room_identify(&t.me_name, &t.me_username),
                first_ticket: wire::str_of(&replies.room, "ticket").to_owned(),
                ticket_body: wire::ticket_body("room", &t.server_id, &t.channel_id, &slug),
            },
            voice: SocketSpec {
                path: format!("/api/channels/{slug}/voice?v=1"),
                identify: wire::voice_identify(&self.me, &self.voice_token),
                first_ticket: wire::str_of(&replies.voice, "ticket").to_owned(),
                ticket_body: wire::ticket_body("voice", &t.server_id, &t.channel_id, &slug),
            },
        });
        fx.push(Effect::OpenMedia(MediaSpec {
            ice_servers,
            stream_id: format!("cam-{}", self.me),
            slug,
            participant_id: self.me.clone(),
            voice_token: self.voice_token.clone(),
        }));
    }

    fn on_published(&mut self, fx: &mut Vec<Effect>, result: Result<Published, String>) {
        if self.published.is_some() || !self.sockets_open {
            return;
        }
        match result {
            Ok(published) => {
                self.published = Some(published);
                if self.muted {
                    fx.push(Effect::SetMicEnabled(false));
                }
                self.emit_roster(fx);
                // Only if the voice socket was ready first (the driver holds
                // socket events until now, so normally it is not).
                if self.voice_ready {
                    self.announce(fx);
                }
            }
            Err(error) => self.end(fx, Some(error)),
        }
    }

    fn announce(&self, fx: &mut Vec<Effect>) {
        if let Some(p) = &self.published {
            let frame =
                wire::announce(&self.me, &self.my_track(), &p.sfu_track_name, &p.session_id);
            fx.push(Effect::Send(Socket::Voice, frame));
        }
    }

    /// Status "Connected" (and Joined) once room, voice and media are all up.
    fn maybe_joined(&mut self, fx: &mut Vec<Effect>) {
        if self.connected && self.room_up && self.voice_ready {
            Self::status(fx, "Connected");
            self.set_state(fx, CallState::Joined);
        }
    }

    fn socket_down(&mut self, fx: &mut Vec<Effect>) {
        Self::status(fx, "Reconnecting");
        if self.state == CallState::Joined {
            self.set_state(fx, CallState::Reconnecting);
        }
    }

    /// Takes a fresh voice token from the room, for the SFU and the voice
    /// socket's next identify.
    fn update_token(&mut self, fx: &mut Vec<Effect>, token: &str) {
        if !token.is_empty() && token != self.voice_token {
            token.clone_into(&mut self.voice_token);
            fx.push(Effect::SetVoiceToken(self.voice_token.clone()));
            fx.push(Effect::SetVoiceIdentify(wire::voice_identify(
                &self.me,
                &self.voice_token,
            )));
        }
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
                    return self.end(
                        fx,
                        Some("The call connection was reset. Join again.".into()),
                    );
                }
                if self.room_identified {
                    fx.push(Effect::Log(
                        "[voice/room] identified fresh after a drop".into(),
                    ));
                }
                self.room_identified = true;
                self.room_up = true;
                self.replace_roster(&d);
                fx.push(Effect::Send(Socket::Room, wire::voice_state(self.muted)));
                self.emit_roster(fx);
                self.maybe_joined(fx);
            }
            RoomEvent::Resumed(d) => {
                self.room_up = true;
                self.update_token(fx, &d.voice_token);
                self.replace_roster(&d);
                self.emit_roster(fx);
                self.maybe_joined(fx);
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
                    let closing = self.tracks.drop_participant(id);
                    close(fx, closing);
                } else if let Some(roster) = &mut self.roster {
                    roster.upsert(&participant);
                }
                self.emit_roster(fx);
            }
            RoomEvent::Credentials { voice_token } => self.update_token(fx, &voice_token),
            RoomEvent::Disconnected => {
                self.room_up = false;
                self.socket_down(fx);
            }
            RoomEvent::Stopped(reason) => self.end(fx, Some(stop_message(reason))),
            RoomEvent::Closed => self.end(fx, Some("Room connection closed".into())),
        }
    }

    fn replace_roster(&mut self, d: &RoomState) {
        if let (Some(roster), Some(list)) = (&mut self.roster, &d.participants) {
            roster.replace_others(list);
        }
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
                    let live: HashSet<&str> =
                        tracks.iter().map(|t| t.sfu_track_name.as_str()).collect();
                    let closing = self.tracks.keep_live(&live);
                    close(fx, closing);
                }
                // Announce (again): a re-identified socket is a new voice
                // session to everyone else.
                self.announce(fx);
                for t in &tracks {
                    self.tracks.cancel_grace(&t.participant_id);
                }
                self.tracks.queue(&tracks, "", &self.me);
                if let Some(roster) = &mut self.roster {
                    for (pid, bit) in &speaking {
                        roster.set_speaking(pid, *bit);
                    }
                }
                self.emit_roster(fx);
                self.maybe_joined(fx);
            }
            VoiceEvent::Video {
                participant_id,
                tracks,
            } => {
                for t in &tracks {
                    let pid = match t.participant_id.as_str() {
                        "" => participant_id.as_str(),
                        pid => pid,
                    };
                    self.tracks.cancel_grace(pid);
                }
                let closing = self.tracks.replace_republished(&tracks, &participant_id);
                close(fx, closing);
                self.tracks.queue(&tracks, &participant_id, &self.me);
            }
            VoiceEvent::Speaking {
                participant_id,
                speaking,
            } => {
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
                let closing = self.tracks.stop(&participant_id, &names);
                close(fx, closing);
            }
            VoiceEvent::ClientDisconnect { participant_id } => {
                // Their voice socket is gone, or they re-identified (the
                // room sends this either way). Keep the row and pulls for a
                // grace; a Video from them in time cancels it.
                if participant_id.is_empty() || participant_id == self.me {
                    return;
                }
                self.next_token += 1;
                self.tracks.start_grace(&participant_id, self.next_token);
                self.tracks.drop_pending_of(&participant_id);
                fx.push(Effect::ArmTimer {
                    timer: Timer::Grace {
                        participant: participant_id,
                        token: self.next_token,
                    },
                    after: REANNOUNCE_GRACE,
                });
            }
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
                self.voice_ready = false;
                self.socket_down(fx);
            }
            VoiceEvent::Stopped(reason) => self.end(fx, Some(stop_message(reason))),
            VoiceEvent::Closed => self.end(fx, Some("Voice connection closed".into())),
        }
    }

    fn on_media(&mut self, fx: &mut Vec<Effect>, event: MediaEvent) {
        match event {
            MediaEvent::State(MediaState::Connected) => {
                if !self.connected {
                    self.connected = true;
                    fx.push(Effect::Notify(Notice::Connected));
                    Self::status(fx, "Connected");
                    if self.room_up && self.voice_ready {
                        self.set_state(fx, CallState::Joined);
                    }
                }
            }
            MediaEvent::State(MediaState::Failed) => {
                self.end(fx, Some("Media connection failed".into()));
            }
            MediaEvent::State(_) => {}
            MediaEvent::Closed => self.end(fx, Some("Media connection ended".into())),
        }
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
                if self.sockets_open {
                    fx.push(Effect::Send(Socket::Room, wire::voice_state(muted)));
                }
                self.emit_roster(fx);
            }
            UserCommand::Leave => {
                if !self.sockets_open {
                    return self.end(fx, None);
                }
                // A socket that is not ready queues these and drops them
                // when it closes.
                if self.voice_ready {
                    fx.push(Effect::Send(
                        Socket::Voice,
                        wire::stop_tracks(&self.my_track()),
                    ));
                }
                fx.push(Effect::Send(
                    Socket::Voice,
                    wire::empty_frame(GatewayOp::ClientDisconnect),
                ));
                fx.push(Effect::Send(
                    Socket::Room,
                    wire::empty_frame(GatewayOp::ClientDisconnect),
                ));
                fx.push(Effect::ArmTimer {
                    timer: Timer::Linger,
                    after: LEAVE_LINGER,
                });
                self.set_state(fx, CallState::Leaving);
            }
        }
    }

    fn on_timer(&mut self, fx: &mut Vec<Effect>, timer: Timer) {
        match timer {
            Timer::Grace { participant, token } => {
                if !self.tracks.expire_grace(&participant, token) {
                    return;
                }
                let closing = self.tracks.retire_pulls_of(&participant);
                close(fx, closing);
                // The room keeps a dropped participant through its resume
                // grace, so drop the row here; a later room update re-adds
                // it with its known name.
                if let Some(roster) = &mut self.roster {
                    roster.remove(&participant);
                }
                self.emit_roster(fx);
            }
            Timer::Linger => {}
            Timer::Stats => {
                if self.connected {
                    fx.push(Effect::LogStats);
                }
            }
            Timer::RefreshCredentials => {
                if self.sockets_open {
                    fx.push(Effect::Send(
                        Socket::Room,
                        wire::empty_frame(GatewayOp::RefreshVoiceCredentials),
                    ));
                }
            }
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
