//! One ticketed realtime WebSocket (room or voice): Hello → Identify →
//! heartbeats, frames out as `(op, d)`. No reconnect: the voice session
//! owner decides what a drop means (4008/4010 mean stop).

use std::time::Duration;

use anyhow::Result;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::time::{interval_at, Instant, Interval};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;

use yap_protocol::{GatewayOp, HEARTBEAT_FRAME};

use super::config;

pub enum Frame {
    /// `{op, d}` from the server (Hello and HeartbeatACK are handled here,
    /// not forwarded; frames with an opcode this build does not know are dropped).
    Op(GatewayOp, Value),
    /// The socket ended; the close code if the server sent one.
    Closed(Option<u16>),
}

/// Opens `path` (e.g. `/api/channels/<slug>/voice?v=1`) with `ticket`, sends
/// `identify` on Hello, and returns a sender for outgoing JSON text.
pub async fn open(
    path: &str,
    ticket: &str,
    identify: Value,
    frames: UnboundedSender<Frame>,
) -> Result<UnboundedSender<String>> {
    let cfg = config::get();
    let base = cfg
        .api_base
        .replacen("https://", "wss://", 1)
        .replacen("http://", "ws://", 1);
    let mut request = format!("{base}{path}").into_client_request()?;
    let headers = request.headers_mut();
    headers.insert("Origin", HeaderValue::from_str(&cfg.gateway_origin)?);
    headers.insert(
        "Sec-WebSocket-Protocol",
        HeaderValue::from_str(&format!("ralph.realtime.v1, ralph.ticket.{ticket}"))?,
    );
    let (socket, _) = tokio_tungstenite::connect_async(request).await?;
    let (out_tx, out_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(pump(socket, identify, frames, out_rx));
    Ok(out_tx)
}

async fn tick(heartbeat: &mut Option<Interval>) {
    match heartbeat {
        Some(interval) => {
            interval.tick().await;
        }
        None => std::future::pending().await,
    }
}

async fn pump(
    socket: tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    identify: Value,
    frames: UnboundedSender<Frame>,
    mut outgoing: UnboundedReceiver<String>,
) {
    let (mut writer, mut reader) = socket.split();
    let mut heartbeat: Option<Interval> = None;
    let mut identify = Some(identify);
    let close_code = loop {
        tokio::select! {
            frame = reader.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    let Ok(message) = serde_json::from_str::<Value>(&text) else { continue };
                    let Some(op) = message
                        .get("op")
                        .and_then(Value::as_i64)
                        .and_then(|op| GatewayOp::try_from(op).ok())
                    else {
                        continue;
                    };
                    let data = message.get("d").cloned().unwrap_or(Value::Null);
                    if op == GatewayOp::Hello {
                        let ms = data.get("heartbeat_interval").and_then(Value::as_u64).unwrap_or(15_000);
                        let period = Duration::from_millis(ms);
                        heartbeat = Some(interval_at(Instant::now() + period, period));
                        if let Some(identify) = identify.take() {
                            if writer.send(Message::text(identify.to_string())).await.is_err() {
                                break None;
                            }
                        }
                    } else if op != GatewayOp::HeartbeatACK && frames.send(Frame::Op(op, data)).is_err() {
                        break None;
                    }
                }
                Some(Ok(Message::Close(close))) => break close.map(|c| u16::from(c.code)),
                Some(Ok(_)) => {}
                Some(Err(_)) | None => break None,
            },
            out = outgoing.recv() => match out {
                Some(text) => {
                    if writer.send(Message::text(text)).await.is_err() {
                        break None;
                    }
                }
                None => {
                    let _ = writer
                        .send(Message::Close(Some(tokio_tungstenite::tungstenite::protocol::CloseFrame {
                            code: CloseCode::Normal,
                            reason: "leave".into(),
                        })))
                        .await;
                    break Some(1000);
                }
            },
            _ = tick(&mut heartbeat) => {
                if writer.send(Message::text(HEARTBEAT_FRAME)).await.is_err() {
                    break None;
                }
            }
        }
    };
    let _ = frames.send(Frame::Closed(close_code));
}
