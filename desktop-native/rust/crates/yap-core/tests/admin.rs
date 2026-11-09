//! Server settings, channel settings, the tree's management and member
//! moderation: request shapes (as the web sends them), the reducer's state
//! and view, the `GUILD_*` / `CHANNEL_*` dispatches, and the pure rules
//! (permissions, channel names, invite countdowns, the audit log).
//! Moderation that must not run against production (kick, ban, delete
//! server) is covered only here.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::admin::{
    self, apply_tri_state, describe_audit, has_permission, invite_uses, perm, previous_role_states,
    remaining_label, sanitize_channel_name, tri_state, AuditDetail, AuditLookup, ServerAccess,
};
use yap_core::store::view::{AdminView, ChannelTreeRow, ServerRow, TreeKind};
use yap_core::store::{
    reduce, AdminCommand, AdminList, AdminRequest, ApiRequest, ApiResponse, Command, Dispatch,
    Effect, GatewayEvent, Input, Method, OverrideDraft, ProfileAsset, RequestId, ServerDraft,
    State, ViewUpdate,
};
use yap_core::ApiError;
use yap_protocol::rest::AuditLog;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const VOICE: &str = "chan_fixture_voice";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";
const EVERYONE: &str = "role_everyone";
const MOD: &str = "role_mod";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn role(id: &str, name: &str, permissions: i64, position: i64, default: bool) -> Value {
    json!({
        "id": id, "name": name, "color": if default { Value::Null } else { json!("#ff0000") },
        "permissions": permissions, "position": position, "is_default": default,
    })
}

fn everyone() -> Value {
    role(
        EVERYONE,
        "@everyone",
        perm::VIEW_CHANNELS | perm::SEND_MESSAGES,
        0,
        true,
    )
}

fn user(id: &str, username: &str, display: &str) -> Value {
    json!({ "id": id, "username": username, "display_name": display, "avatar_url": null })
}

/// Me with `my_permissions` from a role; Alice a plain member.
fn members(my_permissions: i64) -> Value {
    json!([
        { "user": user(ME, "me", "Fixture Me"),
          "roles": [everyone(), role("role_owner", "Owner", my_permissions, 1, false)] },
        { "user": user(ALICE, "alice", "Alice"), "roles": [everyone()] },
    ])
}

fn http_error(status: u16) -> ApiError {
    ApiError::Http {
        status,
        message: "nope".into(),
    }
}

fn admin(request: AdminRequest) -> ApiRequest {
    ApiRequest::Admin(request)
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    effects: Vec<Effect>,
    tree: Vec<ChannelTreeRow>,
    servers: Vec<ServerRow>,
    view: AdminView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Channels(diff)) => {
                    assert!(diff.clone().apply(&mut self.tree), "{diff:?}");
                }
                Effect::View(ViewUpdate::Servers(diff)) => {
                    assert!(diff.clone().apply(&mut self.servers), "{diff:?}");
                }
                Effect::View(ViewUpdate::Admin(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        self.effects.extend(fx.iter().cloned());
        fx
    }

    fn command(&mut self, command: AdminCommand) -> Vec<Effect> {
        self.input(Input::User(Command::Admin(command)))
    }

    fn id_of(&self, pick: impl Fn(&ApiRequest) -> bool) -> (RequestId, ApiRequest) {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| pick(r))
            .cloned()
            .unwrap_or_else(|| panic!("no such request in {:#?}", self.asked))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let (id, _) = self.id_of(|r| r == request);
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    /// Answers the newest request of a kind.
    fn answer(
        &mut self,
        pick: impl Fn(&ApiRequest) -> bool,
        result: impl FnOnce(&ApiRequest) -> Result<ApiResponse, ApiError>,
    ) -> Vec<Effect> {
        let (id, request) = self.id_of(pick);
        let result = result(&request);
        self.input(Input::Http { id, result })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(dispatch),
        })
    }

    /// Signed in on the fixture server, its first page shown and members
    /// loaded with my role's `permissions`.
    fn online(permissions: i64) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            effects: Vec::new(),
            tree: Vec::new(),
            servers: Vec::new(),
            view: AdminView::default(),
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        client.respond(&channels, fixture("channels-response.json"));
        let page = ApiRequest::Messages {
            channel_id: TEXT.into(),
            before: None,
        };
        client.respond(&page, fixture("messages-page.json"));
        let members_request = ApiRequest::Members {
            server_id: SERVER.into(),
        };
        client.respond(&members_request, members(permissions));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn owner() -> Self {
        Self::online(perm::ADMINISTRATOR)
    }

    fn act(&self, key: &str) -> yap_core::store::Act {
        self.view.acts.get(key).cloned().unwrap_or_default()
    }

    fn channel_row(&self, id: &str) -> &ChannelTreeRow {
        self.tree
            .iter()
            .find(|r| r.kind == TreeKind::Channel && r.id == id)
            .unwrap_or_else(|| panic!("no {id} in {:#?}", self.tree))
    }

    fn tree_ids(&self) -> Vec<&str> {
        self.tree
            .iter()
            .filter(|r| r.kind == TreeKind::Channel)
            .map(|r| r.id.as_str())
            .collect()
    }
}

fn draft(name: &str) -> ServerDraft {
    ServerDraft {
        name: name.into(),
        allow_public_shares: true,
        ..ServerDraft::default()
    }
}

// Request shapes

#[test]
fn invite_create_should_leave_out_defaults() {
    let request = AdminRequest::CreateInvite {
        server_id: SERVER.into(),
        channel_id: None,
        max_age: 0,
        max_uses: 0,
        temporary: false,
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Post,
            "/api/servers/srv_fixture/invites".into(),
            json!({})
        )
    );
}

