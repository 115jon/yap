//! A phone's push token (`/api/push-tokens`) through sign-in and sign-out,
//! and the background suspend that closes the gateway after a grace.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::ApiError;
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, Command, Effect, GatewayEvent, HttpReason, Input, RequestId, State,
    ViewUpdate, SUSPEND_GRACE,
};

const BASE: &str = "https://api.fixture.invalid";
const FCM: &str = "fcm_token_fixture";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn register(token: &str) -> Command {
    Command::RegisterPush {
        token: token.into(),
        platform: "android".into(),
        format: Some("native".into()),
        app_variant: Some("qt".into()),
    }
}

fn registered(token: &str) -> ApiRequest {
    ApiRequest::RegisterPush {
        token: token.into(),
        platform: "android".into(),
        app_variant: Some("qt".into()),
        push_format: Some("native".into()),
    }
}

fn push_requests(fx: &[Effect]) -> Vec<(ApiRequest, HttpReason)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Http {
                request: request @ (ApiRequest::RegisterPush { .. } | ApiRequest::UnregisterPush { .. }),
                reason,
                ..
            } => Some((request.clone(), *reason)),
            _ => None,
        })
        .collect()
}

fn request_for(fx: &[Effect], wanted: &ApiRequest) -> RequestId {
    fx.iter()
        .find_map(|e| match e {
            Effect::Http { id, request, .. } if request == wanted => Some(*id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no {wanted:?} in {fx:#?}"))
}

fn later(fx: &[Effect]) -> Option<(std::time::Duration, Input)> {
    fx.iter().find_map(|e| match e {
        Effect::Later { delay, input } => Some((*delay, (**input).clone())),
        _ => None,
    })
}

fn count(fx: &[Effect], pred: impl Fn(&Effect) -> bool) -> usize {
    fx.iter().filter(|e| pred(e)).count()
}

struct Client {
    state: State,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        reduce(&mut self.state, input)
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// A token pasted in, before `me` answers.
    fn booting() -> (Self, RequestId) {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        let me = request_for(&fx, &ApiRequest::Me);
        (client, me)
    }

    /// `me` answered: the session is the account's. Returns that input's effects.
    fn me(&mut self, id: RequestId) -> Vec<Effect> {
        let result = ApiRequest::Me.decode(fixture("user-me.json"));
        self.input(Input::Http { id, result })
    }

    fn signed_in() -> Self {
        let (mut client, me) = Self::booting();
        client.me(me);
        client
    }
}

// The token

#[test]
fn a_token_given_while_booting_should_register_once_me_answers() {
    let (mut client, me) = Client::booting();
    let fx = client.command(register(FCM));
    assert!(push_requests(&fx).is_empty(), "not before me: {fx:#?}");
    let fx = client.me(me);
    assert_eq!(push_requests(&fx), vec![(registered(FCM), HttpReason::Other)]);
}

#[test]
fn a_token_given_when_signed_in_should_register_at_once_and_only_once() {
    let mut client = Client::signed_in();
    let fx = client.command(register(FCM));
    assert_eq!(push_requests(&fx), vec![(registered(FCM), HttpReason::Other)]);
    let fx = client.command(register(FCM));
    assert!(push_requests(&fx).is_empty(), "the same token again: {fx:#?}");
}

#[test]
fn a_rotated_token_should_register_again() {
    let mut client = Client::signed_in();
    client.command(register(FCM));
    let fx = client.command(register("fcm_token_rotated"));
    assert_eq!(
        push_requests(&fx),
        vec![(registered("fcm_token_rotated"), HttpReason::Other)]
    );
}

#[test]
fn the_register_body_should_match_what_the_expo_app_sends() {
    let request = registered(FCM);
    assert_eq!(request.path(), "/api/push-tokens");
    assert_eq!(
        request.body(),
        json!({
            "token": FCM,
            "platform": "android",
            "app_variant": "qt",
            "push_format": "native",
        })
    );
    let bare = ApiRequest::RegisterPush {
        token: FCM.into(),
        platform: "android".into(),
        app_variant: None,
        push_format: None,
    };
    assert_eq!(bare.body(), json!({ "token": FCM, "platform": "android" }));
    let gone = ApiRequest::UnregisterPush { token: FCM.into() };
    assert_eq!(gone.path(), "/api/push-tokens");
    assert_eq!(gone.body(), json!({ "token": FCM }));
}

#[test]
fn sign_out_should_unregister_before_it_revokes_and_register_the_next_account() {
    let mut client = Client::signed_in();
    client.command(register(FCM));
    let fx = client.command(Command::SignOut);
    assert_eq!(
        push_requests(&fx),
        vec![(
            ApiRequest::UnregisterPush { token: FCM.into() },
            HttpReason::SignOut
        )]
    );
    let unregister = fx
        .iter()
        .position(|e| matches!(e, Effect::Http { request: ApiRequest::UnregisterPush { .. }, .. }))
        .expect("an unregister");
    let revoke = fx
        .iter()
        .position(|e| matches!(e, Effect::Revoke(_)))
        .expect("a revoke");
    assert!(unregister < revoke, "the DELETE goes first: {fx:#?}");
    // With the old session's token, which the revoke then ends.
    assert!(matches!(
        &fx[unregister],
        Effect::Http { token, .. } if token.as_str() == "tok_fixture"
    ));

    // The next account gets the same device's pushes.
    let fx = client.command(Command::UseToken("tok_next".into()));
    let me = request_for(&fx, &ApiRequest::Me);
    let fx = client.me(me);
    assert_eq!(push_requests(&fx), vec![(registered(FCM), HttpReason::Other)]);
}

#[test]
fn sign_out_without_a_registered_token_should_not_unregister() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::SignOut);
    assert!(push_requests(&fx).is_empty(), "{fx:#?}");
    assert_eq!(count(&fx, |e| matches!(e, Effect::Revoke(_))), 1);
}

#[test]
fn unregister_should_forget_the_token() {
    let mut client = Client::signed_in();
    client.command(register(FCM));
    let fx = client.command(Command::UnregisterPush);
    assert_eq!(
        push_requests(&fx),
        vec![(
            ApiRequest::UnregisterPush { token: FCM.into() },
            HttpReason::SignOut
        )]
    );
    // Not registered again for the next account.
    client.command(Command::SignOut);
    let fx = client.command(Command::UseToken("tok_next".into()));
    let me = request_for(&fx, &ApiRequest::Me);
    assert!(push_requests(&client.me(me)).is_empty());
}

// The background suspend

#[test]
fn the_background_should_close_the_gateway_after_the_grace_and_the_front_reconnect() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let fx = client.command(Command::Suspend(true));
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 0);
    let (delay, grace) = later(&fx).expect("a grace timer");
    assert_eq!(delay, SUSPEND_GRACE);

    let fx = client.input(grace);
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 1, "{fx:#?}");
    assert!(fx.contains(&Effect::View(ViewUpdate::Connected(false))));

    let before = client.state.session().expect("session").id;
    let fx = client.command(Command::Suspend(false));
    let connects: Vec<_> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::GatewayConnect { session, .. } => Some(*session),
            _ => None,
        })
        .collect();
    assert_eq!(connects.len(), 1, "{fx:#?}");
    assert_ne!(connects[0], before, "a fresh connection id");
    // The new socket's Ready refetches what is on screen (a reconnect).
    let fx = client.gateway(GatewayEvent::Ready);
    assert!(fx.contains(&Effect::View(ViewUpdate::Connected(true))));
}

