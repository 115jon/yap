//! Reducer tests for Home and DMs, presence and status, the member panel
//! and the user popout. Inputs go into `reduce`; the tests assert on the
//! rows and effects that come out.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::{DmRow, MemberListRow, ProfileView};
use yap_core::store::{
    reduce, ApiRequest, CacheWrite, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, Row,
    State, ViewDiff, ViewUpdate,
};
use yap_protocol::GatewayOp;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const ME: &str = "user_fixture_me";
const DM: &str = "dm_fixture_alice";
const ALICE: &str = "user_fixture_alice";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn user(id: &str, username: &str, display: Option<&str>, status: &str) -> Value {
    json!({
        "id": id, "username": username, "display_name": display,
        "avatar_url": null, "status": status, "custom_status": null,
    })
}

fn dm(id: &str, recipient: Value) -> Value {
    json!({ "id": id, "channel_type": "dm", "name": "x", "created_at": null, "recipient": recipient })
}

fn role(id: &str, name: &str, position: i64, color: &str) -> Value {
    json!({ "id": id, "name": name, "position": position, "color": color, "is_default": false })
}

fn members() -> Value {
    json!([
        { "user": user(ME, "me", Some("Fixture Me"), "online"), "roles": [] },
        { "user": user(ALICE, "alice", Some("Alice"), "online"),
          "roles": [role("r_mod", "Mods", 5, "#ff0000")] },
        { "user": user("user_fixture_bob", "bob", None, "online"), "roles": [] },
        { "user": user("user_fixture_cy", "Cy", None, "offline"),
          "roles": [role("r_mod", "Mods", 5, "#ff0000")] },
    ])
}

fn decode(event: &str, data: Value) -> Dispatch {
    Dispatch::decode(event, data).expect("decodes")
}

struct Client {
    state: State,
    /// The newest id each kind of request went out with.
    asked: Vec<(RequestId, ApiRequest)>,
    rows: Vec<MemberListRow>,
    dms: Vec<DmRow>,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Members(diff)) => {
                    assert!(diff.clone().apply(&mut self.rows), "{diff:?}");
                }
                Effect::View(ViewUpdate::Dms(diff)) => {
                    assert!(diff.clone().apply(&mut self.dms), "{diff:?}");
                }
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn id_of(&self, wanted: &ApiRequest) -> RequestId {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == wanted)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {wanted:?} in {:#?}", self.asked))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self.id_of(request);
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(decode(event, data)),
        })
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// Signed in on the fixture server, the first page shown, members and
    /// DMs loaded (Alice's DM), the gateway up and its presence list in.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            rows: Vec::new(),
            dms: Vec::new(),
        };
        client.command(Command::UseToken("tok_fixture".into()));
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
        client.respond(&members_request, members());
        client.respond(
            &ApiRequest::Dms,
            json!([dm(DM, user(ALICE, "alice", Some("Alice"), "online"))]),
        );
        client.gateway(GatewayEvent::Ready);
        // The subscribe's presence list: everyone but Cy is connected.
        client.dispatch(
            "PRESENCE_LIST",
            json!({ "users": [
                { "user_id": ME, "status": "online" },
                { "user_id": ALICE, "status": "online" },
                { "user_id": "user_fixture_bob", "status": "online" },
            ]}),
        );
        client
    }

    /// A fresh gateway session reloads the DMs; answer with `list`.
    fn reload_dms(&mut self, list: Value) {
        self.gateway(GatewayEvent::Closed);
        self.gateway(GatewayEvent::Ready);
        self.respond(&ApiRequest::Dms, list);
    }

    fn member(&self, id: &str) -> &MemberListRow {
        self.rows
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no {id} in {:#?}", self.rows))
    }

    /// The panel as `label:count` and member ids, top to bottom.
    fn outline(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|r| match r.user_id.as_str() {
                "" => format!("{}:{}", r.name, r.count),
                id => id.to_owned(),
            })
            .collect()
    }
}

fn views(fx: &[Effect]) -> Vec<&ViewUpdate> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(update) => Some(update),
            _ => None,
        })
        .collect()
}

