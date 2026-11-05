//! Creating and joining servers and the links into the app: request shapes
//! (as the web sends them), invite and deep-link parsing, the scheme's
//! registry values, and the reducer's landing, create flow and staleness.
//! Creating and joining are covered only here, never against production.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::links::{
    deep_link_arg, invite_code, open_command, parse_link, scheme_registration,
    scheme_registration_lines, Link, SCHEME_KEY,
};
use yap_core::store::invites::{create_icon_problem, member_count, InviteView};
use yap_core::store::view::ServerRow;
use yap_core::store::{
    reduce, AdminResponse, ApiRequest, ApiResponse, Command, Effect, InviteCommand, InviteRequest,
    Method, ProfileAsset, RequestId, State, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SITE: &str = "https://yap.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn http_error(status: u16) -> ApiError {
    ApiError::Http {
        status,
        message: "nope".into(),
    }
}

fn preview_body(code: &str) -> Value {
    json!({
        "code": code,
        "server": { "id": "srv_new", "name": "lab crew", "icon_url": "/api/icons/x.png", "member_count": 1204 },
        "inviter": { "username": "alice", "display_name": "Alice", "avatar_url": null },
    })
}

fn server_row(id: &str, name: &str) -> Value {
    json!({ "id": id, "name": name, "owner_id": "user_fixture_alice", "icon_url": null,
            "invites_paused": 0, "created_at": "2026-11-05T00:00:00.000Z" })
}

fn invites(request: InviteRequest) -> ApiRequest {
    ApiRequest::Invites(request)
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    effects: Vec<Effect>,
    servers: Vec<ServerRow>,
    active_server: String,
    view: InviteView,
}

impl Client {
    fn new() -> Self {
        Self {
            state: State::new(BASE),
            asked: Vec::new(),
            effects: Vec::new(),
            servers: Vec::new(),
            active_server: String::new(),
            view: InviteView::default(),
        }
    }

    fn input(&mut self, input: yap_core::store::Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Servers(diff)) => {
                    assert!(diff.clone().apply(&mut self.servers), "{diff:?}");
                }
                Effect::View(ViewUpdate::ActiveServer { id, .. }) => {
                    self.active_server = id.clone()
                }
                Effect::View(ViewUpdate::Invites(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        self.effects.extend(fx.iter().cloned());
        fx
    }

    fn command(&mut self, command: InviteCommand) -> Vec<Effect> {
        self.input(yap_core::store::Input::User(Command::Invites(command)))
    }

    fn id_of(&self, request: &ApiRequest) -> RequestId {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?} in {:#?}", self.asked))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self.id_of(request);
        let result = request.decode(body);
        self.input(yap_core::store::Input::Http { id, result })
    }

    fn fail(&mut self, request: &ApiRequest, error: ApiError) -> Vec<Effect> {
        let id = self.id_of(request);
        self.input(yap_core::store::Input::Http {
            id,
            result: Err(error),
        })
    }

    fn sign_in(&mut self) {
        self.input(yap_core::store::Input::User(Command::UseToken(
            "tok_fixture".into(),
        )));
        self.respond(&ApiRequest::Me, fixture("user-me.json"));
        self.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        self.respond(&channels, fixture("channels-response.json"));
        let page = ApiRequest::Messages {
            channel_id: TEXT.into(),
            before: None,
        };
        self.respond(&page, fixture("messages-page.json"));
    }

    fn online() -> Self {
        let mut client = Self::new();
        client.sign_in();
        client
    }

    fn asked_for(&self, request: &ApiRequest) -> bool {
        self.asked.iter().any(|(_, r)| r == request)
    }

    fn step(&self) -> String {
        self.view
            .landing
            .as_ref()
            .map(|l| l.step.clone())
            .unwrap_or_default()
    }

    /// The landing on `code`, its preview loaded.
    fn previewed(code: &str) -> Self {
        let mut client = Self::online();
        client.command(InviteCommand::OpenLink(Link::Invite(code.into())));
        let preview = invites(InviteRequest::Preview { code: code.into() });
        client.respond(&preview, preview_body(code));
        client
    }
}

fn join(code: &str) -> ApiRequest {
    invites(InviteRequest::Join { code: code.into() })
}

fn preview(code: &str) -> ApiRequest {
    invites(InviteRequest::Preview { code: code.into() })
}

// Request shapes

#[test]
fn create_server_should_post_the_name_alone_without_an_icon() {
    let request = InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Post,
            "/api/servers".into(),
            json!({ "name": "Lab" })
        )
    );
}

