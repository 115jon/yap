//! Reducer tests for game activity: others' activity from presence, and
//! ours sent with op 43 only on a change, restated after Ready and Resumed,
//! `null` once while sharing is off or invisible, and never to a server
//! whose presence does not carry `game_activity`.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::{DmRow, MemberListRow};
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State,
    ViewUpdate,
};
use yap_protocol::game_activity::GameActivity;
use yap_protocol::GatewayOp;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";
const HASH: &str = "a60bb76ba4d4acafbd4cb9aad6e61739";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

fn user(id: &str, username: &str, status: &str) -> Value {
    json!({
        "id": id, "username": username, "display_name": null,
        "avatar_url": null, "status": status, "custom_status": null,
    })
}

fn wire_game(name: &str) -> Value {
    json!({
        "kind": "playing", "name": name, "app_id": "356875221078245376",
        "icon_url": format!("/api/game-icons/356875221078245376/{HASH}.png"),
        "started_at": 1_000, "source": "detected",
    })
}

fn game(name: &str, started_at: i64) -> GameActivity {
    GameActivity {
        app_id: Some("1".into()),
        icon_url: Some(format!("/api/game-icons/1/{HASH}.png")),
        ..GameActivity::playing(name, started_at)
    }
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    rows: Vec<MemberListRow>,
    dms: Vec<DmRow>,
    my_game: Option<(String, String)>,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Members(diff)) => {
                    assert!(diff.clone().apply(&mut self.rows), "{diff:?}");
                }
                Effect::View(ViewUpdate::Dms(diff)) => {
                    assert!(diff.clone().apply(&mut self.dms), "{diff:?}");
                }
                Effect::View(ViewUpdate::MyGame { name, icon }) => {
                    self.my_game = Some((name.clone(), icon.clone()));
                }
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn play(&mut self, activity: Option<GameActivity>) -> Vec<Effect> {
        self.command(Command::SetGameActivity(activity.map(Box::new)))
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .expect("asked");
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn respond_err(&mut self, request: &ApiRequest) -> Vec<Effect> {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .expect("asked");
        let result = Err(yap_core::ApiError::Http {
            status: 500,
            message: "nope".into(),
        });
        self.input(Input::Http { id, result })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        let event = GatewayEvent::Dispatch(Dispatch::decode(event, data).expect("decodes"));
        self.input(Input::Gateway { session, event })
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// Everyone online, as a server that knows game activity lists them.
    fn presence_list(&mut self, with_activity: bool) -> Vec<Effect> {
        let entry = |id: &str| {
            let mut entry = json!({ "user_id": id, "status": "online" });
            if with_activity {
                entry["game_activity"] = Value::Null;
            }
            entry
        };
        let users = vec![entry(ME), entry(ALICE)];
        self.dispatch("PRESENCE_LIST", json!({ "users": users }))
    }

    /// Signed in, members loaded, the gateway up and its list in.
    fn online_with(state: State, new_server: bool) -> Self {
        let mut client = Self {
            state,
            asked: Vec::new(),
            rows: Vec::new(),
            dms: Vec::new(),
            my_game: None,
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
        let members = ApiRequest::Members {
            server_id: SERVER.into(),
        };
        client.respond(
            &members,
            json!([
                { "user": user(ME, "me", "online"), "roles": [] },
                { "user": user(ALICE, "alice", "online"), "roles": [] },
            ]),
        );
        // The server lists presence before Ready, as the gateway does.
        client.presence_list(new_server);
        client.gateway(GatewayEvent::Ready);
        client
    }

    fn online(new_server: bool) -> Self {
        Self::online_with(State::new(BASE), new_server)
    }

    fn member(&self, id: &str) -> &MemberListRow {
        self.rows.iter().find(|r| r.id == id).expect("member row")
    }
}

/// The op 43 frames among `fx`, as their `game_activity` values.
fn sent(fx: &[Effect]) -> Vec<Value> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::GatewaySend(frame)
                if frame.op == i64::from(GatewayOp::ActivityUpdate.code()) =>
            {
                Some(frame.d["game_activity"].clone())
            }
            _ => None,
        })
        .collect()
}

