//! The link's error states (UI parity pass 13): the gateway's retries as
//! the connection strip reads them, a launch that cannot reach the server,
//! and a channel page the server refuses.

use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use yap_core::store::{reduce, ApiRequest, Command, Effect, GatewayEvent, Input, Phase, RequestId};
use yap_core::store::{State, ViewUpdate};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const DNS: &str = "error sending request: client error (Connect): dns error: no network";
const REFUSED: &str = "error sending request: client error (Connect): tcp connect error: refused";

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

fn asked_for(fx: &[Effect], wanted: &ApiRequest) -> bool {
    fx.iter()
        .any(|e| matches!(e, Effect::Http { request, .. } if request == wanted))
}

fn links(fx: &[Effect]) -> Vec<(u32, bool)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Link { attempt, offline }) => Some((*attempt, *offline)),
            _ => None,
        })
        .collect()
}

fn connects(fx: &[Effect]) -> usize {
    fx.iter()
        .filter(|e| matches!(e, Effect::GatewayConnect { .. }))
        .count()
}

fn messages() -> ApiRequest {
    ApiRequest::Messages {
        channel_id: TEXT.into(),
        before: None,
    }
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

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// A launch with a stored token and no cached view: me + servers asked.
    fn launch() -> (Self, Vec<Effect>) {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.input(Input::User(Command::UseToken("tok_fixture".into())));
        (client, fx)
    }

    /// Signed in, before the channel's first page answers.
    fn opening() -> (Self, RequestId) {
        let (mut client, fx) = Self::launch();
        let me = request_for(&fx, &ApiRequest::Me);
        let servers = request_for(&fx, &ApiRequest::Servers);
        client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        let fx = client.respond(
            request_for(&fx, &channels),
            &channels,
            fixture("channels-response.json"),
        );
        let page = request_for(&fx, &messages());
        (client, page)
    }

    fn signed_in() -> Self {
        let (mut client, page) = Self::opening();
        client.respond(page, &messages(), fixture("messages-page.json"));
        client
    }
}

// The gateway's retries

#[test]
fn a_scheduled_retry_should_reach_the_strip() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Reconnecting {
        attempt: 3,
        offline: false,
    });
    assert_eq!(links(&fx), vec![(3, false)]);
    let fx = client.gateway(GatewayEvent::Reconnecting {
        attempt: 4,
        offline: true,
    });
    assert_eq!(links(&fx), vec![(4, true)]);
}

#[test]
fn ready_should_clear_the_retries() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Reconnecting {
        attempt: 2,
        offline: true,
    });
    let fx = client.gateway(GatewayEvent::Ready);
    assert_eq!(links(&fx), vec![(0, false)]);
    // Nothing changed: nothing is sent again.
    let fx = client.gateway(GatewayEvent::Resumed);
    assert!(links(&fx).is_empty(), "{fx:#?}");
}

#[test]
fn a_retry_before_the_first_ready_should_mark_the_link_down() {
    let mut client = Client::signed_in();
    let fx = client.gateway(GatewayEvent::Reconnecting {
        attempt: 1,
        offline: true,
    });
    assert_eq!(links(&fx), vec![(1, true)]);
    assert!(!fx.contains(&Effect::View(ViewUpdate::Connected(true))));
}

#[test]
fn retry_should_connect_the_gateway_now() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Reconnecting {
        attempt: 6,
        offline: false,
    });
    let fx = client.input(Input::User(Command::RetryGateway));
    assert_eq!(connects(&fx), 1, "{fx:#?}");
    assert_eq!(links(&fx), vec![(0, false)]);
}

#[test]
fn a_late_event_from_the_connection_a_retry_replaced_should_be_ignored() {
    let mut client = Client::signed_in();
    let old = client.state.session().expect("session").id;
    client.gateway(GatewayEvent::Reconnecting {
        attempt: 5,
        offline: false,
    });
    client.input(Input::User(Command::RetryGateway));
    // Queued by the old connection before it was dropped.
    let fx = client.input(Input::Gateway {
        session: old,
        event: GatewayEvent::Reconnecting {
            attempt: 6,
            offline: false,
        },
    });
    assert!(links(&fx).is_empty(), "{fx:#?}");
    // The new connection's own events still land.
    let fx = client.gateway(GatewayEvent::Ready);
    assert!(fx.contains(&Effect::View(ViewUpdate::Connected(true))));
}

#[test]
fn signing_out_during_a_drop_should_clear_the_strip() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Reconnecting {
        attempt: 6,
        offline: true,
    });
    let fx = client.input(Input::User(Command::SignOut));
    assert_eq!(links(&fx), vec![(0, false)], "{fx:#?}");
}

#[test]
fn retry_should_do_nothing_while_the_gateway_is_up() {
    let mut client = Client::signed_in();
    client.gateway(GatewayEvent::Ready);
    let fx = client.input(Input::User(Command::RetryGateway));
    assert_eq!(connects(&fx), 0, "{fx:#?}");
}

