//! The legacy UI projection: [`Ui`] updates with lists as JSON, exactly what
//! the QML side binds today. The Rust list models (PR 3) replace this.

use serde_json::{json, Value};
use yap_protocol::rest::{ChannelType, Message, MessagePatch, Server, User};

use super::state::{ServerChannels, VoiceCall};

/// Updates for the Qt side. Lists travel as JSON so QML can bind them
/// directly; the message list is incremental (reset / upsert / remove).
#[derive(Debug, Clone, PartialEq)]
pub enum Ui {
    Phase(&'static str),
    DeviceCode {
        user_code: String,
        url: String,
    },
    Status(String),
    Connected(bool),
    Me(Value),
    Servers(Value),
    ActiveServer(String),
    Channels(Value),
    ActiveChannel {
        id: String,
        name: String,
    },
    MessagesReset(Value),
    MessageUpsert(Value),
    MessageRemoved(String),
    Typing(String),
    /// The call: channel id (empty when not in one), status line, roster, mute.
    Voice {
        channel_id: String,
        status: String,
        roster: Value,
        muted: bool,
    },
}

pub(super) fn absolute_url(api_base: &str, url: Option<&str>) -> String {
    match url.unwrap_or_default() {
        url if url.starts_with('/') => format!("{api_base}{url}"),
        url => url.to_owned(),
    }
}

pub(super) fn me(user: &User, api_base: &str) -> Value {
    json!({
        "id": user.id,
        "name": user.label(),
        "username": user.username,
        "avatar": absolute_url(api_base, user.avatar_url.as_deref()),
    })
}

pub(super) fn servers(servers: &[Server], api_base: &str) -> Value {
    servers
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "name": s.name,
                "icon": absolute_url(api_base, s.icon_url.as_deref()),
            })
        })
        .collect()
}

fn channel_type(kind: ChannelType) -> &'static str {
    match kind {
        ChannelType::Text => "text",
        ChannelType::Voice => "voice",
        ChannelType::Dm => "dm",
        ChannelType::Other => "other",
    }
}

pub(super) fn channels(list: &ServerChannels) -> Value {
    list.channels
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "name": c.name,
                "type": channel_type(c.channel_type),
                "category": list.category_name(c),
            })
        })
        .collect()
}

/// The fields the message list needs, in QML-friendly names.
pub(super) fn message(message: &Message, api_base: &str) -> Value {
    let author = message.author.as_ref();
    let attachments: Vec<Value> = message
        .attachments
        .iter()
        .map(|a| {
            json!({
                "name": a.filename,
                "url": absolute_url(api_base, a.url.as_deref()),
                "contentType": a.content_type.as_deref().unwrap_or_default(),
            })
        })
        .collect();
    json!({
        "id": message.id,
        "channelId": message.channel_id,
        "authorId": message.author_id,
        "author": author.map(User::label).unwrap_or_default(),
        "avatar": absolute_url(api_base, author.and_then(|a| a.avatar_url.as_deref())),
        "content": message.content,
        "createdAt": message.created_at,
        "edited": message.updated_at.is_some(),
        "attachments": attachments,
    })
}

pub(super) fn messages<'a>(list: impl Iterator<Item = &'a Message>, api_base: &str) -> Value {
    list.map(|m| message(m, api_base)).collect()
}

/// A partial update passes on only the fields it carries; QML merges it.
pub(super) fn patch(patch: &MessagePatch) -> Value {
    let mut out = json!({ "id": patch.id, "partial": true });
    if let Some(content) = &patch.content {
        out["content"] = json!(content);
    }
    if patch.updated_at.is_some() {
        out["edited"] = json!(true);
    }
    out
}

pub(super) fn voice(call: Option<&VoiceCall>) -> Ui {
    match call {
        Some(call) => Ui::Voice {
            channel_id: call.channel_id.clone(),
            status: call.status.clone(),
            roster: call.roster.clone(),
            muted: call.muted,
        },
        None => Ui::Voice {
            channel_id: String::new(),
            status: String::new(),
            roster: json!([]),
            muted: false,
        },
    }
}
