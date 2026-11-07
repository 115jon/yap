//! The reducer: one arm per input, emitting effects in the order the old
//! sequential core performed them, so the UI sees the same updates.

mod admin;
mod apps;
mod backgrounds;
mod calls;
mod embeds;
mod extras;
mod favorites;
mod friends;
mod invites;
mod media;
mod search;
mod settings;
mod shop;
mod social;
mod threads;

use std::time::Duration;

use serde::Serialize;
use yap_protocol::payload::{ChannelRef, ServerSubscribe, VoiceChannelJoin, VoiceChannelLeave};
use yap_protocol::rest::{ChannelType, Message, MessagePatch, ReplyTo, Server, User};
use yap_protocol::{Frame, GatewayOp};

use super::state::{
    pending_id, Boot, Divider, MessageLog, Outgoing, Pending, Phase, ServerChannels, Session,
    Standby, Startup, State, Typer, VoiceCall, VoicePrefs,
};
use super::time::{is_after, parse_rfc3339, Stamp};
use super::view::{
    self, ChannelTreeRow, MessageRow, Row, RowContext, TreeKind, ViewDiff, ViewUpdate,
};
use super::view::{MeView, MemberKind, MemberRole, Seat, UnreadView, VoicePanelView, VoiceView};
use super::{
    ApiRequest, ApiResponse, CacheWrite, CallId, Command, Dispatch, Effect, GatewayEvent, Input,
    Occupant, Page, RequestId, SessionId, SignInEvent, SignInId, Snapshot, VoiceCommand,
    VoiceTarget, VoiceUpdate,
};
use crate::api::{ApiError, Token};
use crate::command_menu::RecentTarget;
use crate::shell::{self, Sound};
use crate::store::calls::{CallsCommand, RingStatus};
use yap_protocol::payload::VoiceChannelMember;

const SESSION_ENDED: &str = "Your session ended. Sign in again.";

/// The status line while a view shown from the cache is revalidated.
pub const REFRESHING: &str = "Refreshing…";

/// How long someone stays on the typing line after their last typing event.
pub const TYPING_TIMEOUT: Duration = Duration::from_secs(8);

/// How long a hidden window keeps its standby session before dropping it
/// (voice-standby.ts STANDBY_HIDDEN_GRACE_MS), so an alt-tab or a quick
/// trip to the tray does not churn sessions.
pub const STANDBY_HIDDEN_GRACE: Duration = Duration::from_secs(15);

/// How long after a gateway Ready or Resumed the server's roster snapshot
/// may take. It sends `VOICE_CHANNEL_STATES` only when some channel has
/// people in it, so a channel it has not resent by then is empty.
pub const VOICE_ROSTER_GRACE: Duration = Duration::from_secs(5);

/// Mention suggestions shown at once (MentionAutocomplete.tsx).
pub const MENTION_SUGGESTIONS: usize = 5;

const EDIT_FAILED: &str =
    "Couldn't save your edit. Your message is unchanged. Try editing it again.";
/// The text of a toast reply that could not be sent follows, so it is not
/// lost with the toast (the channel is not open, so no composer to keep it).
const TOAST_REPLY_FAILED: &str =
    "Couldn't send your reply. Check your connection and try again. Your reply was:";
const DELETE_FAILED: &str = "Couldn't delete the message. Check your connection and try again.";

/// Permission bits (src/lib/permissions.ts).
const ADMINISTRATOR: i64 = 1;
const MANAGE_MESSAGES: i64 = 1 << 10;

fn is_pending(message: &Message) -> bool {
    message.id.starts_with("pending-")
}

/// Applies one input and returns the effects to run, in order.
pub fn reduce(state: &mut State, input: Input) -> Vec<Effect> {
    let mut fx = Effects(Vec::new());
    match input {
        Input::User(command) => state.on_command(&mut fx, command),
        Input::TokenLoaded(token) => state.on_token(&mut fx, token),
        Input::SnapshotLoaded(snapshot) => state.on_snapshot(&mut fx, snapshot),
        Input::CatalogLoaded(stored) => state.on_catalog_loaded(&mut fx, stored),
        Input::Http { id, result } => state.on_http(&mut fx, id, result),
        Input::SignIn { attempt, event } => state.on_sign_in(&mut fx, attempt, event),
        Input::Gateway { session, event } => state.on_gateway(&mut fx, session, event),
        Input::Voice { call, update } => state.on_voice(&mut fx, call, update),
        Input::UploadProgress { id, sent } => state.on_upload_progress(&mut fx, id, sent),
        Input::Audio(event) => state.on_audio(&mut fx, event),
        Input::TypingExpired {
            user_id,
            generation,
        } => {
            let before = state.typing.len();
            state
                .typing
                .retain(|t| !(t.user_id == user_id && t.generation == generation));
            if state.typing.len() != before {
                state.emit_typing(&mut fx);
            }
        }
        Input::VoiceRosterCheck { generation } => {
            state.check_voice_rosters(&mut fx, generation);
        }
        Input::StandbyHidden { generation } => {
            if generation == state.window_generation && !state.window_shown {
                state.standby_paused = true;
                state.sync_standby(&mut fx);
            }
        }
    }
    state.sync_settings(&mut fx);
    state.sync_admin(&mut fx);
    state.sync_panels(&mut fx);
    state.sync_search(&mut fx);
    state.sync_friends(&mut fx);
    state.sync_invites(&mut fx);
    state.sync_apps(&mut fx);
    fx.0
}

struct Effects(Vec<Effect>);

impl Effects {
    fn push(&mut self, effect: Effect) {
        self.0.push(effect);
    }

    fn view(&mut self, update: ViewUpdate) {
        self.0.push(Effect::View(update));
    }

    fn messages(&mut self, diff: ViewDiff<MessageRow>) {
        self.view(ViewUpdate::Messages(Box::new(diff)));
    }
}

fn frame<T: Serialize>(op: GatewayOp, payload: &T) -> Option<Frame> {
    // These payloads are plain structs of strings and bools: they always encode.
    Frame::with_payload(op, payload).ok()
}

impl State {
    fn set_phase(&mut self, fx: &mut Effects, phase: Phase) {
        self.phase = phase;
        fx.view(ViewUpdate::Phase(phase));
    }

    fn set_status(&mut self, fx: &mut Effects, text: impl Into<String>) {
        self.status = text.into();
        fx.view(ViewUpdate::Status(self.status.clone()));
    }

    fn set_connected(&mut self, fx: &mut Effects, connected: bool) {
        self.connected = connected;
        fx.view(ViewUpdate::Connected(connected));
    }

    /// The session once `me` is known (what the old core called a session).
    fn signed_in(&self) -> Option<(&Session, &User)> {
        let session = self.session.as_ref()?;
        Some((session, session.me.as_ref()?))
    }

    fn http(&mut self, fx: &mut Effects, request: ApiRequest) -> Option<RequestId> {
        let token = self.session.as_ref()?.token.clone();
        let id = RequestId(self.next_id());
        fx.push(Effect::Http { id, token, request });
        Some(id)
    }

    /// Sends a frame if the gateway is up; dropped otherwise (it re-syncs on Ready).
    fn gateway_send(&self, fx: &mut Effects, frame: Option<Frame>) {
        let ready = self.session.as_ref().is_some_and(|s| s.gateway.ready);
        if let (true, Some(frame)) = (ready, frame) {
            fx.push(Effect::GatewaySend(frame));
        }
    }

    fn voice_join_frame(call: &VoiceCall) -> Option<Frame> {
        let join = VoiceChannelJoin {
            channel_id: call.channel_id.clone(),
            self_mute: Some(call.muted),
            started_at: None,
            move_grant: call.move_grant.clone(),
        };
        frame(GatewayOp::VoiceChannelJoin, &join)
    }

    fn voice_leave_frame(channel_id: &str) -> Option<Frame> {
        let leave = VoiceChannelLeave {
            channel_id: Some(channel_id.to_owned()),
        };
        frame(GatewayOp::VoiceChannelLeave, &leave)
    }

    fn channel_frame(op: GatewayOp, channel_id: &str) -> Option<Frame> {
        let channel = ChannelRef {
            channel_id: channel_id.to_owned(),
        };
        frame(op, &channel)
    }

    // View projection

