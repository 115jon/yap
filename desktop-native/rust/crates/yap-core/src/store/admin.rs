//! Server and channel administration: what the UI asks for
//! ([`AdminCommand`]), the REST calls it makes ([`AdminRequest`], shaped as
//! the web sends them), their answers, and what the store keeps while the
//! settings are open ([`AdminState`]).

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::{json, Map, Value};
use yap_protocol::rest::{
    AuditLog, Ban, Channel, CreatedInvite, Invite, PermissionOverride, Role, Server,
};

use super::view::{query_value, Load};
use super::Method;
use crate::api::ApiError;

/// A server settings or channel settings action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminCommand {
    /// Server settings opened on a server: its lists load as their pages show.
    OpenServer(String),
    /// Server settings closed.
    CloseServer,
    /// Load a page's list for the open server (`roles`, `invites`, `bans`,
    /// `audit`); also the roles of the shown server for the profile card.
    Load {
        server_id: String,
        list: AdminList,
    },
    SaveServer(Box<ServerDraft>),
    /// Owner only. `confirm` must be the server's name exactly.
    DeleteServer {
        confirm: String,
    },
    SetInvitesPaused(bool),
    /// Server settings → Invites → New invite link (max age in seconds).
    CreateInvite {
        channel_id: Option<String>,
        max_age: i64,
        max_uses: i64,
        temporary: bool,
    },
    /// The "Invite people" dialog (InviteModal.tsx), for the shown server.
    CreateQuickInvite {
        expires_hours: i64,
        max_uses: i64,
    },
    RevokeInvite(String),
    /// Create (no id) or save a role.
    SaveRole {
        role_id: Option<String>,
        name: String,
        color: String,
        permissions: i64,
    },
    DeleteRole(String),
    Unban(String),
    /// Members of the shown server.
    Ban {
        user_id: String,
        reason: String,
    },
    Kick(String),
    /// Adds the role to the member, or takes it away (UserProfilePopover).
    ToggleMemberRole {
        user_id: String,
        role_id: String,
    },
    /// Channel settings opened on a channel.
    OpenChannel(String),
    CloseChannel,
    LoadOverrides,
    SaveChannel {
        name: String,
        description: String,
        /// `None`: use the server setting.
        allow_public_shares: Option<bool>,
    },
    /// Every override that differs from what the server has.
    SaveOverrides(Vec<OverrideDraft>),
    RemoveOverride(String),
    /// In the shown server.
    CreateChannel {
        name: String,
        voice: bool,
        category_id: Option<String>,
    },
    CreateCategory(String),
    DeleteChannel(String),
    DeleteCategory(String),
    /// A dragged channel's group in its new order (ChannelSidebar.tsx
    /// `handleDragEnd`): positions 0..n in `category_id`.
    Reorder {
        category_id: Option<String>,
        channel_ids: Vec<String>,
    },
    /// A channel dropped onto another of its group (`arrayMove` from the
    /// dragged row's index to the target's).
    MoveChannel {
        channel_id: String,
        onto: String,
    },
    /// Forget an action's outcome (a dialog opening again starts clean).
    Clear(String),
}

/// A list the server settings pages load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdminList {
    Roles,
    Invites,
    Bans,
    Audit,
}

/// What Server settings → Overview saves.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ServerDraft {
    pub name: String,
    /// A new icon (path, content type).
    pub icon: Option<(String, String)>,
    pub remove_icon: bool,
    pub allow_public_shares: bool,
    pub show_source_in_shares: bool,
    pub allow_share_indexing: bool,
}

/// One override as the permissions page holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverrideDraft {
    pub target_id: String,
    /// `role` or `user`.
    pub target_type: String,
    pub allow: i64,
    pub deny: i64,
}

