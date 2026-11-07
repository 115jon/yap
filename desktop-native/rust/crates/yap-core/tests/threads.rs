//! Threads, pins, jump to message and the message link: request shapes as
//! the web sends them, the thread panel's rows and live updates, reply
//! counts, pinning with its permission and rollback, and the detached
//! window a jump loads (read marks, live messages, newer pages, jump to
//! present).

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::admin::perm;
use yap_core::store::view::MessageRow;
use yap_core::store::{
    message_link, reduce, ApiRequest, ApiResponse, CacheWrite, Command, Dispatch, Effect,
    GatewayEvent, Input, Method, PanelsView, PinsCommand, RequestId, State, ThreadCommand,
    ThreadRequest, ViewDiff, ViewUpdate,
};
use yap_core::ApiError;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const A: &str = "chan_a";
const B: &str = "chan_b";
const ME: &str = "user_fixture_me";
const ALICE: &str = "user_fixture_alice";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text = std::fs::read_to_string(&path).expect("fixture");
    serde_json::from_str(&text).expect("json")
}

fn user(id: &str, username: &str) -> Value {
    json!({ "id": id, "username": username, "display_name": null, "avatar_url": null })
}

/// A message by Alice (or me), `minute` minutes past noon.
fn msg(channel: &str, id: &str, author: &str, minute: u32) -> Value {
    json!({
        "id": id, "channel_id": channel, "author_id": author,
        "author": user(author, if author == ME { "me" } else { "alice" }),
        "content": format!("text of {id}"), "is_pinned": false,
        "created_at": format!("2026-11-04T12:{minute:02}:00.000Z"),
        "attachments": [], "embeds": [],
    })
}

fn reply(channel: &str, id: &str, author: &str, minute: u32, parent: &str) -> Value {
    let mut message = msg(channel, id, author, minute);
    message["reply_to_id"] = json!(parent);
    message
}

fn tree() -> Value {
    json!({
        "categories": [],
        "channels": [
            { "id": A, "server_id": SERVER, "name": "general", "channel_type": "text",
              "category_id": null, "position": 0 },
            { "id": B, "server_id": SERVER, "name": "random", "channel_type": "text",
              "category_id": null, "position": 1 },
        ],
    })
}

fn members(my_permissions: i64) -> Value {
    let role = |id: &str, permissions: i64| {
        json!({ "id": id, "name": id, "color": null, "permissions": permissions,
                "position": 0, "is_default": false })
    };
    json!([
        { "user": user(ME, "me"), "roles": [role("r_me", my_permissions)] },
        { "user": user(ALICE, "alice"), "roles": [role("r_alice", 0)] },
    ])
}

fn page(messages: Vec<Value>, more: bool) -> Value {
    json!({ "messages": messages, "hasMoreBefore": more, "hasMoreAfter": false })
}

/// m1..m3 in A: m1 by Alice, m2 my reply to it, m3 Alice's.
fn three() -> Vec<Value> {
    vec![
        msg(A, "m1", ALICE, 0),
        reply(A, "m2", ME, 1, "m1"),
        msg(A, "m3", ALICE, 2),
    ]
}

fn threads(request: ThreadRequest) -> ApiRequest {
    ApiRequest::Threads(request)
}

