//! The typed client state. Only [`super::reduce`] mutates it.

use std::collections::HashMap;

use yap_cache::Snapshot;
use yap_protocol::rest::{Category, Channel, ChannelType, ChannelsResponse, Message, MessagePatch};
use yap_protocol::rest::{Server, User};

use super::time::LocalOffset;
use super::view::{ChannelTreeRow, ServerRow, VoiceMemberRow};
use super::{CallId, Occupant, RequestId, SessionId, SignInId, VoiceMember};
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
        // The category id keeps categories that share a rank contiguous.
        channels.sort_by_cached_key(|c| (rank(c), c.category_id.clone(), c.position));
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

    /// The tree as the REST response had it (for the cache).
    pub fn to_response(&self) -> ChannelsResponse {
        ChannelsResponse {
            channels: self.channels.clone(),
            categories: self.categories.clone(),
        }
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

    /// The message at a list position.
    pub fn at(&self, index: usize) -> Option<&Message> {
        self.messages.get(index)
    }

    /// The list position of a message.
    pub fn position(&self, id: &str) -> Option<usize> {
        self.index.get(id).copied()
    }

    /// Inserts (at the end) or replaces by id; returns the position.
    pub fn upsert(&mut self, message: Message) -> usize {
        match self.index.get(&message.id) {
            Some(&i) => {
                self.messages[i] = message;
                i
            }
            None => {
                let i = self.messages.len();
                self.index.insert(message.id.clone(), i);
                self.messages.push(message);
                i
            }
        }
    }

    /// Applies the fields a partial update carries; returns the position, or
    /// `None` if the message is not loaded.
    pub fn patch(&mut self, patch: &MessagePatch) -> Option<usize> {
        let &i = self.index.get(&patch.id)?;
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
        Some(i)
    }

    /// Removes by id; returns the position it had.
    pub fn remove(&mut self, id: &str) -> Option<(usize, Message)> {
        let i = self.index.remove(id)?;
        let message = self.messages.remove(i);
        for (j, m) in self.messages.iter().enumerate().skip(i) {
            self.index.insert(m.id.clone(), j);
        }
        Some((i, message))
    }
}

/// The call as the sidebar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceCall {
    pub id: CallId,
    pub channel_id: String,
    pub status: String,
    /// The roster as the voice session reports it.
    pub roster: Vec<VoiceMember>,
    pub muted: bool,
    pub deafened: bool,
    /// The media path came up once: our voice state goes to the gateway.
    pub joined: bool,
}

/// Mic and deafen, kept between calls and in the snapshot cache (the
/// account row shows them always).
pub use yap_cache::VoicePrefs;

/// The rows each list model shows, as last emitted. Diffs are computed
/// against these, so they must only change together with a diff.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Projection {
    pub servers: Vec<ServerRow>,
    pub tree: Vec<ChannelTreeRow>,
    pub roster: Vec<VoiceMemberRow>,
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

/// The launch: the stored token and the cached snapshot are read in
/// parallel, and the session starts once both are known.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) enum Startup {
    /// No `Start` yet: a token on its own starts a session (tests, tools).
    #[default]
    Idle,
    /// Waiting for the token read and the snapshot read; `None` = not yet.
    Waiting {
        token: Option<Option<Token>>,
        snapshot: Option<Option<Box<Snapshot>>>,
    },
    /// The launch is decided; late reads are ignored.
    Done,
}

/// Everything the client knows. Built by [`State::new`], changed only by
/// [`super::reduce`].
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub(super) api_base: String,
    pub(super) local_offset: LocalOffset,
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
    /// Names seen typing in the open channel, in arrival order.
    pub(super) typing: Vec<String>,
    /// Bumped by each typing event; only the newest expiry clears the line.
    pub(super) typing_generation: u64,
    pub(super) voice: Option<VoiceCall>,
    pub(super) voice_prefs: VoicePrefs,
    /// Why the last call failed; cleared by the next join.
    pub(super) voice_error: String,
    /// Who is in each voice channel, by channel id (from the gateway).
    pub(super) voice_channels: HashMap<String, Vec<Occupant>>,
    /// The voice channel whose view is open (landing or call), if any.
    pub(super) voice_view: Option<String>,
    /// The server whose channel tree is shown (the last one that loaded).
    pub(super) tree_server: Option<String>,
    pub(super) view: Projection,
    pub(super) pending: Pending,
    pub(super) boot: Option<Boot>,
    pub(super) first_page_shown: bool,
    pub(super) startup: Startup,
    /// The view came from the cache and is being revalidated.
    pub(super) refreshing: bool,
    pub(super) next_id: u64,
}

impl State {
    /// `api_base` turns relative avatar, icon and attachment URLs absolute.
    pub fn new(api_base: &str) -> Self {
        Self {
            api_base: api_base.to_owned(),
            local_offset: LocalOffset::UTC,
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
            typing: Vec::new(),
            typing_generation: 0,
            voice: None,
            voice_prefs: VoicePrefs::default(),
            voice_error: String::new(),
            voice_channels: HashMap::new(),
            voice_view: None,
            tree_server: None,
            view: Projection::default(),
            pending: Pending::default(),
            boot: None,
            first_page_shown: false,
            startup: Startup::Idle,
            refreshing: false,
            next_id: 0,
        }
    }

    /// Formats message times with this offset source (UTC by default).
    pub fn with_local_offset(mut self, offset: LocalOffset) -> Self {
        self.local_offset = offset;
        self
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

    /// `true` while a view shown from the cache waits for the network.
    pub fn refreshing(&self) -> bool {
        self.refreshing
    }

    /// Names typing in the open channel.
    pub fn typing(&self) -> &[String] {
        &self.typing
    }

    pub fn voice(&self) -> Option<&VoiceCall> {
        self.voice.as_ref()
    }

    /// Mic and deafen, kept between calls.
    pub fn voice_prefs(&self) -> VoicePrefs {
        self.voice_prefs
    }

    /// Who the gateway says is in a voice channel.
    pub fn occupants(&self, channel_id: &str) -> &[Occupant] {
        self.voice_channels
            .get(channel_id)
            .map_or(&[], Vec::as_slice)
    }

    /// The voice channel whose view is open.
    pub fn voice_view(&self) -> Option<&str> {
        self.voice_view.as_deref()
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
