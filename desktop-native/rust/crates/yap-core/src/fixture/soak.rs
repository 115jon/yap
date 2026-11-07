//! The soak driver (`YAP_FIXTURE=soak`): replays gateway traffic into the
//! core at speed so a long run shows what memory the client keeps.
//!
//! `YAP_FIXTURE_SOAK=<events a second>` (default 8) sets the rate and
//! `YAP_FIXTURE_SOAK_SWITCH=<seconds>` (default 30, 0 never) how often it
//! opens the next text channel. Each tick is one of: a new message (with a
//! media server, some carry a GIF, a picture or an image embed, each at a
//! URL of its own), an edit, a delete, a reaction added or removed, typing,
//! a presence change, or a new voice roster for the lounge. Deterministic,
//! so runs compare.

use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;
use yap_protocol::payload::{
    MessageDeleteEvent, PresenceUpdateEvent, ReactionEvent, TypingStartEvent, VoiceChannelMember,
    VoiceChannelStateUpdate,
};
use yap_protocol::rest::{Attachment, Embed, EmbedImage, Message, MessagePatch};

use super::{blank_message, media_base, sentence, user, Rng, AUTHORS, SOAK_TEXT, VOICE};
use crate::store::time::format_rfc3339;
use crate::store::{Command, Dispatch, GatewayEvent, Input, SessionId};

const EMOJI: &[&str] = &["👍", "❤️", "😂", "🔥", "🎉", "👀"];
const STATUS: &[&str] = &["online", "idle", "dnd", "offline"];

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn pick<'a>(rng: &mut Rng, list: &[&'a str]) -> &'a str {
    list[rng.below(list.len() as u64) as usize]
}

/// A new message in `channel_id`; with a media server, one in ten carries a
/// GIF, one a picture and one an image embed.
fn new_message(rng: &mut Rng, serial: u64, channel_id: &str, base: Option<&str>) -> Message {
    let (author_id, username, display) = AUTHORS[rng.below(AUTHORS.len() as u64) as usize];
    let mut message = Message {
        id: format!("msg_soak_{serial}"),
        channel_id: channel_id.into(),
        author_id: author_id.into(),
        author: Some(user(author_id, username, display)),
        content: sentence(rng),
        created_at: format_rfc3339(now_ms()),
        ..blank_message()
    };
    let Some(base) = base else {
        return message;
    };
    match rng.below(10) {
        0 => message.attachments.push(Attachment {
            id: Some(format!("att_soak_{serial}")),
            filename: format!("soak-{serial}.gif"),
            content_type: Some("image/gif".into()),
            url: Some(format!("{base}/gif/{}.gif?n={serial}", serial % 6)),
            size_bytes: Some(40_000),
            ..Attachment::default()
        }),
        1 => message.attachments.push(Attachment {
            id: Some(format!("att_soak_{serial}")),
            filename: format!("soak-{serial}.png"),
            content_type: Some("image/png".into()),
            url: Some(format!("{base}/img/{}.png?n={serial}", serial % 12)),
            size_bytes: Some(200_000),
            ..Attachment::default()
        }),
        2 => {
            let url = format!("{base}/img/{}.png?e={serial}", serial % 12);
            message.content = format!("{} {url}", message.content);
            message.embeds.push(Embed {
                id: format!("emb_soak_{serial}"),
                url: url.clone(),
                kind: "image".into(),
                direct: true,
                thumbnail: Some(EmbedImage {
                    url,
                    width: Some(1280.0),
                    height: Some(720.0),
                }),
                ..Embed::default()
            });
        }
        _ => {}
    }
    message
}

/// A new roster for the lounge: a random half of the authors.
fn roster(rng: &mut Rng) -> VoiceChannelStateUpdate {
    let mut members = Vec::new();
    for &(user_id, username, display) in AUTHORS.iter().skip(1) {
        if rng.below(2) == 0 {
            continue;
        }
        members.push(VoiceChannelMember {
            user_id: user_id.into(),
            name: display.map(Into::into),
            username: Some(username.into()),
            display_name: display.map(Into::into),
            avatar_url: user(user_id, username, display).avatar_url,
            self_mute: Some(rng.below(2) == 0),
            self_deaf: Some(false),
            self_video: Some(false),
            self_stream: Some(false),
            joined_at: None,
            connection_state: None,
            connected: Some(true),
        });
    }
    VoiceChannelStateUpdate {
        channel_id: VOICE.into(),
        members,
        started_at: None,
    }
}