#[derive(Default)]
struct Seen {
    rows: Vec<MessageRow>,
    thread: Vec<MessageRow>,
    pins: Vec<MessageRow>,
    panels: PanelsView,
    jumps: Vec<String>,
    newer: (bool, bool),
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    seen: Seen,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(update) => self.see(update),
                _ => {}
            }
        }
        fx
    }

    fn see(&mut self, update: &ViewUpdate) {
        let seen = &mut self.seen;
        let apply = |diff: &ViewDiff<MessageRow>, rows: &mut Vec<MessageRow>| {
            assert!(diff.clone().apply(rows), "{diff:?}");
        };
        match update {
            ViewUpdate::Messages(diff) => apply(diff, &mut seen.rows),
            ViewUpdate::ThreadMessages(diff) => apply(diff, &mut seen.thread),
            ViewUpdate::PinnedMessages(diff) => apply(diff, &mut seen.pins),
            ViewUpdate::Panels(view) => seen.panels = (**view).clone(),
            ViewUpdate::JumpTo(id) => seen.jumps.push(id.clone()),
            ViewUpdate::Newer { loading, more } => seen.newer = (*loading, *more),
            _ => {}
        }
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn last(&self, pick: impl Fn(&ApiRequest) -> bool) -> Option<(RequestId, ApiRequest)> {
        self.asked.iter().rev().find(|(_, r)| pick(r)).cloned()
    }

    fn asked_for(&self, request: &ApiRequest) -> bool {
        self.asked.iter().any(|(_, r)| r == request)
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let (id, _) = self
            .last(|r| r == request)
            .unwrap_or_else(|| panic!("no {request:?} in {:#?}", self.asked));
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn fail(&mut self, request: &ApiRequest, status: u16) -> Vec<Effect> {
        let (id, _) = self.last(|r| r == request).expect("asked");
        let result = Err(ApiError::Http {
            status,
            message: "nope".into(),
        });
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

    /// Signed in, channel A open with `messages`, members loaded with my
    /// role's `permissions`, read states answered, the list at its end.
    fn online(messages: Vec<Value>, permissions: i64) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            seen: Seen::default(),
        };
        client.command(Command::UseToken("tok_fixture".into()));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        let channels = ApiRequest::Channels {
            server_id: SERVER.into(),
        };
        client.respond(&channels, tree());
        client.respond(&messages_of(A), page(messages, true));
        let members_request = ApiRequest::Members {
            server_id: SERVER.into(),
        };
        client.respond(&members_request, members(permissions));
        client.respond(
            &ApiRequest::ReadStates,
            json!({ "read_states": [], "last_messages": [] }),
        );
        client.respond(&ApiRequest::Notifications, json!({ "notifications": [] }));
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client.command(Command::SetAtBottom(true));
        client
    }

    fn row(&self, id: &str) -> &MessageRow {
        self.seen
            .rows
            .iter()
            .find(|r| r.id == id)
            .unwrap_or_else(|| panic!("no row {id}"))
    }

    fn ids(rows: &[MessageRow]) -> Vec<&str> {
        rows.iter().map(|r| r.id.as_str()).collect()
    }

    fn open_thread(&mut self, root: &str, replies: Vec<Value>) {
        self.command(Command::Thread(ThreadCommand::Open(root.into())));
        let root_message = self
            .state
            .messages()
            .and_then(|log| log.get(root))
            .map(|m| serde_json::to_value(m).expect("encodes"))
            .unwrap_or_else(|| msg(A, root, ALICE, 0));
        let body = json!({ "root": root_message, "replies": replies, "reply_count": 0 });
        self.respond(&thread_of(root), body);
    }

    fn mark_reads(&self) -> usize {
        self.asked
            .iter()
            .filter(|(_, r)| matches!(r, ApiRequest::MarkRead { .. }))
            .count()
    }
}

fn messages_of(channel: &str) -> ApiRequest {
    ApiRequest::Messages {
        channel_id: channel.into(),
        before: None,
    }
}

fn thread_of(root: &str) -> ApiRequest {
    threads(ThreadRequest::Thread {
        channel_id: A.into(),
        message_id: root.into(),
    })
}

fn pins_of(channel: &str) -> ApiRequest {
    threads(ThreadRequest::Pins {
        channel_id: channel.into(),
    })
}

fn set_pin(id: &str, pinned: bool) -> ApiRequest {
    threads(ThreadRequest::SetPin {
        channel_id: A.into(),
        message_id: id.into(),
        pinned,
    })
}

fn around(channel: &str, id: &str) -> ApiRequest {
    threads(ThreadRequest::Around {
        channel_id: channel.into(),
        message_id: id.into(),
    })
}

fn jump(channel: &str, id: &str) -> Command {
    Command::JumpToMessage {
        channel_id: channel.into(),
        message_id: id.into(),
    }
}

/// A window around `old5` in A: old1..old9, more after it.
fn window(more_after: bool) -> Value {
    let messages: Vec<Value> = (1..=9)
        .map(|i| msg(A, &format!("old{i}"), ALICE, i))
        .collect();
    json!({ "messages": messages, "hasMoreBefore": true, "hasMoreAfter": more_after })
}

/// Channel A jumped to `old5`, the window detached.
fn detached() -> Client {
    let mut client = Client::online(three(), 0);
    client.command(jump(A, "old5"));
    client.respond(&around(A, "old5"), window(true));
    client
}

// Request shapes

#[test]
fn thread_request_should_ask_for_the_root_by_message_id() {
    let request = ThreadRequest::Thread {
        channel_id: A.into(),
        message_id: "m1".into(),
    };
    assert_eq!(
        (request.method(), request.path()),
        (
            Method::Get,
            "/api/channels/chan_a/thread?message_id=m1".into()
        )
    );
}

#[test]
fn pin_request_should_put_the_message_id_and_flag() {
    let request = ThreadRequest::SetPin {
        channel_id: A.into(),
        message_id: "m1".into(),
        pinned: true,
    };
    assert_eq!(
        (request.method(), request.path(), request.body()),
        (
            Method::Put,
            "/api/channels/chan_a/pins".into(),
            json!({ "message_id": "m1", "pinned": true })
        )
    );
}

#[test]
fn around_request_should_carry_only_the_message_id() {
    let request = ThreadRequest::Around {
        channel_id: A.into(),
        message_id: "m 1".into(),
    };
    assert_eq!(request.path(), "/api/channels/chan_a/messages?around=m%201");
}

#[test]
fn after_request_should_encode_the_cursor_with_a_limit_of_50() {
    let request = ThreadRequest::After {
        channel_id: A.into(),
        after: "2026-11-04T12:00:00.000Z|m1".into(),
    };
    assert_eq!(
        request.path(),
        "/api/channels/chan_a/messages?after=2026-11-04T12%3A00%3A00.000Z%7Cm1&limit=50"
    );
}