#[test]
fn create_server_should_send_the_uploaded_icon_url() {
    let request = InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: Some("https://cdn.invalid/i.png".into()),
    };
    assert_eq!(
        request.body(),
        json!({ "name": "Lab", "icon_url": "https://cdn.invalid/i.png" })
    );
}

#[test]
fn preview_should_get_the_encoded_code() {
    let request = InviteRequest::Preview { code: "a b".into() };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (Method::Get, "/api/invites/a%20b".into(), Value::Null)
    );
}

#[test]
fn join_should_post_an_empty_object() {
    let request = InviteRequest::Join {
        code: "a1b2c3d4".into(),
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (Method::Post, "/api/invites/a1b2c3d4/join".into(), json!({}))
    );
}

// Links

#[test]
fn site_invite_link_should_be_an_invite() {
    assert_eq!(
        parse_link("https://yap.fixture.invalid/invite/a1b2c3d4", SITE),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn site_host_should_match_without_case() {
    assert_eq!(
        parse_link("https://YAP.fixture.invalid/invite/a1b2c3d4", SITE),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn another_site_should_not_be_a_link() {
    assert_eq!(
        parse_link("https://evil.invalid/invite/a1b2c3d4", SITE),
        None
    );
}

#[test]
fn plain_http_should_not_match_an_https_site() {
    assert_eq!(
        parse_link("http://yap.fixture.invalid/invite/a1b2c3d4", SITE),
        None
    );
}

#[test]
fn native_invite_deep_link_should_be_an_invite() {
    assert_eq!(
        parse_link("yap-native://invite/a1b2c3d4", SITE),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn desktop_app_invite_link_should_be_an_invite() {
    assert_eq!(
        parse_link("yap://invite/a1b2c3d4", SITE),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn desktop_app_channel_link_should_not_be_read() {
    assert_eq!(parse_link("yap://chat/srv/chan", SITE), None);
}

#[test]
fn empty_invite_should_not_be_a_link() {
    assert_eq!(parse_link("yap-native://invite/", SITE), None);
}

#[test]
fn an_overlong_invite_code_should_not_be_a_link() {
    let long = "a".repeat(65);
    assert_eq!(
        parse_link(&format!("yap-native://invite/{long}"), SITE),
        None
    );
    assert_eq!(invite_code(&long, SITE), None);
    assert_eq!(invite_code(&"a".repeat(64), SITE), Some("a".repeat(64)));
}

#[test]
fn channel_link_should_name_the_server_and_channel() {
    assert_eq!(
        parse_link("https://yap.fixture.invalid/chat/srv/chan", SITE),
        Some(Link::Channel {
            server_id: "srv".into(),
            channel_id: "chan".into(),
            message_id: None,
        })
    );
}

#[test]
fn message_link_should_carry_the_message() {
    assert_eq!(
        parse_link(
            "https://yap.fixture.invalid/chat/srv/chan?message=msg_1",
            SITE
        ),
        Some(Link::Channel {
            server_id: "srv".into(),
            channel_id: "chan".into(),
            message_id: Some("msg_1".into()),
        })
    );
}

#[test]
fn dm_link_should_decode_its_server_segment() {
    assert_eq!(
        parse_link("yap-native://chat/%40me/dm_1?message=m", SITE),
        Some(Link::Channel {
            server_id: "@me".into(),
            channel_id: "dm_1".into(),
            message_id: Some("m".into()),
        })
    );
}

#[test]
fn server_link_without_a_channel_should_not_be_a_link() {
    assert_eq!(
        parse_link("https://yap.fixture.invalid/chat/srv", SITE),
        None
    );
}

#[test]
fn bare_code_should_be_an_invite_code() {
    assert_eq!(invite_code("  a1b2c3d4 ", SITE), Some("a1b2c3d4".into()));
}

#[test]
fn pasted_invite_link_should_give_its_code() {
    assert_eq!(
        invite_code("https://yap.fixture.invalid/invite/a1b2c3d4", SITE),
        Some("a1b2c3d4".into())
    );
}

#[test]
fn channel_link_should_not_be_an_invite_code() {
    assert_eq!(
        invite_code("https://yap.fixture.invalid/chat/s/c", SITE),
        None
    );
}

#[test]
fn deep_link_arg_should_take_our_scheme() {
    assert_eq!(
        deep_link_arg("\"yap-native://invite/a1b2c3d4\""),
        Some(Link::Invite("a1b2c3d4".into()))
    );
}

#[test]
fn deep_link_arg_should_ignore_the_desktop_apps_scheme() {
    assert_eq!(deep_link_arg("yap://invite/a1b2c3d4"), None);
}

#[test]
fn deep_link_arg_should_ignore_flags_and_paths() {
    let args = [
        "--autostart",
        r"C:\Yap\YapNative.exe",
        "https://yap.115jon.com/invite/x",
    ];
    assert!(args.iter().all(|arg| deep_link_arg(arg).is_none()));
}

// The scheme's registration

const EXE: &str = r"C:\Apps\Yap Native\YapNative.exe";

#[test]
fn open_command_should_quote_the_executable_and_the_url() {
    assert_eq!(
        open_command(EXE),
        r#""C:\Apps\Yap Native\YapNative.exe" "%1""#
    );
}

#[test]
fn registration_should_stay_under_our_own_key() {
    assert!(scheme_registration(EXE)
        .iter()
        .all(|v| v.key == SCHEME_KEY || v.key.starts_with(&format!("{SCHEME_KEY}\\"))));
}

#[test]
fn registration_should_never_touch_the_yap_scheme() {
    assert!(scheme_registration(EXE).iter().all(|v| {
        let key = v.key.to_ascii_lowercase();
        key != r"software\classes\yap" && !key.starts_with(r"software\classes\yap\")
    }));
}

#[test]
fn registration_should_mark_a_url_protocol() {
    assert!(scheme_registration(EXE)
        .iter()
        .any(|v| v.key == SCHEME_KEY && v.name == "URL Protocol" && v.data.is_empty()));
}

#[test]
fn registration_should_open_with_the_command() {
    assert!(scheme_registration(EXE).iter().any(|v| {
        v.key == format!(r"{SCHEME_KEY}\shell\open\command") && v.data == open_command(EXE)
    }));
}

#[test]
fn registration_lines_should_be_tab_separated() {
    let lines = scheme_registration_lines(EXE);
    assert_eq!(
        lines.lines().next(),
        Some("Software\\Classes\\yap-native\t\tURL:Yap Native")
    );
}

// Copy rules

#[test]
fn one_member_should_be_singular() {
    assert_eq!(member_count(1), "1 member");
}

#[test]
fn member_count_should_group_thousands() {
    assert_eq!(member_count(1_204_567), "1,204,567 members");
}

#[test]
fn a_text_file_should_not_be_a_server_icon() {
    assert_eq!(
        create_icon_problem(None, Some(10)),
        Some("Only image files are allowed")
    );
}

#[test]
fn an_icon_over_8_mb_should_be_refused() {
    assert_eq!(
        create_icon_problem(Some("image/png"), Some(8 * 1024 * 1024 + 1)),
        Some("Image too large (max 8MB)")
    );
}

// Create a server

#[test]
fn create_without_a_name_should_ask_for_one() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "   ".into(),
        icon: None,
    });
    assert_eq!(client.view.create_name_error, "Give your server a name.");
}

#[test]
fn create_without_a_name_should_send_nothing() {
    let mut client = Client::online();
    let before = client.asked.len();
    client.command(InviteCommand::CreateServer {
        name: "".into(),
        icon: None,
    });
    assert_eq!(client.asked.len(), before);
}

#[test]
fn create_should_send_the_trimmed_name() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "  Lab  ".into(),
        icon: None,
    });
    assert!(client.asked_for(&invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    })));
}

#[test]
fn created_server_should_join_the_rail() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: None,
    });
    let create = invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    });
    client.respond(&create, server_row("srv_lab", "Lab"));
    assert_eq!(
        client.servers.last().map(|s| s.id.as_str()),
        Some("srv_lab")
    );
}