#[test]
fn invite_create_should_send_the_chosen_limits() {
    let request = AdminRequest::CreateInvite {
        server_id: SERVER.into(),
        channel_id: Some(TEXT.into()),
        max_age: 604_800,
        max_uses: 10,
        temporary: true,
    };
    assert_eq!(
        request.body(),
        json!({ "channel_id": TEXT, "max_age": 604_800, "max_uses": 10, "temporary": true })
    );
}

#[test]
fn quick_invite_should_send_its_expiry_in_seconds() {
    let request = AdminRequest::CreateQuickInvite {
        server_id: SERVER.into(),
        expires_hours: 24,
        max_uses: 0,
    };
    assert_eq!(request.body(), json!({ "max_age": 86_400 }));
}

#[test]
fn quick_invite_without_limits_should_send_nothing() {
    let request = AdminRequest::CreateQuickInvite {
        server_id: SERVER.into(),
        expires_hours: 0,
        max_uses: 0,
    };
    assert_eq!(request.body(), json!({}));
}

#[test]
fn ban_should_post_the_user_and_reason() {
    let request = AdminRequest::Ban {
        server_id: SERVER.into(),
        user_id: ALICE.into(),
        reason: Some("spam".into()),
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Post,
            "/api/servers/srv_fixture/bans".into(),
            json!({ "user_id": ALICE, "reason": "spam" })
        )
    );
}

#[test]
fn ban_without_a_reason_should_send_none() {
    let request = AdminRequest::Ban {
        server_id: SERVER.into(),
        user_id: ALICE.into(),
        reason: None,
    };
    assert_eq!(request.body(), json!({ "user_id": ALICE }));
}

#[test]
fn unban_should_delete_with_the_user_in_the_body() {
    let request = AdminRequest::Unban {
        server_id: SERVER.into(),
        user_id: ALICE.into(),
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Delete,
            "/api/servers/srv_fixture/bans".into(),
            json!({ "user_id": ALICE })
        )
    );
}

#[test]
fn kick_should_delete_the_member() {
    let request = AdminRequest::Kick {
        server_id: SERVER.into(),
        user_id: ALICE.into(),
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Delete,
            "/api/servers/srv_fixture/members/user_fixture_alice".into(),
            Value::Null
        )
    );
}

#[test]
fn member_roles_should_put_every_role_id() {
    let request = AdminRequest::SetMemberRoles {
        server_id: SERVER.into(),
        user_id: ALICE.into(),
        role_ids: vec![EVERYONE.into(), MOD.into()],
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Put,
            "/api/servers/srv_fixture/members/user_fixture_alice/roles".into(),
            json!({ "roleIds": [EVERYONE, MOD] })
        )
    );
}

#[test]
fn role_update_should_patch_name_colour_and_bits() {
    let request = AdminRequest::UpdateRole {
        server_id: SERVER.into(),
        role_id: MOD.into(),
        name: "Mods".into(),
        color: None,
        permissions: 34,
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Patch,
            "/api/servers/srv_fixture/roles/role_mod".into(),
            json!({ "name": "Mods", "color": null, "permissions": 34 })
        )
    );
}

#[test]
fn delete_server_should_delete_its_settings() {
    let request = AdminRequest::DeleteServer {
        server_id: SERVER.into(),
    };
    assert_eq!(
        (request.method(), request.path()),
        (Method::Delete, "/api/servers/srv_fixture/settings".into())
    );
}

#[test]
fn override_put_should_name_the_target_type() {
    let request = AdminRequest::PutOverride {
        channel_id: TEXT.into(),
        target_id: MOD.into(),
        target_type: "role".into(),
        allow: 512,
        deny: 256,
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Put,
            "/api/channels/chan_fixture_text/permissions/role_mod".into(),
            json!({ "target_type": "role", "allow": 512, "deny": 256 })
        )
    );
}

#[test]
fn reorder_should_number_the_group_from_zero() {
    let request = AdminRequest::Reorder {
        server_id: SERVER.into(),
        category_id: Some("cat".into()),
        channel_ids: vec!["b".into(), "a".into()],
    };
    assert_eq!(
        request.body(),
        json!({ "channels": [
            { "id": "b", "position": 0, "category_id": "cat" },
            { "id": "a", "position": 1, "category_id": "cat" },
        ]})
    );
}

#[test]
fn create_channel_should_send_its_type_and_category() {
    let request = AdminRequest::CreateChannel {
        server_id: SERVER.into(),
        name: "lounge".into(),
        voice: true,
        category_id: Some("cat".into()),
    };
    assert_eq!(
        request.body(),
        json!({ "name": "lounge", "channel_type": "voice", "category_id": "cat" })
    );
}

#[test]
fn invite_codes_should_be_escaped_in_the_path() {
    let request = AdminRequest::RevokeInvite {
        server_id: SERVER.into(),
        code: "a/b".into(),
    };
    assert_eq!(request.path(), "/api/servers/srv_fixture/invites/a%2Fb");
}

// Access

#[test]
fn an_administrator_should_see_every_page() {
    let access = ServerAccess::new(perm::ADMINISTRATOR, false);
    assert_eq!(
        access.pages(),
        ["overview", "roles", "invites", "bans", "audit"]
    );
}

#[test]
fn a_plain_member_should_see_only_the_overview() {
    let access = ServerAccess::new(perm::SEND_MESSAGES, false);
    assert_eq!(access.pages(), ["overview"]);
}

#[test]
fn manage_server_should_open_the_audit_log() {
    let access = ServerAccess::new(perm::MANAGE_SERVER, false);
    assert!(access.view_audit_log);
}

#[test]
fn members_should_give_the_shown_server_access() {
    let client = Client::online(perm::MANAGE_CHANNELS | perm::BAN_MEMBERS);
    let access = client.view.access;
    assert_eq!(
        (
            access.owner,
            access.manage_channels,
            access.ban,
            access.kick
        ),
        (true, true, true, false)
    );
}

