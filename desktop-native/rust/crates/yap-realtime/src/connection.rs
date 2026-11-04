//! One realtime WebSocket, kept alive: the Rust side of
//! `packages/realtime/src/realtime-socket.ts`.
//!
//! ```text
//! ticket → open → Hello (heartbeat starts) → Identify or Resume
//!        → Ready / Resumed / VoiceReady (backoff resets, queue flushes)
//! ```
//!
//! When the socket goes it backs off and tries again with a fresh ticket,
//! unless the close says stop (see [`crate::policy::close_action`]). The
//! connection runs as its own task; [`Connection`] is the handle, and
//! dropping it closes the socket with `1000` and ends the task.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::OnceLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::net::TcpStream;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval_at, timeout, Instant, Interval, MissedTickBehavior};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::handshake::client::Request;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use yap_protocol::payload::{ChatReady, GatewayError, Hello, Resume};
use yap_protocol::{
    Frame, GatewayCloseCode, GatewayOp, DEFAULT_HEARTBEAT_INTERVAL_MS, HEARTBEAT_FRAME,
};

use crate::policy::{close_action, CloseAction, Policy, ABANDONED_CODE};

/// The realtime subprotocol; the ticket rides as a second one.
pub const SUBPROTOCOL: &str = "ralph.realtime.v1";

/// How long a close frame may take to go out before the socket is dropped.
const CLOSE_TIMEOUT: Duration = Duration::from_secs(1);

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
type Writer = SplitSink<Socket, Message>;

/// Why a ticket could not be had.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TicketError {
    /// The ticket endpoint answered 401: the session may be gone.
    #[error("ticket refused (401)")]
    Unauthorized,
    #[error("ticket request failed: {0}")]
    Failed(String),
}

pub type TicketFuture = Pin<Box<dyn Future<Output = Result<String, TicketError>> + Send>>;

/// Hands out one fresh ticket per connection attempt (tickets are single use).
pub trait TicketSource: Send + 'static {
    fn ticket(&mut self) -> TicketFuture;
}

impl<F> TicketSource for F
where
    F: FnMut() -> TicketFuture + Send + 'static,
{
    fn ticket(&mut self) -> TicketFuture {
        self()
    }
}

/// What a socket does after a drop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reconnect {
    /// MeetingRoom sockets (main gateway, call room): Ready's
    /// `participant_id` is the session; the next Hello is answered with
    /// Resume (op 7 `{session_id, seq_ack}`), and Resumed (op 9) means ready.
    /// Error 4006 while resuming, or close 4006, falls back to Identify.
    Resume,
    /// VoiceRoom: identify again on every socket. An Error 4004 to the
    /// identify (expired voice token) surfaces [`Event::IdentifyRejected`];
    /// [`Connection::set_identify`] with a fresh token sends it again.
    Reidentify,
}

pub struct Config {
    /// For log lines.
    pub label: &'static str,
    /// The full `ws://` or `wss://` URL.
    pub url: String,
    /// The `Origin` header.
    pub origin: String,
    /// The whole identify frame (`{"op":0,"d":{..}}` or op 100).
    pub identify: Value,
    pub reconnect: Reconnect,
    /// The op that answers the identify (Ready or VoiceReady).
    pub ready_op: GatewayOp,
    pub policy: Policy,
    /// Debug only: drop the TCP stream abruptly this long after the first
    /// ready, once per connection (see [`debug_drop_after`]).
    pub debug_drop_after: Option<Duration>,
}

