//! Reducer tests for notification levels and mutes (notification-levels.ts):
//! the list at sign-in and on every fresh gateway session, changes applied
//! at once and put back on a refusal, the gateway's copy from other
//! devices, timed mutes running out, muted rows without unread marks, and
//! plain server messages toasting only at "All messages".

use std::path::PathBuf;
use std::time::Duration;

use serde_json::{json, Value};
use yap_core::notification_levels::{MuteChoice, NotificationLevel, NotificationTargetType};
use yap_core::shell::{Sound, ToastView};
use yap_core::store::notify::{NotifyCommand, NotifyRequest};
use yap_core::store::view::{ChannelTreeRow, DmRow, ServerRow, TreeKind};
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State,
    ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const VOICE: &str = "chan_fixture_voice";
const ME: &str = "user_fixture_me";
const DM: &str = "dm_fixture_alice";
const ALICE: &str = "user_fixture_alice";
/// 2026-11-04 12:00 UTC, before the fixture messages' 13:00.
const NOW: i64 = 1_793_793_600_000;
const HOUR: i64 = 3_600_000;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn user(id: &str, username: &str, display: Option<&str>) -> Value {
    json!({ "id": id, "username": username, "display_name": display,
            "avatar_url": null, "status": "online", "custom_status": null })
}

fn list() -> ApiRequest {
    ApiRequest::Notify(NotifyRequest::List)
}

fn setting(target_type: &str, id: &str, level: Value, muted: bool, until: Value) -> Value {
    json!({ "target_type": target_type, "target_id": id, "level": level,
            "muted": muted, "muted_until": until })
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    servers: Vec<ServerRow>,
    tree: Vec<ChannelTreeRow>,
    dms: Vec<DmRow>,
    target: String,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Servers(diff)) => {
                    assert!(diff.clone().apply(&mut self.servers), "{diff:?}");
                }
                Effect::View(ViewUpdate::Channels(diff)) => {
                    assert!(diff.clone().apply(&mut self.tree), "{diff:?}");
                }
                Effect::View(ViewUpdate::Dms(diff)) => {
                    assert!(diff.clone().apply(&mut self.dms), "{diff:?}");
                }
                Effect::View(ViewUpdate::NotifyTarget(text)) => self.target = text.clone(),
                _ => {}
            }
        }
        fx
    }

    /// A command at `NOW` (the store's clock).
    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.command_at(command, NOW)
    }

    fn command_at(&mut self, command: Command, now_ms: i64) -> Vec<Effect> {
        self.input(Input::UserAt { command, now_ms })
    }

    fn id_of(&self, request: &ApiRequest) -> RequestId {
        self.asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?}"))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self.id_of(request);
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(dispatch),
        })
    }

    fn update(&mut self, data: Value) -> Vec<Effect> {
        self.dispatch("NOTIFICATION_SETTINGS_UPDATE", data)
    }

    /// Signed in, `#general` open, Alice's DM loaded, the gateway up, no
    /// settings stored.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            servers: Vec::new(),
            tree: Vec::new(),
            dms: Vec::new(),
            target: String::new(),
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
                     "recipient": user(ALICE, "alice", Some("Alice")) }]),
        );
        client.respond(
            &list(),
            json!({ "settings": [], "server_time": "2026-11-04T12:00:00.000Z" }),
        );
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn message(&mut self, id: &str, channel: &str, content: &str) -> Vec<Effect> {
        self.dispatch(
            "MESSAGE_CREATE",
            json!({ "id": id, "channel_id": channel, "author_id": ALICE,
                    "author": user(ALICE, "alice", Some("Alice")),
                    "content": content, "created_at": "2026-11-04T13:00:00.000Z" }),
        )
    }

    fn tree_row(&self, id: &str) -> &ChannelTreeRow {
        self.tree
            .iter()
            .find(|r| r.kind == TreeKind::Channel && r.id == id)
            .expect("row")
    }

    fn server_row(&self) -> &ServerRow {
        self.servers
            .iter()
            .find(|r| r.id == SERVER)
            .expect("server")
    }

    fn dm_row(&self) -> &DmRow {
        self.dms.iter().find(|r| r.id == DM).expect("dm")
    }

    fn target(&self) -> Value {
        serde_json::from_str(&self.target).expect("target json")
    }
}