#[test]
fn message_link_should_match_the_web_format() {
    assert_eq!(
        message_link("https://yap.example/", Some(SERVER), A, "m1"),
        "https://yap.example/chat/srv_fixture/chan_a?message=m1"
    );
}

#[test]
fn message_link_outside_a_server_should_use_at_me() {
    assert_eq!(
        message_link("https://yap.example", None, "dm_1", "m1"),
        "https://yap.example/chat/%40me/dm_1?message=m1"
    );
}

#[test]
fn message_unpin_should_decode_its_id() {
    let dispatch = Dispatch::decode(
        "MESSAGE_UNPIN",
        json!({ "id": "m1", "channel_id": A, "is_pinned": false }),
    );
    assert!(matches!(dispatch, Some(Dispatch::MessageUnpin(e)) if e.id == "m1"));
}

// Reply counts

#[test]
fn a_page_should_carry_reply_counts() {
    let mut root = msg(A, "m1", ALICE, 0);
    root["reply_count"] = json!(4);
    let client = Client::online(vec![root], 0);
    assert_eq!(client.row("m1").reply_count, 4);
}

#[test]
fn a_live_reply_should_bump_its_parents_count() {
    let mut client = Client::online(three(), 0);
    client.dispatch("MESSAGE_CREATE", reply(A, "m4", ALICE, 3, "m1"));
    assert_eq!(client.row("m1").reply_count, 1);
}

#[test]
fn a_deleted_reply_should_lower_its_parents_count() {
    let mut root = msg(A, "m1", ALICE, 0);
    root["reply_count"] = json!(1);
    let mut client = Client::online(vec![root, reply(A, "m2", ME, 1, "m1")], 0);
    client.dispatch("MESSAGE_DELETE", json!({ "id": "m2", "channel_id": A }));
    assert_eq!(client.row("m1").reply_count, 0);
}

#[test]
fn our_reply_should_count_once_after_its_echo() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Send {
        content: "hi".into(),
        reply_to_id: Some("m1".into()),
        nonce: "n1".into(),
        sent_at: "2026-11-04T12:05:00.000Z".into(),
    });
    let mut echo = reply(A, "m9", ME, 5, "m1");
    echo["nonce"] = json!("n1");
    client.dispatch("MESSAGE_CREATE", echo);
    assert_eq!(client.row("m1").reply_count, 1);
}

// The thread panel

#[test]
fn a_thread_should_show_its_root_then_the_replies() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "m2", ME, 1, "m1")]);
    assert_eq!(Client::ids(&client.seen.thread), ["m1", "m2"]);
}

#[test]
fn thread_rows_should_hide_the_reply_quote() {
    let mut client = Client::online(three(), 0);
    let mut answer = reply(A, "m2", ME, 1, "m1");
    answer["reply_to"] = json!({ "id": "m1", "content": "text of m1", "author_id": ALICE });
    client.open_thread("m1", vec![answer]);
    assert_eq!(client.seen.thread[1].reply_id, "");
}

#[test]
fn replies_by_one_author_within_five_minutes_should_group() {
    let mut client = Client::online(three(), 0);
    client.open_thread(
        "m1",
        vec![
            reply(A, "r1", ME, 1, "m1"),
            reply(A, "r2", ME, 3, "m1"),
            reply(A, "r3", ME, 9, "m1"),
        ],
    );
    let grouped: Vec<bool> = client.seen.thread.iter().map(|r| r.grouped).collect();
    assert_eq!(grouped, [false, false, true, false]);
}

#[test]
fn a_thread_should_count_its_replies() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "m2", ME, 1, "m1")]);
    assert_eq!(client.seen.panels.thread_replies, 1);
}

#[test]
fn a_failed_thread_load_should_say_why() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Thread(ThreadCommand::Open("m1".into())));
    client.fail(&thread_of("m1"), 500);
    assert_eq!(
        client.seen.panels.thread_error,
        "Unable to load this thread. Try again in a moment."
    );
}

#[test]
fn a_live_reply_should_append_to_the_open_thread() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.dispatch("MESSAGE_CREATE", reply(A, "m4", ALICE, 3, "m1"));
    assert_eq!(Client::ids(&client.seen.thread), ["m1", "m4"]);
}

#[test]
fn a_message_that_is_not_a_reply_should_stay_out_of_the_thread() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.dispatch("MESSAGE_CREATE", msg(A, "m4", ALICE, 3));
    assert_eq!(Client::ids(&client.seen.thread), ["m1"]);
}

#[test]
fn a_deleted_reply_should_leave_the_thread() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "m2", ME, 1, "m1")]);
    client.dispatch("MESSAGE_DELETE", json!({ "id": "m2", "channel_id": A }));
    assert_eq!(Client::ids(&client.seen.thread), ["m1"]);
}

#[test]
fn an_edit_should_reach_a_thread_row() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "r1", ALICE, 1, "m1")]);
    client.dispatch(
        "MESSAGE_UPDATE",
        json!({ "id": "r1", "channel_id": A, "content": "changed", "updated_at": "2026-11-04T13:00:00.000Z" }),
    );
    assert_eq!(client.seen.thread[1].body, "changed");
}

