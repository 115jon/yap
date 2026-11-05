//! Server settings (ServerSettingsModal.tsx and its pages), channel
//! settings (ChannelSettingsModal.tsx), the tree's create, delete and
//! reorder, and member moderation: each call shaped as the web makes it,
//! with its error copy, plus the `GUILD_*` / `CHANNEL_*` dispatches that
//! keep the tree and the open settings current.

use serde_json::{json, Map, Value};
use yap_protocol::payload::{ChannelDeleteEvent, ChannelUpdateEvent};
use yap_protocol::rest::{Channel, ChannelType, PermissionOverride, Role, Server};

use super::Effects;
use crate::admin::{
    self, describe_audit, invite_link, invite_uses, is_role_color, member_permissions,
    previous_role_states, sanitize_channel_name, ServerAccess, BAN_REASON_MAX,
};
use crate::api::ApiError;
use crate::settings::describe_api_error;
use crate::store::admin::{AdminOp, AdminThen};
use crate::store::state::{ServerChannels, State};
use crate::store::time::{local_date, local_date_time, parse_rfc3339};
use crate::store::view::{
    absolute_url, initials, AdminChannelView, AdminServerView, AdminView, AuditRow, BanRow,
    InviteRow, Load, MemberOption, NamedOption, OverrideRow, RoleRow, ViewUpdate,
};
use crate::store::{
    AdminCommand, AdminList, AdminRequest, AdminResponse, ApiRequest, ApiResponse, CacheWrite,
    Effect, OverrideDraft, ProfileAsset, RequestId, ServerDraft,
};

fn role_row(role: &Role) -> RoleRow {
    RoleRow {
        id: role.id.clone(),
        name: role.name.clone(),
        color: role.color.clone().unwrap_or_default(),
        permissions: role.permissions,
        position: role.position,
        is_default: role.is_default,
    }
}

fn map_load<T, U>(load: &Load<T>, f: impl FnOnce(&T) -> U) -> Load<U> {
    match load {
        Load::Idle => Load::Idle,
        Load::Loading => Load::Loading,
        Load::Ready(value) => Load::Ready(f(value)),
        Load::Failed(error) => Load::Failed(error.clone()),
    }
}

fn admin_response(result: Result<ApiResponse, ApiError>) -> Result<AdminResponse, ApiError> {
    match result? {
        ApiResponse::Admin(response) => Ok(response),
        // Fire-and-forget answers (an upload) say nothing more.
        ApiResponse::Done => Ok(AdminResponse::Done),
        _ => Err(ApiError::Decode("unexpected response".into())),
    }
}

/// Applies a `PATCH /settings` body to the server it was sent for.
fn apply_server_patch(server: &mut Server, patch: &Value) {
    let flag = |key: &str| patch.get(key).and_then(Value::as_bool);
    if let Some(name) = patch.get("name").and_then(Value::as_str) {
        server.name = name.to_owned();
    }
    if let Some(icon) = patch.get("icon_url") {
        server.icon_url = icon.as_str().map(str::to_owned);
    }
    if let Some(on) = flag("invites_paused") {
        server.invites_paused = Some(on);
    }
    if let Some(on) = flag("allow_public_shares") {
        server.allow_public_shares = Some(on);
    }
    if let Some(on) = flag("show_source_in_shares") {
        server.show_source_in_shares = Some(on);
    }
    if let Some(on) = flag("allow_share_indexing") {
        server.allow_share_indexing = Some(on);
    }
}

/// A channel as a dispatch or a save stores it, over what the tree has: the
/// viewer's `permissions` are not in it, so the old ones stay.
fn merge_channel(old: &mut Channel, new: Channel) {
    let permissions = old.permissions;
    *old = Channel {
        permissions: new.permissions.or(permissions),
        ..new
    };
}

impl State {
    // Small helpers

    fn act_start(&mut self, key: &str) -> bool {
        let act = self.admin.acts.entry(key.to_owned()).or_default();
        if act.busy {
            return false;
        }
        act.busy = true;
        act.error.clear();
        true
    }

    fn act_fail(&mut self, key: &str, error: String) {
        let act = self.admin.acts.entry(key.to_owned()).or_default();
        act.busy = false;
        act.error = error;
    }

    fn act_done(&mut self, key: &str) {
        let act = self.admin.acts.entry(key.to_owned()).or_default();
        act.busy = false;
        act.error.clear();
        act.done += 1;
    }

    /// Clears a field error (valid input) without touching its action.
    fn field_ok(&mut self, key: &str) {
        self.admin.acts.remove(key);
    }

    fn admin_http(&mut self, fx: &mut Effects, request: AdminRequest, op: AdminOp) -> bool {
        match self.http(fx, ApiRequest::Admin(request)) {
            Some(id) => {
                self.pending.admin.insert(id, op);
                true
            }
            None => false,
        }
    }

    /// Starts `key` and sends `request`; its answer lands through `then`.
    fn admin_act(&mut self, fx: &mut Effects, key: &str, request: AdminRequest, then: AdminThen) {
        if !self.act_start(key) {
            return;
        }
        let op = AdminOp::Act {
            key: key.to_owned(),
            then,
        };
        if !self.admin_http(fx, request, op) {
            self.act_fail(key, "Sign in again to do that.".into());
        }
    }

    fn server(&self, id: &str) -> Option<&Server> {
        self.servers.iter().find(|s| s.id == id)
    }

    fn shown_server(&self) -> Option<String> {
        self.active_server.clone()
    }

    /// What the signed-in user may do in a server: their roles' bits.
    pub(super) fn access(&self, server_id: &str) -> ServerAccess {
        let Some(me) = self.me() else {
            return ServerAccess::default();
        };
        let owner = self
            .server(server_id)
            .and_then(|s| s.owner_id.as_deref())
            .is_some_and(|owner| owner == me.id);
        let permissions = self
            .members
            .get(server_id)
            .and_then(|list| list.iter().find(|m| m.user.id == me.id))
            .map_or(0, member_permissions);
        ServerAccess::new(permissions, owner)
    }

    /// Manage channels in the tree's server (the tree's gear and menus).
    pub(super) fn tree_manage_all(&self) -> bool {
        self.tree_server
            .as_deref()
            .is_some_and(|id| self.access(id).manage_channels)
    }

    /// The lists below now belong to `server_id`.
    fn use_server(&mut self, server_id: &str) {
        if self.admin.server_id.as_deref() != Some(server_id) {
            self.admin.server_id = Some(server_id.to_owned());
            self.admin.roles = Load::Idle;
            self.admin.invites = Load::Idle;
            self.admin.bans = Load::Idle;
            self.admin.audit = Load::Idle;
        }
    }

