//! The typed client state. Only [`super::reduce`] mutates it.

use std::collections::{HashMap, HashSet};

use yap_cache::Snapshot;
use yap_protocol::rest::Reaction;
use yap_protocol::rest::{Category, Channel, ChannelType, ChannelsResponse, Message, MessagePatch};
use yap_protocol::rest::{DmChannel, GifItem, Member, Notification, Server, UploadedFile, User};

use super::time::{is_after, LocalOffset};
use super::view::{
    ChannelTreeRow, DmRow, DmView, GifRow, MemberListRow, MentionRow, ProfileView, ServerRow,
    UploadRow, VoiceMemberRow, VoiceTileRow,
};
use super::{CallId, Occupant, RequestId, SessionId, SignInId, VoiceMember};
use crate::api::{ApiError, Token};
use crate::shell::{BadgeView, Toasted};

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
    /// The server has older messages than the first one here.
    pub has_more_before: bool,
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

    pub fn iter(&self) -> std::slice::Iter<'_, Message> {
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
    /// An update older than the stored revision is dropped (chat-reducer.ts
    /// `UPDATE_MESSAGE`; a missing revision counts as 0).
    pub fn patch(&mut self, patch: &MessagePatch) -> Option<usize> {
        let &i = self.index.get(&patch.id)?;
        let message = &mut self.messages[i];
        if patch.content_revision.unwrap_or(0) < message.content_revision.unwrap_or(0) {
            return None;
        }
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

    /// Adds or removes one user's reaction (chat-reducer.ts `ADD_REACTION`,
    /// `REMOVE_REACTION`): matched by the exact emoji string; an add that
    /// is already there or a remove that is not changes nothing. Returns the
    /// position when the message changed.
    pub fn react(&mut self, id: &str, emoji: &str, user_id: &str, add: bool) -> Option<usize> {
        let &i = self.index.get(id)?;
        let reactions = &mut self.messages[i].reactions;
        let found = reactions.iter().position(|r| r.emoji == emoji);
        match (found, add) {
            (Some(at), true) => {
                let reaction = &mut reactions[at];
                if reaction.users.iter().any(|u| u == user_id) {
                    return None;
                }
                reaction.users.push(user_id.to_owned());
                reaction.count = i64::try_from(reaction.users.len()).unwrap_or(i64::MAX);
            }
            (None, true) => reactions.push(Reaction {
                emoji: emoji.to_owned(),
                count: 1,
                me: false,
                users: vec![user_id.to_owned()],
            }),
            (Some(at), false) => {
                let reaction = &mut reactions[at];
                let before = reaction.users.len();
                reaction.users.retain(|u| u != user_id);
                if reaction.users.len() == before {
                    return None;
                }
                reaction.count = i64::try_from(reaction.users.len()).unwrap_or(i64::MAX);
                if reaction.count <= 0 {
                    reactions.remove(at);
                }
            }
            (None, false) => return None,
        }
        Some(i)
    }

    /// Removes by id; returns the position it had.
    pub fn remove(&mut self, id: &str) -> Option<(usize, Message)> {
        let i = self.index.remove(id)?;
        let message = self.messages.remove(i);
        self.reindex(i);
        Some((i, message))
    }

    fn reindex(&mut self, from: usize) {
        for (j, m) in self.messages.iter().enumerate().skip(from) {
            self.index.insert(m.id.clone(), j);
        }
    }

    pub fn first(&self) -> Option<&Message> {
        self.messages.first()
    }

    /// Puts an older page in front, skipping messages already here; returns
    /// how many went in.
    pub fn prepend(&mut self, older: Vec<Message>) -> usize {
        let fresh: Vec<Message> = older
            .into_iter()
            .filter(|m| !self.index.contains_key(&m.id))
            .collect();
        let count = fresh.len();
        if count > 0 {
            self.messages.splice(0..0, fresh);
            self.reindex(0);
        }
        count
    }

    /// Inserts by `created_at` (after equal times); returns the position.
    /// For putting back a message whose delete failed.
    pub fn insert_sorted(&mut self, message: Message) -> usize {
        if let Some(&i) = self.index.get(&message.id) {
            self.messages[i] = message;
            return i;
        }
        let at = self
            .messages
            .partition_point(|m| m.created_at <= message.created_at);
        self.messages.insert(at, message);
        self.reindex(at);
        at
    }

    /// Puts `message` where `old_id` is (a pending row taking its server
    /// id); `None` if `old_id` is not here or `message` already is.
    pub fn replace(&mut self, old_id: &str, message: Message) -> Option<usize> {
        if self.index.contains_key(&message.id) {
            return None;
        }
        let i = self.index.remove(old_id)?;
        self.index.insert(message.id.clone(), i);
        self.messages[i] = message;
        Some(i)
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
    /// The room said Ready once (STATE_MACHINE.md `joined`): a leave is a
    /// joined teardown (sound, presence leave).
    pub joined: bool,
    /// Participants whose shares the user watches.
    pub watched: std::collections::BTreeSet<String>,
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
    pub tiles: Vec<VoiceTileRow>,
    pub mentions: Vec<MentionRow>,
    pub dms: Vec<DmRow>,
    pub members: Vec<MemberListRow>,
    /// Unread DMs, as last emitted.
    pub dm_unread: i64,
    pub profile: ProfileView,
    pub dm: DmView,
    /// The tray badge, as last emitted.
    pub badge: BadgeView,
    /// The composer tray of the open channel.
    pub uploads: Vec<UploadRow>,
    /// (ready, busy) uploads of the open channel, as last emitted.
    pub upload_counts: (i64, i64),
    pub gifs: Vec<GifRow>,
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
    /// request → nonce of the send
    pub sends: HashMap<RequestId, String>,
    /// (request, channel id) of the page before the open window
    pub older: Option<(RequestId, String)>,
    /// Channel trees of servers other than the shown one (for unread).
    pub trees: HashMap<RequestId, String>,
    pub read_states: Option<RequestId>,
    pub notifications: Option<RequestId>,
    /// request → server id
    pub members: HashMap<RequestId, String>,
    /// request → (channel, the mark before, the optimistic mark)
    pub reads: HashMap<RequestId, (String, Option<String>, String)>,
    /// request → the message as it was before the edit
    pub edits: HashMap<RequestId, Message>,
    /// request → the message the delete removed
    pub deletes: HashMap<RequestId, Message>,
    pub dms: Option<RequestId>,
    /// (request, user id) of the DM being opened
    pub open_dm: Option<(RequestId, String)>,
    /// (request, user id) of the popout's profile
    pub profile: Option<(RequestId, String)>,
    /// request → the reaction change to undo if it fails
    pub reactions: HashMap<RequestId, ReactionChange>,
    /// request → local id of the upload
    pub uploads: HashMap<RequestId, String>,
    /// request → nonce of the send waiting for its GIF
    pub gif_uploads: HashMap<RequestId, String>,
    /// The GIF page being fetched (true: the next page, false: a new query).
    pub gifs: Option<(RequestId, bool)>,
    /// request → (file name shown, open it when saved)
    pub downloads: HashMap<RequestId, (String, bool)>,
}

/// An optimistic reaction add or remove, undone if the server refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReactionChange {
    pub message_id: String,
    pub emoji: String,
    pub user_id: String,
    pub added: bool,
}