#[test]
fn a_reaction_should_reach_a_thread_row() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "r1", ALICE, 1, "m1")]);
    client.dispatch(
        "REACTION_ADD",
        json!({ "message_id": "r1", "channel_id": A, "user_id": ALICE, "emoji": "+1" }),
    );
    assert!(!client.seen.thread[1].reactions.is_empty());
}

fn thread_send(content: &str) -> Command {
    Command::Thread(ThreadCommand::Send {
        content: content.into(),
        nonce: "t1".into(),
        sent_at: "2026-11-04T12:30:00.000Z".into(),
    })
}

fn thread_post() -> ApiRequest {
    ApiRequest::SendMessage {
        channel_id: A.into(),
        content: "hello".into(),
        reply_to_id: Some("m1".into()),
        nonce: "t1".into(),
        attachment_ids: Vec::new(),
        nsfw_attachment_ids: Vec::new(),
    }
}

#[test]
fn a_thread_reply_should_post_with_the_root_as_reply_to_id() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.command(thread_send("  hello "));
    assert!(client.asked_for(&thread_post()));
}

#[test]
fn a_thread_reply_should_show_pending_at_once() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.command(thread_send("hello"));
    assert!(client.seen.thread[1].pending);
}

#[test]
fn a_failed_thread_reply_should_drop_its_row() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.command(thread_send("hello"));
    client.fail(&thread_post(), 500);
    assert_eq!(Client::ids(&client.seen.thread), ["m1"]);
}

#[test]
fn the_echo_should_take_the_pending_thread_rows_place() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![]);
    client.command(thread_send("hello"));
    let mut echo = reply(A, "m9", ME, 30, "m1");
    echo["nonce"] = json!("t1");
    client.dispatch("MESSAGE_CREATE", echo);
    assert_eq!(Client::ids(&client.seen.thread), ["m1", "m9"]);
}

#[test]
fn closing_the_thread_should_empty_its_rows() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "m2", ME, 1, "m1")]);
    client.command(Command::Thread(ThreadCommand::Close));
    assert!(client.seen.thread.is_empty());
}

// Pins

#[test]
fn manage_messages_should_allow_pinning() {
    let client = Client::online(three(), perm::MANAGE_MESSAGES);
    assert!(client.seen.panels.can_pin);
}

#[test]
fn a_plain_member_should_not_pin() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Pins(PinsCommand::SetPinned {
        message_id: "m1".into(),
        pinned: true,
    }));
    assert!(!client.asked_for(&set_pin("m1", true)));
}

#[test]
fn pinning_should_put_the_pin_and_show_the_glyph_at_once() {
    let mut client = Client::online(three(), perm::MANAGE_MESSAGES);
    client.command(Command::Pins(PinsCommand::SetPinned {
        message_id: "m1".into(),
        pinned: true,
    }));
    assert!(client.asked_for(&set_pin("m1", true)) && client.row("m1").pinned);
}

#[test]
fn a_refused_pin_should_be_undone() {
    let mut client = Client::online(three(), perm::MANAGE_MESSAGES);
    client.command(Command::Pins(PinsCommand::SetPinned {
        message_id: "m1".into(),
        pinned: true,
    }));
    client.fail(&set_pin("m1", true), 400);
    assert!(!client.row("m1").pinned);
}

#[test]
fn a_refused_unpin_should_put_it_back_on_the_list() {
    let mut pinned = msg(A, "m1", ALICE, 0);
    pinned["is_pinned"] = json!(true);
    let mut client = Client::online(vec![pinned.clone()], perm::MANAGE_MESSAGES);
    client.command(Command::Pins(PinsCommand::Open));
    client.respond(&pins_of(A), json!([pinned]));
    client.command(Command::Pins(PinsCommand::SetPinned {
        message_id: "m1".into(),
        pinned: false,
    }));
    client.fail(&set_pin("m1", false), 500);
    assert_eq!(Client::ids(&client.seen.pins), ["m1"]);
}

#[test]
fn pins_should_load_once_per_channel() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Pins(PinsCommand::Open));
    client.respond(&pins_of(A), json!([]));
    client.command(Command::Pins(PinsCommand::Open));
    let asked = client
        .asked
        .iter()
        .filter(|(_, r)| *r == pins_of(A))
        .count();
    assert_eq!(asked, 1);
}

#[test]
fn a_pin_dispatch_should_add_the_message_newest_first() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Pins(PinsCommand::Open));
    let mut old = msg(A, "m1", ALICE, 0);
    old["is_pinned"] = json!(true);
    client.respond(&pins_of(A), json!([old]));
    let mut pinned = msg(A, "m3", ALICE, 2);
    pinned["is_pinned"] = json!(true);
    client.dispatch("MESSAGE_PIN", pinned);
    assert_eq!(Client::ids(&client.seen.pins), ["m3", "m1"]);
}