fn toasts(fx: &[Effect]) -> Vec<&ToastView> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Toast(toast)) => Some(&**toast),
            _ => None,
        })
        .collect()
}

fn later(fx: &[Effect]) -> Option<(Duration, &Input)> {
    fx.iter().find_map(|e| match e {
        Effect::Later { delay, input } => Some((*delay, &**input)),
        _ => None,
    })
}

#[test]
fn settings_should_load_at_sign_in_and_after_a_reconnect() {
    let mut client = Client::online();
    assert_eq!(client.asked.iter().filter(|(_, r)| *r == list()).count(), 1);
    let session = client.state.session().expect("session").id;
    client.input(Input::Gateway {
        session,
        event: GatewayEvent::Closed,
    });
    client.input(Input::Gateway {
        session,
        event: GatewayEvent::Ready,
    });
    assert_eq!(client.asked.iter().filter(|(_, r)| *r == list()).count(), 2);
}

#[test]
fn a_muted_channel_should_show_no_unread_mark_but_keep_its_mentions() {
    let mut client = Client::online();
    client.message("m1", VOICE, "lunch?");
    assert!(client.tree_row(VOICE).unread);
    assert!(!client.tree_row(VOICE).notify_muted);
    client.update(setting("channel", VOICE, Value::Null, true, Value::Null));
    let row = client.tree_row(VOICE);
    assert!(!row.unread && row.notify_muted, "{row:?}");
    assert!(!client.state.shows_unread(VOICE));
    // A mention still badges.
    client.dispatch(
        "NOTIFICATION_CREATE",
        json!({ "id": "n_m2", "type": "mention", "channel_id": VOICE, "server_id": SERVER,
                "message_id": "m2", "is_read": false,
                "from_user": { "id": ALICE, "username": "alice", "display_name": "Alice" },
                "content": "hey @me" }),
    );
    assert_eq!(client.tree_row(VOICE).mentions, 1);
    // Unmuted elsewhere: the mark is back.
    client.update(setting("channel", VOICE, Value::Null, false, Value::Null));
    assert!(client.tree_row(VOICE).unread);
}

#[test]
fn a_muted_server_should_show_no_unread_pill_and_dim() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    client.message("m1", TEXT, "lunch?");
    assert!(client.state.server_unread(SERVER));
    assert!(client.server_row().unread);
    client.update(setting("server", SERVER, Value::Null, true, Value::Null));
    assert!(!client.state.server_unread(SERVER));
    let row = client.server_row();
    assert!(!row.unread && row.notify_muted, "{row:?}");
    // Its channels are muted through it.
    assert!(client.state.channel_notify_muted(TEXT));
    assert!(!client.state.shows_unread(TEXT));
}

#[test]
fn a_muted_channel_should_not_light_its_server_pill() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(DM.into()));
    client.message("m1", TEXT, "lunch?");
    client.update(setting("channel", TEXT, Value::Null, true, Value::Null));
    assert!(!client.state.server_unread(SERVER));
    assert!(!client.server_row().notify_muted);
}

#[test]
fn a_muted_dm_should_show_no_unread_mark() {
    let mut client = Client::online();
    client.message("m1", DM, "psst");
    assert!(client.dm_row().unread);
    client.update(setting("channel", DM, Value::Null, true, Value::Null));
    let row = client.dm_row();
    assert!(!row.unread && row.notify_muted && row.rail == -1, "{row:?}");
}

