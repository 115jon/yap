//! Home and DMs, presence and status, the member panel and the user popout
//! (chat-gateway.ts, chat-actions.ts, MemberList.tsx, UserProfilePopover.tsx).

use std::borrow::Cow;

use yap_protocol::payload::{
    GuildMemberAdd, GuildMemberRemove, GuildMemberUpdate, PresenceList, PresenceStatus,
    PresenceUpdateEvent, ServerSubscribe, UserProfileUpdate,
};
use yap_protocol::rest::{DmChannel, Member, User};
use yap_protocol::GatewayOp;

use super::{frame, Effects};
use crate::api::ApiError;
use crate::store::state::{live_status, Presence, State};
use crate::store::view::{self, DmBadge, DmView, MemberContext, ProfileView, ViewUpdate};
use crate::store::{ApiRequest, ApiResponse, CacheWrite};

/// What `/api/presence` and op 26 accept; `offline` is Invisible.
const STATUSES: [&str; 4] = ["online", "idle", "dnd", "offline"];

const DM_FAILED: &str = "Couldn't open the conversation. Try again.";

fn status_frame(status: &str) -> Option<yap_protocol::Frame> {
    let presence = PresenceStatus {
        status: status.to_owned(),
    };
    frame(GatewayOp::PresenceUpdate, &presence)
}

impl State {
    /// Sign-out: no DMs, presence or popout carry over to the next user.
    pub(super) fn clear_social(&mut self, fx: &mut Effects) {
        self.dms.clear();
        self.presence = Presence::default();
        self.clear_activity();
        self.profile = None;
        if std::mem::replace(&mut self.home, false) {
            fx.view(ViewUpdate::Home(false));
        }
        self.sync_dms(fx);
        self.sync_members(fx);
        self.emit_profile(fx);
        self.emit_active_dm(fx);
    }

    /// Presence, members or DMs changed: every list that shows them.
    fn sync_social(&mut self, fx: &mut Effects) {
        self.sync_dms(fx);
        self.sync_members(fx);
        self.emit_profile(fx);
    }

    // Projections

    pub(super) fn sync_dms(&mut self, fx: &mut Effects) {
        let (rows, unread) =
            view::dm_rows(&self.dms, &self.api_base, &self.presence, |id| DmBadge {
                unread: self.is_unread(id),
                has_unread: self.has_unread(id),
                mentions: self.dm_mentions(id),
                notifications: self.dm_notifications(id),
            });
        for diff in view::ordered_diff(&self.view.dms, &rows) {
            fx.view(ViewUpdate::Dms(diff));
        }
        self.view.dms = rows;
        if unread != self.view.dm_unread {
            self.view.dm_unread = unread;
            fx.view(ViewUpdate::DmUnread(unread));
        }
    }

    /// The open DM, if a DM is open.
    pub(super) fn active_dm(&self) -> Option<&DmChannel> {
        self.active_channel.as_deref().and_then(|id| self.dm(id))
    }

    /// Who the member panel lists, and the server owner: the shown server's
    /// members, or in a DM the two of you (ChatArea.tsx `computedMembers`).
    fn panel_members(&self) -> (Cow<'_, [Member]>, Option<&str>) {
        if self.home {
            return match (self.active_dm(), self.me()) {
                (Some(dm), Some(me)) => {
                    let pair = [me, &dm.recipient].map(|user| Member {
                        user: user.clone(),
                        roles: Vec::new(),
                        joined_at: None,
                    });
                    (Cow::Owned(pair.to_vec()), None)
                }
                _ => (Cow::Borrowed(&[]), None),
            };
        }
        let Some(server) = self.active_server.as_deref() else {
            return (Cow::Borrowed(&[]), None);
        };
        let owner = self
            .servers
            .iter()
            .find(|s| s.id == server)
            .and_then(|s| s.owner_id.as_deref());
        let members = self
            .members
            .get(server)
            .map_or(Cow::Borrowed(&[][..]), |list| {
                Cow::Borrowed(list.as_slice())
            });
        (members, owner)
    }