#[test]
fn a_pin_dispatch_should_flag_the_row() {
    let mut client = Client::online(three(), 0);
    let mut pinned = msg(A, "m3", ALICE, 2);
    pinned["is_pinned"] = json!(true);
    client.dispatch("MESSAGE_PIN", pinned);
    assert!(client.row("m3").pinned);
}

#[test]
fn an_unpin_dispatch_should_take_it_off_the_list() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Pins(PinsCommand::Open));
    let mut old = msg(A, "m1", ALICE, 0);
    old["is_pinned"] = json!(true);
    client.respond(&pins_of(A), json!([old]));
    client.dispatch(
        "MESSAGE_UNPIN",
        json!({ "id": "m1", "channel_id": A, "is_pinned": false }),
    );
    assert!(client.seen.pins.is_empty());
}

#[test]
fn a_pin_in_another_channel_should_stay_off_the_list() {
    let mut client = Client::online(three(), 0);
    client.command(Command::Pins(PinsCommand::Open));
    client.respond(&pins_of(A), json!([]));
    let mut pinned = msg(B, "b1", ALICE, 2);
    pinned["is_pinned"] = json!(true);
    client.dispatch("MESSAGE_PIN", pinned);
    assert!(client.seen.pins.is_empty());
}

// Jump to message

#[test]
fn jumping_to_a_loaded_message_should_highlight_it_without_a_request() {
    let mut client = Client::online(three(), 0);
    client.command(jump(A, "m2"));
    assert!(client.seen.jumps == ["m2"] && !client.asked_for(&around(A, "m2")));
}

#[test]
fn jumping_to_an_unloaded_message_should_ask_for_its_window() {
    let mut client = Client::online(three(), 0);
    client.command(jump(A, "old5"));
    assert!(client.asked_for(&around(A, "old5")));
}

#[test]
fn the_window_should_replace_the_list_and_highlight_the_message() {
    let client = detached();
    assert!(
        Client::ids(&client.seen.rows).first() == Some(&"old1") && client.seen.jumps == ["old5"]
    );
}

#[test]
fn a_window_short_of_the_end_should_detach_the_list() {
    let client = detached();
    assert_eq!(client.seen.newer, (false, true));
}

#[test]
fn a_detached_list_should_not_mark_read_at_its_end() {
    let mut client = detached();
    let before = client.mark_reads();
    client.command(Command::SetAtBottom(true));
    assert_eq!(client.mark_reads(), before);
}

#[test]
fn a_detached_list_should_not_append_live_messages() {
    let mut client = detached();
    client.dispatch("MESSAGE_CREATE", msg(A, "m4", ALICE, 30));
    assert!(!client.seen.rows.iter().any(|r| r.id == "m4"));
}

#[test]
fn a_live_message_in_a_detached_window_should_not_mark_read() {
    let mut client = detached();
    let before = client.mark_reads();
    client.dispatch("MESSAGE_CREATE", msg(A, "m4", ALICE, 30));
    assert_eq!(client.mark_reads(), before);
}

#[test]
fn a_live_reply_in_a_detached_window_should_bump_a_loaded_parent() {
    let mut client = detached();
    assert_eq!(client.row("old5").reply_count, 0);
    client.dispatch("MESSAGE_CREATE", reply(A, "m4", ALICE, 30, "old5"));
    assert!(client.row("old5").reply_count == 1 && !client.seen.rows.iter().any(|r| r.id == "m4"));
}

#[test]
fn a_detached_window_should_not_reach_the_cache() {
    let mut client = detached();
    let edit = json!({ "id": "old5", "channel_id": A, "content": "changed" });
    let fx = client.dispatch("MESSAGE_UPDATE", edit);
    let wrote = fx.iter().any(|e| {
        matches!(
            e,
            Effect::CacheWrite {
                write: CacheWrite::Messages { .. },
                ..
            }
        )
    });
    assert!(!wrote);
}

fn after_old9() -> ApiRequest {
    threads(ThreadRequest::After {
        channel_id: A.into(),
        after: "2026-11-04T12:09:00.000Z|old9".into(),
    })
}

#[test]
fn load_newer_should_ask_after_the_windows_newest_message() {
    let mut client = detached();
    client.command(Command::LoadNewer);
    assert!(client.asked_for(&after_old9()));
}

#[test]
fn the_newest_page_should_attach_the_list_and_mark_it_read() {
    let mut client = detached();
    client.command(Command::SetAtBottom(true));
    client.command(Command::LoadNewer);
    let before = client.mark_reads();
    let rest = json!({ "messages": [msg(A, "m4", ALICE, 30)], "hasMoreAfter": false });
    client.respond(&after_old9(), rest);
    assert!(client.seen.newer == (false, false) && client.mark_reads() > before);
}

#[test]
fn jump_to_present_should_load_the_newest_page_in_place_of_the_window() {
    let mut client = detached();
    client.command(Command::JumpToPresent);
    client.respond(&messages_of(A), page(three(), true));
    assert_eq!(Client::ids(&client.seen.rows), ["m1", "m2", "m3"]);
}

