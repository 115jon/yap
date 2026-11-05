//! Threads, pins and jumps (`store/threads.rs`): the thread panel's load,
//! replies and live rows (ThreadSidebar.tsx), pin and unpin with the pins
//! list (chat-actions.ts `pinMessage`, `loadPins`; chat-reducer.ts
//! `PIN_MESSAGE`), reply counts (chat-reducer.ts `APPEND_MESSAGE`,
//! `DELETE_MESSAGE`), and jump to message with its detached window
//! (useChatArea.ts `handleJumpToMessage`, `handleLoadAfter`,
//! `handleJumpToPresent`).

use std::cmp::Reverse;

use yap_protocol::rest::Message;

use super::{is_pending, Effects};
use crate::admin::{has_any_permission, member_permissions, perm};
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::store::state::{pending_id, MessageLog, Outgoing, State};
use crate::store::threads::{PanelOp, PanelsView, ThreadState};
use crate::store::time::parse_rfc3339;
use crate::store::view::{self, Load, MessageRow, RowContext, ViewDiff, ViewUpdate};
use crate::store::{
    ApiRequest, ApiResponse, Dispatch, PinsCommand, RequestId, ThreadCommand, ThreadRequest,
    ThreadResponse,
};

fn by_time(message: &Message) -> Option<i64> {
    parse_rfc3339(&message.created_at)
}

impl State {
    /// The open channel's list does not reach the newest message (a jump's
    /// window): nothing is marked read and live messages are not appended
    /// until it does (useChatArea.ts `isDetached`).
    pub(super) fn detached(&self) -> bool {
        self.messages.as_ref().is_some_and(|log| log.has_more_after)
    }

    /// Pin and unpin are offered (useChatArea.ts `canPin`: Manage
    /// messages, Manage channels, Manage server or Administrator from the
    /// user's server roles). In a DM the server allows any participant
    /// (pins.ts checks only server channels); the web's `canPin` there
    /// reads whatever server members are loaded, so the native client
    /// follows the server.
    fn can_pin(&self) -> bool {
        let Some(channel) = self.active_channel.as_deref() else {
            return false;
        };
        if self.dm(channel).is_some() {
            return true;
        }
        let (Some(server), Some(me)) = (self.active_server.as_deref(), self.me()) else {
            return false;
        };
        let mask = perm::MANAGE_MESSAGES | perm::MANAGE_CHANNELS | perm::MANAGE_SERVER;
        self.members
            .get(server)
            .and_then(|list| list.iter().find(|m| m.user.id == me.id))
            .is_some_and(|member| has_any_permission(member_permissions(member), mask))
    }

    fn panel_request(&mut self, fx: &mut Effects, request: ThreadRequest, op: PanelOp) {
        if let Some(id) = self.http(fx, ApiRequest::Threads(request)) {
            self.panels.pending.insert(id, op);
        }
    }

    // Commands

    pub(super) fn on_thread(&mut self, fx: &mut Effects, command: ThreadCommand) {
        if self.signed_in().is_none() {
            return;
        }
        match command {
            ThreadCommand::Open(root_id) => self.open_thread(fx, root_id),
            ThreadCommand::Close => {
                self.panels.thread = ThreadState::default();
                self.forget_thread_requests();
                self.panels.thread_dirty = true;
            }
            ThreadCommand::Send {
                content,
                nonce,
                sent_at,
            } => self.send_thread_reply(fx, content.trim(), nonce, sent_at),
        }
    }

    fn forget_thread_requests(&mut self) {
        self.panels
            .pending
            .retain(|_, op| !matches!(op, PanelOp::Thread { .. } | PanelOp::ThreadSend { .. }));
    }

