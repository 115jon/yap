//! `Admin`: server settings, channel settings and the tree's management.
//! What the user may do in the shown server comes as flags (menus and the
//! tree bind them); everything the open settings show comes as one JSON
//! `state` ([`yap_core::store::view::AdminView`]); the copy and lists the
//! pages draw are `schema()` ([`yap_core::admin`], the web's tables).

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        // The shown server (ServerAccess).
        #[qproperty(bool, is_owner, cxx_name = "isOwner")]
        #[qproperty(bool, can_manage_server, cxx_name = "canManageServer")]
        #[qproperty(bool, can_manage_roles, cxx_name = "canManageRoles")]
        #[qproperty(bool, can_manage_channels, cxx_name = "canManageChannels")]
        #[qproperty(bool, can_manage_categories, cxx_name = "canManageCategories")]
        #[qproperty(bool, can_kick, cxx_name = "canKick")]
        #[qproperty(bool, can_ban, cxx_name = "canBan")]
        #[qproperty(bool, can_create_invite, cxx_name = "canCreateInvite")]
        #[qproperty(bool, can_view_audit_log, cxx_name = "canViewAuditLog")]
        /// The web app's origin (channel links).
        #[qproperty(QString, web_origin, cxx_name = "webOrigin")]
        /// The open settings as JSON: `{server, roles, invites, bans, audit,
        /// channel, overrides, members, textChannels, categories, acts,
        /// createdInvite, createdInviteLink, createdRole}`; each list is
        /// `{state, error, items}`.
        #[qproperty(QString, state)]
        type Admin = super::AdminRust;

        /// The pages' tables as JSON: `{rolePermissions, textPermissions,
        /// voicePermissions, inviteExpiry, inviteUses, quickExpiry,
        /// quickUses}`.
        #[qinvokable]
        fn schema(self: &Admin) -> QString;

        #[qinvokable]
        #[cxx_name = "sanitizeChannelName"]
        fn sanitize_channel_name(self: &Admin, name: &QString, voice: bool, last: bool) -> QString;

        /// "Expires in 2d 3h" countdown text, or empty once expired.
        #[qinvokable]
        fn remaining(self: &Admin, expires_ms: f64, now_ms: f64) -> QString;

        /// Why an icon file can't be used, or empty.
        #[qinvokable]
        #[cxx_name = "iconProblem"]
        fn icon_problem(self: &Admin, path: &QString) -> QString;

        #[qinvokable]
        #[cxx_name = "openServer"]
        fn open_server(self: &Admin, server_id: &QString);

        #[qinvokable]
        #[cxx_name = "closeServer"]
        fn close_server(self: &Admin);

        /// `roles`, `invites`, `bans` or `audit` for a server.
        #[qinvokable]
        fn load(self: &Admin, server_id: &QString, list: &QString);

        /// `{name, iconPath, removeIcon, allowPublicShares,
        /// showSourceInShares, allowShareIndexing}`.
        #[qinvokable]
        #[cxx_name = "saveServer"]
        fn save_server(self: &Admin, draft: &QString);

        #[qinvokable]
        #[cxx_name = "deleteServer"]
        fn delete_server(self: &Admin, confirm: &QString);

        /// The server menu's Leave server (members, not the owner).
        #[qinvokable]
        #[cxx_name = "leaveServer"]
        fn leave_server(self: &Admin, server_id: &QString);

        #[qinvokable]
        #[cxx_name = "setInvitesPaused"]
        fn set_invites_paused(self: &Admin, paused: bool);

        #[qinvokable]
        #[cxx_name = "createInvite"]
        fn create_invite(
            self: &Admin,
            channel_id: &QString,
            max_age: i32,
            max_uses: i32,
            temporary: bool,
        );

        #[qinvokable]
        #[cxx_name = "createQuickInvite"]
        fn create_quick_invite(self: &Admin, expires_hours: i32, max_uses: i32);

        #[qinvokable]
        #[cxx_name = "revokeInvite"]
        fn revoke_invite(self: &Admin, code: &QString);

        /// An empty `role_id` creates one.
        #[qinvokable]
        #[cxx_name = "saveRole"]
        fn save_role(
            self: &Admin,
            role_id: &QString,
            name: &QString,
            color: &QString,
            permissions: i32,
        );

        #[qinvokable]
        #[cxx_name = "deleteRole"]
        fn delete_role(self: &Admin, role_id: &QString);

        #[qinvokable]
        fn unban(self: &Admin, user_id: &QString);

        #[qinvokable]
        fn ban(self: &Admin, user_id: &QString, reason: &QString);

        #[qinvokable]
        fn kick(self: &Admin, user_id: &QString);

        #[qinvokable]
        #[cxx_name = "toggleMemberRole"]
        fn toggle_member_role(self: &Admin, user_id: &QString, role_id: &QString);

        #[qinvokable]
        #[cxx_name = "openChannel"]
        fn open_channel(self: &Admin, channel_id: &QString);

        #[qinvokable]
        #[cxx_name = "closeChannel"]
        fn close_channel(self: &Admin);

        #[qinvokable]
        #[cxx_name = "loadOverrides"]
        fn load_overrides(self: &Admin);

        /// `shares`: `inherit`, `allow` or `deny`.
        #[qinvokable]
        #[cxx_name = "saveChannel"]
        fn save_channel(self: &Admin, name: &QString, description: &QString, shares: &QString);

        /// `[{targetId, targetType, allow, deny}]`.
        #[qinvokable]
        #[cxx_name = "saveOverrides"]
        fn save_overrides(self: &Admin, drafts: &QString);

        #[qinvokable]
        #[cxx_name = "removeOverride"]
        fn remove_override(self: &Admin, target_id: &QString);

        #[qinvokable]
        #[cxx_name = "createChannel"]
        fn create_channel(self: &Admin, name: &QString, voice: bool, category_id: &QString);

        #[qinvokable]
        #[cxx_name = "createCategory"]
        fn create_category(self: &Admin, name: &QString);

        #[qinvokable]
        #[cxx_name = "deleteChannel"]
        fn delete_channel(self: &Admin, channel_id: &QString);

        #[qinvokable]
        #[cxx_name = "deleteCategory"]
        fn delete_category(self: &Admin, category_id: &QString);

        /// A category's channels in their new order (`ids`: a JSON array).
        #[qinvokable]
        fn reorder(self: &Admin, category_id: &QString, ids: &QString);

        /// A dragged channel dropped onto another row of its group.
        #[qinvokable]
        #[cxx_name = "moveChannel"]
        fn move_channel(self: &Admin, channel_id: &QString, onto: &QString);

        /// Forget an action's outcome (a dialog opening again).
        #[qinvokable]
        fn clear(self: &Admin, key: &QString);
    }

    impl cxx_qt::Initialize for Admin {}
}

