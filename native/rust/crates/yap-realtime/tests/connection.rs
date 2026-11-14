//! `Connection` against an in-process WebSocket server on 127.0.0.1.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::timeout;
use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use yap_protocol::GatewayOp;
use yap_realtime::{
    Backoff, Config, Connection, Event, Policy, Reconnect, StopReason, TicketError, TicketFuture,
};

const WAIT: Duration = Duration::from_secs(5);
const ORIGIN: &str = "https://yap.test";

type Ws = WebSocketStream<TcpStream>;

/// Fast retries, no jitter: 10, 20, 40, 80 ms, then 80.
const FAST: Policy = Policy {
    backoff: Backoff {
        base: Duration::from_millis(10),
        max: Duration::from_millis(80),
        scale_jitter: 0.0,
        add_jitter: Duration::ZERO,
    },
    max_missed_acks: 3,
    refusals_to_fail: 2,
};

fn identify() -> Value {
    json!({ "op": 0, "d": { "name": "ChatClient", "platform": "desktop" } })
}

fn voice_identify(token: &str) -> Value {
    json!({ "op": 100, "d": { "participant_id": "p1", "voice_token": token } })
}

struct Server {
    listener: TcpListener,
    url: String,
}

/// One accepted socket with the headers it came with.
struct Accepted {
    ws: Ws,
    protocols: String,
    origin: String,
}

impl Server {
    async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/api/gateway", listener.local_addr().unwrap());
        Self { listener, url }
    }

    fn config(&self, reconnect: Reconnect) -> Config {
        let (identify, ready_op) = match reconnect {
            Reconnect::Resume => (identify(), GatewayOp::Ready),
            Reconnect::Reidentify => (voice_identify("v1"), GatewayOp::VoiceReady),
        };
        Config {
            label: "test",
            url: self.url.clone(),
            origin: ORIGIN.into(),
            identify,
            reconnect,
            ready_op,
            policy: FAST,
            debug_drop_after: None,
        }
    }

    async fn accept(&self) -> Accepted {
        timeout(WAIT, self.try_accept())
            .await
            .expect("no connection")
    }

    // The handshake callback signature is tungstenite's.
    #[expect(clippy::result_large_err)]
    async fn try_accept(&self) -> Accepted {
        let (stream, _) = self.listener.accept().await.unwrap();
        let seen = Arc::new(Mutex::new((String::new(), String::new())));
        let headers = Arc::clone(&seen);
        let ws =
            tokio_tungstenite::accept_hdr_async(stream, move |req: &Request, mut res: Response| {
                let get = |name: &str| {
                    req.headers()
                        .get(name)
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or_default()
                        .to_owned()
                };
                *headers.lock().unwrap() = (get("Sec-WebSocket-Protocol"), get("Origin"));
                res.headers_mut().insert(
                    "Sec-WebSocket-Protocol",
                    HeaderValue::from_static("ralph.realtime.v1"),
                );
                Ok(res)
            })
            .await
            .unwrap();
        let (protocols, origin) = seen.lock().unwrap().clone();
        Accepted {
            ws,
            protocols,
            origin,
        }
    }

    /// No connection arrives within `ms`.
    async fn quiet_for(&self, ms: u64) -> bool {
        timeout(Duration::from_millis(ms), self.listener.accept())
            .await
            .is_err()
    }
}

/// `t1`, `t2`, ... one per call.
fn counting_tickets() -> (
    impl FnMut() -> TicketFuture + Send + 'static,
    Arc<AtomicU32>,
) {
    let count = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&count);
    let source = move || -> TicketFuture {
        let n = counter.fetch_add(1, Ordering::SeqCst) + 1;
        Box::pin(async move { Ok(format!("t{n}")) })
    };
    (source, count)
}

fn spawn(server: &Server, reconnect: Reconnect) -> (Connection, UnboundedReceiver<Event>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let (tickets, _) = counting_tickets();
    (Connection::spawn(server.config(reconnect), tickets, tx), rx)
}

async fn send(ws: &mut Ws, frame: Value) {
    ws.send(Message::text(frame.to_string())).await.unwrap();
}

async fn hello(ws: &mut Ws, interval_ms: u64) {
    send(
        ws,
        json!({ "op": 8, "d": { "heartbeat_interval": interval_ms } }),
    )
    .await;
}