#[test]
fn manage_channels_should_mark_every_tree_row() {
    let client = Client::online(perm::MANAGE_CHANNELS);
    assert!(client.channel_row(VOICE).manage);
}

#[test]
fn without_manage_channels_only_the_channels_own_bits_count() {
    // The text fixture's own permissions lack Manage channels.
    let client = Client::online(0);
    assert!(!client.channel_row(TEXT).manage);
}

#[test]
fn channel_rows_should_carry_their_category() {
    let client = Client::owner();
    assert_eq!(client.channel_row(VOICE).category, "cat_fixture_voice");
}

// Overview

#[test]
fn an_empty_server_name_should_be_refused() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::SaveServer(Box::new(draft("  "))));
    assert_eq!(client.act("overview.name").error, "Enter a server name.");
}

#[test]
fn saving_should_patch_the_name_only_when_it_changed() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::SaveServer(Box::new(draft(" Lab "))));
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::UpdateServer { .. })));
    assert_eq!(
        request.body(),
        json!({ "name": "Lab", "allow_public_shares": true,
                "show_source_in_shares": false, "allow_share_indexing": false })
    );
}

#[test]
fn a_saved_name_should_reach_the_rail() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::SaveServer(Box::new(draft("Lab"))));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::UpdateServer { .. })),
        |r| r.decode(json!({ "id": SERVER, "name": "Lab", "owner_id": ME })),
    );
    assert_eq!(
        (client.servers[0].name.as_str(), client.act("overview").done),
        ("Lab", 1)
    );
}

#[test]
fn a_new_icon_should_upload_first_then_save_its_url() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    let fx = client.command(AdminCommand::SaveServer(Box::new(ServerDraft {
        icon: Some(("C:/icon.png".into(), "image/png".into())),
        ..draft("Fixture Server")
    })));
    let upload = fx.iter().find_map(|e| match e {
        Effect::ProfileUpload { id, asset, .. } => Some((*id, asset.clone())),
        _ => None,
    });
    let (id, asset) = upload.expect("an upload");
    assert_eq!(asset, ProfileAsset::ServerIcon);
    client.input(Input::Http {
        id,
        result: Ok(ApiResponse::Admin(yap_core::store::AdminResponse::IconUrl(
            "/api/media/icons/new.png".into(),
        ))),
    });
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::UpdateServer { .. })));
    assert_eq!(
        request.body()["icon_url"],
        json!("/api/media/icons/new.png")
    );
}

#[test]
fn a_failed_icon_upload_should_say_so_on_the_icon_row() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    let fx = client.command(AdminCommand::SaveServer(Box::new(ServerDraft {
        icon: Some(("C:/icon.png".into(), "image/png".into())),
        ..draft("Fixture Server")
    })));
    let id = fx
        .iter()
        .find_map(|e| match e {
            Effect::ProfileUpload { id, .. } => Some(*id),
            _ => None,
        })
        .expect("an upload");
    client.input(Input::Http {
        id,
        result: Err(http_error(413)),
    });
    assert_eq!(
        (
            client.act("overview").busy,
            client.act("overview.icon").error.as_str()
        ),
        (
            false,
            "Unable to upload the server icon. The file is too large."
        )
    );
}

#[test]
fn deleting_should_need_the_exact_name() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::DeleteServer {
        confirm: "fixture server".into(),
    });
    assert_eq!(
        client.act("delete-server").error,
        "Type Fixture Server exactly to delete it."
    );
}

#[test]
fn a_deleted_server_should_leave_the_rail() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::DeleteServer {
        confirm: "Fixture Server".into(),
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::DeleteServer { .. })),
        |r| r.decode(json!({ "deleted": true })),
    );
    let ids: Vec<&str> = client.servers.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["srv_fixture_two"]);
}

#[test]
fn only_the_owner_should_delete() {
    // Alice owns the second server; an administrator still can't delete it.
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer("srv_fixture_two".into()));
    let fx = client.command(AdminCommand::DeleteServer {
        confirm: "Second Fixture".into(),
    });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Admin(AdminRequest::DeleteServer { .. }),
            ..
        }
    )));
}

#[test]
fn pausing_invites_should_flip_at_once() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::SetInvitesPaused(true));
    assert!(client.view.server.as_ref().expect("server").invites_paused);
}

#[test]
fn a_refused_pause_should_roll_back_with_the_reason() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::SetInvitesPaused(true));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::UpdateServer { .. })),
        |_| Err(http_error(403)),
    );
    assert_eq!(
        (
            client.view.server.as_ref().expect("server").invites_paused,
            client.act("pause").error.as_str()
        ),
        (false, "You don't have permission to pause invites.")
    );
}

// Invites

fn invite(code: &str, uses: i64, max: Option<i64>) -> Value {
    json!({
        "code": code, "server_id": SERVER, "inviter_id": ME, "uses": uses,
        "max_uses": max, "temporary": 0, "expires_at": "2026-11-10T00:00:00.000Z",
        "channel_name": "general",
    })
}

fn with_invites(client: &mut Client) {
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Invites,
    });
    client.respond(
        &admin(AdminRequest::Invites {
            server_id: SERVER.into(),
        }),
        json!([invite("abc", 2, Some(10)), invite("def", 1, None)]),
    );
}

#[test]
fn invite_rows_should_read_like_the_web() {
    let mut client = Client::owner();
    with_invites(&mut client);
    let rows = client.view.invites.ready().expect("invites");
    assert_eq!(
        (
            rows[0].uses.as_str(),
            rows[1].uses.as_str(),
            rows[0].channel.as_str(),
            rows[0].inviter.as_str(),
            rows[0].link.as_str()
        ),
        (
            "2 of 10 uses",
            "1 use",
            "#general",
            "Fixture Me",
            "https://api.fixture.invalid/invite/abc"
        )
    );
}

