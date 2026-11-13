//! Account deletion through the reducer: Settings' Delete account, and the
//! hold (restore screen, no gateway) for an account waiting for deletion.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::account_deletion::{DeletionRefusal, DeletionRequest};
use yap_core::store::deletion::{DeletionCommand, DeletionView};
use yap_core::store::{
    reduce, ApiRequest, Command, Effect, GatewayEvent, Input, Phase, RequestId, State, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn request_for(fx: &[Effect], wanted: &ApiRequest) -> RequestId {
    fx.iter()
        .find_map(|e| match e {
            Effect::Http { id, request, .. } if request == wanted => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {wanted:?} in {fx:#?}"))
}

fn deletion(request: DeletionRequest) -> ApiRequest {
    ApiRequest::Deletion(request)
}

fn view(fx: &[Effect]) -> Option<DeletionView> {
    fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Deletion(view)) => Some(view.clone()),
        _ => None,
    })
}

fn connects(fx: &[Effect]) -> usize {
    fx.iter()
        .filter(|e| matches!(e, Effect::GatewayConnect { .. }))
        .count()
}

fn pending_deletion() -> ApiError {
    ApiError::Deletion(DeletionRefusal::PendingDeletion)
}

struct Client {
    state: State,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        reduce(&mut self.state, input)
    }

    fn fail(&mut self, id: RequestId, error: ApiError) -> Vec<Effect> {
        self.input(Input::Http {
            id,
            result: Err(error),
        })
    }

    fn respond(&mut self, id: RequestId, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn launch() -> (Self, Vec<Effect>) {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        (client, fx)
    }

    /// Launched; `users/me` answered 403 ACCOUNT_PENDING_DELETION. The
    /// effects of that answer.
    fn held() -> (Self, Vec<Effect>) {
        let (mut client, fx) = Self::launch();
        let me = request_for(&fx, &ApiRequest::Me);
        let fx = client.fail(me, pending_deletion());
        (client, fx)
    }

    fn signed_in() -> Self {
        let (mut client, fx) = Self::launch();
        let me = request_for(&fx, &ApiRequest::Me);
        client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        client
    }
}

#[test]
fn a_pending_account_at_launch_should_hold_and_ask_until_when() {
    let (mut client, fx) = Client::held();
    // The session stays (no sign-in screen), the gateway closes, the
    // status is asked for.
    assert!(client.state.session().is_some());
    assert!(fx.contains(&Effect::GatewayDisconnect));
    assert!(!fx.contains(&Effect::ClearToken));
    assert!(!fx.contains(&Effect::View(ViewUpdate::Phase(Phase::SignedOut))));
    let status = request_for(&fx, &deletion(DeletionRequest::Status));
    let shown = view(&fx).expect("view");
    assert!(shown.pending && shown.checking);

    let fx = client.respond(
        status,
        &deletion(DeletionRequest::Status),
        json!({
            "status": "pending",
            "requested_at": "2026-11-13T10:00:00.000Z",
            "delete_after": "2026-12-13T10:00:00.000Z",
            "purging": false,
        }),
    );
    let shown = view(&fx).expect("view");
    assert_eq!(shown.date, "December 13, 2026");
    assert!(shown.pending && !shown.checking && !shown.purging);
}

#[test]
fn the_gateway_should_not_come_back_while_held() {
    let mut client = Client::signed_in();
    let fx = client.input(Input::Gateway {
        session: client.state.session().expect("session").id,
        event: GatewayEvent::PendingDeletion,
    });
    assert!(fx.contains(&Effect::GatewayDisconnect));
    // The strip's Retry and a phone coming back to the front do nothing.
    assert_eq!(connects(&client.command(Command::RetryGateway)), 0);
    client.command(Command::Suspend(true));
    assert_eq!(connects(&client.command(Command::Suspend(false))), 0);
    // A second refusal asks nothing new.
    let fx = client.input(Input::Gateway {
        session: client.state.session().expect("session").id,
        event: GatewayEvent::PendingDeletion,
    });
    assert!(!fx.iter().any(|e| matches!(e, Effect::Http { .. })));
}

#[test]
fn restore_should_start_the_load_over() {
    let (mut client, fx) = Client::held();
    let status = request_for(&fx, &deletion(DeletionRequest::Status));
    client.respond(
        status,
        &deletion(DeletionRequest::Status),
        json!({"status": "pending", "delete_after": "2026-12-13T10:00:00.000Z", "purging": false}),
    );
    let fx = client.command(Command::Deletion(DeletionCommand::Restore));
    let restore = request_for(&fx, &deletion(DeletionRequest::Restore));
    assert!(view(&fx).expect("view").restoring);

    let fx = client.respond(
        restore,
        &deletion(DeletionRequest::Restore),
        json!({"status": "active"}),
    );
    request_for(&fx, &ApiRequest::Me);
    request_for(&fx, &ApiRequest::Servers);
    assert!(!view(&fx).expect("view").pending);
    assert!(fx.contains(&Effect::View(ViewUpdate::Phase(Phase::Loading))));
}

#[test]
fn a_purging_account_should_not_offer_restore() {
    let (mut client, fx) = Client::held();
    let status = request_for(&fx, &deletion(DeletionRequest::Status));
    client.respond(
        status,
        &deletion(DeletionRequest::Status),
        json!({"status": "pending", "delete_after": "2026-12-13T10:00:00.000Z", "purging": true}),
    );
    let fx = client.command(Command::Deletion(DeletionCommand::Restore));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Http { .. })));
    assert!(view(&fx).is_none_or(|v| v.purging));
}