#[test]
fn jumping_into_another_channel_should_open_it_then_jump() {
    let mut client = Client::online(three(), 0);
    client.command(jump(B, "b2"));
    client.respond(
        &messages_of(B),
        page(vec![msg(B, "b1", ALICE, 0), msg(B, "b2", ALICE, 1)], false),
    );
    assert!(client.state.active_channel() == Some(B) && client.seen.jumps == ["b2"]);
}

#[test]
fn jumping_into_an_unknown_channel_should_say_it_is_not_available() {
    let mut client = Client::online(three(), 0);
    let fx = client.command(jump("chan_nowhere", "m1"));
    assert!(fx.contains(&Effect::View(ViewUpdate::Notice(
        "That channel isn't available".into()
    ))));
    assert_eq!(client.state.active_channel(), Some(A));
}

#[test]
fn a_reply_quote_jump_outside_the_page_should_use_the_window() {
    let mut client = Client::online(three(), 0);
    client.command(jump(B, "old"));
    client.respond(&messages_of(B), page(vec![msg(B, "b1", ALICE, 0)], true));
    assert!(client.asked_for(&around(B, "old")));
}

#[test]
fn a_detached_send_should_go_back_to_the_newest_page() {
    let mut client = detached();
    client.command(Command::Send {
        content: "hi".into(),
        reply_to_id: None,
        nonce: "n2".into(),
        sent_at: "2026-11-04T12:40:00.000Z".into(),
    });
    assert!(client.asked_for(&messages_of(A)));
}

#[test]
fn the_window_answer_should_decode_both_flags() {
    let request = around(A, "old5");
    let decoded = request.decode(window(true));
    assert!(matches!(
        decoded,
        Ok(ApiResponse::Threads(
            yap_core::store::ThreadResponse::Window {
                has_more_before: true,
                has_more_after: true,
                ..
            }
        ))
    ));
}

// The thread's own replies: edit, delete and react from the panel when the
// open list has not loaded them

/// Thread m1 open with m2 (also in the list) and r9, ours and in the
/// thread only.
fn thread_only_reply() -> Client {
    let mut client = Client::online(three(), 0);
    client.open_thread(
        "m1",
        vec![reply(A, "m2", ME, 1, "m1"), reply(A, "r9", ME, 9, "m1")],
    );
    client
}

fn edit_of(id: &str, content: &str) -> ApiRequest {
    ApiRequest::EditMessage {
        channel_id: A.into(),
        message_id: id.into(),
        content: content.into(),
    }
}

fn edit(id: &str, content: &str) -> Command {
    Command::EditMessage {
        message_id: id.into(),
        content: content.into(),
    }
}

fn delete_of(id: &str) -> ApiRequest {
    ApiRequest::DeleteMessage {
        channel_id: A.into(),
        message_id: id.into(),
    }
}

fn thumbs_up(id: &str) -> Command {
    Command::ToggleReaction {
        message_id: id.into(),
        emoji: "+1".into(),
    }
}

fn thread_row<'a>(client: &'a Client, id: &str) -> Option<&'a MessageRow> {
    client.seen.thread.iter().find(|r| r.id == id)
}

#[test]
fn editing_a_thread_only_reply_should_patch_it_on_the_server() {
    let mut client = thread_only_reply();
    client.command(edit("r9", "fixed"));
    assert!(client.asked_for(&edit_of("r9", "fixed")));
}

#[test]
fn editing_a_thread_only_reply_should_show_at_once() {
    let mut client = thread_only_reply();
    client.command(edit("r9", "fixed"));
    assert_eq!(
        thread_row(&client, "r9").map(|r| r.body.as_str()),
        Some("fixed")
    );
}

#[test]
fn a_refused_thread_edit_should_put_the_old_text_back() {
    let mut client = thread_only_reply();
    client.command(edit("r9", "fixed"));
    client.fail(&edit_of("r9", "fixed"), 500);
    assert_eq!(
        thread_row(&client, "r9").map(|r| r.body.as_str()),
        Some("text of r9")
    );
}

#[test]
fn a_refused_thread_edit_should_not_add_the_reply_to_the_list() {
    let mut client = thread_only_reply();
    client.command(edit("r9", "fixed"));
    client.fail(&edit_of("r9", "fixed"), 500);
    assert_eq!(Client::ids(&client.seen.rows), ["m1", "m2", "m3"]);
}

#[test]
fn another_persons_thread_reply_should_not_be_edited() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "r8", ALICE, 8, "m1")]);
    client.command(edit("r8", "mine now"));
    assert!(!client.asked_for(&edit_of("r8", "mine now")));
}

#[test]
fn deleting_a_thread_only_reply_should_delete_it_on_the_server() {
    let mut client = thread_only_reply();
    client.command(Command::DeleteMessage("r9".into()));
    assert!(client.asked_for(&delete_of("r9")));
}

#[test]
fn deleting_a_thread_only_reply_should_take_it_out_of_the_thread() {
    let mut client = thread_only_reply();
    client.command(Command::DeleteMessage("r9".into()));
    assert_eq!(Client::ids(&client.seen.thread), ["m1", "m2"]);
}

