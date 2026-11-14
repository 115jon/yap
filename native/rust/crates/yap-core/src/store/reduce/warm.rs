//! Instant channel and server switches: the warm cache of channels left
//! recently, shown at once and revalidated (stale-while-revalidate), the
//! window kept on disk for a channel that is not warm, prefetches of a
//! hovered channel, live events applied to warm logs, and the channel each
//! server opens on (useChatPageLogic.ts `lastActiveChannels`).

use yap_protocol::rest::{ChannelType, Message, MessagePatch};

use super::{is_pending, Effects};
use crate::api::ApiError;
use crate::store::state::{
    MessageLog, Outgoing, ServerChannels, State, SCROLL_BOTTOM, WARM_FRESH_MS, WARM_WINDOW,
};
use crate::store::time::{is_after, parse_rfc3339};
use crate::store::view::{TreeKind, ViewDiff, ViewUpdate};
use crate::store::{
    ApiRequest, ApiResponse, CacheWrite, Effect, HttpReason, Page, RequestId, SwitchSource,
};

/// Prefetches in flight at most.
pub const MAX_PREFETCHES: usize = 2;

/// A warm log keeps at most this many messages when the channel was left
/// scrolled up (down to where it was left, plus a little).
const WARM_WINDOW_MAX: usize = 400;

/// The `@me` key of [`State::last_channels`]: the last DM.
pub const HOME_KEY: &str = "@me";

/// The older messages of `old` a newest page keeps (they are older than
/// the page's oldest one): only when the page joins up with them, by
/// sharing a message or by not starting after `old`'s newest. A page
/// entirely newer than `old` (more than a page arrived since `old` was
/// confirmed) would leave the messages between them missing: none are
/// kept then.
pub(super) fn older_kept<'a>(old: &'a MessageLog, page: &[Message]) -> Vec<&'a Message> {
    let Some(oldest) = page
        .iter()
        .map(|m| m.created_at.as_str())
        .min_by_key(|at| parse_rfc3339(at))
    else {
        return Vec::new();
    };
    let overlaps = page.iter().any(|m| old.get(&m.id).is_some());
    let joins = overlaps
        || old
            .newest_sent()
            .is_none_or(|newest| !is_after(oldest, &newest.created_at));
    if !joins {
        return Vec::new();
    }
    old.iter()
        .filter(|m| !is_pending(m) && is_after(oldest, &m.created_at))
        .collect()
}

/// Where a log opens: the saved message (`restore`), or the end; `None`
/// when the saved message is not in it (its window must load).
fn landing(log: &MessageLog, saved: Option<&str>) -> Option<(String, bool)> {
    match saved {
        None => Some((String::new(), false)),
        Some(SCROLL_BOTTOM) => Some((String::new(), true)),
        Some(id) => {
            log.get(id)?;
            if log.newest_sent().is_some_and(|m| m.id == id) {
                Some((String::new(), true))
            } else {
                Some((id.to_owned(), false))
            }
        }
    }
}

