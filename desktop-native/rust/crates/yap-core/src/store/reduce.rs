//! The reducer: one arm per input, emitting effects in the order the old
//! sequential core performed them, so the UI sees the same updates.

use std::time::Duration;

use serde::Serialize;
use yap_protocol::payload::{ChannelRef, ServerSubscribe, VoiceChannelJoin, VoiceChannelLeave};
use yap_protocol::rest::{ChannelType, Message, Server, User};
use yap_protocol::{Frame, GatewayOp};

use super::state::{
    Boot, MessageLog, Pending, Phase, ServerChannels, Session, Startup, State, VoiceCall,
};
use super::view::{self, ChannelTreeRow, MessageRow, Row, RowContext, ViewDiff, ViewUpdate};
use super::view::{MeView, Seat, VoicePanelView, VoiceView};
use super::{
    ApiRequest, ApiResponse, CacheWrite, CallId, Command, Dispatch, Effect, GatewayEvent, Input,
    Occupant, RequestId, SessionId, SignInEvent, SignInId, Snapshot, VoiceCommand, VoiceTarget,
    VoiceUpdate,
};
use crate::api::{ApiError, Token};
use yap_protocol::payload::VoiceChannelMember;

const SESSION_ENDED: &str = "Your session ended. Sign in again.";

/// The status line while a view shown from the cache is revalidated.
pub const REFRESHING: &str = "Refreshing…";

/// How long the typing line stays after the last typing event.
pub const TYPING_TIMEOUT: Duration = Duration::from_secs(8);