#[test]
fn a_timed_mute_should_apply_at_once_and_end_on_its_own() {
    let mut client = Client::online();
    client.message("m1", VOICE, "lunch?");
    let fx = client.command(Command::Notify(NotifyCommand::Mute {
        target_type: NotificationTargetType::Channel,
        target_id: VOICE.into(),
        mute: MuteChoice::Hour,
    }));
    let put = fx
        .iter()
        .find_map(|e| match e {
            Effect::Http {
                request: ApiRequest::Notify(request @ NotifyRequest::Update { .. }),
                ..
            } => Some(request.clone()),
            _ => None,
        })
        .expect("a PUT");
    assert_eq!(
        put.body(),
        json!({ "target_type": "channel", "target_id": VOICE, "mute": "1h" })
    );
    assert!(client.tree_row(VOICE).notify_muted);
    let (delay, wake) = later(&fx).expect("a wake-up");
    assert!(delay >= Duration::from_millis(HOUR as u64), "{delay:?}");
    let Input::UserAt { command, now_ms } = wake.clone() else {
        panic!("{wake:?}");
    };
    assert_eq!(now_ms, NOW + HOUR);
    // The server's answer stands; the hour passes.
    let until = "2026-11-04T13:00:00.000Z";
    client.respond(
        &ApiRequest::Notify(put),
        json!({ "setting": setting("channel", VOICE, Value::Null, true, json!(until)) }),
    );
    assert!(client.tree_row(VOICE).notify_muted);
    client.input(Input::UserAt { command, now_ms });
    let row = client.tree_row(VOICE);
    assert!(!row.notify_muted && row.unread, "{row:?}");
}

#[test]
fn any_command_after_a_mute_ran_out_should_end_it() {
    let mut client = Client::online();
    client.update(setting(
        "channel",
        VOICE,
        Value::Null,
        true,
        json!("2026-11-04T12:30:00.000Z"),
    ));
    client.command(Command::SetFocused(true));
    assert!(client.tree_row(VOICE).notify_muted);
    client.command_at(Command::SetFocused(true), NOW + HOUR);
    assert!(!client.tree_row(VOICE).notify_muted);
}

#[test]
fn a_timed_mute_from_another_device_should_be_judged_against_the_wall_clock() {
    let mut client = Client::online();
    // The app sat idle since NOW; another device muted for an hour two
    // hours later. The store's clock still says NOW.
    let fx = client.update(setting(
        "channel",
        VOICE,
        Value::Null,
        true,
        json!("2026-11-04T15:00:00.000Z"),
    ));
    let command = fx
        .iter()
        .find_map(|e| match e {
            Effect::Later { delay, input } if delay.is_zero() => match &**input {
                Input::UserAt { command, .. } => Some(command.clone()),
                _ => None,
            },
            _ => None,
        })
        .expect("a nudge");
    assert_eq!(command, Command::Notify(NotifyCommand::Refresh));
    // The runtime stamps it with the wall clock: the wake-up is an hour out,
    // not three.
    let fx = client.command_at(command, NOW + 2 * HOUR);
    let (delay, _) = later(&fx).expect("a wake-up");
    assert!(
        delay >= Duration::from_millis(HOUR as u64)
            && delay < Duration::from_millis(HOUR as u64 + 1000),
        "{delay:?}"
    );
    assert!(client.tree_row(VOICE).notify_muted);
}

#[test]
fn a_refused_change_should_be_put_back() {
    let mut client = Client::online();
    client.command(Command::Notify(NotifyCommand::SetLevel {
        target_type: NotificationTargetType::Server,
        target_id: SERVER.into(),
        level: Some(NotificationLevel::All),
    }));
    client.command(Command::Notify(NotifyCommand::Inspect(Some((
        NotificationTargetType::Server,
        SERVER.into(),
    )))));
    assert_eq!(client.target()["level"], "all");
    let put = ApiRequest::Notify(NotifyRequest::Update {
        target_type: NotificationTargetType::Server,
        target_id: SERVER.into(),
        level: Some(Some(NotificationLevel::All)),
        mute: None,
    });
    let id = client.id_of(&put);
    let fx = client.input(Input::Http {
        id,
        result: Err(ApiError::Network("down".into())),
    });
    assert!(fx
        .iter()
        .any(|e| matches!(e, Effect::View(ViewUpdate::Notice(_)))));
    assert_eq!(client.target()["level"], "");
}