#[test]
fn deleting_a_thread_only_reply_should_lower_the_roots_count_once() {
    let mut root = msg(A, "m1", ALICE, 0);
    root["reply_count"] = json!(2);
    let mut client = Client::online(vec![root, reply(A, "m2", ME, 1, "m1")], 0);
    client.open_thread(
        "m1",
        vec![reply(A, "m2", ME, 1, "m1"), reply(A, "r9", ME, 9, "m1")],
    );
    client.command(Command::DeleteMessage("r9".into()));
    client.dispatch("MESSAGE_DELETE", json!({ "id": "r9", "channel_id": A }));
    assert_eq!(client.row("m1").reply_count, 1);
}

#[test]
fn a_refused_thread_delete_should_put_the_reply_back() {
    let mut client = thread_only_reply();
    client.command(Command::DeleteMessage("r9".into()));
    client.fail(&delete_of("r9"), 500);
    assert_eq!(Client::ids(&client.seen.thread), ["m1", "m2", "r9"]);
}

#[test]
fn a_refused_thread_delete_should_leave_the_list_alone() {
    let mut client = thread_only_reply();
    client.command(Command::DeleteMessage("r9".into()));
    client.fail(&delete_of("r9"), 500);
    assert_eq!(Client::ids(&client.seen.rows), ["m1", "m2", "m3"]);
}

#[test]
fn another_persons_thread_reply_should_need_manage_messages_to_delete() {
    let mut client = Client::online(three(), 0);
    client.open_thread("m1", vec![reply(A, "r8", ALICE, 8, "m1")]);
    client.command(Command::DeleteMessage("r8".into()));
    assert!(!client.asked_for(&delete_of("r8")));
}

#[test]
fn a_thread_reaction_should_show_at_once() {
    let mut client = thread_only_reply();
    client.command(thumbs_up("r9"));
    assert!(thread_row(&client, "r9").is_some_and(|r| r.reactions.contains("+1")));
}

#[test]
fn a_thread_reaction_echo_should_change_nothing() {
    let mut client = thread_only_reply();
    client.command(thumbs_up("r9"));
    let shown = thread_row(&client, "r9").map(|r| r.reactions.clone());
    client.dispatch(
        "REACTION_ADD",
        json!({ "message_id": "r9", "channel_id": A, "user_id": ME, "emoji": "+1" }),
    );
    assert_eq!(
        thread_row(&client, "r9").map(|r| r.reactions.clone()),
        shown
    );
}

#[test]
fn a_refused_thread_reaction_should_be_taken_back() {
    let mut client = thread_only_reply();
    client.command(thumbs_up("r9"));
    let react = ApiRequest::React {
        channel_id: A.into(),
        message_id: "r9".into(),
        emoji: "+1".into(),
        add: true,
    };
    client.fail(&react, 500);
    assert!(thread_row(&client, "r9").is_some_and(|r| !r.reactions.contains("+1")));
}

// The member list's Threads tab

fn thread_list(channel: &str) -> ApiRequest {
    threads(ThreadRequest::ChannelThreads {
        channel_id: channel.into(),
    })
}

/// 2026-11-04T13:00:00Z.
const ONE_PM: i64 = 1_793_797_200_000;

fn list_threads() -> Command {
    Command::Thread(ThreadCommand::List { now_ms: ONE_PM })
}

fn summary(id: &str, replies: i64, last_minute: u32) -> Value {
    json!({
        "id": id, "content": format!("starter {id}"),
        "author": { "id": ALICE, "username": "alice", "display_name": "Alice A", "avatar_url": "/a.png" },
        "reply_count": replies,
        "last_reply_at": format!("2026-11-04 12:{last_minute:02}:00"),
        "created_at": "2026-11-04 12:00:00",
    })
}

#[test]
fn the_threads_tab_should_ask_for_the_channels_threads() {
    let request = ThreadRequest::ChannelThreads {
        channel_id: A.into(),
    };
    assert_eq!(
        (request.method(), request.path()),
        (Method::Get, "/api/channels/chan_a/threads".into())
    );
}

#[test]
fn showing_the_threads_tab_should_load_the_open_channels_threads() {
    let mut client = Client::online(three(), 0);
    client.command(list_threads());
    assert!(client.asked_for(&thread_list(A)));
}

#[test]
fn the_threads_tab_should_show_each_starter_with_its_replies() {
    let mut client = Client::online(three(), 0);
    client.command(list_threads());
    client.respond(
        &thread_list(A),
        json!({ "threads": [summary("m1", 3, 30)] }),
    );
    let row = &client.seen.panels.threads[0];
    assert_eq!(
        (
            row.author.as_str(),
            row.initials.as_str(),
            row.avatar.as_str(),
            row.replies,
            row.last_reply.as_str()
        ),
        (
            "Alice A",
            "AA",
            "https://api.fixture.invalid/a.png",
            3,
            "30m ago"
        )
    );
}