/// The next text frame from the client, as sent.
async fn text(ws: &mut Ws) -> String {
    let next = async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Text(text))) => return text.to_string(),
                Some(Ok(Message::Close(close))) => panic!("client closed: {close:?}"),
                Some(Ok(_)) => {}
                other => panic!("socket ended: {other:?}"),
            }
        }
    };
    timeout(WAIT, next).await.expect("no frame")
}

async fn json_frame(ws: &mut Ws) -> Value {
    serde_json::from_str(&text(ws).await).unwrap()
}

/// The next event that matches, skipping the rest.
async fn event(rx: &mut UnboundedReceiver<Event>, wanted: impl Fn(&Event) -> bool) -> Event {
    let next = async {
        loop {
            let event = rx.recv().await.expect("connection task ended");
            if wanted(&event) {
                return event;
            }
        }
    };
    timeout(WAIT, next).await.expect("no such event")
}

/// Hello, Identify, Ready with session `s1`.
async fn handshake(ws: &mut Ws) {
    hello(ws, 60_000).await;
    assert_eq!(json_frame(ws).await, identify());
    send(ws, json!({ "op": 2, "d": { "participant_id": "s1" } })).await;
}

#[tokio::test]
async fn hello_should_be_answered_with_identify_and_ready_reported() {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Resume);
    let mut socket = server.accept().await;
    assert_eq!(socket.protocols, "ralph.realtime.v1, ralph.ticket.t1");
    assert_eq!(socket.origin, ORIGIN);
    handshake(&mut socket.ws).await;
    assert_eq!(rx.recv().await, Some(Event::Connected));
    assert_eq!(
        event(&mut rx, |e| matches!(e, Event::Ready(_))).await,
        Event::Ready(json!({ "participant_id": "s1" }))
    );
}

#[tokio::test]
async fn heartbeat_should_be_exactly_op_3() {
    let server = Server::new().await;
    let (_conn, _rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 20).await;
    text(&mut ws).await; // identify
    assert_eq!(text(&mut ws).await, r#"{"op":3}"#);
}

#[tokio::test]
async fn acknowledged_heartbeats_should_keep_the_socket() {
    let server = Server::new().await;
    let (_conn, _rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 10).await;
    text(&mut ws).await;
    for _ in 0..8 {
        assert_eq!(text(&mut ws).await, r#"{"op":3}"#);
        send(&mut ws, json!({ "op": 6, "d": { "seq": 1 } })).await;
    }
    assert!(server.quiet_for(50).await);
}

#[tokio::test]
async fn three_missed_acks_should_abandon_and_reconnect() {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 20).await;
    text(&mut ws).await;
    let mut beats = 0;
    let close = loop {
        match timeout(WAIT, ws.next()).await.expect("no frame") {
            Some(Ok(Message::Text(t))) if t.as_str() == r#"{"op":3}"# => beats += 1,
            Some(Ok(Message::Close(close))) => break close,
            other => panic!("unexpected {other:?}"),
        }
    };
    assert_eq!(beats, 3);
    assert_eq!(close.map(|c| u16::from(c.code)), Some(4000));
    let again = server.accept().await;
    assert_eq!(again.protocols, "ralph.realtime.v1, ralph.ticket.t2");
    event(&mut rx, |e| {
        matches!(
            e,
            Event::Disconnected {
                code: Some(4000),
                ..
            }
        )
    })
    .await;
}

#[tokio::test]
async fn a_drop_after_ready_should_resume_with_the_last_seq() {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    let dispatch =
        |seq: u64| json!({ "op": 19, "d": { "event": "MESSAGE_CREATE", "data": {}, "seq": seq } });
    send(&mut ws, dispatch(5)).await;
    send(&mut ws, dispatch(7)).await;
    // ACK seqs are not the replay cursor.
    send(&mut ws, json!({ "op": 6, "d": { "seq": 99 } })).await;
    send(&mut ws, dispatch(6)).await;
    event(
        &mut rx,
        |e| matches!(e, Event::Frame(GatewayOp::Dispatch, d) if d["seq"] == 6),
    )
    .await;
    drop(ws);

    let mut again = server.accept().await;
    assert_eq!(again.protocols, "ralph.realtime.v1, ralph.ticket.t2");
    hello(&mut again.ws, 60_000).await;
    assert_eq!(
        json_frame(&mut again.ws).await,
        json!({ "op": 7, "d": { "session_id": "s1", "seq_ack": 7 } })
    );
    send(
        &mut again.ws,
        json!({ "op": 9, "d": { "voice_token": "v2" } }),
    )
    .await;
    assert_eq!(
        event(&mut rx, |e| matches!(
            e,
            Event::Resumed(_) | Event::Ready(_)
        ))
        .await,
        Event::Resumed(json!({ "voice_token": "v2" }))
    );
}

#[tokio::test]
async fn a_refused_resume_should_identify_on_the_same_socket() {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    drop(ws);

    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await["op"], 7);
    let error = json!({ "code": 4006, "message": "Session not found for resume" });
    send(&mut ws, json!({ "op": 18, "d": error })).await;
    assert_eq!(json_frame(&mut ws).await, identify());
    send(&mut ws, json!({ "op": 2, "d": { "participant_id": "s2" } })).await;
    event(
        &mut rx,
        |e| matches!(e, Event::Ready(d) if d["participant_id"] == "s2"),
    )
    .await;
}

