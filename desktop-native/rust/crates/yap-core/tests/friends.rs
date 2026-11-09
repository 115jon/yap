//! Friends and relationships: request shapes (as the web sends them), the
//! list at bootstrap and after a fresh gateway session, the add form and
//! the card's actions with their copy, the `RELATIONSHIP_*` dispatches, tab
//! filtering and the Home badge. Nothing here talks to a server.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::ViewUpdate;
use yap_core::store::{
    friends_view, reduce, ApiRequest, Command, Dispatch, Effect, FriendAction, FriendsCommand,
    FriendsContext, FriendsRequest, FriendsState, FriendsView, GatewayEvent, Input, Method,
    Presence, RequestId, State,
};
use yap_core::ApiError;
use yap_protocol::rest::Relationship;

const BASE: &str = "https://api.fixture.invalid";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";
const BOB: &str = "user_fixture_bob";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn rel(id: &str, username: &str, kind: i64) -> Value {
    json!({
        "user": { "id": id, "username": username, "display_name": null,
                  "avatar_url": null, "avatar_display": null, "status": "online",
                  "custom_status": "Out walking" },
        "type": kind,
        "created_at": "2026-11-01T10:00:00.000Z",
    })
}

fn friends(request: FriendsRequest) -> ApiRequest {
    ApiRequest::Friends(request)
}

fn http_error(status: u16) -> ApiError {
    ApiError::Http {
        status,
        message: "nope".into(),
    }
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    view: FriendsView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Friends(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: FriendsCommand) -> Vec<Effect> {
        self.input(Input::User(Command::Friends(command)))
    }

    fn act(&mut self, action: FriendAction, user_id: &str) -> Vec<Effect> {
        self.command(FriendsCommand::Act {
            action,
            user_id: user_id.into(),
            username: "alice".into(),
            name: "Alice".into(),
        })
    }

    fn newest(&self, request: &ApiRequest) -> Option<RequestId> {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
    }

    fn count(&self, request: &ApiRequest) -> usize {
        self.asked.iter().filter(|(_, r)| r == request).count()
    }

    fn respond_to(&mut self, id: RequestId, request: &ApiRequest, body: Value) {
        let result = request.decode(body);
        self.input(Input::Http { id, result });
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) {
        let id = self.newest(request).expect("asked");
        self.respond_to(id, request, body);
    }

    fn fail(&mut self, request: &ApiRequest, error: ApiError) {
        let id = self.newest(request).expect("asked");
        self.input(Input::Http {
            id,
            result: Err(error),
        });
    }

    fn dispatch(&mut self, event: &str, data: Value) {
        let session = self.state.session().expect("session").id;
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(dispatch),
        });
    }

    fn gateway(&mut self, event: GatewayEvent) {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event });
    }

    /// Signed in, the bootstrap's friends list asked for (not answered).
    fn signed_in() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            view: FriendsView::default(),
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        client.gateway(GatewayEvent::Ready);
        client
    }

    /// Signed in with Alice a friend, Bob an incoming request.
    fn with_list() -> Self {
        let mut client = Self::signed_in();
        client.respond(
            &friends(FriendsRequest::List),
            json!([rel(ALICE, "alice", 0), rel(BOB, "bob", 2)]),
        );
        client
    }
}

fn ids(rows: &[yap_core::store::FriendRow]) -> Vec<&str> {
    rows.iter().map(|r| r.user_id.as_str()).collect()
}

// Request shapes

#[test]
fn list_should_get_api_friends() {
    let request = FriendsRequest::List;
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (Method::Get, "/api/friends".to_owned(), Value::Null)
    );
}

#[test]
fn send_should_post_the_username() {
    let request = FriendsRequest::Send {
        username: "alice".into(),
    };
    assert_eq!(
        (request.method(), request.body()),
        (Method::Post, json!({ "username": "alice" }))
    );
}

#[test]
fn accept_should_put_the_target_with_accept() {
    let request = FriendsRequest::Accept {
        user_id: ALICE.into(),
    };
    assert_eq!(
        (request.method(), request.body()),
        (
            Method::Put,
            json!({ "target_user_id": ALICE, "action": "accept" })
        )
    );
}