#[test]
fn a_revoked_invite_should_leave_the_list() {
    let mut client = Client::owner();
    with_invites(&mut client);
    client.command(AdminCommand::RevokeInvite("abc".into()));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::RevokeInvite { .. })),
        |r| r.decode(json!({ "deleted": true })),
    );
    let codes: Vec<&str> = client
        .view
        .invites
        .ready()
        .expect("invites")
        .iter()
        .map(|i| i.code.as_str())
        .collect();
    assert_eq!(codes, ["def"]);
}

#[test]
fn a_failed_revoke_should_keep_the_row_with_the_reason() {
    let mut client = Client::owner();
    with_invites(&mut client);
    client.command(AdminCommand::RevokeInvite("abc".into()));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::RevokeInvite { .. })),
        |_| Err(ApiError::Network("down".into())),
    );
    assert_eq!(
        (
            client.view.invites.ready().expect("invites").len(),
            client.act("revoke:abc").error.as_str()
        ),
        (
            2,
            "Unable to revoke this invite. Check your connection and try again."
        )
    );
}

#[test]
fn a_created_invite_should_be_kept_and_the_list_reloaded() {
    let mut client = Client::owner();
    with_invites(&mut client);
    client.command(AdminCommand::CreateInvite {
        channel_id: None,
        max_age: 86_400,
        max_uses: 0,
        temporary: false,
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::CreateInvite { .. })),
        |r| r.decode(json!({ "code": "xyz" })),
    );
    assert_eq!(
        (
            client.view.created_invite.as_str(),
            client.view.invites.state()
        ),
        ("xyz", "loading")
    );
}

#[test]
fn the_invite_dialog_should_invite_to_the_shown_server() {
    let mut client = Client::owner();
    client.command(AdminCommand::CreateQuickInvite {
        expires_hours: 24,
        max_uses: 0,
    });
    let (_, request) = client.id_of(|r| matches!(r, ApiRequest::Admin(_)));
    assert_eq!(request.path(), "/api/servers/srv_fixture/invites");
}

// Roles

fn with_roles(client: &mut Client) {
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Roles,
    });
    client.respond(
        &admin(AdminRequest::Roles {
            server_id: SERVER.into(),
        }),
        json!([role(MOD, "Mods", perm::KICK_MEMBERS, 1, false), everyone()]),
    );
}

#[test]
fn a_role_needs_a_name() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::SaveRole {
        role_id: None,
        name: " ".into(),
        color: String::new(),
        permissions: 0,
    });
    assert_eq!(client.act("role.name").error, "Enter a role name.");
}

#[test]
fn a_role_colour_must_be_six_hex_digits() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::SaveRole {
        role_id: Some(MOD.into()),
        name: "Mods".into(),
        color: "red".into(),
        permissions: 0,
    });
    assert_eq!(
        client.act("role.color").error,
        "Enter a colour as # and six hex digits, like #3e9b4f."
    );
}

#[test]
fn a_created_role_should_be_selected_and_the_roles_reloaded() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::SaveRole {
        role_id: None,
        name: "New role".into(),
        color: String::new(),
        permissions: perm::SEND_MESSAGES,
    });
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::CreateRole { .. })));
    assert_eq!(
        request.body(),
        json!({ "name": "New role", "color": null, "permissions": 512 })
    );
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::CreateRole { .. })),
        |r| r.decode(json!({ "id": "role_new", "name": "New role" })),
    );
    assert_eq!(
        (client.view.created_role.as_str(), client.view.roles.state()),
        ("role_new", "loading")
    );
}

#[test]
fn a_deleted_role_should_leave_the_list() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::DeleteRole(MOD.into()));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::DeleteRole { .. })),
        |r| r.decode(json!({ "deleted": true })),
    );
    let ids: Vec<&str> = client
        .view
        .roles
        .ready()
        .expect("roles")
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    assert_eq!(ids, [EVERYONE]);
}

#[test]
fn giving_a_role_should_show_at_once() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::ToggleMemberRole {
        user_id: ALICE.into(),
        role_id: MOD.into(),
    });
    let alice = client
        .view
        .members
        .iter()
        .find(|m| m.id == ALICE)
        .expect("alice");
    assert_eq!(alice.role_ids, [EVERYONE, MOD]);
}

#[test]
fn taking_a_role_should_drop_it_at_once_and_send_the_rest() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::ToggleMemberRole {
        user_id: ALICE.into(),
        role_id: MOD.into(),
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::SetMemberRoles { .. })),
        |r| r.decode(json!([everyone(), role(MOD, "Mods", perm::KICK_MEMBERS, 1, false)])),
    );
    let fx = client.command(AdminCommand::ToggleMemberRole {
        user_id: ALICE.into(),
        role_id: MOD.into(),
    });
    let sent = fx.iter().find_map(|e| match e {
        Effect::Http {
            request: ApiRequest::Admin(AdminRequest::SetMemberRoles { role_ids, .. }),
            ..
        } => Some(role_ids.clone()),
        _ => None,
    });
    assert_eq!(sent, Some(vec![EVERYONE.to_owned()]));
    let alice = client
        .view
        .members
        .iter()
        .find(|m| m.id == ALICE)
        .expect("alice");
    assert_eq!(alice.role_ids, [EVERYONE]);
}

#[test]
fn a_refused_role_change_should_roll_back() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.command(AdminCommand::ToggleMemberRole {
        user_id: ALICE.into(),
        role_id: MOD.into(),
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::SetMemberRoles { .. })),
        |_| Err(http_error(403)),
    );
    let alice = client
        .view
        .members
        .iter()
        .find(|m| m.id == ALICE)
        .expect("alice");
    assert_eq!(
        (
            alice.role_ids.clone(),
            client.act("member-roles:user_fixture_alice").error
        ),
        (
            vec![EVERYONE.to_owned()],
            "You don't have permission to update Alice's roles.".to_owned()
        )
    );
}

