//! Profile boards through the reducer: the board loads when the full
//! profile opens (from the real endpoint's shape), the Rack cells and the
//! widgets' data reach the view, the guestbook posts optimistically and
//! shows the server's words when it refuses, and the editor saves with its
//! version, meets a 409 and keeps the draft on the server's version.
//! Nothing here talks to a server.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::profile_board::EditAction;
use yap_core::store::board::{BoardCommand, BoardRequest, BoardView};
use yap_core::store::view::ViewUpdate;
use yap_core::store::{reduce, ApiRequest, Command, Effect, GatewayEvent, Input, RequestId, State};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    view: BoardView,
}

impl Client {
    fn input(&mut self, input: Input) {
        for effect in reduce(&mut self.state, input) {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((id, request)),
                Effect::View(ViewUpdate::Board(view)) => self.view = *view,
                _ => {}
            }
        }
    }

    fn board(&mut self, command: BoardCommand) {
        self.input(Input::User(Command::Board(command)));
    }

    fn newest(&self, matches: impl Fn(&ApiRequest) -> bool) -> Option<(RequestId, ApiRequest)> {
        self.asked.iter().rev().find(|(_, r)| matches(r)).cloned()
    }

    fn respond_to(&mut self, matches: impl Fn(&ApiRequest) -> bool, body: Value) {
        let (id, request) = self.newest(matches).expect("asked");
        let result = request.decode(body);
        self.input(Input::Http { id, result });
    }

    fn fail(&mut self, matches: impl Fn(&ApiRequest) -> bool, status: u16, message: &str) {
        let (id, _) = self.newest(matches).expect("asked");
        self.input(Input::Http {
            id,
            result: Err(ApiError::Http {
                status,
                message: message.into(),
            }),
        });
    }

    fn signed_in() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            view: BoardView::default(),
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        client.respond_to(|r| *r == ApiRequest::Me, fixture("user-me.json"));
        client.respond_to(|r| *r == ApiRequest::Servers, fixture("servers-list.json"));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }

    fn state(&self) -> Value {
        serde_json::from_str(&self.view.state).expect("state json")
    }

    fn rack(&self) -> Value {
        serde_json::from_str(&self.view.rack).expect("rack json")
    }

    fn data(&self) -> Value {
        serde_json::from_str(&self.view.data).expect("data json")
    }
}

fn is_get(r: &ApiRequest) -> bool {
    matches!(r, ApiRequest::Board(BoardRequest::Get(_)))
}

fn is_save(r: &ApiRequest) -> bool {
    matches!(r, ApiRequest::Board(BoardRequest::Save { .. }))
}

fn own_board() -> Client {
    let mut client = Client::signed_in();
    client.board(BoardCommand::Open {
        user_id: "u-me".into(),
        now_ms: 1_795_811_400_000,
    });
    assert_eq!(client.state()["loading"], true);
    client.respond_to(is_get, fixture("profile-board.json"));
    client
}

#[test]
fn opening_a_profile_loads_its_board_into_rack_cells() {
    let client = own_board();
    let (_, request) = client.newest(is_get).unwrap();
    assert_eq!(request.path(), "/api/users/u-me/board");
    let state = client.state();
    assert_eq!(state["loading"], false);
    assert_eq!(state["isOwner"], true);
    assert_eq!(state["count"], 13);
    let rack = client.rack();
    let cells = rack["cells"].as_array().unwrap();
    assert_eq!(cells.len(), 13, "the owner sees their draft too");
    // now_playing L takes the first row; mood S and countdown S share the next.
    assert_eq!(
        (
            cells[0]["col"].as_u64(),
            cells[0]["row"].as_u64(),
            cells[0]["cols"].as_u64()
        ),
        (Some(0), Some(0), Some(2))
    );
    assert_eq!(
        (cells[1]["row"].as_u64(), cells[2]["col"].as_u64()),
        (Some(1), Some(1))
    );
    let custom = cells.iter().find(|c| c["type"] == "custom").unwrap();
    assert_eq!(custom["draft"], true);
    assert_eq!(
        custom["cfg"]["asset_url"],
        "https://api.fixture.invalid/api/profile-assets/widget/u-me/kiwicustom0001chibi.webp"
    );
    let data = client.data();
    assert_eq!(data["top_games"]["games"].as_array().map(Vec::len), Some(5));
    assert!(data["badges_view"].as_str().unwrap().contains("night-owl"));
}

#[test]
fn others_never_get_drafts_and_an_empty_board_says_so() {
    let mut client = Client::signed_in();
    client.board(BoardCommand::Open {
        user_id: "u-fixture-peer".into(),
        now_ms: 0,
    });
    client.respond_to(is_get, fixture("profile-board-empty.json"));
    let state = client.state();
    assert_eq!(state["empty"], true);
    assert_eq!(state["isOwner"], false);
    let mut board = fixture("profile-board.json");
    board["is_owner"] = json!(false);
    client.board(BoardCommand::Retry);
    client.board(BoardCommand::Open {
        user_id: "u-me".into(),
        now_ms: 0,
    });
    client.respond_to(is_get, board);
    let cells = client.rack()["cells"].as_array().unwrap().clone();
    assert!(cells.iter().all(|c| c["type"] != "custom"));
    assert_eq!(client.state()["count"], 12);
}

