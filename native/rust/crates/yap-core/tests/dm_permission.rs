//! "Who can DM you" on the native client (dm_permission.rs): the setting
//! is read from the account, saved through update-profile at once and
//! rolled back if the save fails, follows the user's other sessions; a send
//! the recipient's setting refuses drops its row and closes the DM's
//! composer with the server's sentence; a ring it refuses says why.
//! Nothing here talks to a server.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::blocks::DmBlock;
use yap_core::store::view::{DmView, MessageRow, SettingsView};
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State, ViewDiff,
    ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const DM: &str = "dm_fixture_alice";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";
const NOT_ACCEPTING: &str = "Alice isn't accepting messages from you.";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    rows: Vec<MessageRow>,
    dm: DmView,
    settings: SettingsView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Messages(diff)) => {
                    assert!(
                        (**diff).clone().apply(&mut self.rows)
                            || matches!(**diff, ViewDiff::Reset(_))
                    );
                }
                Effect::View(ViewUpdate::ActiveDm(dm)) => self.dm = dm.clone(),
                Effect::View(ViewUpdate::Settings(view)) => self.settings = (**view).clone(),
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

    fn fail(&mut self, request: &ApiRequest, error: ApiError) -> Vec<Effect> {
        let id = self.id_of(request);
        self.input(Input::Http {
            id,
            result: Err(error),
        })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(Dispatch::decode(event, data).expect("decodes")),
        })
    }

    /// Signed in as `me` (extra fields on the account), #text open, a DM
    /// with Alice listed.
    fn online(me: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            rows: Vec::new(),
            dm: DmView::default(),
            settings: SettingsView::default(),
        };
        let mut account = fixture("user-me.json");
        if let (Some(account), Some(extra)) = (account.as_object_mut(), me.as_object()) {
            account.extend(extra.clone());
        }
        client.command(Command::UseToken("tok_fixture".into()));
        client.respond(&ApiRequest::Me, account);
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
        client.respond(
            &ApiRequest::Dms,
            json!([{ "id": DM, "channel_type": "dm", "name": "Alice", "created_at": null,
                     "recipient": {
                         "id": ALICE, "username": "alice", "display_name": "Alice",
                         "avatar_url": null, "status": "online", "custom_status": null,
                     } }]),
        );
        client.input(Input::Gateway {
            session: client.state.session().expect("session").id,
            event: GatewayEvent::Ready,
        });
        client
    }

    /// The DM with Alice open, empty.
    fn in_dm(&mut self) {
        self.command(Command::SelectChannel(DM.into()));
        let page = ApiRequest::Messages {
            channel_id: DM.into(),
            before: None,
        };
        self.respond(
            &page,
            json!({ "messages": [], "hasMoreBefore": false, "hasMoreAfter": false }),
        );
    }
}

fn views(fx: &[Effect]) -> Vec<&ViewUpdate> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(v) => Some(v),
            _ => None,
        })
        .collect()
}

fn patch(fx: &[Effect]) -> Option<Value> {
    fx.iter().find_map(|e| match e {
        Effect::Http {
            request: ApiRequest::UpdateProfile(body),
            ..
        } => Some(body.clone()),
        _ => None,
    })
}

fn not_accepting(message: &str) -> ApiError {
    ApiError::Blocked {
        block: DmBlock::NotAccepting,
        status: 403,
        message: message.into(),
    }
}

#[test]
fn an_account_without_the_field_should_read_everyone() {
    let client = Client::online(json!({}));
    assert_eq!(client.settings.dm_permission, "everyone");
}

#[test]
fn the_accounts_setting_should_show() {
    let client = Client::online(json!({ "dm_permission": "nobody" }));
    assert_eq!(client.settings.dm_permission, "nobody");
}

#[test]
fn picking_a_value_should_save_it_at_once() {
    let mut client = Client::online(json!({}));
    let fx = client.command(Command::SetDmPermission("friends_and_servers".into()));
    assert_eq!(
        patch(&fx),
        Some(json!({ "dmPermission": "friends_and_servers" }))
    );
    assert_eq!(client.settings.dm_permission, "friends_and_servers");
    assert_eq!(client.settings.dm_permission_saving, "friends_and_servers");

    let request = ApiRequest::UpdateProfile(json!({ "dmPermission": "friends_and_servers" }));
    client.respond(&request, json!({ "user": {} }));
    assert_eq!(client.settings.dm_permission, "friends_and_servers");
    assert!(client.settings.dm_permission_saving.is_empty());
    assert!(client.settings.dm_permission_error.is_empty());
}

#[test]
fn an_unknown_or_unchanged_value_should_send_nothing() {
    let mut client = Client::online(json!({ "dm_permission": "nobody" }));
    assert_eq!(
        patch(&client.command(Command::SetDmPermission("admins".into()))),
        None
    );
    assert_eq!(
        patch(&client.command(Command::SetDmPermission("nobody".into()))),
        None
    );
}