    /// The voice channel you are in: the call's, else where the gateway
    /// lists you (MemberList.tsx `findCallChannelId`).
    pub(super) fn call_channel(&self) -> Option<&str> {
        if let Some(call) = &self.voice {
            return Some(&call.channel_id);
        }
        let me = self.me()?.id.as_str();
        self.voice_channels
            .iter()
            .find(|(_, list)| list.iter().any(|o| o.user_id == me))
            .map(|(id, _)| id.as_str())
    }

    pub(super) fn sync_members(&mut self, fx: &mut Effects) {
        let rows = {
            let (members, owner_id) = self.panel_members();
            let call = self
                .call_channel()
                .map(|id| self.seats(id))
                .unwrap_or_default();
            let ctx = MemberContext {
                api_base: &self.api_base,
                me_id: self.me().map_or("", |me| me.id.as_str()),
                owner_id,
                presence: &self.presence,
                typing: &self.typing,
                call: &call,
            };
            view::member_list_rows(&members, &ctx)
        };
        for diff in view::moving_diff(&self.view.members, &rows) {
            fx.view(ViewUpdate::Members(diff));
        }
        self.view.members = rows;
        let names: Vec<String> = self
            .mention_candidates()
            .iter()
            .map(|u| u.username.to_lowercase())
            .collect();
        let names = serde_json::to_string(&names).unwrap_or_default();
        if names != self.view.mention_names {
            self.view.mention_names.clone_from(&names);
            fx.view(ViewUpdate::MentionNames(names));
        }
    }

    pub(super) fn emit_active_dm(&mut self, fx: &mut Effects) {
        let dm = self
            .active_dm()
            .map(|dm| {
                let avatar = view::avatar_url(
                    &self.api_base,
                    dm.recipient.avatar_url.as_deref(),
                    dm.recipient.avatar_display.as_ref(),
                );
                let deco = if avatar.is_empty() {
                    String::new()
                } else {
                    crate::collectibles::decoration_url(dm.recipient.avatar_display.as_ref())
                };
                DmView {
                    user_id: dm.recipient.id.clone(),
                    username: dm.recipient.username.clone(),
                    avatar,
                    deco,
                }
            })
            .unwrap_or_default();
        if dm != self.view.dm {
            self.view.dm = dm.clone();
            fx.view(ViewUpdate::ActiveDm(dm));
        }
        // The open DM's call area and header key follow the open DM.
        self.sync_calls(fx);
    }

    /// A user as the lists know them: you, a member of any server, a DM
    /// recipient, or an author in the open channel.
    pub(in crate::store) fn known_user(&self, id: &str) -> Option<&User> {
        if let Some(me) = self.me().filter(|me| me.id == id) {
            return Some(me);
        }
        let active = self
            .active_server
            .as_deref()
            .and_then(|server| self.members.get(server));
        active
            .into_iter()
            .chain(self.members.values())
            .flatten()
            .map(|m| &m.user)
            .chain(self.dms.iter().map(|dm| &dm.recipient))
            .find(|u| u.id == id)
            .or_else(|| {
                self.messages
                    .as_ref()?
                    .iter()
                    .filter_map(|m| m.author.as_deref())
                    .find(|u| u.id == id)
            })
    }