use core::pin::Pin;

use cxx_qt_lib::QString;
use serde::Deserialize;
use serde_json::json;
use yap_core::admin::{
    self as rules, INVITE_EXPIRY_OPTIONS, INVITE_USES_OPTIONS, MAX_ICON_BYTES,
    QUICK_INVITE_EXPIRY_OPTIONS, QUICK_INVITE_USES_OPTIONS, ROLE_PERMISSION_GROUPS,
    TEXT_CHANNEL_PERMISSIONS, VOICE_CHANNEL_PERMISSIONS,
};
use yap_core::store::view::AdminView;
use yap_core::store::{AdminCommand, AdminList, Command, OverrideDraft, ServerDraft};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct AdminRust {
    is_owner: bool,
    can_manage_server: bool,
    can_manage_roles: bool,
    can_manage_channels: bool,
    can_manage_categories: bool,
    can_kick: bool,
    can_ban: bool,
    can_create_invite: bool,
    can_view_audit_log: bool,
    web_origin: QString,
    state: QString,
}

fn q(text: &str) -> QString {
    QString::from(text)
}

fn send(command: AdminCommand) {
    client::command(Command::Admin(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn some(value: &QString) -> Option<String> {
    Some(value.to_string()).filter(|text| !text.is_empty())
}

fn apply(mut object: Pin<&mut qobject::Admin>, view: &AdminView) {
    let access = view.access;
    object.as_mut().set_is_owner(access.owner);
    object.as_mut().set_can_manage_server(access.manage_server);
    object.as_mut().set_can_manage_roles(access.manage_roles);
    object
        .as_mut()
        .set_can_manage_channels(access.manage_channels);
    object
        .as_mut()
        .set_can_manage_categories(access.manage_categories);
    object.as_mut().set_can_kick(access.kick);
    object.as_mut().set_can_ban(access.ban);
    object.as_mut().set_can_create_invite(access.create_invite);
    object
        .as_mut()
        .set_can_view_audit_log(access.view_audit_log);
    let state = serde_json::to_string(view).unwrap_or_else(|_| "{}".into());
    object.set_state(q(&state));
}

/// The store's admin view changed.
pub fn on_view(view: AdminView) {
    if let Some(object) = gui::object::<qobject::Admin>() {
        apply(object, &view);
    }
    gui::keep_admin(view);
}

impl gui::Slot for qobject::Admin {
    crate::slot_cell!(qobject::Admin);

    fn restore(mut self: Pin<&mut Self>) {
        self.as_mut()
            .set_web_origin(q(&yap_core::config::get().api_base));
        apply(self, &gui::admin_view());
    }
}

impl cxx_qt::Initialize for qobject::Admin {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for AdminRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Admin>();
    }
}

/// Overview's draft as QML sends it.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct ServerForm {
    name: String,
    icon_path: String,
    remove_icon: bool,
    allow_public_shares: bool,
    show_source_in_shares: bool,
    allow_share_indexing: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverrideForm {
    target_id: String,
    target_type: String,
    allow: i64,
    deny: i64,
}

/// An image file a server icon can be (OverviewPage.tsx `ICON_TYPES`).
pub fn icon_content_type(path: &str) -> Option<&'static str> {
    let ext = std::path::Path::new(path)
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "avif" => Some("image/avif"),
        _ => None,
    }
}