    fn row_context(&self) -> RowContext<'_> {
        let me = self.me();
        RowContext {
            api_base: &self.api_base,
            me_id: me.map_or("", |me| me.id.as_str()),
            me_username: me.map_or("", |me| me.username.as_str()),
            offset: self.local_offset,
            outbox: &self.outbox,
            divider: self.divider.as_ref().map(|d| d.message_id.as_str()),
            names: &self.names,
            hydrations: &self.hydrations,
            members: self
                .active_server
                .as_ref()
                .and_then(|server| self.members.get(server))
                .map_or(&[], Vec::as_slice),
        }
    }

    /// Runs `change`, then emits updates for the rows at `indices` that it
    /// changed (a flag the row reads from the state, not the message).
    fn touch_rows(&mut self, fx: &mut Effects, indices: &[usize], change: impl FnOnce(&mut Self)) {
        let before: Vec<(usize, Option<MessageRow>)> = indices
            .iter()
            .map(|&i| (i, self.message_row_at(i)))
            .collect();
        change(self);
        for (at, old) in before {
            if let (Some(old), Some(row)) = (old, self.message_row_at(at)) {
                let roles = old.changed(&row);
                if !roles.is_empty() {
                    fx.messages(ViewDiff::Update { at, row, roles });
                }
            }
        }
    }

    fn position_of(&self, id: &str) -> Option<usize> {
        self.messages.as_ref()?.position(id)
    }

    /// Removes a row from the open channel (the one below may regroup).
    fn remove_row(&mut self, fx: &mut Effects, id: &str) -> Option<Message> {
        let at = self.position_of(id)?;
        let below = self.message_row_at(at + 1);
        let (_, message) = self.messages.as_mut()?.remove(id)?;
        fx.messages(ViewDiff::Remove { at, count: 1 });
        self.emit_message_changes(fx, at, [below, None]);
        self.bump_reply_count(fx, message.reply_to_id.as_deref(), -1);
        Some(message)
    }

    /// Puts a message back into the open channel by time.
    fn insert_row_sorted(&mut self, fx: &mut Effects, message: Message) {
        let Some(log) = self.messages.as_ref() else {
            return;
        };
        if log.channel_id != message.channel_id || log.get(&message.id).is_some() {
            return;
        }
        let at = log
            .iter()
            .position(|m| m.created_at > message.created_at)
            .unwrap_or(log.len());
        // The row at `at` moves down one and may regroup under the new one.
        let shifted = self.message_row_at(at);
        let parent = message.reply_to_id.clone();
        if let Some(log) = self.messages.as_mut() {
            log.insert_sorted(message);
        }
        if let Some(row) = self.message_row_at(at) {
            fx.messages(ViewDiff::Insert {
                at,
                rows: vec![row],
            });
        }
        self.emit_message_changes(fx, at, [None, shifted]);
        self.bump_reply_count(fx, parent.as_deref(), 1);
    }

    /// The open channel's row at `index`, grouped against the one above.
    fn message_row_at(&self, index: usize) -> Option<MessageRow> {
        let log = self.messages.as_ref()?;
        let message = log.at(index)?;
        let previous = index.checked_sub(1).and_then(|i| log.at(i));
        Some(view::message_row(message, previous, &self.row_context()))
    }

    fn message_rows(&self) -> Vec<MessageRow> {
        let Some(log) = &self.messages else {
            return Vec::new();
        };
        let ctx = self.row_context();
        let mut previous = None;
        log.iter()
            .map(|message| {
                let row = view::message_row(message, previous, &ctx);
                previous = Some(message);
                row
            })
            .collect()
    }

    /// Emits updates for rows `at` and `at + 1` that differ from `before`
    /// (a change at `at` can regroup the row below it).
    fn emit_message_changes(&self, fx: &mut Effects, at: usize, before: [Option<MessageRow>; 2]) {
        for (offset, old) in before.into_iter().enumerate() {
            let index = at + offset;
            if let (Some(old), Some(row)) = (old, self.message_row_at(index)) {
                let roles = old.changed(&row);
                if !roles.is_empty() {
                    fx.messages(ViewDiff::Update {
                        at: index,
                        row,
                        roles,
                    });
                }
            }
        }
    }

    fn sync_servers(&mut self, fx: &mut Effects) {
        let here = self.voice.as_ref().map(|v| v.server_id.as_str());
        let rows = view::server_rows(
            &self.servers,
            &self.api_base,
            |id| (self.server_unread(id), self.server_mentions(id)),
            |id| self.server_voice(id, here == Some(id)),
        );
        for diff in view::ordered_diff(&self.view.servers, &rows) {
            fx.view(ViewUpdate::Servers(diff));
        }
        self.view.servers = rows;
    }

    /// The server's voice channels with people in them, in position order.
    fn server_voice(&self, server_id: &str, here: bool) -> view::ServerVoice {
        let Some(list) = self.channels.get(server_id) else {
            return view::ServerVoice::default();
        };
        let mut active: Vec<_> = list
            .channels
            .iter()
            .filter(|c| c.channel_type == ChannelType::Voice)
            .filter_map(|c| {
                let occupants = self.voice_channels.get(&c.id)?;
                (!occupants.is_empty()).then(|| {
                    let names = occupants.iter().map(|o| o.name.clone()).collect();
                    (c.position, (c.name.clone(), names))
                })
            })
            .collect();
        active.sort_by_key(|(position, _)| *position);
        view::ServerVoice {
            channels: active.into_iter().map(|(_, channel)| channel).collect(),
            here,
        }
    }

    /// Who is in a voice channel, as the tree and the voice view show it.
    fn seats(&self, channel_id: &str) -> Vec<Seat> {
        let me = self.me().map_or("", |me| me.id.as_str());
        view::seats(
            channel_id,
            &self.voice_channels,
            self.voice.as_ref(),
            me,
            &self.api_base,
        )
    }

    fn tree_rows(&self) -> Vec<ChannelTreeRow> {
        let list = self
            .tree_server
            .as_ref()
            .and_then(|id| self.channels.get(id));
        let mut rows = view::tree_rows(
            list,
            |id| self.seats(id),
            |id| (self.is_unread(id), self.channel_mentions(id)),
            self.tree_manage_all(),
        );
        self.add_voice_status(&mut rows);
        rows
    }

    /// ChannelSidebar.tsx: the status shows under a voice channel while
    /// anyone is in it, after its people (on the last one's row, or the
    /// channel's when nobody is listed); we may change it once our voice
    /// session is up there. The channel row also carries how many are in
    /// the call and when it started, for "2 · 0:47".
    fn add_voice_status(&self, rows: &mut [ChannelTreeRow]) {
        for i in 0..rows.len() {
            if !rows[i].voice || rows[i].kind != TreeKind::Channel {
                continue;
            }
            let id = rows[i].id.clone();
            let mut end = i;
            while end + 1 < rows.len() && rows[end + 1].kind == TreeKind::Member {
                end += 1;
            }
            rows[i].occupants = i64::try_from(end - i).unwrap_or(i64::MAX);
            rows[i].started_at = if end > i {
                self.voice_started_at.get(&id).copied().unwrap_or_default()
            } else {
                0
            };
            rows[end].status_here = true;
            rows[end].status_channel = rows[i].name.clone();
            self.fill_voice_status(&id, &mut rows[end]);
            if end != i {
                let (status, media, editable) = (
                    rows[end].status.clone(),
                    rows[end].status_media.clone(),
                    rows[end].status_editable,
                );
                rows[i].status = status;
                rows[i].status_media = media;
                rows[i].status_editable = editable;
            }
        }
    }

    fn fill_voice_status(&self, channel_id: &str, row: &mut ChannelTreeRow) {
        let connected = self
            .voice
            .as_ref()
            .filter(|_| !self.apps.participant_id.is_empty())
            .map(|call| call.channel_id.as_str());
        let editable = connected == Some(channel_id);
        let occupied = editable
            || self
                .voice_channels
                .get(channel_id)
                .is_some_and(|people| !people.is_empty());
        let status = self
            .find_channel(channel_id)
            .and_then(|c| c.voice_status.as_ref())
            .filter(|_| occupied);
        row.status_editable = editable;
        row.status = status
            .and_then(|s| s.text.as_deref())
            .map(str::trim)
            .unwrap_or_default()
            .to_owned();
        row.status_media = status
            .and_then(|s| s.media.as_ref())
            .map(|m| {
                view::absolute_url(
                    &self.api_base,
                    Some(&crate::store::extras::status_media_url(m)),
                )
            })
            .unwrap_or_default();
    }

    /// Read marks, newest messages or notifications changed.
    fn sync_badges(&mut self, fx: &mut Effects) {
        self.sync_servers(fx);
        self.sync_tree(fx);
        self.sync_dms(fx);
        self.sync_tray_badge(fx);
        self.sync_extras(fx);
    }

    /// The tray badge (desktop-notifications.ts): unread notifications,
    /// else a dot while any DM or server channel has unread messages.
    fn sync_tray_badge(&mut self, fx: &mut Effects) {
        let notifications = self.notifications.iter().filter(|n| !n.is_read).count();
        let any_unread = self.last_message_at.keys().any(|id| self.has_unread(id));
        let badge = shell::badge(notifications, any_unread);
        if badge != self.view.badge {
            self.view.badge = badge.clone();
            fx.view(ViewUpdate::Badge(badge));
        }
    }

    /// A server channel's name and its server's name, from any loaded tree.
    fn channel_and_server(&self, id: &str) -> Option<(&str, Option<&str>)> {
        self.channels.iter().find_map(|(server_id, list)| {
            let channel = list.get(id)?;
            let server = self
                .servers
                .iter()
                .find(|s| s.id == *server_id)
                .map(|s| s.name.as_str());
            Some((channel.name.as_str(), server))
        })
    }

    /// The server whose tree holds a channel.
    fn server_of(&self, channel_id: &str) -> Option<String> {
        self.channels
            .iter()
            .find(|(_, list)| list.get(channel_id).is_some())
            .map(|(server_id, _)| server_id.clone())
    }

    /// A toast click: the DM, or the channel in its server (switching
    /// servers first when it is another one).
    fn open_channel(&mut self, fx: &mut Effects, id: &str, hint: Option<String>) {
        if self.signed_in().is_none() || id.is_empty() {
            return;
        }
        if self.dm(id).is_some() {
            return self.select_dm(fx, id);
        }
        let hint = hint
            .filter(|s| !s.is_empty())
            .filter(|s| self.servers.iter().any(|known| known.id == *s));
        match self.server_of(id).or(hint) {
            Some(server)
                if !self.home && self.active_server.as_deref() == Some(server.as_str()) =>
            {
                self.select_channel(fx, id);
            }
            Some(server) => {
                self.wanted_channel = Some(id.to_owned());
                self.select_server(fx, server);
            }
            None => {}
        }
    }

    /// The voice channel the roster model shows: the open voice view, else
    /// the call's channel.
    fn roster_channel(&self) -> Option<&str> {
        self.voice_view
            .as_deref()
            .or(self.voice.as_ref().map(|c| c.channel_id.as_str()))
    }

    /// A newly loaded tree (another server, or a refetch): one reset.
    fn reset_tree(&mut self, fx: &mut Effects) {
        let rows = self.tree_rows();
        fx.view(ViewUpdate::Channels(ViewDiff::Reset(rows.clone())));
        self.view.tree = rows;
    }

    /// The same tree with the call's members changed: row diffs only.
    fn sync_tree(&mut self, fx: &mut Effects) {
        let rows = self.tree_rows();
        for diff in view::ordered_diff(&self.view.tree, &rows) {
            fx.view(ViewUpdate::Channels(diff));
        }
        self.view.tree = rows;
    }

    fn sync_roster(&mut self, fx: &mut Effects) {
        let seats = self
            .roster_channel()
            .map(|id| self.seats(id))
            .unwrap_or_default();
        let rows = view::roster_rows(&seats);
        for diff in view::ordered_diff(&self.view.roster, &rows) {
            fx.view(ViewUpdate::Roster(diff));
        }
        self.view.roster = rows;
        let tiles = self
            .voice
            .as_ref()
            .map(|call| view::tile_rows(&self.seats(&call.channel_id), call))
            .unwrap_or_default()
            .into_iter()
            .map(|mut tile| {
                if tile.kind == "person" {
                    tile.deco = self
                        .known_user(&tile.member_id)
                        .map(|u| crate::collectibles::decoration_url(u.avatar_display.as_ref()))
                        .unwrap_or_default();
                } else if tile.me || tile.watching {
                    tile.watchers = self.watchers_json(&tile.member_id);
                }
                tile
            })
            .collect::<Vec<_>>();
        for diff in view::ordered_diff(&self.view.tiles, &tiles) {
            fx.view(ViewUpdate::Tiles(diff));
        }
        self.view.tiles = tiles;
    }

    /// The people watching a share (stream-watchers.ts
    /// `buildStreamWatcherIdentities`): the call's names, "You" for us.
    fn watchers_json(&self, streamer: &str) -> String {
        let Some(viewers) = self.apps.watchers.get(streamer) else {
            return String::new();
        };
        let me = self.me().map(|me| me.id.clone()).unwrap_or_default();
        let seats = self
            .voice
            .as_ref()
            .map(|call| self.seats(&call.channel_id))
            .unwrap_or_default();
        let people: Vec<serde_json::Value> = viewers
            .iter()
            .map(|id| {
                let seat = seats.iter().find(|s| &s.key == id);
                let name = if *id == me {
                    "You".to_owned()
                } else {
                    seat.map(|s| s.name.clone())
                        .or_else(|| self.known_user(id).map(|u| u.label().to_owned()))
                        .unwrap_or_else(|| "Someone".to_owned())
                };
                serde_json::json!({
                    "userId": id,
                    "initials": view::initials(&name),
                    "name": name,
                    "avatar": seat.map(|s| s.avatar.clone()).unwrap_or_default(),
                })
            })
            .collect();
        serde_json::Value::Array(people).to_string()
    }

    /// The open voice view's scalars (its count follows the roster).
    fn emit_voice_panel(&mut self, fx: &mut Effects) {
        let panel = match self.voice_view.as_deref() {
            Some(id) => VoicePanelView {
                id: id.to_owned(),
                name: self
                    .find_channel(id)
                    .map(|c| c.name.clone())
                    .unwrap_or_default(),
                count: i64::try_from(self.seats(id).len()).unwrap_or(i64::MAX),
            },
            None => VoicePanelView::default(),
        };
        fx.view(ViewUpdate::VoicePanel(panel));
    }

    /// The call's scalars, the roster, and the voice channels in the tree.
    fn emit_voice(&mut self, fx: &mut Effects) {
        let prefs = self.voice_prefs;
        let voice = match &self.voice {
            Some(call) => VoiceView {
                channel_id: call.channel_id.clone(),
                channel_name: self
                    .find_channel(&call.channel_id)
                    .map(|c| c.name.clone())
                    .or_else(|| {
                        self.dm(&call.channel_id)
                            .map(|dm| dm.recipient.label().to_owned())
                    })
                    .unwrap_or_default(),
                live: call.status == "Connected",
                status: call.status.clone(),
                muted: call.muted,
                deafened: call.deafened,
                speaking: call.roster.iter().any(|m| m.me && m.speaking),
                error: String::new(),
                camera_wanted: call.camera_wanted,
                camera: call.camera,
                camera_error: call.camera_error.clone(),
                share_pending: call.share.is_none() && call.share_wanted.is_some(),
                share: call.share.clone().unwrap_or_default(),
                sharing: call.share.is_some(),
                share_error: call.share_error.clone(),
            },
            None => VoiceView {
                muted: prefs.muted,
                deafened: prefs.deafened,
                error: self.voice_error.clone(),
                ..VoiceView::default()
            },
        };
        fx.view(ViewUpdate::Voice(voice));
        if self.voice_view.is_some() {
            self.emit_voice_panel(fx);
        }
        self.sync_roster(fx);
        self.sync_tree(fx);
        self.sync_member_call(fx);
    }

    /// The call changed (who is in it, who speaks, mute): only the member
    /// rows whose LED or in-call mark changed are updated. Neither moves a
    /// row, so the list is not rebuilt, and a speaking change of someone the
    /// panel does not show emits nothing.
    fn sync_member_call(&mut self, fx: &mut Effects) {
        let seats = self
            .call_channel()
            .map(|id| self.seats(id))
            .unwrap_or_default();
        for (at, row) in self.view.members.iter_mut().enumerate() {
            if row.kind != MemberKind::Member {
                continue;
            }
            let seat = seats.iter().find(|s| s.key == row.user_id);
            let in_call = seat.is_some();
            let speaking = seat.is_some_and(|s| s.speaking && !s.muted);
            let mut roles = Vec::new();
            if row.in_call != in_call {
                row.in_call = in_call;
                roles.push(MemberRole::InCall);
            }
            if row.speaking != speaking {
                row.speaking = speaking;
                roles.push(MemberRole::Speaking);
            }
            if !roles.is_empty() {
                fx.view(ViewUpdate::Members(ViewDiff::Update {
                    at,
                    row: row.clone(),
                    roles,
                }));
            }
        }
    }

    /// Opens a voice channel's view (its landing; the call once joined).
    fn open_voice_panel(&mut self, fx: &mut Effects, id: &str) {
        self.voice_view = Some(id.to_owned());
        self.emit_voice_panel(fx);
        self.sync_roster(fx);
        self.sync_standby(fx);
    }

    fn close_voice_panel(&mut self, fx: &mut Effects) {
        if self.voice_view.take().is_some() {
            fx.view(ViewUpdate::VoicePanel(VoicePanelView::default()));
            self.sync_roster(fx);
        }
        self.sync_standby(fx);
    }

    /// Holds a standby for the open voice landing while someone else is in
    /// that channel and we are in no call (voice-standby.ts: a join only has
    /// to swap the mic in); drops it otherwise.
    fn sync_standby(&mut self, fx: &mut Effects) {
        let me = self.me().map(|me| me.id.clone()).unwrap_or_default();
        let wanted = self
            .voice_view
            .clone()
            .filter(|_| {
                self.standby_enabled && !self.standby_paused && self.voice.is_none() && !self.home
            })
            .filter(|id| {
                self.voice_channels
                    .get(id)
                    .is_some_and(|list| list.iter().any(|o| o.user_id != me))
            })
            .zip(self.active_server.clone());
        let held = self
            .standby
            .as_ref()
            .map(|s| (s.channel_id.clone(), s.server_id.clone()));
        if wanted == held {
            return;
        }
        if let Some(standby) = self.standby.take() {
            fx.push(Effect::Voice {
                call: standby.id,
                command: VoiceCommand::Leave,
            });
        }
        let (Some((channel_id, server_id)), Some((session, me))) = (wanted, self.signed_in())
        else {
            return;
        };
        let target = VoiceTarget {
            server_id: server_id.clone(),
            me_id: me.id.clone(),
            channel_id: channel_id.clone(),
            me_name: me.label().to_owned(),
            me_username: me.username.clone(),
            standby: true,
            ..VoiceTarget::default()
        };
        let token = session.token.clone();
        let call = CallId(self.next_id());
        fx.push(Effect::VoiceStart {
            call,
            token,
            target,
        });
        self.standby = Some(Standby {
            id: call,
            server_id,
            channel_id,
        });
    }

    /// The window went on or off screen. Hidden, the standby goes after
    /// [`STANDBY_HIDDEN_GRACE`]; shown, it comes back at once.
    fn set_window_shown(&mut self, fx: &mut Effects, shown: bool) {
        if shown == self.window_shown {
            return;
        }
        self.window_shown = shown;
        self.window_generation += 1;
        if shown {
            self.standby_paused = false;
            self.sync_standby(fx);
        } else {
            fx.push(Effect::Later {
                delay: STANDBY_HIDDEN_GRACE,
                input: Box::new(Input::StandbyHidden {
                    generation: self.window_generation,
                }),
            });
        }
    }

    /// A fresh or resumed gateway session: the server resends the rosters
    /// with people in them, and nothing for empty ones, so whoever left
    /// while we were away would stay listed. A check after
    /// [`VOICE_ROSTER_GRACE`] drops what it has not confirmed.
    fn expect_voice_rosters(&mut self, fx: &mut Effects) {
        self.voice_roster_check += 1;
        self.voice_confirmed.clear();
        if self.voice_channels.is_empty() {
            return;
        }
        fx.push(Effect::Later {
            delay: VOICE_ROSTER_GRACE,
            input: Box::new(Input::VoiceRosterCheck {
                generation: self.voice_roster_check,
            }),
        });
    }

    fn check_voice_rosters(&mut self, fx: &mut Effects, generation: u64) {
        if generation != self.voice_roster_check {
            return;
        }
        let confirmed = std::mem::take(&mut self.voice_confirmed);
        let before = self.voice_channels.len();
        self.voice_channels.retain(|id, _| confirmed.contains(id));
        self.voice_started_at.retain(|id, _| confirmed.contains(id));
        if self.voice_channels.len() != before {
            self.sync_voice_channels(fx);
            self.sync_standby(fx);
            self.sync_calls(fx);
        }
    }

    /// A gateway roster for one voice channel, profiles kept from the last
    /// full member with the same user (deltas leave them out).
    fn occupants_from(&self, channel_id: &str, members: Vec<VoiceChannelMember>) -> Vec<Occupant> {
        let previous = self.voice_channels.get(channel_id);
        members
            .into_iter()
            .map(|m| {
                let known = previous.and_then(|list| list.iter().find(|o| o.user_id == m.user_id));
                let name = [&m.display_name, &m.name, &m.username]
                    .into_iter()
                    .flatten()
                    .find(|n| !n.is_empty())
                    .cloned()
                    .or_else(|| known.map(|o| o.name.clone()))
                    .unwrap_or_else(|| "Someone".to_owned());
                Occupant {
                    avatar_url: m
                        .avatar_url
                        .or_else(|| known.map(|o| o.avatar_url.clone()))
                        .unwrap_or_default(),
                    name,
                    muted: m.self_mute.unwrap_or(false),
                    deafened: m.self_deaf.unwrap_or(false),
                    video: m.self_video.unwrap_or(false),
                    stream: m.self_stream.unwrap_or(false),
                    reconnecting: m.connection_state.as_deref() == Some("reconnecting")
                        || m.connected == Some(false),
                    user_id: m.user_id,
                }
            })
            .collect()
    }

    /// The voice channels changed: the tree, the roster and the open view.
    fn sync_voice_channels(&mut self, fx: &mut Effects) {
        if self.voice_view.is_some() {
            self.emit_voice_panel(fx);
        }
        self.sync_roster(fx);
        self.sync_tree(fx);
        self.sync_members(fx);
        self.sync_servers(fx);
    }

    fn active_server_name(&self) -> String {
        let id = self.active_server.as_deref();
        self.servers
            .iter()
            .find(|s| Some(s.id.as_str()) == id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    fn emit_typing(&mut self, fx: &mut Effects) {
        let names: Vec<&str> = self.typing.iter().map(|t| t.name.as_str()).collect();
        let text = view::typing_text(&names);
        if text != self.typing_text {
            self.typing_text.clone_from(&text);
            fx.view(ViewUpdate::Typing(text));
        }
        self.sync_members(fx);
    }

    fn clear_typing(&mut self, fx: &mut Effects) {
        self.typing.clear();
        self.emit_typing(fx);
    }

    /// A user's name for the typing line: what messages and the member
    /// list said, else "Someone" (useChatArea.ts).
    fn user_name(&self, user_id: &str) -> String {
        if let Some(name) = self.names.get(user_id) {
            return name.clone();
        }
        self.active_server
            .as_ref()
            .and_then(|server| self.members.get(server))
            .and_then(|list| list.iter().find(|m| m.user.id == user_id))
            .map(|m| &m.user)
            .or_else(|| {
                self.dms
                    .iter()
                    .map(|dm| &dm.recipient)
                    .find(|u| u.id == user_id)
            })
            .map_or_else(|| "Someone".to_owned(), |u| u.label().to_owned())
    }

    /// The open channel, and whether this user may delete others'
    /// messages in it (useChatArea.ts `canDeleteMessages`; the channel's
    /// `permissions` are the user's effective ones there).
    fn emit_active_channel(&mut self, fx: &mut Effects, id: String, name: String) {
        fx.view(ViewUpdate::ActiveChannel { id, name });
        fx.view(ViewUpdate::ManageMessages(self.can_manage_messages()));
        // The tray shows the open channel's uploads.
        self.sync_uploads(fx);
    }

    fn can_manage_messages(&self) -> bool {
        self.active_channel
            .as_deref()
            .and_then(|id| self.find_channel(id))
            .and_then(|c| c.permissions)
            .is_some_and(|p| p & ADMINISTRATOR != 0 || p & MANAGE_MESSAGES != 0)
    }

    // Unread

    /// The banner and the divider's scalars.
    fn emit_unread(&self, fx: &mut Effects) {
        let view = self
            .divider
            .as_ref()
            .map(|d| {
                let sent = self
                    .messages
                    .as_ref()
                    .and_then(|log| log.get(&d.message_id))
                    .and_then(|m| parse_rfc3339(&m.created_at))
                    .map(|ms| Stamp::new(ms, self.local_offset))
                    .unwrap_or_default();
                UnreadView {
                    divider_id: d.message_id.clone(),
                    count: d.count,
                    banner: d.banner,
                    since_time: sent.time,
                    since_date: sent.date,
                    since_day: sent.day,
                }
            })
            .unwrap_or_default();
        fx.view(ViewUpdate::Unread(view));
    }

    /// Moves (or drops) the divider, updating the rows it leaves and joins.
    fn set_divider(&mut self, fx: &mut Effects, divider: Option<Divider>) {
        let indices: Vec<usize> = [self.divider.as_ref(), divider.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|d| self.position_of(&d.message_id))
            .collect();
        self.touch_rows(fx, &indices, |state| state.divider = divider);
        self.emit_unread(fx);
    }

    /// Keeps the newest message time per channel.
    fn bump_last_message(&mut self, channel_id: &str, at: &str) {
        let newer = self
            .last_message_at
            .get(channel_id)
            .is_none_or(|last| is_after(at, last));
        if newer && !at.is_empty() {
            self.last_message_at
                .insert(channel_id.to_owned(), at.to_owned());
        }
    }

    /// Marks a channel read up to `at` (its newest message by default),
    /// optimistically; skipped when it already is (chat-actions.ts).
    fn mark_read(&mut self, fx: &mut Effects, channel_id: &str, at: Option<&str>) {
        let Some(mark) = at
            .map(str::to_owned)
            .or_else(|| self.last_message_at.get(channel_id).cloned())
        else {
            return;
        };
        let previous = self.read_states.get(channel_id).cloned();
        if previous
            .as_deref()
            .is_some_and(|read| !is_after(&mark, read))
        {
            return;
        }
        self.read_states.insert(channel_id.to_owned(), mark.clone());
        let request = ApiRequest::MarkRead {
            channel_id: channel_id.to_owned(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending
                .reads
                .insert(id, (channel_id.to_owned(), previous, mark));
        }
        self.sync_badges(fx);
    }

    /// Marks read the unread notifications for messages in the open channel
    /// that have been on screen: the list is at its end, which shows the
    /// rows from the top one it reported, the divider's batch and whatever
    /// arrived while it was open, not rows far above (useChatArea.ts marks
    /// only messages that become visible).
    /// `all` (Mark as read) takes every loaded one.
    fn read_seen_mentions(&mut self, fx: &mut Effects, all: bool) {
        let Some(log) = &self.messages else {
            return;
        };
        let from = [
            self.divider.as_ref().map(|d| d.message_id.as_str()),
            self.seen_from.as_deref(),
        ]
        .into_iter()
        .flatten()
        .filter_map(|id| log.position(id))
        .min();
        let seen = |message_id: &str| {
            let Some(at) = log.position(message_id) else {
                return false;
            };
            all || from.is_some_and(|from| at >= from)
                || log.at(at).is_some_and(|m| {
                    self.seen_floor
                        .as_deref()
                        .is_none_or(|floor| is_after(&m.created_at, floor))
                })
        };
        let ids: Vec<String> = self
            .notifications
            .iter()
            .filter(|n| n.channel_id.as_deref() == Some(log.channel_id.as_str()))
            .filter(|n| n.message_id.as_deref().is_some_and(seen))
            .map(|n| n.id.clone())
            .collect();
        if ids.is_empty() {
            return;
        }
        self.notifications.retain(|n| !ids.contains(&n.id));
        self.inbox_read(Some(&ids));
        let _ = self.http(fx, ApiRequest::MarkNotificationsRead { ids });
        self.sync_badges(fx);
    }

    /// The open channel shows its end in a focused window: read it, clear
    /// its seen mentions and drop the banner (useChatArea.ts
    /// `handleAtBottom`). Coming back into focus there also locks the
    /// divider (`onReturn`). Not before the read states have answered: the
    /// divider is not placed yet, and marking would lose it everywhere.
    fn on_seen(&mut self, fx: &mut Effects, returned: bool) {
        if !(self.focused && self.at_bottom)
            || self.pending.page.is_some()
            || self.pending.read_states.is_some()
            || self.detached()
        {
            return;
        }
        let Some(channel) = self.active_channel.clone() else {
            return;
        };
        if self
            .messages
            .as_ref()
            .is_none_or(|l| l.channel_id != channel)
        {
            return;
        }
        if let Some(divider) = self
            .divider
            .as_mut()
            .filter(|d| d.banner || (returned && !d.locked))
        {
            divider.banner = false;
            divider.locked |= returned;
            self.emit_unread(fx);
        }
        // A Mark unread waits for the user.
        if self.read_held() {
            return;
        }
        self.mark_read(fx, &channel, None);
        self.read_seen_mentions(fx, false);
    }

    /// Where the divider goes in `log`: above the first message after the
    /// read mark, when the channel has unread messages (useChatArea.ts).
    fn divider_for(&self, log: &MessageLog) -> Option<Divider> {
        let read = self
            .read_states
            .get(&log.channel_id)
            .filter(|_| self.has_unread(&log.channel_id))?;
        let first = log
            .iter()
            .position(|m| !is_pending(m) && is_after(&m.created_at, read))?;
        Some(Divider {
            message_id: log.at(first)?.id.clone(),
            count: i64::try_from(log.len() - first).unwrap_or(i64::MAX),
            banner: true,
            locked: false,
        })
    }

    /// Places the divider in the list already on screen when it has none
    /// (read states that answered after the rows): the list lands on it
    /// again, and reports where it is before anything is marked read.
    fn place_divider(&mut self, fx: &mut Effects) {
        if self.divider.is_some() {
            return;
        }
        let Some(log) = self
            .messages
            .as_ref()
            .filter(|l| self.active_channel.as_deref() == Some(l.channel_id.as_str()))
        else {
            return;
        };
        let Some(divider) = self.divider_for(log) else {
            return;
        };
        self.at_bottom = false;
        self.set_divider(fx, Some(divider));
        fx.view(ViewUpdate::Reland);
    }

    /// Read states and notifications (at sign-in and after a fresh
    /// gateway session).
    fn load_unread(&mut self, fx: &mut Effects) {
        self.pending.read_states = self.http(fx, ApiRequest::ReadStates);
        self.pending.notifications = self.http(fx, ApiRequest::Notifications);
        self.pending.dms = self.http(fx, ApiRequest::Dms);
        self.load_friends(fx, false);
    }

    fn mention_query(&mut self, fx: &mut Effects, query: Option<String>) {
        let rows = match query {
            Some(query) => {
                let query = query.to_lowercase();
                let hits: Vec<&User> = self
                    .mention_candidates()
                    .into_iter()
                    .filter(|u| u.username.to_lowercase().contains(&query))
                    .take(MENTION_SUGGESTIONS)
                    .collect();
                view::mention_rows(&hits, &self.api_base)
            }
            None => Vec::new(),
        };
        for diff in view::ordered_diff(&self.view.mentions, &rows) {
            fx.view(ViewUpdate::Mentions(diff));
        }
        self.view.mentions = rows;
    }

    // Snapshot cache (write-through; the writer coalesces)

    /// Stores part of the signed-in user's snapshot.
    fn persist(&self, fx: &mut Effects, write: CacheWrite) {
        if let Some(me) = self.me() {
            fx.push(Effect::CacheWrite {
                user_id: me.id.clone(),
                write,
            });
        }
    }

    fn persist_route(&self, fx: &mut Effects) {
        // Home is not stored: the next launch opens the last server.
        if self.home {
            return;
        }
        self.persist(
            fx,
            CacheWrite::Route {
                server_id: self.active_server.clone(),
                channel_id: self.active_channel.clone(),
            },
        );
    }

    /// The open channel's newest window. Not while a reaction on one of its
    /// messages is unconfirmed: the optimistic chip must not reach the
    /// cache, where a refusal after leaving the channel could not undo it.
    fn persist_messages(&self, fx: &mut Effects) {
        // A jump's window is not the newest one.
        let Some(log) = self.messages.as_ref().filter(|log| !log.has_more_after) else {
            return;
        };
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

    /// Wipes the signed-in user's snapshot (sign-out, session gone).
    fn forget_user(&self, fx: &mut Effects) {
        if let Some(me) = self.me() {
            fx.push(Effect::CacheClear {
                user_id: me.id.clone(),
            });
        }
    }

    // Commands

    fn on_command(&mut self, fx: &mut Effects, command: Command) {
        match command {
            Command::Start => {
                // The keyring and the cache are read in parallel.
                self.startup = Startup::Waiting {
                    token: None,
                    snapshot: None,
                };
                fx.push(Effect::LoadToken);
                fx.push(Effect::LoadSnapshot);
            }
            Command::StartSignIn => {
                let attempt = SignInId(self.next_id());
                self.sign_in = Some(attempt);
                self.set_phase(fx, Phase::DeviceCode);
                fx.view(ViewUpdate::DeviceCode {
                    user_code: String::new(),
                    url: String::new(),
                });
                fx.push(Effect::SignInStart(attempt));
            }
            Command::CancelSignIn => {
                if self.sign_in.take().is_some() {
                    fx.push(Effect::SignInCancel);
                }
                self.set_phase(fx, Phase::SignedOut);
            }
            Command::UseToken(token) => {
                let token = token.trim();
                if !token.is_empty() {
                    self.begin_session(fx, Token::new(token));
                }
            }
            Command::SignOut => {
                self.forget_user(fx);
                self.end_session(fx, true);
            }
            Command::SelectServer(id) => self.select_server(fx, id),
            Command::SelectHome => self.select_home(fx),
            Command::SelectChannel(id) => self.select_channel(fx, &id),
            Command::OpenChannel {
                channel_id,
                server_id,
            } => self.open_channel(fx, &channel_id, server_id),
            Command::OpenDm(user_id) => self.open_dm(fx, &user_id),
            Command::ToastReply {
                channel_id,
                message_id,
                content,
                nonce,
                sent_at,
            } => self.toast_reply(fx, &channel_id, message_id, content.trim(), nonce, sent_at),
            Command::ToastMarkRead(channel_id) => self.toast_mark_read(fx, &channel_id),
            Command::SetStatus {
                status,
                custom_status,
            } => self.set_presence(fx, &status, custom_status),
            Command::ShowProfile(user_id) => self.show_profile(fx, user_id),
            Command::HideProfile(user_id) => {
                if self.profile_user() == Some(user_id.as_str()) {
                    self.show_profile(fx, None);
                }
            }
            Command::Send {
                content,
                reply_to_id,
                nonce,
                sent_at,
            } => self.send(fx, content.trim(), reply_to_id, nonce, sent_at),
            Command::RetrySend(nonce) => self.retry_send(fx, &nonce),
            Command::DiscardSend(nonce) => {
                if self.outbox.remove(&nonce).is_some() {
                    self.remove_row(fx, &pending_id(&nonce));
                }
            }
            Command::EditMessage {
                message_id,
                content,
            } => self.edit_message(fx, &message_id, content.trim()),
            Command::DeleteMessage(id) => self.delete_message(fx, &id),
            Command::WindowShown(shown) => self.set_window_shown(fx, shown),
            Command::Typing => {
                if let (Some(channel_id), Some(_)) = (self.active_channel.clone(), self.signed_in())
                {
                    let _ = self.http(fx, ApiRequest::Typing { channel_id });
                }
            }
            Command::LoadOlder => self.load_older(fx),
            Command::SetAtBottom(at_bottom) => {
                self.at_bottom = at_bottom;
                self.on_seen(fx, false);
            }
            Command::SeenFrom(id) => self.seen_from = Some(id),
            Command::SetFocused(focused) => {
                let returned = focused && !self.focused;
                self.focused = focused;
                self.on_seen(fx, returned);
            }
            Command::MarkRead => {
                self.release_read_hold();
                if let Some(channel) = self.active_channel.clone() {
                    self.mark_read(fx, &channel, None);
                    self.read_seen_mentions(fx, true);
                    self.set_divider(fx, None);
                }
            }
            Command::MentionQuery(query) => self.mention_query(fx, query),
            Command::ToggleReaction { message_id, emoji } => {
                self.toggle_reaction(fx, &message_id, &emoji);
            }
            Command::AddUploads(files) => self.add_uploads(fx, files),
            Command::RemoveUpload(id) => self.remove_upload(fx, &id),
            Command::RetryUpload(id) => self.retry_upload(fx, &id),
            Command::SetUploadSensitive { local_id, nsfw } => {
                self.set_upload_sensitive(fx, &local_id, nsfw);
            }
            Command::GifSearch { query, media_type } => self.gif_search(fx, &query, &media_type),
            Command::GifMore => self.gif_more(fx),
            Command::SendGif {
                gif_id,
                reply_to_id,
                nonce,
                sent_at,
            } => self.send_gif(fx, &gif_id, reply_to_id, nonce, sent_at),
            Command::Download { url, dest, open } => self.download(fx, &url, &dest, open),
            Command::LoadAccount => self.load_account(fx),
            Command::LoadSessions => self.load_sessions(fx),
            Command::LoadShares => self.load_shares(fx),
            Command::RevokeShare(share_id) => self.revoke_share(fx, share_id),
            Command::SaveDisplayName(name) => self.save_display_name(fx, &name),
            Command::SaveProfile(draft) => self.save_profile(fx, &draft),
            Command::SetMediaFilter(level) => self.set_media_filter(fx, &level),
            Command::SaveTheme { theme, sync } => self.save_theme(fx, theme, sync),
            Command::JoinVoice(id) => self.join_voice(fx, id),
            Command::WatchStream {
                participant_id,
                watch,
            } => self.watch_stream(fx, participant_id, watch),
            Command::LeaveVoice => self.leave_voice(fx),
            Command::StartVoiceShare(share) => {
                self.voice_share(fx, |call| {
                    call.share_wanted = Some(share.clone());
                    call.share_error.clear();
                    Some(VoiceCommand::StartShare(share))
                });
            }
            Command::StopVoiceShare => {
                self.voice_share(fx, |call| {
                    call.share_wanted.take().map(|_| VoiceCommand::StopShare)
                });
            }
            Command::SetVoiceShareAudio(on) => {
                self.voice_share(fx, |call| {
                    let wanted = call.share_wanted.as_mut()?;
                    wanted.audio = on;
                    Some(VoiceCommand::SetShareAudio(on))
                });
            }
            Command::SetVoiceShareQuality(quality) => {
                self.voice_share(fx, |call| {
                    let wanted = call.share_wanted.as_mut()?;
                    wanted.quality.clone_from(&quality);
                    Some(VoiceCommand::SetShareQuality(quality))
                });
            }
            Command::SetVoiceCamera(on) => self.set_voice_camera(fx, on, None),
            Command::VoiceCameraFailed(reason) => {
                self.set_voice_camera(fx, false, Some(reason));
            }
            // useVoiceSettingsStore.ts: unmuting while deafened undeafens;
            // a mute change outside deafen is the one to come back to.
            Command::SetVoiceMuted(muted) => {
                // useVoiceChannel.ts `toggleMic`, in or out of a call.
                if muted != self.voice_prefs.muted {
                    let sound = if muted { Sound::Mute } else { Sound::Unmute };
                    fx.view(ViewUpdate::Sound(sound));
                }
                let mut prefs = self.voice_prefs;
                if !muted {
                    prefs.deafened = false;
                }
                if !prefs.deafened {
                    prefs.muted_before_deafen = muted;
                }
                prefs.muted = muted;
                self.set_voice_prefs(fx, prefs);
            }
            // Deafening mutes too and remembers the mic; undeafening puts
            // it back.
            Command::SetVoiceDeafened(deafened) => {
                if deafened != self.voice_prefs.deafened {
                    let sound = if deafened {
                        Sound::Deafen
                    } else {
                        Sound::Undeafen
                    };
                    fx.view(ViewUpdate::Sound(sound));
                }
                let mut prefs = self.voice_prefs;
                if deafened && !prefs.deafened {
                    prefs.muted_before_deafen = prefs.muted;
                    prefs.muted = true;
                } else if !deafened && prefs.deafened {
                    prefs.muted = prefs.muted_before_deafen;
                }
                prefs.deafened = deafened;
                self.set_voice_prefs(fx, prefs);
            }
            Command::Admin(command) => self.on_admin(fx, command),
            Command::JumpToMessage {
                channel_id,
                message_id,
            } => self.jump_to_message(fx, &channel_id, &message_id),
            Command::LoadNewer => self.load_newer(fx),
            Command::JumpToPresent => self.jump_to_present(fx),
            Command::Thread(command) => self.on_thread(fx, command),
            Command::Pins(command) => self.on_pins(fx, command),
            Command::Search(command) => self.on_search(fx, command),
            Command::Friends(command) => self.on_friends(fx, command),
            Command::Apps(command) => self.on_apps(fx, command),
            Command::Shop(command) => self.on_shop(fx, command),
            Command::Embeds(command) => self.on_embeds(fx, command),
            Command::Favorites(command) => self.on_favorites(fx, command),
            Command::Backgrounds(command) => self.on_backgrounds(fx, command),
            Command::RemoveEmbeds(id) => self.remove_embeds(fx, &id),
            Command::Extras(command) => self.on_extras(fx, command),
            Command::Calls(command) => self.on_calls(fx, command),
            Command::Quit => {
                if let Some(standby) = self.standby.take() {
                    fx.push(Effect::Voice {
                        call: standby.id,
                        command: VoiceCommand::Leave,
                    });
                }
                self.leave_voice(fx);
                // A ring still going: an outgoing one is cancelled (op 39) so
                // the peer stops ringing, and ours goes quiet.
                let status = self.calls.ring.as_ref().map(|r| r.status);
                if matches!(
                    status,
                    Some(RingStatus::RingingIncoming | RingStatus::RingingOutgoing)
                ) {
                    if status == Some(RingStatus::RingingOutgoing) {
                        self.on_calls(fx, CallsCommand::Cancel);
                    }
                    self.end_ring(fx, "left");
                    self.sync_calls(fx);
                }
            }
            Command::Invites(command) => self.on_invites(fx, command),
        }
    }

    // Session

    fn on_token(&mut self, fx: &mut Effects, token: Option<Token>) {
        match &mut self.startup {
            Startup::Idle => match token {
                Some(token) => self.begin_session(fx, token),
                None => self.set_phase(fx, Phase::SignedOut),
            },
            Startup::Waiting { token: slot, .. } => {
                *slot = Some(token);
                self.launch(fx);
            }
            Startup::Done => {}
        }
    }

    fn on_snapshot(&mut self, fx: &mut Effects, snapshot: Option<Box<Snapshot>>) {
        if let Startup::Waiting { snapshot: slot, .. } = &mut self.startup {
            *slot = Some(snapshot);
            self.launch(fx);
        }
    }

    /// Starts the launch once the token and the snapshot are both read.
    fn launch(&mut self, fx: &mut Effects) {
        let Startup::Waiting {
            token: Some(_),
            snapshot: Some(_),
        } = &self.startup
        else {
            return;
        };
        let Startup::Waiting {
            token: Some(token),
            snapshot: Some(snapshot),
        } = std::mem::replace(&mut self.startup, Startup::Done)
        else {
            return;
        };
        match (token, snapshot) {
            (None, snapshot) => {
                // Signed out since: what the snapshot shows is no one's now.
                if let Some(snapshot) = snapshot {
                    fx.push(Effect::CacheClear {
                        user_id: snapshot.me.id,
                    });
                }
                self.set_phase(fx, Phase::SignedOut);
            }
            (Some(token), None) => self.begin_session(fx, token),
            (Some(token), Some(snapshot)) => self.begin_cached_session(fx, token, *snapshot),
        }
    }

    /// Ends whatever ran before a new session (and any launch still waiting).
    fn replace_session(&mut self, fx: &mut Effects) {
        // A pasted token wins over a device sign-in still in flight; its
        // later approval would otherwise replace this session.
        if self.sign_in.take().is_some() {
            fx.push(Effect::SignInCancel);
        }
        if matches!(self.startup, Startup::Waiting { .. }) {
            self.startup = Startup::Done;
        }
        self.end_session(fx, false);
    }

    /// Shows the last session at once and revalidates all of it in parallel:
    /// `me`, servers, the cached server's channels and the cached channel's
    /// page, with the gateway (its ticket) connecting alongside as the
    /// cached user. Results reconcile through the same diffs as live events.
    fn begin_cached_session(&mut self, fx: &mut Effects, token: Token, snapshot: Snapshot) {
        self.replace_session(fx);
        let Snapshot {
            me,
            servers,
            channels,
            active_server,
            active_channel,
            messages,
            voice,
            ..
        } = snapshot;
        if let Some(prefs) = voice.filter(|p| *p != self.voice_prefs) {
            self.voice_prefs = prefs;
            self.emit_voice(fx);
        }
        let session = SessionId(self.next_id());
        fx.push(Effect::GatewayConnect {
            session,
            token: token.clone(),
            user_id: me.id.clone(),
        });
        self.names.insert(me.id.clone(), me.label().to_owned());
        fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
        fx.view(ViewUpdate::MediaAuth(Some(token.clone())));
        self.session = Some(Session {
            id: session,
            token,
            me: Some(me),
            gateway: Default::default(),
        });
        self.refreshing = true;

        self.servers = servers;
        self.sync_servers(fx);
        self.channels = channels
            .into_iter()
            .map(|(id, tree)| (id, ServerChannels::new(tree)))
            .collect();
        let server = active_server
            .filter(|id| self.servers.iter().any(|s| &s.id == id))
            .or_else(|| self.servers.first().map(|s| s.id.clone()));
        if let Some(server) = &server {
            self.active_server = Some(server.clone());
            fx.view(ViewUpdate::ActiveServer {
                id: server.clone(),
                name: self.active_server_name(),
            });
            if self.channels.contains_key(server) {
                self.tree_server = Some(server.clone());
                self.reset_tree(fx);
            }
        }
        let channel = active_channel.filter(|_| server.is_some()).and_then(|id| {
            let list = self.channels.get(server.as_deref()?)?;
            list.get(&id)
                .filter(|c| c.channel_type == ChannelType::Text)
                .map(|c| (id.clone(), c.name.clone()))
        });
        if let Some((id, name)) = &channel {
            self.active_channel = Some(id.clone());
            self.emit_active_channel(fx, id.clone(), name.clone());
            let mut log = MessageLog::new(id);
            // Unknown until the page answers; it keeps what is older.
            log.has_more_before = true;
            for message in messages.into_iter().filter(|m| &m.channel_id == id) {
                self.remember_author(&message);
                log.upsert(message);
            }
            let shown = !log.is_empty();
            self.seen_floor = log.iter().last().map(|m| m.created_at.clone());
            self.messages = Some(log);
            fx.messages(ViewDiff::Reset(self.message_rows()));
            self.emit_history(fx);
            if shown {
                fx.push(Effect::StartupMark("cached content"));
            }
        }
        self.set_phase(fx, Phase::Ready);
        self.set_status(fx, REFRESHING);

        self.pending.me = self.http(fx, ApiRequest::Me);
        self.pending.servers = self.http(fx, ApiRequest::Servers);
        if let Some(server_id) = server {
            self.request_members(fx, &server_id);
            let request = ApiRequest::Channels {
                server_id: server_id.clone(),
            };
            self.pending.channels = self.http(fx, request).map(|req| (req, server_id));
        }
        if let Some((channel_id, _)) = channel {
            self.load_page(fx, &channel_id);
        }
        // Before the page answers, so the divider can be placed in the
        // cached rows (chat-actions.ts loads read states before the channel).
        self.load_unread(fx);
    }

    /// Ends the revalidation once every request it started has answered.
    fn settle_refresh(&mut self, fx: &mut Effects) {
        let pending = &self.pending;
        let waiting = pending.me.is_some()
            || pending.servers.is_some()
            || pending.channels.is_some()
            || pending.page.is_some();
        if self.refreshing && !waiting {
            self.refreshing = false;
            // An error shown meanwhile stays.
            if self.status == REFRESHING {
                self.set_status(fx, "");
            }
        }
    }

    fn begin_session(&mut self, fx: &mut Effects, token: Token) {
        self.replace_session(fx);
        self.set_phase(fx, Phase::Loading);
        self.set_status(fx, "Signing in…");
        fx.view(ViewUpdate::MediaAuth(Some(token.clone())));
        self.session = Some(Session {
            id: SessionId(self.next_id()),
            token,
            me: None,
            gateway: Default::default(),
        });
        self.boot = Some(Boot::default());
        // Independent requests overlap: each one costs a Kova session check.
        self.pending.me = self.http(fx, ApiRequest::Me);
        self.pending.servers = self.http(fx, ApiRequest::Servers);
    }

    fn end_session(&mut self, fx: &mut Effects, revoke: bool) {
        self.clear_settings();
        self.clear_admin();
        self.clear_panels();
        self.clear_search();
        self.clear_friends();
        self.clear_invites();
        self.clear_shop();
        self.leave_voice(fx);
        self.calls = Default::default();
        fx.view(ViewUpdate::StopSound(Sound::Ring));
        fx.view(ViewUpdate::StopSound(Sound::OutgoingRing));
        self.close_voice_panel(fx);
        self.voice_channels.clear();
        // The next user brings their own (from their snapshot).
        self.voice_prefs = VoicePrefs::default();
        self.voice_error.clear();
        if let Some(session) = self.session.take() {
            fx.view(ViewUpdate::MediaAuth(None));
            fx.push(Effect::GatewayDisconnect);
            if session.me.is_some() {
                fx.view(ViewUpdate::Me(MeView::default()));
            }
            if revoke {
                fx.push(Effect::Revoke(session.token));
            }
        }
        if revoke {
            fx.push(Effect::ClearToken);
        }
        self.servers.clear();
        self.channels.clear();
        if self.active_server.take().is_some() {
            fx.view(ViewUpdate::ActiveServer {
                id: String::new(),
                name: String::new(),
            });
        }
        if self.active_channel.take().is_some() {
            self.emit_active_channel(fx, String::new(), String::new());
        }
        if self.messages.take().is_some() {
            fx.messages(ViewDiff::Reset(Vec::new()));
        }
        self.names.clear();
        self.clear_typing(fx);
        self.read_states.clear();
        self.last_message_at.clear();
        self.notifications.clear();
        self.members.clear();
        self.clear_social(fx);
        self.wanted_channel = None;
        self.toasted = shell::Toasted::default();
        self.sync_tray_badge(fx);
        self.outbox.clear();
        self.clear_media(fx);
        self.at_bottom = false;
        if self.divider.take().is_some() {
            self.emit_unread(fx);
        }
        self.mention_query(fx, None);
        self.tree_server = None;
        self.sync_servers(fx);
        self.sync_tree(fx);
        self.sync_calls(fx);
        self.pending = Pending::default();
        self.boot = None;
        self.refreshing = false;
        self.set_connected(fx, false);
        self.set_phase(fx, Phase::SignedOut);
    }

    /// A request failed: a 401 ends the session, anything else is a status line.
    fn fail(&mut self, fx: &mut Effects, error: &ApiError) {
        if error.is_unauthorized() {
            self.forget_user(fx);
            self.end_session(fx, false);
            fx.push(Effect::ClearToken);
            self.set_status(fx, SESSION_ENDED);
        } else {
            self.set_status(fx, error.to_string());
        }
    }

    /// The boot chain reached its end.
    fn finish_boot(&mut self, fx: &mut Effects) {
        if self.boot.take().is_some() {
            self.set_phase(fx, Phase::Ready);
            self.set_status(fx, "");
        }
    }

    /// A step of the select/load chain failed. Signed in but a list failed:
    /// show the shell, not the boot screen.
    fn chain_failed(&mut self, fx: &mut Effects, error: &ApiError) {
        if self.boot.take().is_some() {
            self.set_phase(fx, Phase::Ready);
        }
        self.fail(fx, error);
    }

    // Sign-in

    fn on_sign_in(&mut self, fx: &mut Effects, attempt: SignInId, event: SignInEvent) {
        if self.sign_in != Some(attempt) {
            return;
        }
        match event {
            SignInEvent::DeviceCode { user_code, url } => {
                fx.view(ViewUpdate::DeviceCode { user_code, url });
            }
            SignInEvent::Approved(token) => {
                self.sign_in = None;
                self.begin_session(fx, token);
            }
            SignInEvent::Failed(message) => {
                self.sign_in = None;
                self.set_phase(fx, Phase::SignedOut);
                self.set_status(fx, message);
            }
        }
    }

    // HTTP results

    fn on_http(&mut self, fx: &mut Effects, id: RequestId, result: Result<ApiResponse, ApiError>) {
        self.route_http(fx, id, result);
        self.settle_refresh(fx);
    }

    fn route_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: Result<ApiResponse, ApiError>,
    ) {
        let mut answer = Some(result);
        if self.route_calls_http(fx, id, &mut answer)
            || self.route_settings_http(fx, id, &mut answer)
            || self.route_admin_http(fx, id, &mut answer)
            || self.route_thread_http(fx, id, &mut answer)
            || self.route_search_http(fx, id, &mut answer)
            || self.route_friends_http(fx, id, &mut answer)
            || self.route_invites_http(fx, id, &mut answer)
            || self.route_apps_http(fx, id, &mut answer)
            || self.route_shop_http(fx, id, &mut answer)
            || self.route_embeds_http(fx, id, &mut answer)
            || self.route_favorites_http(fx, id, &mut answer)
            || self.route_backgrounds_http(fx, id, &mut answer)
            || self.route_extras_http(fx, id, &mut answer)
        {
            return;
        }
        let Some(result) = answer else {
            return;
        };
        let pending = &mut self.pending;
        if pending.me == Some(id) {
            pending.me = None;
            return self.on_me(fx, result);
        }
        if pending.servers == Some(id) {
            pending.servers = None;
            let servers = result.and_then(|response| match response {
                ApiResponse::Servers(list) => Ok(list),
                other => Err(unexpected(&other)),
            });
            return self.on_servers(fx, servers);
        }
        if let Some((_, server_id)) = pending.channels.take_if(|(slot, _)| *slot == id) {
            return self.on_channels(fx, server_id, result);
        }
        if let Some((_, channel_id)) = pending.page.take_if(|(slot, _)| *slot == id) {
            return self.on_page(fx, &channel_id, result);
        }
        if let Some((_, channel_id)) = pending.older.take_if(|(slot, _)| *slot == id) {
            return self.on_older(fx, &channel_id, result);
        }
        if let Some(server_id) = pending.trees.remove(&id) {
            return self.on_tree(fx, server_id, result);
        }
        if pending.read_states == Some(id) {
            pending.read_states = None;
            return self.on_read_states(fx, result);
        }
        if pending.notifications == Some(id) {
            pending.notifications = None;
            return self.on_notifications(fx, result);
        }
        if let Some(server_id) = pending.members.remove(&id) {
            return self.on_members(fx, server_id, result);
        }
        if pending.dms == Some(id) {
            pending.dms = None;
            return self.on_dms(fx, result);
        }
        if let Some((_, user_id)) = pending.open_dm.take_if(|(slot, _)| *slot == id) {
            return self.on_dm_opened(fx, &user_id, result);
        }
        if let Some((_, user_id)) = pending.profile.take_if(|(slot, _)| *slot == id) {
            return self.on_profile(fx, &user_id, result);
        }
        if let Some(read) = pending.reads.remove(&id) {
            return self.on_marked(fx, read, result);
        }
        if let Some(original) = pending.edits.remove(&id) {
            return self.on_edited(fx, original, result);
        }
        if let Some(message) = pending.deletes.remove(&id) {
            return self.on_deleted(fx, message, result);
        }
        if let Some(nonce) = pending.sends.remove(&id) {
            return self.on_sent(fx, &nonce, result);
        }
        if let Some(change) = pending.reactions.remove(&id) {
            return self.on_reacted(fx, change, result);
        }
        if let Some(local_id) = pending.uploads.remove(&id) {
            return self.on_uploaded(fx, &local_id, result);
        }
        if let Some(nonce) = pending.gif_uploads.remove(&id) {
            return self.on_gif_uploaded(fx, &nonce, result);
        }
        if let Some((_, more)) = pending.gifs.take_if(|(slot, _)| *slot == id) {
            return self.on_gifs(fx, more, result);
        }
        if let Some(download) = pending.downloads.remove(&id) {
            self.on_downloaded(fx, download, result);
        }
        // Anything else is stale (a newer request replaced it, or the
        // session ended) or fire-and-forget (typing).
    }

    fn on_me(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let me = match result {
            Ok(ApiResponse::Me(me)) => *me,
            Ok(other) => return self.me_failed(fx, &unexpected(&other)),
            Err(error) => return self.me_failed(fx, &error),
        };
        self.apps_on_me(&me);
        let Some(session) = &mut self.session else {
            return;
        };
        if let Some(cached) = &session.me {
            // A session shown from the cache: the gateway is already up as
            // the cached user.
            if cached.id != me.id {
                // The token is someone else's: drop the cached view and
                // start over as them.
                fx.push(Effect::CacheClear {
                    user_id: cached.id.clone(),
                });
                let token = session.token.clone();
                return self.begin_session(fx, token);
            }
            fx.push(Effect::StoreToken(session.token.clone()));
            if *cached != me {
                self.names.insert(me.id.clone(), me.label().to_owned());
                fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
            }
            session.me = Some(me.clone());
            // Its read states and notifications were asked for already.
            self.persist(fx, CacheWrite::Me(Box::new(me)));
            return;
        }
        fx.push(Effect::StoreToken(session.token.clone()));
        self.names.insert(me.id.clone(), me.label().to_owned());
        fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
        fx.push(Effect::GatewayConnect {
            session: session.id,
            token: session.token.clone(),
            user_id: me.id.clone(),
        });
        session.me = Some(me.clone());
        self.persist(fx, CacheWrite::Me(Box::new(me)));
        self.load_unread(fx);
        self.set_status(fx, "Loading servers…");
        let early = self.boot.as_mut().and_then(|boot| {
            boot.me_done = true;
            boot.early_servers.take()
        });
        if let Some(servers) = early {
            self.on_servers(fx, servers);
        }
    }

    fn me_failed(&mut self, fx: &mut Effects, error: &ApiError) {
        if self.me().is_some() {
            // A cached session: a 401 signs out (and wipes the snapshot);
            // anything else keeps the cached view up, offline.
            if !error.is_unauthorized() {
                self.refreshing = false;
            }
            return self.fail(fx, error);
        }
        self.session = None;
        self.boot = None;
        self.pending = Pending::default();
        self.set_phase(fx, Phase::SignedOut);
        // On a 401 this ends the session and clears the stored token.
        self.fail(fx, error);
    }

    fn on_servers(&mut self, fx: &mut Effects, result: Result<Vec<Server>, ApiError>) {
        if let Some(boot) = &mut self.boot {
            if !boot.me_done {
                boot.early_servers = Some(result);
                return;
            }
        }
        match result {
            Ok(servers) => self.apply_servers(fx, servers),
            Err(error) => self.chain_failed(fx, &error),
        }
    }

    fn apply_servers(&mut self, fx: &mut Effects, servers: Vec<Server>) {
        self.select_servers(fx, servers);
        self.prefetch_trees(fx);
    }

    /// Every other server's channel tree, for its unread pill.
    fn prefetch_trees(&mut self, fx: &mut Effects) {
        let wanted: Vec<String> = self
            .servers
            .iter()
            .map(|s| s.id.clone())
            .filter(|id| Some(id) != self.active_server.as_ref())
            .filter(|id| !self.pending.trees.values().any(|pending| pending == id))
            .collect();
        for server_id in wanted {
            let request = ApiRequest::Channels {
                server_id: server_id.clone(),
            };
            if let Some(id) = self.http(fx, request) {
                self.pending.trees.insert(id, server_id);
            }
        }
    }

    fn select_servers(&mut self, fx: &mut Effects, servers: Vec<Server>) {
        self.servers = servers;
        self.sync_servers(fx);
        self.persist(fx, CacheWrite::Servers(self.servers.clone()));
        let keep = self
            .active_server
            .clone()
            .filter(|id| self.servers.iter().any(|s| &s.id == id));
        match keep {
            // Revalidating the cached server: its channels were requested
            // with everything else, so only its name can have changed.
            // Home stays where it is.
            Some(id) if self.refreshing || self.home => {
                let name = self.active_server_name();
                fx.view(ViewUpdate::ActiveServer { id, name });
            }
            None if self.home => {
                // The server behind Home went away: the next one takes its
                // place without leaving Home.
                self.active_server = self.servers.first().map(|s| s.id.clone());
                let id = self.active_server.clone().unwrap_or_default();
                let name = self.active_server_name();
                fx.view(ViewUpdate::ActiveServer { id, name });
                self.sync_members(fx);
            }
            Some(id) => self.select_server(fx, id),
            None => match self.servers.first().map(|s| s.id.clone()) {
                Some(first) => self.select_server(fx, first),
                None => {
                    self.clear_route(fx);
                    self.finish_boot(fx);
                }
            },
        }
    }

    /// No server is left (a cached one went away): clear what showed it.
    fn clear_route(&mut self, fx: &mut Effects) {
        if self.active_server.take().is_none() {
            return;
        }
        fx.view(ViewUpdate::ActiveServer {
            id: String::new(),
            name: String::new(),
        });
        self.tree_server = None;
        self.sync_tree(fx);
        self.pending.channels = None;
        self.clear_channel(fx);
        self.persist_route(fx);
    }

    /// No text channel is open.
    fn clear_channel(&mut self, fx: &mut Effects) {
        self.active_channel = None;
        self.messages = None;
        self.pending.page = None;
        self.pending.older = None;
        self.seen_floor = None;
        self.seen_from = None;
        self.emit_history(fx);
        self.emit_active_channel(fx, String::new(), String::new());
        self.emit_active_dm(fx);
        self.clear_typing(fx);
        if self.divider.take().is_some() {
            self.emit_unread(fx);
        }
        fx.messages(ViewDiff::Reset(Vec::new()));
    }

    fn select_server(&mut self, fx: &mut Effects, id: String) {
        if self.signed_in().is_none() {
            return;
        }
        if self.active_server.as_deref() != Some(id.as_str()) {
            self.close_voice_panel(fx);
        }
        self.leave_home(fx);
        // A DM does not carry over into the server: nothing is open until
        // its channels answer.
        if let Some(dm) = self.active_channel.clone().filter(|c| self.dm(c).is_some()) {
            self.gateway_send(fx, Self::channel_frame(GatewayOp::ChannelUnsubscribe, &dm));
            self.clear_channel(fx);
        }
        self.active_server = Some(id.clone());
        fx.view(ViewUpdate::ActiveServer {
            id: id.clone(),
            name: self.active_server_name(),
        });
        self.persist_route(fx);
        self.request_members(fx, &id);
        let request = ApiRequest::Channels {
            server_id: id.clone(),
        };
        self.pending.channels = self.http(fx, request).map(|req| (req, id));
    }

    /// A server's members, once per session; dispatches keep them current.
    fn request_members(&mut self, fx: &mut Effects, server_id: &str) {
        let asked = self.pending.members.values().any(|s| s == server_id);
        if asked || self.members.contains_key(server_id) {
            return;
        }
        let request = ApiRequest::Members {
            server_id: server_id.to_owned(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.members.insert(id, server_id.to_owned());
        }
    }

    fn on_channels(
        &mut self,
        fx: &mut Effects,
        server_id: String,
        result: Result<ApiResponse, ApiError>,
    ) {
        let response = match result {
            Ok(ApiResponse::Channels(response)) => response,
            Ok(other) => return self.chain_failed(fx, &unexpected(&other)),
            Err(error) => return self.chain_failed(fx, &error),
        };
        let list = ServerChannels::new(response);
        // A refetch of the shown server (e.g. after GUILD_UPDATE) keeps the
        // open channel while it is still one of its text channels.
        let kept = self
            .active_channel
            .as_deref()
            .and_then(|id| list.get(id))
            .filter(|c| c.channel_type == ChannelType::Text)
            .map(|c| (c.id.clone(), c.name.clone()));
        let first_text = list.first_text().map(|c| c.id.clone());
        let same_tree = self.tree_server.as_deref() == Some(server_id.as_str());
        let tree = list.to_response();
        self.persist(
            fx,
            CacheWrite::Channels {
                server_id: server_id.clone(),
                tree,
            },
        );
        self.channels.insert(server_id.clone(), list);
        self.tree_server = Some(server_id);
        if same_tree {
            self.sync_tree(fx);
        } else {
            self.reset_tree(fx);
        }
        self.sync_subscriptions(fx);
        if self.home {
            // A tree that answered after Home opened: kept, not shown.
            return self.finish_boot(fx);
        }
        let wanted = self
            .wanted_channel
            .take()
            .filter(|id| self.find_channel(id).is_some());
        if let Some(id) = wanted {
            self.select_channel(fx, &id);
            return self.finish_boot(fx);
        }
        if let Some((id, name)) = kept {
            // Only the name and permissions can have changed; the
            // messages stay.
            self.emit_active_channel(fx, id, name);
            return self.finish_boot(fx);
        }
        match first_text {
            Some(channel) => self.select_channel(fx, &channel),
            None => {
                self.clear_channel(fx);
                self.persist_route(fx);
                self.finish_boot(fx);
            }
        }
    }

    fn select_channel(&mut self, fx: &mut Effects, id: &str) {
        if self.signed_in().is_none() {
            return;
        }
        if self.dm(id).is_some() {
            return self.select_dm(fx, id);
        }
        let Some(channel) = self.find_channel(id) else {
            return;
        };
        let kind = channel.channel_type;
        let name = channel.name.clone();
        if matches!(kind, ChannelType::Dm | ChannelType::Other) {
            return;
        }
        // A server channel leaves Home.
        self.leave_home(fx);
        match kind {
            ChannelType::Voice => self.open_voice_panel(fx, id),
            _ => self.open_text(fx, id, name),
        }
    }

    /// Opens a text channel or a DM: subscribe, an empty list, the page.
    fn open_text(&mut self, fx: &mut Effects, id: &str, name: String) {
        self.close_voice_panel(fx);
        if let Some(previous) = self.active_channel.replace(id.to_owned()) {
            if previous != id {
                self.gateway_send(
                    fx,
                    Self::channel_frame(GatewayOp::ChannelUnsubscribe, &previous),
                );
            }
        }
        self.messages = Some(MessageLog::new(id));
        self.pending.older = None;
        self.seen_floor = None;
        self.seen_from = None;
        // Where the list lands is the UI's to report once it has rows.
        self.at_bottom = false;
        if self.divider.take().is_some() {
            self.emit_unread(fx);
        }
        let name_for_recent = name.clone();
        self.emit_active_channel(fx, id.to_owned(), name);
        self.emit_active_dm(fx);
        self.clear_typing(fx);
        fx.messages(ViewDiff::Reset(Vec::new()));
        self.emit_history(fx);
        self.persist_route(fx);
        self.gateway_send(fx, Self::channel_frame(GatewayOp::ChannelSubscribe, id));
        self.load_page(fx, id);
        // The open channel stops showing as unread.
        self.sync_badges(fx);
        self.sync_members(fx);
        self.emit_recent(fx, id, name_for_recent);
    }

    /// The switcher's Recent list (CommandMenu.tsx `recordRecentTarget`):
    /// recorded here, once both the open channel and the open DM are set,
    /// so a DM never lands under the server it was opened from or the
    /// other way round.
    fn emit_recent(&self, fx: &mut Effects, id: &str, name: String) {
        let (server_id, server_name) = if self.active_dm().is_some() {
            ("@me".to_owned(), String::new())
        } else {
            (
                self.active_server.clone().unwrap_or_default(),
                self.active_server_name(),
            )
        };
        if server_id.is_empty() || id.is_empty() {
            return;
        }
        fx.view(ViewUpdate::Recent(RecentTarget {
            server_id,
            channel_id: id.to_owned(),
            name,
            voice: false,
            server_name,
        }));
    }

    /// Fetches the newest page of a channel; the result replaces the list.
    fn load_page(&mut self, fx: &mut Effects, channel_id: &str) {
        let request = ApiRequest::Messages {
            channel_id: channel_id.to_owned(),
            before: None,
        };
        self.pending.page = self
            .http(fx, request)
            .map(|req| (req, channel_id.to_owned()));
    }

    fn on_page(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        let Page {
            messages,
            has_more_before,
        } = match result {
            Ok(ApiResponse::Messages(page)) => page,
            Ok(other) => return self.chain_failed(fx, &unexpected(&other)),
            Err(error) => return self.chain_failed(fx, &error),
        };
        if self.active_channel.as_deref() != Some(channel_id) {
            return;
        }
        // A refetch of what is on screen (the cached window, or a refetch
        // after a reconnect) changes only the rows that differ. A jump's
        // window is replaced whole.
        let shown = self.messages.as_ref().is_some_and(|log| {
            log.channel_id == channel_id && !log.is_empty() && !log.has_more_after
        });
        let before = if shown {
            self.message_rows()
        } else {
            Vec::new()
        };
        let old = self.messages.take();
        let mut log = MessageLog::new(channel_id);
        log.has_more_before = has_more_before;
        // A refetch keeps the older pages already loaded.
        let oldest = messages
            .iter()
            .map(|m| m.created_at.as_str())
            .min_by_key(|at| parse_rfc3339(at));
        if let (Some(old), Some(oldest)) = (old.as_ref().filter(|_| shown), oldest) {
            let older = old
                .iter()
                .filter(|m| !is_pending(m) && is_after(oldest, &m.created_at));
            for message in older {
                log.upsert(message.clone());
                log.has_more_before = old.has_more_before;
            }
        }
        for message in messages {
            self.remember_author(&message);
            log.upsert(message);
        }
        // Sends still in flight (or failed) stay at the end.
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
        if let Some(newest) = log.iter().rev().find(|m| !is_pending(m)) {
            let at = newest.created_at.clone();
            self.bump_last_message(channel_id, &at);
        }
        // With unread messages the divider goes above the first one after
        // the read mark (useChatArea.ts), cached rows on screen or not.
        let placed = match &self.divider {
            Some(_) => false,
            None => {
                self.divider = self.divider_for(&log);
                self.divider.is_some()
            }
        };
        if self.seen_floor.is_none() {
            self.seen_floor = log
                .iter()
                .rev()
                .find(|m| !is_pending(m))
                .map(|m| m.created_at.clone());
        }
        self.messages = Some(log);
        let rows = self.message_rows();
        if shown {
            for diff in view::ordered_diff(&before, &rows) {
                fx.messages(diff);
            }
        } else {
            fx.messages(ViewDiff::Reset(rows));
        }
        if placed {
            self.emit_unread(fx);
            if shown {
                // The cached rows had landed at the end: land on it again
                // and hear where the list is before marking it read.
                self.at_bottom = false;
                fx.view(ViewUpdate::Reland);
            }
        }
        self.persist_messages(fx);
        if !std::mem::replace(&mut self.first_page_shown, true) {
            fx.push(Effect::StartupMark("messages shown"));
        }
        self.finish_boot(fx);
        self.emit_history(fx);
        self.sync_badges(fx);
        self.on_seen(fx, false);
        self.after_page_jump(fx, channel_id);
    }

    /// Asks for the page before the oldest loaded message.
    fn load_older(&mut self, fx: &mut Effects) {
        if self.pending.older.is_some() || self.pending.page.is_some() {
            return;
        }
        let Some(log) = self.messages.as_ref().filter(|l| l.has_more_before) else {
            return;
        };
        let Some(first) = log.iter().find(|m| !is_pending(m)) else {
            return;
        };
        let channel_id = log.channel_id.clone();
        let request = ApiRequest::Messages {
            channel_id: channel_id.clone(),
            before: Some(format!("{}|{}", first.created_at, first.id)),
        };
        self.pending.older = self.http(fx, request).map(|id| (id, channel_id));
        self.emit_history(fx);
    }

    /// Whether an older page is loading and whether there are more, for
    /// the list's next ask.
    fn emit_history(&self, fx: &mut Effects) {
        fx.view(ViewUpdate::History {
            loading: self.pending.older.is_some(),
            more: self.messages.as_ref().is_some_and(|l| l.has_more_before),
        });
    }

    /// Every answer ends the loading state, whatever it was.
    fn on_older(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        self.apply_older(fx, channel_id, result);
        self.emit_history(fx);
    }

    /// An older page goes in at the top as one insert; the old first row
    /// may regroup under it.
    fn apply_older(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        let page = match result {
            Ok(ApiResponse::Messages(page)) => page,
            Ok(_) => return,
            Err(error) if error.is_unauthorized() => return self.fail(fx, &error),
            // Scrolling up again retries.
            Err(_) => return,
        };
        if self
            .messages
            .as_ref()
            .is_none_or(|l| l.channel_id != channel_id)
        {
            return;
        }
        for message in &page.messages {
            self.remember_author(message);
        }
        let old_first = self.message_row_at(0);
        let Some(log) = self.messages.as_mut() else {
            return;
        };
        let added = log.prepend(page.messages);
        // A page with nothing new would ask for the same cursor again.
        log.has_more_before = page.has_more_before && added > 0;
        if added == 0 {
            return;
        }
        let rows: Vec<MessageRow> = (0..added).filter_map(|i| self.message_row_at(i)).collect();
        fx.messages(ViewDiff::Insert { at: 0, rows });
        if let (Some(old), Some(row)) = (old_first, self.message_row_at(added)) {
            let roles = old.changed(&row);
            if !roles.is_empty() {
                fx.messages(ViewDiff::Update {
                    at: added,
                    row,
                    roles,
                });
            }
        }
    }

    /// Shows the message at once as a pending row, then posts it with its
    /// nonce; the echo (or the answer) takes the row's place.
    fn send(
        &mut self,
        fx: &mut Effects,
        content: &str,
        reply_to_id: Option<String>,
        nonce: String,
        sent_at: String,
    ) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        if self.signed_in().is_none() || nonce.is_empty() || self.outbox.contains_key(&nonce) {
            return;
        }
        let (attachments, ids, nsfw) = self.take_ready_uploads(&channel_id);
        if content.is_empty() && attachments.is_empty() {
            return;
        }
        // An attachments-only message sends a single space (useMessageInput.ts).
        let content = if content.is_empty() { " " } else { content };
        if self.queue_send(
            fx,
            content,
            reply_to_id,
            nonce.clone(),
            sent_at,
            attachments,
        ) {
            if let Some(out) = self.outbox.get_mut(&nonce) {
                out.attachment_ids = ids;
                out.nsfw_ids = nsfw;
            }
            self.sync_uploads(fx);
            self.post(fx, &nonce);
        }
    }

    /// A toast's Reply. The open channel takes the composer's path (its
    /// pending row shows at once); any other channel is posted to directly,
    /// and the gateway echo brings the message wherever it shows.
    fn toast_reply(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        message_id: String,
        content: &str,
        nonce: String,
        sent_at: String,
    ) {
        if channel_id.is_empty()
            || content.is_empty()
            || nonce.is_empty()
            || self.signed_in().is_none()
            || self.outbox.contains_key(&nonce)
        {
            return;
        }
        let reply_to_id = Some(message_id).filter(|id| !id.is_empty());
        if self.active_channel.as_deref() == Some(channel_id) {
            if self.queue_send(fx, content, reply_to_id, nonce.clone(), sent_at, Vec::new()) {
                self.post(fx, &nonce);
            }
            return;
        }
        let request = ApiRequest::SendMessage {
            channel_id: channel_id.to_owned(),
            content: content.to_owned(),
            reply_to_id,
            nonce: nonce.clone(),
            attachment_ids: Vec::new(),
            nsfw_attachment_ids: Vec::new(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.sends.insert(id, nonce.clone());
            self.pending.toast_replies.insert(nonce, content.to_owned());
        }
    }

    /// A toast's Mark as read (ChatPageClient.tsx `handleMarkRead`): the
    /// channel's unread notifications, then the channel itself.
    fn toast_mark_read(&mut self, fx: &mut Effects, channel_id: &str) {
        if channel_id.is_empty() || self.signed_in().is_none() {
            return;
        }
        let ids: Vec<String> = self
            .notifications
            .iter()
            .filter(|n| n.channel_id.as_deref() == Some(channel_id))
            .map(|n| n.id.clone())
            .collect();
        if !ids.is_empty() {
            self.notifications.retain(|n| !ids.contains(&n.id));
            self.inbox_read(Some(&ids));
            let _ = self.http(fx, ApiRequest::MarkNotificationsRead { ids });
        }
        if self.active_channel.as_deref() == Some(channel_id) {
            self.release_read_hold();
            self.set_divider(fx, None);
        }
        self.mark_read(fx, channel_id, None);
        self.sync_badges(fx);
    }

    /// The pending row and its outbox entry; `false` when nothing is open.
    fn queue_send(
        &mut self,
        fx: &mut Effects,
        content: &str,
        reply_to_id: Option<String>,
        nonce: String,
        sent_at: String,
        attachments: Vec<yap_protocol::rest::Attachment>,
    ) -> bool {
        let Some(channel_id) = self.active_channel.clone() else {
            return false;
        };
        let Some((_, me)) = self.signed_in() else {
            return false;
        };
        if nonce.is_empty() || self.outbox.contains_key(&nonce) {
            return false;
        }
        let me = me.clone();
        // A send from a jump's window goes back to the newest page, where
        // its row shows (the web appends it to the window instead).
        if self.detached() {
            self.jump_to_present(fx);
        }
        let reply_to_id = reply_to_id.filter(|id| !id.is_empty());
        let reply_to = reply_to_id
            .as_deref()
            .and_then(|id| self.messages.as_ref()?.get(id))
            .map(|parent| ReplyTo {
                id: parent.id.clone(),
                content: parent.content.chars().take(200).collect(),
                author_id: Some(parent.author_id.clone()),
                author: parent.author.clone(),
                attachment_count: i64::try_from(parent.attachments.len()).unwrap_or(0),
            });
        let message = Message {
            id: pending_id(&nonce),
            channel_id,
            author_id: me.id.clone(),
            author: Some(me),
            content: content.to_owned(),
            reply_to_id,
            reply_to,
            is_pinned: false,
            created_at: sent_at,
            updated_at: None,
            attachments,
            reactions: Vec::new(),
            embeds: Vec::new(),
            nonce: Some(nonce.clone()),
            content_revision: None,
            kind: None,
            reply_count: 0,
        };
        self.outbox.insert(
            nonce.clone(),
            Outgoing {
                message: message.clone(),
                failed: false,
                attachment_ids: Vec::new(),
                nsfw_ids: Vec::new(),
                gif: None,
            },
        );
        self.upsert_if_open(fx, message);
        true
    }

    fn post(&mut self, fx: &mut Effects, nonce: &str) {
        let Some(out) = self.outbox.get(nonce) else {
            return;
        };
        // A GIF pick registers its attachment first; that answer posts.
        if out.gif.is_some() {
            return self.upload_gif(fx, nonce);
        }
        let request = ApiRequest::SendMessage {
            channel_id: out.message.channel_id.clone(),
            content: out.message.content.clone(),
            reply_to_id: out.message.reply_to_id.clone(),
            nonce: nonce.to_owned(),
            attachment_ids: out.attachment_ids.clone(),
            nsfw_attachment_ids: out.nsfw_ids.clone(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.sends.insert(id, nonce.to_owned());
        }
    }

    fn set_send_failed(&mut self, fx: &mut Effects, nonce: &str, failed: bool) {
        let at: Vec<usize> = self.position_of(&pending_id(nonce)).into_iter().collect();
        self.touch_rows(fx, &at, |state| {
            if let Some(out) = state.outbox.get_mut(nonce) {
                out.failed = failed;
            }
        });
    }

    fn retry_send(&mut self, fx: &mut Effects, nonce: &str) {
        if self.outbox.get(nonce).is_some_and(|o| o.failed) {
            self.set_send_failed(fx, nonce, false);
            self.post(fx, nonce);
        }
    }

    fn on_sent(&mut self, fx: &mut Effects, nonce: &str, result: Result<ApiResponse, ApiError>) {
        let toast_reply = self.pending.toast_replies.remove(nonce);
        match result {
            Ok(ApiResponse::Sent(Some(message))) => self.upsert_if_open(fx, *message),
            // An odd body: the gateway echo still reconciles the row.
            Ok(_) => {}
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Err(_) if self.outbox.contains_key(nonce) => self.set_send_failed(fx, nonce, true),
            // A toast reply to a channel that is not open has no row to
            // mark failed: say so, with the text.
            Err(_) => {
                if let Some(text) = toast_reply {
                    fx.view(ViewUpdate::Notice(format!("{TOAST_REPLY_FAILED} {text}")));
                }
            }
        }
    }

    /// Edits one of our messages in place at once; the answer confirms it
    /// and a failure puts the old text back.
    fn edit_message(&mut self, fx: &mut Effects, id: &str, content: &str) {
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        let me = me.id.clone();
        // A thread reply the open list has not loaded edits too (the same
        // PATCH; only its author may).
        let Some(original) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(id))
            .or_else(|| self.thread_message(id))
            .filter(|m| m.author_id == me && !is_pending(m))
            .cloned()
        else {
            return;
        };
        if content.is_empty() || content == original.content {
            return;
        }
        let patch = MessagePatch {
            id: id.to_owned(),
            channel_id: Some(original.channel_id.clone()),
            content: Some(content.to_owned()),
            // Shows "(edited)" until the server's time arrives.
            updated_at: Some(
                original
                    .updated_at
                    .clone()
                    .unwrap_or_else(|| original.created_at.clone()),
            ),
            embeds: None,
            content_revision: None,
        };
        self.apply_patch(fx, &patch);
        let request = ApiRequest::EditMessage {
            channel_id: original.channel_id.clone(),
            message_id: id.to_owned(),
            content: content.to_owned(),
        };
        if let Some(request) = self.http(fx, request) {
            self.pending.edits.insert(request, original);
        }
    }

    fn on_edited(
        &mut self,
        fx: &mut Effects,
        original: Message,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(ApiResponse::Edited(Some(patch))) => self.apply_patch(fx, &patch),
            Ok(_) => {}
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Err(_) => {
                self.restore_thread_copy(&original);
                if self.position_of(&original.id).is_some() {
                    self.upsert_if_open(fx, original);
                }
                fx.view(ViewUpdate::Notice(EDIT_FAILED.to_owned()));
            }
        }
    }

    /// `removeEmbeds`: clears the previews of one of our messages for
    /// everyone. The row clears at once; the web keeps it cleared even when
    /// the request fails (no `MESSAGE_UPDATE` then), and so does this.
    fn remove_embeds(&mut self, fx: &mut Effects, id: &str) {
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        let me = me.id.clone();
        let Some(original) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(id))
            .filter(|m| m.author_id == me && !is_pending(m) && !m.embeds.is_empty())
            .cloned()
        else {
            return;
        };
        let patch = MessagePatch {
            id: id.to_owned(),
            channel_id: Some(original.channel_id.clone()),
            content: None,
            updated_at: None,
            embeds: Some(Vec::new()),
            // Its own revision, so the revision rule lets the clear through.
            content_revision: original.content_revision,
        };
        self.apply_patch(fx, &patch);
        let _ = self.http(
            fx,
            ApiRequest::RemoveEmbeds {
                channel_id: original.channel_id,
                message_id: id.to_owned(),
            },
        );
    }

    /// Deletes one of our messages: the row goes at once and comes back if
    /// the server refuses.
    fn delete_message(&mut self, fx: &mut Effects, id: &str) {
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        let me = me.id.clone();
        // Our own, or anyone's with MANAGE_MESSAGES (messages.ts DELETE);
        // a thread reply the open list has not loaded too.
        let manage = self.can_manage_messages();
        let Some(message) = self
            .messages
            .as_ref()
            .and_then(|log| log.get(id))
            .or_else(|| self.thread_message(id))
            .filter(|m| (manage || m.author_id == me) && !is_pending(m))
            .cloned()
        else {
            return;
        };
        let listed = self.remove_row(fx, id).is_some();
        if listed {
            self.persist_messages(fx);
        }
        if self.drop_thread_copy(id) && !listed {
            // The list's copy would have moved its parent's count.
            self.bump_reply_count(fx, message.reply_to_id.as_deref(), -1);
        }
        let request = ApiRequest::DeleteMessage {
            channel_id: message.channel_id.clone(),
            message_id: id.to_owned(),
        };
        if let Some(request) = self.http(fx, request) {
            self.pending.deletes.insert(request, (message, listed));
        }
    }

    fn on_deleted(
        &mut self,
        fx: &mut Effects,
        (message, listed): (Message, bool),
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(_) => {}
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Err(_) => {
                if self.put_back_thread_copy(&message) && !listed {
                    self.bump_reply_count(fx, message.reply_to_id.as_deref(), 1);
                }
                if listed {
                    self.insert_row_sorted(fx, message);
                }
                fx.view(ViewUpdate::Notice(DELETE_FAILED.to_owned()));
            }
        }
    }

    fn apply_patch(&mut self, fx: &mut Effects, patch: &MessagePatch) {
        let active = self.active_channel.as_deref();
        if patch.channel_id.is_some() && patch.channel_id.as_deref() != active {
            return;
        }
        self.patch_thread_copy(patch);
        let Some(at) = self.position_of(&patch.id) else {
            return;
        };
        let before = [self.message_row_at(at), self.message_row_at(at + 1)];
        if let Some(log) = &mut self.messages {
            log.patch(patch);
        }
        self.emit_message_changes(fx, at, before);
        self.persist_messages(fx);
    }

    fn remember_author(&mut self, message: &Message) {
        if let Some(author) = &message.author {
            self.names
                .insert(author.id.clone(), author.label().to_owned());
        }
    }

    /// Upserts into the open channel (by id, so the POST answer and the
    /// gateway echo of the same message end up as one row). Our own send
    /// coming back takes its pending row's place (chat-reducer.ts).
    fn upsert_if_open(&mut self, fx: &mut Effects, message: Message) {
        let nonce = message.nonce.clone().filter(|_| !is_pending(&message));
        if let Some(nonce) = nonce.filter(|n| self.outbox.contains_key(n)) {
            let stand_in = pending_id(&nonce);
            let at = self.position_of(&stand_in);
            // Read while the row still counts as pending, so the diff says so.
            let before = at.map(|at| [self.message_row_at(at), self.message_row_at(at + 1)]);
            self.outbox.remove(&nonce);
            if let (Some(at), Some(before)) = (at, before) {
                let replaced = self
                    .messages
                    .as_mut()
                    .and_then(|log| log.replace(&stand_in, message.clone()));
                if replaced.is_some() {
                    self.emit_message_changes(fx, at, before);
                    self.persist_messages(fx);
                    return;
                }
                // The real row is already here: drop the stand-in.
                self.remove_row(fx, &stand_in);
            }
        }
        let open = self.messages.as_ref();
        let Some(log) = open.filter(|log| log.channel_id == message.channel_id) else {
            return;
        };
        let before = log
            .position(&message.id)
            .map(|i| [self.message_row_at(i), self.message_row_at(i + 1)]);
        let parent = message.reply_to_id.clone();
        // A jump's window does not reach the end: a new message belongs to
        // a page it has not loaded yet.
        if before.is_none() && log.has_more_after {
            // The row is not loaded, but a loaded parent's count is.
            self.bump_reply_count(fx, parent.as_deref(), 1);
            return;
        }
        let Some(log) = &mut self.messages else {
            return;
        };
        let at = log.upsert(message);
        self.persist_messages(fx);
        match before {
            Some(before) => self.emit_message_changes(fx, at, before),
            None => {
                if let Some(row) = self.message_row_at(at) {
                    fx.messages(ViewDiff::Insert {
                        at,
                        rows: vec![row],
                    });
                }
                self.bump_reply_count(fx, parent.as_deref(), 1);
            }
        }
    }

    // Gateway

    /// (Re)subscribes the gateway to what is on screen.
    fn sync_subscriptions(&self, fx: &mut Effects) {
        // Every server (chat-gateway.ts): presence, member and profile
        // events fan out to a server's subscribers.
        for server in &self.servers {
            let subscribe = ServerSubscribe {
                server_id: server.id.clone(),
            };
            self.gateway_send(fx, frame(GatewayOp::ServerSubscribe, &subscribe));
        }
        if let Some(channel) = &self.active_channel {
            self.gateway_send(
                fx,
                Self::channel_frame(GatewayOp::ChannelSubscribe, channel),
            );
        }
        // No voice presence here: the room is the roster's source of truth
        // (realtime/VOICE_ROSTER.md), so a fresh gateway session needs no
        // op 33 to keep us in anyone's sidebar.
    }

    fn on_gateway(&mut self, fx: &mut Effects, session: SessionId, event: GatewayEvent) {
        let Some(current) = self.session.as_mut().filter(|s| s.id == session) else {
            return;
        };
        match event {
            GatewayEvent::Ready => {
                current.gateway.ready = true;
                let reconnect = std::mem::replace(&mut current.gateway.was_ready, true);
                self.set_connected(fx, true);
                // A fresh gateway session starts online: say otherwise.
                self.send_status(fx);
                self.sync_subscriptions(fx);
                self.expect_voice_rosters(fx);
                // A fresh session after a drop (Resume failed or was not
                // possible): events during the gap are lost, so refetch what
                // is on screen.
                if reconnect {
                    if let Some(channel) = self.active_channel.clone() {
                        self.load_page(fx, &channel);
                    }
                    self.load_unread(fx);
                }
            }
            GatewayEvent::Resumed => {
                current.gateway.ready = true;
                current.gateway.was_ready = true;
                self.set_connected(fx, true);
                // The server restored the old subscriptions and voice
                // presence; resend the subscriptions anyway, since switches
                // made while offline were not sent (they are idempotent).
                self.sync_subscriptions(fx);
                self.expect_voice_rosters(fx);
            }
            GatewayEvent::Closed => {
                current.gateway.ready = false;
                self.set_connected(fx, false);
            }
            GatewayEvent::SessionExpired => {
                self.forget_user(fx);
                self.end_session(fx, false);
                fx.push(Effect::ClearToken);
                self.set_status(fx, SESSION_ENDED);
            }
            GatewayEvent::Dispatch(dispatch) => self.on_dispatch(fx, dispatch),
        }
    }

    fn on_dispatch(&mut self, fx: &mut Effects, dispatch: Dispatch) {
        let active = self.active_channel.clone().unwrap_or_default();
        self.panels_dispatch(&dispatch);
        match dispatch {
            Dispatch::MessageCreate(message) => self.on_message_create(fx, *message),
            Dispatch::MessageUpdate(message) => {
                self.remember_author(&message);
                self.upsert_if_open(fx, *message);
            }
            Dispatch::MessagePatch(patch) => {
                if patch.channel_id.as_deref() == Some(active.as_str()) {
                    self.apply_patch(fx, &patch);
                }
            }
            Dispatch::ReadStateUpdate(read) => {
                self.read_states.insert(read.channel_id, read.last_read_at);
                self.sync_badges(fx);
            }
            Dispatch::NotificationCreate(notification) => {
                self.inbox_created(&notification);
                self.sync_extras(fx);
                let known = self.notifications.iter().any(|n| n.id == notification.id);
                if known {
                    return;
                }
                fx.view(ViewUpdate::Sound(Sound::Notification));
                let message_id = notification.message_id.as_deref().unwrap_or_default();
                let channel_id = notification.channel_id.as_deref().unwrap_or_default();
                if !self.toasted.contains(message_id)
                    && shell::should_notify(
                        channel_id,
                        self.active_channel.as_deref(),
                        self.focused,
                    )
                {
                    let toast = shell::notification_toast(&notification);
                    self.toasted.note(&toast.message_id);
                    fx.view(ViewUpdate::Toast(Box::new(toast)));
                }
                if !notification.is_read {
                    self.notifications.push(*notification);
                    if self.focused && self.at_bottom {
                        self.read_seen_mentions(fx, false);
                    }
                    self.sync_badges(fx);
                }
            }
            Dispatch::NotificationsRead(read) => {
                self.inbox_read(read.ids.as_deref().filter(|_| !read.all));
                match read.ids {
                    Some(ids) if !read.all => self.notifications.retain(|n| !ids.contains(&n.id)),
                    _ => self.notifications.clear(),
                }
                self.sync_badges(fx);
            }
            Dispatch::NotificationsClear => {
                self.inbox_cleared();
                self.notifications.clear();
                self.sync_badges(fx);
            }
            Dispatch::ReactionAdd(event) => self.on_reaction_event(fx, &event, true),
            Dispatch::ReactionRemove(event) => self.on_reaction_event(fx, &event, false),
            Dispatch::MessageDelete(delete) => {
                let here = delete.channel_id.as_deref().is_none_or(|c| c == active);
                let Some(at) = self
                    .messages
                    .as_ref()
                    .filter(|_| here)
                    .and_then(|log| log.position(&delete.id))
                else {
                    return;
                };
                // The row below moves up into `at` and may regroup.
                let below = self.message_row_at(at + 1);
                let removed = self
                    .messages
                    .as_mut()
                    .and_then(|log| log.remove(&delete.id));
                fx.messages(ViewDiff::Remove { at, count: 1 });
                self.emit_message_changes(fx, at, [below, None]);
                let parent = removed.and_then(|(_, message)| message.reply_to_id);
                self.bump_reply_count(fx, parent.as_deref(), -1);
                self.persist_messages(fx);
            }
            Dispatch::TypingStart(typing) => {
                let me = self.me().map(|me| me.id.as_str()).unwrap_or_default();
                if typing.channel_id == active && typing.user_id != me {
                    self.typing_generation += 1;
                    let generation = self.typing_generation;
                    let user_id = typing.user_id;
                    match self.typing.iter_mut().find(|t| t.user_id == user_id) {
                        Some(typer) => typer.generation = generation,
                        None => {
                            let name = self.user_name(&user_id);
                            self.typing.push(Typer {
                                user_id: user_id.clone(),
                                name,
                                generation,
                            });
                        }
                    }
                    self.emit_typing(fx);
                    fx.push(Effect::Later {
                        delay: TYPING_TIMEOUT,
                        input: Box::new(Input::TypingExpired {
                            user_id,
                            generation,
                        }),
                    });
                }
            }
            Dispatch::GuildUpdate(server) => self.on_guild_update(fx, *server),
            Dispatch::GuildDelete(delete) => self.on_guild_delete(fx, &delete.id),
            Dispatch::ChannelUpdate(update) => self.on_channel_update(fx, *update),
            Dispatch::ChannelDelete(delete) => self.on_channel_delete(fx, delete),
            Dispatch::ServersChanged => {
                if self.signed_in().is_some() {
                    self.pending.servers = self.http(fx, ApiRequest::Servers);
                }
            }
            Dispatch::VoiceChannelStates(states) => {
                // The whole picture: no check pending is needed.
                self.voice_roster_check += 1;
                self.voice_confirmed.clear();
                let mut started = std::collections::HashMap::new();
                for (id, members) in &states.voice_states {
                    let at = earliest_start(
                        states.voice_started_at.get(id),
                        self.voice_started_at.get(id).copied(),
                        members,
                    );
                    if let Some(at) = at {
                        started.insert(id.clone(), at);
                    }
                }
                self.voice_started_at = started;
                let next = states
                    .voice_states
                    .into_iter()
                    .filter(|(_, members)| !members.is_empty())
                    .map(|(id, members)| {
                        let occupants = self.occupants_from(&id, members);
                        (id, occupants)
                    })
                    .collect();
                self.voice_channels = next;
                self.sync_voice_channels(fx);
                self.sync_standby(fx);
                self.sync_calls(fx);
            }
            Dispatch::VoiceChannelState(update) => {
                let me = self.me().map(|me| me.id.clone()).unwrap_or_default();
                let sound = {
                    let prev = self
                        .voice_channels
                        .get(&update.channel_id)
                        .into_iter()
                        .flatten()
                        .map(|o| o.user_id.as_str());
                    let next = update.members.iter().map(|m| m.user_id.as_str());
                    shell::voice_presence_sound(prev, next, &me)
                };
                if let Some(sound) = sound {
                    fx.view(ViewUpdate::Sound(sound));
                }
                if update.members.is_empty() {
                    self.voice_started_at.remove(&update.channel_id);
                } else if let Some(at) = earliest_start(
                    update.started_at.as_ref(),
                    self.voice_started_at.get(&update.channel_id).copied(),
                    &update.members,
                ) {
                    self.voice_started_at.insert(update.channel_id.clone(), at);
                }
                self.voice_confirmed.insert(update.channel_id.clone());
                if update.members.is_empty() {
                    self.voice_channels.remove(&update.channel_id);
                } else {
                    let occupants = self.occupants_from(&update.channel_id, update.members);
                    self.voice_channels.insert(update.channel_id, occupants);
                }
                self.sync_voice_channels(fx);
                self.sync_standby(fx);
                self.sync_calls(fx);
            }
            Dispatch::PresenceUpdate(update) => {
                if update.status == "offline" {
                    self.on_call_peer_offline(fx, &update.user_id);
                }
                self.on_presence(fx, update);
            }
            Dispatch::PresenceList(list) => self.on_presence_list(fx, list),
            Dispatch::MemberAdd(add) => self.on_member_add(fx, *add),
            Dispatch::MemberRemove(remove) => self.on_member_remove(fx, remove),
            Dispatch::MemberUpdate(update) => self.on_member_update(fx, update),
            Dispatch::DmCreate(dm) => self.add_dm(fx, *dm),
            Dispatch::MessagePin(message) => {
                let id = message.id.clone();
                self.on_pin_dispatch(fx, &id, true, Some(*message));
            }
            Dispatch::MessageUnpin(unpin) => self.on_pin_dispatch(fx, &unpin.id, false, None),
            Dispatch::ProfileUpdate(update) => {
                self.on_friend_profile(&update);
                self.on_profile_update(fx, &update);
            }
            Dispatch::RelationshipAdd(relationship) => self.on_relationship_add(*relationship),
            Dispatch::RelationshipRemove(remove) => self.on_relationship_remove(&remove.user_id),
            Dispatch::CallRing(ring) => self.on_call_ring(fx, ring),
            Dispatch::CallRinging(ringing) => self.on_call_ringing(fx, ringing),
            Dispatch::CallRingStop(stop) => self.on_call_ring_stop(fx, stop),
        }
    }

    /// `MESSAGE_CREATE` (chat-gateway.ts): the author stops typing, the
    /// channel's newest time moves, the open channel is read up to it. A
    /// message from someone else while the window is away moves the
    /// divider to it (or extends the batch it already marks).
    fn on_message_create(&mut self, fx: &mut Effects, message: Message) {
        self.remember_author(&message);
        let channel = message.channel_id.clone();
        if self.find_channel(&channel).is_none()
            && self.dm(&channel).is_none()
            && self.pending.dms.is_none()
        {
            self.pending.dms = self.http(fx, ApiRequest::Dms);
        }
        let at = message.created_at.clone();
        let mine = self.me().is_some_and(|me| me.id == message.author_id);
        // Typers are the open channel's: a post elsewhere leaves them.
        let here = self.active_channel.as_deref() == Some(channel.as_str());
        if here && self.typing.iter().any(|t| t.user_id == message.author_id) {
            self.typing.retain(|t| t.user_id != message.author_id);
            self.emit_typing(fx);
        }
        self.bump_last_message(&channel, &at);
        let open = self
            .messages
            .as_ref()
            .is_some_and(|log| log.channel_id == channel && log.get(&message.id).is_none());
        // useChatArea.ts: a knock for someone else's message in the open
        // channel, focused or not.
        if open && !mine {
            fx.view(ViewUpdate::Sound(Sound::Message));
        }
        // chat-gateway.ts: a server message that does not mention you
        // toasts unless you are looking at its channel; mentions toast
        // through their notification.
        let targets_me = self
            .me()
            .is_some_and(|me| shell::targets_user(&message, me));
        if self.dm(&channel).is_none()
            && !mine
            && !targets_me
            && shell::should_notify(&channel, self.active_channel.as_deref(), self.focused)
        {
            let (name, server) = self
                .channel_and_server(&channel)
                .map_or((None, None), |(name, server)| (Some(name), server));
            let mut toast = shell::message_toast(&message, name, server);
            toast.server_id = self.server_of(&channel).unwrap_or_default();
            self.toasted.note(&toast.message_id);
            fx.view(ViewUpdate::Toast(Box::new(toast)));
        }
        if open && !mine && !self.focused {
            match self.divider.as_mut().filter(|d| !d.locked) {
                Some(divider) => {
                    divider.count += 1;
                    divider.banner = true;
                    self.emit_unread(fx);
                }
                None => {
                    let divider = Divider {
                        message_id: message.id.clone(),
                        count: 1,
                        banner: true,
                        locked: false,
                    };
                    self.set_divider(fx, Some(divider));
                }
            }
        }
        self.upsert_if_open(fx, message);
        // A jump's window has not reached the new message: unseen, not read.
        if self.active_channel.as_deref() == Some(channel.as_str()) && !self.detached() {
            self.mark_read(fx, &channel, Some(&at));
        }
        self.sync_badges(fx);
    }

    // Read states, notifications, members

    fn on_read_states(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let states = match result {
            Ok(ApiResponse::ReadStates(states)) => states,
            Ok(_) => return,
            Err(error) => return self.fail(fx, &error),
        };
        // Marks made while it was in flight are newer: keep the later one.
        for read in states.read_states {
            let newer = self
                .read_states
                .get(&read.channel_id)
                .is_none_or(|mine| is_after(&read.last_read_at, mine));
            if newer {
                self.read_states.insert(read.channel_id, read.last_read_at);
            }
        }
        for last in states.last_messages {
            self.bump_last_message(&last.channel_id, &last.last_message_at);
        }
        self.place_divider(fx);
        self.sync_badges(fx);
        self.on_seen(fx, false);
    }

    fn on_notifications(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let list = match result {
            Ok(ApiResponse::Notifications(list)) => list,
            Ok(_) => return,
            Err(error) => return self.fail(fx, &error),
        };
        // Live ones that arrived meanwhile stay.
        let live = std::mem::take(&mut self.notifications);
        self.notifications = list.into_iter().filter(|n| !n.is_read).collect();
        for n in live {
            if !self.notifications.iter().any(|known| known.id == n.id) {
                self.notifications.push(n);
            }
        }
        if self.focused
            && self.at_bottom
            && self.pending.page.is_none()
            && self.pending.read_states.is_none()
        {
            self.read_seen_mentions(fx, false);
        }
        self.sync_badges(fx);
    }

    fn on_members(
        &mut self,
        fx: &mut Effects,
        server_id: String,
        result: Result<ApiResponse, ApiError>,
    ) {
        let members = match result {
            Ok(ApiResponse::Members(members)) => members,
            Ok(_) => return,
            Err(error) => return self.fail(fx, &error),
        };
        for member in &members {
            let user = &member.user;
            self.names
                .entry(user.id.clone())
                .or_insert_with(|| user.label().to_owned());
        }
        // Author names take their role colour once the members are known.
        let before = self.message_rows();
        self.members.insert(server_id, members);
        let rows = self.message_rows();
        for diff in view::ordered_diff(&before, &rows) {
            fx.messages(diff);
        }
        self.sync_members(fx);
        self.emit_profile(fx);
    }

    /// Another server's tree: kept for its unread pill (and shown if it is
    /// the tree on screen).
    fn on_tree(
        &mut self,
        fx: &mut Effects,
        server_id: String,
        result: Result<ApiResponse, ApiError>,
    ) {
        let response = match result {
            Ok(ApiResponse::Channels(response)) => response,
            Ok(_) => return,
            Err(error) if error.is_unauthorized() => return self.fail(fx, &error),
            Err(_) => return,
        };
        let list = ServerChannels::new(response);
        let tree = list.to_response();
        self.persist(
            fx,
            CacheWrite::Channels {
                server_id: server_id.clone(),
                tree,
            },
        );
        self.channels.insert(server_id, list);
        self.sync_badges(fx);
    }

    fn on_marked(
        &mut self,
        fx: &mut Effects,
        (channel, previous, mark): (String, Option<String>, String),
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(_) => {}
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            // Roll back unless a newer mark replaced ours.
            Err(_) => {
                if self.read_states.get(&channel) == Some(&mark) {
                    match previous {
                        Some(previous) => self.read_states.insert(channel, previous),
                        None => self.read_states.remove(&channel),
                    };
                    self.sync_badges(fx);
                }
            }
        }
    }

    // Voice

    fn join_voice(&mut self, fx: &mut Effects, channel_id: String) {
        if self
            .voice
            .as_ref()
            .is_some_and(|c| c.channel_id == channel_id)
        {
            return;
        }
        let Some(server_id) = self.active_server.clone() else {
            return;
        };
        self.start_voice(fx, server_id, channel_id, None, None);
    }

    /// Joins a voice channel (or, with `room_slug`, a DM call's room),
    /// leaving any other call first; a standby held for it is promoted.
    fn start_voice(
        &mut self,
        fx: &mut Effects,
        server_id: String,
        channel_id: String,
        room_slug: Option<String>,
        move_grant: Option<String>,
    ) {
        let promoted = self.standby.take_if(|s| {
            s.channel_id == channel_id
                && s.server_id == server_id
                && room_slug.is_none()
                && move_grant.is_none()
        });
        self.leave_call(fx);
        if let Some(stale) = self.standby.take() {
            fx.push(Effect::Voice {
                call: stale.id,
                command: VoiceCommand::Leave,
            });
        }
        let Some((session, me)) = self.signed_in() else {
            return;
        };
        let dm = room_slug.is_some();
        let target = VoiceTarget {
            server_id: server_id.clone(),
            me_id: me.id.clone(),
            channel_id: channel_id.clone(),
            me_name: me.label().to_owned(),
            me_username: me.username.clone(),
            room_slug,
            move_grant: move_grant.clone(),
            standby: false,
        };
        let token = session.token.clone();
        self.voice_error.clear();
        let call = match promoted {
            Some(standby) => {
                fx.push(Effect::Voice {
                    call: standby.id,
                    command: VoiceCommand::Promote,
                });
                standby.id
            }
            None => {
                let call = CallId(self.next_id());
                fx.push(Effect::VoiceStart {
                    call,
                    token,
                    target,
                });
                call
            }
        };
        // The kept mic and deafen apply from the start.
        let prefs = self.voice_prefs;
        if prefs.muted {
            fx.push(Effect::Voice {
                call,
                command: VoiceCommand::SetMuted(true),
            });
        }
        if prefs.deafened {
            fx.push(Effect::Voice {
                call,
                command: VoiceCommand::SetDeafened(true),
            });
        }
        self.voice = Some(VoiceCall {
            id: call,
            server_id,
            channel_id,
            status: "Connecting".into(),
            roster: Vec::new(),
            muted: prefs.muted,
            deafened: prefs.deafened,
            joined: false,
            watched: Default::default(),
            camera_wanted: false,
            camera: false,
            camera_error: String::new(),
            share_wanted: None,
            share: None,
            share_error: String::new(),
            dm,
            move_grant,
        });
        self.emit_voice(fx);
    }

    /// A user leave (STATE_MACHINE.md "Teardown"): a joined call plays the
    /// leave sound and sends the presence leave (op 34); one the room never
    /// answered just goes.
    fn leave_voice(&mut self, fx: &mut Effects) {
        self.leave_call(fx);
        self.sync_standby(fx);
        self.sync_calls(fx);
    }

    /// [`Self::leave_voice`] without re-deciding the standby (a join is
    /// about to start).
    fn leave_call(&mut self, fx: &mut Effects) {
        // The leave sound goes before the leave (auto-soundboard.ts).
        if let (Some(frame), Some(call)) = (self.apps_leave_frame(), self.voice.as_ref()) {
            fx.push(Effect::Voice {
                call: call.id,
                command: VoiceCommand::SendApp(frame),
            });
        }
        if let Some(call) = self.voice.take() {
            self.apps_call_ended(fx);
            self.calls_call_ended();
            // A DM call plays its own end tone (DMCallRegion `handleLeave`).
            if call.joined && !call.dm {
                fx.view(ViewUpdate::Sound(Sound::Disconnect));
            }
            fx.push(Effect::Voice {
                call: call.id,
                command: VoiceCommand::Leave,
            });
            if call.joined {
                self.gateway_send(fx, Self::voice_leave_frame(&call.channel_id));
            }
        }
        self.emit_voice(fx);
    }

    /// Watches a share (or stops): the call pulls it and says so (op 106).
    fn watch_stream(&mut self, fx: &mut Effects, participant_id: String, watch: bool) {
        let Some(call) = self.voice.as_mut() else {
            return;
        };
        let changed = if watch {
            call.watched.insert(participant_id)
        } else {
            call.watched.remove(&participant_id)
        };
        if changed {
            fx.push(Effect::Voice {
                call: call.id,
                command: VoiceCommand::SetWatched(call.watched.iter().cloned().collect()),
            });
            self.sync_roster(fx);
        }
    }

    /// A share command in the call (SCREEN_SHARE.md: commands only change
    /// what is wanted; the call takes the steps). `change` updates the
    /// call's wanted share and names the command, if any.
    fn voice_share(
        &mut self,
        fx: &mut Effects,
        change: impl FnOnce(&mut VoiceCall) -> Option<VoiceCommand>,
    ) {
        let Some(call) = self.voice.as_mut() else {
            return;
        };
        let Some(command) = change(call) else {
            return;
        };
        fx.push(Effect::Voice {
            call: call.id,
            command,
        });
        self.emit_voice(fx);
    }

    /// The camera toggle: the call pushes it (or resumes its kept track),
    /// and reports back when it is on. A capture failure turns it off with
    /// the reason; a new toggle clears it.
    fn set_voice_camera(&mut self, fx: &mut Effects, on: bool, error: Option<String>) {
        let Some(call) = self.voice.as_mut() else {
            return;
        };
        let reason = error.unwrap_or_default();
        if call.camera_wanted == on && call.camera_error == reason {
            return;
        }
        call.camera_wanted = on;
        call.camera_error = reason;
        fx.push(Effect::Voice {
            call: call.id,
            command: VoiceCommand::SetCamera(on),
        });
        self.emit_voice(fx);
    }

    /// New mic and deafen: kept, stored, sent to the call and, once it is
    /// up, to the gateway.
    fn set_voice_prefs(&mut self, fx: &mut Effects, prefs: VoicePrefs) {
        if prefs == self.voice_prefs {
            return;
        }
        self.voice_prefs = prefs;
        self.persist(fx, CacheWrite::Voice(prefs));
        if let Some(call) = &mut self.voice {
            if call.muted != prefs.muted {
                call.muted = prefs.muted;
                fx.push(Effect::Voice {
                    call: call.id,
                    command: VoiceCommand::SetMuted(prefs.muted),
                });
            }
            if call.deafened != prefs.deafened {
                call.deafened = prefs.deafened;
                fx.push(Effect::Voice {
                    call: call.id,
                    command: VoiceCommand::SetDeafened(prefs.deafened),
                });
            }
        }
        self.emit_voice(fx);
    }

    fn on_voice(&mut self, fx: &mut Effects, call: CallId, update: VoiceUpdate) {
        if self.standby.as_ref().is_some_and(|s| s.id == call) {
            // Hidden until promoted: only its end matters.
            if matches!(update, VoiceUpdate::Ended(_)) {
                self.standby = None;
                fx.push(Effect::VoiceForget(call));
            }
            return;
        }
        let Some(current) = self.voice.as_mut().filter(|c| c.id == call) else {
            return;
        };
        match update {
            VoiceUpdate::Status(status) => current.status = status,
            VoiceUpdate::Roster(roster) => current.roster = roster,
            VoiceUpdate::Camera { on, error } => {
                current.camera = on;
                if let Some(error) = error {
                    current.camera_wanted = false;
                    current.camera_error = error;
                }
            }
            VoiceUpdate::ShareStarted(share) => {
                fx.view(ViewUpdate::Sound(Sound::ScreenShareStart));
                current.share = Some(share);
            }
            VoiceUpdate::ShareUpdated(share) => current.share = Some(share),
            VoiceUpdate::ShareEnded { stop_sound, error } => {
                if stop_sound {
                    fx.view(ViewUpdate::Sound(Sound::ScreenShareStop));
                }
                current.share = None;
                current.share_wanted = None;
                current.share_error = error.unwrap_or_default();
            }
            VoiceUpdate::Connected => {
                // `joined`: the connect sound and one presence join (op 33),
                // as the web sends once the room join completes. Mute and
                // deafen go on the room socket only (VOICE_ROSTER.md).
                // A DM call plays the call connect tone from its ring stop.
                if !std::mem::replace(&mut current.joined, true) && !current.dm {
                    fx.view(ViewUpdate::Sound(Sound::Connect));
                }
                let dm = current.dm;
                let join = Self::voice_join_frame(current);
                // The grant did its job (both tickets and this join).
                current.move_grant = None;
                self.gateway_send(fx, join);
                if dm {
                    self.dm_call_joined(fx);
                }
            }
            VoiceUpdate::Rejoined => {
                let join = Self::voice_join_frame(current);
                self.gateway_send(fx, join);
            }
            VoiceUpdate::VoiceReady { participant_id } => {
                return self.on_apps_voice_ready(fx, participant_id);
            }
            VoiceUpdate::App { event, now_ms } => {
                return self.on_app_event(fx, *event, now_ms);
            }
            VoiceUpdate::Moved(moved) => return self.on_moved(fx, moved),
            VoiceUpdate::Stats(stats) => return self.on_call_stats(fx, *stats),
            VoiceUpdate::InboundStalled(stalled) => {
                return self.on_call_stalls(fx, Some(stalled), None);
            }
            VoiceUpdate::MicStalled(stalled) => {
                return self.on_call_stalls(fx, None, Some(stalled))
            }
            VoiceUpdate::Reset => {
                // The room lost our session (its object reset) and made a
                // new participant the old media cannot serve. A joined
                // channel call carries on as a fresh join of the same
                // channel: no leave sound, no presence leave, and the room
                // hands the seat over (one session per user). A DM call or
                // one never joined just ends.
                if !current.joined || current.dm {
                    let reason = super::CALL_RESET.to_owned();
                    return self.on_voice(fx, call, VoiceUpdate::Ended(Some(reason)));
                }
                let Some(ended) = self.voice.take() else {
                    return;
                };
                self.apps_call_ended(fx);
                self.calls_call_ended();
                fx.push(Effect::VoiceForget(ended.id));
                return self.start_voice(fx, ended.server_id, ended.channel_id, None, None);
            }
            VoiceUpdate::Ended(error) => {
                // A forced end: the leave sound and presence leave only if
                // the room had said Ready.
                if let Some(ended) = self.voice.take() {
                    self.apps_call_ended(fx);
                    self.calls_call_ended();
                    if ended.joined {
                        if !ended.dm {
                            fx.view(ViewUpdate::Sound(Sound::Disconnect));
                        }
                        self.gateway_send(fx, Self::voice_leave_frame(&ended.channel_id));
                    }
                    fx.push(Effect::VoiceForget(ended.id));
                    if ended.dm {
                        self.dm_call_ended(fx);
                    }
                    self.sync_standby(fx);
                }
                if let Some(error) = error {
                    self.voice_error = join_error_copy(&error);
                    self.set_status(fx, format!("Left the call: {error}"));
                }
            }
        }
        self.emit_voice(fx);
    }
}

