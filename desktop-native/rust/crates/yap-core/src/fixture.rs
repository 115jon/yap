//! Offline fixture mode for perf work: `YAP_FIXTURE=chat-10k` answers every
//! request locally with one server, one text channel of 10,000 synthetic
//! messages (varied lengths, authors and grouping) and one voice channel.
//! The keyring, the gateway and voice are never touched. Off unless the
//! variable is set.
//!
//! `YAP_FIXTURE=soak` is the long-run memory fixture (docs/PERFORMANCE.md,
//! pass 4): four text channels of 400 messages, and a driver ([`soak`])
//! that replays gateway traffic at speed (messages with GIFs, pictures and
//! embeds, edits, deletes, reactions, typing, presence, voice rosters) and
//! switches channels. `YAP_FIXTURE_MEDIA=<http base>` serves the avatars
//! and media from a local server (`tools/p4/media-server.mjs`).

pub mod soak;

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
    Soak,
}

impl Fixture {
    /// The fixture named by `YAP_FIXTURE`, if any.
    pub fn from_env() -> Option<Self> {
        let name = std::env::var("YAP_FIXTURE").ok()?;
        match name.trim() {
            "" => None,
            "chat-10k" => Some(Self::Chat10k),
            "soak" => Some(Self::Soak),
            other => {
                eprintln!("[fixture] unknown YAP_FIXTURE={other}; using the network");
                None
            }
        }
    }

    pub fn message_count(self) -> usize {
        match self {
            Self::Chat10k => 10_000,
            Self::Soak => 400,
        }
    }

    /// The text channels it serves, first one opened at launch.
    pub fn text_channels(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Chat10k => &[(TEXT, "general")],
            Self::Soak => SOAK_TEXT,
        }
    }

    /// The local answer to a request. `now_ms` stamps sent messages.
    pub fn respond(self, request: &ApiRequest, now_ms: i64, serial: u64) -> ApiResponse {
        match request {
            ApiRequest::Apps(_) => ApiResponse::Done,
            // No shop or lookups offline: they fail like a dead network.
            ApiRequest::Shop(_) | ApiRequest::Embeds(_) | ApiRequest::RemoveEmbeds { .. } => {
                ApiResponse::Done
            }
            ApiRequest::Favorites(_) => ApiResponse::Favorites(Vec::new()),
            ApiRequest::Backgrounds(_) => ApiResponse::Backgrounds(
                crate::store::backgrounds::BackgroundsResponse::List(Vec::new()),
            ),
            ApiRequest::Extras(_) | ApiRequest::VoiceMove(_) => ApiResponse::Done,
            ApiRequest::Me => ApiResponse::Me(Box::new(user(ME, "me", Some("Fixture Me")))),
            ApiRequest::Servers => ApiResponse::Servers(vec![Server {
                id: SERVER.into(),
                name: "Fixture Server".into(),
                owner_id: Some(ME.into()),
                // YAP_FIXTURE_ICON=<url>: a server icon (a GIF one animates).
                icon_url: std::env::var("YAP_FIXTURE_ICON").ok(),
                created_at: None,
                invites_paused: None,
                allow_public_shares: None,
                show_source_in_shares: None,
                allow_share_indexing: None,
            }]),
            ApiRequest::Channels { .. } => ApiResponse::Channels(channels(self)),
            // All of it on the first page, so a scroll test has the whole
            // channel at once.
            ApiRequest::Messages {
                channel_id,
                before: None,
            } if self.text_channels().iter().any(|(id, _)| id == channel_id) => {
                ApiResponse::Messages(Page {
                    messages: messages_in(self.message_count(), channel_id),
                    has_more_before: false,
                })
            }
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
            | ApiRequest::GifShare { .. }
            | ApiRequest::UpdateProfile(_)
            | ApiRequest::DeleteBanner
            | ApiRequest::RevokeShare { .. } => ApiResponse::Done,
            // No pins, no windows; a thread is empty.
            ApiRequest::Threads(request) => ApiResponse::Threads(
                request
                    .decode(serde_json::Value::Array(Vec::new()))
                    .unwrap_or(crate::store::ThreadResponse::Done),
            ),
            // No servers to create or join here: the dialogs show the error.
            ApiRequest::Invites(_) => ApiResponse::Done,
            // Empty lists; every action succeeds.
            ApiRequest::Admin(request) => ApiResponse::Admin(
                request
                    .decode(serde_json::Value::Null)
                    .unwrap_or(crate::store::AdminResponse::Done),
            ),
            ApiRequest::Search(_) => ApiResponse::Search(Box::default()),
            ApiRequest::Friends(request) => ApiResponse::Friends(
                request
                    .decode(serde_json::Value::Null)
                    .unwrap_or(crate::store::FriendsResponse::Done),
            ),
            ApiRequest::Sessions => ApiResponse::Sessions(Vec::new()),
            ApiRequest::Shares => ApiResponse::Shares(Vec::new()),
            ApiRequest::KovaSession => ApiResponse::KovaEmail(None),
            ApiRequest::KovaAccounts => ApiResponse::KovaAccounts(Vec::new()),
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

/// `YAP_FIXTURE_MEDIA`: the local media server's base URL, if one is set.
pub(crate) fn media_base() -> Option<String> {
    std::env::var("YAP_FIXTURE_MEDIA")
        .ok()
        .map(|base| base.trim_end_matches('/').to_owned())
        .filter(|base| !base.is_empty())
}

pub(crate) fn user(id: &str, username: &str, display_name: Option<&str>) -> User {
    User {
        id: id.into(),
        username: username.into(),
        display_name: display_name.map(Into::into),
        // With a media server, everyone but "me" has a picture.
        avatar_url: media_base()
            .filter(|_| id != ME)
            .map(|base| format!("{base}/av/{username}.png")),
        avatar_display: None,
        status: None,
        custom_status: None,
        ..User::default()
    }
}

/// The soak fixture's text channels.
pub const SOAK_TEXT: &[(&str, &str)] = &[
    (TEXT, "general"),
    ("chan_fixture_media", "media"),
    ("chan_fixture_dev", "dev"),
    ("chan_fixture_random", "random"),
];

fn channels(fixture: Fixture) -> ChannelsResponse {
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
        allow_public_shares: None,
        voice_status: None,
    };
    let category = |id: &str, name: &str, rank| Category {
        id: id.into(),
        server_id: Some(SERVER.into()),
        name: name.into(),
        rank,
    };
    ChannelsResponse {
        channels: fixture
            .text_channels()
            .iter()
            .map(|&(id, name)| channel(id, name, ChannelType::Text, "cat_fixture_text"))
            .chain([channel(VOICE, "lounge", ChannelType::Voice, "cat_fixture_voice")])
            .collect(),
        categories: vec![
            category("cat_fixture_text", "Text", 0),
            category("cat_fixture_voice", "Voice", 1),
        ],
    }
}

pub(crate) fn blank_message() -> Message {
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
        reply_count: 0,
    }
}