#[test]
fn block_should_put_the_target_with_block() {
    let request = FriendsRequest::Block {
        user_id: ALICE.into(),
    };
    assert_eq!(
        request.body(),
        json!({ "target_user_id": ALICE, "action": "block" })
    );
}

#[test]
fn remove_should_delete_with_the_target_in_the_body() {
    let request = FriendsRequest::Remove {
        user_id: ALICE.into(),
    };
    assert_eq!(
        (request.method(), request.body()),
        (Method::Delete, json!({ "target_user_id": ALICE }))
    );
}

#[test]
fn an_odd_list_row_should_not_hide_the_rest() {
    let decoded = FriendsRequest::List
        .decode(json!([rel(ALICE, "alice", 0), { "type": 0 }]))
        .expect("decodes");
    assert_eq!(
        decoded,
        yap_core::store::FriendsResponse::List(vec![serde_json::from_value(rel(
            ALICE, "alice", 0
        ))
        .expect("rel")])
    );
}

// Loading

#[test]
fn sign_in_should_load_the_relationships() {
    let client = Client::signed_in();
    assert_eq!(client.count(&friends(FriendsRequest::List)), 1);
}

#[test]
fn a_fresh_gateway_session_after_a_drop_should_load_them_again() {
    let mut client = Client::with_list();
    client.gateway(GatewayEvent::Closed);
    client.gateway(GatewayEvent::Ready);
    assert_eq!(client.count(&friends(FriendsRequest::List)), 2);
}

#[test]
fn the_view_loading_should_show_until_the_list_answers() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::Load);
    let loading = client.view.loading;
    client.respond(&friends(FriendsRequest::List), json!([]));
    assert_eq!((loading, client.view.loading), (true, false));
}

#[test]
fn a_quiet_refresh_should_not_show_the_loading_rows() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::Refresh);
    assert!(!client.view.loading);
}

#[test]
fn an_older_list_answer_should_be_dropped() {
    let mut client = Client::signed_in();
    let list = friends(FriendsRequest::List);
    let old = client.newest(&list).expect("asked");
    client.command(FriendsCommand::Refresh);
    client.respond_to(old, &list, json!([rel(ALICE, "alice", 0)]));
    assert!(client.state.friends().relationships.is_empty());
}

#[test]
fn a_failed_load_should_keep_the_list() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::Refresh);
    client.fail(&friends(FriendsRequest::List), http_error(500));
    assert_eq!(client.state.friends().relationships.len(), 2);
}

#[test]
fn sign_out_should_forget_the_relationships() {
    let mut client = Client::with_list();
    client.input(Input::User(Command::SignOut));
    assert_eq!(client.view.pending, 0);
}

// Tabs and badges

#[test]
fn tabs_should_split_friends_requests_and_blocked() {
    let mut client = Client::signed_in();
    client.respond(
        &friends(FriendsRequest::List),
        json!([
            rel(ALICE, "alice", 0),
            rel(BOB, "bob", 2),
            rel("u3", "cy", 3),
            rel("u4", "di", 1),
        ]),
    );
    let v = &client.view;
    assert_eq!(
        (ids(&v.all), ids(&v.incoming), ids(&v.sent), ids(&v.blocked)),
        (vec![ALICE], vec![BOB], vec!["u3"], vec!["u4"])
    );
}

#[test]
fn online_should_list_only_friends_whose_presence_is_online() {
    let mut client = Client::signed_in();
    client.respond(
        &friends(FriendsRequest::List),
        json!([
            rel(ALICE, "alice", 0),
            rel("u3", "cy", 0),
            rel(BOB, "bob", 2)
        ]),
    );
    client.dispatch(
        "PRESENCE_LIST",
        json!({ "users": [
            { "user_id": ALICE, "status": "online" },
            { "user_id": "u3", "status": "idle" },
            { "user_id": BOB, "status": "online" },
        ] }),
    );
    assert_eq!(ids(&client.view.online), vec![ALICE]);
}

#[test]
fn a_friend_unseen_by_the_gateway_should_be_offline() {
    let client = Client::with_list();
    assert_eq!(client.view.all[0].presence, "offline");
}

