//! `yap://` and `yap-native://` deep links: the strict parser (every form in
//! the `links` table under both schemes, and what it refuses), the Tauri
//! app's older `yap://invite` links, the run's test scheme, and what the
//! store does with the links the invite tests do not cover (Home, a
//! server, settings, the sign-in nudge).

use std::path::PathBuf;

use serde_json::Value;
use yap_core::links::{
    deep_link_arg, deep_link_arg_with, parse_link, test_scheme, user_code, Link,
};
use yap_core::store::invites::InviteView;
use yap_core::store::{
    reduce, ApiRequest, Command, Effect, Input, InviteCommand, RequestId, SignInEvent, State,
    ViewUpdate,
};

const BASE: &str = "https://api.fixture.invalid";
const SITE: &str = "https://yap.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";

fn arg(url: &str) -> Option<Link> {
    deep_link_arg(url)
}

fn channel(server: &str, channel: &str, message: Option<&str>) -> Option<Link> {
    Some(Link::Channel {
        server_id: server.into(),
        channel_id: channel.into(),
        message_id: message.map(Into::into),
    })
}

fn settings(panel: &str, row: &str) -> Option<Link> {
    Some(Link::Settings {
        panel: panel.into(),
        row: row.into(),
    })
}

// The forms

#[test]
fn invite_should_open_the_landing() {
    assert_eq!(
        arg("yap-native://invite/a1b2c3d4"),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn one_trailing_slash_should_be_fine() {
    assert_eq!(
        arg("yap-native://invite/a1b2c3d4/"),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn the_route_should_match_without_case() {
    assert_eq!(
        arg("YAP-NATIVE://Invite/a1b2c3d4"),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn chat_at_me_should_be_home() {
    assert_eq!(arg("yap-native://chat/@me"), Some(Link::Home));
    assert_eq!(arg("yap-native://chat/%40me"), Some(Link::Home));
}

#[test]
fn dm_should_name_the_channel_and_message() {
    assert_eq!(
        arg("yap-native://chat/@me/dm_1"),
        channel("@me", "dm_1", None)
    );
    assert_eq!(
        arg("yap-native://chat/%40me/dm_1?message=m_1"),
        channel("@me", "dm_1", Some("m_1"))
    );
}

#[test]
fn server_alone_should_be_a_server() {
    assert_eq!(
        arg("yap-native://chat/srv"),
        Some(Link::Server("srv".into()))
    );
}

#[test]
fn channel_and_message_should_be_read() {
    let uuid = "5f0c8a52-1d7e-4b1a-9c11-0f3a2b4c5d6e";
    assert_eq!(
        arg(&format!("yap-native://chat/{uuid}/chan")),
        channel(uuid, "chan", None)
    );
    assert_eq!(
        arg("yap-native://chat/srv/chan?message=msg_1"),
        channel("srv", "chan", Some("msg_1"))
    );
}

#[test]
fn settings_should_default_to_the_account_panel() {
    assert_eq!(arg("yap-native://settings"), settings("account", ""));
}

#[test]
fn settings_should_open_a_panel() {
    assert_eq!(arg("yap-native://settings/voice"), settings("voice", ""));
}

#[test]
fn a_settings_row_should_pick_its_own_panel() {
    // As the web's resolveSettingsLocation: the row is the more specific ask.
    assert_eq!(
        arg("yap-native://settings?row=sign-out"),
        settings("account", "sign-out")
    );
    assert_eq!(
        arg("yap-native://settings/voice?row=sign-out"),
        settings("account", "sign-out")
    );
}

#[test]
fn signin_should_carry_only_a_user_code() {
    assert_eq!(
        arg("yap-native://signin"),
        Some(Link::SignIn { user_code: None })
    );
    assert_eq!(
        arg("yap-native://signin?code=abcd-efgh"),
        Some(Link::SignIn {
            user_code: Some("ABCD-EFGH".into())
        })
    );
    assert_eq!(
        arg("yap-native://signin?code=ABCDEFGH"),
        Some(Link::SignIn {
            user_code: Some("ABCD-EFGH".into())
        })
    );
}

// What is refused

#[test]
fn unknown_routes_should_be_refused() {
    for url in [
        "yap-native://",
        "yap-native:///",
        "yap-native://open",
        "yap-native://room/abc",
        "yap-native://share/token",
        "yap-native://activate?code=ABCD-EFGH",
    ] {
        assert_eq!(arg(url), None, "{url}");
    }
}

#[test]
fn extra_or_empty_segments_should_be_refused() {
    for url in [
        "yap-native://invite/a/b",
        "yap-native://invite//a",
        "yap-native://chat/srv/chan/extra",
        "yap-native://chat//chan",
        "yap-native://settings/voice/extra",
        "yap-native://signin/now",
        "yap-native://invite/a1b2c3d4//",
    ] {
        assert_eq!(arg(url), None, "{url}");
    }
}

#[test]
fn unknown_repeated_or_empty_parameters_should_be_refused() {
    for url in [
        "yap-native://invite/a1b2c3d4?x=1",
        "yap-native://chat/srv?message=m",
        "yap-native://chat/srv/chan?message=",
        "yap-native://chat/srv/chan?message=a&message=b",
        "yap-native://chat/srv/chan?utm_source=x",
        "yap-native://settings?panel=voice",
        "yap-native://signin?code",
        "yap-native://signin?token=abc",
        "yap-native://signin?code=ABCD-EFGH&session=abc",
    ] {
        assert_eq!(arg(url), None, "{url}");
    }
}

#[test]
fn a_fragment_should_be_refused() {
    assert_eq!(arg("yap-native://invite/a1b2c3d4#x"), None);
}

#[test]
fn ids_outside_the_id_alphabet_should_be_refused() {
    for url in [
        "yap-native://chat/srv/ch%20an",
        "yap-native://chat/s.rv/chan",
        "yap-native://chat/srv/chan?message=..%2F..",
        "yap-native://chat/%E2%9C%93/chan",
        "yap-native://chat/srv/%zz",
    ] {
        assert_eq!(arg(url), None, "{url}");
    }
    let long = "a".repeat(129);
    assert_eq!(arg(&format!("yap-native://chat/{long}/chan")), None);
    assert!(arg(&format!("yap-native://chat/{}/chan", "a".repeat(128))).is_some());
}

#[test]
fn unknown_settings_should_be_refused() {
    assert_eq!(arg("yap-native://settings/nope"), None);
    assert_eq!(arg("yap-native://settings?row=nope"), None);
    assert_eq!(arg("yap-native://settings/nope?row=sign-out"), None);
}

#[test]
fn a_malformed_user_code_should_be_refused() {
    for code in [
        "ABCD",
        "ABCD-EFG",
        "ABCD-EFGHJ",
        "ABC0-EFGH",
        "ABCI-EFGH",
        "AB-CDEFGH",
    ] {
        assert_eq!(
            arg(&format!("yap-native://signin?code={code}")),
            None,
            "{code}"
        );
        assert_eq!(user_code(code), None, "{code}");
    }
}

#[test]
fn signin_and_settings_should_never_come_from_a_site_link() {
    assert_eq!(
        parse_link("https://yap.fixture.invalid/settings/voice", SITE),
        None
    );
    assert_eq!(parse_link("https://yap.fixture.invalid/signin", SITE), None);
    // Our own scheme is another matter: `yap://` is ours now.
    assert_eq!(
        parse_link("yap://signin", SITE),
        Some(Link::SignIn { user_code: None })
    );
}

#[test]
fn site_chat_links_should_read_as_the_web_reads_them() {
    assert_eq!(
        parse_link("https://yap.fixture.invalid/chat/srv", SITE),
        Some(Link::Server("srv".into()))
    );
    assert_eq!(
        parse_link("https://yap.fixture.invalid/chat/@me", SITE),
        Some(Link::Home)
    );
    // The web ignores later segments and other parameters.
    assert_eq!(
        parse_link(
            "https://yap.fixture.invalid/chat/srv/chan/x?utm=1&message=m#top",
            SITE
        ),
        channel("srv", "chan", Some("m"))
    );
}

#[test]
fn another_scheme_should_never_be_a_deep_link() {
    for url in [
        "yap-native-test://invite/a1b2c3d4",
        "yapp://invite/a1b2c3d4",
        "yap-nativex://invite/a1b2c3d4",
        "ralphmeet://invite/a1b2c3d4",
        "https://yap.fixture.invalid/invite/a1b2c3d4",
        "yap-native:invite/a1b2c3d4",
        "yap:invite/a1b2c3d4",
    ] {
        assert_eq!(arg(url), None, "{url}");
    }
}

// yap:// and its yap-native:// alias

/// Every strict form, after the scheme.
const FORMS: [&str; 14] = [
    "invite/a1b2c3d4",
    "invite/a1b2c3d4/",
    "Invite/a1b2c3d4",
    "chat/@me",
    "chat/%40me",
    "chat/@me/dm_1",
    "chat/%40me/dm_1?message=m_1",
    "chat/srv",
    "chat/srv/chan?message=msg_1",
    "settings",
    "settings/voice",
    "settings/voice?row=sign-out",
    "signin",
    "signin?code=abcd-efgh",
];

#[test]
fn every_form_should_read_the_same_under_yap_and_yap_native() {
    for form in FORMS {
        let primary = arg(&format!("yap://{form}"));
        assert!(primary.is_some(), "yap://{form}");
        assert_eq!(primary, arg(&format!("yap-native://{form}")), "{form}");
        assert_eq!(
            parse_link(&format!("yap://{form}"), SITE),
            primary,
            "parse_link yap://{form}"
        );
        assert_eq!(arg(&format!("YAP://{form}")), primary, "YAP://{form}");
    }
}

#[test]
fn what_yap_native_refuses_yap_should_refuse_too() {
    for rest in [
        "",
        "/",
        "open",
        "room/abc",
        "activate?code=ABCD-EFGH",
        "invite/a/b",
        "invite//a",
        "chat/srv/chan/extra",
        "chat/srv?message=m",
        "chat/srv/chan?message=a&message=b",
        "settings/nope",
        "signin?token=abc",
        "signin?code=ABCD-EFGH&session=abc",
        "chat/srv/ch%20an",
    ] {
        assert_eq!(arg(&format!("yap://{rest}")), None, "yap://{rest}");
        assert_eq!(
            arg(&format!("yap-native://{rest}")),
            None,
            "yap-native://{rest}"
        );
    }
}

#[test]
fn the_tauri_apps_invite_links_should_still_open_the_landing() {
    let invite = Some(Link::Invite("a1b2c3d4".into()));
    // desktop-deep-link.ts read the code and ignored the query and fragment.
    for url in [
        "yap://invite/a1b2c3d4",
        "yap://invite/a1b2c3d4?utm_source=x",
        "yap://invite/a1b2c3d4#join",
        "\"yap://invite/a1b2c3d4\"",
    ] {
        assert_eq!(arg(url), invite, "{url}");
        assert_eq!(parse_link(url, SITE), invite, "{url}");
    }
    // Only for invites, and only under yap://: the alias stays strict.
    assert_eq!(arg("yap-native://invite/a1b2c3d4?utm_source=x"), None);
    assert_eq!(arg("yap://chat/srv?utm_source=x"), None);
    assert_eq!(arg("yap://invite/bad%20code?x=1"), None);
}

// The test scheme

#[test]
fn a_test_scheme_should_be_yap_native_dash_a_name() {
    assert_eq!(
        test_scheme("yap-native-test"),
        Some("yap-native-test".into())
    );
    assert_eq!(
        test_scheme("yap-native-e2e-2"),
        Some("yap-native-e2e-2".into())
    );
    for value in [
        "",
        "yap-native",
        "yap-native-",
        "yap",
        "https",
        "yap-native-Test",
        "yap-native-a_b",
        "other-native-test",
    ] {
        assert_eq!(test_scheme(value), None, "{value}");
    }
}

#[test]
fn the_test_scheme_should_read_as_ours_only_when_given() {
    let url = "yap-native-test://chat/srv/chan?message=m";
    assert_eq!(deep_link_arg(url), None);
    assert_eq!(
        deep_link_arg_with(url, Some("yap-native-test")),
        channel("srv", "chan", Some("m"))
    );
    // Strict there too.
    assert_eq!(
        deep_link_arg_with(
            "yap-native-test://chat/srv/chan?x=1",
            Some("yap-native-test")
        ),
        None
    );
    // And the real scheme still reads.
    assert_eq!(
        deep_link_arg_with("yap-native://chat/@me", Some("yap-native-test")),
        Some(Link::Home)
    );
}

#[test]
fn link_kinds_should_never_carry_ids() {
    let kinds: Vec<&str> = [
        Link::Invite("a".into()),
        Link::Home,
        Link::Server("s".into()),
        Link::Channel {
            server_id: "s".into(),
            channel_id: "c".into(),
            message_id: None,
        },
        Link::Channel {
            server_id: "s".into(),
            channel_id: "c".into(),
            message_id: Some("m".into()),
        },
        Link::Settings {
            panel: "voice".into(),
            row: String::new(),
        },
        Link::SignIn {
            user_code: Some("ABCD-EFGH".into()),
        },
    ]
    .iter()
    .map(Link::kind)
    .collect();
    assert_eq!(
        kinds,
        ["invite", "home", "server", "channel", "message", "settings", "signin"]
    );
}

// The store

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    view: InviteView,
}

impl Client {
    fn new() -> Self {
        Self {
            state: State::new(BASE),
            asked: Vec::new(),
            view: InviteView::default(),
        }
    }

    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Invites(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        fx
    }

    fn user(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn link(&mut self, link: Link) -> Vec<Effect> {
        self.user(Command::Invites(InviteCommand::OpenLink(link)))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?}"));
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn online() -> Self {
        let mut client = Self::new();
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
        client
    }

    /// A device sign-in waiting on `code`.
    fn signing_in(code: &str) -> Self {
        let mut client = Self::new();
        let fx = client.user(Command::StartSignIn);
        let Some(Effect::SignInStart(attempt)) = fx.last().cloned() else {
            panic!("{fx:#?}");
        };
        client.input(Input::SignIn {
            attempt,
            event: SignInEvent::DeviceCode {
                user_code: code.into(),
                url: "https://yap.fixture.invalid/activate?code=x".into(),
            },
        });
        client
    }
}

fn polls_now(fx: &[Effect]) -> bool {
    fx.contains(&Effect::SignInPollNow)
}

#[test]
fn signin_naming_the_waiting_code_should_poll_now() {
    let mut client = Client::signing_in("ABCD-EFGH");
    let fx = client.link(Link::SignIn {
        user_code: Some("ABCD-EFGH".into()),
    });
    assert!(polls_now(&fx), "{fx:#?}");
}

#[test]
fn signin_without_a_code_should_poll_now() {
    let mut client = Client::signing_in("ABCD-EFGH");
    assert!(polls_now(&client.link(Link::SignIn { user_code: None })));
}

#[test]
fn signin_naming_another_code_should_change_nothing() {
    let mut client = Client::signing_in("ABCD-EFGH");
    let fx = client.link(Link::SignIn {
        user_code: Some("WXYZ-2345".into()),
    });
    assert!(fx.is_empty(), "{fx:#?}");
}

#[test]
fn signin_before_kova_gave_a_code_should_wait_for_its_own_poll() {
    let mut client = Client::new();
    client.user(Command::StartSignIn);
    let fx = client.link(Link::SignIn {
        user_code: Some("ABCD-EFGH".into()),
    });
    assert!(!polls_now(&fx), "{fx:#?}");
}

#[test]
fn signin_with_no_sign_in_running_should_change_nothing() {
    let mut signed_out = Client::new();
    assert!(signed_out.link(Link::SignIn { user_code: None }).is_empty());
    let mut online = Client::online();
    let fx = online.link(Link::SignIn { user_code: None });
    assert!(!polls_now(&fx), "{fx:#?}");
    // Never held for later, either.
    online.user(Command::SignOut);
    let mut client = online;
    let fx = client.user(Command::StartSignIn);
    assert!(!polls_now(&fx), "{fx:#?}");
}

#[test]
fn a_cancelled_sign_in_should_not_poll() {
    let mut client = Client::signing_in("ABCD-EFGH");
    client.user(Command::CancelSignIn);
    assert!(client.link(Link::SignIn { user_code: None }).is_empty());
}

#[test]
fn home_link_should_show_home() {
    let mut client = Client::online();
    let fx = client.link(Link::Home);
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::View(ViewUpdate::Home(true)))),
        "{fx:#?}"
    );
}

#[test]
fn server_link_should_select_a_known_server() {
    let mut client = Client::online();
    let fx = client.link(Link::Server("srv_fixture_two".into()));
    assert!(
        fx.iter().any(|e| matches!(
            e,
            Effect::View(ViewUpdate::ActiveServer { id, .. }) if id == "srv_fixture_two"
        )),
        "{fx:#?}"
    );
}

#[test]
fn server_link_to_an_unknown_server_should_change_nothing() {
    let mut client = Client::online();
    let fx = client.link(Link::Server("srv_nope".into()));
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::View(ViewUpdate::ActiveServer { .. }))),
        "{fx:#?}"
    );
}