#[test]
fn created_server_should_be_selected() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: None,
    });
    let create = invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    });
    client.respond(&create, server_row("srv_lab", "Lab"));
    assert_eq!(client.active_server, "srv_lab");
}

#[test]
fn created_server_should_count_as_done() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: None,
    });
    let create = invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    });
    client.respond(&create, server_row("srv_lab", "Lab"));
    assert_eq!(
        (client.view.create.busy, client.view.create.done),
        (false, 1)
    );
}

#[test]
fn failed_create_should_show_the_webs_copy_whatever_the_status() {
    let mut client = Client::online();
    client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: None,
    });
    let create = invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: None,
    });
    client.fail(&create, http_error(429));
    assert_eq!(
        client.view.create.error,
        "Unable to create the server. Check your connection and try again."
    );
}

#[test]
fn create_with_an_icon_should_upload_it_first() {
    let mut client = Client::online();
    let fx = client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: Some((r"C:\i.png".into(), "image/png".into())),
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::ProfileUpload { asset: ProfileAsset::ServerIcon, path, .. } if path == r"C:\i.png"
    )));
}

fn upload_id(effects: &[Effect]) -> RequestId {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::ProfileUpload { id, .. } => Some(*id),
            _ => None,
        })
        .expect("an upload")
}