fn frames(fx: &[Effect]) -> Vec<(i64, Value)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::GatewaySend(frame) => Some((frame.op, frame.d.clone())),
            _ => None,
        })
        .collect()
}

fn op(code: GatewayOp) -> i64 {
    i64::from(code.code())
}

fn profile(fx: &[Effect]) -> Option<ProfileView> {
    views(fx).into_iter().rev().find_map(|v| match v {
        ViewUpdate::Profile(view) => Some((**view).clone()),
        _ => None,
    })
}

// Member panel

#[test]
fn member_panel_should_group_online_members_by_their_highest_role_then_offline() {
    let client = Client::online();
    assert_eq!(
        client.outline(),
        [
            "Mods:1",
            ALICE,
            "Online:2",
            "user_fixture_bob",
            ME,
            "Offline:1",
            "user_fixture_cy",
        ]
    );
}

#[test]
fn member_panel_should_colour_online_names_with_their_highest_role() {
    let client = Client::online();
    assert_eq!(client.member(ALICE).color, "#ff0000");
}

#[test]
fn member_panel_should_not_colour_offline_names() {
    let client = Client::online();
    assert_eq!(client.member("user_fixture_cy").color, "");
}

#[test]
fn member_panel_should_mark_the_server_owner() {
    // servers-list.json: the fixture server is owned by the fixture user.
    let client = Client::online();
    assert!(client.member(ME).owner);
}

#[test]
fn presence_list_should_move_unlisted_members_offline_without_a_reset() {
    let mut client = Client::online();
    let fx = client.dispatch(
        "PRESENCE_LIST",
        json!({ "users": [
            { "user_id": ME, "status": "online" },
            { "user_id": ALICE, "status": "dnd" },
        ]}),
    );
    let resets = views(&fx)
        .iter()
        .filter(|v| matches!(v, ViewUpdate::Members(ViewDiff::Reset(_))))
        .count();
    assert_eq!(resets, 0);
    assert_eq!(
        client.outline(),
        [
            "Mods:1",
            ALICE,
            "Online:1",
            ME,
            // Offline sorts by role too.
            "Offline:2",
            "user_fixture_cy",
            "user_fixture_bob",
        ]
    );
}

#[test]
fn presence_update_should_light_the_status_and_move_the_member() {
    let mut client = Client::online();
    client.dispatch("PRESENCE_LIST", json!({ "users": [{ "user_id": ME }] }));
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": "user_fixture_cy", "status": "idle" }),
    );
    let cy = client.member("user_fixture_cy");
    assert_eq!((cy.presence.as_str(), cy.online), ("idle", true));
}

#[test]
fn presence_update_without_a_custom_status_should_keep_the_known_one() {
    let mut client = Client::online();
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "custom_status": "lunch" }),
    );
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "idle" }),
    );
    assert_eq!(client.member(ALICE).custom_status, "lunch");
}

#[test]
fn presence_update_with_a_null_custom_status_should_clear_it() {
    let mut client = Client::online();
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "custom_status": "lunch" }),
    );
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "custom_status": null }),
    );
    assert_eq!(client.member(ALICE).custom_status, "");
}

#[test]
fn member_add_should_list_the_newcomer_online() {
    let mut client = Client::online();
    client.dispatch(
        "GUILD_MEMBER_ADD",
        json!({ "server_id": SERVER, "user": user("user_fixture_dee", "dee", None, "online"), "roles": [] }),
    );
    assert!(client.member("user_fixture_dee").online);
}

#[test]
fn member_remove_should_drop_the_row() {
    let mut client = Client::online();
    client.dispatch(
        "GUILD_MEMBER_REMOVE",
        json!({ "server_id": SERVER, "user_id": "user_fixture_bob" }),
    );
    assert!(client.rows.iter().all(|r| r.id != "user_fixture_bob"));
}