    fn load_list(&mut self, fx: &mut Effects, server_id: &str, list: AdminList) {
        self.use_server(server_id);
        let slot = match list {
            AdminList::Roles => self.admin.roles.is_loading(),
            AdminList::Invites => self.admin.invites.is_loading(),
            AdminList::Bans => self.admin.bans.is_loading(),
            AdminList::Audit => self.admin.audit.is_loading(),
        };
        if slot {
            return;
        }
        let server_id = server_id.to_owned();
        let request = match list {
            AdminList::Roles => AdminRequest::Roles {
                server_id: server_id.clone(),
            },
            AdminList::Invites => AdminRequest::Invites {
                server_id: server_id.clone(),
            },
            AdminList::Bans => AdminRequest::Bans {
                server_id: server_id.clone(),
            },
            AdminList::Audit => AdminRequest::AuditLogs {
                server_id: server_id.clone(),
            },
        };
        if !self.admin_http(fx, request, AdminOp::List { server_id, list }) {
            return;
        }
        match list {
            AdminList::Roles => self.admin.roles = Load::Loading,
            AdminList::Invites => self.admin.invites = Load::Loading,
            AdminList::Bans => self.admin.bans = Load::Loading,
            AdminList::Audit => {
                self.admin.audit = Load::Loading;
                // Names for its targets, from what this user may read
                // (AuditLogTab.tsx `useAuditLookup`; each is optional).
                let access = self.admin_access();
                if access.manage_roles && matches!(self.admin.roles, Load::Idle) {
                    self.load_list(fx, &self.admin_server_id(), AdminList::Roles);
                }
                if access.ban && matches!(self.admin.bans, Load::Idle) {
                    self.load_list(fx, &self.admin_server_id(), AdminList::Bans);
                }
            }
        }
    }

    fn admin_server_id(&self) -> String {
        self.admin.server_id.clone().unwrap_or_default()
    }

    fn admin_access(&self) -> ServerAccess {
        self.admin
            .server_id
            .as_deref()
            .map(|id| self.access(id))
            .unwrap_or_default()
    }

    /// The tree of `server_id` again (after a create, delete or reorder).
    fn refetch_channels(&mut self, fx: &mut Effects, server_id: &str) {
        let shown = !self.home && self.active_server.as_deref() == Some(server_id);
        if shown {
            let request = ApiRequest::Channels {
                server_id: server_id.to_owned(),
            };
            self.pending.channels = self.http(fx, request).map(|id| (id, server_id.to_owned()));
        } else if !self.pending.trees.values().any(|s| s == server_id) {
            let request = ApiRequest::Channels {
                server_id: server_id.to_owned(),
            };
            if let Some(id) = self.http(fx, request) {
                self.pending.trees.insert(id, server_id.to_owned());
            }
        }
    }

    /// A server's members afresh (their roles changed).
    fn refetch_members(&mut self, fx: &mut Effects, server_id: &str) {
        if self.pending.members.values().any(|s| s == server_id) {
            return;
        }
        let request = ApiRequest::Members {
            server_id: server_id.to_owned(),
        };
        if let Some(id) = self.http(fx, request) {
            self.pending.members.insert(id, server_id.to_owned());
        }
    }

    /// Edits a server's tree in place, re-sorted, and shows it.
    fn edit_tree(
        &mut self,
        fx: &mut Effects,
        server_id: &str,
        edit: impl FnOnce(&mut ServerChannels),
    ) {
        let Some(list) = self.channels.get_mut(server_id) else {
            return;
        };
        edit(list);
        let sorted = ServerChannels::new(list.to_response());
        *list = sorted;
        let tree = list.to_response();
        self.persist(
            fx,
            CacheWrite::Channels {
                server_id: server_id.to_owned(),
                tree,
            },
        );
        if self.tree_server.as_deref() == Some(server_id) {
            self.sync_tree(fx);
        }
        self.sync_badges(fx);
    }

    fn server_of_channel(&self, channel_id: &str) -> Option<String> {
        self.channels
            .iter()
            .find(|(_, list)| list.get(channel_id).is_some())
            .map(|(id, _)| id.clone())
    }

    /// A channel left the tree: off it if it was open.
    fn drop_channel(&mut self, fx: &mut Effects, server_id: &str, channel_id: &str) {
        let known = self
            .channels
            .get(server_id)
            .is_some_and(|list| list.get(channel_id).is_some());
        if known {
            self.edit_tree(fx, server_id, |list| {
                list.channels.retain(|c| c.id != channel_id)
            });
        }
        if self.admin.channel_id.as_deref() == Some(channel_id) {
            self.admin.channel_id = None;
            self.admin.overrides = Load::Idle;
        }
        if self.voice_view.as_deref() == Some(channel_id) {
            self.close_voice_panel(fx);
        }
        if self.active_channel.as_deref() == Some(channel_id) {
            let next = self
                .channels
                .get(server_id)
                .and_then(|list| list.first_text())
                .map(|c| c.id.clone());
            match next {
                Some(next) => self.select_channel(fx, &next),
                None => {
                    self.clear_channel(fx);
                    self.persist_route(fx);
                }
            }
        }
    }

    /// A server is gone for this user (deleted, or they left it).
    fn drop_server(&mut self, fx: &mut Effects, server_id: &str) {
        if self.admin.open_server.as_deref() == Some(server_id) {
            self.admin.open_server = None;
        }
        if self.admin.server_id.as_deref() == Some(server_id) {
            self.admin.server_id = None;
            self.admin.channel_id = None;
        }
        if self.server(server_id).is_none() {
            return;
        }
        let rest: Vec<Server> = self
            .servers
            .iter()
            .filter(|s| s.id != server_id)
            .cloned()
            .collect();
        self.channels.remove(server_id);
        self.members.remove(server_id);
        self.select_servers(fx, rest);
    }

    // Commands

