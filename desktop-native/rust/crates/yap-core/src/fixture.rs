//! Offline fixture mode for perf work: `YAP_FIXTURE=chat-10k` answers every
//! request locally with one server, one text channel of 10,000 synthetic
//! messages (varied lengths, authors and grouping) and one voice channel.
//! The keyring, the gateway and voice are never touched. Off unless the
//! variable is set.

use yap_protocol::rest::{
    Attachment, Category, Channel, ChannelType, ChannelsResponse, GifPage, Member, Message,
    MessagePatch, Reaction, ReadStates, Server, UploadedFile, User,
};

use crate::store::time::{format_rfc3339, parse_rfc3339};
use crate::store::{ApiRequest, ApiResponse, Page};

pub const SERVER: &str = "srv_fixture";
pub const TEXT: &str = "chan_fixture_general";
pub const VOICE: &str = "chan_fixture_lounge";
pub const ME: &str = "user_fixture_me";
const START: &str = "2026-01-05T09:00:00.000Z";

/// The fixtures the runtime knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fixture {
    Chat10k,
}

impl Fixture {
    /// The fixture named by `YAP_FIXTURE`, if any.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("YAP_FIXTURE").ok()?;
        match name.trim() {
            "" => None,
            "chat-10k" => Some(Self::Chat10k),
            other => {
                eprintln!("[fixture] unknown YAP_FIXTURE={other}; using the network");
                None
            }
        }
    }

    pub fn message_count(self) -> usize {
        match self {
            Self::Chat10k => 10_000,
        }
    }

    /// The local answer to a request. `now_ms` stamps sent messages.
    pub fn respond(self, request: &ApiRequest, now_ms: i64, serial: u64) -> ApiResponse {
        match request {
            ApiRequest::Me => ApiResponse::Me(Box::new(user(ME, "me", Some("Fixture Me")))),
            ApiRequest::Servers => ApiResponse::Servers(vec![Server {
                id: SERVER.into(),
                name: "Fixture Server".into(),
                owner_id: Some(ME.into()),
                icon_url: None,
                created_at: None,
            }]),
            ApiRequest::Channels { .. } => ApiResponse::Channels(channels()),
            // All of it on the first page, so a scroll test has the whole
            // channel at once.
            ApiRequest::Messages {
                channel_id,
                before: None,
            } if channel_id == TEXT => ApiResponse::Messages(Page {
                messages: messages(self.message_count()),
                has_more_before: false,
            }),
            ApiRequest::Messages { .. } => ApiResponse::Messages(Page {
                messages: Vec::new(),
                has_more_before: false,
            }),
            ApiRequest::SendMessage {
                channel_id,
                content,
                reply_to_id,
                nonce,
                ..
            } => ApiResponse::Sent(Some(Box::new(Message {
                id: format!("msg_fixture_sent_{serial}"),
                channel_id: channel_id.clone(),
                author_id: ME.into(),
                author: Some(user(ME, "me", Some("Fixture Me"))),
                content: content.clone(),
                reply_to_id: reply_to_id.clone(),
                nonce: Some(nonce.clone()),
                created_at: format_rfc3339(now_ms),
                ..blank_message()
            }))),
            ApiRequest::EditMessage {
                channel_id,
                message_id,
                content,
            } => ApiResponse::Edited(Some(MessagePatch {
                id: message_id.clone(),
                channel_id: Some(channel_id.clone()),
                content: Some(content.clone()),
                updated_at: Some(format_rfc3339(now_ms)),
                embeds: None,
                content_revision: None,
            })),
            ApiRequest::ReadStates => ApiResponse::ReadStates(ReadStates::default()),
            ApiRequest::Notifications => ApiResponse::Notifications(Vec::new()),
            ApiRequest::Members { .. } => ApiResponse::Members(
                AUTHORS
                    .iter()
                    .map(|&(id, username, display)| Member {
                        user: user(id, username, display),
                        roles: Vec::new(),
                    })
                    .collect(),
            ),
            ApiRequest::Dms => ApiResponse::Dms(Vec::new()),
            ApiRequest::DeleteMessage { .. }
            | ApiRequest::Typing { .. }
            | ApiRequest::MarkRead { .. }
            | ApiRequest::MarkNotificationsRead { .. }
            | ApiRequest::SetPresence { .. }
            | ApiRequest::OpenDm { .. }
            | ApiRequest::Profile { .. }
            | ApiRequest::React { .. }
            | ApiRequest::GifShare { .. } => ApiResponse::Done,
            ApiRequest::Gifs { .. } => ApiResponse::Gifs(GifPage::default()),
            ApiRequest::GifUpload { source_url, .. } => ApiResponse::Uploaded(UploadedFile {
                id: format!("att_fixture_gif_{serial}"),
                file_url: source_url.clone(),
                file_name: "gif.gif".into(),
                file_size: 0,
                content_type: Some("image/gif".into()),
            }),
        }
    }
}

fn user(id: &str, username: &str, display_name: Option<&str>) -> User {
    User {
        id: id.into(),
        username: username.into(),
        display_name: display_name.map(Into::into),
        avatar_url: None,
        avatar_display: None,
        status: None,
        custom_status: None,
        ..User::default()
    }
}

