//! The typed client state. Only [`super::reduce`] mutates it.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use yap_cache::Snapshot;
use yap_protocol::game_activity::GameActivity;
use yap_protocol::rest::Reaction;
use yap_protocol::rest::{Category, Channel, ChannelType, ChannelsResponse, Message, MessagePatch};
use yap_protocol::rest::{
    DmChannel, GifCategory, GifItem, Member, Notification, Server, UploadedFile, User,
};
use yap_protocol::rest::{SessionInfo, ShareItem};

use super::time::{is_after, LocalOffset};
use super::view::{
    AdminView, ChannelTreeRow, DmRow, DmView, GifRow, Load, MemberListRow, MentionRow, ProfileView,
    ServerRow, SettingsView, UploadRow, VoiceMemberRow, VoiceTileRow,
};
use super::ProfileAsset;
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
    /// The retry scheduled since the last ready (0: none), for the
    /// connection strip's "Try N" and its turn to "No chat link".
    pub attempt: u32,
    /// The last try found no network (the strip's "Offline").
    pub offline: bool,
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
    /// The server has newer messages than the last one here: a jump's
    /// window, detached from the newest end (useChatArea.ts isDetached).
    pub has_more_after: bool,
    messages: Vec<Message>,
    index: HashMap<String, usize>,
    /// One shared copy of each author, by user id: a page carries the author
    /// in full on every message (performance pass 6).
    authors: HashMap<String, Arc<User>>,
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

    /// A message to change in place (fields that keep its position).
    pub fn get_mut(&mut self, id: &str) -> Option<&mut Message> {
        self.index.get(id).map(|&i| &mut self.messages[i])
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
    /// `message`'s author as the copy already shared, when it is the same.
    fn intern(&mut self, mut message: Message) -> Message {
        if let Some(author) = &message.author {
            match self.authors.get(&author.id) {
                Some(known) if Arc::ptr_eq(known, author) || **known == **author => {
                    message.author = Some(Arc::clone(known));
                }
                _ => {
                    self.authors.insert(author.id.clone(), Arc::clone(author));
                }
            }
        }
        message
    }

    pub fn upsert(&mut self, message: Message) -> usize {
        let message = self.intern(message);
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
        let fresh: Vec<Message> = fresh.into_iter().map(|m| self.intern(m)).collect();
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
        let message = self.intern(message);
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
        let message = self.intern(message);
        let i = self.index.remove(old_id)?;
        self.index.insert(message.id.clone(), i);
        self.messages[i] = message;
        Some(i)
    }

    /// The newest message that is not a pending send.
    pub fn newest_sent(&self) -> Option<&Message> {
        self.messages
            .iter()
            .rev()
            .find(|m| !m.id.starts_with("pending-"))
    }

    /// Drops pending sends (their rows come back from the outbox) and
    /// keeps only the newest `keep` messages; trimmed older ones can be
    /// paged in again, so the log then has more before.
    pub(super) fn trim_for_warm(&mut self, keep: usize) {
        let before = self.messages.len();
        self.messages.retain(|m| !m.id.starts_with("pending-"));
        let extra = self.messages.len().saturating_sub(keep);
        if extra > 0 {
            self.messages.drain(..extra);
            self.has_more_before = true;
        }
        if self.messages.len() != before {
            self.index.clear();
            self.reindex(0);
            let used: HashSet<&str> = self
                .messages
                .iter()
                .filter_map(|m| m.author.as_ref().map(|a| a.id.as_str()))
                .collect();
            self.authors.retain(|id, _| used.contains(id.as_str()));
        }
    }

    /// A rough heap size of the log, for the warm cache's byte cap: each
    /// message's strings, attachments, reactions and embeds, plus one copy
    /// of each author (the log shares them).
    pub fn estimated_bytes(&self) -> usize {
        const MESSAGE: usize = 480;
        const ATTACHMENT: usize = 160;
        const EMBED: usize = 1024;
        const REACTION: usize = 64;
        const AUTHOR: usize = 640;
        let messages: usize = self
            .messages
            .iter()
            .map(|m| {
                let attachments: usize = m
                    .attachments
                    .iter()
                    .map(|a| {
                        ATTACHMENT
                            + a.filename.len()
                            + a.url.as_ref().map_or(0, String::len)
                            + a.file_key.as_ref().map_or(0, String::len)
                    })
                    .sum();
                let reactions: usize = m
                    .reactions
                    .iter()
                    .map(|r| REACTION + r.emoji.len() + r.users.len() * 32)
                    .sum();
                let embeds: usize = m.embeds.iter().map(|e| EMBED + e.url.len()).sum();
                let reply = m.reply_to.as_ref().map_or(0, |_| AUTHOR);
                MESSAGE + m.content.len() + attachments + reactions + embeds + reply
            })
            .sum();
        messages + self.authors.len() * AUTHOR
    }
}