#[tokio::test]
async fn close_4006_should_drop_the_session_and_identify_fresh() {
    let server = Server::new().await;
    let (_conn, _rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    let close = CloseFrame {
        code: CloseCode::from(4006),
        reason: "Resume failed".into(),
    };
    ws.close(Some(close)).await.unwrap();

    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, identify());
}

async fn assert_stops(code: u16, reason: &str, expected: StopReason) {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Resume);
    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    let close = CloseFrame {
        code: CloseCode::from(code),
        reason: reason.to_owned().into(),
    };
    ws.close(Some(close)).await.unwrap();
    assert_eq!(
        event(&mut rx, |e| matches!(e, Event::Stopped(_))).await,
        Event::Stopped(expected)
    );
    assert!(server.quiet_for(200).await, "reconnected after {code}");
}

#[tokio::test]
async fn close_4010_should_stop_for_good() {
    let reason = "Moved to another voice channel";
    let expected = StopReason::Replaced {
        code: 4010,
        reason: reason.into(),
    };
    assert_stops(4010, reason, expected).await;
}

#[tokio::test]
async fn close_4008_should_stop_for_good() {
    let expected = StopReason::AdmissionRejected {
        reason: "no".into(),
    };
    assert_stops(4008, "no", expected).await;
}

#[tokio::test]
async fn replaced_by_new_connection_should_stop_for_good() {
    let reason = "Replaced by new connection";
    let expected = StopReason::Replaced {
        code: 1000,
        reason: reason.into(),
    };
    assert_stops(1000, reason, expected).await;
}

#[tokio::test]
async fn sends_should_wait_for_ready_and_survive_a_reconnect() {
    let server = Server::new().await;
    let (conn, mut rx) = spawn(&server, Reconnect::Resume);
    conn.send("a");
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, identify());
    conn.send("b");
    // Let "b" reach the queue before Ready.
    tokio::time::sleep(Duration::from_millis(20)).await;
    send(&mut ws, json!({ "op": 2, "d": { "participant_id": "s1" } })).await;
    assert_eq!(text(&mut ws).await, "a");
    assert_eq!(text(&mut ws).await, "b");
    drop(ws);

    event(&mut rx, |e| matches!(e, Event::Disconnected { .. })).await;
    conn.send("c");
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await["op"], 7);
    send(&mut ws, json!({ "op": 9, "d": {} })).await;
    assert_eq!(text(&mut ws).await, "c");
}

#[tokio::test]
async fn backoff_should_double_and_reset_after_ready() {
    let server = Server::new().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let calls = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&calls);
    // Three failures, then tickets.
    let tickets = move || -> TicketFuture {
        let n = counter.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if n < 3 {
                Err(TicketError::Failed("down".into()))
            } else {
                Ok(format!("t{n}"))
            }
        })
    };
    let _conn = Connection::spawn(server.config(Reconnect::Resume), tickets, tx);
    let mut delays = Vec::new();
    for _ in 0..3 {
        if let Event::Reconnecting { attempt, delay } =
            event(&mut rx, |e| matches!(e, Event::Reconnecting { .. })).await
        {
            delays.push((attempt, delay.as_millis()));
        }
    }
    assert_eq!(delays, [(1, 10), (2, 20), (3, 40)]);

    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    event(&mut rx, |e| matches!(e, Event::Ready(_))).await;
    drop(ws);
    assert_eq!(
        event(&mut rx, |e| matches!(e, Event::Reconnecting { .. })).await,
        Event::Reconnecting {
            attempt: 1,
            delay: Duration::from_millis(10)
        }
    );
}