#[test]
fn the_menu_view_should_show_inheritance_and_the_mute() {
    let mut client = Client::online();
    client.update(setting("server", SERVER, json!("all"), true, Value::Null));
    client.command(Command::Notify(NotifyCommand::Inspect(Some((
        NotificationTargetType::Channel,
        TEXT.into(),
    )))));
    let view = client.target();
    assert_eq!(view["key"], format!("channel:{TEXT}"));
    assert_eq!(view["level"], "");
    assert_eq!(view["inherited"], "all");
    assert_eq!(view["effective"], "all");
    assert_eq!(view["muted"], false);
    assert_eq!(view["serverMuted"], true);
    // The menu stays current: a mute of its own, for an hour.
    client.update(setting(
        "channel",
        TEXT,
        json!("none"),
        true,
        json!("2026-11-04T13:00:00.000Z"),
    ));
    let view = client.target();
    assert_eq!(view["effective"], "none");
    assert_eq!(view["mutedLabel"], "Muted until 1:00 PM");
    // A DM inherits all and has no server.
    client.command(Command::Notify(NotifyCommand::Inspect(Some((
        NotificationTargetType::Channel,
        DM.into(),
    )))));
    let view = client.target();
    assert_eq!(
        (view["isDm"].clone(), view["inherited"].clone()),
        (json!(true), json!("all"))
    );
    client.command(Command::Notify(NotifyCommand::Inspect(None)));
    assert_eq!(client.target, "");
}

#[test]
fn a_plain_server_message_should_toast_only_at_all_messages() {
    let mut client = Client::online();
    // The default is mentions only.
    assert!(toasts(&client.message("m1", VOICE, "lunch?")).is_empty());
    client.update(setting("server", SERVER, json!("all"), false, Value::Null));
    assert_eq!(toasts(&client.message("m2", VOICE, "lunch?")).len(), 1);
    // A channel's own level wins, and a mute silences plain messages.
    client.update(setting(
        "channel",
        VOICE,
        json!("mentions"),
        false,
        Value::Null,
    ));
    assert!(toasts(&client.message("m3", VOICE, "lunch?")).is_empty());
    client.update(setting("channel", VOICE, json!("all"), true, Value::Null));
    assert!(toasts(&client.message("m4", VOICE, "lunch?")).is_empty());
}

#[test]
fn a_muted_open_channel_should_keep_its_message_knock_quiet() {
    let knocks = |fx: &[Effect]| {
        fx.iter()
            .filter(|e| matches!(e, Effect::View(ViewUpdate::Sound(Sound::Message))))
            .count()
    };
    let mut client = Client::online();
    assert_eq!(knocks(&client.message("m1", TEXT, "hi")), 1);
    client.update(setting("channel", TEXT, Value::Null, true, Value::Null));
    assert_eq!(knocks(&client.message("m2", TEXT, "hi")), 0);
    client.update(setting("channel", TEXT, json!("none"), false, Value::Null));
    assert_eq!(knocks(&client.message("m3", TEXT, "hi")), 0);
    client.update(setting(
        "channel",
        TEXT,
        json!("mentions"),
        false,
        Value::Null,
    ));
    assert_eq!(knocks(&client.message("m4", TEXT, "hi")), 1);
}

#[test]
fn sign_out_should_forget_the_settings() {
    let mut client = Client::online();
    client.update(setting("channel", VOICE, Value::Null, true, Value::Null));
    assert!(client.state.channel_notify_muted(VOICE));
    client.command(Command::SignOut);
    assert!(!client.state.channel_notify_muted(VOICE));
    let _ = ME;
}