fn options(list: &[(i64, &str)]) -> serde_json::Value {
    list.iter()
        .map(|(value, label)| json!({ "value": value, "label": label }))
        .collect()
}

impl qobject::Admin {
    pub fn schema(&self) -> QString {
        let schema = json!({
            "rolePermissions": ROLE_PERMISSION_GROUPS,
            "textPermissions": TEXT_CHANNEL_PERMISSIONS,
            "voicePermissions": VOICE_CHANNEL_PERMISSIONS,
            "inviteExpiry": options(INVITE_EXPIRY_OPTIONS),
            "inviteUses": options(INVITE_USES_OPTIONS),
            "quickExpiry": options(QUICK_INVITE_EXPIRY_OPTIONS),
            "quickUses": options(QUICK_INVITE_USES_OPTIONS),
        });
        q(&schema.to_string())
    }

    pub fn sanitize_channel_name(&self, name: &QString, voice: bool, last: bool) -> QString {
        q(&rules::sanitize_channel_name(&text(name), voice, last))
    }

    pub fn remaining(&self, expires_ms: f64, now_ms: f64) -> QString {
        rules::remaining_label(expires_ms as i64, now_ms as i64)
            .map(|left| q(&left))
            .unwrap_or_default()
    }

    pub fn icon_problem(&self, path: &QString) -> QString {
        let path = text(path);
        if icon_content_type(&path).is_none() {
            return q("Choose an image file: PNG, JPG, GIF, WebP or AVIF.");
        }
        match std::fs::metadata(&path) {
            Ok(meta) if meta.len() > MAX_ICON_BYTES => q("Choose an image under 8 MB."),
            Ok(_) => QString::default(),
            Err(_) => q("Unable to read that file. Choose another one."),
        }
    }

    pub fn open_server(&self, server_id: &QString) {
        send(AdminCommand::OpenServer(text(server_id)));
    }

    pub fn close_server(&self) {
        send(AdminCommand::CloseServer);
    }

    pub fn load(&self, server_id: &QString, list: &QString) {
        let list = match text(list).as_str() {
            "roles" => AdminList::Roles,
            "invites" => AdminList::Invites,
            "bans" => AdminList::Bans,
            "audit" => AdminList::Audit,
            _ => return,
        };
        send(AdminCommand::Load {
            server_id: text(server_id),
            list,
        });
    }

    pub fn save_server(&self, draft: &QString) {
        let form: ServerForm = serde_json::from_str(&text(draft)).unwrap_or_default();
        let icon = icon_content_type(&form.icon_path)
            .map(|content_type| (form.icon_path.clone(), content_type.to_owned()));
        send(AdminCommand::SaveServer(Box::new(ServerDraft {
            name: form.name,
            icon,
            remove_icon: form.remove_icon,
            allow_public_shares: form.allow_public_shares,
            show_source_in_shares: form.show_source_in_shares,
            allow_share_indexing: form.allow_share_indexing,
        })));
    }

    pub fn delete_server(&self, confirm: &QString) {
        send(AdminCommand::DeleteServer {
            confirm: text(confirm),
        });
    }

    pub fn leave_server(&self, server_id: &QString) {
        send(AdminCommand::LeaveServer {
            server_id: text(server_id),
        });
    }

    pub fn set_invites_paused(&self, paused: bool) {
        send(AdminCommand::SetInvitesPaused(paused));
    }