/// Where a composer upload stands.
#[derive(Debug, Clone, PartialEq)]
pub enum UploadStatus {
    /// Bytes sent so far.
    Uploading(u64),
    Done(UploadedFile),
    /// The error copy (api-errors.ts `describeApiError`).
    Failed(String),
}

/// One file in the composer tray (useMessageInput.ts `pendingUploads` and
/// `uploadedFiles`), tied to the channel it was added in.
#[derive(Debug, Clone, PartialEq)]
pub struct Upload {
    /// Chosen by the UI.
    pub local_id: String,
    pub channel_id: String,
    pub path: String,
    pub name: String,
    pub size: u64,
    pub content_type: String,
    /// Marked sensitive (sent in `nsfw_attachment_ids`).
    pub nsfw: bool,
    pub request: Option<RequestId>,
    pub status: UploadStatus,
}

/// The GIF picker's results (GifPickerModal.tsx), one query at a time.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GifBrowse {
    pub query: String,
    /// `gifs`, `stickers` or `memes`.
    pub media_type: String,
    pub items: Vec<GifItem>,
    pub next: Option<String>,
    pub loading: bool,
    pub error: String,
}

/// A message this client posted that the server has not confirmed. Its row
/// is `pending-<nonce>` until the echo (or the POST answer) replaces it.
#[derive(Debug, Clone, PartialEq)]
pub struct Outgoing {
    /// The pending row as shown (`pending-<nonce>`, local time).
    pub message: Message,
    pub failed: bool,
    /// Uploaded files the message links (`attachment_ids`).
    pub attachment_ids: Vec<String>,
    pub nsfw_ids: Vec<String>,
    /// A GIF pick that still has to be registered as an attachment.
    pub gif: Option<GifItem>,
}

/// The row id of a pending send.
pub fn pending_id(nonce: &str) -> String {
    format!("pending-{nonce}")
}

/// Where the "New" divider sits in the open channel.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Divider {
    /// The first unread message; the divider is drawn above it.
    pub message_id: String,
    /// Messages from it on (the banner count).
    pub count: i64,
    /// The banner shows until the list reaches its end.
    pub banner: bool,
    /// Seen at the end when the window came back into focus: the next
    /// batch that arrives while away moves it (useChatArea.ts `onReturn`).
    pub locked: bool,
}