/// `YAP_DEBUG_DROP_AFTER_SECS=<n>`, read once: simulates a network loss
/// (no close frame) `n` s after a connection is first ready. Unset: off.
pub fn debug_drop_after() -> Option<Duration> {
    static AFTER: OnceLock<Option<Duration>> = OnceLock::new();
    *AFTER.get_or_init(|| {
        std::env::var("YAP_DEBUG_DROP_AFTER_SECS")
            .ok()
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|secs| secs.is_finite() && *secs >= 0.0)
            .map(Duration::from_secs_f64)
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The socket opened (Hello comes next).
    Connected,
    /// The identify was answered (`ready_op`'s `d`).
    Ready(Value),
    /// The Resume was accepted (op 9's `d`); the missed events came first.
    Resumed(Value),
    /// Any other frame with a known opcode. Error frames always come through.
    Frame(GatewayOp, Value),
    /// The voice token was refused (Error or close 4004 before ready).
    IdentifyRejected,
    /// An open socket went away; a retry follows unless [`Event::Stopped`] does.
    Disconnected { code: Option<u16>, reason: String },
    /// A retry is scheduled; `attempt` counts from 1 since the last ready.
    Reconnecting { attempt: u32, delay: Duration },
    /// No more retries.
    Stopped(StopReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// A newer connection took over (4010, or 1000 "Replaced by new
    /// connection"). `reason` is the close reason (e.g. a moderator move).
    Replaced { code: u16, reason: String },
    /// Admission rejected (4008).
    AdmissionRejected { reason: String },
    /// The ticket was refused (401) too many times in a row.
    SessionExpired,
}

enum Command {
    Send(String),
    SetIdentify(Value),
}

/// The handle. Dropping it closes the socket and stops reconnecting.
pub struct Connection {
    commands: UnboundedSender<Command>,
}

impl Connection {
    /// Starts connecting now. Must be called inside a Tokio runtime.
    pub fn spawn(
        config: Config,
        tickets: impl TicketSource,
        events: UnboundedSender<Event>,
    ) -> Self {
        let (commands, rx) = mpsc::unbounded_channel();
        let actor = Actor {
            config,
            tickets,
            commands: rx,
            events,
            queue: VecDeque::new(),
            session: None,
            last_seq: 0,
            retries: 0,
            refusals: 0,
            rng: seed(),
            debug_dropped: false,
        };
        tokio::spawn(actor.run());
        Self { commands }
    }

    /// Sends a text frame once ready (Ready, Resumed or VoiceReady); until
    /// then, and across reconnects, it waits in order in a queue.
    pub fn send(&self, text: impl Into<String>) {
        let _ = self.commands.send(Command::Send(text.into()));
    }

    /// Replaces the identify frame for the next socket. If the current one
    /// had its identify refused (4004), this one is sent on it now.
    pub fn set_identify(&self, frame: Value) {
        let _ = self.commands.send(Command::SetIdentify(frame));
    }
}

/// How one attempt ended.
enum Ended {
    /// The handle was dropped.
    Shutdown,
    /// No socket: the ticket or the handshake failed.
    Failed(String),
    /// The ticket endpoint said 401.
    Refused,
    /// An open socket closed (or was abandoned).
    Closed { code: Option<u16>, reason: String },
}

/// Per-socket state.
#[derive(Default)]
struct Link {
    heartbeat: Option<Interval>,
    acked: bool,
    missed: u32,
    hello: bool,
    identify_sent: bool,
    resuming: bool,
    ready: bool,
}

struct Actor<T> {
    config: Config,
    tickets: T,
    commands: UnboundedReceiver<Command>,
    events: UnboundedSender<Event>,
    /// Sent once ready, in order.
    queue: VecDeque<String>,
    /// Resume session id (Ready's `participant_id`).
    session: Option<String>,
    /// The highest `d.seq` seen on any frame but HeartbeatACK.
    last_seq: u64,
    /// Backoff retries since the last ready.
    retries: u32,
    /// Ticket refusals in a row.
    refusals: u32,
    rng: u64,
    /// The debug drop has been scheduled (it fires once per connection).
    debug_dropped: bool,
}

impl<T: TicketSource> Actor<T> {
    async fn run(mut self) {
        loop {
            match self.attempt().await {
                Ended::Shutdown => return,
                Ended::Failed(error) => {
                    self.refusals = 0;
                    eprintln!("[{}] connect failed: {error}", self.config.label);
                }
                Ended::Refused => {
                    self.refusals += 1;
                    if self.refusals >= self.config.policy.refusals_to_fail {
                        eprintln!("[{}] ticket refused (401); stopping", self.config.label);
                        return self.emit(Event::Stopped(StopReason::SessionExpired));
                    }
                    eprintln!(
                        "[{}] ticket refused (401); retrying once",
                        self.config.label
                    );
                }
                Ended::Closed { code, reason } => {
                    eprintln!(
                        "[{}] connection lost (code={code:?}, reason={reason})",
                        self.config.label
                    );
                    let action = close_action(code, &reason);
                    self.emit(Event::Disconnected {
                        code,
                        reason: reason.clone(),
                    });
                    match action {
                        CloseAction::StopReplaced => {
                            let code = code.unwrap_or(GatewayCloseCode::SessionReplaced.code());
                            return self
                                .emit(Event::Stopped(StopReason::Replaced { code, reason }));
                        }
                        CloseAction::StopAdmissionRejected => {
                            return self
                                .emit(Event::Stopped(StopReason::AdmissionRejected { reason }));
                        }
                        CloseAction::RetryFresh => self.forget_session(),
                        CloseAction::RetryAfterTokenRefresh => {
                            if self.config.reconnect == Reconnect::Reidentify {
                                self.emit(Event::IdentifyRejected);
                            }
                        }
                        CloseAction::Retry => {}
                    }
                }
            }
            if !self.backoff().await {
                return;
            }
        }
    }

    /// One ticket, one socket.
    async fn attempt(&mut self) -> Ended {
        let ticket = self.tickets.ticket();
        let ticket = match self.idle(ticket).await {
            None => return Ended::Shutdown,
            Some(Ok(ticket)) => ticket,
            Some(Err(TicketError::Unauthorized)) => return Ended::Refused,
            Some(Err(TicketError::Failed(error))) => return Ended::Failed(error),
        };
        self.refusals = 0;
        let request = match self.request(&ticket) {
            Ok(request) => request,
            Err(error) => return Ended::Failed(error),
        };
        let socket = match self.idle(tokio_tungstenite::connect_async(request)).await {
            None => return Ended::Shutdown,
            Some(Ok((socket, _))) => socket,
            Some(Err(error)) => return Ended::Failed(error.to_string()),
        };
        self.emit(Event::Connected);
        self.session_loop(socket).await
    }

    fn request(&self, ticket: &str) -> Result<Request, String> {
        let mut request = self
            .config
            .url
            .as_str()
            .into_client_request()
            .map_err(|e| e.to_string())?;
        let headers = request.headers_mut();
        let origin = HeaderValue::from_str(&self.config.origin).map_err(|e| e.to_string())?;
        // The error never echoes the value, so the ticket stays out of logs.
        let protocols = HeaderValue::from_str(&format!("{SUBPROTOCOL}, ralph.ticket.{ticket}"))
            .map_err(|e| format!("bad ticket: {e}"))?;
        headers.insert("Origin", origin);
        headers.insert("Sec-WebSocket-Protocol", protocols);
        Ok(request)
    }

    /// Waits for `fut` while still taking commands (sends queue up). `None`
    /// when the handle was dropped.
    async fn idle<F: Future>(&mut self, fut: F) -> Option<F::Output> {
        tokio::pin!(fut);
        loop {
            tokio::select! {
                out = &mut fut => return Some(out),
                command = self.commands.recv() => match command? {
                    Command::Send(text) => self.queue.push_back(text),
                    Command::SetIdentify(frame) => self.config.identify = frame,
                },
            }
        }
    }

    /// Schedules the next attempt; `false` when the handle was dropped.
    async fn backoff(&mut self) -> bool {
        let (r1, r2) = (self.random(), self.random());
        let delay = self.config.policy.backoff.delay(self.retries, r1, r2);
        self.retries += 1;
        eprintln!(
            "[{}] reconnecting in {} ms (attempt {})",
            self.config.label,
            delay.as_millis(),
            self.retries
        );
        self.emit(Event::Reconnecting {
            attempt: self.retries,
            delay,
        });
        self.idle(tokio::time::sleep(delay)).await.is_some()
    }

    async fn session_loop(&mut self, socket: Socket) -> Ended {
        let (mut writer, mut reader) = socket.split();
        let mut link = Link::default();
        let mut drop_at: Option<Instant> = None;
        loop {
            let out: Vec<String> = tokio::select! {
                frame = reader.next() => match frame {
                    Some(Ok(Message::Text(text))) => self.on_text(&mut link, text.as_str()),
                    Some(Ok(Message::Close(close))) => {
                        let (code, reason) = close
                            .map(|c| (Some(u16::from(c.code)), c.reason.to_string()))
                            .unwrap_or((None, String::new()));
                        return Ended::Closed { code, reason };
                    }
                    Some(Ok(_)) => Vec::new(),
                    Some(Err(error)) => return closed(error.to_string()),
                    None => return closed("socket ended".into()),
                },
                command = self.commands.recv() => match command {
                    Some(Command::Send(text)) if link.ready => vec![text],
                    Some(Command::Send(text)) => {
                        self.queue.push_back(text);
                        Vec::new()
                    }
                    Some(Command::SetIdentify(frame)) => {
                        self.config.identify = frame;
                        let retry = link.hello && !link.ready && !link.identify_sent && !link.resuming;
                        if retry {
                            link.identify_sent = true;
                            vec![self.config.identify.to_string()]
                        } else {
                            Vec::new()
                        }
                    }
                    None => {
                        send_close(&mut writer, CloseCode::Normal, "leave").await;
                        return Ended::Shutdown;
                    }
                },
                () = sleep_until_some(drop_at) => {
                    // No close frame: the server sees a lost connection.
                    eprintln!("[realtime] debug: dropping {}", url_path(&self.config.url));
                    return closed("debug drop".into());
                }
                _ = tick(&mut link.heartbeat) => {
                    if !link.acked {
                        link.missed += 1;
                        eprintln!(
                            "[{}] heartbeat not acknowledged ({}/{})",
                            self.config.label, link.missed, self.config.policy.max_missed_acks
                        );
                        if link.missed >= self.config.policy.max_missed_acks {
                            // A dead link may never finish the close handshake:
                            // give up on it now.
                            let reason = "heartbeat not acknowledged";
                            send_close(&mut writer, CloseCode::from(ABANDONED_CODE), reason).await;
                            return Ended::Closed { code: Some(ABANDONED_CODE), reason: reason.into() };
                        }
                    }
                    link.acked = false;
                    vec![HEARTBEAT_FRAME.to_owned()]
                }
            };
            if link.ready && !self.debug_dropped {
                if let Some(after) = self.config.debug_drop_after {
                    self.debug_dropped = true;
                    drop_at = Some(Instant::now() + after);
                }
            }
            for text in out {
                if let Err(error) = writer.send(Message::text(text)).await {
                    return closed(error.to_string());
                }
            }
        }
    }

    /// Handles one text frame; returns what to write.
    fn on_text(&mut self, link: &mut Link, text: &str) -> Vec<String> {
        let Ok(frame) = Frame::decode(text) else {
            return Vec::new();
        };
        let op = frame.gateway_op();
        // The replay cursor is read from every well-formed frame.
        if op != Some(GatewayOp::HeartbeatACK) {
            if let Some(seq) = frame.d.get("seq").and_then(Value::as_u64) {
                self.last_seq = self.last_seq.max(seq);
            }
        }
        let Some(op) = op else {
            return Vec::new();
        };
        match op {
            GatewayOp::Hello => {
                let hello: Option<Hello> = frame.payload().ok();
                let ms = hello
                    .and_then(|h| h.heartbeat_interval)
                    .unwrap_or(DEFAULT_HEARTBEAT_INTERVAL_MS);
                let period = Duration::from_millis(ms.max(1));
                let mut heartbeat = interval_at(Instant::now() + period, period);
                heartbeat.set_missed_tick_behavior(MissedTickBehavior::Delay);
                link.heartbeat = Some(heartbeat);
                link.acked = true;
                link.missed = 0;
                link.hello = true;
                vec![self.hello_reply(link)]
            }
            GatewayOp::HeartbeatACK => {
                link.acked = true;
                link.missed = 0;
                Vec::new()
            }
            op if op == self.config.ready_op && !link.ready => {
                if self.config.reconnect == Reconnect::Resume {
                    let ready: Option<ChatReady> = frame.payload().ok();
                    self.session = ready
                        .and_then(|r| r.participant_id)
                        .filter(|id| !id.is_empty());
                    self.last_seq = 0;
                }
                link.resuming = false;
                let out = self.mark_ready(link);
                self.emit(Event::Ready(frame.d));
                out
            }
            GatewayOp::Resumed if link.resuming => {
                link.resuming = false;
                let out = self.mark_ready(link);
                self.emit(Event::Resumed(frame.d));
                out
            }
            GatewayOp::Error => {
                let code = frame
                    .payload::<GatewayError>()
                    .ok()
                    .and_then(|e| e.close_code());
                let mut out = Vec::new();
                match code {
                    Some(GatewayCloseCode::SessionInvalid) if link.resuming => {
                        // Unknown session or continuity lost: Identify and
                        // let the owner reconcile.
                        eprintln!("[{}] resume refused; identifying", self.config.label);
                        self.forget_session();
                        link.resuming = false;
                        link.identify_sent = true;
                        out.push(self.config.identify.to_string());
                    }
                    Some(GatewayCloseCode::AdmissionRejected) if link.resuming => {
                        self.forget_session();
                        link.resuming = false;
                    }
                    Some(GatewayCloseCode::AuthenticationFailed)
                        if self.config.reconnect == Reconnect::Reidentify
                            && link.identify_sent
                            && !link.ready =>
                    {
                        // The socket stays open: the next set_identify retries.
                        link.identify_sent = false;
                        self.emit(Event::IdentifyRejected);
                    }
                    _ => {}
                }
                self.emit(Event::Frame(op, frame.d));
                out
            }
            op => {
                self.emit(Event::Frame(op, frame.d));
                Vec::new()
            }
        }
    }

    fn hello_reply(&mut self, link: &mut Link) -> String {
        if self.config.reconnect == Reconnect::Resume {
            if let Some(session_id) = &self.session {
                link.resuming = true;
                let resume = Resume {
                    session_id: session_id.clone(),
                    seq_ack: self.last_seq,
                };
                if let Ok(frame) = Frame::with_payload(GatewayOp::Resume, &resume) {
                    return frame.encode();
                }
            }
        }
        link.identify_sent = true;
        self.config.identify.to_string()
    }

    fn mark_ready(&mut self, link: &mut Link) -> Vec<String> {
        link.ready = true;
        self.retries = 0;
        self.queue.drain(..).collect()
    }

    fn forget_session(&mut self) {
        self.session = None;
        self.last_seq = 0;
    }

    fn emit(&self, event: Event) {
        let _ = self.events.send(event);
    }

    /// Uniform in `[0, 1)` (xorshift64*; jitter only, not security).
    fn random(&mut self) -> f64 {
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        let bits = x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 11;
        bits as f64 / (1u64 << 53) as f64
    }
}

fn closed(reason: String) -> Ended {
    Ended::Closed { code: None, reason }
}

async fn send_close(writer: &mut Writer, code: CloseCode, reason: &str) {
    let frame = CloseFrame {
        code,
        reason: reason.to_owned().into(),
    };
    let _ = timeout(CLOSE_TIMEOUT, writer.send(Message::Close(Some(frame)))).await;
}

async fn sleep_until_some(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The URL without scheme, host and query, for log lines.
fn url_path(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let path = rest.find('/').map_or("/", |i| &rest[i..]);
    path.split('?').next().unwrap_or(path)
}

async fn tick(heartbeat: &mut Option<Interval>) {
    match heartbeat {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending().await,
    }
}

fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64);
    // Never zero (xorshift would stay at zero).
    (nanos ^ 0x9E37_79B9_7F4A_7C15) | 1
}