#[test]
fn back_in_front_within_the_grace_should_keep_the_gateway() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let (_, grace) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    let fx = client.command(Command::Suspend(false));
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayConnect { .. })), 0);
    let fx = client.input(grace);
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 0, "{fx:#?}");
}

#[test]
fn back_in_front_with_the_link_down_should_reconnect_at_once() {
    // The socket dropped in the background (the phone blocked the app's
    // network) and its retries are backing off.
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let (_, grace) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    client.gateway(GatewayEvent::Closed);
    client.gateway(GatewayEvent::Reconnecting {
        attempt: 5,
        offline: true,
    });
    let before = client.state.session().expect("session").id;
    let fx = client.command(Command::Suspend(false));
    let connects: Vec<_> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::GatewayConnect { session, .. } => Some(*session),
            _ => None,
        })
        .collect();
    assert_eq!(connects.len(), 1, "{fx:#?}");
    assert_ne!(connects[0], before, "a fresh connection, its backoff from the start");
    assert!(fx.contains(&Effect::View(ViewUpdate::Link {
        attempt: 0,
        offline: true,
    })));
    // The old socket's late retry no longer counts.
    let fx = client.input(Input::Gateway {
        session: before,
        event: GatewayEvent::Reconnecting {
            attempt: 6,
            offline: true,
        },
    });
    assert!(fx.is_empty(), "{fx:#?}");
    // And the grace that was running no longer closes anything.
    assert_eq!(
        count(&client.input(grace), |e| matches!(e, Effect::GatewayDisconnect)),
        0
    );
    let fx = client.gateway(GatewayEvent::Ready);
    assert!(fx.contains(&Effect::View(ViewUpdate::Connected(true))));
}