/// What the voice view says when a call ends with an error. The raw text
/// stays in the session status (and the log); users get one plain line.
fn join_error_copy(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&["somewhere else", "moved to another", "was reset"]) {
        // Already written for people (yap-voice close reasons).
        return error.to_owned();
    }
    if has(&["403", "forbidden", "connect_denied"]) {
        return "You can't join this channel.".to_owned();
    }
    if has(&[
        "error sending request",
        "connect",
        "timed out",
        "timeout",
        "dns",
        "network",
    ]) {
        return "Couldn't reach Yap. Check your connection and try again.".to_owned();
    }
    "Couldn't join the call. Try again.".to_owned()
}

fn unexpected(response: &ApiResponse) -> ApiError {
    ApiError::Decode(format!("unexpected response: {response:?}"))
}

/// The call's start for the sidebar clock (chat-reducer.ts
/// SET_VOICE_CHANNEL_STATES): the earliest of what the server sent, what
/// we had and the first member's join, ignoring anything not positive.
fn earliest_start(
    sent: Option<&serde_json::Value>,
    had: Option<i64>,
    members: &[yap_protocol::payload::VoiceChannelMember],
) -> Option<i64> {
    let ms = |v: &serde_json::Value| {
        v.as_f64()
            .filter(|n| n.is_finite() && *n > 0.0)
            .map(|n| n as i64)
    };
    let joined = members
        .iter()
        .filter_map(|m| m.joined_at.as_ref().and_then(ms))
        .min();
    [sent.and_then(ms), had.filter(|n| *n > 0), joined]
        .into_iter()
        .flatten()
        .min()
}