fn channels() -> ChannelsResponse {
    let channel = |id: &str, name: &str, kind, category: &str| Channel {
        id: id.into(),
        server_id: Some(SERVER.into()),
        name: name.into(),
        description: None,
        channel_type: kind,
        category_id: Some(category.into()),
        position: 0,
        created_at: None,
        permissions: None,
    };
    let category = |id: &str, name: &str, rank| Category {
        id: id.into(),
        server_id: Some(SERVER.into()),
        name: name.into(),
        rank,
    };
    ChannelsResponse {
        channels: vec![
            channel(TEXT, "general", ChannelType::Text, "cat_fixture_text"),
            channel(VOICE, "lounge", ChannelType::Voice, "cat_fixture_voice"),
        ],
        categories: vec![
            category("cat_fixture_text", "Text", 0),
            category("cat_fixture_voice", "Voice", 1),
        ],
    }
}

fn blank_message() -> Message {
    Message {
        id: String::new(),
        channel_id: String::new(),
        author_id: String::new(),
        author: None,
        content: String::new(),
        reply_to_id: None,
        reply_to: None,
        is_pinned: false,
        created_at: String::new(),
        updated_at: None,
        attachments: Vec::new(),
        reactions: Vec::new(),
        embeds: Vec::new(),
        nonce: None,
        content_revision: None,
        kind: None,
    }
}

/// A small deterministic generator (xorshift64*), so runs are comparable.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const WORDS: &[&str] = &[
    "the",
    "build",
    "is",
    "green",
    "again",
    "ship",
    "it",
    "voice",
    "channel",
    "latency",
    "looks",
    "fine",
    "on",
    "my",
    "end",
    "can",
    "you",
    "check",
    "**this**",
    "`cargo test`",
    "tonight",
    "rebase",
    "main",
    "after",
    "lunch",
    "*maybe*",
    "QML",
    "delegate",
    "scroll",
    "frame",
    "budget",
    "works",
    "for",
    "me",
    "ok",
    "https://yap.115jon.com",
    "nice",
    "lol",
    "brb",
    "merging",
];

const AUTHORS: &[(&str, &str, Option<&str>)] = &[
    (ME, "me", Some("Fixture Me")),
    ("user_fixture_alice", "alice", Some("Alice")),
    ("user_fixture_bob", "bob", None),
    ("user_fixture_cara", "cara", Some("Cara Lindqvist")),
    ("user_fixture_dev", "dev", Some("dev bot")),
    ("user_fixture_eli", "eli", Some("Eli")),
];

fn sentence(rng: &mut Rng) -> String {
    // Mostly short lines, some paragraphs, a few walls of text.
    let words = match rng.below(20) {
        0 => 60 + rng.below(140),
        1..=4 => 15 + rng.below(30),
        _ => 1 + rng.below(12),
    };
    let mut text = String::new();
    for i in 0..words {
        if i > 0 {
            text.push(if rng.below(25) == 0 { '\n' } else { ' ' });
        }
        text.push_str(WORDS[rng.below(WORDS.len() as u64) as usize]);
    }
    text
}

/// `count` messages, oldest first, in bursts by one author (grouped) with
/// longer gaps between bursts.
pub fn messages(count: usize) -> Vec<Message> {
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let mut at = parse_rfc3339(START).unwrap_or_default();
    let mut author = 0;
    let mut out = Vec::with_capacity(count);
    for n in 0..count {
        if rng.below(3) == 0 {
            author = rng.below(AUTHORS.len() as u64) as usize;
            at += 60_000 * (5 + rng.below(240)) as i64;
        } else {
            at += 1000 * (2 + rng.below(90)) as i64;
        }
        let (id, username, display) = AUTHORS[author];
        let attachments = if rng.below(40) == 0 {
            // The app icon, so a scroll test lays out real pictures offline.
            vec![Attachment {
                id: Some(format!("att_fixture_{n}")),
                filename: format!("capture-{n}.png"),
                content_type: Some("image/png".into()),
                url: Some("qrc:/app/yap-256.png".into()),
                size_bytes: Some(1024),
                ..Attachment::default()
            }]
        } else {
            Vec::new()
        };
        out.push(Message {
            id: format!("msg_fixture_{n:05}"),
            channel_id: TEXT.into(),
            author_id: id.into(),
            author: Some(user(id, username, display)),
            content: sentence(&mut rng),
            created_at: format_rfc3339(at),
            updated_at: (rng.below(30) == 0).then(|| format_rfc3339(at + 30_000)),
            attachments,
            reactions: if rng.below(12) == 0 {
                vec![Reaction {
                    emoji: ["👍", "❤️", "😂", "🔥"][rng.below(4) as usize].into(),
                    count: 2,
                    me: false,
                    users: vec![ME.into(), "user_fixture_alice".into()],
                }]
            } else {
                Vec::new()
            },
            ..blank_message()
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_10k_should_have_ten_thousand_messages_in_order() {
        let list = messages(Fixture::Chat10k.message_count());
        let ordered = list
            .windows(2)
            .all(|w| parse_rfc3339(&w[0].created_at) < parse_rfc3339(&w[1].created_at));
        assert!(list.len() == 10_000 && ordered);
    }
}