// Bans and moderation (never run live)

#[test]
fn a_ban_should_trim_its_reason() {
    let mut client = Client::owner();
    client.command(AdminCommand::Ban {
        user_id: ALICE.into(),
        reason: "  spam  ".into(),
    });
    let (_, request) = client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::Ban { .. })));
    assert_eq!(
        request.body(),
        json!({ "user_id": ALICE, "reason": "spam" })
    );
}

#[test]
fn a_failed_ban_should_keep_the_dialog_with_the_reason() {
    let mut client = Client::owner();
    client.command(AdminCommand::Ban {
        user_id: ALICE.into(),
        reason: String::new(),
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::Ban { .. })),
        |_| Err(http_error(403)),
    );
    assert_eq!(
        client.act("ban:user_fixture_alice").error,
        "You don't have permission to ban this member."
    );
}

#[test]
fn a_kick_should_finish_its_action() {
    let mut client = Client::owner();
    client.command(AdminCommand::Kick(ALICE.into()));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::Kick { .. })),
        |r| r.decode(json!({ "kicked": true })),
    );
    assert_eq!(client.act("kick:user_fixture_alice").done, 1);
}

#[test]
fn ban_rows_should_say_who_and_when() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Bans,
    });
    client.respond(
        &admin(AdminRequest::Bans {
            server_id: SERVER.into(),
        }),
        json!([{ "user_id": "u9", "username": "spammer", "reason": "ads",
                 "banned_by_username": "me", "created_at": "2026-11-04 10:00:00" }]),
    );
    let row = &client.view.bans.ready().expect("bans")[0];
    assert_eq!(
        (row.name.as_str(), row.meta.as_str()),
        ("spammer", "Banned by me on Nov 4, 2026")
    );
}

#[test]
fn an_unban_should_leave_the_list() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Bans,
    });
    client.respond(
        &admin(AdminRequest::Bans {
            server_id: SERVER.into(),
        }),
        json!([{ "user_id": "u9", "username": "spammer", "created_at": "" }]),
    );
    client.command(AdminCommand::Unban("u9".into()));
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::Unban { .. })),
        |r| r.decode(json!({ "unbanned": true })),
    );
    assert_eq!(client.view.bans.ready().expect("bans").len(), 0);
}

// Channel settings

fn with_overrides(client: &mut Client) {
    client.command(AdminCommand::OpenChannel(TEXT.into()));
    client.command(AdminCommand::LoadOverrides);
    client.respond(
        &admin(AdminRequest::Roles {
            server_id: SERVER.into(),
        }),
        json!([role(MOD, "Mods", 0, 1, false), everyone()]),
    );
    client.respond(
        &admin(AdminRequest::Overrides {
            channel_id: TEXT.into(),
        }),
        json!([
            { "target_id": ALICE, "target_type": "user", "allow": 0, "deny": 512 },
            { "target_id": MOD, "target_type": "role", "allow": 512, "deny": 0 },
        ]),
    );
}

#[test]
fn overrides_should_list_everyone_then_roles_then_members() {
    let mut client = Client::owner();
    with_overrides(&mut client);
    let names: Vec<(String, bool)> = client
        .view
        .overrides
        .ready()
        .expect("overrides")
        .iter()
        .map(|o| (o.name.clone(), o.saved))
        .collect();
    assert_eq!(
        names,
        [
            ("@everyone".to_owned(), false),
            ("Mods".to_owned(), true),
            ("Alice".to_owned(), true)
        ]
    );
}

#[test]
fn saving_overrides_should_put_each_and_settle_once() {
    let mut client = Client::owner();
    with_overrides(&mut client);
    client.command(AdminCommand::SaveOverrides(vec![
        OverrideDraft {
            target_id: EVERYONE.into(),
            target_type: "role".into(),
            allow: 0,
            deny: perm::SEND_MESSAGES,
        },
        OverrideDraft {
            target_id: MOD.into(),
            target_type: "role".into(),
            allow: 0,
            deny: 0,
        },
    ]));
    let puts: Vec<RequestId> = client
        .asked
        .iter()
        .filter(|(_, r)| matches!(r, ApiRequest::Admin(AdminRequest::PutOverride { .. })))
        .map(|(id, _)| *id)
        .collect();
    assert_eq!(puts.len(), 2);
    client.input(Input::Http {
        id: puts[0],
        result: Ok(ApiResponse::Admin(yap_core::store::AdminResponse::Done)),
    });
    let busy_after_one = client.act("overrides").busy;
    client.input(Input::Http {
        id: puts[1],
        result: Err(http_error(500)),
    });
    assert_eq!(
        (busy_after_one, client.act("overrides").error.as_str()),
        (
            true,
            "Unable to save the permission overrides. Try again in a moment."
        )
    );
}

#[test]
fn a_channel_save_should_send_only_what_changed() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenChannel(TEXT.into()));
    client.command(AdminCommand::SaveChannel {
        name: "General Chat".into(),
        description: String::new(),
        allow_public_shares: None,
    });
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::UpdateChannel { .. })));
    assert_eq!(request.body(), json!({ "name": "general-chat" }));
}

#[test]
fn the_channel_view_should_say_its_kind_and_age() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenChannel(TEXT.into()));
    let channel = client.view.channel.clone().expect("channel");
    assert_eq!(
        (channel.label.as_str(), channel.created.as_str()),
        ("#general", "Text channel, created November 1, 2026.")
    );
}

// The tree