#[test]
fn the_threads_tab_should_be_loading_while_it_asks() {
    let mut client = Client::online(three(), 0);
    client.command(list_threads());
    assert!(client.seen.panels.threads_loading);
}

#[test]
fn a_failed_threads_tab_should_say_why() {
    let mut client = Client::online(three(), 0);
    client.command(list_threads());
    client.fail(&thread_list(A), 500);
    assert_eq!(
        client.seen.panels.threads_error,
        "Unable to load threads. Try again in a moment."
    );
}

#[test]
fn an_answer_for_another_channel_should_not_fill_the_threads_tab() {
    let mut client = Client::online(three(), 0);
    client.command(list_threads());
    client.command(Command::OpenChannel {
        server_id: Some(SERVER.into()),
        channel_id: B.into(),
    });
    client.command(list_threads());
    client.respond(
        &thread_list(A),
        json!({ "threads": [summary("m1", 3, 30)] }),
    );
    assert!(client.seen.panels.threads.is_empty());
}

// Where each channel was left (useChatArea.ts scroll restore)

fn restores(fx: &[Effect]) -> Vec<&str> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::ScrollRestore(id)) => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

fn scroll_writes(fx: &[Effect]) -> Vec<(&str, &str)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::CacheWrite {
                write:
                    CacheWrite::Scroll {
                        channel_id,
                        message_id,
                    },
                ..
            } => Some((channel_id.as_str(), message_id.as_str())),
            _ => None,
        })
        .collect()
}

#[test]
fn a_channel_should_reopen_at_the_message_it_was_left_on() {
    let mut client = Client::online(three(), 0);
    client.command(Command::SetAtBottom(false));
    let fx = client.command(Command::ScrollAt("m2".into()));
    assert_eq!(scroll_writes(&fx), vec![(A, "m2")]);
    client.command(Command::SelectChannel(B.into()));
    client.command(Command::SelectChannel(A.into()));
    let fx = client.respond(&messages_of(A), page(three(), true));
    assert_eq!(restores(&fx), vec!["m2"]);
}

#[test]
fn a_channel_left_at_its_end_should_reopen_at_the_end_without_a_divider() {
    let mut client = Client::online(three(), 0);
    // `online` reports the end: the position is the bottom.
    client.command(Command::SelectChannel(B.into()));
    // Alice wrote while A was closed: the read mark is behind.
    client.dispatch("MESSAGE_CREATE", msg(A, "m4", ALICE, 3));
    assert!(client.state.is_unread(A));
    client.command(Command::SelectChannel(A.into()));
    let mut newer = three();
    newer.push(msg(A, "m4", ALICE, 3));
    let fx = client.respond(&messages_of(A), page(newer, true));
    assert_eq!(restores(&fx), vec![""]);
    assert!(client.state.divider().is_none());
}

#[test]
fn a_saved_message_outside_the_newest_page_should_load_its_window_unhighlighted() {
    let mut client = Client::online(three(), 0);
    client.command(Command::SetAtBottom(false));
    client.command(Command::ScrollAt("m1".into()));
    client.command(Command::SelectChannel(B.into()));
    client.command(Command::SelectChannel(A.into()));
    // The newest page no longer holds m1.
    client.respond(&messages_of(A), page(vec![msg(A, "m3", ALICE, 2)], true));
    assert!(client.asked_for(&around(A, "m1")));
    let window = json!({ "messages": three(), "hasMoreBefore": false, "hasMoreAfter": true });
    let fx = client.respond(&around(A, "m1"), window);
    assert_eq!(restores(&fx), vec!["m1"]);
    assert!(client.seen.jumps.is_empty());
}

#[test]
fn scrolling_a_jumps_window_should_keep_the_jumped_position() {
    let mut client = Client::online(three(), 0);
    client.command(jump(A, "m9"));
    let window = json!({ "messages": three(), "hasMoreBefore": false, "hasMoreAfter": true });
    client.respond(&around(A, "m9"), window);
    client.command(Command::SetAtBottom(false));
    let fx = client.command(Command::ScrollAt("m1".into()));
    assert!(scroll_writes(&fx).is_empty(), "{fx:#?}");
}

#[test]
fn the_newest_page_shown_while_a_saved_window_loads_should_not_overwrite_the_position() {
    let mut client = Client::online(three(), 0);
    client.command(Command::SetAtBottom(false));
    client.command(Command::ScrollAt("m1".into()));
    client.command(Command::SelectChannel(B.into()));
    client.command(Command::SelectChannel(A.into()));
    client.respond(&messages_of(A), page(vec![msg(A, "m3", ALICE, 2)], true));
    // The list lands on the newest page and says it is at its end.
    let fx = client.command(Command::SetAtBottom(true));
    assert!(scroll_writes(&fx).is_empty(), "{fx:#?}");
    let window = json!({ "messages": three(), "hasMoreBefore": false, "hasMoreAfter": true });
    let fx = client.respond(&around(A, "m1"), window);
    assert_eq!(restores(&fx), vec!["m1"]);
}