#[test]
fn member_update_should_regroup_by_the_new_roles() {
    let mut client = Client::online();
    client.dispatch(
        "GUILD_MEMBER_UPDATE",
        json!({ "server_id": SERVER, "user_id": "user_fixture_bob",
                "roles": [role("r_dj", "DJs", 2, "#00ff00")] }),
    );
    assert_eq!(
        client.outline()[..6],
        [
            "Mods:1".to_owned(),
            ALICE.to_owned(),
            "DJs:1".to_owned(),
            "user_fixture_bob".to_owned(),
            "Online:1".to_owned(),
            ME.to_owned(),
        ]
    );
}

#[test]
fn member_update_should_take_roles_sent_with_a_numeric_default_flag() {
    let mut client = Client::online();
    client.dispatch(
        "GUILD_MEMBER_UPDATE",
        json!({ "server_id": SERVER, "user_id": "user_fixture_bob",
                "roles": [{ "id": "r_all", "name": "everyone", "position": 0, "is_default": 1 }] }),
    );
    assert!(client.outline().contains(&"Online:2".to_owned()));
}

#[test]
fn typing_should_show_on_the_member_row() {
    let mut client = Client::online();
    client.dispatch(
        "TYPING_START",
        json!({ "channel_id": TEXT, "user_id": ALICE }),
    );
    assert!(client.member(ALICE).typing);
}

#[test]
fn profile_update_should_rename_the_member_and_the_dm() {
    let mut client = Client::online();
    client.dispatch(
        "USER_PROFILE_UPDATE",
        json!({ "user_id": ALICE, "display_name": "Alice B", "avatar_url": "/api/avatars/a.png" }),
    );
    assert_eq!(
        (
            client.member(ALICE).name.as_str(),
            client.dms[0].name.as_str(),
            client.dms[0].avatar.as_str(),
        ),
        (
            "Alice B",
            "Alice B",
            "https://api.fixture.invalid/api/avatars/a.png"
        )
    );
}

// Home and DMs

#[test]
fn dm_list_should_show_the_recipient_with_presence() {
    let client = Client::online();
    let row = &client.dms[0];
    assert_eq!(
        (
            row.name.as_str(),
            row.presence.as_str(),
            row.user_id.as_str()
        ),
        ("Alice", "online", ALICE)
    );
}

#[test]
fn select_home_should_show_home_with_no_conversation_open() {
    let mut client = Client::online();
    let fx = client.command(Command::SelectHome);
    assert!(views(&fx).contains(&&ViewUpdate::Home(true)));
    assert_eq!(client.state.active_channel(), None);
}

#[test]
fn select_home_should_unsubscribe_the_open_channel() {
    let mut client = Client::online();
    let fx = client.command(Command::SelectHome);
    assert_eq!(
        frames(&fx),
        [(
            op(GatewayOp::ChannelUnsubscribe),
            json!({ "channel_id": TEXT })
        )]
    );
}

#[test]
fn opening_a_dm_should_load_it_like_a_channel() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    client.id_of(&ApiRequest::Messages {
        channel_id: DM.into(),
        before: None,
    });
    assert_eq!(client.state.active_channel(), Some(DM));
}

#[test]
fn opening_a_dm_should_name_the_recipient_in_the_header() {
    let mut client = Client::online();
    let fx = client.command(Command::SelectChannel(DM.into()));
    let shown = views(&fx);
    assert!(shown.contains(&&ViewUpdate::ActiveChannel {
        id: DM.into(),
        name: "Alice".into()
    }));
    assert!(shown.iter().any(
        |v| matches!(v, ViewUpdate::ActiveDm(dm) if dm.username == "alice" && dm.user_id == ALICE)
    ));
}

#[test]
fn a_dm_should_list_the_two_of_you_in_the_member_panel() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    assert_eq!(client.outline(), ["Online:2", ALICE, ME]);
}

#[test]
fn mention_suggestions_in_a_dm_should_offer_the_recipient() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    let fx = client.command(Command::MentionQuery(Some("ali".into())));
    let hit = views(&fx).into_iter().find_map(|v| match v {
        ViewUpdate::Mentions(ViewDiff::Insert { rows, .. }) => Some(rows[0].id.clone()),
        _ => None,
    });
    assert_eq!(hit.as_deref(), Some(ALICE));
}