    /// ThreadSidebar.tsx `fetchThread`: the root and its replies, afresh.
    fn open_thread(&mut self, fx: &mut Effects, root_id: String) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        if root_id.is_empty() || root_id.starts_with("pending-") {
            return;
        }
        self.forget_thread_requests();
        self.panels.thread = ThreadState {
            root_id: root_id.clone(),
            channel_id: channel_id.clone(),
            log: MessageLog::new(&channel_id),
            load: Load::Loading,
        };
        self.panels.thread_dirty = true;
        let request = ThreadRequest::Thread {
            channel_id: channel_id.clone(),
            message_id: root_id.clone(),
        };
        self.panel_request(
            fx,
            request,
            PanelOp::Thread {
                channel_id,
                root_id,
            },
        );
    }

    /// A reply from the thread's composer: a pending row in the thread at
    /// once, then the post (ThreadSidebar.tsx sends `{content,
    /// reply_to_id}`; the nonce rides along so the echo finds the row).
    fn send_thread_reply(
        &mut self,
        fx: &mut Effects,
        content: &str,
        nonce: String,
        sent_at: String,
    ) {
        let thread = &self.panels.thread;
        if thread.root_id.is_empty() || content.is_empty() || nonce.is_empty() {
            return;
        }
        let id = pending_id(&nonce);
        if thread.log.get(&id).is_some() {
            return;
        }
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        let me = me.clone();
        let channel_id = thread.channel_id.clone();
        let root_id = thread.root_id.clone();
        self.panels.thread.log.upsert(Message {
            id,
            channel_id: channel_id.clone(),
            author_id: me.id.clone(),
            author: Some(me),
            content: content.to_owned(),
            reply_to_id: Some(root_id.clone()),
            reply_to: None,
            is_pinned: false,
            created_at: sent_at,
            updated_at: None,
            attachments: Vec::new(),
            reactions: Vec::new(),
            embeds: Vec::new(),
            nonce: Some(nonce.clone()),
            content_revision: None,
            kind: None,
            reply_count: 0,
        });
        self.panels.thread_dirty = true;
        let request = ApiRequest::SendMessage {
            channel_id,
            content: content.to_owned(),
            reply_to_id: Some(root_id),
            nonce: nonce.clone(),
            attachment_ids: Vec::new(),
            nsfw_attachment_ids: Vec::new(),
        };
        if let Some(id) = self.http(fx, request) {
            self.panels
                .pending
                .insert(id, PanelOp::ThreadSend { nonce });
        }
    }

    pub(super) fn on_pins(&mut self, fx: &mut Effects, command: PinsCommand) {
        if self.signed_in().is_none() {
            return;
        }
        match command {
            PinsCommand::Open => self.load_pins(fx),
            PinsCommand::SetPinned { message_id, pinned } => {
                self.set_pinned(fx, &message_id, pinned);
            }
        }
    }

    /// chat-actions.ts `loadPins`: once per channel (`pinsLoadedFor`).
    fn load_pins(&mut self, fx: &mut Effects) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        let pins = &mut self.panels.pins;
        if pins.channel_id == channel_id && !matches!(pins.load, Load::Idle) {
            return;
        }
        pins.channel_id.clone_from(&channel_id);
        pins.load = Load::Loading;
        self.panels.pins_dirty = true;
        self.panels
            .pending
            .retain(|_, op| !matches!(op, PanelOp::Pins { .. }));
        let request = ThreadRequest::Pins {
            channel_id: channel_id.clone(),
        };
        self.panel_request(fx, request, PanelOp::Pins { channel_id });
    }

    /// A message by id: the open channel's list, the thread, or the pins.
    fn panel_message(&self, id: &str) -> Option<&Message> {
        self.messages
            .as_ref()
            .and_then(|log| log.get(id))
            .or_else(|| self.panels.thread.log.get(id))
            .or_else(|| {
                self.panels
                    .pins
                    .load
                    .ready()
                    .and_then(|list| list.iter().find(|m| m.id == id))
            })
    }

    /// Pin or unpin at once, then on the server; a refusal puts it back.
    fn set_pinned(&mut self, fx: &mut Effects, message_id: &str, pinned: bool) {
        if !self.can_pin() {
            return;
        }
        let Some(message) = self
            .panel_message(message_id)
            .filter(|m| !is_pending(m) && m.is_pinned != pinned)
            .cloned()
        else {
            return;
        };
        let request = ThreadRequest::SetPin {
            channel_id: message.channel_id.clone(),
            message_id: message_id.to_owned(),
            pinned,
        };
        self.apply_pin(fx, message_id, pinned, Some(message.clone()));
        let op = PanelOp::SetPin {
            message: Box::new(message),
            pinned,
        };
        self.panel_request(fx, request, op);
    }

    /// `PIN_MESSAGE` (chat-reducer.ts): the flag on every copy, and the
    /// pins list (newest first) when it is the message's channel's.
    fn apply_pin(&mut self, fx: &mut Effects, id: &str, pinned: bool, full: Option<Message>) {
        if let Some(at) = self.position_of(id) {
            let before = [self.message_row_at(at), None];
            if let Some(message) = self.messages.as_mut().and_then(|log| log.get_mut(id)) {
                message.is_pinned = pinned;
            }
            self.emit_message_changes(fx, at, before);
            self.persist_messages(fx);
        }
        if let Some(message) = self.panels.thread.log.get_mut(id) {
            message.is_pinned = pinned;
            self.panels.thread_dirty = true;
        }
        let candidate = if pinned {
            full.or_else(|| self.panel_message(id).cloned())
        } else {
            None
        };
        let pins = &mut self.panels.pins;
        let Load::Ready(list) = &mut pins.load else {
            return;
        };
        if pinned {
            let Some(mut message) = candidate.filter(|m| m.channel_id == pins.channel_id) else {
                return;
            };
            if list.iter().any(|m| m.id == id) {
                return;
            }
            message.is_pinned = true;
            list.push(message);
            list.sort_by_key(|m| Reverse(by_time(m)));
        } else {
            let before = list.len();
            list.retain(|m| m.id != id);
            if list.len() == before {
                return;
            }
        }
        self.panels.pins_dirty = true;
    }

    /// `MESSAGE_PIN` (the whole message) and `MESSAGE_UNPIN`.
    pub(super) fn on_pin_dispatch(
        &mut self,
        fx: &mut Effects,
        id: &str,
        pinned: bool,
        full: Option<Message>,
    ) {
        self.apply_pin(fx, id, pinned, full);
    }

    // Reply counts

    /// A reply came or went: its parent's count moves (chat-reducer.ts
    /// `APPEND_MESSAGE` and `DELETE_MESSAGE`; never below 0).
    pub(super) fn bump_reply_count(&mut self, fx: &mut Effects, parent: Option<&str>, delta: i64) {
        let Some(parent) = parent else {
            return;
        };
        if let Some(at) = self.position_of(parent) {
            let before = [self.message_row_at(at), None];
            if let Some(message) = self.messages.as_mut().and_then(|log| log.get_mut(parent)) {
                message.reply_count = (message.reply_count + delta).max(0);
            }
            self.emit_message_changes(fx, at, before);
        } else if let Some(message) = self.panels.thread.log.get_mut(parent) {
            message.reply_count = (message.reply_count + delta).max(0);
            self.panels.thread_dirty = true;
        }
    }

    // Jumps

    /// `handleJumpToMessage`: the channel first (as a toast click opens
    /// it), then the message.
    pub(super) fn jump_to_message(&mut self, fx: &mut Effects, channel_id: &str, message_id: &str) {
        if self.signed_in().is_none() || channel_id.is_empty() || message_id.is_empty() {
            return;
        }
        if self.active_channel.as_deref() == Some(channel_id) {
            self.panels.jump.wanted = None;
            return self.show_message(fx, channel_id, message_id);
        }
        // Its first page answers once it is open; the jump goes on from there.
        self.panels.jump.wanted = Some((channel_id.to_owned(), message_id.to_owned()));
        self.open_channel(fx, channel_id, None);
        let opening = self.active_channel.as_deref() == Some(channel_id)
            || self.wanted_channel.as_deref() == Some(channel_id);
        if !opening {
            self.panels.jump.wanted = None;
        }
    }

    /// A loaded message is highlighted; anything else loads its window.
    fn show_message(&mut self, fx: &mut Effects, channel_id: &str, message_id: &str) {
        let loaded = self
            .messages
            .as_ref()
            .is_some_and(|log| log.channel_id == channel_id && log.get(message_id).is_some());
        if loaded {
            fx.view(ViewUpdate::JumpTo(message_id.to_owned()));
            return self.read_message_notifications(fx, message_id);
        }
        self.panels
            .pending
            .retain(|_, op| !matches!(op, PanelOp::Around { .. } | PanelOp::Newer { .. }));
        self.panels.jump.newer_loading = false;
        let request = ThreadRequest::Around {
            channel_id: channel_id.to_owned(),
            message_id: message_id.to_owned(),
        };
        let op = PanelOp::Around {
            channel_id: channel_id.to_owned(),
            message_id: message_id.to_owned(),
        };
        self.panel_request(fx, request, op);
    }

    /// A channel's page answered: a jump that waited for it goes on.
    pub(super) fn after_page_jump(&mut self, fx: &mut Effects, channel_id: &str) {
        if let Some((channel, message)) = self.panels.jump.wanted.take() {
            if channel == channel_id {
                self.show_message(fx, &channel, &message);
            }
        }
    }

    /// useChatArea.ts `markNotificationsForMessage`: its unread
    /// notifications are read once it is shown.
    fn read_message_notifications(&mut self, fx: &mut Effects, message_id: &str) {
        let ids: Vec<String> = self
            .notifications
            .iter()
            .filter(|n| n.message_id.as_deref() == Some(message_id))
            .map(|n| n.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.notifications.retain(|n| !ids.contains(&n.id));
        let _ = self.http(fx, ApiRequest::MarkNotificationsRead { ids });
        self.sync_badges(fx);
    }

    /// `REPLACE_MESSAGES`: the window replaces the list (sends in flight
    /// stay at its end), and the message is brought into view.
    fn on_around(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        message_id: &str,
        result: Result<ThreadResponse, ApiError>,
    ) {
        // A failed jump changes nothing, as on the web.
        let Ok(ThreadResponse::Window {
            messages,
            has_more_before,
            has_more_after,
        }) = result
        else {
            return;
        };
        if self.active_channel.as_deref() != Some(channel_id) || messages.is_empty() {
            return;
        }
        let mut log = MessageLog::new(channel_id);
        log.has_more_before = has_more_before;
        log.has_more_after = has_more_after;
        for message in messages {
            self.remember_author(&message);
            log.upsert(message);
        }
        let mut outgoing: Vec<&Outgoing> = self
            .outbox
            .values()
            .filter(|o| o.message.channel_id == channel_id)
            .collect();
        outgoing.sort_by(|a, b| a.message.created_at.cmp(&b.message.created_at));
        for o in outgoing {
            if log.get(&o.message.id).is_none() {
                log.upsert(o.message.clone());
            }
        }
        self.messages = Some(log);
        // A newest page or an older one still in flight is for the list
        // this replaced.
        self.pending.page = None;
        self.pending.older = None;
        self.at_bottom = false;
        fx.messages(ViewDiff::Reset(self.message_rows()));
        self.emit_history(fx);
        fx.view(ViewUpdate::JumpTo(message_id.to_owned()));
        self.read_message_notifications(fx, message_id);
    }

    /// `handleLoadAfter`: the page after the window's newest message.
    pub(super) fn load_newer(&mut self, fx: &mut Effects) {
        if self.panels.jump.newer_loading || self.pending.page.is_some() {
            return;
        }
        let Some(log) = self.messages.as_ref().filter(|log| log.has_more_after) else {
            return;
        };
        let Some(last) = log.iter().rev().find(|m| !is_pending(m)) else {
            return;
        };
        let channel_id = log.channel_id.clone();
        let request = ThreadRequest::After {
            channel_id: channel_id.clone(),
            after: format!("{}|{}", last.created_at, last.id),
        };
        self.panels.jump.newer_loading = true;
        self.panel_request(fx, request, PanelOp::Newer { channel_id });
    }

    /// `APPEND_MESSAGES_AFTER`; the list is attached again once the server
    /// has nothing newer.
    fn on_newer(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        result: Result<ThreadResponse, ApiError>,
    ) {
        self.panels.jump.newer_loading = false;
        // Near the end again retries.
        let Ok(ThreadResponse::Window {
            messages,
            has_more_after,
            ..
        }) = result
        else {
            return;
        };
        if self
            .messages
            .as_ref()
            .is_none_or(|log| log.channel_id != channel_id || !log.has_more_after)
        {
            return;
        }
        for message in &messages {
            self.remember_author(message);
        }
        let before = self.message_rows();
        let Some(log) = self.messages.as_mut() else {
            return;
        };
        let mut added = 0;
        for message in messages {
            if log.get(&message.id).is_none() {
                log.insert_sorted(message);
                added += 1;
            }
        }
        // A page with nothing new would ask for the same cursor again.
        log.has_more_after = has_more_after && added > 0;
        if let Some(newest) = log.iter().rev().find(|m| !is_pending(m)) {
            let at = newest.created_at.clone();
            self.bump_last_message(channel_id, &at);
        }
        let rows = self.message_rows();
        for diff in view::ordered_diff(&before, &rows) {
            fx.messages(diff);
        }
        self.on_seen(fx, false);
    }

    /// `handleJumpToPresent`: read up to the newest message, drop the
    /// divider and load the newest page in place of the window.
    pub(super) fn jump_to_present(&mut self, fx: &mut Effects) {
        let Some(channel) = self.active_channel.clone() else {
            return;
        };
        if !self.detached() || self.pending.page.is_some() {
            return;
        }
        self.panels
            .pending
            .retain(|_, op| !matches!(op, PanelOp::Around { .. } | PanelOp::Newer { .. }));
        self.panels.jump.newer_loading = false;
        self.mark_read(fx, &channel, None);
        self.set_divider(fx, None);
        self.load_page(fx, &channel);
    }

    // HTTP

    /// Answers for this sub-store; `true` when the id was one of its own.
    pub(super) fn route_thread_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.panels.pending.remove(&id) else {
            return false;
        };
        let Some(result) = result.take() else {
            return true;
        };
        if let Err(error) = &result {
            if error.is_unauthorized() {
                self.fail(fx, error);
                return true;
            }
        }
        if let PanelOp::ThreadSend { nonce } = &op {
            self.on_thread_sent(nonce, result);
            return true;
        }
        let result = result.and_then(|response| match response {
            ApiResponse::Threads(response) => Ok(response),
            other => Err(ApiError::Decode(format!("unexpected response: {other:?}"))),
        });
        match op {
            PanelOp::Thread {
                channel_id,
                root_id,
            } => self.on_thread_loaded(&channel_id, &root_id, result),
            PanelOp::Pins { channel_id } => self.on_pins_loaded(&channel_id, result),
            PanelOp::SetPin { message, pinned } => {
                if result.is_err() {
                    let id = message.id.clone();
                    // An unpin that failed goes back on the list.
                    let full = (!pinned).then_some(*message);
                    self.apply_pin(fx, &id, !pinned, full);
                }
            }
            PanelOp::Around {
                channel_id,
                message_id,
            } => self.on_around(fx, &channel_id, &message_id, result),
            PanelOp::Newer { channel_id } => self.on_newer(fx, &channel_id, result),
            PanelOp::ThreadSend { .. } => {}
        }
        true
    }

    fn on_thread_loaded(
        &mut self,
        channel_id: &str,
        root_id: &str,
        result: Result<ThreadResponse, ApiError>,
    ) {
        let thread = &mut self.panels.thread;
        if thread.root_id != root_id || thread.channel_id != channel_id {
            return;
        }
        match result {
            Ok(ThreadResponse::Thread { root, replies }) => {
                // Replies posted from the panel meanwhile stay at the end.
                let waiting: Vec<Message> = thread
                    .log
                    .iter()
                    .filter(|m| is_pending(m))
                    .cloned()
                    .collect();
                let mut log = MessageLog::new(channel_id);
                log.upsert(*root);
                for reply in replies {
                    log.upsert(reply);
                }
                for message in waiting {
                    log.upsert(message);
                }
                thread.log = log;
                thread.load = Load::Ready(());
            }
            Ok(_) => thread.load = Load::Ready(()),
            Err(error) => {
                thread.load = Load::Failed(describe_api_error(&error, "load this thread"))
            }
        }
        self.panels.thread_dirty = true;
        let State { panels, names, .. } = self;
        for author in panels.thread.log.iter().filter_map(|m| m.author.as_ref()) {
            names.insert(author.id.clone(), author.label().to_owned());
        }
    }

    /// The post's answer: its row takes the pending one's place; a failure
    /// drops the pending row (ThreadSidebar.tsx reverts the optimistic add).
    fn on_thread_sent(&mut self, nonce: &str, result: Result<ApiResponse, ApiError>) {
        let stand_in = pending_id(nonce);
        let log = &mut self.panels.thread.log;
        if log.get(&stand_in).is_none() {
            return;
        }
        match result {
            Ok(ApiResponse::Sent(Some(message))) => {
                if log.replace(&stand_in, *message).is_none() {
                    log.remove(&stand_in);
                }
            }
            // An odd body: the gateway echo still reconciles the row.
            Ok(_) => return,
            Err(_) => {
                log.remove(&stand_in);
            }
        }
        self.panels.thread_dirty = true;
    }

    fn on_pins_loaded(&mut self, channel_id: &str, result: Result<ThreadResponse, ApiError>) {
        let pins = &mut self.panels.pins;
        if pins.channel_id != channel_id {
            return;
        }
        pins.load = match result {
            Ok(ThreadResponse::Pins(mut list)) => {
                for message in &mut list {
                    message.is_pinned = true;
                }
                Load::Ready(list)
            }
            // The web keeps "No pinned messages" and asks again next time.
            _ => {
                pins.channel_id.clear();
                Load::Idle
            }
        };
        self.panels.pins_dirty = true;
    }

    // Gateway

    /// What the thread and the pins take from a dispatch, before the open
    /// channel does (ThreadSidebar.tsx listens for `MESSAGE_CREATE` and
    /// `MESSAGE_DELETE`; edits and reactions reach its rows here too).
    pub(super) fn panels_dispatch(&mut self, dispatch: &Dispatch) {
        let thread = &mut self.panels.thread;
        let open = !thread.root_id.is_empty();
        let mut thread_changed = false;
        let mut pins_changed = false;
        let pins = self.panels.pins.load.ready().is_some();
        match dispatch {
            Dispatch::MessageCreate(message) => {
                let reply = message.reply_to_id.as_deref() == Some(thread.root_id.as_str())
                    && message.channel_id == thread.channel_id;
                if open && reply {
                    let stand_in = message.nonce.as_deref().map(pending_id);
                    let replaced = stand_in
                        .and_then(|id| thread.log.replace(&id, (**message).clone()))
                        .is_some();
                    if !replaced {
                        thread.log.upsert((**message).clone());
                    }
                    thread_changed = true;
                }
            }
            Dispatch::MessageUpdate(message) => {
                if thread.log.get(&message.id).is_some() {
                    thread.log.upsert((**message).clone());
                    thread_changed = true;
                }
                if let Load::Ready(list) = &mut self.panels.pins.load {
                    if let Some(pin) = list.iter_mut().find(|m| m.id == message.id) {
                        *pin = Message {
                            is_pinned: true,
                            ..(**message).clone()
                        };
                        pins_changed = true;
                    }
                }
            }
            Dispatch::MessagePatch(patch) => {
                thread_changed = thread.log.patch(patch).is_some();
                if let Load::Ready(list) = &mut self.panels.pins.load {
                    if let Some(at) = list.iter().position(|m| m.id == patch.id) {
                        let mut one = MessageLog::new(&list[at].channel_id);
                        one.upsert(list[at].clone());
                        if one.patch(patch).is_some() {
                            if let Some(patched) = one.get(&patch.id) {
                                list[at] = patched.clone();
                                pins_changed = true;
                            }
                        }
                    }
                }
            }
            Dispatch::MessageDelete(delete) => {
                // The root stays shown, as the web keeps it.
                if delete.id != thread.root_id {
                    thread_changed = thread.log.remove(&delete.id).is_some();
                }
                if let Load::Ready(list) = &mut self.panels.pins.load {
                    let before = list.len();
                    list.retain(|m| m.id != delete.id);
                    pins_changed = list.len() != before;
                }
            }
            Dispatch::ReactionAdd(event) | Dispatch::ReactionRemove(event) => {
                let add = matches!(dispatch, Dispatch::ReactionAdd(_));
                let here = event
                    .channel_id
                    .as_deref()
                    .is_none_or(|c| c == thread.channel_id);
                if here {
                    thread_changed = thread
                        .log
                        .react(&event.message_id, &event.emoji, &event.user_id, add)
                        .is_some();
                }
            }
            _ => {}
        }
        self.panels.thread_dirty |= thread_changed;
        self.panels.pins_dirty |= pins_changed && pins;
    }

    // The view

    fn panel_context(&self) -> RowContext<'_> {
        RowContext {
            divider: None,
            ..self.row_context()
        }
    }

    /// The thread as ThreadSidebar.tsx draws it: the root with its header,
    /// then the replies grouped by author within five minutes; no reply
    /// quotes (`hideReplyConnector`).
    fn thread_rows(&self) -> Vec<MessageRow> {
        let ctx = self.panel_context();
        let log = &self.panels.thread.log;
        let mut previous: Option<&Message> = None;
        log.iter()
            .enumerate()
            .map(|(i, message)| {
                let bare = Message {
                    reply_to: None,
                    ..message.clone()
                };
                let mut row = view::message_row(&bare, None, &ctx);
                row.grouped = i > 1
                    && previous.is_some_and(|p| {
                        p.author_id == message.author_id
                            && matches!(
                                (by_time(p), by_time(message)),
                                (Some(a), Some(b)) if b - a <= view::GROUP_WINDOW_MS
                            )
                    });
                row.pending = is_pending(message);
                previous = Some(message);
                row
            })
            .collect()
    }

    fn pin_rows(&self) -> Vec<MessageRow> {
        let ctx = self.panel_context();
        self.panels
            .pins
            .load
            .ready()
            .map(|list| {
                list.iter()
                    .map(|m| view::message_row(m, None, &ctx))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn panels_view(&self) -> PanelsView {
        let thread = &self.panels.thread;
        let pins = &self.panels.pins;
        PanelsView {
            can_pin: self.can_pin(),
            thread_root_id: thread.root_id.clone(),
            thread_loading: thread.load.is_loading(),
            thread_error: thread.load.error().to_owned(),
            thread_replies: i64::try_from(thread.log.len().saturating_sub(1)).unwrap_or(i64::MAX),
            pins_loading: pins.load.is_loading(),
            pins_count: pins
                .load
                .ready()
                .map_or(0, |l| i64::try_from(l.len()).unwrap_or(i64::MAX)),
        }
    }

    /// The thread's and the pins' copies of messages the open channel has
    /// follow it (edits, reactions and pins made there).
    fn follow_open_channel(&mut self) {
        let Some(log) = &self.messages else {
            return;
        };
        let panels = &mut self.panels;
        if !panels.thread.root_id.is_empty() && log.channel_id == panels.thread.channel_id {
            let newer: Vec<Message> = panels
                .thread
                .log
                .iter()
                .filter_map(|m| log.get(&m.id).filter(|open| *open != m).cloned())
                .collect();
            panels.thread_dirty |= !newer.is_empty();
            for message in newer {
                panels.thread.log.upsert(message);
            }
        }
        if let Load::Ready(list) = &mut panels.pins.load {
            if log.channel_id == panels.pins.channel_id {
                for pin in list.iter_mut() {
                    if let Some(open) = log.get(&pin.id).filter(|open| open.is_pinned) {
                        if open != pin {
                            *pin = open.clone();
                            panels.pins_dirty = true;
                        }
                    }
                }
            }
        }
    }

    /// Emits what changed: the thread's and the pins' rows, the panels'
    /// scalars and the detached list's state. Runs after every input.
    pub(super) fn sync_panels(&mut self, fx: &mut Effects) {
        self.follow_open_channel();
        if std::mem::take(&mut self.panels.thread_dirty) {
            let rows = self.thread_rows();
            for diff in view::ordered_diff(&self.panels.thread_rows, &rows) {
                fx.view(ViewUpdate::ThreadMessages(Box::new(diff)));
            }
            self.panels.thread_rows = rows;
        }
        if std::mem::take(&mut self.panels.pins_dirty) {
            let rows = self.pin_rows();
            for diff in view::ordered_diff(&self.panels.pin_rows, &rows) {
                fx.view(ViewUpdate::PinnedMessages(Box::new(diff)));
            }
            self.panels.pin_rows = rows;
        }
        let view = self.panels_view();
        if view != self.panels.view {
            self.panels.view = view.clone();
            fx.view(ViewUpdate::Panels(Box::new(view)));
        }
        let newer = (self.panels.jump.newer_loading, self.detached());
        if newer != self.panels.newer {
            self.panels.newer = newer;
            fx.view(ViewUpdate::Newer {
                loading: newer.0,
                more: newer.1,
            });
        }
    }

    /// Sign-out: nothing of the last session's panels stays.
    pub(super) fn clear_panels(&mut self) {
        let panels = &mut self.panels;
        panels.thread = ThreadState::default();
        panels.pins = Default::default();
        panels.jump = Default::default();
        panels.pending.clear();
        panels.thread_dirty = true;
        panels.pins_dirty = true;
    }
}