// A launch that cannot reach the server

#[test]
fn no_network_at_launch_should_keep_booting_and_ask_again() {
    let (mut client, fx) = Client::launch();
    let me = request_for(&fx, &ApiRequest::Me);
    let fx = client.fail(me, ApiError::Network(DNS.into()));
    assert_eq!(client.state.phase(), Phase::Loading);
    assert!(client.state.session().is_some());
    assert_eq!(links(&fx), vec![(1, true)]);
    let session = client.state.session().expect("session").id;
    assert!(fx.contains(&Effect::Later {
        delay: Duration::from_secs(1),
        input: Box::new(Input::BootRetry { session }),
    }));

    let fx = client.input(Input::BootRetry { session });
    let me = request_for(&fx, &ApiRequest::Me);
    // The servers request from the launch is still out: not asked twice.
    assert!(!asked_for(&fx, &ApiRequest::Servers), "{fx:#?}");
    let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    assert_eq!(connects(&fx), 1, "{fx:#?}");
    // "Waiting for network" ends; the gateway reports its own tries.
    assert_eq!(links(&fx), vec![(0, false)]);
}

#[test]
fn launch_retries_should_back_off() {
    let (mut client, fx) = Client::launch();
    let session = client.state.session().expect("session").id;
    let mut me = request_for(&fx, &ApiRequest::Me);
    let mut delays = Vec::new();
    for _ in 0..7 {
        let fx = client.fail(me, ApiError::Network(REFUSED.into()));
        delays.extend(fx.iter().filter_map(|e| match e {
            Effect::Later { delay, .. } => Some(delay.as_secs()),
            _ => None,
        }));
        me = request_for(&client.input(Input::BootRetry { session }), &ApiRequest::Me);
    }
    assert_eq!(delays, vec![1, 2, 4, 8, 16, 30, 30]);
    // The server is down, not the network.
    assert!(!client.state.session().expect("session").gateway.offline);
}

#[test]
fn a_failed_servers_answer_should_be_asked_again_on_the_retry() {
    let (mut client, fx) = Client::launch();
    let session = client.state.session().expect("session").id;
    let me = request_for(&fx, &ApiRequest::Me);
    let servers = request_for(&fx, &ApiRequest::Servers);
    client.fail(servers, ApiError::Network(DNS.into()));
    client.fail(me, ApiError::Network(DNS.into()));
    let fx = client.input(Input::BootRetry { session });
    assert!(asked_for(&fx, &ApiRequest::Me));
    assert!(asked_for(&fx, &ApiRequest::Servers), "{fx:#?}");
}

#[test]
fn a_server_error_at_launch_should_retry_too() {
    let (mut client, fx) = Client::launch();
    let me = request_for(&fx, &ApiRequest::Me);
    client.fail(
        me,
        ApiError::Http {
            status: 503,
            message: "unavailable".into(),
        },
    );
    assert_eq!(client.state.phase(), Phase::Loading);
}

#[test]
fn an_ended_session_at_launch_should_still_sign_out() {
    let (mut client, fx) = Client::launch();
    let me = request_for(&fx, &ApiRequest::Me);
    let fx = client.fail(me, ApiError::Unauthorized);
    assert_eq!(client.state.phase(), Phase::SignedOut);
    assert!(fx.contains(&Effect::ClearToken));
}

#[test]
fn a_stale_boot_retry_should_be_ignored() {
    let (mut client, fx) = Client::launch();
    let session = client.state.session().expect("session").id;
    let me = request_for(&fx, &ApiRequest::Me);
    client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let fx = client.input(Input::BootRetry { session });
    assert!(!asked_for(&fx, &ApiRequest::Me), "{fx:#?}");
}

// A channel page the server refuses

#[test]
fn a_gone_channel_should_say_so_in_the_webs_words() {
    let (mut client, page) = Client::opening();
    client.fail(
        page,
        ApiError::Http {
            status: 404,
            message: "Channel not found".into(),
        },
    );
    assert_eq!(client.state.phase(), Phase::Ready);
    assert_eq!(
        client.state.status(),
        "Unable to open this channel. It may have been deleted."
    );
}

#[test]
fn a_page_lost_to_the_network_should_leave_the_header_to_the_strip() {
    let (mut client, page) = Client::opening();
    client.fail(page, ApiError::Network(DNS.into()));
    assert_eq!(client.state.phase(), Phase::Ready);
    assert_eq!(client.state.status(), "");
}

#[test]
fn a_rate_limited_page_should_ask_to_wait() {
    let (mut client, page) = Client::opening();
    client.fail(
        page,
        ApiError::Http {
            status: 429,
            message: "Rate limit exceeded. Please slow down.".into(),
        },
    );
    assert_eq!(
        client.state.status(),
        "Unable to open this channel right now. Wait a moment and try again."
    );
}