/// Applies one input and returns the effects to run, in order.
pub fn reduce(state: &mut State, input: Input) -> Vec<Effect> {
    let mut fx = Effects(Vec::new());
    match input {
        Input::User(command) => state.on_command(&mut fx, command),
        Input::TokenLoaded(token) => state.on_token(&mut fx, token),
        Input::SnapshotLoaded(snapshot) => state.on_snapshot(&mut fx, snapshot),
        Input::Http { id, result } => state.on_http(&mut fx, id, result),
        Input::SignIn { attempt, event } => state.on_sign_in(&mut fx, attempt, event),
        Input::Gateway { session, event } => state.on_gateway(&mut fx, session, event),
        Input::Voice { call, update } => state.on_voice(&mut fx, call, update),
        Input::TypingExpired(generation) => {
            if generation == state.typing_generation {
                state.set_typing(&mut fx, Vec::new());
            }
        }
    }
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
        self.view(ViewUpdate::Messages(diff));
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
            move_grant: None,
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
        RowContext {
            api_base: &self.api_base,
            me_id: self.me().map_or("", |me| me.id.as_str()),
            offset: self.local_offset,
        }
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
        let rows = view::server_rows(&self.servers, &self.api_base);
        for diff in view::ordered_diff(&self.view.servers, &rows) {
            fx.view(ViewUpdate::Servers(diff));
        }
        self.view.servers = rows;
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
        view::tree_rows(list, |id| self.seats(id))
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
                    .unwrap_or_default(),
                live: call.status == "Connected",
                status: call.status.clone(),
                muted: call.muted,
                deafened: call.deafened,
            },
            None => VoiceView {
                muted: prefs.muted,
                deafened: prefs.deafened,
                ..VoiceView::default()
            },
        };
        fx.view(ViewUpdate::Voice(voice));
        if self.voice_view.is_some() {
            self.emit_voice_panel(fx);
        }
        self.sync_roster(fx);
        self.sync_tree(fx);
    }

    /// Opens a voice channel's view (its landing; the call once joined).
    fn open_voice_panel(&mut self, fx: &mut Effects, id: &str) {
        self.voice_view = Some(id.to_owned());
        self.emit_voice_panel(fx);
        self.sync_roster(fx);
    }

    fn close_voice_panel(&mut self, fx: &mut Effects) {
        if self.voice_view.take().is_some() {
            fx.view(ViewUpdate::VoicePanel(VoicePanelView::default()));
            self.sync_roster(fx);
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
    }

    fn active_server_name(&self) -> String {
        let id = self.active_server.as_deref();
        self.servers
            .iter()
            .find(|s| Some(s.id.as_str()) == id)
            .map(|s| s.name.clone())
            .unwrap_or_default()
    }

    fn set_typing(&mut self, fx: &mut Effects, names: Vec<String>) {
        if self.typing != names {
            fx.view(ViewUpdate::Typing(view::typing_text(&names)));
            self.typing = names;
        }
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
        self.persist(
            fx,
            CacheWrite::Route {
                server_id: self.active_server.clone(),
                channel_id: self.active_channel.clone(),
            },
        );
    }

    /// The open channel's newest window.
    fn persist_messages(&self, fx: &mut Effects) {
        let Some(log) = &self.messages else {
            return;
        };
        let skip = log.len().saturating_sub(yap_cache::MESSAGE_WINDOW);
        self.persist(
            fx,
            CacheWrite::Messages {
                channel_id: log.channel_id.clone(),
                messages: log.iter().skip(skip).cloned().collect(),
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
            Command::SelectChannel(id) => self.select_channel(fx, &id),
            Command::Send(text) => self.send(fx, text.trim()),
            Command::JoinVoice(id) => self.join_voice(fx, id),
            Command::LeaveVoice => self.leave_voice(fx),
            Command::SetVoiceMuted(muted) => {
                self.voice_prefs.muted = muted;
                if let Some(call) = &mut self.voice {
                    call.muted = muted;
                    fx.push(Effect::Voice {
                        call: call.id,
                        command: VoiceCommand::SetMuted(muted),
                    });
                }
                self.emit_voice(fx);
            }
            Command::SetVoiceDeafened(deafened) => {
                self.voice_prefs.deafened = deafened;
                if let Some(call) = &mut self.voice {
                    call.deafened = deafened;
                    fx.push(Effect::Voice {
                        call: call.id,
                        command: VoiceCommand::SetDeafened(deafened),
                    });
                }
                self.emit_voice(fx);
            }
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
            ..
        } = snapshot;
        let session = SessionId(self.next_id());
        fx.push(Effect::GatewayConnect {
            session,
            token: token.clone(),
            user_id: me.id.clone(),
        });
        self.names.insert(me.id.clone(), me.label().to_owned());
        fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
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
            fx.view(ViewUpdate::ActiveChannel {
                id: id.clone(),
                name: name.clone(),
            });
            let mut log = MessageLog::new(id);
            for message in messages.into_iter().filter(|m| &m.channel_id == id) {
                self.remember_author(&message);
                log.upsert(message);
            }
            let shown = !log.is_empty();
            self.messages = Some(log);
            fx.messages(ViewDiff::Reset(self.message_rows()));
            if shown {
                fx.push(Effect::StartupMark("cached content"));
            }
        }
        self.set_phase(fx, Phase::Ready);
        self.set_status(fx, REFRESHING);

        self.pending.me = self.http(fx, ApiRequest::Me);
        self.pending.servers = self.http(fx, ApiRequest::Servers);
        if let Some(server_id) = server {
            let request = ApiRequest::Channels {
                server_id: server_id.clone(),
            };
            self.pending.channels = self.http(fx, request).map(|req| (req, server_id));
        }
        if let Some((channel_id, _)) = channel {
            self.load_page(fx, &channel_id);
        }
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
        self.leave_voice(fx);
        self.close_voice_panel(fx);
        self.voice_channels.clear();
        if let Some(session) = self.session.take() {
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
            fx.view(ViewUpdate::ActiveChannel {
                id: String::new(),
                name: String::new(),
            });
        }
        if self.messages.take().is_some() {
            fx.messages(ViewDiff::Reset(Vec::new()));
        }
        self.names.clear();
        self.set_typing(fx, Vec::new());
        self.tree_server = None;
        self.sync_servers(fx);
        self.sync_tree(fx);
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
        // Anything else is stale: a newer request replaced it, or the session ended.
        if let Some(channel_id) = pending.sends.remove(&id) {
            self.on_sent(fx, &channel_id, result);
        }
    }

    fn on_me(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let me = match result {
            Ok(ApiResponse::Me(me)) => *me,
            Ok(other) => return self.me_failed(fx, &unexpected(&other)),
            Err(error) => return self.me_failed(fx, &error),
        };
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
            return self.persist(fx, CacheWrite::Me(Box::new(me)));
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
            Some(id) if self.refreshing => {
                let name = self.active_server_name();
                fx.view(ViewUpdate::ActiveServer { id, name });
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
        fx.view(ViewUpdate::ActiveChannel {
            id: String::new(),
            name: String::new(),
        });
        self.set_typing(fx, Vec::new());
        fx.messages(ViewDiff::Reset(Vec::new()));
    }

    fn select_server(&mut self, fx: &mut Effects, id: String) {
        if self.signed_in().is_none() {
            return;
        }
        if self.active_server.as_deref() != Some(id.as_str()) {
            self.close_voice_panel(fx);
        }
        self.active_server = Some(id.clone());
        fx.view(ViewUpdate::ActiveServer {
            id: id.clone(),
            name: self.active_server_name(),
        });
        self.persist_route(fx);
        let request = ApiRequest::Channels {
            server_id: id.clone(),
        };
        self.pending.channels = self.http(fx, request).map(|req| (req, id));
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
        self.sync_subscriptions(fx, true);
        if let Some((id, name)) = kept {
            // Only the name can have changed; the messages stay.
            fx.view(ViewUpdate::ActiveChannel { id, name });
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
        let Some(channel) = self.find_channel(id) else {
            return;
        };
        match channel.channel_type {
            ChannelType::Voice => return self.open_voice_panel(fx, id),
            ChannelType::Text => {}
            ChannelType::Dm | ChannelType::Other => return,
        }
        let name = channel.name.clone();
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
        fx.view(ViewUpdate::ActiveChannel {
            id: id.to_owned(),
            name,
        });
        self.set_typing(fx, Vec::new());
        fx.messages(ViewDiff::Reset(Vec::new()));
        self.persist_route(fx);
        self.gateway_send(fx, Self::channel_frame(GatewayOp::ChannelSubscribe, id));
        self.load_page(fx, id);
    }

    /// Fetches the newest page of a channel; the result replaces the list.
    fn load_page(&mut self, fx: &mut Effects, channel_id: &str) {
        let request = ApiRequest::Messages {
            channel_id: channel_id.to_owned(),
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
        let messages = match result {
            Ok(ApiResponse::Messages(messages)) => messages,
            Ok(other) => return self.chain_failed(fx, &unexpected(&other)),
            Err(error) => return self.chain_failed(fx, &error),
        };
        if self.active_channel.as_deref() != Some(channel_id) {
            return;
        }
        // A refetch of what is on screen (the cached window, or a refetch
        // after a reconnect) changes only the rows that differ.
        let shown = self
            .messages
            .as_ref()
            .is_some_and(|log| log.channel_id == channel_id && !log.is_empty());
        let before = if shown {
            self.message_rows()
        } else {
            Vec::new()
        };
        let mut log = MessageLog::new(channel_id);
        for message in messages {
            self.remember_author(&message);
            log.upsert(message);
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
        self.persist_messages(fx);
        if !std::mem::replace(&mut self.first_page_shown, true) {
            fx.push(Effect::StartupMark("messages shown"));
        }
        self.finish_boot(fx);
    }

    fn send(&mut self, fx: &mut Effects, text: &str) {
        let Some(channel_id) = self.active_channel.clone() else {
            return;
        };
        if self.signed_in().is_none() || text.is_empty() {
            return;
        }
        let request = ApiRequest::SendMessage {
            channel_id: channel_id.clone(),
            content: text.to_owned(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.sends.insert(id, channel_id);
        }
    }

    fn on_sent(
        &mut self,
        fx: &mut Effects,
        channel_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(ApiResponse::Sent(Some(message))) => {
                if message.channel_id == channel_id {
                    self.upsert_if_open(fx, *message);
                }
            }
            Ok(_) => {}
            Err(error) => self.fail(fx, &error),
        }
    }

    fn remember_author(&mut self, message: &Message) {
        if let Some(author) = &message.author {
            self.names
                .insert(author.id.clone(), author.label().to_owned());
        }
    }

    /// Upserts into the open channel (by id, so the POST answer and the
    /// gateway echo of the same message end up as one row). A new message
    /// takes its author off the typing line.
    fn upsert_if_open(&mut self, fx: &mut Effects, message: Message) {
        let open = self.messages.as_ref();
        let Some(log) = open.filter(|log| log.channel_id == message.channel_id) else {
            return;
        };
        let before = log
            .position(&message.id)
            .map(|i| [self.message_row_at(i), self.message_row_at(i + 1)]);
        let author = message.author.as_ref().map(|a| a.label().to_owned());
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
                if let Some(author) = author {
                    let names = self.typing.iter().filter(|n| **n != author).cloned();
                    let names = names.collect();
                    self.set_typing(fx, names);
                }
            }
        }
    }

    // Gateway

    /// (Re)subscribes the gateway to what is on screen; `voice` also
    /// re-asserts the call's voice presence.
    fn sync_subscriptions(&self, fx: &mut Effects, voice: bool) {
        if let Some(server) = &self.active_server {
            let subscribe = ServerSubscribe {
                server_id: server.clone(),
            };
            self.gateway_send(fx, frame(GatewayOp::ServerSubscribe, &subscribe));
        }
        if let Some(channel) = &self.active_channel {
            self.gateway_send(
                fx,
                Self::channel_frame(GatewayOp::ChannelSubscribe, channel),
            );
        }
        // A re-identified gateway session has no voice presence: assert it
        // again or we drop out of everyone's sidebar mid-call.
        if let Some(call) = self.voice.as_ref().filter(|_| voice) {
            self.gateway_send(fx, Self::voice_join_frame(call));
        }
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
                self.sync_subscriptions(fx, true);
                // A fresh session after a drop (Resume failed or was not
                // possible): events during the gap are lost, so refetch what
                // is on screen.
                if reconnect {
                    if let Some(channel) = self.active_channel.clone() {
                        self.load_page(fx, &channel);
                    }
                }
            }
            GatewayEvent::Resumed => {
                current.gateway.ready = true;
                current.gateway.was_ready = true;
                self.set_connected(fx, true);
                // The server restored the old subscriptions and voice
                // presence; resend the subscriptions anyway, since switches
                // made while offline were not sent (they are idempotent).
                self.sync_subscriptions(fx, false);
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
        match dispatch {
            Dispatch::MessageCreate(message) | Dispatch::MessageUpdate(message) => {
                self.remember_author(&message);
                self.upsert_if_open(fx, *message);
            }
            Dispatch::MessagePatch(patch) => {
                if patch.channel_id.as_deref() != Some(active.as_str()) {
                    return;
                }
                let Some(at) = self.messages.as_ref().and_then(|l| l.position(&patch.id)) else {
                    return;
                };
                let before = [self.message_row_at(at), self.message_row_at(at + 1)];
                if let Some(log) = &mut self.messages {
                    log.patch(&patch);
                }
                self.emit_message_changes(fx, at, before);
                self.persist_messages(fx);
            }
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
                if let Some(log) = &mut self.messages {
                    log.remove(&delete.id);
                }
                fx.messages(ViewDiff::Remove { at, count: 1 });
                self.emit_message_changes(fx, at, [below, None]);
                self.persist_messages(fx);
            }
            Dispatch::TypingStart(typing) => {
                let me = self.me().map(|me| me.id.as_str()).unwrap_or_default();
                if typing.channel_id == active && typing.user_id != me {
                    let name = self
                        .names
                        .get(&typing.user_id)
                        .cloned()
                        .unwrap_or_else(|| "Someone".to_owned());
                    let mut names = self.typing.clone();
                    if !names.contains(&name) {
                        names.push(name);
                    }
                    self.set_typing(fx, names);
                    self.typing_generation += 1;
                    fx.push(Effect::Later {
                        delay: TYPING_TIMEOUT,
                        input: Box::new(Input::TypingExpired(self.typing_generation)),
                    });
                }
            }
            Dispatch::ServersChanged => {
                if self.signed_in().is_some() {
                    self.pending.servers = self.http(fx, ApiRequest::Servers);
                }
            }
            Dispatch::VoiceChannelStates(states) => {
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
            }
            Dispatch::VoiceChannelState(update) => {
                if update.members.is_empty() {
                    self.voice_channels.remove(&update.channel_id);
                } else {
                    let occupants = self.occupants_from(&update.channel_id, update.members);
                    self.voice_channels.insert(update.channel_id, occupants);
                }
                self.sync_voice_channels(fx);
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
        self.leave_voice(fx);
        let (Some((session, me)), Some(server_id)) = (self.signed_in(), self.active_server.clone())
        else {
            return;
        };
        let target = VoiceTarget {
            server_id,
            channel_id: channel_id.clone(),
            me_name: me.label().to_owned(),
            me_username: me.username.clone(),
        };
        let token = session.token.clone();
        let call = CallId(self.next_id());
        fx.push(Effect::VoiceStart {
            call,
            token,
            target,
        });
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
            channel_id,
            status: "Connecting".into(),
            roster: Vec::new(),
            muted: prefs.muted,
            deafened: prefs.deafened,
        });
        self.emit_voice(fx);
    }

    fn leave_voice(&mut self, fx: &mut Effects) {
        if let Some(call) = self.voice.take() {
            fx.push(Effect::Voice {
                call: call.id,
                command: VoiceCommand::Leave,
            });
            self.gateway_send(fx, Self::voice_leave_frame(&call.channel_id));
        }
        self.emit_voice(fx);
    }

    fn on_voice(&mut self, fx: &mut Effects, call: CallId, update: VoiceUpdate) {
        let Some(current) = self.voice.as_mut().filter(|c| c.id == call) else {
            return;
        };
        match update {
            VoiceUpdate::Status(status) => current.status = status,
            VoiceUpdate::Roster(roster) => current.roster = roster,
            VoiceUpdate::Connected => {
                // Sidebar presence for everyone else (VOICE_PROTOCOL.md §7).
                let join = Self::voice_join_frame(current);
                self.gateway_send(fx, join);
            }
            VoiceUpdate::Ended(error) => {
                if let Some(ended) = self.voice.take() {
                    fx.push(Effect::VoiceForget(ended.id));
                    self.gateway_send(fx, Self::voice_leave_frame(&ended.channel_id));
                }
                if let Some(error) = error {
                    self.set_status(fx, format!("Left the call: {error}"));
                }
            }
        }
        self.emit_voice(fx);
    }
}

fn unexpected(response: &ApiResponse) -> ApiError {
    ApiError::Decode(format!("unexpected response: {response:?}"))
}