/// A small least-recently-used map of at most `CAP` entries (the pins and
/// media tabs of channels left).
#[derive(Debug, Clone, PartialEq)]
pub struct Lru<K, V, const CAP: usize> {
    /// Least recently used first.
    entries: Vec<(K, V)>,
}

impl<K, V, const CAP: usize> Default for Lru<K, V, CAP> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: PartialEq, V, const CAP: usize> Lru<K, V, CAP> {
    pub fn take(&mut self, key: &K) -> Option<V> {
        let at = self.entries.iter().position(|(k, _)| k == key)?;
        Some(self.entries.remove(at).1)
    }

    /// Stores `value` as the most recently used, dropping the least
    /// recently used one over `CAP`.
    pub fn put(&mut self, key: K, value: V) {
        let _ = self.take(&key);
        self.entries.push((key, value));
        if self.entries.len() > CAP {
            self.entries.remove(0);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&K, &V) -> bool) {
        self.entries.retain(|(k, v)| keep(k, v));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

/// Channels kept warm at most (besides the open one).
pub const WARM_ENTRIES: usize = 24;

/// The warm cache's estimated size at most ([`MessageLog::estimated_bytes`]).
pub const WARM_BYTES: usize = 6 * 1024 * 1024;

/// A warm channel's newest messages kept (as many as the disk keeps).
pub const WARM_WINDOW: usize = yap_cache::MESSAGE_WINDOW;

/// A warm channel is shown without a refetch for this long after its page
/// was confirmed, in the same gateway session (server channels only).
pub const WARM_FRESH_MS: i64 = 30_000;

/// A channel left recently: its newest window, kept current by live
/// events, shown at once when it opens again (and then revalidated).
#[derive(Debug, Clone, PartialEq)]
pub struct WarmEntry {
    pub log: MessageLog,
    /// The gateway session ([`State::gateway_generation`]) the log was last
    /// confirmed in: events from an older one may have been missed.
    pub generation: u64,
    /// When its page was asked for (epoch ms; 0: unknown, never fresh).
    pub confirmed_at_ms: i64,
    /// [`MessageLog::estimated_bytes`] when it was stored or changed.
    pub bytes: usize,
}

/// The channels left recently, least recently used first, within
/// [`WARM_ENTRIES`] and [`WARM_BYTES`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WarmCache {
    entries: Vec<WarmEntry>,
    bytes: usize,
}

impl WarmCache {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The estimated size of every entry.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    pub fn get(&self, channel_id: &str) -> Option<&WarmEntry> {
        self.entries.iter().find(|e| e.log.channel_id == channel_id)
    }

    pub fn contains(&self, channel_id: &str) -> bool {
        self.get(channel_id).is_some()
    }

    /// Channel ids, least recently used first.
    pub fn channel_ids(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.log.channel_id.as_str())
    }

    /// Takes a channel's entry out (it becomes the open log).
    pub(super) fn take(&mut self, channel_id: &str) -> Option<WarmEntry> {
        let at = self
            .entries
            .iter()
            .position(|e| e.log.channel_id == channel_id)?;
        let entry = self.entries.remove(at);
        self.bytes = self.bytes.saturating_sub(entry.bytes);
        Some(entry)
    }

    /// Stores a log left on screen: pending rows dropped, trimmed to its
    /// newest `keep` (at least [`WARM_WINDOW`]). A jump's window is not the newest one and
    /// is not kept. Evicts the least recently used entries over the caps,
    /// except `protect`ed channels; returns the evicted channel ids.
    pub(super) fn put(
        &mut self,
        mut log: MessageLog,
        keep: usize,
        generation: u64,
        confirmed_at_ms: i64,
        protect: &[&str],
    ) -> Vec<String> {
        let _ = self.take(&log.channel_id);
        if log.has_more_after || log.channel_id.is_empty() {
            return Vec::new();
        }
        log.trim_for_warm(keep.max(WARM_WINDOW));
        if log.is_empty() {
            return Vec::new();
        }
        let bytes = log.estimated_bytes();
        self.bytes += bytes;
        self.entries.push(WarmEntry {
            log,
            generation,
            confirmed_at_ms,
            bytes,
        });
        self.enforce(protect)
    }

    /// Changes one entry's log in place (a live event) and keeps it within
    /// the window; `None` when the channel is not warm.
    pub(super) fn update<R>(
        &mut self,
        channel_id: &str,
        protect: &[&str],
        change: impl FnOnce(&mut MessageLog) -> R,
    ) -> Option<R> {
        let entry = self
            .entries
            .iter_mut()
            .find(|e| e.log.channel_id == channel_id)?;
        let result = change(&mut entry.log);
        if entry.log.len() > WARM_WINDOW + WARM_WINDOW / 4 {
            entry.log.trim_for_warm(WARM_WINDOW);
        }
        let bytes = entry.log.estimated_bytes();
        self.bytes = self.bytes.saturating_sub(entry.bytes) + bytes;
        entry.bytes = bytes;
        // Growth from live events counts against the caps too.
        self.enforce(protect);
        Some(result)
    }