/// A small deterministic generator (xorshift64*), so runs are comparable.
pub(crate) struct Rng(pub(crate) u64);

impl Rng {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, n: u64) -> u64 {
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

pub(crate) const AUTHORS: &[(&str, &str, Option<&str>)] = &[
    (ME, "me", Some("Fixture Me")),
    ("user_fixture_alice", "alice", Some("Alice")),
    ("user_fixture_bob", "bob", None),
    ("user_fixture_cara", "cara", Some("Cara Lindqvist")),
    ("user_fixture_dev", "dev", Some("dev bot")),
    ("user_fixture_eli", "eli", Some("Eli")),
];

pub(crate) fn sentence(rng: &mut Rng) -> String {
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
    messages_in(count, TEXT)
}

/// [`messages`] for one channel; each channel has its own history.
pub fn messages_in(count: usize, channel_id: &str) -> Vec<Message> {
    let seed = channel_id
        .bytes()
        .fold(0x9E37_79B9_7F4A_7C15_u64, |h, b| {
            if channel_id == TEXT {
                h
            } else {
                (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3)
            }
        });
    let mut rng = Rng(seed);
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
        // YAP_FIXTURE_GIF=<url>: the last message and the fifth from last
        // carry that GIF (animated idle with chat GIFs, docs/PERFORMANCE.md).
        let gif = std::env::var("YAP_FIXTURE_GIF").ok().filter(|_| n + 1 == count || n + 6 == count);
        let attachments = if let Some(url) = gif {
            vec![Attachment {
                id: Some(format!("att_fixture_{n}")),
                filename: format!("loop-{n}.gif"),
                content_type: Some("image/gif".into()),
                url: Some(url),
                size_bytes: Some(1024),
                ..Attachment::default()
            }]
        } else if rng.below(40) == 0 {
            // The app icon, so a scroll test lays out real pictures offline.
            vec![Attachment {
                id: Some(format!("att_fixture_{n}")),
                filename: format!("capture-{n}.png"),
                content_type: Some("image/png".into()),
                url: Some(media_base().map_or_else(
                    || "qrc:/app/yap-256.png".into(),
                    |base| format!("{base}/img/{}.png", n % 12),
                )),
                size_bytes: Some(1024),
                ..Attachment::default()
            }]
        } else {
            Vec::new()
        };
        out.push(Message {
            id: if channel_id == TEXT {
                format!("msg_fixture_{n:05}")
            } else {
                format!("msg_{channel_id}_{n:05}")
            },
            channel_id: channel_id.into(),
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