#[test]
fn uploaded_icon_should_go_into_the_create() {
    let mut client = Client::online();
    let fx = client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: Some((r"C:\i.png".into(), "image/png".into())),
    });
    client.input(yap_core::store::Input::Http {
        id: upload_id(&fx),
        result: Ok(ApiResponse::Admin(AdminResponse::IconUrl(
            "https://cdn.invalid/i.png".into(),
        ))),
    });
    assert!(client.asked_for(&invites(InviteRequest::CreateServer {
        name: "Lab".into(),
        icon_url: Some("https://cdn.invalid/i.png".into()),
    })));
}

#[test]
fn failed_icon_upload_should_say_so_and_not_create() {
    let mut client = Client::online();
    let fx = client.command(InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: Some((r"C:\i.png".into(), "image/png".into())),
    });
    client.input(yap_core::store::Input::Http {
        id: upload_id(&fx),
        result: Err(http_error(413)),
    });
    assert_eq!(
        client.view.create.error,
        "Unable to upload that icon. The file is too large."
    );
}

#[test]
fn a_second_create_while_busy_should_be_ignored() {
    let mut client = Client::online();
    let name = || InviteCommand::CreateServer {
        name: "Lab".into(),
        icon: None,
    };
    client.command(name());
    client.command(name());
    let creates = client
        .asked
        .iter()
        .filter(|(_, r)| matches!(r, ApiRequest::Invites(InviteRequest::CreateServer { .. })))
        .count();
    assert_eq!(creates, 1);
}

// The invite landing

#[test]
fn invite_link_should_load_the_preview() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("a1b2c3d4".into())));
    assert!(client.asked_for(&preview("a1b2c3d4")));
}

#[test]
fn invite_link_should_open_the_landing_loading() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("a1b2c3d4".into())));
    assert_eq!(
        (client.step(), client.view.landing_seq),
        ("loading".to_owned(), 1)
    );
}

#[test]
fn preview_should_show_the_server_and_inviter() {
    let client = Client::previewed("a1b2c3d4");
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!(
        (
            landing.step.as_str(),
            landing.server_name.as_str(),
            landing.server_initial.as_str(),
            landing.members.as_str(),
            landing.inviter.as_str(),
        ),
        ("preview", "lab crew", "L", "1,204 members", "Alice")
    );
}

#[test]
fn preview_icon_should_be_absolute() {
    let client = Client::previewed("a1b2c3d4");
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!(landing.server_icon, format!("{BASE}/api/icons/x.png"));
}

#[test]
fn missing_invite_should_be_invalid_not_expired() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("gone".into())));
    client.fail(&preview("gone"), http_error(404));
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!((landing.step.as_str(), landing.expired), ("invalid", false));
}

#[test]
fn expired_invite_should_be_invalid_and_expired() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("old".into())));
    client.fail(&preview("old"), http_error(410));
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!((landing.step.as_str(), landing.expired), ("invalid", true));
}

#[test]
fn failed_preview_should_be_an_error_without_an_invite() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("x".into())));
    client.fail(&preview("x"), http_error(500));
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!(
        (landing.step.as_str(), landing.has_invite),
        ("error", false)
    );
}

#[test]
fn retry_after_a_failed_preview_should_load_it_again() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("x".into())));
    client.fail(&preview("x"), http_error(500));
    client.command(InviteCommand::Retry);
    let previews = client
        .asked
        .iter()
        .filter(|(_, r)| *r == preview("x"))
        .count();
    assert_eq!(previews, 2);
}

#[test]
fn an_older_preview_should_be_dropped_after_another_link() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("first".into())));
    let first = client.id_of(&preview("first"));
    client.command(InviteCommand::OpenLink(Link::Invite("second".into())));
    client.input(yap_core::store::Input::Http {
        id: first,
        result: preview("first").decode(preview_body("first")),
    });
    assert_eq!(client.step(), "loading");
}

#[test]
fn accept_should_post_the_join() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    assert!(client.asked_for(&join("a1b2c3d4")));
}

#[test]
fn accept_should_show_joining() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    assert_eq!(client.step(), "joining");
}

#[test]
fn accept_before_the_preview_should_do_nothing() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Invite("a1b2c3d4".into())));
    client.command(InviteCommand::Accept);
    assert!(!client.asked_for(&join("a1b2c3d4")));
}

