//! Settings › Connections through the reducer, and the `connection` link the
//! browser hands back: the list, Connect (the begin URL to open), the claim
//! of a verified account, the outcomes, Unlink and Show on profile.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::connections::ConnectionsRequest;
use yap_core::links::{deep_link_arg, ConnectionOutcome, Link};
use yap_core::store::connections::{ConnectionsCommand, ConnectionsView};
use yap_core::store::{
    reduce, ApiRequest, Command, Effect, Input, InviteCommand, RequestId, State, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const CODE: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNO_-";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn connections(request: ConnectionsRequest) -> ApiRequest {
    ApiRequest::Connections(request)
}

fn list() -> ApiRequest {
    connections(ConnectionsRequest::List)
}

fn steam_row(show: bool) -> Value {
    json!({ "connections": [{
        "provider": "steam",
        "display_name": "Spike",
        "profile_url": "https://steamcommunity.com/profiles/76561197960287930",
        "avatar_url": "https://avatars.steamstatic.com/a_full.jpg",
        "show_on_profile": show,
        "verified_at": "2026-11-14T10:00:00.000Z",
    }]})
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    view: ConnectionsView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Connections(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        fx
    }

    fn user(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn command(&mut self, command: ConnectionsCommand) -> Vec<Effect> {
        self.user(Command::Connections(command))
    }

    fn asked_for(&self, request: &ApiRequest) -> Option<RequestId> {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self
            .asked_for(request)
            .unwrap_or_else(|| panic!("no {request:?}"));
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn fail(&mut self, request: &ApiRequest, error: ApiError) -> Vec<Effect> {
        let id = self
            .asked_for(request)
            .unwrap_or_else(|| panic!("no {request:?}"));
        self.input(Input::Http {
            id,
            result: Err(error),
        })
    }

    /// Signed in and loaded, with the list answered `body`.
    fn with_list(body: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            view: ConnectionsView::default(),
        };
        client.user(Command::UseToken("tok_fixture".into()));
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
        client.command(ConnectionsCommand::Load);
        client.respond(&list(), body);
        client
    }
}

#[test]
fn the_link_should_be_read_strictly() {
    let code = |url: &str| deep_link_arg(url);
    assert_eq!(
        code(&format!(
            "yap-native://connection?provider=steam&code={CODE}"
        )),
        Some(Link::Connection {
            provider: "steam".into(),
            outcome: ConnectionOutcome::Code(CODE.into()),
        })
    );
    assert_eq!(
        code("yap://connection?provider=steam&status=cancelled"),
        Some(Link::Connection {
            provider: "steam".into(),
            outcome: ConnectionOutcome::Status("cancelled".into()),
        })
    );
    for refused in [
        "yap://connection?provider=myspace&status=linked",
        "yap://connection?provider=steam&status=hacked",
        "yap://connection?provider=steam&code=short",
        "yap://connection?provider=steam",
        &format!("yap://connection?provider=steam&status=linked&code={CODE}"),
        "yap://connection/extra?provider=steam&status=linked",
        "yap://connection?provider=steam&status=linked&token=x",
    ] {
        assert_eq!(code(refused), None, "{refused}");
    }
}

#[test]
fn the_list_should_draw_one_card_per_provider() {
    let client = Client::with_list(json!({ "connections": [] }));
    assert!(client.view.loaded);
    assert_eq!(client.view.cards.len(), 1);
    assert_eq!(client.view.cards[0].label, "Steam");
    assert!(!client.view.cards[0].linked);

    let client = Client::with_list(steam_row(true));
    let card = &client.view.cards[0];
    assert!(card.linked && card.show_on_profile);
    assert_eq!(card.display_name, "Spike");
    assert!(card.verified.contains("2026"), "{}", card.verified);
}

#[test]
fn connect_should_hand_the_begin_url_to_the_ui() {
    let mut client = Client::with_list(json!({ "connections": [] }));
    client.command(ConnectionsCommand::Connect {
        provider: "steam".into(),
        app: "android".into(),
    });
    let start = connections(ConnectionsRequest::Start {
        provider: "steam".into(),
        app: "android".into(),
    });
    assert_eq!(client.view.busy, "connect:steam");
    let seq = client.view.open_seq;
    client.respond(
        &start,
        json!({ "url": "https://yap.example/api/connections/steam/begin?x=1" }),
    );
    assert_eq!(client.view.busy, "");
    assert_eq!(
        client.view.open_url,
        "https://yap.example/api/connections/steam/begin?x=1"
    );
    assert_eq!(client.view.open_seq, seq + 1);
}

#[test]
fn a_code_coming_back_should_be_claimed_then_the_list_reloaded() {
    let mut client = Client::with_list(json!({ "connections": [] }));
    let shown = client.view.show_seq;
    client.user(Command::Invites(InviteCommand::OpenLink(
        Link::Connection {
            provider: "steam".into(),
            outcome: ConnectionOutcome::Code(CODE.into()),
        },
    )));
    assert_eq!(client.view.show_seq, shown + 1);
    let complete = connections(ConnectionsRequest::Complete {
        provider: "steam".into(),
        code: CODE.into(),
    });
    client.respond(
        &complete,
        json!({ "provider": "steam", "status": "linked" }),
    );
    assert_eq!(client.view.notice, "Steam connected");
    assert_eq!(client.view.notice_tone, "success");
    client.respond(&list(), steam_row(true));
    assert!(client.view.cards[0].linked);
}

#[test]
fn a_refused_claim_should_say_so() {
    let mut client = Client::with_list(json!({ "connections": [] }));
    client.user(Command::Invites(InviteCommand::OpenLink(
        Link::Connection {
            provider: "steam".into(),
            outcome: ConnectionOutcome::Code(CODE.into()),
        },
    )));
    let complete = connections(ConnectionsRequest::Complete {
        provider: "steam".into(),
        code: CODE.into(),
    });
    client.fail(
        &complete,
        ApiError::Http {
            status: 403,
            message: "This sign-in was started from another Yap account.".into(),
        },
    );
    assert_eq!(client.view.notice_tone, "error");
    assert!(client.view.notice.contains("Steam"));
}

#[test]
fn a_taken_or_cancelled_outcome_should_read_as_the_web_words_it() {
    let mut client = Client::with_list(json!({ "connections": [] }));
    client.user(Command::Invites(InviteCommand::OpenLink(
        Link::Connection {
            provider: "steam".into(),
            outcome: ConnectionOutcome::Status("cancelled".into()),
        },
    )));
    assert_eq!(client.view.notice_tone, "info");
    assert_eq!(
        client.view.notice,
        "Steam wasn't connected. You cancelled the sign-in."
    );
    client.command(ConnectionsCommand::DismissNotice);
    assert_eq!(client.view.notice, "");
}

#[test]
fn unlink_should_drop_the_card_and_a_refusal_should_say_why() {
    let mut client = Client::with_list(steam_row(true));
    client.command(ConnectionsCommand::Unlink("steam".into()));
    let unlink = connections(ConnectionsRequest::Unlink {
        provider: "steam".into(),
    });
    client.respond(&unlink, json!({ "provider": "steam", "removed": true }));
    assert!(!client.view.cards[0].linked);

    let mut client = Client::with_list(steam_row(true));
    client.command(ConnectionsCommand::Unlink("steam".into()));
    client.fail(&unlink, ApiError::Network("offline".into()));
    assert!(client.view.cards[0].linked);
    assert!(!client.view.action_error.is_empty());
}

#[test]
fn show_on_profile_should_flip_at_once_and_back_on_a_refusal() {
    let mut client = Client::with_list(steam_row(true));
    client.command(ConnectionsCommand::SetShown {
        provider: "steam".into(),
        show: false,
    });
    assert!(!client.view.cards[0].show_on_profile);
    let patch = connections(ConnectionsRequest::SetShown {
        provider: "steam".into(),
        show: false,
    });
    client.fail(&patch, ApiError::Network("offline".into()));
    assert!(client.view.cards[0].show_on_profile);
    assert!(!client.view.action_error.is_empty());
}

#[test]
fn signing_out_should_forget_the_list() {
    let mut client = Client::with_list(steam_row(true));
    client.user(Command::SignOut);
    assert!(!client.view.loaded);
}