#[test]
fn a_created_text_channel_should_open() {
    let mut client = Client::owner();
    client.command(AdminCommand::CreateChannel {
        name: "New Room".into(),
        voice: false,
        category_id: Some("cat_fixture_text".into()),
    });
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::CreateChannel { .. })));
    assert_eq!(request.body()["name"], json!("new-room"));
    let fx = client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::CreateChannel { .. })),
        |r| {
            r.decode(
                json!({ "id": "chan_new", "server_id": SERVER, "name": "new-room",
                             "channel_type": "text", "category_id": "cat_fixture_text",
                             "position": 1 }),
            )
        },
    );
    assert!(fx.contains(&Effect::View(ViewUpdate::ActiveChannel {
        id: "chan_new".into(),
        name: "new-room".into()
    })));
}

#[test]
fn a_blank_channel_name_should_be_refused() {
    let mut client = Client::owner();
    client.command(AdminCommand::CreateChannel {
        name: " - ".into(),
        voice: false,
        category_id: None,
    });
    assert_eq!(
        client.act("create-channel.name").error,
        "Give the channel a name."
    );
}

#[test]
fn a_category_name_should_be_upper_case() {
    let mut client = Client::owner();
    client.command(AdminCommand::CreateCategory(" games ".into()));
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::CreateCategory { .. })));
    assert_eq!(request.body(), json!({ "name": "GAMES" }));
}

#[test]
fn reordering_should_move_rows_before_the_server_answers() {
    let mut client = Client::owner();
    // A second text channel in the text category.
    client.dispatch("CHANNEL_UPDATE", json!({ "server_id": SERVER }));
    let channels = ApiRequest::Channels {
        server_id: SERVER.into(),
    };
    let mut tree = fixture("channels-response.json");
    tree["channels"]
        .as_array_mut()
        .expect("channels")
        .push(json!({
            "id": "chan_b", "server_id": SERVER, "name": "b", "channel_type": "text",
            "category_id": "cat_fixture_text", "position": 1,
        }));
    client.respond(&channels, tree);
    client.command(AdminCommand::Reorder {
        category_id: Some("cat_fixture_text".into()),
        channel_ids: vec!["chan_b".into(), TEXT.into()],
    });
    assert_eq!(client.tree_ids(), ["chan_b", TEXT, VOICE]);
}

fn with_loose_channels(client: &mut Client) {
    client.dispatch("CHANNEL_UPDATE", json!({ "server_id": SERVER }));
    let mut tree = fixture("channels-response.json");
    tree["categories"]
        .as_array_mut()
        .expect("categories")
        .push(json!({
            "id": "cat_empty", "server_id": SERVER, "name": "EMPTY", "rank": 5,
        }));
    for (id, kind) in [("loose_text", "text"), ("loose_voice", "voice")] {
        tree["channels"]
            .as_array_mut()
            .expect("channels")
            .push(json!({
                "id": id, "server_id": SERVER, "name": id, "channel_type": kind, "position": 0,
            }));
    }
    client.respond(
        &ApiRequest::Channels {
            server_id: SERVER.into(),
        },
        tree,
    );
}

#[test]
fn the_tree_should_group_like_the_web() {
    let mut client = Client::owner();
    with_loose_channels(&mut client);
    let outline: Vec<String> = client
        .tree
        .iter()
        .filter(|r| r.kind != TreeKind::Member)
        .map(|r| match r.kind {
            TreeKind::Label => format!("[{}]", r.name),
            _ => r.id.clone(),
        })
        .collect();
    assert_eq!(
        outline,
        [
            "[TEXT CHANNELS]",
            "loose_text",
            "[VOICE CHANNELS]",
            "loose_voice",
            "[Text]",
            TEXT,
            "[Voice]",
            VOICE,
            "[EMPTY]"
        ]
    );
}

#[test]
fn the_loose_groups_should_have_no_category_menu() {
    let mut client = Client::owner();
    with_loose_channels(&mut client);
    let label = client
        .tree
        .iter()
        .find(|r| r.kind == TreeKind::Label && r.id == "__uncategorized_text__")
        .expect("label");
    assert!(!label.manage);
}

#[test]
fn loose_text_and_voice_should_not_reorder_together() {
    let mut client = Client::owner();
    with_loose_channels(&mut client);
    let fx = client.command(AdminCommand::MoveChannel {
        channel_id: "loose_text".into(),
        onto: "loose_voice".into(),
    });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Admin(AdminRequest::Reorder { .. }),
            ..
        }
    )));
}

#[test]
fn dropping_onto_another_group_should_do_nothing() {
    let mut client = Client::owner();
    let fx = client.command(AdminCommand::MoveChannel {
        channel_id: TEXT.into(),
        onto: VOICE.into(),
    });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Admin(AdminRequest::Reorder { .. }),
            ..
        }
    )));
}

#[test]
fn dropping_a_channel_should_move_it_to_the_targets_place() {
    let mut client = Client::owner();
    client.dispatch("CHANNEL_UPDATE", json!({ "server_id": SERVER }));
    let mut tree = fixture("channels-response.json");
    for (id, position) in [("chan_b", 1), ("chan_c", 2)] {
        tree["channels"]
            .as_array_mut()
            .expect("channels")
            .push(json!({
                "id": id, "server_id": SERVER, "name": id, "channel_type": "text",
                "category_id": "cat_fixture_text", "position": position,
            }));
    }
    client.respond(
        &ApiRequest::Channels {
            server_id: SERVER.into(),
        },
        tree,
    );
    client.command(AdminCommand::MoveChannel {
        channel_id: "chan_c".into(),
        onto: TEXT.into(),
    });
    let (_, request) =
        client.id_of(|r| matches!(r, ApiRequest::Admin(AdminRequest::Reorder { .. })));
    let order: Vec<Value> = request.body()["channels"]
        .as_array()
        .expect("channels")
        .iter()
        .map(|c| c["id"].clone())
        .collect();
    assert_eq!(order, [json!("chan_c"), json!(TEXT), json!("chan_b")]);
}