/// Someone typing in the open channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Typer {
    pub user_id: String,
    pub name: String,
    pub generation: u64,
}

/// Who is connected and as what (chat-gateway.ts `PRESENCE_LIST`,
/// `PRESENCE_UPDATE`). Everyone is offline until the first list arrives.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Presence {
    /// A list has arrived: anyone missing from `live` is offline.
    pub listed: bool,
    /// User id → `online`, `idle` or `dnd`, for people connected now.
    pub live: HashMap<String, &'static str>,
    /// User id → the custom status a live update carried (`None`: cleared).
    pub custom: HashMap<String, Option<String>>,
    /// Users a `PRESENCE_UPDATE` named since sign-in (offline ones too).
    pub reported: HashSet<String>,
}

/// A status as the UI shows it: `online`, `idle`, `dnd`, or `None` for
/// offline (and invisible).
pub fn live_status(status: Option<&str>) -> Option<&'static str> {
    match status {
        Some("online") => Some("online"),
        Some("idle") => Some("idle"),
        Some("dnd") => Some("dnd"),
        _ => None,
    }
}

impl Presence {
    /// `online`, `idle`, `dnd` or `offline`.
    /// Nobody is online before the first list: a REST row's `status` is
    /// the chosen one, not whether they are connected (the web's
    /// `onlineUsers` starts empty too). `_fallback` is kept for the
    /// callers' symmetry with [`Self::custom_status`].
    pub fn of(&self, user_id: &str, _fallback: Option<&str>) -> &'static str {
        if !self.listed {
            return "offline";
        }
        self.live.get(user_id).copied().unwrap_or("offline")
    }

    /// A DM recipient's status (DMListPanel.tsx shows `recipient.status`):
    /// before any list, the last update about them, else the status
    /// `/api/dms` sent. At Home with no conversation open the server sends
    /// no list (it answers `ChannelSubscribe` only), so this is what the web
    /// shows there too. Once a list arrived, the same as [`Self::of`].
    pub fn of_recipient(&self, user_id: &str, rest: Option<&str>) -> &'static str {
        if self.listed || self.reported.contains(user_id) {
            return self.live.get(user_id).copied().unwrap_or("offline");
        }
        live_status(rest).unwrap_or("offline")
    }

    /// The live custom status when an update carried one, else `fallback`.
    pub fn custom_status(&self, user_id: &str, fallback: Option<&str>) -> String {
        let value = match self.custom.get(user_id) {
            Some(live) => live.as_deref(),
            None => fallback,
        };
        value.unwrap_or_default().trim().to_owned()
    }
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
    /// Who is typing in the open channel, in arrival order.
    pub(super) typing: Vec<Typer>,
    /// The typing line as last emitted.
    pub(super) typing_text: String,
    /// Bumped by each typing event; only the newest one of a typer clears them.
    pub(super) typing_generation: u64,
    /// Read marks by channel (RFC 3339; compared as instants).
    pub(super) read_states: HashMap<String, String>,
    /// Newest message time by channel.
    pub(super) last_message_at: HashMap<String, String>,
    /// Unread notifications; `mention` and `reply` ones are the badges.
    pub(super) notifications: Vec<Notification>,
    /// Members by server id, with their roles (the member panel, mention
    /// suggestions, typing names).
    pub(super) members: HashMap<String, Vec<Member>>,
    /// Home (the DM list) is shown instead of a server.
    pub(super) home: bool,
    /// DM channels, newest first (as `/api/dms` sorts them; new ones go on top).
    pub(super) dms: Vec<DmChannel>,
    pub(super) presence: Presence,
    /// The user whose popout is open, and their full profile once fetched.
    pub(super) profile: Option<(String, Option<User>)>,
    pub(super) divider: Option<Divider>,
    /// The open channel's newest message time when it loaded: mentions on
    /// messages after it arrived while it was open (and were seen).
    pub(super) seen_floor: Option<String>,
    /// The top-most message on screen, as the list last reported it.
    pub(super) seen_from: Option<String>,
    /// Sends not yet confirmed, by nonce.
    pub(super) outbox: HashMap<String, Outgoing>,
    /// Composer uploads, every channel's, in the order they were added.
    pub(super) uploads: Vec<Upload>,
    pub(super) gifs: GifBrowse,
    pub(super) focused: bool,
    pub(super) at_bottom: bool,
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
    /// A channel to open once its server's tree answers (a toast click).
    pub(super) wanted_channel: Option<String>,
    /// Messages that were toasted (the notification for one does not toast).
    pub(super) toasted: Toasted,
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
            typing_text: String::new(),
            typing_generation: 0,
            read_states: HashMap::new(),
            last_message_at: HashMap::new(),
            notifications: Vec::new(),
            members: HashMap::new(),
            home: false,
            dms: Vec::new(),
            presence: Presence::default(),
            profile: None,
            divider: None,
            seen_floor: None,
            seen_from: None,
            outbox: HashMap::new(),
            uploads: Vec::new(),
            gifs: GifBrowse::default(),
            focused: true,
            at_bottom: false,
            voice: None,
            voice_prefs: VoicePrefs::default(),
            voice_error: String::new(),
            voice_channels: HashMap::new(),
            voice_view: None,
            tree_server: None,
            wanted_channel: None,
            toasted: Toasted::default(),
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

    /// Who is typing in the open channel.
    pub fn typing(&self) -> &[Typer] {
        &self.typing
    }

    /// A channel read mark.
    pub fn read_at(&self, channel_id: &str) -> Option<&str> {
        self.read_states.get(channel_id).map(String::as_str)
    }

    /// The open channel "New" divider.
    pub fn divider(&self) -> Option<&Divider> {
        self.divider.as_ref()
    }

    /// Sends not yet confirmed, by nonce.
    pub fn outbox(&self) -> &HashMap<String, Outgoing> {
        &self.outbox
    }

    /// Composer uploads of every channel.
    pub fn uploads(&self) -> &[Upload] {
        &self.uploads
    }

    /// The GIF picker's results.
    pub fn gifs(&self) -> &GifBrowse {
        &self.gifs
    }

    /// A channel has messages newer than its read mark (ChannelSidebar.tsx;
    /// the open channel never shows as unread).
    pub fn is_unread(&self, channel_id: &str) -> bool {
        self.active_channel.as_deref() != Some(channel_id) && self.has_unread(channel_id)
    }

    /// The newest message is after the read mark (or nothing was read).
    pub(super) fn has_unread(&self, channel_id: &str) -> bool {
        match (
            self.last_message_at.get(channel_id),
            self.read_states.get(channel_id),
        ) {
            (Some(last), Some(read)) => is_after(last, read),
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    /// Unread mention and reply notifications in a channel.
    pub fn channel_mentions(&self, channel_id: &str) -> i64 {
        let n = self
            .notifications
            .iter()
            .filter(|n| n.is_mention() && n.channel_id.as_deref() == Some(channel_id))
            .count();
        i64::try_from(n).unwrap_or(i64::MAX)
    }

    /// Unread mention and reply notifications in a server.
    pub fn server_mentions(&self, server_id: &str) -> i64 {
        let n = self
            .notifications
            .iter()
            .filter(|n| n.is_mention() && n.server_id.as_deref() == Some(server_id))
            .count();
        i64::try_from(n).unwrap_or(i64::MAX)
    }

    /// Any text channel of the server is unread.
    pub fn server_unread(&self, server_id: &str) -> bool {
        self.channels.get(server_id).is_some_and(|list| {
            list.channels
                .iter()
                .any(|c| c.channel_type == ChannelType::Text && self.is_unread(&c.id))
        })
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

    /// Home (the DM list) is shown.
    pub fn home(&self) -> bool {
        self.home
    }

    pub fn dms(&self) -> &[DmChannel] {
        &self.dms
    }

    pub fn dm(&self, channel_id: &str) -> Option<&DmChannel> {
        self.dms.iter().find(|dm| dm.id == channel_id)
    }

    pub fn presence(&self) -> &Presence {
        &self.presence
    }

    /// A server's members, once loaded.
    pub fn members(&self, server_id: &str) -> Option<&[Member]> {
        self.members.get(server_id).map(Vec::as_slice)
    }

    /// The user whose popout is open.
    pub fn profile_user(&self) -> Option<&str> {
        self.profile.as_ref().map(|(id, _)| id.as_str())
    }

    /// Unread DM, mention and reply notifications of a DM (DMListPanel.tsx).
    pub fn dm_mentions(&self, channel_id: &str) -> i64 {
        let n = self
            .notifications
            .iter()
            .filter(|n| {
                !n.is_read
                    && n.server_id.is_none()
                    && n.channel_id.as_deref() == Some(channel_id)
                    && matches!(n.kind.as_str(), "dm" | "mention" | "reply")
            })
            .count();
        i64::try_from(n).unwrap_or(i64::MAX)
    }

    /// Unread notifications outside servers in a channel (the rail's DM key).
    pub(super) fn dm_notifications(&self, channel_id: &str) -> i64 {
        let n = self
            .notifications
            .iter()
            .filter(|n| {
                !n.is_read && n.server_id.is_none() && n.channel_id.as_deref() == Some(channel_id)
            })
            .count();
        i64::try_from(n).unwrap_or(i64::MAX)
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