impl State {
    /// Channels never evicted from the warm cache: the open one and the
    /// call's (its chat sits beside the call).
    fn warm_protected(&self) -> Vec<String> {
        [
            self.active_channel.clone(),
            self.voice.as_ref().map(|call| call.channel_id.clone()),
            self.voice_view.clone(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }

    /// The open log leaves the screen: kept warm (down to where it was
    /// left, when that is above its newest window).
    pub(super) fn stash_open_log(&mut self) {
        let Some(log) = self.messages.take() else {
            return;
        };
        // A channel no tree or DM list has any more (deleted, access lost)
        // could never open again.
        if self.find_channel(&log.channel_id).is_none() && self.dm(&log.channel_id).is_none() {
            return;
        }
        let keep = self
            .scroll_positions
            .get(&log.channel_id)
            .and_then(|id| log.position(id))
            .map_or(WARM_WINDOW, |at| {
                (log.len() - at + 10).clamp(WARM_WINDOW, WARM_WINDOW_MAX)
            });
        let (generation, confirmed_at_ms) = self.open_confirmed;
        let protect = self.warm_protected();
        let protect: Vec<&str> = protect.iter().map(String::as_str).collect();
        self.warm
            .put(log, keep, generation, confirmed_at_ms, &protect);
        self.open_confirmed = (0, 0);
        self.disk_shown = false;
    }

    /// A warm log may show without its refetch: a server channel (their
    /// events reach every member; a DM's edits and deletes do not)
    /// confirmed in this gateway session less than [`WARM_FRESH_MS`] ago.
    /// Unknown clocks are never fresh.
    pub(super) fn warm_fresh(
        &self,
        channel_id: &str,
        generation: u64,
        confirmed_at_ms: i64,
    ) -> bool {
        let Some(now) = self.now_ms else {
            return false;
        };
        let ready = self.session.as_ref().is_some_and(|s| s.gateway.ready);
        ready
            && confirmed_at_ms > 0
            && now >= confirmed_at_ms
            && now - confirmed_at_ms < WARM_FRESH_MS
            && generation == self.gateway_generation
            && self.dm(channel_id).is_none()
    }

    /// Sends in flight (or failed) of a channel, at the end of its log.
    fn append_outgoing(&self, log: &mut MessageLog) {
        let mut outgoing: Vec<&Outgoing> = self
            .outbox
            .values()
            .filter(|o| o.message.channel_id == log.channel_id)
            .collect();
        outgoing.sort_by(|a, b| a.message.created_at.cmp(&b.message.created_at));
        for o in outgoing {
            if log.get(&o.message.id).is_none() {
                log.upsert(o.message.clone());
            }
        }
    }

    /// Puts rows that came from a cache on screen as the open channel's:
    /// where it was left, the "New" divider (as `on_page` places it), and
    /// the end of the skeleton.
    fn show_cached(
        &mut self,
        fx: &mut Effects,
        mut log: MessageLog,
        restore: String,
        at_end: bool,
    ) {
        self.append_outgoing(&mut log);
        self.open_at_end = at_end;
        if !(at_end && self.in_view_since_open()) {
            self.divider = self.divider_for(&log);
        }
        self.seen_floor = log.newest_sent().map(|m| m.created_at.clone());
        self.messages = Some(log);
        let rows = self.message_rows();
        fx.view(ViewUpdate::ScrollRestore(restore));
        fx.messages(ViewDiff::Reset(rows));
        if self.divider.is_some() {
            self.emit_unread(fx);
        }
        if !std::mem::replace(&mut self.first_page_shown, true) {
            fx.push(Effect::StartupMark("messages shown"));
        }
    }

    /// Opens `id` from the warm cache: its rows at once, no skeleton. The
    /// caller then asks for the page unless it is fresh (`Some(true)`); its
    /// row diff makes the refetch invisible. `None` when it is not warm, a
    /// jump into it waits, or it was left scrolled to a message the warm
    /// log no longer has.
    pub(super) fn open_warm(&mut self, fx: &mut Effects, id: &str) -> Option<bool> {
        let jumping = self
            .panels
            .jump
            .wanted
            .as_ref()
            .is_some_and(|(channel, _)| channel == id);
        if jumping {
            return None;
        }
        let saved = self.scroll_positions.get(id).map(String::as_str);
        let (restore, at_end) = self.warm.get(id).and_then(|e| landing(&e.log, saved))?;
        let entry = self.warm.take(id)?;
        let fresh = self.warm_fresh(id, entry.generation, entry.confirmed_at_ms);
        self.open_confirmed = (entry.generation, entry.confirmed_at_ms);
        self.disk_shown = false;
        fx.push(Effect::SwitchMark {
            channel_id: id.to_owned(),
            source: Some(SwitchSource::Warm),
            rows: entry.log.len(),
        });
        self.show_cached(fx, entry.log, restore, at_end);
        Some(fresh)
    }

    /// A channel with nothing warm: its window kept on disk, if any, shows
    /// while its page loads.
    pub(super) fn load_window(&mut self, fx: &mut Effects, channel_id: &str) {
        let jumping = self
            .panels
            .jump
            .wanted
            .as_ref()
            .is_some_and(|(channel, _)| channel == channel_id);
        if jumping || self.pending.page.is_none() {
            return;
        }
        if let Some(me) = self.me() {
            fx.push(Effect::LoadWindow {
                user_id: me.id.clone(),
                channel_id: channel_id.to_owned(),
            });
        }
    }

    /// The disk window of the channel being opened: shown when the page
    /// has not answered yet and the list is still empty. The page then
    /// revalidates it like the launch's cached rows.
    pub(super) fn on_window(&mut self, fx: &mut Effects, channel_id: &str, messages: Vec<Message>) {
        if messages.is_empty() || self.active_channel.as_deref() != Some(channel_id) {
            return;
        }
        let waiting = self
            .pending
            .page
            .as_ref()
            .is_some_and(|(_, channel)| channel == channel_id);
        let empty = self.messages.as_ref().is_some_and(|log| {
            log.channel_id == channel_id && log.is_empty() && !log.has_more_after
        });
        if !waiting || !empty || self.restoring() {
            return;
        }
        let mut log = MessageLog::new(channel_id);
        // Unknown until the page answers; it keeps what is older.
        log.has_more_before = true;
        for message in messages.into_iter().filter(|m| m.channel_id == channel_id) {
            self.remember_author(&message);
            log.upsert(message);
        }
        let saved = self.scroll_positions.get(channel_id).map(String::as_str);
        let Some((restore, at_end)) = landing(&log, saved) else {
            return;
        };
        self.disk_shown = true;
        self.open_confirmed = (0, 0);
        fx.push(Effect::SwitchMark {
            channel_id: channel_id.to_owned(),
            source: Some(SwitchSource::Disk),
            rows: log.len(),
        });
        fx.view(ViewUpdate::PageLoading(false));
        self.show_cached(fx, log, restore, at_end);
        self.emit_history(fx);
    }

    /// `PrefetchChannel`: the newest page of a channel the pointer rests
    /// on, into the warm cache only. Skipped when it is open, warm and
    /// fresh, already asked for, or [`MAX_PREFETCHES`] are in flight (the
    /// runner also drops one while the network is busy).
    pub(super) fn prefetch_channel(&mut self, fx: &mut Effects, id: &str) {
        if self.signed_in().is_none() || id.is_empty() || self.active_channel.as_deref() == Some(id)
        {
            return;
        }
        let text = self.dm(id).is_some()
            || self
                .find_channel(id)
                .is_some_and(|c| c.channel_type == ChannelType::Text);
        if !text {
            return;
        }
        let asked = &self.pending.prefetch;
        if asked.len() >= MAX_PREFETCHES || asked.values().any(|(channel, ..)| channel == id) {
            return;
        }
        if let Some(entry) = self.warm.get(id) {
            if self.warm_fresh(id, entry.generation, entry.confirmed_at_ms) {
                return;
            }
        }
        let request = ApiRequest::Messages {
            channel_id: id.to_owned(),
            before: None,
        };
        // When and in which gateway session it was asked: a page asked
        // before a gap is not confirmed in the session after it.
        let asked = (
            id.to_owned(),
            self.now_ms.unwrap_or(0),
            self.gateway_generation,
        );
        if let Some(request) = self.http_for(fx, request, HttpReason::Prefetch) {
            self.pending.prefetch.insert(request, asked);
        }
    }

    pub(super) fn on_prefetched(
        &mut self,
        fx: &mut Effects,
        (channel_id, asked_ms, generation): (String, i64, u64),
        result: Result<ApiResponse, ApiError>,
    ) {
        let Page {
            messages,
            has_more_before,
        } = match result {
            Ok(ApiResponse::Messages(page)) => page,
            Err(error) if error.is_unauthorized() => return self.fail(fx, &error),
            // Busy, offline or refused: the open path fetches it anyway.
            _ => return,
        };
        // Opened meanwhile: its own page is on the way.
        if self.active_channel.as_deref() == Some(channel_id.as_str()) {
            return;
        }
        let mut log = MessageLog::new(&channel_id);
        log.has_more_before = has_more_before;
        if let Some(old) = self.warm.get(&channel_id) {
            for message in older_kept(&old.log, &messages) {
                log.upsert(message.clone());
                log.has_more_before = old.log.has_more_before;
            }
        }
        for message in messages {
            self.remember_author(&message);
            log.upsert(message);
        }
        // Memory only: the disk keeps the windows of channels opened (a
        // sweep down the sidebar must not push those out).
        let protect = self.warm_protected();
        let protect: Vec<&str> = protect.iter().map(String::as_str).collect();
        self.warm
            .put(log, WARM_WINDOW, generation, asked_ms, &protect);
    }

    /// [`WarmCache::update`] with the channels never evicted.
    fn warm_update<R>(
        &mut self,
        channel_id: &str,
        change: impl FnOnce(&mut MessageLog) -> R,
    ) -> Option<R> {
        let protect = self.warm_protected();
        let protect: Vec<&str> = protect.iter().map(String::as_str).collect();
        self.warm.update(channel_id, &protect, change)
    }

    /// A prefetch answer, if `id` is one.
    pub(super) fn route_prefetch(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(asked) = self.pending.prefetch.remove(&id) else {
            return false;
        };
        if let Some(result) = result.take() {
            self.on_prefetched(fx, asked, result);
        }
        true
    }

    // Live events on warm logs

    /// A message for a warm channel: replaces its copy, or goes in at the
    /// newest end (anything older than that is not in its window).
    pub(super) fn warm_upsert(&mut self, fx: &mut Effects, message: Message) {
        let channel = message.channel_id.clone();
        let changed = self.warm_update(&channel, |log| {
            let known = log.get(&message.id).is_some();
            let newest = log
                .newest_sent()
                .is_none_or(|m| !is_after(&m.created_at, &message.created_at));
            if known || newest {
                log.upsert(message);
            }
            known || newest
        });
        if changed == Some(true) {
            self.persist_warm(fx, &channel);
        }
    }

    pub(super) fn warm_patch(&mut self, fx: &mut Effects, patch: &MessagePatch) {
        let Some(channel) = patch
            .channel_id
            .clone()
            .or_else(|| self.warm.channel_of(&patch.id))
        else {
            return;
        };
        if self.warm_update(&channel, |log| log.patch(patch).is_some()) == Some(true) {
            self.persist_warm(fx, &channel);
        }
    }

    pub(super) fn warm_delete(&mut self, fx: &mut Effects, channel: Option<&str>, id: &str) {
        let Some(channel) = channel
            .map(str::to_owned)
            .or_else(|| self.warm.channel_of(id))
        else {
            return;
        };
        if self.warm_update(&channel, |log| log.remove(id).is_some()) == Some(true) {
            self.persist_warm(fx, &channel);
        }
    }

    pub(super) fn warm_react(
        &mut self,
        fx: &mut Effects,
        channel: Option<&str>,
        (id, emoji, user_id): (&str, &str, &str),
        add: bool,
    ) {
        let Some(channel) = channel
            .map(str::to_owned)
            .or_else(|| self.warm.channel_of(id))
        else {
            return;
        };
        let changed =
            self.warm_update(&channel, |log| log.react(id, emoji, user_id, add).is_some());
        if changed == Some(true) {
            self.persist_warm(fx, &channel);
        }
    }

    /// A warm log's newest window, to disk (the writer coalesces).
    fn persist_warm(&self, fx: &mut Effects, channel_id: &str) {
        if let Some(entry) = self.warm.get(channel_id) {
            self.persist_log(fx, &entry.log);
        }
    }

    /// A log's newest window to disk. Not a jump's window, and not while a
    /// reaction on one of its messages is unconfirmed: the optimistic chip
    /// must not reach the cache, where a refusal could not undo it.
    pub(super) fn persist_log(&self, fx: &mut Effects, log: &MessageLog) {
        if log.has_more_after {
            return;
        }
        if self
            .pending
            .reactions
            .values()
            .any(|change| log.get(&change.message_id).is_some())
        {
            return;
        }
        let sent: Vec<&Message> = log.iter().filter(|m| !is_pending(m)).collect();
        let skip = sent.len().saturating_sub(yap_cache::MESSAGE_WINDOW);
        self.persist(
            fx,
            CacheWrite::Messages {
                channel_id: log.channel_id.clone(),
                messages: sent.into_iter().skip(skip).cloned().collect(),
            },
        );
    }

    // Eviction

    /// A server's tree is replaced: channels it no longer has (deleted, or
    /// no longer visible) leave the caches.
    pub(super) fn forget_lost_channels(&mut self, server_id: &str, list: &ServerChannels) {
        let Some(old) = self.channels.get(server_id) else {
            return;
        };
        let lost: Vec<String> = old
            .channels
            .iter()
            .filter(|c| list.get(&c.id).is_none())
            .map(|c| c.id.clone())
            .collect();
        for id in lost {
            self.forget_channel(&id);
        }
    }

    /// Every cache of a channel goes (deleted, access lost, server left).
    pub(super) fn forget_channel(&mut self, channel_id: &str) {
        self.warm.remove(channel_id);
        self.forget_channel_extras(channel_id);
    }

    /// A server is gone: its channels leave the caches.
    pub(super) fn forget_server_channels(&mut self, server_id: &str) {
        let ids: Vec<String> = self
            .channels
            .get(server_id)
            .map(|list| list.channels.iter().map(|c| c.id.clone()).collect())
            .unwrap_or_default();
        for id in ids {
            self.forget_channel(&id);
        }
        self.last_channels.remove(server_id);
    }

    /// The DM list was replaced: warm DMs no longer in it go.
    pub(super) fn forget_lost_dms(&mut self) {
        let lost: Vec<String> = self
            .warm
            .channel_ids()
            .filter(|id| self.find_channel(id).is_none() && self.dm(id).is_none())
            .map(str::to_owned)
            .collect();
        for id in lost {
            self.forget_channel(&id);
        }
    }

    // The channel each server opens on

    /// The text channel opened last in `server_id`, if `list` still has it.
    pub(super) fn remembered_channel(
        &self,
        server_id: &str,
        list: &ServerChannels,
    ) -> Option<String> {
        let id = self.last_channels.get(server_id)?;
        list.get(id)
            .filter(|c| c.channel_type == ChannelType::Text)
            .map(|c| c.id.clone())
    }

    /// `id` was opened: its server (or `@me` for a DM) opens on it next
    /// time. Text channels and DMs only (useChatPageLogic.ts).
    pub(super) fn remember_channel(&mut self, fx: &mut Effects, id: &str) {
        let key = if self.dm(id).is_some() {
            HOME_KEY.to_owned()
        } else {
            let text = self
                .find_channel(id)
                .is_some_and(|c| c.channel_type == ChannelType::Text);
            match self.server_of(id).filter(|_| text) {
                Some(server) => server,
                None => return,
            }
        };
        if self.last_channels.get(&key).map(String::as_str) == Some(id) {
            return;
        }
        self.last_channels.insert(key.clone(), id.to_owned());
        self.persist(
            fx,
            CacheWrite::LastChannel {
                server_id: key,
                channel_id: id.to_owned(),
            },
        );
    }

    /// A server whose tree is loaded: the tree, the members and its
    /// channel at once, in one batch (the tree's fetch revalidates them).
    pub(super) fn show_known_server(&mut self, fx: &mut Effects, server_id: &str) {
        let Some(list) = self.channels.get(server_id) else {
            return;
        };
        // A toast click or a jump waits for a channel this tree has.
        let wanted = self
            .wanted_channel
            .clone()
            .filter(|id| list.get(id).is_some());
        // The server already shown, with one of its text channels open:
        // nothing moves (as `on_channels` keeps it).
        let kept = self
            .active_channel
            .as_deref()
            .and_then(|id| list.get(id))
            .filter(|c| c.channel_type == ChannelType::Text)
            .map(|c| (c.id.clone(), c.name.clone()));
        let target = wanted
            .clone()
            .or_else(|| self.remembered_channel(server_id, list))
            .or_else(|| list.first_text().map(|c| c.id.clone()));
        if wanted.is_some() {
            self.wanted_channel = None;
        }
        let same_tree = self.tree_server.as_deref() == Some(server_id);
        self.tree_server = Some(server_id.to_owned());
        if same_tree {
            self.sync_tree(fx);
        } else {
            self.reset_tree(fx);
        }
        if let (None, Some((id, name))) = (&wanted, kept) {
            self.emit_active_channel(fx, id, name);
            self.sync_members(fx);
            return;
        }
        match target {
            Some(id) => self.select_channel(fx, &id),
            None => self.clear_channel(fx),
        }
        self.sync_members(fx);
    }

    /// The per-channel panel caches (pins, the details' media tabs) of a
    /// channel that is gone.
    pub(super) fn forget_channel_extras(&mut self, channel_id: &str) {
        self.panels.pins_cache.retain(|kept, _| kept != channel_id);
        self.extras
            .tab_cache
            .retain(|(kept, _), _| kept != channel_id);
    }

    /// A fresh gateway session or a sign-out: the panel caches may have
    /// missed events (or belong to the last account).
    pub(super) fn forget_volatile_caches(&mut self) {
        self.panels.pins_cache.clear();
        self.extras.tab_cache.clear();
    }

    // Keyboard navigation

    /// A channel or DM the unread keys stop on.
    fn nav_unread(&self, id: &str) -> bool {
        self.is_unread(id) || self.channel_mentions(id) > 0 || self.dm_mentions(id) > 0
    }

    /// Alt+Up/Down: the previous or next text channel in the sidebar's
    /// order (the DM list at Home), wrapping; `unread`: the previous or
    /// next unread one (nothing when there is none). Voice channels are
    /// passed over: opening one would join it.
    pub(super) fn navigate_channel(&mut self, fx: &mut Effects, forward: bool, unread: bool) {
        if self.signed_in().is_none() {
            return;
        }
        let order: Vec<String> = if self.home {
            self.dms.iter().map(|dm| dm.id.clone()).collect()
        } else {
            self.view
                .tree
                .iter()
                .filter(|row| row.kind == TreeKind::Channel && !row.voice)
                .map(|row| row.id.clone())
                .collect()
        };
        let n = order.len();
        if n == 0 {
            return;
        }
        let here = self
            .active_channel
            .as_deref()
            .and_then(|id| order.iter().position(|c| c == id));
        // From nowhere, the first step lands on the first (or last) one.
        let start = here.unwrap_or(if forward { n - 1 } else { 0 });
        let reach = if here.is_some() { n - 1 } else { n };
        for step in 1..=reach {
            let at = if forward {
                (start + step) % n
            } else {
                (start + n - step % n) % n
            };
            if unread && !self.nav_unread(&order[at]) {
                continue;
            }
            let id = order[at].clone();
            return self.select_channel(fx, &id);
        }
    }

    /// Ctrl+Alt+Up/Down: the previous or next server in the rail, Home
    /// first (its last DM), wrapping.
    pub(super) fn navigate_server(&mut self, fx: &mut Effects, forward: bool) {
        if self.signed_in().is_none() {
            return;
        }
        let mut order: Vec<Option<String>> = vec![None];
        order.extend(self.servers.iter().map(|s| Some(s.id.clone())));
        let n = order.len();
        let here = if self.home {
            0
        } else {
            self.active_server
                .as_ref()
                .and_then(|id| order.iter().position(|s| s.as_ref() == Some(id)))
                .unwrap_or(0)
        };
        let at = if forward {
            (here + 1) % n
        } else {
            (here + n - 1) % n
        };
        match order[at].clone() {
            Some(server) => self.select_server(fx, server),
            None => self.select_home_last(fx),
        }
    }

    /// The rail's Home key: the last DM, when it is still listed.
    pub(super) fn select_home_last(&mut self, fx: &mut Effects) {
        let last = self
            .last_channels
            .get(HOME_KEY)
            .filter(|id| self.dm(id).is_some())
            .cloned();
        match last {
            Some(id) if !self.home => self.select_dm(fx, &id),
            _ => self.select_home(fx),
        }
    }
}