#[test]
fn a_new_dm_message_should_mark_the_dm_unread_and_put_it_on_the_rail() {
    let mut client = Client::online();
    client.dispatch(
        "MESSAGE_CREATE",
        json!({ "id": "m_dm", "channel_id": DM, "author_id": ALICE,
                "content": "hi", "created_at": "2026-11-04T13:00:00.000Z" }),
    );
    let row = &client.dms[0];
    assert_eq!((row.unread, row.rail, row.rail_count), (true, 0, 1));
}

#[test]
fn dm_notifications_should_count_on_the_dm_row() {
    let mut client = Client::online();
    client.dispatch(
        "NOTIFICATION_CREATE",
        json!({ "id": "n1", "type": "dm", "channel_id": DM, "message_id": "m_dm" }),
    );
    assert_eq!(client.dms[0].mentions, 1);
}

#[test]
fn every_unread_dm_should_count_for_the_rail() {
    let mut client = Client::online();
    let list: Vec<Value> = (0..5)
        .map(|i| {
            dm(
                &format!("dm_{i}"),
                user(&format!("u_{i}"), "u", None, "online"),
            )
        })
        .collect();
    client.reload_dms(Value::Array(list));
    let mut unread = 0;
    for i in 0..4 {
        let fx = client.dispatch(
            "MESSAGE_CREATE",
            json!({ "id": format!("m_{i}"), "channel_id": format!("dm_{i}"),
                    "author_id": format!("u_{i}"), "content": "hi",
                    "created_at": "2026-11-04T13:00:00.000Z" }),
        );
        for v in views(&fx) {
            if let ViewUpdate::DmUnread(n) = v {
                unread = *n;
            }
        }
    }
    assert_eq!(unread, 4);
}

#[test]
fn dm_channel_create_should_put_the_dm_on_top() {
    let mut client = Client::online();
    client.dispatch(
        "DM_CHANNEL_CREATE",
        dm("dm_new", user("user_fixture_bob", "bob", None, "online")),
    );
    assert_eq!(client.dms[0].id, "dm_new");
}

#[test]
fn a_message_in_an_unknown_channel_should_reload_the_dms() {
    let mut client = Client::online();
    let fx = client.dispatch(
        "MESSAGE_CREATE",
        json!({ "id": "m_x", "channel_id": "dm_unknown", "author_id": "u_x",
                "content": "hi", "created_at": "2026-11-04T13:00:00.000Z" }),
    );
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::Dms,
            ..
        }
    )));
}

#[test]
fn open_dm_with_a_known_recipient_should_open_it_without_a_request() {
    let mut client = Client::online();
    let fx = client.command(Command::OpenDm(ALICE.into()));
    assert!(!fx.iter().any(|e| matches!(
        e,
        Effect::Http {
            request: ApiRequest::OpenDm { .. },
            ..
        }
    )));
    assert_eq!(client.state.active_channel(), Some(DM));
}

#[test]
fn open_dm_with_someone_new_should_create_and_open_it() {
    let mut client = Client::online();
    let request = ApiRequest::OpenDm {
        user_id: "user_fixture_bob".into(),
    };
    client.command(Command::OpenDm("user_fixture_bob".into()));
    client.respond(
        &request,
        dm("dm_bob", user("user_fixture_bob", "bob", None, "online")),
    );
    assert_eq!(
        (client.state.active_channel(), client.dms[0].id.as_str()),
        (Some("dm_bob"), "dm_bob")
    );
}

#[test]
fn open_dm_should_send_the_target_user() {
    let request = ApiRequest::OpenDm {
        user_id: "u1".into(),
    };
    assert_eq!(
        (request.path(), request.body()),
        ("/api/dms".to_owned(), json!({ "target_user_id": "u1" }))
    );
}

#[test]
fn select_server_from_home_should_leave_home() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    let fx = client.command(Command::SelectServer(SERVER.into()));
    assert!(views(&fx).contains(&&ViewUpdate::Home(false)));
}

