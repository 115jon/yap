//! Camera backgrounds through the reducer: a failed picture fetch waits
//! before it runs again (a 404 is not fetched in a loop), a fetch asked for
//! before the list waits for it, and a delete removes the kept picture.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::backgrounds::{BackgroundsCommand, BackgroundsRequest};
use yap_core::store::{reduce, ApiRequest, ApiResponse, Command, Effect, GatewayEvent, Input};
use yap_core::store::{RequestId, State};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const ID: &str = "bg_fixture_beach";
const DEST: &str = "app/camera-backgrounds/bg_fixture_beach";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn list_body() -> Value {
    json!({ "backgrounds": [{
        "id": ID, "name": "Beach", "url": "/api/camera-backgrounds/bg_fixture_beach",
        "contentType": "image/png", "sizeBytes": 10, "createdAt": 5
    }]})
}

fn not_found() -> ApiError {
    ApiError::Http {
        status: 404,
        message: "not found".into(),
    }
}

/// The downloads among `fx`.
fn downloads(fx: &[Effect]) -> Vec<RequestId> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Download { id, .. } => Some(*id),
            _ => None,
        })
        .collect()
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            if let Effect::Http { id, request, .. } = effect {
                self.asked.push((*id, request.clone()));
            }
        }
        fx
    }

    fn command(&mut self, command: BackgroundsCommand) -> Vec<Effect> {
        self.input(Input::User(Command::Backgrounds(command)))
    }

    fn newest(&self, request: &ApiRequest) -> RequestId {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .expect("asked")
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self.newest(request);
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn answer(&mut self, id: RequestId, result: Result<ApiResponse, ApiError>) -> Vec<Effect> {
        self.input(Input::Http { id, result })
    }

    fn fetch(&mut self, now_ms: i64, retry: bool) -> Vec<Effect> {
        self.command(BackgroundsCommand::Fetch {
            id: ID.into(),
            dest: DEST.into(),
            now_ms,
            retry,
        })
    }

    fn signed_in() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    /// Signed in with the list loaded.
    fn listed() -> Self {
        let mut client = Self::signed_in();
        client.command(BackgroundsCommand::Load);
        let list = ApiRequest::Backgrounds(BackgroundsRequest::List);
        client.respond(&list, list_body());
        client
    }

    /// Listed, and one fetch of the picture answered 404 at 1 s.
    fn failed_once() -> Self {
        let mut client = Self::listed();
        let fetch = downloads(&client.fetch(1_000, false));
        assert_eq!(fetch.len(), 1, "the first fetch runs");
        client.answer(fetch[0], Err(not_found()));
        client
    }
}

#[test]
fn a_404_should_not_fetch_again_on_the_next_ask() {
    let mut client = Client::failed_once();
    assert!(downloads(&client.fetch(2_000, false)).is_empty());
}

#[test]
fn a_404_should_not_fetch_again_on_its_own_answer() {
    let mut client = Client::listed();
    let fetch = downloads(&client.fetch(1_000, false));
    let fx = client.answer(fetch[0], Err(not_found()));
    assert!(downloads(&fx).is_empty());
}

#[test]
fn a_failed_fetch_should_run_again_after_thirty_seconds() {
    let mut client = Client::failed_once();
    assert_eq!(downloads(&client.fetch(31_000, false)).len(), 1);
}

#[test]
fn a_second_failure_should_wait_twice_as_long() {
    let mut client = Client::failed_once();
    let again = downloads(&client.fetch(31_000, false));
    client.answer(again[0], Err(not_found()));
    assert!(downloads(&client.fetch(31_000 + 59_000, false)).is_empty());
}

#[test]
fn picking_the_background_again_should_fetch_at_once() {
    let mut client = Client::failed_once();
    assert_eq!(downloads(&client.fetch(2_000, true)).len(), 1);
}

#[test]
fn a_fetch_before_the_list_should_run_when_the_list_arrives() {
    let mut client = Client::signed_in();
    assert!(downloads(&client.fetch(1_000, false)).is_empty());
    client.command(BackgroundsCommand::Load);
    let list = ApiRequest::Backgrounds(BackgroundsRequest::List);
    assert_eq!(downloads(&client.respond(&list, list_body())).len(), 1);
}

#[test]
fn an_unsafe_id_should_never_be_fetched() {
    let mut client = Client::listed();
    let fx = client.command(BackgroundsCommand::Fetch {
        id: "../settings".into(),
        dest: "app/settings".into(),
        now_ms: 1_000,
        retry: true,
    });
    assert!(downloads(&fx).is_empty());
}

#[test]
fn a_delete_should_remove_the_kept_picture() {
    let mut client = Client::listed();
    client.command(BackgroundsCommand::Delete(ID.into()));
    let delete = ApiRequest::Backgrounds(BackgroundsRequest::Delete(ID.into()));
    let fx = client.respond(&delete, json!({ "ok": true }));
    assert!(fx
        .iter()
        .any(|e| matches!(e, Effect::RemoveCachedBackground { id } if id == ID)));
}

#[test]
fn a_refused_delete_should_keep_the_picture() {
    let mut client = Client::listed();
    client.command(BackgroundsCommand::Delete(ID.into()));
    let delete = ApiRequest::Backgrounds(BackgroundsRequest::Delete(ID.into()));
    let id = client.newest(&delete);
    let fx = client.answer(id, Err(not_found()));
    assert!(!fx
        .iter()
        .any(|e| matches!(e, Effect::RemoveCachedBackground { .. })));
}