    pub fn create_invite(
        &self,
        channel_id: &QString,
        max_age: i32,
        max_uses: i32,
        temporary: bool,
    ) {
        send(AdminCommand::CreateInvite {
            channel_id: some(channel_id),
            max_age: i64::from(max_age),
            max_uses: i64::from(max_uses),
            temporary,
        });
    }

    pub fn create_quick_invite(&self, expires_hours: i32, max_uses: i32) {
        send(AdminCommand::CreateQuickInvite {
            expires_hours: i64::from(expires_hours),
            max_uses: i64::from(max_uses),
        });
    }

    pub fn revoke_invite(&self, code: &QString) {
        send(AdminCommand::RevokeInvite(text(code)));
    }

    pub fn save_role(&self, role_id: &QString, name: &QString, color: &QString, permissions: i32) {
        send(AdminCommand::SaveRole {
            role_id: some(role_id),
            name: text(name),
            color: text(color),
            permissions: i64::from(permissions),
        });
    }

    pub fn delete_role(&self, role_id: &QString) {
        send(AdminCommand::DeleteRole(text(role_id)));
    }

    pub fn unban(&self, user_id: &QString) {
        send(AdminCommand::Unban(text(user_id)));
    }

    pub fn ban(&self, user_id: &QString, reason: &QString) {
        send(AdminCommand::Ban {
            user_id: text(user_id),
            reason: text(reason),
        });
    }

    pub fn kick(&self, user_id: &QString) {
        send(AdminCommand::Kick(text(user_id)));
    }

    pub fn toggle_member_role(&self, user_id: &QString, role_id: &QString) {
        send(AdminCommand::ToggleMemberRole {
            user_id: text(user_id),
            role_id: text(role_id),
        });
    }

    pub fn open_channel(&self, channel_id: &QString) {
        send(AdminCommand::OpenChannel(text(channel_id)));
    }

    pub fn close_channel(&self) {
        send(AdminCommand::CloseChannel);
    }

    pub fn load_overrides(&self) {
        send(AdminCommand::LoadOverrides);
    }

    pub fn save_channel(&self, name: &QString, description: &QString, shares: &QString) {
        let allow_public_shares = match text(shares).as_str() {
            "allow" => Some(true),
            "deny" => Some(false),
            _ => None,
        };
        send(AdminCommand::SaveChannel {
            name: text(name),
            description: text(description),
            allow_public_shares,
        });
    }

    pub fn save_overrides(&self, drafts: &QString) {
        let forms: Vec<OverrideForm> = serde_json::from_str(&text(drafts)).unwrap_or_default();
        let drafts = forms
            .into_iter()
            .filter(|form| form.target_type == "role" || form.target_type == "user")
            .map(|form| OverrideDraft {
                target_id: form.target_id,
                target_type: form.target_type,
                allow: form.allow,
                deny: form.deny,
            })
            .collect();
        send(AdminCommand::SaveOverrides(drafts));
    }

    pub fn remove_override(&self, target_id: &QString) {
        send(AdminCommand::RemoveOverride(text(target_id)));
    }

    pub fn create_channel(&self, name: &QString, voice: bool, category_id: &QString) {
        send(AdminCommand::CreateChannel {
            name: text(name),
            voice,
            category_id: some(category_id),
        });
    }

    pub fn create_category(&self, name: &QString) {
        send(AdminCommand::CreateCategory(text(name)));
    }

    pub fn delete_channel(&self, channel_id: &QString) {
        send(AdminCommand::DeleteChannel(text(channel_id)));
    }

    pub fn delete_category(&self, category_id: &QString) {
        send(AdminCommand::DeleteCategory(text(category_id)));
    }

    pub fn reorder(&self, category_id: &QString, ids: &QString) {
        let channel_ids: Vec<String> = serde_json::from_str(&text(ids)).unwrap_or_default();
        send(AdminCommand::Reorder {
            category_id: some(category_id),
            channel_ids,
        });
    }

    pub fn move_channel(&self, channel_id: &QString, onto: &QString) {
        send(AdminCommand::MoveChannel {
            channel_id: text(channel_id),
            onto: text(onto),
        });
    }

    pub fn clear(&self, key: &QString) {
        send(AdminCommand::Clear(text(key)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn avif_should_be_a_server_icon() {
        assert_eq!(icon_content_type("a.AVIF"), Some("image/avif"));
    }

    #[test]
    fn a_text_file_should_not_be_an_icon() {
        assert_eq!(icon_content_type("notes.txt"), None);
    }
}
