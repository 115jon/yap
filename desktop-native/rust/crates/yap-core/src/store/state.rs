//! The typed client state. Only [`super::reduce`] mutates it.

use std::collections::HashMap;

use serde_json::Value;
use yap_protocol::rest::{Category, Channel, ChannelType, ChannelsResponse, Message, MessagePatch};
use yap_protocol::rest::{Server, User};

use super::{CallId, RequestId, SessionId, SignInId};
use crate::api::{ApiError, Token};

/// Which top-level screen the UI shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before `Start` has been handled.
    Boot,
    SignedOut,
    DeviceCode,
    Loading,
    Ready,
}

impl Phase {
    /// The name QML compares against.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::SignedOut => "signedOut",
            Self::DeviceCode => "deviceCode",
            Self::Loading => "loading",
            Self::Ready => "ready",
        }
    }
}

/// The gateway as the store sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GatewayStatus {
    pub ready: bool,
    /// Set after the first Ready; a later Ready is a reconnect.
    pub was_ready: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Session {
    pub id: SessionId,
    pub(super) token: Token,
    /// Set once `/api/users/me` answered; until then the session is booting.
    pub me: Option<User>,
    pub gateway: GatewayStatus,
}

/// One server's channel tree, sorted by category rank, then position.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerChannels {
    pub channels: Vec<Channel>,
    pub categories: Vec<Category>,
}

impl ServerChannels {
    pub fn new(response: ChannelsResponse) -> Self {
        let ChannelsResponse {
            mut channels,
            categories,
        } = response;
        let rank = |channel: &Channel| {
            categories
                .iter()
                .find(|c| Some(c.id.as_str()) == channel.category_id.as_deref())
                .map_or(-1, |c| c.rank)
        };
        channels.sort_by_cached_key(|c| (rank(c), c.position));
        Self {
            channels,
            categories,
        }
    }

    /// The channel's category name, or `""` when it has none.
    pub fn category_name(&self, channel: &Channel) -> &str {
        self.categories
            .iter()
            .find(|c| Some(c.id.as_str()) == channel.category_id.as_deref())
            .map_or("", |c| c.name.as_str())
    }

    pub fn get(&self, id: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.id == id)
    }

    pub fn first_text(&self) -> Option<&Channel> {
        self.channels
            .iter()
            .find(|c| c.channel_type == ChannelType::Text)
    }
}

/// The open channel's messages, in page order then arrival order, with an
/// id → index map so upserts and patches are O(1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MessageLog {
    pub channel_id: String,
    messages: Vec<Message>,
    index: HashMap<String, usize>,
}

impl MessageLog {
    pub fn new(channel_id: &str) -> Self {
        Self {
            channel_id: channel_id.to_owned(),
            ..Self::default()
        }
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Message> {
        self.messages.iter()
    }

    pub fn get(&self, id: &str) -> Option<&Message> {
        self.index.get(id).map(|&i| &self.messages[i])
    }

    /// Inserts or replaces by id.
    pub fn upsert(&mut self, message: Message) {
        match self.index.get(&message.id) {
            Some(&i) => self.messages[i] = message,
            None => {
                self.index.insert(message.id.clone(), self.messages.len());
                self.messages.push(message);
            }
        }
    }

    /// Applies the fields a partial update carries. `false` if the message
    /// is not loaded.
    pub fn patch(&mut self, patch: &MessagePatch) -> bool {
        let Some(&i) = self.index.get(&patch.id) else {
            return false;
        };
        let message = &mut self.messages[i];
        if let Some(content) = &patch.content {
            message.content.clone_from(content);
        }
        if let Some(updated_at) = &patch.updated_at {
            message.updated_at = Some(updated_at.clone());
        }
        if let Some(embeds) = &patch.embeds {
            message.embeds.clone_from(embeds);
        }
        if patch.content_revision.is_some() {
            message.content_revision = patch.content_revision;
        }
        true
    }

    pub fn remove(&mut self, id: &str) -> Option<Message> {
        let i = self.index.remove(id)?;
        let message = self.messages.remove(i);
        for (j, m) in self.messages.iter().enumerate().skip(i) {
            self.index.insert(m.id.clone(), j);
        }
        Some(message)
    }
}

/// The call as the sidebar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceCall {
    pub id: CallId,
    pub channel_id: String,
    pub status: String,
    /// The roster as the voice session reports it.
    pub roster: Value,
    pub muted: bool,
}