#[test]
fn a_failed_save_should_roll_back_with_the_reason() {
    let mut client = Client::online(json!({ "dm_permission": "friends_and_servers" }));
    client.command(Command::SetDmPermission("nobody".into()));
    let request = ApiRequest::UpdateProfile(json!({ "dmPermission": "nobody" }));
    client.fail(
        &request,
        ApiError::Http {
            status: 500,
            message: "Failed to update profile".into(),
        },
    );
    assert_eq!(client.settings.dm_permission, "friends_and_servers");
    assert!(client.settings.dm_permission_saving.is_empty());
    assert!(client
        .settings
        .dm_permission_error
        .contains("save who can DM you"));
}

#[test]
fn a_change_from_another_session_should_apply() {
    let mut client = Client::online(json!({}));
    client.dispatch(
        "USER_PROFILE_UPDATE",
        json!({ "user_id": ME, "dm_permission": "nobody" }),
    );
    assert_eq!(client.settings.dm_permission, "nobody");
}

#[test]
fn a_send_refused_by_their_setting_should_drop_the_row_and_name_them() {
    let mut client = Client::online(json!({}));
    client.in_dm();
    client.command(Command::Send {
        content: "hello?".into(),
        reply_to_id: None,
        nonce: "n1".into(),
        sent_at: "2026-11-14T00:00:01.000Z".into(),
    });
    assert_eq!(client.rows.len(), 1, "the pending row");
    let post = ApiRequest::SendMessage {
        channel_id: DM.into(),
        content: "hello?".into(),
        reply_to_id: None,
        nonce: "n1".into(),
        attachment_ids: Vec::new(),
        nsfw_attachment_ids: Vec::new(),
    };

    client.fail(&post, not_accepting(NOT_ACCEPTING));

    assert!(client.rows.is_empty(), "{:#?}", client.rows);
    assert_eq!(client.dm.block, "not_accepting");
    assert_eq!(client.dm.block_notice, NOT_ACCEPTING);

    // Their reply opens the DM again (the server now lets us write).
    client.dispatch(
        "MESSAGE_CREATE",
        json!({ "id": "m_reply", "channel_id": DM, "author_id": ALICE,
                "content": "hi!", "created_at": "2026-11-14T00:00:02.000Z",
                "author": { "id": ALICE, "username": "alice", "display_name": "Alice",
                            "avatar_url": null } }),
    );
    assert!(client.dm.block.is_empty(), "{:?}", client.dm);

    // Blocking them yourself wins the composer (Unblock).
    client.dispatch(
        "RELATIONSHIP_ADD",
        json!({ "user": { "id": ALICE, "username": "alice", "display_name": "Alice",
                          "avatar_url": null, "status": "online", "custom_status": null },
                "type": 1, "created_at": "2026-11-14T00:00:00.000Z" }),
    );
    assert_eq!(client.dm.block, "you_blocked");

    // Unblocked and back: the next send tries again.
    client.dispatch("RELATIONSHIP_REMOVE", json!({ "user_id": ALICE }));
    client.command(Command::SelectChannel(TEXT.into()));
    client.command(Command::SelectChannel(DM.into()));
    assert!(client.dm.block.is_empty());
}

#[test]
fn a_ring_refused_by_their_setting_should_say_why() {
    let mut client = Client::online(json!({}));
    let fx = client.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": null, "reason": "not_accepting",
                "message": "Alice isn't accepting calls from you." }),
    );
    assert!(views(&fx).contains(&&ViewUpdate::Notice(
        "Alice isn't accepting calls from you.".into()
    )));
    assert!(yap_core::store::calls::silent_stop("not_accepting"));

    // An older server's stop without the sentence still says something.
    let fx = client.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": null, "reason": "not_accepting" }),
    );
    assert!(views(&fx).contains(&&ViewUpdate::Notice(
        "This person isn't accepting calls from you.".into()
    )));
}

#[test]
fn the_api_reads_the_refusal_by_its_code() {
    let body = json!({ "error": NOT_ACCEPTING, "code": "NOT_ACCEPTING_DMS" });
    assert_eq!(
        yap_core::store::blocks::refusal(403, &body),
        Some(DmBlock::NotAccepting)
    );
    assert_eq!(
        yap_core::settings::describe_api_error(&not_accepting(NOT_ACCEPTING), "open this DM"),
        NOT_ACCEPTING
    );
}

#[test]
fn a_new_dm_refused_by_their_setting_should_say_why() {
    let mut client = Client::online(json!({}));
    client.command(Command::OpenDm("user_fixture_carol".into()));
    let request = ApiRequest::OpenDm {
        user_id: "user_fixture_carol".into(),
    };
    let fx = client.fail(
        &request,
        not_accepting("Carol isn't accepting messages from you."),
    );
    assert!(views(&fx).contains(&&ViewUpdate::Notice(
        "Carol isn't accepting messages from you.".into()
    )));
}