#[test]
fn only_a_friend_should_get_a_status_line() {
    let client = Client::with_list();
    assert_eq!(
        (
            client.view.all[0].status_line.as_str(),
            client.view.incoming[0].status_line.as_str()
        ),
        ("Out walking", "")
    );
}

#[test]
fn pending_should_count_incoming_requests_only() {
    let mut client = Client::signed_in();
    client.respond(
        &friends(FriendsRequest::List),
        json!([rel(ALICE, "alice", 2), rel(BOB, "bob", 3)]),
    );
    assert_eq!((client.view.pending, client.view.home_badge), (1, 1));
}

#[test]
fn the_home_badge_should_add_the_overflowing_dms() {
    let state = FriendsState {
        relationships: vec![serde_json::from_value(rel(ALICE, "alice", 2)).expect("rel")],
        ..FriendsState::default()
    };
    let presence = Presence::default();
    let view = friends_view(
        &state,
        &FriendsContext {
            presence: &presence,
            api_base: BASE,
            me_id: ME,
            card_user: None,
            dm_unread: 5,
        },
    );
    assert_eq!(view.home_badge, 2 + 1);
}

#[test]
fn the_card_should_know_the_open_profiles_relation() {
    let mut client = Client::with_list();
    client.input(Input::User(Command::ShowProfile(Some(BOB.into()))));
    assert_eq!(
        (client.view.card.user_id.as_str(), client.view.card.relation),
        (BOB, "incoming")
    );
}

#[test]
fn my_own_card_should_be_self() {
    let mut client = Client::with_list();
    client.input(Input::User(Command::ShowProfile(Some(ME.into()))));
    assert_eq!(client.view.card.relation, "self");
}

// Dispatches

#[test]
fn relationship_add_should_replace_theirs_and_go_on_top() {
    let mut client = Client::with_list();
    client.dispatch("RELATIONSHIP_ADD", rel(BOB, "bob", 0));
    assert_eq!(ids(&client.view.all), vec![BOB, ALICE]);
}

#[test]
fn relationship_remove_should_drop_them() {
    let mut client = Client::with_list();
    client.dispatch("RELATIONSHIP_REMOVE", json!({ "user_id": ALICE }));
    assert!(client.view.all.is_empty());
}

#[test]
fn a_profile_update_should_rename_their_row() {
    let mut client = Client::with_list();
    client.dispatch(
        "USER_PROFILE_UPDATE",
        json!({ "user_id": ALICE, "display_name": "Alice A" }),
    );
    assert_eq!(client.view.all[0].name, "Alice A");
}

// The add form

#[test]
fn an_empty_username_should_ask_for_one() {
    let mut client = Client::with_list();
    let fx = client.command(FriendsCommand::SendRequest("   ".into()));
    let posted = fx.iter().any(|e| {
        matches!(
            e,
            Effect::Http {
                request: ApiRequest::Friends(FriendsRequest::Send { .. }),
                ..
            }
        )
    });
    assert_eq!(
        (posted, client.view.add.message.as_str()),
        (false, "Enter a username.")
    );
}

#[test]
fn a_request_should_send_the_trimmed_username() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest("  cy ".into()));
    assert!(client
        .newest(&friends(FriendsRequest::Send {
            username: "cy".into()
        }))
        .is_some());
}

#[test]
fn a_sent_request_should_say_so_and_reload() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest("cy".into()));
    let send = friends(FriendsRequest::Send {
        username: "cy".into(),
    });
    client.respond(&send, json!({ "user": { "id": "u3" }, "type": 3 }));
    assert_eq!(
        (
            client.view.add.message.as_str(),
            client.view.add.sent,
            client.count(&friends(FriendsRequest::List))
        ),
        ("Friend request sent.", 1, 2)
    );
}

#[test]
fn accepting_their_request_by_name_should_say_friend_added() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest("bob".into()));
    let send = friends(FriendsRequest::Send {
        username: "bob".into(),
    });
    client.respond(&send, json!({ "user": { "id": BOB }, "type": 0 }));
    assert_eq!(client.view.add.message, "Friend added.");
}

#[test]
fn an_unknown_username_should_say_no_one_has_it() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest("zed".into()));
    let send = friends(FriendsRequest::Send {
        username: "zed".into(),
    });
    client.fail(&send, http_error(404));
    assert_eq!(
        (client.view.add.kind, client.view.add.message.as_str()),
        (
            "error",
            "No one has that username. Check the spelling and try again."
        )
    );
}