/// The request id each slot is waiting for. A result for any other id is stale.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Pending {
    pub me: Option<RequestId>,
    pub servers: Option<RequestId>,
    /// (request, server id)
    pub channels: Option<(RequestId, String)>,
    /// (request, channel id)
    pub page: Option<(RequestId, String)>,
    /// request → channel id
    pub sends: HashMap<RequestId, String>,
}

/// The sign-in chain (me + servers, then channels, then the first page).
/// Phase `ready` is shown when it settles, as before.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Boot {
    pub me_done: bool,
    /// A servers result that arrived before `me`; applied right after it.
    pub early_servers: Option<Result<Vec<Server>, ApiError>>,
}

/// Everything the client knows. Built by [`State::new`], changed only by
/// [`super::reduce`].
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub(super) api_base: String,
    pub(super) phase: Phase,
    pub(super) status: String,
    pub(super) connected: bool,
    pub(super) session: Option<Session>,
    pub(super) sign_in: Option<SignInId>,
    pub(super) servers: Vec<Server>,
    /// Channel trees by server id, as last fetched.
    pub(super) channels: HashMap<String, ServerChannels>,
    pub(super) active_server: Option<String>,
    pub(super) active_channel: Option<String>,
    pub(super) messages: Option<MessageLog>,
    /// Display names seen so far, for typing indicators.
    pub(super) names: HashMap<String, String>,
    /// The last user seen typing in the open channel.
    pub(super) typing: Option<String>,
    pub(super) voice: Option<VoiceCall>,
    pub(super) pending: Pending,
    pub(super) boot: Option<Boot>,
    pub(super) first_page_shown: bool,
    pub(super) next_id: u64,
}

impl State {
    /// `api_base` turns relative avatar, icon and attachment URLs absolute.
    pub fn new(api_base: &str) -> Self {
        Self {
            api_base: api_base.to_owned(),
            phase: Phase::Boot,
            status: String::new(),
            connected: false,
            session: None,
            sign_in: None,
            servers: Vec::new(),
            channels: HashMap::new(),
            active_server: None,
            active_channel: None,
            messages: None,
            names: HashMap::new(),
            typing: None,
            voice: None,
            pending: Pending::default(),
            boot: None,
            first_page_shown: false,
            next_id: 0,
        }
    }

    pub fn phase(&self) -> Phase {
        self.phase
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn connected(&self) -> bool {
        self.connected
    }

    pub fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    pub fn me(&self) -> Option<&User> {
        self.session.as_ref()?.me.as_ref()
    }

    pub fn servers(&self) -> &[Server] {
        &self.servers
    }

    pub fn channels(&self, server_id: &str) -> Option<&ServerChannels> {
        self.channels.get(server_id)
    }

    pub fn active_server(&self) -> Option<&str> {
        self.active_server.as_deref()
    }

    pub fn active_channel(&self) -> Option<&str> {
        self.active_channel.as_deref()
    }

    pub fn messages(&self) -> Option<&MessageLog> {
        self.messages.as_ref()
    }

    pub fn typing(&self) -> Option<&str> {
        self.typing.as_deref()
    }

    pub fn voice(&self) -> Option<&VoiceCall> {
        self.voice.as_ref()
    }

    /// A channel from any loaded server.
    pub(super) fn find_channel(&self, id: &str) -> Option<&Channel> {
        self.channels.values().find_map(|list| list.get(id))
    }

    pub(super) fn next_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }
}