#[test]
fn a_renamed_channel_should_keep_the_viewers_permissions() {
    let mut client = Client::owner();
    client.dispatch(
        "CHANNEL_UPDATE",
        json!({ "server_id": SERVER, "channel": {
            "id": TEXT, "server_id": SERVER, "name": "lobby", "channel_type": "text",
            "category_id": "cat_fixture_text", "position": 0 } }),
    );
    assert_eq!(
        (
            client.channel_row(TEXT).name.as_str(),
            client
                .state
                .channels(SERVER)
                .and_then(|list| list.get(TEXT))
                .and_then(|c| c.permissions)
        ),
        ("lobby", Some(1024))
    );
}

#[test]
fn an_unknown_channel_update_should_refetch_the_tree() {
    let mut client = Client::owner();
    let fx = client.dispatch(
        "CHANNEL_UPDATE",
        json!({ "server_id": SERVER, "channel": {
            "id": "chan_new", "server_id": SERVER, "name": "x", "channel_type": "text" } }),
    );
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Channels { .. },
            ..
        }
    )));
}

#[test]
fn deleting_the_open_channel_should_move_to_the_next_text_channel() {
    let mut client = Client::owner();
    client.dispatch("CHANNEL_DELETE", json!({ "id": TEXT, "server_id": SERVER }));
    assert_eq!(client.tree_ids(), [VOICE]);
}

#[test]
fn guild_update_should_rename_the_server_in_the_rail() {
    let mut client = Client::owner();
    client.dispatch(
        "GUILD_UPDATE",
        json!({ "id": SERVER, "name": "Renamed", "owner_id": ME, "icon_url": null,
                "invites_paused": 1 }),
    );
    assert_eq!(client.servers[0].name, "Renamed");
}

#[test]
fn guild_delete_should_drop_the_server() {
    let mut client = Client::owner();
    client.dispatch("GUILD_DELETE", json!({ "id": "srv_fixture_two" }));
    assert_eq!(client.servers.len(), 1);
}

#[test]
fn sign_out_should_forget_the_admin_state() {
    let mut client = Client::owner();
    with_roles(&mut client);
    client.input(Input::User(Command::SignOut));
    assert_eq!(client.view.roles.state(), "idle");
}

// Rules

#[test]
fn text_channel_names_should_be_lowercase_with_dashes() {
    assert_eq!(
        sanitize_channel_name("  Hello   World--x ", false, true),
        "hello-world-x"
    );
}

#[test]
fn voice_channel_names_should_keep_their_spaces() {
    assert_eq!(
        sanitize_channel_name(" Game Night ", true, true),
        "Game Night"
    );
}

#[test]
fn typing_a_text_channel_name_should_keep_a_trailing_dash() {
    assert_eq!(sanitize_channel_name("new ", false, false), "new-");
}

#[test]
fn remaining_should_read_in_the_largest_units() {
    let hour = 3_600_000;
    assert_eq!(
        [
            remaining_label(50 * hour + 1000, 0),
            remaining_label(19 * hour + 59 * 60_000, 0),
            remaining_label(24 * 60_000 + 9000, 0),
            remaining_label(9000, 0),
            remaining_label(0, 0),
        ],
        [
            Some("2d 2h".to_owned()),
            Some("19h 59m".to_owned()),
            Some("24m 09s".to_owned()),
            Some("9s".to_owned()),
            None
        ]
    );
}

#[test]
fn invite_uses_should_count_like_the_web() {
    assert_eq!(invite_uses(3, Some(0)), "3 uses");
}

#[test]
fn tri_state_should_round_trip() {
    let (allow, deny) = apply_tri_state(0, perm::SEND_MESSAGES, perm::SEND_MESSAGES, "allow");
    assert_eq!(
        (allow, deny, tri_state(allow, deny, perm::SEND_MESSAGES)),
        (perm::SEND_MESSAGES, 0, "allow")
    );
}

#[test]
fn administrator_should_imply_any_bit() {
    assert!(has_permission(perm::ADMINISTRATOR, perm::MOVE_MEMBERS));
}

fn log(id: &str, action: &str, target: &str, changes: Value) -> AuditLog {
    serde_json::from_value(json!({
        "id": id, "actor_id": ME, "action_type": action, "target_id": target,
        "changes": changes, "created_at": "2026-11-04 23:22:59",
        "actor": { "id": ME, "username": "me", "display_name": "Fixture Me" },
    }))
    .expect("log")
}

fn lookup() -> AuditLookup {
    let mut lookup = AuditLookup::default();
    lookup.roles.insert(MOD.into(), "Mods".into());
    lookup
}

#[test]
fn a_role_update_should_diff_against_the_older_entry() {
    let logs = [
        log(
            "2",
            "ROLE_UPDATE",
            MOD,
            json!({ "name": "Mods", "permissions": 1024 }),
        ),
        log(
            "1",
            "ROLE_CREATE",
            MOD,
            json!({ "name": "Mods", "permissions": 32 }),
        ),
    ];
    let previous = previous_role_states(&logs);
    let entry = describe_audit(&logs[0], &lookup(), previous["2"].as_ref());
    assert_eq!(
        entry.details,
        [
            AuditDetail::Perm {
                sign: "+",
                name: "Manage messages"
            },
            AuditDetail::Perm {
                sign: "−",
                name: "Kick members"
            },
        ]
    );
}

#[test]
fn a_role_update_with_no_history_should_list_what_it_allows() {
    let entry = describe_audit(
        &log("1", "ROLE_UPDATE", MOD, json!({ "permissions": 0 })),
        &lookup(),
        None,
    );
    assert_eq!(
        entry.details,
        [AuditDetail::List {
            label: "Now allows".into(),
            items: vec!["Nothing".into()]
        }]
    );
}