// Others

#[test]
fn a_presence_update_should_show_what_someone_plays() {
    let mut client = Client::online(true);
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "game_activity": wire_game("Overwatch") }),
    );
    let alice = client.member(ALICE);
    assert_eq!(alice.game, "Overwatch");
    assert_eq!(
        alice.game_icon,
        format!("{BASE}/api/game-icons/356875221078245376/{HASH}.png")
    );
}

#[test]
fn a_status_only_update_should_keep_the_game_and_null_should_clear_it() {
    let mut client = Client::online(true);
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "game_activity": wire_game("Celeste") }),
    );
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "idle" }),
    );
    assert_eq!(client.member(ALICE).game, "Celeste");
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "idle", "game_activity": null }),
    );
    assert_eq!(client.member(ALICE).game, "");
}

#[test]
fn going_offline_should_clear_the_game() {
    let mut client = Client::online(true);
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "game_activity": wire_game("Celeste") }),
    );
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "offline" }),
    );
    assert_eq!(client.member(ALICE).game, "");
    // Back online with a status-only update: nothing to show until told.
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online" }),
    );
    assert_eq!(client.member(ALICE).game, "");
}

#[test]
fn an_invalid_activity_should_read_as_none() {
    let mut client = Client::online(true);
    let mut bad = wire_game("Celeste");
    bad["icon_url"] = json!("https://elsewhere.example/icon.png");
    let fx = client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "dnd", "game_activity": bad }),
    );
    assert!(!fx.is_empty(), "the update still applies");
    let alice = client.member(ALICE);
    assert_eq!((alice.presence.as_str(), alice.game.as_str()), ("dnd", ""));
}

#[test]
fn a_presence_list_should_replace_games_and_keep_absent_fields() {
    let mut client = Client::online(true);
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "online", "game_activity": wire_game("Celeste") }),
    );
    // An entry without the field keeps what was known.
    client.dispatch(
        "PRESENCE_LIST",
        json!({ "users": [{ "user_id": ME, "status": "online" }, { "user_id": ALICE, "status": "online" }] }),
    );
    assert_eq!(client.member(ALICE).game, "Celeste");
    // Missing from the list: offline, nothing shown.
    client.dispatch(
        "PRESENCE_LIST",
        json!({ "users": [{ "user_id": ME, "status": "online", "game_activity": null }] }),
    );
    assert_eq!(client.member(ALICE).game, "");
}

// Ours

#[test]
fn our_game_should_be_sent_once_per_change() {
    let mut client = Client::online(true);
    let fx = client.play(Some(game("Celeste", 5)));
    assert_eq!(sent(&fx).len(), 1);
    assert_eq!(sent(&fx)[0]["name"], "Celeste");
    assert_eq!(sent(&fx)[0]["source"], "detected");
    // The same activity again (another scan): nothing sent.
    assert!(sent(&client.play(Some(game("Celeste", 5)))).is_empty());
    // Unrelated inputs send nothing either.
    let fx = client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "idle" }),
    );
    assert!(sent(&fx).is_empty());
    // Quitting the game clears it, once.
    assert_eq!(sent(&client.play(None)), vec![Value::Null]);
    assert!(sent(&client.play(None)).is_empty());
}

#[test]
fn our_game_should_show_on_our_row_and_account_line() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    assert_eq!(client.member(ME).game, "Celeste");
    let (name, icon) = client.my_game.clone().expect("my game");
    assert_eq!(name, "Celeste");
    assert_eq!(icon, format!("{BASE}/api/game-icons/1/{HASH}.png"));
    // The server echoing ours changes nothing.
    client.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ME, "status": "online", "game_activity": null }),
    );
    assert_eq!(client.member(ME).game, "Celeste");
}

#[test]
fn nothing_should_be_sent_before_a_game_was_seen() {
    let mut client = Client::online(true);
    assert!(sent(&client.play(None)).is_empty());
    assert!(sent(&client.gateway(GatewayEvent::Resumed)).is_empty());
}