#[test]
fn settings_link_should_ask_for_the_panel_and_row() {
    let mut client = Client::online();
    client.link(Link::Settings {
        panel: "account".into(),
        row: "sign-out".into(),
    });
    assert_eq!(
        (
            client.view.settings_panel.as_str(),
            client.view.settings_row.as_str(),
            client.view.settings_seq
        ),
        ("account", "sign-out", 1)
    );
}

#[test]
fn settings_link_while_signed_out_should_wait_for_the_shell() {
    let mut client = Client::new();
    client.link(Link::Settings {
        panel: "voice".into(),
        row: String::new(),
    });
    assert_eq!(client.view.settings_seq, 0);
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
    assert_eq!(
        (
            client.view.settings_panel.as_str(),
            client.view.settings_seq
        ),
        ("voice", 1)
    );
}

#[test]
fn links_into_the_chat_should_ask_to_uncover_it() {
    let mut client = Client::online();
    client.link(Link::Settings {
        panel: "voice".into(),
        row: String::new(),
    });
    assert_eq!(
        client.view.nav_seq, 0,
        "settings is not a link into the chat"
    );
    client.link(Link::Home);
    assert_eq!(client.view.nav_seq, 1);
    client.link(Link::Server("srv_fixture".into()));
    client.link(Link::Channel {
        server_id: "srv_fixture".into(),
        channel_id: TEXT.into(),
        message_id: None,
    });
    assert_eq!(client.view.nav_seq, 3);
    client.link(Link::SignIn { user_code: None });
    assert_eq!(client.view.nav_seq, 3, "signin only brings the window up");
}