/// The REST calls of server and channel administration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdminRequest {
    Roles {
        server_id: String,
    },
    Invites {
        server_id: String,
    },
    Bans {
        server_id: String,
    },
    AuditLogs {
        server_id: String,
    },
    Overrides {
        channel_id: String,
    },
    /// `PATCH /settings` with the changed fields (`UpdateServerSchema`).
    UpdateServer {
        server_id: String,
        patch: Value,
    },
    DeleteServer {
        server_id: String,
    },
    /// Server settings → Invites: `{channel_id?, max_age?, max_uses?,
    /// temporary?}`, zero and false left out.
    CreateInvite {
        server_id: String,
        channel_id: Option<String>,
        max_age: i64,
        max_uses: i64,
        temporary: bool,
    },
    /// InviteModal.tsx's choices. The web sends `{expires_hours, max_uses}`
    /// with `null` for none, which the server refuses (`max_uses` must be a
    /// number) or ignores (it reads `max_age`), so this sends what it reads:
    /// `{max_age?, max_uses?}` in seconds, none left out.
    CreateQuickInvite {
        server_id: String,
        expires_hours: i64,
        max_uses: i64,
    },
    RevokeInvite {
        server_id: String,
        code: String,
    },
    CreateRole {
        server_id: String,
        name: String,
        color: Option<String>,
        permissions: i64,
    },
    UpdateRole {
        server_id: String,
        role_id: String,
        name: String,
        color: Option<String>,
        permissions: i64,
    },
    DeleteRole {
        server_id: String,
        role_id: String,
    },
    Ban {
        server_id: String,
        user_id: String,
        reason: Option<String>,
    },
    Unban {
        server_id: String,
        user_id: String,
    },
    Kick {
        server_id: String,
        user_id: String,
    },
    SetMemberRoles {
        server_id: String,
        user_id: String,
        role_ids: Vec<String>,
    },
    /// `PATCH /api/channels/:id` with the changed fields.
    UpdateChannel {
        channel_id: String,
        patch: Value,
    },
    DeleteChannel {
        channel_id: String,
    },
    PutOverride {
        channel_id: String,
        target_id: String,
        target_type: String,
        allow: i64,
        deny: i64,
    },
    DeleteOverride {
        channel_id: String,
        target_id: String,
    },
    CreateChannel {
        server_id: String,
        name: String,
        voice: bool,
        category_id: Option<String>,
    },
    CreateCategory {
        server_id: String,
        name: String,
    },
    DeleteCategory {
        server_id: String,
        category_id: String,
    },
    /// `{channels: [{id, position, category_id}]}`.
    Reorder {
        server_id: String,
        category_id: Option<String>,
        channel_ids: Vec<String>,
    },
}

/// The answers the store reads.
#[derive(Debug, Clone, PartialEq)]
pub enum AdminResponse {
    Roles(Vec<Role>),
    Invites(Vec<Invite>),
    Bans(Vec<Ban>),
    Audit(Vec<AuditLog>),
    Overrides(Vec<PermissionOverride>),
    Server(Option<Box<Server>>),
    InviteCode(String),
    /// The created role's id, when the body has one.
    RoleId(Option<String>),
    Channel(Option<Box<Channel>>),
    /// An uploaded server icon's URL.
    IconUrl(String),
    Done,
}

fn lenient<T: serde::de::DeserializeOwned>(value: Value) -> Vec<T> {
    match value {
        Value::Array(items) => items
            .into_iter()
            .filter_map(|item| serde_json::from_value(item).ok())
            .collect(),
        _ => Vec::new(),
    }
}

fn servers(server_id: &str) -> String {
    format!("/api/servers/{}", query_value(server_id))
}

fn channels(channel_id: &str) -> String {
    format!("/api/channels/{}", query_value(channel_id))
}

impl AdminRequest {
    pub fn method(&self) -> Method {
        match self {
            Self::Roles { .. }
            | Self::Invites { .. }
            | Self::Bans { .. }
            | Self::AuditLogs { .. }
            | Self::Overrides { .. } => Method::Get,
            Self::CreateInvite { .. }
            | Self::CreateQuickInvite { .. }
            | Self::CreateRole { .. }
            | Self::Ban { .. }
            | Self::CreateChannel { .. }
            | Self::CreateCategory { .. } => Method::Post,
            Self::UpdateServer { .. }
            | Self::UpdateRole { .. }
            | Self::UpdateChannel { .. }
            | Self::Reorder { .. } => Method::Patch,
            Self::SetMemberRoles { .. } | Self::PutOverride { .. } => Method::Put,
            Self::DeleteServer { .. }
            | Self::RevokeInvite { .. }
            | Self::DeleteRole { .. }
            | Self::Unban { .. }
            | Self::Kick { .. }
            | Self::DeleteChannel { .. }
            | Self::DeleteOverride { .. }
            | Self::DeleteCategory { .. } => Method::Delete,
        }
    }