#[test]
fn ready_and_resumed_should_restate_our_activity() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Resumed);
    assert_eq!(sent(&fx).len(), 1);
    assert_eq!(sent(&fx)[0]["started_at"], 5);
    // While the socket is down nothing goes out; the next Ready restates.
    client.gateway(GatewayEvent::Closed);
    assert!(sent(&client.play(None)).is_empty());
    client.presence_list(true);
    assert_eq!(
        sent(&client.gateway(GatewayEvent::Ready)),
        vec![Value::Null]
    );
}

#[test]
fn sharing_off_should_send_null_once_and_then_nothing() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    let fx = client.command(Command::SetShareActivity(false));
    assert_eq!(sent(&fx), vec![Value::Null]);
    let patch = fx.iter().find_map(|e| match e {
        Effect::Http {
            request: ApiRequest::UpdateProfile(body),
            ..
        } => Some(body.clone()),
        _ => None,
    });
    assert_eq!(patch, Some(json!({ "shareActivity": false })));
    assert_eq!(client.member(ME).game, "");
    assert!(sent(&client.play(Some(game("Hades", 9)))).is_empty());
    client.gateway(GatewayEvent::Closed);
    // Restated as null after a resume, still nothing shown.
    assert_eq!(
        sent(&client.gateway(GatewayEvent::Resumed)),
        vec![Value::Null]
    );
    // Turned back on: the game running now goes out.
    let request = ApiRequest::UpdateProfile(json!({ "shareActivity": false }));
    client.respond(&request, json!({ "user": {} }));
    let fx = client.command(Command::SetShareActivity(true));
    assert_eq!(sent(&fx)[0]["name"], "Hades");
}

#[test]
fn a_failed_share_save_should_roll_back() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    client.command(Command::SetShareActivity(false));
    let request = ApiRequest::UpdateProfile(json!({ "shareActivity": false }));
    let fx = client.respond_err(&request);
    assert_eq!(sent(&fx)[0]["name"], "Celeste");
    assert_eq!(client.member(ME).game, "Celeste");
}

#[test]
fn a_share_flag_from_another_session_should_apply() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    let fx = client.dispatch(
        "USER_PROFILE_UPDATE",
        json!({ "user_id": ME, "share_activity": false }),
    );
    assert_eq!(sent(&fx), vec![Value::Null]);
}

#[test]
fn invisible_should_send_null_and_hide_our_game() {
    let mut client = Client::online(true);
    client.play(Some(game("Celeste", 5)));
    let fx = client.command(Command::SetStatus {
        status: "offline".into(),
        custom_status: None,
    });
    assert_eq!(sent(&fx), vec![Value::Null]);
    assert_eq!(client.my_game.clone().unwrap_or_default().0, "");
    let fx = client.command(Command::SetStatus {
        status: "online".into(),
        custom_status: None,
    });
    assert_eq!(sent(&fx)[0]["name"], "Celeste");
}

#[test]
fn a_server_without_game_activity_should_never_get_op_43() {
    let mut client = Client::online(false);
    assert!(sent(&client.play(Some(game("Celeste", 5)))).is_empty());
    client.gateway(GatewayEvent::Closed);
    assert!(sent(&client.gateway(GatewayEvent::Resumed)).is_empty());
    // Still shown here, as what we would share.
    assert_eq!(client.member(ME).game, "Celeste");
    // A list that carries the field turns sending on.
    assert_eq!(sent(&client.presence_list(true)).len(), 1);
    // And one that does not (the server rolled back) turns it off again.
    client.presence_list(false);
    assert!(sent(&client.play(Some(game("Hades", 9)))).is_empty());
}

#[test]
fn the_send_switch_off_should_never_send() {
    let mut client = Client::online_with(State::new(BASE).with_activity_send(false), true);
    assert!(sent(&client.play(Some(game("Celeste", 5)))).is_empty());
    assert_eq!(client.member(ME).game, "Celeste");
}