#[test]
fn joined_server_should_join_the_rail() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.respond(
        &join("a1b2c3d4"),
        json!({ "joined": true, "server": server_row("srv_new", "lab crew") }),
    );
    assert!(client.servers.iter().any(|s| s.id == "srv_new"));
}

#[test]
fn joined_should_show_the_server_it_joined() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.respond(
        &join("a1b2c3d4"),
        json!({ "joined": true, "server": server_row("srv_new", "lab crew") }),
    );
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!(
        (landing.step.as_str(), landing.server_id.as_str()),
        ("joined", "srv_new")
    );
}

#[test]
fn already_a_member_should_say_so() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.respond(
        &join("a1b2c3d4"),
        json!({ "already_member": true, "server": server_row(SERVER, "Fixture Server") }),
    );
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!(
        (landing.step.as_str(), landing.server_name.as_str()),
        ("member", "Fixture Server")
    );
}

#[test]
fn join_without_a_server_should_be_an_error() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.respond(&join("a1b2c3d4"), json!({ "joined": true }));
    assert_eq!(client.step(), "error");
}

#[test]
fn failed_join_should_keep_the_invite_for_try_again() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.fail(&join("a1b2c3d4"), http_error(403));
    let landing = client.view.landing.clone().expect("landing");
    assert_eq!((landing.step.as_str(), landing.has_invite), ("error", true));
}

#[test]
fn retry_after_a_failed_join_should_join_again() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.fail(&join("a1b2c3d4"), http_error(500));
    client.command(InviteCommand::Retry);
    let joins = client
        .asked
        .iter()
        .filter(|(_, r)| *r == join("a1b2c3d4"))
        .count();
    assert_eq!(joins, 2);
}

#[test]
fn join_answer_after_close_should_be_dropped() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    let id = client.id_of(&join("a1b2c3d4"));
    // Closing is refused while joining; another link replaces the landing.
    client.command(InviteCommand::OpenLink(Link::Invite("other".into())));
    client.input(yap_core::store::Input::Http {
        id,
        result: join("a1b2c3d4")
            .decode(json!({ "joined": true, "server": server_row("srv_new", "lab crew") })),
    });
    assert_eq!(client.step(), "loading");
}

#[test]
fn close_should_be_refused_while_joining() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.command(InviteCommand::CloseLanding);
    assert_eq!(client.step(), "joining");
}

#[test]
fn open_server_should_close_the_landing_on_it() {
    let mut client = Client::previewed("a1b2c3d4");
    client.command(InviteCommand::Accept);
    client.respond(
        &join("a1b2c3d4"),
        json!({ "joined": true, "server": server_row("srv_new", "lab crew") }),
    );
    client.command(InviteCommand::OpenServer("srv_new".into()));
    assert_eq!(
        (client.view.landing.is_none(), client.active_server.as_str()),
        (true, "srv_new")
    );
}

// Links into the app

#[test]
fn a_link_before_sign_in_should_wait() {
    let mut client = Client::new();
    client.command(InviteCommand::OpenLink(Link::Invite("a1b2c3d4".into())));
    assert!(!client.asked_for(&preview("a1b2c3d4")));
}

#[test]
fn a_waiting_link_should_open_once_the_shell_is_ready() {
    let mut client = Client::new();
    client.command(InviteCommand::OpenLink(Link::Invite("a1b2c3d4".into())));
    client.sign_in();
    assert!(client.asked_for(&preview("a1b2c3d4")));
}

#[test]
fn channel_link_should_open_its_channel() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Channel {
        server_id: "srv_fixture_two".into(),
        channel_id: "chan_two".into(),
        message_id: None,
    }));
    assert_eq!(client.active_server, "srv_fixture_two");
}

#[test]
fn message_link_should_ask_for_the_jump() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Channel {
        server_id: SERVER.into(),
        channel_id: TEXT.into(),
        message_id: Some("msg_1".into()),
    }));
    assert_eq!(
        (
            client.view.jump_channel_id.as_str(),
            client.view.jump_message_id.as_str(),
            client.view.jump_seq
        ),
        (TEXT, "msg_1", 1)
    );
}

#[test]
fn channel_link_without_a_message_should_not_jump() {
    let mut client = Client::online();
    client.command(InviteCommand::OpenLink(Link::Channel {
        server_id: SERVER.into(),
        channel_id: TEXT.into(),
        message_id: None,
    }));
    assert_eq!(client.view.jump_seq, 0);
}