#[tokio::test]
async fn a_second_refused_ticket_should_stop_with_session_expired() {
    let server = Server::new().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let tickets = || -> TicketFuture { Box::pin(async { Err(TicketError::Unauthorized) }) };
    let _conn = Connection::spawn(server.config(Reconnect::Resume), tickets, tx);
    assert!(matches!(
        rx.recv().await,
        Some(Event::Reconnecting { attempt: 1, .. })
    ));
    assert_eq!(
        rx.recv().await,
        Some(Event::Stopped(StopReason::SessionExpired))
    );
}

#[tokio::test]
async fn voice_should_identify_again_after_a_drop() {
    let server = Server::new().await;
    let (_conn, mut rx) = spawn(&server, Reconnect::Reidentify);
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, voice_identify("v1"));
    send(
        &mut ws,
        json!({ "op": 101, "d": { "tracks": [], "seq": 3 } }),
    )
    .await;
    event(&mut rx, |e| matches!(e, Event::Ready(_))).await;
    drop(ws);

    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, voice_identify("v1"));
}

#[tokio::test]
async fn close_1013_should_identify_again_on_a_fresh_ticket() {
    let server = Server::new().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (tickets, count) = counting_tickets();
    let _conn = Connection::spawn(server.config(Reconnect::Reidentify), tickets, tx);
    let first = server.accept().await;
    assert!(first.protocols.contains("ralph.ticket.t1"));
    let mut ws = first.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, voice_identify("v1"));
    let close = CloseFrame {
        code: CloseCode::from(1013),
        reason: "Voice ticket is for another participant".into(),
    };
    ws.close(Some(close)).await.unwrap();
    event(&mut rx, |e| {
        matches!(
            e,
            Event::Disconnected {
                code: Some(1013),
                ..
            }
        )
    })
    .await;

    let again = server.accept().await;
    assert!(again.protocols.contains("ralph.ticket.t2"));
    let mut ws = again.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await, voice_identify("v1"));
    assert_eq!(count.load(Ordering::SeqCst), 2, "one fresh ticket");
}

#[tokio::test]
async fn a_refused_voice_token_should_ask_for_a_fresh_one() {
    let server = Server::new().await;
    let (conn, mut rx) = spawn(&server, Reconnect::Reidentify);
    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    text(&mut ws).await;
    let error = json!({ "code": 4004, "message": "Invalid voice token" });
    send(&mut ws, json!({ "op": 18, "d": error })).await;
    event(&mut rx, |e| *e == Event::IdentifyRejected).await;
    conn.set_identify(voice_identify("v2"));
    assert_eq!(json_frame(&mut ws).await, voice_identify("v2"));
}

#[tokio::test]
async fn the_debug_drop_should_fire_once_and_resume() {
    let server = Server::new().await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (tickets, _) = counting_tickets();
    let config = Config {
        debug_drop_after: Some(Duration::from_millis(30)),
        ..server.config(Reconnect::Resume)
    };
    let _conn = Connection::spawn(config, tickets, tx);
    let mut ws = server.accept().await.ws;
    handshake(&mut ws).await;
    // An abrupt drop: the stream ends without a close frame.
    let end = timeout(WAIT, ws.next()).await.expect("no drop");
    assert!(!matches!(end, Some(Ok(Message::Close(_)))), "{end:?}");
    event(&mut rx, |e| {
        matches!(e, Event::Disconnected { code: None, .. })
    })
    .await;

    let mut ws = server.accept().await.ws;
    hello(&mut ws, 60_000).await;
    assert_eq!(json_frame(&mut ws).await["op"], 7);
    send(&mut ws, json!({ "op": 9, "d": {} })).await;
    event(&mut rx, |e| matches!(e, Event::Resumed(_))).await;
    assert!(server.quiet_for(150).await, "dropped again");
}
