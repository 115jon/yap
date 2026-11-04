//! The global gateway: `wss://{api}/api/gateway`, the MeetingRoom DO.
//!
//! Hello (op 8) → Identify (op 0) → Ready (op 2); heartbeats are exactly
//! `{"op":3}` (the DO answers them with a WebSocket auto-response); events
//! arrive as Dispatch (op 19, `{event, data, seq}`).

use std::time::Duration;

use anyhow::{anyhow, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::time::{interval_at, Instant, Interval};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;

use super::api::{self, Api};
use super::{config, Command, Event};

const HEARTBEAT: &str = r#"{"op":3}"#;

/// Runs until the task is aborted, reconnecting with backoff.
pub async fn run(
    api: Api,
    user_id: String,
    mut outgoing: UnboundedReceiver<String>,
    events: UnboundedSender<Command>,
) {
    let mut attempt: u32 = 0;
    loop {
        let mut reached_ready = false;
        let result = session(&api, &user_id, &mut outgoing, &events, &mut reached_ready).await;
        let _ = events.send(Command::Event(Event::GatewayClosed));
        // Only a session that got to Ready resets the backoff; a server that
        // accepts and then closes must not be retried every second.
        if reached_ready {
            attempt = 0;
        }
        match result {
            Ok(true) => return,
            Ok(false) => {}
            Err(error) => {
                if api::is_unauthorized(&error) {
                    let _ = events.send(Command::Event(Event::SessionExpired));
                    return;
                }
                eprintln!("[gateway] {error:#}");
            }
        }
        let backoff = Duration::from_millis((1000u64 << attempt.min(5)).min(30_000));
        attempt += 1;
        tokio::time::sleep(backoff).await;
    }
}

async fn tick(heartbeat: &mut Option<Interval>) {
    match heartbeat {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending().await,
    }
}

/// One socket. `Ok(true)` means stop for good (the core went away).
async fn session(
    api: &Api,
    user_id: &str,
    outgoing: &mut UnboundedReceiver<String>,
    events: &UnboundedSender<Command>,
    reached_ready: &mut bool,
) -> Result<bool> {
    let cfg = config::get();
    let ticket = api.socket_ticket("global").await?;
    let ws_base = cfg
        .api_base
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut request = format!("{ws_base}/api/gateway").into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Origin", HeaderValue::from_str(&cfg.gateway_origin)?);
    headers.insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_str(&format!("ralph.realtime.v1, ralph.ticket.{ticket}"))?,
    );

    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    let (mut writer, mut reader) = socket.split();
    let mut heartbeat: Option<Interval> = None;

    loop {
        tokio::select! {
            frame = reader.next() => {
                let text = match frame {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None => return Ok(false),
                    Some(Ok(_)) => continue,
                    Some(Err(error)) => return Err(anyhow!(error)),
                };
                let Ok(message) = serde_json::from_str::<Value>(&text) else { continue };
                let data = message.get("d").cloned().unwrap_or(Value::Null);
                match message.get("op").and_then(Value::as_i64) {
                    Some(8) => {
                        let ms = data.get("heartbeat_interval").and_then(Value::as_u64).unwrap_or(45_000);
                        let period = Duration::from_millis(ms);
                        heartbeat = Some(interval_at(Instant::now() + period, period));
                        let identify = json!({
                            "op": 0,
                            "d": {
                                "name": "ChatClient",
                                "clerk_user_id": user_id,
                                "platform": "desktop",
                                "supports_voice_state_deltas": true,
                            }
                        });
                        writer.send(Message::text(identify.to_string())).await?;
                    }
                    Some(2) | Some(9) => {
                        *reached_ready = true;
                        eprintln!("[startup] gateway ready after {} ms", super::started_at().elapsed().as_millis());
                        let _ = events.send(Command::Event(Event::GatewayReady));
                    }
                    Some(19) => {
                        let event = data.get("event").and_then(Value::as_str).unwrap_or_default().to_owned();
                        let payload = data.get("data").cloned().unwrap_or(Value::Null);
                        let _ = events.send(Command::Event(Event::Dispatch(event, payload)));
                    }
                    Some(18) => eprintln!("[gateway] error frame: {data}"),
                    _ => {}
                }
            }
            out = outgoing.recv() => match out {
                Some(text) => writer.send(Message::text(text)).await?,
                None => {
                    let _ = writer.close().await;
                    return Ok(true);
                }
            },
            _ = tick(&mut heartbeat) => {
                writer.send(Message::text(HEARTBEAT)).await?;
            }
        }
    }
}