    pub(super) fn on_admin(&mut self, fx: &mut Effects, command: AdminCommand) {
        if self.signed_in().is_none() {
            return;
        }
        match command {
            AdminCommand::OpenServer(id) => {
                if self.server(&id).is_none() {
                    return;
                }
                self.admin.open_server = Some(id.clone());
                // Every page loads afresh, as the web's do on mount.
                self.admin.server_id = None;
                self.use_server(&id);
                self.admin.acts.clear();
                self.admin.created_invite.clear();
                self.admin.created_role.clear();
                self.request_members(fx, &id);
            }
            AdminCommand::CloseServer => self.admin.open_server = None,
            AdminCommand::Load { server_id, list } => {
                if self.server(&server_id).is_some() {
                    self.load_list(fx, &server_id, list);
                }
            }
            AdminCommand::SaveServer(draft) => self.save_server(fx, *draft),
            AdminCommand::DeleteServer { confirm } => self.delete_server(fx, &confirm),
            AdminCommand::SetInvitesPaused(on) => self.set_invites_paused(fx, on),
            AdminCommand::CreateInvite {
                channel_id,
                max_age,
                max_uses,
                temporary,
            } => {
                let Some(server_id) = self.admin.open_server.clone() else {
                    return;
                };
                let request = AdminRequest::CreateInvite {
                    server_id: server_id.clone(),
                    channel_id: channel_id.filter(|id| !id.is_empty()),
                    max_age,
                    max_uses,
                    temporary,
                };
                self.admin_act(
                    fx,
                    "invite",
                    request,
                    AdminThen::InviteCreated { server_id },
                );
            }
            AdminCommand::CreateQuickInvite {
                expires_hours,
                max_uses,
            } => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let request = AdminRequest::CreateQuickInvite {
                    server_id: server_id.clone(),
                    expires_hours,
                    max_uses,
                };
                self.admin_act(
                    fx,
                    "quick-invite",
                    request,
                    AdminThen::InviteCreated { server_id },
                );
            }
            AdminCommand::RevokeInvite(code) => {
                let Some(server_id) = self.admin.server_id.clone() else {
                    return;
                };
                let key = format!("revoke:{code}");
                let request = AdminRequest::RevokeInvite {
                    server_id,
                    code: code.clone(),
                };
                self.admin_act(fx, &key, request, AdminThen::InviteRevoked { code });
            }
            AdminCommand::SaveRole {
                role_id,
                name,
                color,
                permissions,
            } => self.save_role(fx, role_id, &name, &color, permissions),
            AdminCommand::DeleteRole(role_id) => {
                let Some(server_id) = self.admin.server_id.clone() else {
                    return;
                };
                let request = AdminRequest::DeleteRole {
                    server_id: server_id.clone(),
                    role_id: role_id.clone(),
                };
                self.admin_act(
                    fx,
                    "role-delete",
                    request,
                    AdminThen::RoleDeleted { server_id, role_id },
                );
            }
            AdminCommand::Unban(user_id) => {
                let Some(server_id) = self.admin.server_id.clone() else {
                    return;
                };
                let key = format!("unban:{user_id}");
                let request = AdminRequest::Unban {
                    server_id,
                    user_id: user_id.clone(),
                };
                self.admin_act(fx, &key, request, AdminThen::Unbanned { user_id });
            }
            AdminCommand::Ban { user_id, reason } => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let reason: String = reason.trim().chars().take(BAN_REASON_MAX).collect();
                let key = format!("ban:{user_id}");
                let request = AdminRequest::Ban {
                    server_id,
                    user_id,
                    reason: (!reason.is_empty()).then_some(reason),
                };
                self.admin_act(fx, &key, request, AdminThen::Removed);
            }
            AdminCommand::Kick(user_id) => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let key = format!("kick:{user_id}");
                let request = AdminRequest::Kick { server_id, user_id };
                self.admin_act(fx, &key, request, AdminThen::Removed);
            }
            AdminCommand::ToggleMemberRole { user_id, role_id } => {
                self.toggle_member_role(fx, &user_id, &role_id);
            }
            AdminCommand::OpenChannel(channel_id) => {
                let Some(server_id) = self.server_of_channel(&channel_id) else {
                    return;
                };
                self.admin.channel_id = Some(channel_id);
                self.admin.overrides = Load::Idle;
                self.admin.override_batch = None;
                self.use_server(&server_id);
                self.admin
                    .acts
                    .retain(|key, _| !(key.starts_with("channel") || key.starts_with("override")));
            }
            AdminCommand::CloseChannel => {
                self.admin.channel_id = None;
                self.admin.overrides = Load::Idle;
            }
            AdminCommand::LoadOverrides => self.load_overrides(fx),
            AdminCommand::SaveChannel {
                name,
                description,
                allow_public_shares,
            } => self.save_channel(fx, &name, &description, allow_public_shares),
            AdminCommand::SaveOverrides(drafts) => self.save_overrides(fx, drafts),
            AdminCommand::RemoveOverride(target_id) => {
                let Some(channel_id) = self.admin.channel_id.clone() else {
                    return;
                };
                let key = format!("override-remove:{target_id}");
                let request = AdminRequest::DeleteOverride {
                    channel_id: channel_id.clone(),
                    target_id: target_id.clone(),
                };
                self.admin_act(
                    fx,
                    &key,
                    request,
                    AdminThen::OverrideRemoved {
                        channel_id,
                        target_id,
                    },
                );
            }
            AdminCommand::CreateChannel {
                name,
                voice,
                category_id,
            } => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let name = sanitize_channel_name(&name, voice, true);
                if name.is_empty() {
                    return self.act_fail("create-channel.name", "Give the channel a name.".into());
                }
                self.field_ok("create-channel.name");
                let request = AdminRequest::CreateChannel {
                    server_id: server_id.clone(),
                    name,
                    voice,
                    category_id: category_id.filter(|id| !id.is_empty()),
                };
                self.admin_act(
                    fx,
                    "create-channel",
                    request,
                    AdminThen::ChannelCreated { server_id, voice },
                );
            }
            AdminCommand::CreateCategory(name) => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let name = name.trim().to_uppercase();
                if name.is_empty() {
                    return self
                        .act_fail("create-category.name", "Give the category a name.".into());
                }
                self.field_ok("create-category.name");
                let request = AdminRequest::CreateCategory {
                    server_id: server_id.clone(),
                    name,
                };
                self.admin_act(
                    fx,
                    "create-category",
                    request,
                    AdminThen::CategoryCreated { server_id },
                );
            }
            AdminCommand::DeleteChannel(channel_id) => {
                let request = AdminRequest::DeleteChannel {
                    channel_id: channel_id.clone(),
                };
                self.admin_act(
                    fx,
                    "delete-channel",
                    request,
                    AdminThen::ChannelDeleted { channel_id },
                );
            }
            AdminCommand::DeleteCategory(category_id) => {
                let Some(server_id) = self.shown_server() else {
                    return;
                };
                let request = AdminRequest::DeleteCategory {
                    server_id: server_id.clone(),
                    category_id,
                };
                self.admin_act(
                    fx,
                    "delete-category",
                    request,
                    AdminThen::CategoryDeleted { server_id },
                );
            }
            AdminCommand::Reorder {
                category_id,
                channel_ids,
            } => self.reorder(fx, category_id, channel_ids),
            AdminCommand::MoveChannel { channel_id, onto } => {
                self.move_channel(fx, &channel_id, &onto)
            }
            AdminCommand::Clear(key) => {
                self.admin.acts.remove(&key);
                if key == "invite" || key == "quick-invite" {
                    self.admin.created_invite.clear();
                }
            }
        }
    }

    fn save_server(&mut self, fx: &mut Effects, draft: ServerDraft) {
        let Some(server_id) = self.admin.open_server.clone() else {
            return;
        };
        let Some(server) = self.server(&server_id).cloned() else {
            return;
        };
        let name = draft.name.trim().to_owned();
        if name.is_empty() {
            return self.act_fail("overview.name", "Enter a server name.".into());
        }
        self.field_ok("overview.name");
        self.field_ok("overview.icon");
        let mut patch = Map::new();
        patch.insert(
            "allow_public_shares".into(),
            json!(draft.allow_public_shares),
        );
        patch.insert(
            "show_source_in_shares".into(),
            json!(draft.show_source_in_shares),
        );
        patch.insert(
            "allow_share_indexing".into(),
            json!(draft.allow_share_indexing),
        );
        if name != server.name {
            patch.insert("name".into(), json!(name));
        }
        let patch = Value::Object(patch);
        match draft.icon {
            Some((path, content_type)) => {
                if !self.act_start("overview") {
                    return;
                }
                let Some(token) = self.session.as_ref().map(|s| s.token.clone()) else {
                    return;
                };
                let id = RequestId(self.next_id());
                fx.push(Effect::ProfileUpload {
                    id,
                    token,
                    asset: ProfileAsset::ServerIcon,
                    path,
                    content_type,
                });
                self.pending.admin.insert(
                    id,
                    AdminOp::Act {
                        key: "overview".into(),
                        then: AdminThen::IconUploaded { server_id, patch },
                    },
                );
            }
            None => {
                let mut patch = patch;
                if draft.remove_icon {
                    patch["icon_url"] = Value::Null;
                }
                let request = AdminRequest::UpdateServer {
                    server_id: server_id.clone(),
                    patch: patch.clone(),
                };
                self.admin_act(
                    fx,
                    "overview",
                    request,
                    AdminThen::ServerSaved { server_id, patch },
                );
            }
        }
    }

    fn delete_server(&mut self, fx: &mut Effects, confirm: &str) {
        let Some(server_id) = self.admin.open_server.clone() else {
            return;
        };
        let Some(server) = self.server(&server_id) else {
            return;
        };
        if !self.access(&server_id).owner {
            return;
        }
        if confirm.trim() != server.name {
            let error = format!("Type {} exactly to delete it.", server.name);
            return self.act_fail("delete-server", error);
        }
        let request = AdminRequest::DeleteServer {
            server_id: server_id.clone(),
        };
        self.admin_act(
            fx,
            "delete-server",
            request,
            AdminThen::ServerDeleted { server_id },
        );
    }

    fn set_invites_paused(&mut self, fx: &mut Effects, on: bool) {
        let Some(server_id) = self.admin.open_server.clone() else {
            return;
        };
        if self.admin.act("pause").busy {
            return;
        }
        let Some(server) = self.servers.iter_mut().find(|s| s.id == server_id) else {
            return;
        };
        let before = server.invites_paused;
        server.invites_paused = Some(on);
        let request = AdminRequest::UpdateServer {
            server_id: server_id.clone(),
            patch: json!({ "invites_paused": on }),
        };
        self.admin_act(
            fx,
            "pause",
            request,
            AdminThen::InvitesPaused { server_id, before },
        );
    }

    fn save_role(
        &mut self,
        fx: &mut Effects,
        role_id: Option<String>,
        name: &str,
        color: &str,
        permissions: i64,
    ) {
        let Some(server_id) = self.admin.server_id.clone() else {
            return;
        };
        let (name, color) = (name.trim().to_owned(), color.trim().to_owned());
        let bad_name = name.is_empty();
        let bad_color = !color.is_empty() && !is_role_color(&color);
        if bad_name {
            self.act_fail("role.name", "Enter a role name.".into());
        } else {
            self.field_ok("role.name");
        }
        if bad_color {
            let error = "Enter a colour as # and six hex digits, like #3e9b4f.";
            self.act_fail("role.color", error.into());
        } else {
            self.field_ok("role.color");
        }
        if bad_name || bad_color {
            return;
        }
        let color = (!color.is_empty()).then_some(color);
        let created = role_id.is_none();
        let request = match role_id {
            Some(role_id) => AdminRequest::UpdateRole {
                server_id: server_id.clone(),
                role_id,
                name,
                color,
                permissions,
            },
            None => AdminRequest::CreateRole {
                server_id: server_id.clone(),
                name,
                color,
                permissions,
            },
        };
        self.admin_act(
            fx,
            "role",
            request,
            AdminThen::RoleSaved { server_id, created },
        );
    }

    fn toggle_member_role(&mut self, fx: &mut Effects, user_id: &str, role_id: &str) {
        let Some(server_id) = self.shown_server() else {
            return;
        };
        let key = format!("member-roles:{user_id}");
        if self.admin.act(&key).busy {
            return;
        }
        let known = self
            .admin
            .roles
            .ready()
            .filter(|_| self.admin.server_id.as_deref() == Some(server_id.as_str()))
            .and_then(|roles| roles.iter().find(|r| r.id == role_id))
            .cloned();
        let Some(member) = self
            .members
            .get_mut(&server_id)
            .and_then(|list| list.iter_mut().find(|m| m.user.id == user_id))
        else {
            return;
        };
        let before = member.roles.clone();
        if member.roles.iter().any(|r| r.id == role_id) {
            member.roles.retain(|r| r.id != role_id);
        } else if let Some(role) = known {
            member.roles.push(role);
        } else {
            return;
        }
        let role_ids = member.roles.iter().map(|r| r.id.clone()).collect();
        let request = AdminRequest::SetMemberRoles {
            server_id: server_id.clone(),
            user_id: user_id.to_owned(),
            role_ids,
        };
        self.sync_members(fx);
        self.emit_profile(fx);
        self.admin_act(
            fx,
            &key,
            request,
            AdminThen::MemberRoles {
                server_id,
                user_id: user_id.to_owned(),
                before,
            },
        );
    }

    fn load_overrides(&mut self, fx: &mut Effects) {
        let Some(channel_id) = self.admin.channel_id.clone() else {
            return;
        };
        let Some(server_id) = self.server_of_channel(&channel_id) else {
            return;
        };
        if !self.admin.overrides.is_loading() {
            let request = AdminRequest::Overrides {
                channel_id: channel_id.clone(),
            };
            if self.admin_http(fx, request, AdminOp::Overrides { channel_id }) {
                self.admin.overrides = Load::Loading;
            }
        }
        // Names for the roles and members it lists, and the picker's.
        if matches!(self.admin.roles, Load::Idle | Load::Failed(_)) {
            self.load_list(fx, &server_id, AdminList::Roles);
        }
        self.request_members(fx, &server_id);
    }

    fn save_channel(
        &mut self,
        fx: &mut Effects,
        name: &str,
        description: &str,
        allow_public_shares: Option<bool>,
    ) {
        let Some(channel) = self
            .admin
            .channel_id
            .as_deref()
            .and_then(|id| self.find_channel(id))
            .cloned()
        else {
            return;
        };
        let voice = channel.channel_type == ChannelType::Voice;
        let name = sanitize_channel_name(name, voice, true);
        if name.is_empty() {
            return self.act_fail("channel.name", "Enter a channel name.".into());
        }
        self.field_ok("channel.name");
        let description: String = description.chars().take(admin::TOPIC_MAX).collect();
        let mut patch = Map::new();
        if name != channel.name {
            patch.insert("name".into(), json!(name));
        }
        let stored = channel.description.clone().filter(|d| !d.is_empty());
        let wanted = (!description.is_empty()).then_some(description);
        if wanted != stored {
            patch.insert("description".into(), json!(wanted));
        }
        if allow_public_shares != channel.allow_public_shares {
            patch.insert("allow_public_shares".into(), json!(allow_public_shares));
        }
        if patch.is_empty() {
            return;
        }
        let request = AdminRequest::UpdateChannel {
            channel_id: channel.id.clone(),
            patch: Value::Object(patch),
        };
        self.admin_act(
            fx,
            "channel",
            request,
            AdminThen::ChannelSaved {
                channel_id: channel.id,
            },
        );
    }

    fn save_overrides(&mut self, fx: &mut Effects, drafts: Vec<OverrideDraft>) {
        let Some(channel_id) = self.admin.channel_id.clone() else {
            return;
        };
        if drafts.is_empty() || !self.act_start("overrides") {
            return;
        }
        let mut sent = 0;
        for draft in drafts {
            let request = AdminRequest::PutOverride {
                channel_id: channel_id.clone(),
                target_id: draft.target_id.clone(),
                target_type: draft.target_type.clone(),
                allow: draft.allow,
                deny: draft.deny,
            };
            let op = AdminOp::Act {
                key: "overrides".into(),
                then: AdminThen::OverrideSaved {
                    channel_id: channel_id.clone(),
                    draft,
                },
            };
            if self.admin_http(fx, request, op) {
                sent += 1;
            }
        }
        if sent == 0 {
            self.act_fail("overrides", "Sign in again to do that.".into());
        } else {
            self.admin.override_batch = Some((sent, None));
        }
    }

    fn move_channel(&mut self, fx: &mut Effects, channel_id: &str, onto: &str) {
        let Some(list) = self
            .tree_server
            .as_deref()
            .and_then(|id| self.channels.get(id))
        else {
            return;
        };
        let (Some(dragged), Some(target)) = (list.get(channel_id), list.get(onto)) else {
            return;
        };
        // Only within the same group, as on the web: a category, or the
        // loose text or voice channels.
        let loose_kinds_differ =
            dragged.category_id.is_none() && dragged.channel_type != target.channel_type;
        if channel_id == onto || dragged.category_id != target.category_id || loose_kinds_differ {
            return;
        }
        let category_id = dragged.category_id.clone();
        let kind = dragged.channel_type;
        let mut group: Vec<&yap_protocol::rest::Channel> = list
            .channels
            .iter()
            .filter(|c| c.category_id == category_id)
            .filter(|c| category_id.is_some() || c.channel_type == kind)
            .collect();
        group.sort_by_key(|c| c.position);
        let mut ids: Vec<String> = group.into_iter().map(|c| c.id.clone()).collect();
        let (Some(from), Some(to)) = (
            ids.iter().position(|id| id == channel_id),
            ids.iter().position(|id| id == onto),
        ) else {
            return;
        };
        let id = ids.remove(from);
        ids.insert(to, id);
        self.reorder(fx, category_id, ids);
    }

    fn reorder(&mut self, fx: &mut Effects, category_id: Option<String>, channel_ids: Vec<String>) {
        let Some(server_id) = self.tree_server.clone() else {
            return;
        };
        if !self.access(&server_id).manage_channels || channel_ids.is_empty() {
            return;
        }
        let category_id = category_id.filter(|id| !id.is_empty());
        let unchanged = self.channels.get(&server_id).is_some_and(|list| {
            let group: Vec<&str> = list
                .channels
                .iter()
                .filter(|c| c.category_id == category_id)
                .map(|c| c.id.as_str())
                .collect();
            group == channel_ids.iter().map(String::as_str).collect::<Vec<_>>()
        });
        if unchanged {
            return;
        }
        // Optimistic, so the row does not snap back while it saves.
        let order = channel_ids.clone();
        let group = category_id.clone();
        self.edit_tree(fx, &server_id, |list| {
            for channel in &mut list.channels {
                if let Some(position) = order.iter().position(|id| *id == channel.id) {
                    channel.position = position as i64;
                    channel.category_id = group.clone();
                }
            }
        });
        let request = AdminRequest::Reorder {
            server_id: server_id.clone(),
            category_id,
            channel_ids,
        };
        let op = AdminOp::Act {
            key: "reorder".into(),
            then: AdminThen::Reordered { server_id },
        };
        self.admin_http(fx, request, op);
    }

    // Answers

    /// Answers for admin requests; `false` if `id` is not one.
    pub(super) fn route_admin_http(
        &mut self,
        fx: &mut Effects,
        id: RequestId,
        result: &mut Option<Result<ApiResponse, ApiError>>,
    ) -> bool {
        let Some(op) = self.pending.admin.remove(&id) else {
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
        let result = admin_response(result);
        match op {
            AdminOp::List { server_id, list } => self.on_list(server_id, list, result),
            AdminOp::Overrides { channel_id } => {
                if self.admin.channel_id.as_deref() == Some(channel_id.as_str()) {
                    self.admin.overrides = match result {
                        Ok(AdminResponse::Overrides(list)) => Load::Ready(list),
                        Ok(_) => Load::Ready(Vec::new()),
                        Err(error) => Load::Failed(describe_api_error(
                            &error,
                            "load the permission overrides",
                        )),
                    };
                }
            }
            AdminOp::Act { key, then } => self.on_act(fx, &key, then, result),
        }
        true
    }

    fn on_list(
        &mut self,
        server_id: String,
        list: AdminList,
        result: Result<AdminResponse, ApiError>,
    ) {
        if self.admin.server_id.as_deref() != Some(server_id.as_str()) {
            return;
        }
        let failed = |error: &ApiError, what: &str| describe_api_error(error, what);
        match list {
            AdminList::Roles => {
                self.admin.roles = match result {
                    Ok(AdminResponse::Roles(roles)) => Load::Ready(roles),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => Load::Failed(failed(&error, "load the roles")),
                }
            }
            AdminList::Invites => {
                self.admin.invites = match result {
                    Ok(AdminResponse::Invites(invites)) => Load::Ready(invites),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => Load::Failed(failed(&error, "load the invite links")),
                }
            }
            AdminList::Bans => {
                self.admin.bans = match result {
                    Ok(AdminResponse::Bans(bans)) => Load::Ready(bans),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => Load::Failed(failed(&error, "load the ban list")),
                }
            }
            AdminList::Audit => {
                self.admin.audit = match result {
                    Ok(AdminResponse::Audit(logs)) => Load::Ready(logs),
                    Ok(_) => Load::Ready(Vec::new()),
                    Err(error) => Load::Failed(failed(&error, "load the audit log")),
                }
            }
        }
    }

    fn on_act(
        &mut self,
        fx: &mut Effects,
        key: &str,
        then: AdminThen,
        result: Result<AdminResponse, ApiError>,
    ) {
        let response = match result {
            Ok(response) => response,
            Err(error) => return self.on_act_failed(fx, key, then, &error),
        };
        match then {
            AdminThen::IconUploaded {
                server_id,
                mut patch,
            } => {
                let AdminResponse::IconUrl(url) = response else {
                    let error = ApiError::Decode("no icon URL".into());
                    return self.on_act_failed(
                        fx,
                        key,
                        AdminThen::IconUploaded { server_id, patch },
                        &error,
                    );
                };
                patch["icon_url"] = json!(url);
                let request = AdminRequest::UpdateServer {
                    server_id: server_id.clone(),
                    patch: patch.clone(),
                };
                let op = AdminOp::Act {
                    key: key.to_owned(),
                    then: AdminThen::ServerSaved { server_id, patch },
                };
                if !self.admin_http(fx, request, op) {
                    self.act_fail(key, String::new());
                }
                return;
            }
            AdminThen::ServerSaved { server_id, patch } => {
                let stored = match response {
                    AdminResponse::Server(Some(server)) if server.id == server_id => Some(*server),
                    _ => None,
                };
                if let Some(server) = self.servers.iter_mut().find(|s| s.id == server_id) {
                    match stored {
                        Some(stored) => *server = stored,
                        None => apply_server_patch(server, &patch),
                    }
                }
                self.after_server_change(fx, &server_id);
            }
            AdminThen::ServerDeleted { server_id } => {
                self.act_done(key);
                return self.drop_server(fx, &server_id);
            }
            AdminThen::InvitesPaused { .. } => {}
            AdminThen::InviteCreated { server_id } => {
                if let AdminResponse::InviteCode(code) = response {
                    self.admin.created_invite = code;
                }
                let listed = self.admin.server_id.as_deref() == Some(server_id.as_str())
                    && !matches!(self.admin.invites, Load::Idle);
                if listed {
                    self.admin.invites = Load::Idle;
                    self.load_list(fx, &server_id, AdminList::Invites);
                }
            }
            AdminThen::InviteRevoked { code } => {
                if let Load::Ready(invites) = &mut self.admin.invites {
                    invites.retain(|invite| invite.code != code);
                }
            }
            AdminThen::RoleSaved { server_id, created } => {
                if created {
                    if let AdminResponse::RoleId(Some(id)) = response {
                        self.admin.created_role = id;
                    }
                }
                self.admin.roles = Load::Idle;
                self.load_list(fx, &server_id, AdminList::Roles);
                self.refetch_members(fx, &server_id);
            }
            AdminThen::RoleDeleted { server_id, role_id } => {
                if let Load::Ready(roles) = &mut self.admin.roles {
                    roles.retain(|role| role.id != role_id);
                }
                self.refetch_members(fx, &server_id);
            }
            AdminThen::Unbanned { user_id } => {
                if let Load::Ready(bans) = &mut self.admin.bans {
                    bans.retain(|ban| ban.user_id != user_id);
                }
            }
            AdminThen::Removed => {}
            AdminThen::MemberRoles {
                server_id, user_id, ..
            } => {
                if let AdminResponse::Roles(roles) = response {
                    let member = self
                        .members
                        .get_mut(&server_id)
                        .and_then(|list| list.iter_mut().find(|m| m.user.id == user_id));
                    if let Some(member) = member {
                        member.roles = roles;
                    }
                    self.sync_members(fx);
                    self.emit_profile(fx);
                }
            }
            AdminThen::ChannelSaved { channel_id } => {
                if let AdminResponse::Channel(Some(channel)) = response {
                    self.upsert_channel(fx, *channel);
                } else if let Some(server_id) = self.server_of_channel(&channel_id) {
                    self.refetch_channels(fx, &server_id);
                }
            }
            AdminThen::OverrideSaved { channel_id, draft } => {
                if self.admin.channel_id.as_deref() == Some(channel_id.as_str()) {
                    if let Load::Ready(list) = &mut self.admin.overrides {
                        let saved = PermissionOverride {
                            target_id: draft.target_id.clone(),
                            target_type: draft.target_type.clone(),
                            allow: draft.allow,
                            deny: draft.deny,
                        };
                        match list.iter_mut().find(|o| o.target_id == draft.target_id) {
                            Some(existing) => *existing = saved,
                            None => list.push(saved),
                        }
                    }
                }
                return self.override_answered(key, None);
            }
            AdminThen::OverrideRemoved {
                channel_id,
                target_id,
            } => {
                if self.admin.channel_id.as_deref() == Some(channel_id.as_str()) {
                    if let Load::Ready(list) = &mut self.admin.overrides {
                        list.retain(|o| o.target_id != target_id);
                    }
                }
            }
            AdminThen::ChannelCreated { server_id, voice } => {
                if let AdminResponse::Channel(Some(channel)) = response {
                    let id = channel.id.clone();
                    let new = self
                        .channels
                        .get(&server_id)
                        .is_some_and(|list| list.get(&id).is_none());
                    if new {
                        self.edit_tree(fx, &server_id, |list| list.channels.push(*channel));
                    }
                    let shown = !self.home && self.active_server.as_deref() == Some(&server_id);
                    if !voice && shown {
                        self.select_channel(fx, &id);
                    }
                }
            }
            AdminThen::CategoryCreated { server_id } | AdminThen::CategoryDeleted { server_id } => {
                self.refetch_channels(fx, &server_id);
            }
            AdminThen::ChannelDeleted { channel_id } => {
                if let Some(server_id) = self.server_of_channel(&channel_id) {
                    self.drop_channel(fx, &server_id, &channel_id);
                }
            }
            AdminThen::Reordered { server_id } => {
                self.refetch_channels(fx, &server_id);
                return;
            }
        }
        self.act_done(key);
    }

    fn on_act_failed(&mut self, fx: &mut Effects, key: &str, then: AdminThen, error: &ApiError) {
        let action = match &then {
            AdminThen::IconUploaded { .. } => {
                self.act_fail(key, String::new());
                let copy = describe_api_error(error, "upload the server icon");
                return self.act_fail("overview.icon", copy);
            }
            AdminThen::ServerSaved { .. } => "save the server settings".to_owned(),
            AdminThen::ServerDeleted { .. } => "delete this server".to_owned(),
            AdminThen::InvitesPaused { server_id, before } => {
                let paused = self
                    .server(server_id)
                    .and_then(|s| s.invites_paused)
                    .unwrap_or(false);
                if let Some(server) = self.servers.iter_mut().find(|s| s.id == *server_id) {
                    server.invites_paused = *before;
                }
                if paused {
                    "pause invites".to_owned()
                } else {
                    "resume invites".to_owned()
                }
            }
            AdminThen::InviteCreated { .. } => "create the invite link".to_owned(),
            AdminThen::InviteRevoked { .. } => "revoke this invite".to_owned(),
            AdminThen::RoleSaved { created: true, .. } => "create this role".to_owned(),
            AdminThen::RoleSaved { .. } => "save this role".to_owned(),
            AdminThen::RoleDeleted { role_id, .. } => {
                let name = self
                    .admin
                    .roles
                    .ready()
                    .and_then(|roles| roles.iter().find(|r| r.id == *role_id))
                    .map_or_else(|| "this role".to_owned(), |r| r.name.clone());
                format!("delete {name}")
            }
            AdminThen::Unbanned { user_id } => {
                let name = self
                    .admin
                    .bans
                    .ready()
                    .and_then(|bans| bans.iter().find(|b| b.user_id == *user_id))
                    .and_then(|b| b.username.clone())
                    .unwrap_or_else(|| user_id.clone());
                format!("unban {name}")
            }
            AdminThen::Removed if key.starts_with("ban:") => "ban this member".to_owned(),
            AdminThen::Removed => "kick this member".to_owned(),
            AdminThen::MemberRoles {
                server_id,
                user_id,
                before,
            } => {
                let member = self
                    .members
                    .get_mut(server_id)
                    .and_then(|list| list.iter_mut().find(|m| m.user.id == *user_id));
                let name = member.map(|member| {
                    member.roles = before.clone();
                    member.user.label().to_owned()
                });
                self.sync_members(fx);
                self.emit_profile(fx);
                format!("update {}'s roles", name.unwrap_or_else(|| "their".into()))
            }
            AdminThen::ChannelSaved { .. } => "save the channel".to_owned(),
            AdminThen::OverrideSaved { .. } => {
                let copy = describe_api_error(error, "save the permission overrides");
                return self.override_answered(key, Some(copy));
            }
            AdminThen::OverrideRemoved { .. } => "remove this override".to_owned(),
            AdminThen::ChannelCreated { .. } => "create the channel".to_owned(),
            AdminThen::CategoryCreated { .. } => "create the category".to_owned(),
            AdminThen::ChannelDeleted { .. } => "delete this channel".to_owned(),
            AdminThen::CategoryDeleted { .. } => "delete this category".to_owned(),
            AdminThen::Reordered { server_id } => {
                // The order goes back to what the server has.
                let server_id = server_id.clone();
                self.refetch_channels(fx, &server_id);
                return;
            }
        };
        self.act_fail(key, describe_api_error(error, &action));
    }

    /// One override save answered; the last one settles the batch.
    fn override_answered(&mut self, key: &str, error: Option<String>) {
        let Some((left, first)) = self.admin.override_batch.as_mut() else {
            return;
        };
        *left = left.saturating_sub(1);
        if first.is_none() {
            *first = error;
        }
        if *left > 0 {
            return;
        }
        let first = first.take();
        self.admin.override_batch = None;
        match first {
            Some(error) => self.act_fail(key, error),
            None => self.act_done(key),
        }
    }

    /// The server list, the rail and the header after a server changed.
    fn after_server_change(&mut self, fx: &mut Effects, server_id: &str) {
        self.sync_servers(fx);
        self.persist(fx, CacheWrite::Servers(self.servers.clone()));
        if self.active_server.as_deref() == Some(server_id) {
            fx.view(ViewUpdate::ActiveServer {
                id: server_id.to_owned(),
                name: self.active_server_name(),
            });
        }
    }

    /// A channel as stored now, into its server's tree.
    fn upsert_channel(&mut self, fx: &mut Effects, channel: Channel) {
        let Some(server_id) = channel
            .server_id
            .clone()
            .or_else(|| self.server_of_channel(&channel.id))
        else {
            return;
        };
        let known = self
            .channels
            .get(&server_id)
            .is_some_and(|list| list.get(&channel.id).is_some());
        if !known {
            // New to this user: the list says whether they can see it.
            return self.refetch_channels(fx, &server_id);
        }
        let id = channel.id.clone();
        self.edit_tree(fx, &server_id, |list| {
            if let Some(old) = list.channels.iter_mut().find(|c| c.id == id) {
                merge_channel(old, channel);
            }
        });
        if self.active_channel.as_deref() == Some(id.as_str()) {
            let name = self
                .find_channel(&id)
                .map(|c| c.name.clone())
                .unwrap_or_default();
            self.emit_active_channel(fx, id, name);
        }
    }

    // Dispatches

    pub(super) fn on_channel_update(&mut self, fx: &mut Effects, event: ChannelUpdateEvent) {
        match event.channel {
            Some(channel) => self.upsert_channel(fx, channel),
            None => {
                if let Some(server_id) = event.server_id {
                    if self.channels.contains_key(&server_id) {
                        self.refetch_channels(fx, &server_id);
                    }
                }
            }
        }
    }

    pub(super) fn on_channel_delete(&mut self, fx: &mut Effects, event: ChannelDeleteEvent) {
        let server_id = event
            .server_id
            .or_else(|| self.server_of_channel(&event.id));
        if let Some(server_id) = server_id {
            self.drop_channel(fx, &server_id, &event.id);
        }
    }

    pub(super) fn on_guild_update(&mut self, fx: &mut Effects, server: Server) {
        let id = server.id.clone();
        let Some(known) = self.servers.iter_mut().find(|s| s.id == id) else {
            // A server this user does not have yet: the list says.
            self.pending.servers = self.http(fx, ApiRequest::Servers);
            return;
        };
        *known = server;
        self.after_server_change(fx, &id);
    }

    pub(super) fn on_guild_delete(&mut self, fx: &mut Effects, server_id: &str) {
        self.drop_server(fx, server_id);
    }

    /// Sign-out: nothing loaded for one account shows for the next.
    pub(super) fn clear_admin(&mut self) {
        self.admin = Default::default();
        self.pending.admin.clear();
    }

    // The view

    fn admin_view(&self) -> AdminView {
        let a = &self.admin;
        let mut view = AdminView {
            access: self
                .active_server
                .as_deref()
                .map(|id| self.access(id))
                .unwrap_or_default(),
            acts: a.acts.clone(),
            created_invite: a.created_invite.clone(),
            created_invite_link: if a.created_invite.is_empty() {
                String::new()
            } else {
                invite_link(&self.api_base, &a.created_invite)
            },
            created_role: a.created_role.clone(),
            categories: self
                .active_server
                .as_deref()
                .and_then(|id| self.channels.get(id))
                .map(|list| {
                    list.categories
                        .iter()
                        .map(|c| NamedOption {
                            id: c.id.clone(),
                            name: c.name.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            ..AdminView::default()
        };
        let Some(server_id) = a.server_id.as_deref() else {
            return view;
        };
        let members = self
            .members
            .get(server_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let tree = self.channels.get(server_id);
        let base = &self.api_base;
        view.server = a
            .open_server
            .as_deref()
            .and_then(|id| self.server(id))
            .map(|server| {
                let access = self.access(&server.id);
                AdminServerView {
                    id: server.id.clone(),
                    name: server.name.clone(),
                    icon: server
                        .icon_url
                        .as_deref()
                        .map(|url| absolute_url(base, Some(url)))
                        .unwrap_or_default(),
                    initials: initials(&server.name),
                    invites_paused: server.invites_paused.unwrap_or(false),
                    allow_public_shares: server.allow_public_shares.unwrap_or(true),
                    show_source_in_shares: server.show_source_in_shares.unwrap_or(false),
                    allow_share_indexing: server.allow_share_indexing.unwrap_or(false),
                    pages: access.pages(),
                    access,
                }
            });
        view.roles = map_load(&a.roles, |roles| roles.iter().map(role_row).collect());
        view.members = members
            .iter()
            .map(|m| MemberOption {
                id: m.user.id.clone(),
                name: m.user.label().to_owned(),
                avatar: absolute_url(base, m.user.avatar_url.as_deref()),
                initials: initials(m.user.label()),
                role_ids: m.roles.iter().map(|r| r.id.clone()).collect(),
            })
            .collect();
        view.text_channels = tree
            .map(|list| {
                list.channels
                    .iter()
                    .filter(|c| c.channel_type == ChannelType::Text)
                    .map(|c| NamedOption {
                        id: c.id.clone(),
                        name: c.name.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let member_name = |id: &str| {
            members
                .iter()
                .find(|m| m.user.id == id)
                .map(|m| m.user.label().to_owned())
        };
        view.invites = map_load(&a.invites, |invites| {
            invites
                .iter()
                .map(|invite| {
                    let inviter = member_name(&invite.inviter_id)
                        .or_else(|| invite.inviter.as_ref().map(|u| u.label().to_owned()))
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "Unknown".into());
                    let avatar = members
                        .iter()
                        .find(|m| m.user.id == invite.inviter_id)
                        .map(|m| &m.user)
                        .or(invite.inviter.as_ref())
                        .and_then(|u| u.avatar_url.as_deref());
                    InviteRow {
                        code: invite.code.clone(),
                        link: invite_link(base, &invite.code),
                        channel: invite
                            .channel_name
                            .as_deref()
                            .map(|name| format!("#{name}"))
                            .unwrap_or_default(),
                        inviter_initials: initials(&inviter),
                        inviter,
                        inviter_avatar: avatar
                            .map(|url| absolute_url(base, Some(url)))
                            .unwrap_or_default(),
                        uses: invite_uses(invite.uses, invite.max_uses),
                        temporary: invite.temporary,
                        expires_ms: invite
                            .expires_at
                            .as_deref()
                            .and_then(parse_rfc3339)
                            .unwrap_or(0),
                    }
                })
                .collect()
        });
        view.bans = map_load(&a.bans, |bans| {
            bans.iter()
                .map(|ban| {
                    let name = ban
                        .username
                        .clone()
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| ban.user_id.clone());
                    let by = ban
                        .banned_by_username
                        .clone()
                        .unwrap_or_else(|| "a moderator".into());
                    let date = parse_rfc3339(&ban.created_at)
                        .map(|ms| format!(" on {}", local_date(ms, self.local_offset, false)))
                        .unwrap_or_default();
                    BanRow {
                        user_id: ban.user_id.clone(),
                        initials: initials(&name),
                        avatar: ban
                            .avatar_url
                            .as_deref()
                            .map(|url| absolute_url(base, Some(url)))
                            .unwrap_or_default(),
                        name,
                        reason: ban.reason.clone().unwrap_or_default(),
                        meta: format!("Banned by {by}{date}"),
                    }
                })
                .collect()
        });
        view.audit = map_load(&a.audit, |logs| {
            let roles = a.roles.ready().map(Vec::as_slice).unwrap_or(&[]);
            let banned = a
                .bans
                .ready()
                .into_iter()
                .flatten()
                .filter_map(|b| Some((b.user_id.as_str(), b.username.as_deref()?)));
            let channels = tree.into_iter().flat_map(|list| {
                list.channels.iter().map(|c| {
                    (
                        c.id.as_str(),
                        c.name.as_str(),
                        c.channel_type == ChannelType::Voice,
                    )
                })
            });
            let lookup = admin::audit_lookup(roles, members, banned, channels);
            let previous = previous_role_states(logs);
            logs.iter()
                .map(|log| {
                    let actor = log
                        .actor
                        .as_ref()
                        .map(|u| u.label().trim().to_owned())
                        .filter(|name| !name.is_empty())
                        .unwrap_or_else(|| "Unknown".into());
                    let prior = previous.get(&log.id).and_then(Option::as_ref);
                    AuditRow {
                        id: log.id.clone(),
                        actor_initials: initials(&actor),
                        actor_avatar: log
                            .actor
                            .as_ref()
                            .and_then(|u| u.avatar_url.as_deref())
                            .map(|url| absolute_url(base, Some(url)))
                            .unwrap_or_default(),
                        actor,
                        entry: describe_audit(log, &lookup, prior),
                        reason: log.reason.clone().unwrap_or_default(),
                        when: parse_rfc3339(&log.created_at)
                            .map(|ms| local_date_time(ms, self.local_offset))
                            .unwrap_or_default(),
                    }
                })
                .collect()
        });
        let channel = a.channel_id.as_deref().and_then(|id| self.find_channel(id));
        view.channel = channel.map(|channel| {
            let voice = channel.channel_type == ChannelType::Voice;
            let created = channel
                .created_at
                .as_deref()
                .and_then(parse_rfc3339)
                .map(|ms| local_date(ms, self.local_offset, true))
                .unwrap_or_default();
            AdminChannelView {
                id: channel.id.clone(),
                name: channel.name.clone(),
                description: channel.description.clone().unwrap_or_default(),
                voice,
                label: if voice {
                    channel.name.clone()
                } else {
                    format!("#{}", channel.name)
                },
                public_shares: match channel.allow_public_shares {
                    None => "inherit",
                    Some(true) => "allow",
                    Some(false) => "deny",
                },
                created: if created.is_empty() {
                    format!("{} channel.", if voice { "Voice" } else { "Text" })
                } else {
                    format!(
                        "{} channel, created {created}.",
                        if voice { "Voice" } else { "Text" }
                    )
                },
            }
        });
        view.overrides = match (&a.overrides, &a.roles) {
            // Names come from the roles; wait for them.
            (Load::Ready(_), Load::Loading | Load::Idle) => Load::Loading,
            (load, roles) => map_load(load, |list| {
                let roles = roles.ready().map(Vec::as_slice).unwrap_or(&[]);
                override_rows(list, roles, members, base)
            }),
        };
        view
    }

    /// Sends the admin view when it changed; the tree follows the user's
    /// access (its gear and menus).
    pub(super) fn sync_admin(&mut self, fx: &mut Effects) {
        let view = self.admin_view();
        if view == self.view.admin {
            return;
        }
        let access_changed = view.access != self.view.admin.access;
        self.view.admin = view.clone();
        fx.view(ViewUpdate::Admin(Box::new(view)));
        if access_changed {
            self.sync_tree(fx);
        }
    }
}

/// The overrides as the page lists them (permission-model.ts `load`):
/// @everyone first (always, stored or not), then roles, then members.
fn override_rows(
    list: &[PermissionOverride],
    roles: &[Role],
    members: &[yap_protocol::rest::Member],
    base: &str,
) -> Vec<OverrideRow> {
    let mut rows: Vec<OverrideRow> = list
        .iter()
        .map(|o| {
            let row = OverrideRow {
                target_id: o.target_id.clone(),
                target_type: o.target_type.clone(),
                allow: o.allow,
                deny: o.deny,
                saved: true,
                ..OverrideRow::default()
            };
            if o.target_type == "role" {
                let role = roles.iter().find(|r| r.id == o.target_id);
                OverrideRow {
                    name: role
                        .map(|r| r.name.clone())
                        .filter(|n| !n.is_empty())
                        .unwrap_or_else(|| "Unknown role".into()),
                    color: role.and_then(|r| r.color.clone()).unwrap_or_default(),
                    is_default: role.is_some_and(|r| r.is_default),
                    ..row
                }
            } else {
                let member = members.iter().find(|m| m.user.id == o.target_id);
                let name = member
                    .map(|m| {
                        m.user
                            .display_name
                            .as_deref()
                            .map(str::trim)
                            .filter(|n| !n.is_empty())
                            .unwrap_or(&m.user.username)
                            .to_owned()
                    })
                    .filter(|n| !n.is_empty())
                    .unwrap_or_else(|| "Unknown member".into());
                OverrideRow {
                    initials: initials(&name),
                    avatar: member
                        .and_then(|m| m.user.avatar_url.as_deref())
                        .map(|url| absolute_url(base, Some(url)))
                        .unwrap_or_default(),
                    name,
                    ..row
                }
            }
        })
        .collect();
    if let Some(everyone) = roles.iter().find(|r| r.is_default) {
        if !rows.iter().any(|row| row.target_id == everyone.id) {
            rows.insert(
                0,
                OverrideRow {
                    target_id: everyone.id.clone(),
                    target_type: "role".into(),
                    name: if everyone.name.is_empty() {
                        "@everyone".into()
                    } else {
                        everyone.name.clone()
                    },
                    color: everyone.color.clone().unwrap_or_default(),
                    is_default: true,
                    ..OverrideRow::default()
                },
            );
        }
    }
    let rank = |row: &OverrideRow| {
        if row.is_default {
            0
        } else if row.target_type == "role" {
            1
        } else {
            2
        }
    };
    rows.sort_by_key(rank);
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_should_keep_the_viewers_permissions() {
        let mut old: Channel = serde_json::from_value(json!({
            "id": "c", "name": "a", "channel_type": "text", "permissions": 7
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        let new: Channel = serde_json::from_value(json!({
            "id": "c", "name": "b", "channel_type": "text"
        }))
        .unwrap_or_else(|e| panic!("{e}"));
        merge_channel(&mut old, new);
        assert_eq!((old.name.as_str(), old.permissions), ("b", Some(7)));
    }
}
