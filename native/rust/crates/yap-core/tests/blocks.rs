//! Blocks on the native client (store/blocks.rs): a blocked user's messages
//! collapse and their typing is hidden; a DM with someone you blocked, or
//! one the server refused a send in, closes its composer with the reason;
//! a refused ring says why. Nothing here talks to a server.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::blocks::{CALL_BLOCKED, DM_BLOCKED_YOU, DM_YOU_BLOCKED};
use yap_core::store::view::{DmView, MessageRow};
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State, ViewDiff,
    ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const DM: &str = "dm_fixture_alice";
const ALICE: &str = "user_fixture_alice";
const BOB: &str = "user_fixture_bob";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn user(id: &str, username: &str) -> Value {
    json!({
        "id": id, "username": username, "display_name": null,
        "avatar_url": null, "status": "online", "custom_status": null,
    })
}

fn relationship(id: &str, username: &str, kind: i64) -> Value {
    json!({ "user": user(id, username), "type": kind, "created_at": "2026-11-14T00:00:00.000Z" })
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    rows: Vec<MessageRow>,
    dm: DmView,
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

    /// Signed in, #text open with the page fixture (Bob and Alice wrote).
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            rows: Vec::new(),
            dm: DmView::default(),
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
        client.respond(
            &ApiRequest::Dms,
            json!([{ "id": DM, "channel_type": "dm", "name": "x", "created_at": null,
                     "recipient": user(ALICE, "alice") }]),
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

    fn blocked_rows(&self) -> Vec<(&str, bool)> {
        self.rows
            .iter()
            .map(|r| (r.author_id.as_str(), r.blocked))
            .collect()
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

fn refusal(message: &str) -> ApiError {
    ApiError::Http {
        status: 403,
        message: message.into(),
    }
}

#[test]
fn blocking_someone_should_collapse_their_messages_until_unblocked() {
    let mut client = Client::online();
    assert!(client.rows.iter().any(|r| r.author_id == BOB));
    assert!(client.rows.iter().all(|r| !r.blocked));

    client.dispatch("RELATIONSHIP_ADD", relationship(BOB, "bob", 1));
    for (author, blocked) in client.blocked_rows() {
        assert_eq!(blocked, author == BOB, "{author}");
    }

    client.dispatch("RELATIONSHIP_REMOVE", json!({ "user_id": BOB }));
    assert!(client.rows.iter().all(|r| !r.blocked));
}

#[test]
fn a_friend_is_not_collapsed() {
    let mut client = Client::online();
    let fx = client.dispatch("RELATIONSHIP_ADD", relationship(BOB, "bob", 0));
    // Nothing about blocks changed: the rows are not redrawn.
    assert!(!views(&fx)
        .iter()
        .any(|v| matches!(v, ViewUpdate::Messages(_))));
    assert!(client.rows.iter().all(|r| !r.blocked));
}

#[test]
fn typing_from_someone_blocked_should_not_show() {
    let mut client = Client::online();
    client.dispatch("RELATIONSHIP_ADD", relationship(BOB, "bob", 1));

    let fx = client.dispatch(
        "TYPING_START",
        json!({ "channel_id": TEXT, "user_id": BOB }),
    );
    assert!(!views(&fx)
        .iter()
        .any(|v| matches!(v, ViewUpdate::Typing(_))));

    let fx = client.dispatch(
        "TYPING_START",
        json!({ "channel_id": TEXT, "user_id": ALICE }),
    );
    assert!(views(&fx)
        .iter()
        .any(|v| matches!(v, ViewUpdate::Typing(t) if !t.is_empty())));
}

#[test]
fn a_dm_with_someone_you_blocked_should_close_its_composer() {
    let mut client = Client::online();
    client.in_dm();
    assert_eq!(client.dm.user_id, ALICE);
    assert!(client.dm.block.is_empty());

    client.dispatch("RELATIONSHIP_ADD", relationship(ALICE, "alice", 1));
    assert_eq!(client.dm.block, "you_blocked");
    assert_eq!(client.dm.block_notice, DM_YOU_BLOCKED);

    // Unblocking (anywhere) opens it again.
    client.dispatch("RELATIONSHIP_REMOVE", json!({ "user_id": ALICE }));
    assert!(client.dm.block.is_empty());
    assert!(client.dm.block_notice.is_empty());
}

#[test]
fn a_send_refused_over_a_block_should_drop_the_row_and_say_why() {
    let mut client = Client::online();
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

    client.fail(&post, refusal(DM_BLOCKED_YOU));

    // Not "Not sent" with Retry: the row is gone and the composer says why.
    assert!(client.rows.is_empty(), "{:#?}", client.rows);
    assert_eq!(client.dm.block, "blocked_you");
    assert_eq!(client.dm.block_notice, DM_BLOCKED_YOU);

    // Leaving the DM forgets it: coming back tries again.
    client.command(Command::SelectChannel(TEXT.into()));
    client.command(Command::SelectChannel(DM.into()));
    assert!(client.dm.block.is_empty());
}

#[test]
fn another_send_failure_should_keep_the_row_for_retry() {
    let mut client = Client::online();
    client.in_dm();
    client.command(Command::Send {
        content: "hi".into(),
        reply_to_id: None,
        nonce: "n2".into(),
        sent_at: "2026-11-14T00:00:01.000Z".into(),
    });
    let post = ApiRequest::SendMessage {
        channel_id: DM.into(),
        content: "hi".into(),
        reply_to_id: None,
        nonce: "n2".into(),
        attachment_ids: Vec::new(),
        nsfw_attachment_ids: Vec::new(),
    };
    client.fail(&post, refusal("Channel not found or access denied"));
    assert_eq!(client.rows.len(), 1);
    assert!(client.rows[0].failed);
    assert!(client.dm.block.is_empty());
}

#[test]
fn a_ring_refused_over_a_block_should_say_why() {
    let mut client = Client::online();
    let fx = client.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": null, "reason": "blocked" }),
    );
    assert!(views(&fx).contains(&&ViewUpdate::Notice(CALL_BLOCKED.into())));
    assert!(yap_core::store::calls::silent_stop("blocked"));
}