    /// The warm channel that holds a message.
    pub(super) fn channel_of(&self, message_id: &str) -> Option<String> {
        self.entries
            .iter()
            .find(|e| e.log.get(message_id).is_some())
            .map(|e| e.log.channel_id.clone())
    }

    pub(super) fn remove(&mut self, channel_id: &str) -> bool {
        self.take(channel_id).is_some()
    }

    pub(super) fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }

    /// Evicts least recently used entries until both caps hold.
    fn enforce(&mut self, protect: &[&str]) -> Vec<String> {
        let mut evicted = Vec::new();
        while self.entries.len() > WARM_ENTRIES || self.bytes > WARM_BYTES {
            let Some(at) = self
                .entries
                .iter()
                .position(|e| !protect.contains(&e.log.channel_id.as_str()))
            else {
                break;
            };
            let entry = self.entries.remove(at);
            self.bytes = self.bytes.saturating_sub(entry.bytes);
            evicted.push(entry.log.channel_id);
        }
        evicted
    }
}

/// The call as the sidebar shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceCall {
    pub id: CallId,
    /// The server the call is in (the soundboard's server key).
    pub server_id: String,
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
    /// The user turned the camera on (it captures); `camera` once it is
    /// published and announced.
    pub camera_wanted: bool,
    pub camera: bool,
    /// Why the camera could not go on (`missing`, `busy`, `unknown`, or the
    /// call's error); empty otherwise.
    pub camera_error: String,
    /// The share asked for, until it ends.
    pub share_wanted: Option<super::VoiceShare>,
    /// The share that is live (`self_stream`).
    pub share: Option<super::VoiceShare>,
    /// Why the last share failed; empty otherwise.
    pub share_error: String,
    /// The share still last sent to the gateway (`stream_preview_url`),
    /// cleared with `null` when the share ends.
    pub share_preview: Option<String>,
    /// A sent still to clear (`null`) once the gateway is back: the share
    /// ended while it was down.
    pub share_preview_clear: bool,
    /// A 1:1 DM call (`channel_id` is the DM channel, no server).
    pub dm: bool,
    /// A moderator's move grant, offered once on this call's op 33.
    pub move_grant: Option<String>,
    /// The spatial panel's room map (JSON), empty until the call has one.
    pub spatial: String,
}

/// What [`super::Command::RegisterPush`] gave: the body of
/// `POST /api/push-tokens`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PushRegistration {
    pub(super) token: String,
    pub(super) platform: String,
    pub(super) format: Option<String>,
    pub(super) app_variant: Option<String>,
}

/// A hidden standby session warming a voice channel's join
/// (voice-standby.ts): promoted by the next join of that channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Standby {
    pub id: super::CallId,
    pub server_id: String,
    pub channel_id: String,
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
    /// The composer's hovered mention, as last emitted.
    pub mention_hover: String,
    /// The `@` candidates' usernames, as last emitted.
    pub mention_names: String,
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
    /// Ready, going and too big (never started) in the open channel's tray.
    pub upload_counts: (i64, i64, i64),
    pub gifs: Vec<GifRow>,
    pub settings: SettingsView,
    pub admin: AdminView,
    pub search: super::search::SearchView,
    pub friends: super::FriendsView,
    pub apps: super::apps::AppsView,
    pub shop: super::shop::ShopView,
    pub board: super::board::BoardView,
    /// The saved favourites' `provider:id`.
    pub favorites: Vec<String>,
    pub extras: super::extras::ExtrasView,
    pub calls: super::calls::CallsView,
}

/// One step of a profile save, run in order; a failure stops the rest.
#[derive(Debug, Clone, PartialEq)]
pub enum ProfileStep {
    /// `PATCH /api/update-profile`.
    Patch(serde_json::Value),
    /// The custom status as the status picker sets it (`POST
    /// /api/presence`; `None` clears it), awaited so the profile read back
    /// after it has it.
    Status(Option<String>),
    Upload {
        asset: ProfileAsset,
        path: String,
        content_type: String,
    },
    DeleteBanner,
    DeleteNameplate,
    /// `PATCH /api/collectibles/apply` with the nameplate slot emptied
    /// (SettingsAccountTab.tsx `handleSave`: a removed collectible
    /// nameplate, or a plate that is not a managed upload).
    ClearNameplate {
        body: serde_json::Value,
    },
    /// Read the profile back.
    Refresh,
}