    pub fn path(&self) -> String {
        match self {
            Self::Roles { server_id } | Self::CreateRole { server_id, .. } => {
                format!("{}/roles", servers(server_id))
            }
            Self::UpdateRole {
                server_id, role_id, ..
            }
            | Self::DeleteRole { server_id, role_id } => {
                format!("{}/roles/{}", servers(server_id), query_value(role_id))
            }
            Self::Invites { server_id }
            | Self::CreateInvite { server_id, .. }
            | Self::CreateQuickInvite { server_id, .. } => {
                format!("{}/invites", servers(server_id))
            }
            Self::RevokeInvite { server_id, code } => {
                format!("{}/invites/{}", servers(server_id), query_value(code))
            }
            Self::Bans { server_id }
            | Self::Ban { server_id, .. }
            | Self::Unban { server_id, .. } => {
                format!("{}/bans", servers(server_id))
            }
            Self::AuditLogs { server_id } => format!("{}/audit-logs", servers(server_id)),
            Self::UpdateServer { server_id, .. } | Self::DeleteServer { server_id } => {
                format!("{}/settings", servers(server_id))
            }
            Self::Kick { server_id, user_id } => {
                format!("{}/members/{}", servers(server_id), query_value(user_id))
            }
            Self::SetMemberRoles {
                server_id, user_id, ..
            } => format!(
                "{}/members/{}/roles",
                servers(server_id),
                query_value(user_id)
            ),
            Self::CreateChannel { server_id, .. } => format!("{}/channels", servers(server_id)),
            Self::Reorder { server_id, .. } => format!("{}/channels/reorder", servers(server_id)),
            Self::CreateCategory { server_id, .. } => {
                format!("{}/categories", servers(server_id))
            }
            Self::DeleteCategory {
                server_id,
                category_id,
            } => format!(
                "{}/categories/{}",
                servers(server_id),
                query_value(category_id)
            ),
            Self::Overrides { channel_id } => format!("{}/permissions", channels(channel_id)),
            Self::PutOverride {
                channel_id,
                target_id,
                ..
            }
            | Self::DeleteOverride {
                channel_id,
                target_id,
            } => format!(
                "{}/permissions/{}",
                channels(channel_id),
                query_value(target_id)
            ),
            Self::UpdateChannel { channel_id, .. } | Self::DeleteChannel { channel_id } => {
                channels(channel_id)
            }
        }
    }

    /// The JSON body (`Null` for none).
    pub fn body(&self) -> Value {
        match self {
            Self::UpdateServer { patch, .. } | Self::UpdateChannel { patch, .. } => patch.clone(),
            Self::CreateInvite {
                channel_id,
                max_age,
                max_uses,
                temporary,
                ..
            } => {
                let mut body = Map::new();
                if let Some(channel_id) = channel_id {
                    body.insert("channel_id".into(), json!(channel_id));
                }
                if *max_age > 0 {
                    body.insert("max_age".into(), json!(max_age));
                }
                if *max_uses > 0 {
                    body.insert("max_uses".into(), json!(max_uses));
                }
                if *temporary {
                    body.insert("temporary".into(), json!(true));
                }
                Value::Object(body)
            }
            Self::CreateQuickInvite {
                expires_hours,
                max_uses,
                ..
            } => {
                let mut body = Map::new();
                if *expires_hours > 0 {
                    body.insert("max_age".into(), json!(expires_hours * 3600));
                }
                if *max_uses > 0 {
                    body.insert("max_uses".into(), json!(max_uses));
                }
                Value::Object(body)
            }
            Self::CreateRole {
                name,
                color,
                permissions,
                ..
            }
            | Self::UpdateRole {
                name,
                color,
                permissions,
                ..
            } => json!({ "name": name, "color": color, "permissions": permissions }),
            Self::Ban {
                user_id, reason, ..
            } => match reason {
                Some(reason) => json!({ "user_id": user_id, "reason": reason }),
                None => json!({ "user_id": user_id }),
            },
            Self::Unban { user_id, .. } => json!({ "user_id": user_id }),
            Self::SetMemberRoles { role_ids, .. } => json!({ "roleIds": role_ids }),
            Self::PutOverride {
                target_type,
                allow,
                deny,
                ..
            } => json!({ "target_type": target_type, "allow": allow, "deny": deny }),
            Self::CreateChannel {
                name,
                voice,
                category_id,
                ..
            } => {
                let mut body = json!({
                    "name": name,
                    "channel_type": if *voice { "voice" } else { "text" },
                });
                if let Some(category_id) = category_id {
                    body["category_id"] = json!(category_id);
                }
                body
            }
            Self::CreateCategory { name, .. } => json!({ "name": name }),
            Self::Reorder {
                category_id,
                channel_ids,
                ..
            } => json!({
                "channels": channel_ids
                    .iter()
                    .enumerate()
                    .map(|(position, id)| json!({
                        "id": id,
                        "position": position,
                        "category_id": category_id,
                    }))
                    .collect::<Vec<_>>(),
            }),
            Self::Roles { .. }
            | Self::Invites { .. }
            | Self::Bans { .. }
            | Self::AuditLogs { .. }
            | Self::Overrides { .. }
            | Self::DeleteServer { .. }
            | Self::RevokeInvite { .. }
            | Self::DeleteRole { .. }
            | Self::Kick { .. }
            | Self::DeleteChannel { .. }
            | Self::DeleteOverride { .. }
            | Self::DeleteCategory { .. } => Value::Null,
        }
    }