#[test]
fn a_restore_refused_as_in_progress_should_turn_to_purging() {
    let (mut client, fx) = Client::held();
    let status = request_for(&fx, &deletion(DeletionRequest::Status));
    client.fail(status, ApiError::Network("offline".into()));
    let fx = client.command(Command::Deletion(DeletionCommand::Restore));
    let restore = request_for(&fx, &deletion(DeletionRequest::Restore));
    let fx = client.fail(restore, ApiError::Deletion(DeletionRefusal::InProgress));
    let shown = view(&fx).expect("view");
    assert!(shown.pending && shown.purging && shown.restore_error.is_empty());
}

#[test]
fn continue_deletion_should_sign_out() {
    let (mut client, _) = Client::held();
    let fx = client.command(Command::SignOut);
    assert!(client.state.session().is_none());
    assert!(fx.contains(&Effect::ClearToken));
    assert!(!view(&fx).expect("view").pending);
}

#[test]
fn delete_account_should_sign_out_once_scheduled() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::Deletion(DeletionCommand::Request {
        confirm_username: " @Me ".into(),
    }));
    let request = deletion(DeletionRequest::Request {
        confirm_username: "@Me".into(),
    });
    let id = request_for(&fx, &request);
    assert!(view(&fx).expect("view").deleting);
    // The server closed our socket: its retry's refusal is not the
    // restore screen.
    let fx = client.input(Input::Gateway {
        session: client.state.session().expect("session").id,
        event: GatewayEvent::PendingDeletion,
    });
    assert!(fx.contains(&Effect::GatewayDisconnect));
    assert!(view(&fx).is_none_or(|v| !v.pending));

    let fx = client.respond(
        id,
        &request,
        json!({"status": "pending", "delete_after": "2026-12-13T10:00:00.000Z", "purging": false}),
    );
    assert!(client.state.session().is_none());
    assert!(fx.contains(&Effect::ClearToken));
    assert!(fx.iter().any(|e| matches!(e, Effect::Revoke(_))));
}

#[test]
fn a_refused_request_should_say_why() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::Deletion(DeletionCommand::Request {
        confirm_username: "me".into(),
    }));
    let request = deletion(DeletionRequest::Request {
        confirm_username: "me".into(),
    });
    let id = request_for(&fx, &request);
    let fx = client.fail(
        id,
        ApiError::Deletion(DeletionRefusal::OwnsServers(vec!["Lab".into()])),
    );
    let shown = view(&fx).expect("view");
    assert_eq!(
        shown.delete_error,
        "You own Lab. Delete it in server settings first."
    );
    assert!(!shown.deleting);
    assert!(client.state.session().is_some());
    let fx = client.command(Command::Deletion(DeletionCommand::Dismiss));
    assert!(view(&fx).expect("view").delete_error.is_empty());
}

#[test]
fn a_failed_request_after_a_refusal_should_hold() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::Deletion(DeletionCommand::Request {
        confirm_username: "me".into(),
    }));
    let request = deletion(DeletionRequest::Request {
        confirm_username: "me".into(),
    });
    let id = request_for(&fx, &request);
    client.input(Input::Gateway {
        session: client.state.session().expect("session").id,
        event: GatewayEvent::PendingDeletion,
    });
    // The answer was lost, but the account is pending: the restore screen.
    let fx = client.fail(id, ApiError::Network("timed out".into()));
    request_for(&fx, &deletion(DeletionRequest::Status));
    assert!(view(&fx).expect("view").pending);
}
