//! The site announcement banner through the reducer: fetched once the
//! account is known, refreshed on a timer and when the app is back on
//! screen, dismissed per announcement (never a critical one).

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::site_config::{Announcement, DISMISSED_FILE, REFRESH};
use yap_core::store::site::SiteCommand;
use yap_core::store::{reduce, ApiRequest, Command, Effect, Input, RequestId, State, ViewUpdate};
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

fn find_request(fx: &[Effect], wanted: &ApiRequest) -> Option<RequestId> {
    fx.iter().find_map(|e| match e {
        Effect::Http { id, request, .. } if request == wanted => Some(*id),
        _ => None,
    })
}

fn site_request(fx: &[Effect]) -> RequestId {
    find_request(fx, &ApiRequest::SiteConfig)
        .unwrap_or_else(|| panic!("no site config request in {fx:#?}"))
}

/// The last announcement view in `fx`: `None` when none was sent,
/// `Some(None)` when it went.
fn shown(fx: &[Effect]) -> Option<Option<Announcement>> {
    fx.iter().rev().find_map(|e| match e {
        Effect::View(ViewUpdate::Announcement(view)) => Some(view.clone()),
        _ => None,
    })
}

fn refresh_timer(fx: &[Effect]) -> Option<Input> {
    fx.iter().find_map(|e| match e {
        Effect::Later { delay, input } if matches!(**input, Input::SiteRefresh { .. }) => {
            assert_eq!(*delay, REFRESH);
            Some((**input).clone())
        }
        _ => None,
    })
}

fn config(id: &str, tone: &str) -> Value {
    json!({
        "flags": {},
        "announcement": {
            "id": id,
            "text": "Maintenance tonight",
            "tone": tone,
            "link_url": "https://status.example/now",
            "link_label": null,
            "expires_at": null,
            "updated_at": "2026-11-01T00:00:00.000Z",
        }
    })
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

    fn respond(&mut self, id: RequestId, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    /// Signed in: `users/me` answered. The effects of that answer.
    fn signed_in() -> (Self, Vec<Effect>) {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        assert_eq!(find_request(&fx, &ApiRequest::SiteConfig), None);
        let me = find_request(&fx, &ApiRequest::Me).expect("me");
        let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        (client, fx)
    }

    /// Signed in, with `body` as the site config's answer.
    fn showing(body: Value) -> (Self, Vec<Effect>) {
        let (mut client, fx) = Self::signed_in();
        let id = site_request(&fx);
        let fx = client.respond(id, &ApiRequest::SiteConfig, body);
        (client, fx)
    }
}

#[test]
fn signing_in_should_ask_for_the_site_config_and_show_its_announcement() {
    let (_, fx) = Client::showing(config("a1", "warning"));
    let view = shown(&fx).flatten().expect("announcement");
    assert_eq!(view.id, "a1");
    assert_eq!(view.tone, "warning");
    assert_eq!(view.link_url, "https://status.example/now");
    assert_eq!(view.link_label, "Read more");
    assert!(refresh_timer(&fx).is_some(), "{fx:#?}");
}

#[test]
fn no_announcement_should_show_nothing() {
    let (client, fx) = Client::showing(json!({ "flags": {}, "announcement": null }));
    assert_eq!(shown(&fx), None);
    assert!(client.state.session().is_some());
}

#[test]
fn the_refresh_timer_should_ask_again_and_replace_the_announcement() {
    let (mut client, fx) = Client::showing(config("a1", "info"));
    let timer = refresh_timer(&fx).expect("timer");
    let fx = client.input(timer);
    let id = site_request(&fx);
    let fx = client.respond(id, &ApiRequest::SiteConfig, json!({ "announcement": null }));
    assert_eq!(shown(&fx), Some(None));
}

#[test]
fn a_failed_refresh_should_keep_what_shows_and_try_again_later() {
    let (mut client, fx) = Client::showing(config("a1", "info"));
    let fx = client.input(refresh_timer(&fx).expect("timer"));
    let id = site_request(&fx);
    let fx = client.input(Input::Http {
        id,
        result: Err(ApiError::Network("offline".into())),
    });
    assert_eq!(shown(&fx), None, "nothing changes");
    assert!(refresh_timer(&fx).is_some());
}

#[test]
fn a_stale_timer_should_do_nothing() {
    let (mut client, fx) = Client::showing(config("a1", "info"));
    let stale = refresh_timer(&fx).expect("timer");
    // Back on screen: asked again, which starts a newer timer.
    client.command(Command::WindowShown(false));
    let fx = client.command(Command::WindowShown(true));
    let id = site_request(&fx);
    client.respond(id, &ApiRequest::SiteConfig, config("a1", "info"));
    let fx = client.input(stale);
    assert_eq!(find_request(&fx, &ApiRequest::SiteConfig), None);
}

#[test]
fn a_hidden_app_should_wait_until_it_is_back_on_screen() {
    let (mut client, fx) = Client::showing(config("a1", "info"));
    client.command(Command::Suspend(true));
    let fx = client.input(refresh_timer(&fx).expect("timer"));
    assert_eq!(find_request(&fx, &ApiRequest::SiteConfig), None);
    let fx = client.command(Command::Suspend(false));
    site_request(&fx);
}

#[test]
fn dismissing_should_hide_it_and_keep_the_id_on_this_device() {
    let (mut client, _) = Client::showing(config("a1", "warning"));
    let fx = client.command(Command::Site(SiteCommand::Dismiss));
    assert_eq!(shown(&fx), Some(None));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::SaveLocal { file, contents }
            if *file == DISMISSED_FILE && contents.contains("\"a1\"")
    )));
    // The same announcement again stays hidden; a new one shows.
    client.command(Command::WindowShown(false));
    let fx = client.command(Command::WindowShown(true));
    let id = site_request(&fx);
    let fx = client.respond(id, &ApiRequest::SiteConfig, config("a1", "warning"));
    assert_eq!(shown(&fx), None);
    client.command(Command::WindowShown(false));
    let fx = client.command(Command::WindowShown(true));
    let id = site_request(&fx);
    let fx = client.respond(id, &ApiRequest::SiteConfig, config("a2", "warning"));
    assert_eq!(shown(&fx).flatten().map(|a| a.id), Some("a2".into()));
}