    pub fn decode(&self, body: Value) -> Result<AdminResponse, ApiError> {
        Ok(match self {
            Self::Roles { .. } | Self::SetMemberRoles { .. } => AdminResponse::Roles(lenient(body)),
            Self::Invites { .. } => AdminResponse::Invites(lenient(body)),
            Self::Bans { .. } => AdminResponse::Bans(lenient(body)),
            Self::AuditLogs { .. } => AdminResponse::Audit(lenient(body)),
            Self::Overrides { .. } => AdminResponse::Overrides(lenient(body)),
            Self::UpdateServer { .. } => {
                AdminResponse::Server(serde_json::from_value(body).ok().map(Box::new))
            }
            Self::CreateInvite { .. } | Self::CreateQuickInvite { .. } => {
                let created: CreatedInvite = serde_json::from_value(body)
                    .map_err(|e| ApiError::Decode(format!("invite: {e}")))?;
                AdminResponse::InviteCode(created.code)
            }
            Self::CreateRole { .. } => {
                AdminResponse::RoleId(body.get("id").and_then(Value::as_str).map(str::to_owned))
            }
            Self::UpdateChannel { .. } | Self::CreateChannel { .. } => {
                AdminResponse::Channel(serde_json::from_value(body).ok().map(Box::new))
            }
            Self::DeleteServer { .. }
            | Self::RevokeInvite { .. }
            | Self::UpdateRole { .. }
            | Self::DeleteRole { .. }
            | Self::Ban { .. }
            | Self::Unban { .. }
            | Self::Kick { .. }
            | Self::DeleteChannel { .. }
            | Self::PutOverride { .. }
            | Self::DeleteOverride { .. }
            | Self::CreateCategory { .. }
            | Self::DeleteCategory { .. }
            | Self::Reorder { .. } => AdminResponse::Done,
        })
    }
}

/// One action's outcome, as the UI shows it: busy, the error copy, and a
/// count of successes (a dialog closes, "Changes saved" shows).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Act {
    pub busy: bool,
    pub error: String,
    pub done: u64,
}

/// What the server and channel settings loaded, and how their actions go.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdminState {
    /// Server settings are open on this server.
    pub open_server: Option<String>,
    /// The server the lists below belong to.
    pub server_id: Option<String>,
    pub roles: Load<Vec<Role>>,
    pub invites: Load<Vec<Invite>>,
    pub bans: Load<Vec<Ban>>,
    pub audit: Load<Vec<AuditLog>>,
    /// Channel settings are open on this channel, with its overrides.
    pub channel_id: Option<String>,
    pub overrides: Load<Vec<PermissionOverride>>,
    /// By action key (`overview`, `role`, `revoke:<code>`, …).
    pub acts: BTreeMap<String, Act>,
    /// The code the last invite create made.
    pub created_invite: String,
    /// The role the last create made.
    pub created_role: String,
    /// Override saves still out, and the first failure.
    pub override_batch: Option<(usize, Option<String>)>,
}

impl AdminState {
    pub fn act(&self, key: &str) -> Act {
        self.acts.get(key).cloned().unwrap_or_default()
    }
}

/// What a pending admin request was for.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum AdminOp {
    /// A list for a server (or the open channel's overrides).
    List {
        server_id: String,
        list: AdminList,
    },
    Overrides {
        channel_id: String,
    },
    /// A step of an action: its key and what to do with the answer.
    Act {
        key: String,
        then: AdminThen,
    },
}

/// How an action's answer lands.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum AdminThen {
    /// The icon is up: save the rest with its URL.
    IconUploaded {
        server_id: String,
        patch: Value,
    },
    ServerSaved {
        server_id: String,
        patch: Value,
    },
    ServerDeleted {
        server_id: String,
    },
    InvitesPaused {
        server_id: String,
        before: Option<bool>,
    },
    InviteCreated {
        server_id: String,
    },
    InviteRevoked {
        code: String,
    },
    RoleSaved {
        server_id: String,
        created: bool,
    },
    RoleDeleted {
        server_id: String,
        role_id: String,
    },
    Unbanned {
        user_id: String,
    },
    /// Banned or kicked: the dispatch removes them.
    Removed,
    MemberRoles {
        server_id: String,
        user_id: String,
        before: Vec<Role>,
    },
    ChannelSaved {
        channel_id: String,
    },
    OverrideSaved {
        channel_id: String,
        draft: OverrideDraft,
    },
    OverrideRemoved {
        channel_id: String,
        target_id: String,
    },
    ChannelCreated {
        server_id: String,
        voice: bool,
    },
    CategoryCreated {
        server_id: String,
    },
    ChannelDeleted {
        channel_id: String,
    },
    CategoryDeleted {
        server_id: String,
    },
    Reordered {
        server_id: String,
    },
}