#[test]
fn a_tree_that_answers_in_home_should_not_leave_the_dm() {
    let mut client = Client::online();
    client.command(Command::SelectServer(SERVER.into()));
    client.command(Command::SelectChannel(DM.into()));
    let channels = ApiRequest::Channels {
        server_id: SERVER.into(),
    };
    client.respond(&channels, fixture("channels-response.json"));
    assert_eq!(client.state.active_channel(), Some(DM));
}

// Status

#[test]
fn set_status_should_store_broadcast_and_show_it() {
    let mut client = Client::online();
    let fx = client.command(Command::SetStatus {
        status: "dnd".into(),
        custom_status: Some(" focus ".into()),
    });
    let shown = views(&fx).into_iter().find_map(|v| match v {
        ViewUpdate::Me(me) => Some((me.status.clone(), me.custom_status.clone())),
        _ => None,
    });
    assert_eq!(shown, Some(("dnd".into(), "focus".into())));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::Http { request: ApiRequest::SetPresence { status, custom_status }, .. }
            if status == "dnd" && custom_status.as_deref() == Some("focus")
    )));
    assert_eq!(
        frames(&fx),
        [(op(GatewayOp::PresenceUpdate), json!({ "status": "dnd" }))]
    );
}

#[test]
fn set_status_should_persist_me() {
    let mut client = Client::online();
    let fx = client.command(Command::SetStatus {
        status: "idle".into(),
        custom_status: None,
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::CacheWrite { write: CacheWrite::Me(me), .. } if me.status.as_deref() == Some("idle")
    )));
}

#[test]
fn set_presence_body_should_send_null_to_clear_the_custom_status() {
    let request = ApiRequest::SetPresence {
        status: "online".into(),
        custom_status: None,
    };
    assert_eq!(
        request.body(),
        json!({ "status": "online", "custom_status": null })
    );
}

#[test]
fn set_status_should_ignore_unknown_statuses() {
    let mut client = Client::online();
    let fx = client.command(Command::SetStatus {
        status: "away".into(),
        custom_status: None,
    });
    assert!(fx.is_empty());
}

#[test]
fn ready_should_send_a_status_that_is_not_online() {
    let mut client = Client::online();
    client.command(Command::SetStatus {
        status: "offline".into(),
        custom_status: None,
    });
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Ready);
    assert_eq!(
        frames(&fx).first(),
        Some(&(
            op(GatewayOp::PresenceUpdate),
            json!({ "status": "offline" })
        ))
    );
}

#[test]
fn invisible_should_show_me_offline_in_the_member_panel() {
    let mut client = Client::online();
    client.dispatch("PRESENCE_LIST", json!({ "users": [{ "user_id": ME }] }));
    client.command(Command::SetStatus {
        status: "offline".into(),
        custom_status: None,
    });
    assert!(!client.member(ME).online);
}

#[test]
fn my_status_from_another_device_should_update_the_account_row() {
    let mut client = Client::online();
    let fx = client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ME, "status": "idle" }),
    );
    assert!(views(&fx)
        .iter()
        .any(|v| matches!(v, ViewUpdate::Me(me) if me.status == "idle")));
}

// The popout

#[test]
fn show_profile_should_fetch_the_full_profile() {
    let mut client = Client::online();
    client.command(Command::ShowProfile(Some(ALICE.into())));
    client.id_of(&ApiRequest::Profile {
        user_id: ALICE.into(),
    });
}

#[test]
fn show_profile_should_show_what_the_lists_know_with_roles_at_once() {
    let mut client = Client::online();
    let fx = client.command(Command::ShowProfile(Some(ALICE.into())));
    let view = profile(&fx).expect("profile");
    assert_eq!(
        (view.name.as_str(), view.role_names, view.role_colors),
        ("Alice", vec!["Mods".to_owned()], vec!["#ff0000".to_owned()])
    );
}