impl ProfileStep {
    /// Completes "Unable to …" when the step fails (SettingsAccountTab.tsx).
    pub fn action(&self) -> &'static str {
        match self {
            Self::Patch(_) => "save your profile",
            Self::Status(_) => "save your custom status",
            Self::Upload {
                asset: ProfileAsset::Avatar { .. },
                ..
            } => "upload your avatar",
            Self::Upload {
                asset: ProfileAsset::Nameplate,
                ..
            } => "upload your nameplate",
            Self::Upload { .. } => "upload your banner",
            Self::DeleteBanner => "remove your banner",
            Self::DeleteNameplate | Self::ClearNameplate { .. } => "remove your nameplate",
            Self::Refresh => "refresh your profile",
        }
    }
}

/// What the settings dialog loaded and is saving.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsState {
    pub email: Load<String>,
    pub sign_in_methods: Load<String>,
    pub sessions: Load<Vec<SessionInfo>>,
    pub shares: Load<Vec<ShareItem>>,
    pub display_name_saving: bool,
    pub display_name_error: String,
    /// The name being saved, applied once the server takes it.
    pub display_name_draft: Option<String>,
    /// The profile save's steps still to run (the running one is first).
    pub profile_steps: VecDeque<ProfileStep>,
    pub profile_error: String,
    pub profile_saved: u64,
    /// (level being saved, the level before it).
    pub media_filter_saving: Option<(String, Option<String>)>,
    pub media_filter_error: String,
    /// "Share my activity" being saved: the value before it.
    pub share_activity_saving: Option<Option<bool>>,
    pub share_activity_error: String,
    pub revoking_share: Option<String>,
    pub share_error: (String, String),
    /// `GET /api/billing/status`: ready with `enabled` on a 200; anything
    /// else (501 until Stripe is set up, offline) fails, and the purchase
    /// keys stay off.
    pub billing: Load<bool>,
}

/// The settings dialog's requests in flight.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct SettingsPending {
    pub email: Option<RequestId>,
    pub methods: Option<RequestId>,
    pub sessions: Option<RequestId>,
    pub shares: Option<RequestId>,
    pub display_name: Option<RequestId>,
    pub profile_step: Option<RequestId>,
    pub theme: Option<RequestId>,
    pub media_filter: Option<RequestId>,
    pub revoke: Option<RequestId>,
    pub share_activity: Option<RequestId>,
    pub billing: Option<RequestId>,
}