#[test]
fn a_conflict_should_say_why_it_cannot_be_sent() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest("alice".into()));
    let send = friends(FriendsRequest::Send {
        username: "alice".into(),
    });
    client.fail(&send, http_error(409));
    assert_eq!(
        client.view.add.message,
        "You're already friends, have a request pending, or blocked this person."
    );
}

#[test]
fn typing_should_clear_an_error_under_the_field() {
    let mut client = Client::with_list();
    client.command(FriendsCommand::SendRequest(String::new()));
    client.command(FriendsCommand::ClearAdd);
    assert_eq!(client.view.add.kind, "");
}

// Row and card actions

#[test]
fn accept_should_mark_them_busy_until_it_answers() {
    let mut client = Client::with_list();
    client.act(FriendAction::Accept, BOB);
    let busy = client.view.acts.get(BOB).map(|a| a.busy);
    client.respond(
        &friends(FriendsRequest::Accept {
            user_id: BOB.into(),
        }),
        json!({ "success": true, "type": 0 }),
    );
    let act = client.view.acts.get(BOB).cloned().unwrap_or_default();
    assert_eq!((busy, act.busy, act.done), (Some("accept"), "", 1));
}

#[test]
fn a_finished_action_should_reload_the_list() {
    let mut client = Client::with_list();
    client.act(FriendAction::Remove, ALICE);
    client.respond(
        &friends(FriendsRequest::Remove {
            user_id: ALICE.into(),
        }),
        json!({ "success": true }),
    );
    assert_eq!(client.count(&friends(FriendsRequest::List)), 2);
}

#[test]
fn a_second_action_on_a_busy_person_should_be_ignored() {
    let mut client = Client::with_list();
    client.act(FriendAction::Accept, BOB);
    let fx = client.act(FriendAction::Decline, BOB);
    assert!(fx.iter().all(|e| !matches!(e, Effect::Http { .. })));
}

#[test]
fn a_failed_block_should_say_so_in_the_cards_words() {
    let mut client = Client::with_list();
    client.act(FriendAction::Block, ALICE);
    client.fail(
        &friends(FriendsRequest::Block {
            user_id: ALICE.into(),
        }),
        http_error(500),
    );
    let act = client.view.acts.get(ALICE).cloned().unwrap_or_default();
    assert_eq!(
        (act.failed, act.error.as_str()),
        ("block", "Unable to block Alice. Try again in a moment.")
    );
}

#[test]
fn unblock_should_delete_the_relationship() {
    let mut client = Client::with_list();
    client.act(FriendAction::Unblock, ALICE);
    assert!(client
        .newest(&friends(FriendsRequest::Remove {
            user_id: ALICE.into()
        }))
        .is_some());
}

#[test]
fn add_from_a_card_should_post_their_username() {
    let mut client = Client::with_list();
    client.act(FriendAction::Add, "u9");
    assert!(client
        .newest(&friends(FriendsRequest::Send {
            username: "alice".into()
        }))
        .is_some());
}

#[test]
fn no_action_should_target_myself() {
    let mut client = Client::with_list();
    let fx = client.act(FriendAction::Block, ME);
    assert!(fx.iter().all(|e| !matches!(e, Effect::Http { .. })));
}

#[test]
fn a_401_should_end_the_session() {
    let mut client = Client::with_list();
    client.act(FriendAction::Accept, BOB);
    client.fail(
        &friends(FriendsRequest::Accept {
            user_id: BOB.into(),
        }),
        ApiError::Unauthorized,
    );
    assert!(client.state.session().is_none());
}

#[test]
fn relationship_types_should_name_the_card_relation() {
    assert_eq!(
        [0, 1, 2, 3, 9].map(|k| yap_core::store::relation_name(Some(k))),
        ["friend", "blocked", "incoming", "outgoing", "none"]
    );
}

#[test]
fn relationship_constants_should_match_the_server() {
    assert_eq!(
        [
            Relationship::FRIEND,
            Relationship::BLOCKED,
            Relationship::INCOMING,
            Relationship::OUTGOING
        ],
        [0, 1, 2, 3]
    );
}