#[test]
fn a_deleted_channel_should_be_named_from_the_entry() {
    let entry = describe_audit(
        &log(
            "1",
            "CHANNEL_DELETE",
            "gone",
            json!({ "name": "unread-lab", "channel_type": "text" }),
        ),
        &AuditLookup::default(),
        None,
    );
    let target = entry.target.expect("target");
    assert_eq!(
        (entry.verb.as_str(), target.label.as_str(), target.named),
        ("deleted channel", "#unread-lab", true)
    );
}

#[test]
fn server_updates_should_spell_out_each_change() {
    let entry = describe_audit(
        &log(
            "1",
            "SERVER_UPDATE",
            "",
            json!({ "name": "Lab", "invites_paused": 1, "icon_url": null }),
        ),
        &AuditLookup::default(),
        None,
    );
    let lines: Vec<String> = entry
        .details
        .iter()
        .map(|d| match d {
            AuditDetail::Text { text } => text.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        lines,
        ["Removed the icon", "Paused invites", "Renamed to Lab"]
    );
}

#[test]
fn unknown_actions_should_still_read_as_a_sentence() {
    let entry = describe_audit(
        &log("1", "EMOJI_CREATE", "e1", json!({})),
        &AuditLookup::default(),
        None,
    );
    assert_eq!(entry.verb, "created emoji");
}

#[test]
fn an_unknown_member_should_be_a_generic_noun() {
    let entry = describe_audit(
        &log("1", "MEMBER_KICK", "u_gone", json!({})),
        &AuditLookup::default(),
        None,
    );
    let target = entry.target.expect("target");
    assert_eq!((target.label.as_str(), target.named), ("a member", false));
}

#[test]
fn audit_rows_should_carry_the_local_time() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Audit,
    });
    client.respond(
        &admin(AdminRequest::AuditLogs {
            server_id: SERVER.into(),
        }),
        json!([{ "id": "1", "actor_id": ME, "action_type": "CHANNEL_CREATE",
                 "target_id": TEXT, "changes": { "name": "general" },
                 "created_at": "2026-11-04 23:22:59",
                 "actor": { "id": ME, "username": "me", "display_name": "Fixture Me" } }]),
    );
    let row = &client.view.audit.ready().expect("audit")[0];
    assert_eq!(
        (
            row.actor.as_str(),
            row.when.as_str(),
            row.entry.verb.as_str()
        ),
        ("Fixture Me", "Nov 4, 2026, 11:22 PM", "created channel")
    );
}

#[test]
fn the_audit_page_should_load_names_it_may_read() {
    let mut client = Client::owner();
    client.command(AdminCommand::OpenServer(SERVER.into()));
    client.command(AdminCommand::Load {
        server_id: SERVER.into(),
        list: AdminList::Audit,
    });
    let asked_roles = client
        .asked
        .iter()
        .any(|(_, r)| matches!(r, ApiRequest::Admin(AdminRequest::Roles { .. })));
    let asked_bans = client
        .asked
        .iter()
        .any(|(_, r)| matches!(r, ApiRequest::Admin(AdminRequest::Bans { .. })));
    assert!(asked_roles && asked_bans);
}

#[test]
fn the_role_groups_should_follow_the_web_order() {
    let names: Vec<&str> = admin::ROLE_PERMISSION_GROUPS
        .iter()
        .flat_map(|group| group.items.iter().map(|item| item.name))
        .collect();
    assert_eq!(
        names,
        [
            "Administrator",
            "Manage server",
            "Manage roles",
            "Manage categories",
            "Manage channels",
            "Kick members",
            "Ban members",
            "Move members",
            "Create invites",
            "Send messages",
            "Manage messages"
        ]
    );
}

// Leave server

fn leave_request() -> ApiRequest {
    admin(AdminRequest::LeaveServer {
        server_id: "srv_fixture_two".into(),
    })
}

#[test]
fn leave_server_should_delete_our_own_membership() {
    let request = AdminRequest::LeaveServer {
        server_id: "srv_fixture_two".into(),
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Delete,
            "/api/servers/srv_fixture_two/members/@me".into(),
            Value::Null
        )
    );
}

#[test]
fn a_member_should_leave_and_the_server_should_go_from_the_rail() {
    let mut client = Client::owner();
    client.command(AdminCommand::LeaveServer {
        server_id: "srv_fixture_two".into(),
    });
    client.respond(&leave_request(), json!({ "left": true }));
    let ids: Vec<&str> = client.servers.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, [SERVER]);
}

#[test]
fn the_owner_should_not_be_offered_a_leave() {
    let mut client = Client::owner();
    let fx = client.command(AdminCommand::LeaveServer {
        server_id: SERVER.into(),
    });
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Admin(AdminRequest::LeaveServer { .. }),
            ..
        }
    )));
}

#[test]
fn an_owner_refusal_should_show_the_routes_sentence() {
    let mut client = Client::owner();
    client.command(AdminCommand::LeaveServer {
        server_id: "srv_fixture_two".into(),
    });
    client.answer(
        |r| matches!(r, ApiRequest::Admin(AdminRequest::LeaveServer { .. })),
        |_| {
            Err(ApiError::Http {
                status: 403,
                message:
                    "Server owners can't leave. Transfer ownership or delete the server instead."
                        .into(),
            })
        },
    );
    assert_eq!(
        client.act("leave-server").error,
        "Server owners can't leave. Transfer ownership or delete the server instead."
    );
    assert_eq!(client.servers.len(), 2);
}

#[test]
fn a_missing_route_should_say_leaving_is_not_available_yet() {
    for status in [404, 405] {
        let mut client = Client::owner();
        client.command(AdminCommand::LeaveServer {
            server_id: "srv_fixture_two".into(),
        });
        client.answer(
            |r| matches!(r, ApiRequest::Admin(AdminRequest::LeaveServer { .. })),
            |_| Err(http_error(status)),
        );
        assert_eq!(
            client.act("leave-server").error,
            "Leaving servers isn't available yet. Try again after Yap updates."
        );
    }
}