/// The request id each slot is waiting for. A result for any other id is stale.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Pending {
    /// Voice moves in flight.
    pub moves: std::collections::HashSet<RequestId>,
    pub me: Option<RequestId>,
    /// Our status writes (`POST /api/presence`) in flight.
    pub presence: std::collections::HashSet<RequestId>,
    /// The last request id when the last of them landed: a `/me` asked at
    /// or before it (or while one is in flight) can predate our status.
    pub presence_settled: u64,
    pub servers: Option<RequestId>,
    /// (request, server id)
    pub channels: Option<(RequestId, String)>,
    /// (request, channel id)
    pub page: Option<(RequestId, String)>,
    /// request → nonce of the send
    pub sends: HashMap<RequestId, String>,
    /// nonce → text of a toast reply to a channel that is not open. It has
    /// no outbox row, so this is all that is left of it if the send fails.
    pub toast_replies: HashMap<String, String>,
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
    /// request → the message the delete removed, and whether it was in the
    /// open list (a thread-only reply goes back to the thread alone)
    pub deletes: HashMap<RequestId, (Message, bool)>,
    pub dms: Option<RequestId>,
    /// (request, user id) of the DM being opened
    pub open_dm: Option<(RequestId, String)>,
    /// (request, user id) of the popout's profile
    pub profile: Option<(RequestId, String)>,
    /// request → user id of a hover's profile prefetch
    pub profile_prefetch: HashMap<RequestId, String>,
    /// request → (channel id, asked at ms, gateway generation then) of a
    /// hovered channel's page
    pub prefetch: HashMap<RequestId, (String, i64, u64)>,
    /// request → the reaction change to undo if it fails
    pub reactions: HashMap<RequestId, ReactionChange>,
    /// request → local id of the upload
    pub uploads: HashMap<RequestId, String>,
    /// request → nonce of the send waiting for its GIF
    pub gif_uploads: HashMap<RequestId, String>,
    /// The GIF page being fetched (true: the next page, false: a new query).
    pub gifs: Option<(RequestId, bool)>,
    /// The picker's start screen and search terms in flight.
    pub gif_extra: HashMap<RequestId, GifOp>,
    /// request → (file name shown, open it when saved)
    pub downloads: HashMap<RequestId, (String, bool)>,
    pub settings: SettingsPending,
    /// Server and channel administration.
    pub admin: HashMap<RequestId, super::admin::AdminOp>,
    /// The search in flight (a newer one replaces it).
    pub search: Option<super::search::SearchPending>,
    /// Friends and relationships.
    pub friends: HashMap<RequestId, super::friends::FriendsOp>,
    /// The voice apps.
    pub apps: HashMap<RequestId, super::apps::AppsOp>,
    /// The shop.
    pub shop: HashMap<RequestId, super::shop::ShopOp>,
    /// The open profile's board column.
    pub board: HashMap<RequestId, super::board::BoardOp>,
    /// GIF and clip favourites.
    pub favorites: HashMap<RequestId, super::favorites::FavoritesOp>,
    /// Camera backgrounds.
    pub backgrounds: HashMap<RequestId, super::backgrounds::BackgroundsOp>,
    /// The inbox, details tabs, voice status and the other extras.
    pub extras: HashMap<RequestId, super::extras::ExtrasOp>,
    /// TikTok and Instagram lookups.
    pub embeds: super::embeds::Pending,
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
    /// Over the attachment limit when it was added: never started, shown
    /// as the too-big card until removed.
    TooBig,
    /// The server answered 413 anyway (its limit changed): its message,
    /// which names the limit it holds to.
    Refused(String),
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
    /// `gifs`, `stickers`, `clips` or `memes`.
    pub media_type: String,
    /// `klipy` or `tenor` (Tenor has GIFs only).
    pub provider: String,
    /// The tiles are the saved favourites of `media_type`, not a feed.
    pub favorites: bool,
    pub items: Vec<GifItem>,
    pub next: Option<String>,
    pub loading: bool,
    pub error: String,
    /// The last "more" was refused with 429 (the web waits 5 s to retry).
    pub rate_limited: bool,
    /// The start screen shown (`provider:mediaType`), the categories and
    /// the Trending tile's GIF fetched per key (kept, as the web caches
    /// them), and whether the shown key's categories are loading.
    pub start_key: String,
    pub categories: HashMap<String, Vec<GifCategory>>,
    pub trending: HashMap<String, Option<GifItem>>,
    pub categories_loading: bool,
    /// Search terms for `terms_query` (autocomplete, then suggestions).
    pub terms_query: String,
    pub autocomplete: Vec<String>,
    pub suggestions: Vec<String>,
    /// Answers for `terms_query` so far (autocomplete, suggestions): the
    /// shown terms change once both are in, as the web awaits both.
    pub terms_next: [Option<Vec<String>>; 2],
    /// The feeds left this open (provider, media type and query): their
    /// tiles and cursor, shown again without a fetch when the tab or the
    /// search comes back, as the web keeps a cache per picker. Cleared
    /// when the picker opens.
    pub feeds: HashMap<String, (Vec<GifItem>, Option<String>)>,
}

/// A GIF picker request besides its pages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GifOp {
    /// The categories of a start key.
    Categories(String),
    /// The Trending tile's GIF of a start key.
    Trending(String),
    /// Terms for a query: autocomplete (true) or suggestions.
    Terms { query: String, autocomplete: bool },
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
    /// User id → where they are signed in, as presence last said
    /// (chat-reducer.ts `presencePlatformsByUserId`).
    pub platforms: HashMap<String, Vec<String>>,
    /// User id → what they are playing (chat-reducer.ts
    /// `gameActivityByUserId`): absent when not playing, offline, invisible
    /// or not sharing. Ours is what we report, not what the server echoes.
    pub games: HashMap<String, GameActivity>,
}

/// The platforms presence names, known ones only, in the indicators'
/// order (presence-platform.ts `normalizePresencePlatforms`).
pub fn presence_platforms<S: AsRef<str>>(values: &[S]) -> Vec<&'static str> {
    ["desktop", "mobile", "web"]
        .into_iter()
        .filter(|known| values.iter().any(|v| v.as_ref() == *known))
        .collect()
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

    /// Where someone is signed in, comma-joined (`desktop,web`): the live
    /// list when presence sent a non-empty one, else what their user row
    /// carried (UserPlatformIndicators.tsx).
    pub fn platforms_of(&self, user_id: &str, fallback: Option<&[String]>) -> String {
        let live = self.platforms.get(user_id).filter(|list| !list.is_empty());
        let list = live.map(Vec::as_slice).or(fallback).unwrap_or_default();
        presence_platforms(list).join(",")
    }

    /// What they are playing, if anything is shown.
    pub fn game_of(&self, user_id: &str) -> Option<&GameActivity> {
        self.games.get(user_id)
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

/// Our own game activity, and what the global gateway was told of it
/// (op 43, chat-gateway.ts `setGameActivity`). The server drops a socket's
/// activity when it closes, so every Ready and Resumed restates it.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct ActivityState {
    /// What local detection reports now, before sharing and invisible apply.
    pub own: Option<GameActivity>,
    /// The server takes op 43: its last `PRESENCE_LIST` entries carried
    /// `game_activity` (servers that predate it never send the field, and
    /// answer an unknown op with an Error frame).
    pub server: bool,
    /// Op 43 may be sent at all (`YAP_GAME_ACTIVITY_SEND=off` turns it off).
    pub enabled: bool,
    /// What this socket was last sent; `None` since the last Ready, Resumed
    /// or drop.
    pub sent: Option<Option<GameActivity>>,
    /// An activity was sent once: Ready and Resumed restate it, null too.
    pub used: bool,
}