#[test]
fn a_guestbook_post_shows_at_once_and_the_servers_refusal_comes_back() {
    let mut client = own_board();
    client.board(BoardCommand::GuestbookPost {
        body: "  hello   there ".into(),
        now_ms: 5,
    });
    let notes = client.data()["guestbook"]["notes"].clone();
    assert_eq!(notes[0]["body"], "hello there");
    assert_eq!(notes[0]["id"], "pending-5");
    assert_eq!(client.state()["posting"], true);
    client.fail(
        |r| matches!(r, ApiRequest::Board(BoardRequest::GuestbookPost { .. })),
        429,
        "You've left 5 notes today.",
    );
    assert_eq!(
        client.state()["guestbookError"],
        "You've left 5 notes today."
    );
    assert_ne!(client.data()["guestbook"]["notes"][0]["id"], "pending-5");

    // The owner deletes a note: gone at once, and the total follows.
    let before = client.data()["guestbook"]["total"].as_i64().unwrap();
    client.board(BoardCommand::GuestbookDelete("note-juno".into()));
    let book = client.data()["guestbook"].clone();
    assert!(book["notes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|n| n["id"] != "note-juno"));
    assert_eq!(book["total"].as_i64(), Some(before - 1));
    let (_, request) = client
        .newest(|r| matches!(r, ApiRequest::Board(BoardRequest::GuestbookDelete { .. })))
        .unwrap();
    assert_eq!(request.path(), "/api/users/u-me/guestbook/note-juno");

    // Block: their notes go, a notice names them.
    client.board(BoardCommand::GuestbookBlock("u-mo".into()));
    client.respond_to(
        |r| matches!(r, ApiRequest::Board(BoardRequest::GuestbookBlock(_))),
        json!({ "user_id": "u-mo", "blocked": true }),
    );
    assert_eq!(
        client.state()["guestbookNotice"],
        "Mo can't sign your guestbook any more."
    );

    // Off switch.
    client.board(BoardCommand::GuestbookEnabled(false));
    assert_eq!(client.data()["guestbook"]["enabled"], false);
}

#[test]
fn the_editor_saves_with_its_version_and_handles_a_conflict() {
    let mut client = own_board();
    client.board(BoardCommand::Edit(true));
    assert_eq!(client.state()["editing"], true);
    client.board(BoardCommand::Act(EditAction::Remove("w-mood".into())));
    // The web fixture holds 13 (over the cap): make room first.
    client.board(BoardCommand::Act(EditAction::Remove("w-links".into())));
    client.board(BoardCommand::Act(EditAction::Add {
        kind: "mood".into(),
        index: Some(0),
        seed: 0,
    }));
    let state = client.state();
    assert_eq!(
        (state["dirty"].as_bool(), state["canUndo"].as_bool()),
        (Some(true), Some(true))
    );
    assert_eq!(client.rack()["cells"][0]["type"], "mood");
    client.board(BoardCommand::Done);
    let (_, save) = client.newest(is_save).unwrap();
    let ApiRequest::Board(BoardRequest::Save { version, .. }) = save else {
        unreachable!()
    };
    assert_eq!(version, 7);

    // 409: the server's copy is fetched and the choice offered.
    client.fail(is_save, 409, "Board changed");
    let mut theirs = fixture("profile-board.json");
    theirs["version"] = json!(9);
    client.respond_to(is_get, theirs);
    assert_eq!(client.state()["conflict"], true);
    client.board(BoardCommand::KeepMine);
    let (_, save) = client.newest(is_save).unwrap();
    let ApiRequest::Board(BoardRequest::Save { version, board }) = save else {
        unreachable!()
    };
    assert_eq!(version, 9);
    assert_eq!(board.widgets[0].kind, "mood");

    // The save lands: the editor closes (Done) and the board refetches.
    let saved = json!({ "board": board, "version": 10, "drafts": ["w-custom"], "dropped": [], "updated_at": "2026-11-27T20:31:00Z" });
    client.respond_to(is_save, saved);
    assert_eq!(client.state()["editing"], false);
    assert_eq!(client.state()["version"], 10);
}

#[test]
fn tabs_load_once_and_activity_groups_by_day() {
    let mut client = own_board();
    client.board(BoardCommand::Tab("activity".into()));
    client.respond_to(
        |r| matches!(r, ApiRequest::Board(BoardRequest::Activity { .. })),
        fixture("profile-activity.json"),
    );
    let activity: Value = serde_json::from_str(&client.view.activity).unwrap();
    assert_eq!(activity["loaded"], true);
    let days = activity["days"].as_array().unwrap();
    assert!(!days.is_empty());
    assert_eq!(days[0]["events"][0]["title"], "Played Deadlock");
    client.board(BoardCommand::Tab("board".into()));
    let asked = client.asked.len();
    client.board(BoardCommand::Tab("activity".into()));
    assert_eq!(client.asked.len(), asked, "a loaded tab is not asked again");

    client.board(BoardCommand::Tab("wishlist".into()));
    client.respond_to(
        |r| matches!(r, ApiRequest::Board(BoardRequest::Wishlist(_))),
        fixture("profile-wishlist.json"),
    );
    let wishlist: Value = serde_json::from_str(&client.view.wishlist).unwrap();
    assert_eq!(wishlist["items"].as_array().map(Vec::len), Some(6));
}

#[test]
fn a_board_update_refetches_the_open_profile() {
    let mut client = own_board();
    let asked = client.asked.len();
    client.board(BoardCommand::Updated {
        user_id: "someone-else".into(),
        version: 99,
        data: false,
    });
    assert_eq!(client.asked.len(), asked);
    client.board(BoardCommand::Updated {
        user_id: "u-me".into(),
        version: 8,
        data: false,
    });
    assert!(client.newest(is_get).is_some() && client.asked.len() == asked + 1);
}

#[test]
fn previewing_as_anyone_drops_owner_only_plates() {
    let mut board = fixture("profile-board.json");
    board["data"]["deadlock"] =
        json!({ "state": "not_linked", "digest": null, "fetched_at": null, "stale": false });
    // The web fixture has no Deadlock plate: it takes the second Links slot.
    board["board"]["widgets"][10] =
        json!({ "id": "w-deadlock", "type": "deadlock", "size": "M", "cfg": {} });
    let mut client = Client::signed_in();
    client.board(BoardCommand::Open {
        user_id: "u-me".into(),
        now_ms: 0,
    });
    client.respond_to(is_get, board);
    let types = |client: &Client| -> Vec<String> {
        client.rack()["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["type"].as_str().unwrap().to_owned())
            .collect()
    };
    assert!(
        types(&client).contains(&"deadlock".to_owned()),
        "the owner sees the setup plate"
    );
    client.board(BoardCommand::Edit(true));
    client.board(BoardCommand::Act(EditAction::Preview(
        yap_core::profile_board::Preview::Anyone,
    )));
    let shown = types(&client);
    assert!(!shown.contains(&"deadlock".to_owned()));
    assert!(!shown.contains(&"custom".to_owned()));
}

#[test]
fn the_pinned_message_carries_its_markdown_as_rich_text() {
    let mut board = fixture("profile-board.json");
    board["board"]["widgets"][7]["cfg"]["snapshot"]["text"] = json!("This tests the **Qt** client");
    let mut client = Client::signed_in();
    client.board(BoardCommand::Open {
        user_id: "u-me".into(),
        now_ms: 0,
    });
    client.respond_to(is_get, board);
    let rack = client.rack();
    let quote = rack["cells"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["type"] == "quote")
        .unwrap()
        .clone();
    let html = quote["cfg"]["snapshot"]["html"].as_str().unwrap();
    assert!(
        html.contains("SemiBold") && html.contains(">Qt</span>"),
        "{html}"
    );
    assert!(!html.contains("**"));
}

#[test]
fn the_guestbook_passes_the_servers_denial_on() {
    let mut board = fixture("profile-board.json");
    board["is_owner"] = json!(false);
    board["data"]["guestbook"]["can_post"] = json!(false);
    board["data"]["guestbook"]["denial"] = json!("GUESTBOOK_BLOCKED");
    let mut client = Client::signed_in();
    client.board(BoardCommand::Open {
        user_id: "u-me".into(),
        now_ms: 0,
    });
    client.respond_to(is_get, board);
    assert_eq!(client.data()["guestbook"]["denial"], "GUESTBOOK_BLOCKED");
    // A closed guestbook says so whatever the server's reason was.
    client.board(BoardCommand::GuestbookEnabled(false));
    assert_eq!(client.data()["guestbook"]["denial"], "GUESTBOOK_OFF");
}

#[test]
fn previews_sign_the_guestbook_as_their_audience_would() {
    let mut client = own_board();
    client.board(BoardCommand::Edit(true));
    client.board(BoardCommand::Act(EditAction::Preview(
        yap_core::profile_board::Preview::Friend,
    )));
    assert_eq!(client.data()["guestbook"]["can_post"], true);
    client.board(BoardCommand::Act(EditAction::Preview(
        yap_core::profile_board::Preview::Anyone,
    )));
    let book = client.data()["guestbook"].clone();
    assert_eq!(book["can_post"], false);
    assert_eq!(book["denial"], "GUESTBOOK_NO_SHARED_SPACE");
}