#[test]
fn a_dismissal_from_an_earlier_run_should_hide_that_announcement() {
    let mut client = Client {
        state: State::new(BASE),
    };
    client.command(Command::Site(SiteCommand::Dismissed("a1".into())));
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let me = find_request(&fx, &ApiRequest::Me).expect("me");
    let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let id = site_request(&fx);
    let fx = client.respond(id, &ApiRequest::SiteConfig, config("a1", "info"));
    assert_eq!(shown(&fx), None);
}

#[test]
fn a_critical_announcement_cannot_be_dismissed() {
    let mut client = Client {
        state: State::new(BASE),
    };
    // Even one dismissed while it was a warning shows once it is critical.
    client.command(Command::Site(SiteCommand::Dismissed("a1".into())));
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let me = find_request(&fx, &ApiRequest::Me).expect("me");
    let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let id = site_request(&fx);
    let fx = client.respond(id, &ApiRequest::SiteConfig, config("a1", "critical"));
    assert!(shown(&fx).flatten().is_some_and(|a| a.critical()));
    let fx = client.command(Command::Site(SiteCommand::Dismiss));
    assert_eq!(shown(&fx), None);
    assert!(!fx.iter().any(|e| matches!(e, Effect::SaveLocal { .. })));
}

#[test]
fn signing_out_should_take_the_banner_down() {
    let (mut client, _) = Client::showing(config("a1", "info"));
    let fx = client.command(Command::SignOut);
    assert_eq!(shown(&fx), Some(None));
}

#[test]
fn a_reconnect_of_the_same_sign_in_should_keep_the_banner() {
    let (mut client, _) = Client::showing(config("a1", "warning"));
    // A minute in the background closes the gateway; back in front it
    // reconnects with a new session id.
    let fx = client.command(Command::Suspend(true));
    let grace = fx
        .iter()
        .find_map(|e| match e {
            Effect::Later { input, .. } if matches!(**input, Input::SuspendElapsed { .. }) => {
                Some((**input).clone())
            }
            _ => None,
        })
        .expect("grace");
    let mut seen = client.input(grace);
    seen.extend(client.command(Command::Suspend(false)));
    seen.extend(client.command(Command::RetryGateway));
    seen.extend(client.command(Command::RetryGateway));
    assert_eq!(shown(&seen), None, "the banner never went: {seen:#?}");
    let asked = seen
        .iter()
        .filter(|e| {
            matches!(
                e,
                Effect::Http {
                    request: ApiRequest::SiteConfig,
                    ..
                }
            )
        })
        .count();
    assert!(asked <= 1, "{asked} site config requests");
}