/// The sign-in chain (me + servers, then channels, then the first page).
/// Phase `ready` is shown when it settles, as before.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Boot {
    pub me_done: bool,
    /// A servers result that arrived before `me`; applied right after it.
    pub early_servers: Option<Result<Vec<Server>, ApiError>>,
    /// `me` could not be reached this many times in a row (no network, the
    /// server down): the session is kept and asked again after a backoff.
    pub retries: u32,
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

/// A channel left at its newest end (`scrollPositions` "BOTTOM").
pub(super) const SCROLL_BOTTOM: &str = "BOTTOM";

/// Everything the client knows. Built by [`State::new`], changed only by
/// [`super::reduce`].
#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub(super) api_base: String,
    pub(super) local_offset: LocalOffset,
    pub(super) phase: Phase,
    pub(super) status: String,
    /// `status` holds a failed page of the open channel (a cached channel
    /// that is gone answers 403), cleared once a page loads.
    pub(super) page_status: bool,
    pub(super) connected: bool,
    /// The link retries the view shows (`ViewUpdate::Link`), across sessions.
    pub(super) link_shown: (u32, bool),
    pub(super) session: Option<Session>,
    pub(super) sign_in: Option<SignInId>,
    /// The user code the running sign-in shows (`XXXX-XXXX`), once Kova
    /// gave one: a `signin` link naming it polls at once.
    pub(super) sign_in_code: Option<String>,
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
    pub(super) activity: ActivityState,
    /// The user whose popout is open, and their full profile once fetched.
    pub(super) profile: Option<(String, Option<User>)>,
    /// Full profiles fetched this session (a popout, or a hover's
    /// prefetch), so a card opens with its banner before the refetch.
    pub(super) profile_cache: HashMap<String, User>,
    /// The servers shared with the popout's user, from their profile.
    /// The full profile failed to load: whose, and why (UserProfileModal's
    /// `profileError`).
    pub(super) profile_error: Option<(String, String)>,
    pub(super) profile_mutuals: Option<(
        String,
        yap_protocol::rest::MutualServers,
        yap_protocol::rest::MutualFriends,
    )>,
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
    /// Where each channel was left (useChatArea.ts `scrollPositions`): the
    /// top message on screen, or [`SCROLL_BOTTOM`] at the newest end.
    pub(super) scroll_positions: HashMap<String, String>,
    /// Channels whose position came from a jump: scrolling the jump's
    /// window does not move it (`jumpAnchors`).
    pub(super) jump_anchors: HashSet<String>,
    pub(super) voice: Option<VoiceCall>,
    /// The warm standby, if one is held.
    pub(super) standby: Option<Standby>,
    /// Standby sessions are allowed (`YAP_VOICE_STANDBY`).
    pub(super) standby_enabled: bool,
    /// The window is on screen (not minimised, not in the tray).
    pub(super) window_shown: bool,
    /// Hidden past the grace: no standby (useVoiceStandbyActive).
    pub(super) standby_paused: bool,
    /// Counts hides and shows; a grace timer for an older one is ignored.
    pub(super) window_generation: u64,
    /// This device's push token ([`super::Command::RegisterPush`]), kept
    /// across sessions.
    pub(super) push: Option<PushRegistration>,
    /// It is registered (or being registered) for the current session's
    /// account.
    pub(super) push_registered: bool,
    /// The registration in flight: a failure clears `push_registered`, and
    /// the next gateway Ready tries again.
    pub(super) push_request: Option<super::RequestId>,
    /// A phone app is in the background ([`super::Command::Suspend`]).
    pub(super) suspended: bool,
    /// Counts suspends and resumes; a grace timer for an older one is ignored.
    pub(super) suspend_generation: u64,
    /// The gateway was closed for the background and reconnects on resume.
    pub(super) gateway_suspended: bool,
    /// The pending roster check after a gateway Ready or Resumed, and the
    /// channels whose roster the server sent since.
    pub(super) voice_roster_check: u64,
    pub(super) voice_confirmed: HashSet<String>,
    /// DM calls, moves, stickers, the voice details.
    pub(super) calls: super::calls::CallsState,
    pub(super) voice_prefs: VoicePrefs,
    /// Why the last call failed; cleared by the next join.
    pub(super) voice_error: String,
    /// Who is in each voice channel, by channel id (from the gateway).
    pub(super) voice_channels: HashMap<String, Vec<Occupant>>,
    /// When each occupied voice channel's call started (ms): the earliest
    /// of what the server said, what we had and the first join.
    pub(super) voice_started_at: HashMap<String, i64>,
    /// The voice channel whose view is open (landing or call), if any.
    pub(super) voice_view: Option<String>,
    /// That view's text chat is shown beside it (it is then the active
    /// channel).
    pub(super) voice_chat: bool,
    /// The server whose channel tree is shown (the last one that loaded).
    pub(super) tree_server: Option<String>,
    /// A channel to open once its server's tree answers (a toast click).
    pub(super) wanted_channel: Option<String>,
    /// A DM to open once the DM list answers (a phone notification's tap
    /// at launch, before the list is in).
    pub(super) wanted_dm: Option<String>,
    /// Messages that were toasted (the notification for one does not toast).
    pub(super) toasted: Toasted,
    pub(super) settings: SettingsState,
    pub(super) admin: super::AdminState,
    /// Threads, pins and jumps.
    pub(super) panels: super::threads::PanelsState,
    pub(super) search: super::SearchState,
    /// Relationships (friends, requests, blocked).
    pub(super) friends: super::FriendsState,
    pub(super) invites: super::InviteState,
    pub(super) apps: super::apps::AppsState,
    /// The collectibles shop.
    pub(super) shop: super::shop::ShopState,
    /// The open full profile's board, tabs and editor.
    pub(super) board: super::board::BoardState,
    /// GIF and clip favourites.
    pub(super) favorites: super::favorites::FavoritesState,
    /// The account's camera backgrounds.
    pub(super) backgrounds: super::backgrounds::BackgroundsState,
    pub(super) extras: super::extras::ExtrasState,
    pub(super) deletion: super::deletion::DeletionState,
    /// TikTok and Instagram lookups, by link.
    pub(super) hydrations: super::embeds::Hydrations,
    /// Links the composer asked the server to resolve early.
    pub(super) prefetched: super::prefetch::Prefetched,
    pub(super) view: Projection,
    pub(super) pending: Pending,
    pub(super) boot: Option<Boot>,
    pub(super) first_page_shown: bool,
    pub(super) startup: Startup,
    /// The view came from the cache and is being revalidated.
    pub(super) refreshing: bool,
    /// Channels left recently, shown at once when they open again.
    pub(super) warm: WarmCache,
    /// Counts fresh gateway sessions (Ready, not Resumed): warm logs from an
    /// older one may have missed events.
    pub(super) gateway_generation: u64,
    /// The open log's (generation, confirmed at ms), as a warm entry keeps it.
    pub(super) open_confirmed: (u64, i64),
    /// When the open channel's page was asked for (epoch ms, 0: unknown).
    pub(super) page_asked_ms: i64,
    /// The wall clock of the command being reduced (`Input::UserAt`).
    pub(super) now_ms: Option<i64>,
    /// The latest wall clock any command brought (0 before the first): what
    /// plan expiries and badge months are read against between commands.
    pub(super) clock_ms: i64,
    /// The last text channel opened per server, and the last DM under
    /// `@me` (useChatPageLogic.ts `lastActiveChannels`).
    pub(super) last_channels: HashMap<String, String>,
    /// The sidebar's collapsed categories (ChannelSidebar.tsx
    /// `collapsedCategories`), kept per user in the snapshot.
    pub(super) collapsed: BTreeSet<String>,
    /// The open channel's rows came from the disk window (not yet confirmed).
    pub(super) disk_shown: bool,
    /// The open channel landed at its end when its cached rows showed: the
    /// refetch places no divider there in a focused window (as `on_page`
    /// does for a channel left at its end).
    pub(super) open_at_end: bool,
    /// The window was focused when the open channel was picked. A channel
    /// left at its end shows no divider only when it was opened in view
    /// and is still in view when its rows come: a phone picks it from the
    /// drawer, over which it then slides, before its page answers.
    pub(super) open_focused: bool,
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
            page_status: false,
            connected: false,
            link_shown: (0, false),
            session: None,
            sign_in: None,
            sign_in_code: None,
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
            activity: ActivityState {
                enabled: true,
                ..ActivityState::default()
            },
            profile: None,
            profile_cache: HashMap::new(),
            profile_mutuals: None,
            profile_error: None,
            divider: None,
            seen_floor: None,
            seen_from: None,
            outbox: HashMap::new(),
            uploads: Vec::new(),
            gifs: GifBrowse::default(),
            focused: true,
            at_bottom: false,
            scroll_positions: HashMap::new(),
            jump_anchors: HashSet::new(),
            voice: None,
            standby: None,
            standby_enabled: true,
            window_shown: true,
            standby_paused: false,
            window_generation: 0,
            push: None,
            push_registered: false,
            push_request: None,
            suspended: false,
            suspend_generation: 0,
            gateway_suspended: false,
            voice_roster_check: 0,
            voice_confirmed: HashSet::new(),
            calls: Default::default(),
            voice_prefs: VoicePrefs::default(),
            voice_error: String::new(),
            voice_channels: HashMap::new(),
            voice_started_at: HashMap::new(),
            voice_view: None,
            voice_chat: false,
            tree_server: None,
            wanted_channel: None,
            wanted_dm: None,
            toasted: Toasted::default(),
            settings: SettingsState::default(),
            admin: super::AdminState::default(),
            panels: Default::default(),
            search: super::SearchState::default(),
            friends: super::FriendsState::default(),
            invites: super::InviteState::default(),
            apps: super::apps::AppsState::new(),
            shop: Default::default(),
            board: Default::default(),
            favorites: Default::default(),
            backgrounds: Default::default(),
            extras: Default::default(),
            deletion: Default::default(),
            hydrations: Default::default(),
            prefetched: Default::default(),
            view: Projection::default(),
            pending: Pending::default(),
            boot: None,
            first_page_shown: false,
            startup: Startup::Idle,
            refreshing: false,
            warm: WarmCache::default(),
            gateway_generation: 0,
            open_confirmed: (0, 0),
            page_asked_ms: 0,
            now_ms: None,
            clock_ms: 0,
            last_channels: HashMap::new(),
            collapsed: BTreeSet::new(),
            disk_shown: false,
            open_at_end: false,
            open_focused: true,
            next_id: 0,
        }
    }

    /// Allows hidden standby sessions on occupied voice landings (on by
    /// default).
    pub fn with_voice_standby(mut self, enabled: bool) -> Self {
        self.standby_enabled = enabled;
        self
    }

    /// Whether op 43 (our game activity) may go to a server that takes it
    /// (`YAP_GAME_ACTIVITY_SEND=off`: never).
    pub fn with_activity_send(mut self, enabled: bool) -> Self {
        self.activity.enabled = enabled;
        self
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

    /// The latest wall clock a command brought (epoch ms; 0 before any).
    pub fn clock_ms(&self) -> i64 {
        self.now_ms.unwrap_or(self.clock_ms)
    }

    /// The signed-in user's plan (none: free, as before `users/me`).
    pub fn my_entitlement(&self) -> Option<&yap_protocol::tiers::Entitlement> {
        self.me().and_then(crate::entitlements::of)
    }

    /// The signed-in user's limit for `kind` (`clientUploadLimitFor`).
    pub fn upload_limit(&self, kind: crate::limits::Kind) -> u64 {
        crate::limits::client_upload_limit_for(self.my_entitlement(), kind, self.clock_ms())
    }

    /// The signed-in user's attachment limit: 500 MB with `bigUploads`,
    /// 25 MB without.
    pub fn attachment_limit(&self) -> u64 {
        self.upload_limit(crate::limits::Kind::Attachment)
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

    /// Channels left recently and kept warm.
    pub fn warm(&self) -> &WarmCache {
        &self.warm
    }

    /// Fresh gateway sessions so far.
    pub fn gateway_generation(&self) -> u64 {
        self.gateway_generation
    }

    /// The sidebar's collapsed categories.
    pub fn collapsed_categories(&self) -> &BTreeSet<String> {
        &self.collapsed
    }

    /// The channel a server (or `@me`) opens on next.
    pub fn last_channel(&self, server_key: &str) -> Option<&str> {
        self.last_channels.get(server_key).map(String::as_str)
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
        self.channel_on_screen() != Some(channel_id) && self.has_unread(channel_id)
    }

    /// The channel the user looks at: an open voice view's (ChatPageClient.tsx
    /// makes it the active channel), else the active one.
    pub(super) fn channel_on_screen(&self) -> Option<&str> {
        self.voice_view
            .as_deref()
            .or(self.active_channel.as_deref())
    }

    /// The window is focused and was when the open channel was picked: a
    /// channel left at its end then opens there with no divider
    /// (useChatArea.ts `landingAtBottom && windowFocused`).
    pub(super) fn in_view_since_open(&self) -> bool {
        self.focused && self.open_focused
    }

    /// The active channel's messages are on screen: no voice view covers
    /// them, or they are that view's own chat beside it.
    pub(super) fn chat_on_screen(&self) -> bool {
        match self.voice_view.as_deref() {
            None => true,
            Some(id) => self.voice_chat && self.active_channel.as_deref() == Some(id),
        }
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