/// Runs until the inbox closes (or the task is aborted on a reconnect).
pub async fn run(inbox: UnboundedSender<Input>, session: SessionId) {
    let rate = env_u64("YAP_FIXTURE_SOAK", 8).clamp(1, 1000);
    let switch_every = env_u64("YAP_FIXTURE_SOAK_SWITCH", 30);
    let base = media_base();
    eprintln!("[soak] {rate} events/s, channel switch every {switch_every} s");
    let mut rng = Rng(0xD1B5_4A32_D192_ED03);
    let mut tick = tokio::time::interval(Duration::from_millis(1000 / rate));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Ticks since launch; ids and media URLs come from it, so no two match.
    let mut serial: u64 = 0;
    let mut channel = 0usize;
    // The soak's own messages in the open channel, oldest first.
    let mut live: Vec<String> = Vec::new();
    let mut last_switch = tokio::time::Instant::now();
    loop {
        tick.tick().await;
        if switch_every > 0 && last_switch.elapsed() >= Duration::from_secs(switch_every) {
            last_switch = tokio::time::Instant::now();
            channel = (channel + 1) % SOAK_TEXT.len();
            live.clear();
            let id = SOAK_TEXT[channel].0.to_owned();
            if inbox.send(Input::User(Command::SelectChannel(id))).is_err() {
                return;
            }
            continue;
        }
        serial += 1;
        let channel_id = SOAK_TEXT[channel].0;
        let (author_id, _, _) = AUTHORS[rng.below(AUTHORS.len() as u64) as usize];
        let roll = rng.below(100);
        let event = match roll {
            30..=39 if !live.is_empty() => {
                let id = live[rng.below(live.len() as u64) as usize].clone();
                Dispatch::MessagePatch(MessagePatch {
                    id,
                    channel_id: Some(channel_id.into()),
                    content: Some(sentence(&mut rng)),
                    updated_at: Some(format_rfc3339(now_ms())),
                    embeds: None,
                    content_revision: None,
                })
            }
            // Deletes keep the open channel near its loaded size.
            40..=47 if live.len() > 40 => Dispatch::MessageDelete(MessageDeleteEvent {
                id: live.remove(0),
                channel_id: Some(channel_id.into()),
            }),
            48..=62 if !live.is_empty() => {
                let back = rng.below(live.len().min(8) as u64) as usize;
                let event = ReactionEvent {
                    message_id: live[live.len() - 1 - back].clone(),
                    channel_id: Some(channel_id.into()),
                    user_id: author_id.into(),
                    emoji: pick(&mut rng, EMOJI).into(),
                };
                if roll < 56 {
                    Dispatch::ReactionAdd(event)
                } else {
                    Dispatch::ReactionRemove(event)
                }
            }
            63..=77 => Dispatch::TypingStart(TypingStartEvent {
                channel_id: channel_id.into(),
                user_id: author_id.into(),
            }),
            78..=92 => Dispatch::PresenceUpdate(PresenceUpdateEvent {
                user_id: author_id.into(),
                status: pick(&mut rng, STATUS).into(),
                custom_status: (rng.below(4) == 0).then(|| Some(format!("soak {serial}"))),
                platforms: None,
            }),
            93..=99 => Dispatch::VoiceChannelState(roster(&mut rng)),
            // New messages: 0..=29, and the rolls above whose condition failed.
            _ => {
                let message = new_message(&mut rng, serial, channel_id, base.as_deref());
                live.push(message.id.clone());
                Dispatch::MessageCreate(Box::new(message))
            }
        };
        let input = Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(event),
        };
        if inbox.send(input).is_err() {
            return;
        }
    }
}