#[test]
fn the_profile_answer_should_add_the_bio_and_banner() {
    let mut client = Client::online();
    client.command(Command::ShowProfile(Some(ALICE.into())));
    let mut full = user(ALICE, "alice", Some("Alice"), "online");
    full["bio"] = json!(" hi there ");
    full["profile_banner_color"] = json!("#123456");
    let fx = client.respond(
        &ApiRequest::Profile {
            user_id: ALICE.into(),
        },
        json!({ "user": full, "mutualServers": { "count": 0, "items": [] } }),
    );
    let view = profile(&fx).expect("profile");
    assert_eq!(
        (view.bio.as_str(), view.banner_color.as_str()),
        ("hi there", "#123456")
    );
}

#[test]
fn my_own_profile_should_not_be_fetched() {
    let mut client = Client::online();
    let fx = client.command(Command::ShowProfile(Some(ME.into())));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Http { .. })));
    assert!(profile(&fx).expect("profile").me);
}

#[test]
fn closing_the_profile_should_empty_it() {
    let mut client = Client::online();
    client.command(Command::ShowProfile(Some(ALICE.into())));
    let fx = client.command(Command::ShowProfile(None));
    assert_eq!(profile(&fx), Some(ProfileView::default()));
}

#[test]
fn sign_out_should_empty_the_dm_list_and_the_member_panel() {
    let mut client = Client::online();
    client.command(Command::SignOut);
    assert_eq!((client.dms.len(), client.rows.len()), (0, 0));
}

#[test]
fn google_avatars_should_go_through_the_media_proxy() {
    let mut client = Client::online();
    let mut alice = user(ALICE, "alice", Some("Alice"), "online");
    alice["avatar_url"] = json!("https://lh3.googleusercontent.com/a/b=s96");
    client.reload_dms(json!([dm(DM, alice)]));
    assert_eq!(
        client.dms[0].avatar,
        "https://api.fixture.invalid/api/proxy-media?url=https%3A%2F%2Flh3.googleusercontent.com%2Fa%2Fb%3Ds96"
    );
}

#[test]
fn dm_rows_should_be_keyed_by_channel() {
    let client = Client::online();
    assert_eq!(client.dms[0].key(), DM);
}

#[test]
fn a_post_in_another_channel_should_not_clear_the_typer() {
    let mut client = Client::online();
    client.dispatch(
        "TYPING_START",
        json!({ "channel_id": TEXT, "user_id": ALICE }),
    );
    client.dispatch(
        "MESSAGE_CREATE",
        json!({ "id": "m_dm", "channel_id": DM, "author_id": ALICE,
                "content": "hi", "created_at": "2026-11-04T13:00:00.000Z" }),
    );
    assert!(client.member(ALICE).typing);
}

#[test]
fn nobody_should_show_online_before_the_presence_list() {
    let mut client = Client::online();
    client.command(Command::SignOut);
    client.command(Command::UseToken("tok_fixture".into()));
    client.respond(&ApiRequest::Me, fixture("user-me.json"));
    client.respond(
        &ApiRequest::Dms,
        json!([dm(DM, user(ALICE, "alice", Some("Alice"), "online"))]),
    );
    assert_eq!(client.dms[0].presence, "offline");
}

#[test]
fn closing_a_card_for_someone_else_should_keep_the_open_one() {
    let mut client = Client::online();
    client.command(Command::ShowProfile(Some(ALICE.into())));
    let fx = client.command(Command::HideProfile(ME.into()));
    assert!(fx.is_empty());
    assert_eq!(client.state.profile_user(), Some(ALICE));
}

#[test]
fn a_server_channel_from_home_should_leave_home() {
    let mut client = Client::online();
    client.command(Command::SelectHome);
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    assert!(views(&fx).contains(&&ViewUpdate::Home(false)));
}

#[test]
fn select_server_from_a_dm_should_close_the_dm_at_once() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    let fx = client.command(Command::SelectServer(SERVER.into()));
    assert_eq!(client.state.active_channel(), None);
    assert!(views(&fx)
        .iter()
        .any(|v| matches!(v, ViewUpdate::ActiveDm(dm) if dm.user_id.is_empty())));
}