    fn profile_view(&self) -> ProfileView {
        let Some((id, full)) = &self.profile else {
            return ProfileView::default();
        };
        let Some(user) = full.as_ref().or_else(|| self.known_user(id)) else {
            // Not in any list: an empty card until the profile answers.
            return ProfileView {
                user_id: id.clone(),
                ..ProfileView::default()
            };
        };
        let member = self
            .active_server
            .as_deref()
            .filter(|_| !self.home)
            .and_then(|server| self.members.get(server))
            .and_then(|list| list.iter().find(|m| &m.user.id == id));
        let roles = member.map_or(&[][..], |m| m.roles.as_slice());
        let me = self.me().map_or("", |me| me.id.as_str());
        let mut view = view::profile_view(user, roles, me, &self.presence, &self.api_base);
        // ProfileCard.tsx "Mutual servers": not on your own card.
        view.member_since = member_since_json(user, member);
        if let Some((_, why)) = self.profile_error.as_ref().filter(|(owner, _)| owner == id) {
            view.load_error = why.clone();
        }
        if let Some((_, _, friends)) = self
            .profile_mutuals
            .as_ref()
            .filter(|(owner, _, f)| owner == id && !view.me && f.count > 0)
        {
            let items: Vec<serde_json::Value> = friends
                .items
                .iter()
                .map(|f| {
                    let name = f
                        .display_name
                        .as_deref()
                        .map(str::trim)
                        .filter(|n| !n.is_empty())
                        .unwrap_or(&f.username);
                    let avatar = view::avatar_url(&self.api_base, f.avatar_url.as_deref(), f.avatar_display.as_ref());
                    let deco = if avatar.is_empty() {
                        String::new()
                    } else {
                        crate::collectibles::decoration_url(f.avatar_display.as_ref())
                    };
                    serde_json::json!({
                        "id": f.id, "name": name, "avatar": avatar, "deco": deco,
                        "initial": view::initials(name).chars().next().map(String::from).unwrap_or_default(),
                    })
                })
                .collect();
            view.mutual_friends =
                serde_json::json!({ "count": friends.count, "items": items }).to_string();
        }
        if let Some((_, mutuals, _)) = self
            .profile_mutuals
            .as_ref()
            .filter(|(owner, m, _)| owner == id && !view.me && m.count > 0)
        {
            let items: Vec<serde_json::Value> = mutuals
                .items
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "id": s.id, "name": s.name,
                        "icon": view::absolute_url(&self.api_base, s.icon_url.as_deref()),
                        "initial": s.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default(),
                    })
                })
                .collect();
            view.mutual_servers =
                serde_json::json!({ "count": mutuals.count, "items": items }).to_string();
        }
        view
    }

    pub(super) fn emit_profile(&mut self, fx: &mut Effects) {
        let view = self.profile_view();
        if view != self.view.profile {
            self.view.profile = view.clone();
            fx.view(ViewUpdate::Profile(Box::new(view)));
        }
    }

    /// The `@` candidates: the shown server's members, or the two of you.
    pub(super) fn mention_candidates(&self) -> Vec<&User> {
        if self.home {
            return match (self.me(), self.active_dm()) {
                (Some(me), Some(dm)) => vec![me, &dm.recipient],
                _ => Vec::new(),
            };
        }
        self.active_server
            .as_ref()
            .and_then(|server| self.members.get(server))
            .map(|list| list.iter().map(|m| &m.user).collect())
            .unwrap_or_default()
    }

    // Home and DMs

    fn enter_home(&mut self, fx: &mut Effects) {
        self.close_voice_panel(fx);
        if !std::mem::replace(&mut self.home, true) {
            fx.view(ViewUpdate::Home(true));
        }
    }

    pub(super) fn leave_home(&mut self, fx: &mut Effects) {
        if std::mem::replace(&mut self.home, false) {
            fx.view(ViewUpdate::Home(false));
        }
    }

    /// Home with no conversation open (the web's `/chat/@me`).
    pub(super) fn select_home(&mut self, fx: &mut Effects) {
        if self.signed_in().is_none() {
            return;
        }
        self.enter_home(fx);
        if let Some(previous) = self.active_channel.clone() {
            self.gateway_send(
                fx,
                Self::channel_frame(GatewayOp::ChannelUnsubscribe, &previous),
            );
        }
        if self.active_channel.is_some() || self.messages.is_some() {
            self.clear_channel(fx);
        }
        self.sync_badges(fx);
        self.sync_members(fx);
    }

    pub(super) fn select_dm(&mut self, fx: &mut Effects, id: &str) {
        let Some(dm) = self.dm(id) else {
            return;
        };
        let name = match dm.recipient.label() {
            "" => dm.name.clone(),
            label => label.to_owned(),
        };
        self.enter_home(fx);
        self.open_text(fx, id, name);
    }

    /// "Message" on a profile: the DM with them, created when there is none
    /// (chat-actions.ts `openDm`).
    pub(super) fn open_dm(&mut self, fx: &mut Effects, user_id: &str) {
        let Some((_, me)) = self.signed_in() else {
            return;
        };
        if user_id.is_empty() || me.id == user_id {
            return;
        }
        let known = self
            .dms
            .iter()
            .find(|dm| dm.recipient.id == user_id)
            .map(|dm| dm.id.clone());
        if let Some(id) = known {
            return self.select_dm(fx, &id);
        }
        let request = ApiRequest::OpenDm {
            user_id: user_id.to_owned(),
        };
        self.pending.open_dm = self.http(fx, request).map(|id| (id, user_id.to_owned()));
    }

    pub(super) fn on_dm_opened(
        &mut self,
        fx: &mut Effects,
        _user_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(ApiResponse::Dm(dm)) => {
                let id = dm.id.clone();
                self.add_dm(fx, *dm);
                self.select_dm(fx, &id);
            }
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            Ok(_) | Err(_) => fx.view(ViewUpdate::Notice(DM_FAILED.to_owned())),
        }
    }

    pub(super) fn on_dms(&mut self, fx: &mut Effects, result: Result<ApiResponse, ApiError>) {
        let dms = match result {
            Ok(ApiResponse::Dms(dms)) => dms,
            Err(error) if error.is_unauthorized() => return self.fail(fx, &error),
            // The list stays as it was; the next reconnect asks again.
            Ok(_) | Err(_) => return,
        };
        for dm in &dms {
            self.remember_user(&dm.recipient);
        }
        self.dms = dms;
        self.forget_lost_dms();
        self.sync_social(fx);
        self.emit_active_dm(fx);
    }

    /// A DM someone opened (`DM_CHANNEL_CREATE`) or we did: on top.
    pub(super) fn add_dm(&mut self, fx: &mut Effects, dm: DmChannel) {
        if self.dm(&dm.id).is_some() {
            return;
        }
        self.remember_user(&dm.recipient);
        self.dms.insert(0, dm);
        self.sync_dms(fx);
    }

    fn remember_user(&mut self, user: &User) {
        self.names
            .entry(user.id.clone())
            .or_insert_with(|| user.label().to_owned());
    }

    // Status and presence

    /// The status picker: stored over REST (and broadcast to the servers),
    /// and on the gateway so this session's presence follows at once.
    pub(super) fn set_presence(&mut self, fx: &mut Effects, status: &str, custom: Option<String>) {
        if !STATUSES.contains(&status) {
            return;
        }
        let custom = custom
            .map(|c| c.trim().to_owned())
            .filter(|c| !c.is_empty());
        let Some(me) = self.session.as_mut().and_then(|s| s.me.as_mut()) else {
            return;
        };
        let status_changed = me.status.as_deref() != Some(status);
        if !status_changed && me.custom_status == custom {
            return;
        }
        me.status = Some(status.to_owned());
        me.custom_status.clone_from(&custom);
        let me = me.clone();
        fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
        self.persist(fx, CacheWrite::Me(Box::new(me.clone())));
        self.apply_presence(&me.id, status, Some(custom.clone()));
        let request = ApiRequest::SetPresence {
            status: status.to_owned(),
            custom_status: custom,
        };
        let _ = self.http(fx, request);
        if status_changed {
            self.gateway_send(fx, status_frame(status));
        }
        self.sync_social(fx);
    }

    /// After Ready: a status other than online goes to the new session
    /// (chat-gateway.ts `handleGatewayReady`).
    pub(super) fn send_status(&self, fx: &mut Effects) {
        let status = self
            .me()
            .and_then(|me| me.status.as_deref())
            .filter(|s| *s != "online" && STATUSES.contains(s));
        if let Some(status) = status {
            self.gateway_send(fx, status_frame(status));
        }
    }

    fn apply_presence(&mut self, user_id: &str, status: &str, custom: Option<Option<String>>) {
        match live_status(Some(status)) {
            Some(live) => {
                self.presence.live.insert(user_id.to_owned(), live);
            }
            None => {
                self.presence.live.remove(user_id);
            }
        }
        if let Some(custom) = custom {
            self.presence.custom.insert(user_id.to_owned(), custom);
        }
    }

    pub(super) fn on_presence(&mut self, fx: &mut Effects, update: PresenceUpdateEvent) {
        let PresenceUpdateEvent {
            user_id,
            status,
            custom_status,
            platforms,
            game_activity,
        } = update;
        self.apply_presence_game(&user_id, &status, game_activity.map(|g| g.map(|g| *g)));
        self.presence.reported.insert(user_id.clone());
        if let Some(platforms) = platforms {
            self.presence.platforms.insert(user_id.clone(), platforms);
        }
        self.apply_presence(&user_id, &status, custom_status.clone());
        // Our own status changed on another device: the account row follows.
        let me = self
            .session
            .as_mut()
            .and_then(|s| s.me.as_mut())
            .filter(|me| me.id == user_id && STATUSES.contains(&status.as_str()));
        if let Some(me) = me {
            let before = me.clone();
            me.status = Some(status);
            if let Some(custom) = custom_status {
                me.custom_status = custom;
            }
            if *me != before {
                let me = me.clone();
                fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
                self.persist(fx, CacheWrite::Me(Box::new(me)));
            }
        }
        self.sync_social(fx);
    }

    /// Everyone connected: anyone missing is offline from now on.
    pub(super) fn on_presence_list(&mut self, fx: &mut Effects, list: PresenceList) {
        self.apply_presence_list_games(&list);
        self.presence.listed = true;
        self.presence.live.clear();
        match list.users {
            Some(users) => {
                self.presence.platforms.clear();
                for user in users {
                    self.presence
                        .platforms
                        .insert(user.user_id.clone(), user.platforms.unwrap_or_default());
                    let status = user.status.as_deref().unwrap_or("online");
                    self.apply_presence(&user.user_id, status, user.custom_status);
                }
            }
            None => {
                for id in list.user_ids {
                    self.presence.live.insert(id, "online");
                }
            }
        }
        // Ours is what we chose: the list can predate our op 26.
        if let Some((id, status)) = self
            .me()
            .map(|me| (me.id.clone(), me.status.clone().unwrap_or_default()))
        {
            self.apply_presence(&id, &status, None);
        }
        self.sync_social(fx);
    }

    // Members

    pub(super) fn on_member_add(&mut self, fx: &mut Effects, add: GuildMemberAdd) {
        let GuildMemberAdd {
            server_id,
            user,
            roles,
        } = add;
        if self.me().is_some_and(|me| me.id == user.id) {
            // Joined elsewhere: its events, then the server list.
            let subscribe = ServerSubscribe {
                server_id: server_id.clone(),
            };
            self.gateway_send(fx, frame(GatewayOp::ServerSubscribe, &subscribe));
            self.pending.servers = self.http(fx, ApiRequest::Servers);
        }
        self.remember_user(&user);
        // A member who just joined is online (chat-gateway.ts `USER_ONLINE`).
        if self.presence.listed && !self.presence.live.contains_key(&user.id) {
            self.presence.live.insert(user.id.clone(), "online");
        }
        if let Some(list) = self.members.get_mut(&server_id) {
            match list.iter().position(|m| m.user.id == user.id) {
                // An update keeps when they joined.
                Some(i) => {
                    let joined_at = list[i].joined_at.take();
                    list[i] = Member {
                        user,
                        roles,
                        joined_at,
                    };
                }
                None => list.push(Member {
                    user,
                    roles,
                    joined_at: None,
                }),
            }
        }
        self.sync_social(fx);
    }

    pub(super) fn on_member_remove(&mut self, fx: &mut Effects, remove: GuildMemberRemove) {
        let GuildMemberRemove { server_id, user_id } = remove;
        if self.me().is_some_and(|me| me.id == user_id) {
            // Kicked, banned or left elsewhere: out of its voice channel,
            // then the server list (which moves off the server if shown).
            let in_call = self.voice.as_ref().is_some_and(|call| {
                self.channels
                    .get(&server_id)
                    .is_some_and(|tree| tree.get(&call.channel_id).is_some())
            });
            if in_call {
                self.leave_voice(fx);
            }
            self.members.remove(&server_id);
            self.pending.servers = self.http(fx, ApiRequest::Servers);
        } else if let Some(list) = self.members.get_mut(&server_id) {
            list.retain(|m| m.user.id != user_id);
        }
        self.sync_social(fx);
    }

    pub(super) fn on_member_update(&mut self, fx: &mut Effects, update: GuildMemberUpdate) {
        let member = self
            .members
            .get_mut(&update.server_id)
            .and_then(|list| list.iter_mut().find(|m| m.user.id == update.user_id));
        if let Some(member) = member {
            member.roles = update.roles.unwrap_or_default();
        }
        self.sync_social(fx);
    }

    /// A user changed their profile: every copy of them follows.
    pub(super) fn on_profile_update(&mut self, fx: &mut Effects, update: &UserProfileUpdate) {
        let id = update.user_id.as_str();
        let members = self.members.values_mut().flatten().map(|m| &mut m.user);
        let recipients = self.dms.iter_mut().map(|dm| &mut dm.recipient);
        for user in members.chain(recipients).filter(|u| u.id == id) {
            update.apply(user);
        }
        if let Some((_, Some(full))) = self.profile.as_mut().filter(|(user, _)| user == id) {
            update.apply(full);
        }
        let me = self
            .session
            .as_mut()
            .and_then(|s| s.me.as_mut())
            .filter(|me| me.id == id);
        if let Some(me) = me {
            update.apply(me);
            let me = me.clone();
            fx.view(ViewUpdate::Me(view::me(&me, &self.api_base)));
            self.persist(fx, CacheWrite::Me(Box::new(me)));
        }
        if let Some(user) = self.known_user(id) {
            let name = user.label().to_owned();
            self.names.insert(id.to_owned(), name);
        }
        // Their messages in the open channel wear the new look at once
        // (the web reads authors from the member list).
        let authored: Vec<(usize, String)> = self
            .messages
            .as_ref()
            .map(|log| {
                log.iter()
                    .enumerate()
                    .filter(|(_, m)| m.author_id == id)
                    .map(|(i, m)| (i, m.id.clone()))
                    .collect()
            })
            .unwrap_or_default();
        if !authored.is_empty() {
            let indices: Vec<usize> = authored.iter().map(|(i, _)| *i).collect();
            self.touch_rows(fx, &indices, |state| {
                let Some(log) = state.messages.as_mut() else {
                    return;
                };
                for (_, message_id) in &authored {
                    if let Some(author) = log.get_mut(message_id).and_then(|m| m.author.as_mut()) {
                        update.apply(std::sync::Arc::make_mut(author));
                    }
                }
            });
        }
        self.sync_social(fx);
        self.sync_roster(fx);
        self.sync_shop(fx);
    }

    // The popout

    /// Opens a user's popout (and fetches their full profile), or closes it.
    pub(super) fn show_profile(&mut self, fx: &mut Effects, user_id: Option<String>) {
        self.pending.profile = None;
        self.profile_error = None;
        // A profile fetched before (an earlier open, or a hover's prefetch)
        // shows at once; the refetch below brings it up to date.
        self.profile = user_id.filter(|id| !id.is_empty()).map(|id| {
            let known = self.profile_cache.get(&id).cloned();
            (id, known)
        });
        let fetch = self
            .profile
            .as_ref()
            .map(|(id, _)| id.clone())
            .filter(|id| self.me().is_none_or(|me| &me.id != id));
        if let Some(user_id) = fetch {
            let request = ApiRequest::Profile {
                user_id: user_id.clone(),
            };
            self.pending.profile = self.http(fx, request).map(|id| (id, user_id));
        }
        self.emit_profile(fx);
    }

    pub(super) fn on_profile(
        &mut self,
        fx: &mut Effects,
        user_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        match result {
            Ok(ApiResponse::Profile(user, mutuals, friends)) => {
                self.cache_profile(&user);
                if let Some((_, full)) = self.profile.as_mut().filter(|(id, _)| id == user_id) {
                    *full = Some(*user);
                    self.profile_mutuals = Some((user_id.to_owned(), mutuals, friends));
                }
                self.emit_profile(fx);
            }
            Err(error) if error.is_unauthorized() => self.fail(fx, &error),
            // The card keeps what the lists knew; the full profile says why
            // the rest is missing and offers Try again.
            Err(error) => {
                if self.profile.as_ref().is_some_and(|(id, _)| id == user_id) {
                    let name = self
                        .known_user(user_id)
                        .map(|u| u.label().to_owned())
                        .unwrap_or_else(|| "this user".to_owned());
                    let why =
                        super::media::describe_error(&error, &format!("load {name}'s profile"));
                    self.profile_error = Some((user_id.to_owned(), why));
                }
                self.emit_profile(fx);
            }
            Ok(_) => {}
        }
    }

    /// Profiles kept for the session: a few screens of people, not every
    /// author ever hovered.
    fn cache_profile(&mut self, user: &yap_protocol::rest::User) {
        const KEPT: usize = 200;
        if self.profile_cache.len() >= KEPT && !self.profile_cache.contains_key(&user.id) {
            self.profile_cache.clear();
        }
        self.profile_cache.insert(user.id.clone(), user.clone());
    }

    /// The pointer rests on a name or avatar that opens a popout: its
    /// profile is fetched once, so the card opens with its banner.
    pub(super) fn prefetch_profile(&mut self, fx: &mut Effects, user_id: String) {
        let me = self.me().is_some_and(|me| me.id == user_id);
        let asked = self
            .pending
            .profile_prefetch
            .values()
            .any(|id| *id == user_id)
            || self
                .pending
                .profile
                .as_ref()
                .is_some_and(|(_, id)| *id == user_id);
        if user_id.is_empty() || me || asked || self.profile_cache.contains_key(&user_id) {
            return;
        }
        let request = ApiRequest::Profile {
            user_id: user_id.clone(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.profile_prefetch.insert(id, user_id);
        }
    }

    pub(super) fn on_profile_prefetch(
        &mut self,
        fx: &mut Effects,
        user_id: &str,
        result: Result<ApiResponse, ApiError>,
    ) {
        // A failed prefetch is silent: the popout asks again when it opens.
        let Ok(ApiResponse::Profile(user, mutuals, friends)) = result else {
            return;
        };
        self.cache_profile(&user);
        // The popout opened while this was on its way.
        if let Some((_, full)) = self
            .profile
            .as_mut()
            .filter(|(id, full)| id == user_id && full.is_none())
        {
            *full = Some(*user);
            self.profile_mutuals = Some((user_id.to_owned(), mutuals, friends));
            self.emit_profile(fx);
        }
    }
}

/// resolveProfileReferenceDate: the join date in the shown server, else the
/// account's creation, as `{label, value}` JSON with the date as
/// `Nov 5, 2026` in UTC; empty without either.
pub(super) fn member_since_json(
    user: &yap_protocol::rest::User,
    member: Option<&yap_protocol::rest::Member>,
) -> String {
    let date = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => crate::store::time::parse_rfc3339(s),
        serde_json::Value::Number(n) => n.as_f64().filter(|n| *n > 0.0).map(|n| n as i64),
        _ => None,
    };
    let since = member
        .and_then(|m| m.joined_at.as_ref())
        .and_then(date)
        .map(|ms| ("Member Since", ms))
        .or_else(|| {
            user.created_at
                .as_deref()
                .and_then(crate::store::time::parse_rfc3339)
                .map(|ms| ("Account Created", ms))
        });
    since.map_or_else(String::new, |(label, ms)| {
        let value = crate::store::time::local_date(ms, crate::store::time::LocalOffset::UTC, false);
        serde_json::json!({ "label": label, "value": value }).to_string()
    })
}