#[test]
fn back_in_front_while_still_connecting_the_first_time_should_not_restart() {
    let mut client = Client::signed_in();
    client.command(Command::Suspend(true));
    let fx = client.command(Command::Suspend(false));
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayConnect { .. })), 0);
}

#[test]
fn an_older_grace_should_not_close_the_gateway() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let (_, first) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    client.command(Command::Suspend(false));
    let (_, second) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    assert_eq!(
        count(&client.input(first), |e| matches!(e, Effect::GatewayDisconnect)),
        0
    );
    assert_eq!(
        count(&client.input(second), |e| matches!(e, Effect::GatewayDisconnect)),
        1
    );
}

#[test]
fn a_late_event_of_the_closed_socket_should_not_stop_the_resume() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let old = client.state.session().expect("session").id;
    let (_, grace) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    client.input(grace);
    // Queued by the old socket before it was dropped.
    client.input(Input::Gateway {
        session: old,
        event: GatewayEvent::Ready,
    });
    let fx = client.command(Command::Suspend(false));
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayConnect { .. })), 1);
}

#[test]
fn a_hidden_window_should_keep_the_gateway() {
    // The desktop: WindowShown only, never Suspend.
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let fx = client.command(Command::WindowShown(false));
    if let Some((_, grace)) = later(&fx) {
        let fx = client.input(grace);
        assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 0);
    }
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 0);
}

#[test]
fn a_sign_out_in_the_background_should_not_reconnect_on_resume() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let (_, grace) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    client.input(grace);
    client.command(Command::SignOut);
    let fx = client.command(Command::Suspend(false));
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayConnect { .. })), 0);
}

#[test]
fn a_failed_registration_should_try_again_at_the_next_ready() {
    let mut client = Client::signed_in();
    let fx = client.command(register(FCM));
    let id = request_for(&fx, &registered(FCM));
    client.input(Input::Http {
        id,
        result: Err(ApiError::Network("offline".into())),
    });
    let fx = client.gateway(GatewayEvent::Ready);
    assert_eq!(push_requests(&fx), vec![(registered(FCM), HttpReason::Other)]);
    // Answered this time: no more tries.
    let id = request_for(&fx, &registered(FCM));
    client.input(Input::Http {
        id,
        result: Ok(ApiResponse::Done),
    });
    client.gateway(GatewayEvent::Closed);
    assert!(push_requests(&client.gateway(GatewayEvent::Ready)).is_empty());
}

#[test]
fn a_grace_while_booting_should_ask_again_later() {
    let (mut client, me) = Client::booting();
    let (_, grace) = later(&client.command(Command::Suspend(true))).expect("a grace timer");
    let fx = client.input(grace);
    assert_eq!(count(&fx, |e| matches!(e, Effect::GatewayDisconnect)), 0);
    let (delay, again) = later(&fx).expect("another grace");
    assert_eq!(delay, SUSPEND_GRACE);
    client.me(me);
    client.gateway(GatewayEvent::Ready);
    assert_eq!(
        count(&client.input(again), |e| matches!(e, Effect::GatewayDisconnect)),
        1
    );
}

// A notification's tap at launch

const DM: &str = "dm_from_push";

fn dms() -> Value {
    json!([{ "id": DM, "channel_type": "dm", "name": "x", "created_at": null,
             "recipient": { "id": "user_ada", "username": "ada", "display_name": "Ada",
                            "avatar_url": null, "status": "online", "custom_status": null } }])
}

#[test]
fn a_dm_opened_before_the_dm_list_is_in_should_open_when_it_answers() {
    let (mut client, me) = Client::booting();
    let fx = client.me(me);
    let list = request_for(&fx, &ApiRequest::Dms);
    client.command(Command::OpenChannel {
        channel_id: DM.into(),
        server_id: None,
    });
    assert_ne!(client.state.active_channel(), Some(DM), "not before the list");
    let result = ApiRequest::Dms.decode(dms());
    client.input(Input::Http { id: list, result });
    assert_eq!(client.state.active_channel(), Some(DM));
}

#[test]
fn picking_something_else_first_should_drop_the_wanted_dm() {
    let (mut client, me) = Client::booting();
    let fx = client.me(me);
    let list = request_for(&fx, &ApiRequest::Dms);
    client.command(Command::OpenChannel {
        channel_id: DM.into(),
        server_id: None,
    });
    client.command(Command::SelectHome);
    let result = ApiRequest::Dms.decode(dms());
    client.input(Input::Http { id: list, result });
    assert_ne!(client.state.active_channel(), Some(DM));
}
