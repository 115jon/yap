//! Instant channel and server switches: the warm cache, stale-while-
//! revalidate, disk windows, prefetches, the channel each server opens on,
//! and the page merge that must not leave a gap.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::MessageRow;
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, CacheWrite, Command, Dispatch, Effect, GatewayEvent,
    HttpReason, Input, PinsCommand, RequestId, Snapshot, State, ViewDiff, ViewUpdate, WARM_BYTES,
    WARM_ENTRIES, WARM_WINDOW,
};
use yap_core::Token;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const SERVER_TWO: &str = "srv_fixture_two";
const TEXT: &str = "chan_fixture_text";
const SECOND: &str = "chan_fixture_text_two";
const TWO_TEXT: &str = "chan_two_general";
const TWO_SECOND: &str = "chan_two_random";
const ME: &str = "user_fixture_me";
const DM: &str = "dm_fixture_alice";
/// A wall clock for `Input::UserAt` (epoch ms).
const NOW: i64 = 1_793_000_000_000;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn message_json(channel: &str, id: &str, at: &str) -> Value {
    let mut data = fixture("dispatch-message-create.json")["d"]["data"].clone();
    data["channel_id"] = json!(channel);
    data["id"] = json!(id);
    data["created_at"] = json!(at);
    data["nonce"] = Value::Null;
    data["content"] = json!(format!("message {id}"));
    data
}

/// `n` messages of a channel, a minute apart from 10:00, oldest first.
fn history(channel: &str, prefix: &str, from: u32, n: u32) -> Vec<Value> {
    (from..from + n)
        .map(|i| {
            let at = format!("2026-11-04T{:02}:{:02}:00.000Z", 10 + i / 60, i % 60);
            message_json(channel, &format!("{prefix}{i:03}"), &at)
        })
        .collect()
}

fn page(messages: Vec<Value>, more: bool) -> Value {
    json!({ "messages": messages, "hasMoreBefore": more })
}

fn messages_of(channel: &str) -> ApiRequest {
    ApiRequest::Messages {
        channel_id: channel.into(),
        before: None,
    }
}

fn channels_of(server: &str) -> ApiRequest {
    ApiRequest::Channels {
        server_id: server.into(),
    }
}

fn tree_one() -> Value {
    let mut body = fixture("channels-response.json");
    let channels = body["channels"].as_array_mut().expect("channels");
    channels.push(json!({
        "id": SECOND, "server_id": SERVER, "name": "random",
        "channel_type": "text", "category_id": "cat_fixture_text", "position": 1,
    }));
    body
}

fn tree_two() -> Value {
    json!({
        "categories": [],
        "channels": [
            { "id": TWO_TEXT, "server_id": SERVER_TWO, "name": "general",
              "channel_type": "text", "position": 0 },
            { "id": TWO_SECOND, "server_id": SERVER_TWO, "name": "random",
              "channel_type": "text", "position": 1 },
        ],
    })
}

/// A tree of `n` text channels `c000`.. in the fixture server.
fn tree_many(n: usize) -> Value {
    let channels: Vec<Value> = (0..n)
        .map(|i| {
            json!({ "id": format!("c{i:03}"), "server_id": SERVER, "name": format!("c{i}"),
                    "channel_type": "text", "position": i })
        })
        .collect();
    json!({ "categories": [], "channels": channels })
}

fn requests(fx: &[Effect]) -> Vec<(RequestId, ApiRequest, HttpReason)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Http {
                id,
                request,
                reason,
                ..
            } => Some((*id, request.clone(), *reason)),
            _ => None,
        })
        .collect()
}

fn request_for(fx: &[Effect], wanted: &ApiRequest) -> RequestId {
    requests(fx)
        .into_iter()
        .find(|(_, r, _)| r == wanted)
        .map(|(id, _, _)| id)
        .unwrap_or_else(|| panic!("no {wanted:?} in {fx:#?}"))
}

fn count(fx: &[Effect], wanted: &ApiRequest) -> usize {
    requests(fx).iter().filter(|(_, r, _)| r == wanted).count()
}

fn views(fx: &[Effect]) -> Vec<&ViewUpdate> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(update) => Some(update),
            _ => None,
        })
        .collect()
}

fn message_diffs(fx: &[Effect]) -> Vec<&ViewDiff<MessageRow>> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::Messages(diff) => Some(&**diff),
            _ => None,
        })
        .collect()
}

/// The rows of the last message reset in `fx`.
fn reset_rows(fx: &[Effect]) -> Option<Vec<String>> {
    message_diffs(fx).into_iter().rev().find_map(|d| match d {
        ViewDiff::Reset(rows) => Some(rows.iter().map(|r| r.id.clone()).collect()),
        _ => None,
    })
}

fn loading(fx: &[Effect]) -> Vec<bool> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::PageLoading(on) => Some(*on),
            _ => None,
        })
        .collect()
}

fn ids(state: &State) -> Vec<String> {
    state
        .messages()
        .map(|log| log.iter().map(|m| m.id.clone()).collect())
        .unwrap_or_default()
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

    /// A command at a wall clock time.
    fn at(&mut self, now_ms: i64, command: Command) -> Vec<Effect> {
        self.input(Input::UserAt { command, now_ms })
    }

    fn respond(&mut self, id: RequestId, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    /// Answers the page `fx` asked for `channel` with `body`.
    fn page(&mut self, fx: &[Effect], channel: &str, body: Value) -> Vec<Effect> {
        let request = messages_of(channel);
        self.respond(request_for(fx, &request), &request, body)
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.gateway(GatewayEvent::Dispatch(dispatch))
    }

    /// Signed in on the fixture server's `TEXT` (two messages), with the
    /// second server's tree loaded too, and the gateway up.
    fn boot_with(tree: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
        };
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        let me = request_for(&fx, &ApiRequest::Me);
        let servers = request_for(&fx, &ApiRequest::Servers);
        let after_me = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        let dms = json!([{ "id": DM, "channel_type": "dm", "name": "x", "created_at": null,
            "recipient": { "id": "user_fixture_alice", "username": "alice",
                           "display_name": "Alice", "avatar_url": null, "status": "online" } }]);
        client.respond(
            request_for(&after_me, &ApiRequest::Dms),
            &ApiRequest::Dms,
            dms,
        );
        let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
        let two = channels_of(SERVER_TWO);
        client.respond(request_for(&fx, &two), &two, tree_two());
        let members = ApiRequest::Members {
            server_id: SERVER.into(),
        };
        client.respond(request_for(&fx, &members), &members, json!([]));
        let one = channels_of(SERVER);
        let fx = client.respond(request_for(&fx, &one), &one, tree);
        let first = client
            .state
            .active_channel()
            .expect("a channel opened")
            .to_owned();
        client.page(&fx, &first, page(history(&first, "a", 0, 2), false));
        client.gateway(GatewayEvent::Ready);
        client
    }

    fn boot() -> Self {
        Self::boot_with(tree_one())
    }

    /// Opens `channel` and answers its page with `messages`.
    fn visit(&mut self, channel: &str, messages: Vec<Value>) -> Vec<Effect> {
        let fx = self.command(Command::SelectChannel(channel.into()));
        let mut all = fx.clone();
        if count(&fx, &messages_of(channel)) > 0 {
            all.extend(self.page(&fx, channel, page(messages, false)));
        }
        all
    }
}

// Warm channels

#[test]
fn a_channel_visited_before_should_show_at_once_and_revalidate_quietly() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    // Its rows in the same batch: no skeleton, no empty list first.
    assert_eq!(reset_rows(&fx), Some(vec!["a000".into(), "a001".into()]));
    assert!(!loading(&fx).contains(&true), "{:?}", loading(&fx));
    assert_eq!(count(&fx, &messages_of(TEXT)), 1);
    let (_, _, reason) = requests(&fx)
        .into_iter()
        .find(|(_, r, _)| *r == messages_of(TEXT))
        .expect("revalidate");
    assert_eq!(reason, HttpReason::Revalidate);
    // The same page again changes nothing on screen.
    let fx = client.page(&fx, TEXT, page(history(TEXT, "a", 0, 2), false));
    assert!(message_diffs(&fx).is_empty(), "{fx:#?}");
    // A newer message in the refetch is one insert.
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    let fx = client.page(&fx, TEXT, page(history(TEXT, "a", 0, 3), false));
    assert!(
        matches!(message_diffs(&fx)[..], [ViewDiff::Insert { at: 2, .. }]),
        "{fx:#?}"
    );
}

#[test]
fn a_fresh_warm_channel_should_open_without_any_request() {
    let mut client = Client::boot();
    let fx = client.at(NOW, Command::SelectChannel(SECOND.into()));
    client.page(&fx, SECOND, page(history(SECOND, "b", 0, 3), false));
    client.at(NOW + 1_000, Command::SelectChannel(TEXT.into()));
    // Confirmed 2 s ago in this gateway session: shown, not asked again.
    let fx = client.at(NOW + 2_000, Command::SelectChannel(SECOND.into()));
    assert_eq!(count(&fx, &messages_of(SECOND)), 0, "{fx:#?}");
    assert_eq!(loading(&fx), vec![false]);
    assert_eq!(reset_rows(&fx).map(|r| r.len()), Some(3));
    // After the fresh window it is asked again.
    client.at(NOW + 3_000, Command::SelectChannel(TEXT.into()));
    let fx = client.at(NOW + 40_000, Command::SelectChannel(SECOND.into()));
    assert_eq!(count(&fx, &messages_of(SECOND)), 1);
}

#[test]
fn a_fresh_gateway_session_should_make_every_warm_channel_revalidate() {
    let mut client = Client::boot();
    assert_eq!(client.state.gateway_generation(), 1);
    let fx = client.at(NOW, Command::SelectChannel(SECOND.into()));
    client.page(&fx, SECOND, page(history(SECOND, "b", 0, 3), false));
    client.at(NOW + 1_000, Command::SelectChannel(TEXT.into()));
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Ready);
    assert_eq!(client.state.gateway_generation(), 2);
    // The shown server's members and the DM list come again too.
    assert_eq!(
        count(
            &fx,
            &ApiRequest::Members {
                server_id: SERVER.into()
            }
        ),
        1
    );
    assert_eq!(count(&fx, &ApiRequest::Dms), 1);
    // The open channel refetches without its skeleton.
    assert!(!loading(&fx).contains(&true));
    let fx = client.at(NOW + 2_000, Command::SelectChannel(SECOND.into()));
    assert_eq!(count(&fx, &messages_of(SECOND)), 1);
    // A resume keeps the session: nothing is lost, nothing bumps.
    client.gateway(GatewayEvent::Closed);
    client.gateway(GatewayEvent::Resumed);
    assert_eq!(client.state.gateway_generation(), 2);
}

#[test]
fn the_open_channel_chosen_again_should_load_afresh() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    assert_eq!(reset_rows(&fx), Some(Vec::new()));
    assert!(loading(&fx).contains(&true));
}

#[test]
fn events_should_keep_a_warm_channel_current() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    client.command(Command::SelectChannel(TEXT.into()));
    let warm = |c: &Client| -> Vec<String> {
        let entry = c.state.warm().get(SECOND).expect("warm");
        entry.log.iter().map(|m| m.id.clone()).collect()
    };
    // A new message goes in at its newest end.
    client.dispatch(
        "MESSAGE_CREATE",
        message_json(SECOND, "b100", "2026-11-04T12:00:00.000Z"),
    );
    assert_eq!(warm(&client), ["b000", "b001", "b002", "b100"]);
    // An edit, a reaction and a delete.
    client.dispatch(
        "MESSAGE_UPDATE",
        json!({ "id": "b001", "channel_id": SECOND, "content": "edited", "content_revision": 1 }),
    );
    client.dispatch(
        "REACTION_ADD",
        json!({ "message_id": "b002", "channel_id": SECOND, "user_id": "user_fixture_alice", "emoji": "👍" }),
    );
    let fx = client.dispatch(
        "MESSAGE_DELETE",
        json!({ "id": "b000", "channel_id": SECOND }),
    );
    assert_eq!(warm(&client), ["b001", "b002", "b100"]);
    // Nothing of it reaches the open list, and the disk copy follows.
    assert!(message_diffs(&fx).is_empty());
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::CacheWrite { write: CacheWrite::Messages { channel_id, .. }, .. } if channel_id == SECOND
    )));
    let entry = client.state.warm().get(SECOND).expect("warm");
    assert_eq!(entry.log.get("b001").expect("b001").content, "edited");
    assert_eq!(entry.log.get("b002").expect("b002").reactions.len(), 1);
    // Returning shows all of it at once.
    let fx = client.command(Command::SelectChannel(SECOND.into()));
    assert_eq!(
        reset_rows(&fx),
        Some(vec!["b001".into(), "b002".into(), "b100".into()])
    );
}

#[test]
fn an_older_message_for_a_warm_channel_should_not_go_in_at_its_end() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 10, 3));
    client.command(Command::SelectChannel(TEXT.into()));
    // A full update of a message older than the warm window.
    let mut old = message_json(SECOND, "b001", "2026-11-04T10:01:00.000Z");
    old["author"] = json!({ "id": "user_fixture_alice", "username": "alice" });
    client.dispatch("MESSAGE_UPDATE", old);
    let entry = client.state.warm().get(SECOND).expect("warm");
    assert!(entry.log.get("b001").is_none());
}

#[test]
fn a_deleted_channel_should_leave_the_warm_cache() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    client.command(Command::SelectChannel(TEXT.into()));
    assert!(client.state.warm().contains(SECOND));
    client.dispatch(
        "CHANNEL_DELETE",
        json!({ "id": SECOND, "server_id": SERVER }),
    );
    assert!(!client.state.warm().contains(SECOND));
}

#[test]
fn a_channel_missing_from_a_refetched_tree_should_leave_the_warm_cache() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    client.command(Command::SelectChannel(TEXT.into()));
    // Access lost: the tree comes back without it.
    let fx = client.command(Command::SelectServer(SERVER.into()));
    let one = channels_of(SERVER);
    client.respond(
        request_for(&fx, &one),
        &one,
        fixture("channels-response.json"),
    );
    assert!(!client.state.warm().contains(SECOND));
}

#[test]
fn sign_out_should_empty_the_warm_cache() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    client.command(Command::SelectChannel(TEXT.into()));
    client.command(Command::SignOut);
    assert!(client.state.warm().is_empty());
    assert_eq!(client.state.last_channel(SERVER), None);
}

// Caps

#[test]
fn the_warm_cache_should_keep_at_most_its_entries() {
    let mut client = Client::boot_with(tree_many(WARM_ENTRIES + 6));
    for i in 0..WARM_ENTRIES + 6 {
        let id = format!("c{i:03}");
        client.visit(&id, history(&id, "m", 0, 2));
    }
    let warm = client.state.warm();
    assert_eq!(warm.len(), WARM_ENTRIES);
    // The least recently used went first; the open one is never in it.
    let last = format!("c{:03}", WARM_ENTRIES + 5);
    assert!(!warm.contains(&last));
    assert!(warm.contains(&format!("c{:03}", WARM_ENTRIES + 4)));
    assert!(!warm.contains("c000"));
}

#[test]
fn the_warm_cache_should_stay_within_its_bytes() {
    let mut client = Client::boot_with(tree_many(20));
    let big = "x".repeat(4_000);
    for i in 0..20 {
        let id = format!("c{i:03}");
        let mut rows = history(&id, "m", 0, 100);
        for row in &mut rows {
            row["content"] = json!(big);
        }
        client.visit(&id, rows);
    }
    let warm = client.state.warm();
    assert!(warm.bytes() <= WARM_BYTES, "{} bytes", warm.bytes());
    assert!(warm.len() < 19, "{} entries", warm.len());
    assert!(warm.len() >= 10, "{} entries", warm.len());
}

#[test]
fn a_long_log_should_be_trimmed_to_its_newest_window_when_left() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectChannel(SECOND.into()));
    client.page(&fx, SECOND, page(history(SECOND, "b", 0, 150), true));
    client.command(Command::SelectChannel(TEXT.into()));
    let entry = client.state.warm().get(SECOND).expect("warm");
    assert_eq!(entry.log.len(), WARM_WINDOW);
    assert!(entry.log.has_more_before);
    assert_eq!(entry.log.first().map(|m| m.id.as_str()), Some("b050"));
}

// The page merge

/// The last session, with `cached` as the open channel's window.
fn snapshot(cached: Vec<Value>) -> Snapshot {
    Snapshot {
        me: serde_json::from_value(fixture("user-me.json")).expect("me"),
        servers: serde_json::from_value(fixture("servers-list.json")).expect("servers"),
        channels: vec![(
            SERVER.into(),
            serde_json::from_value(tree_one()).expect("channels"),
        )],
        active_server: Some(SERVER.into()),
        active_channel: Some(TEXT.into()),
        messages: cached
            .into_iter()
            .map(|m| serde_json::from_value(m).expect("message"))
            .collect(),
        written_at_ms: 0,
        voice: None,
        scroll_positions: Vec::new(),
        last_channels: vec![(SERVER_TWO.into(), TWO_SECOND.into())],
    }
}

fn launch(snapshot: Snapshot) -> (Client, Vec<Effect>) {
    let mut client = Client {
        state: State::new(BASE),
    };
    client.command(Command::Start);
    client.input(Input::TokenLoaded(Some(Token::new("tok_fixture"))));
    let fx = client.input(Input::SnapshotLoaded(Some(Box::new(snapshot))));
    (client, fx)
}

#[test]
fn a_page_newer_than_the_cached_rows_should_drop_them_instead_of_leaving_a_gap() {
    // Cached: 0..5. Since then 60 arrived; the page is the newest 50.
    let (mut client, fx) = launch(snapshot(history(TEXT, "m", 0, 5)));
    assert_eq!(ids(&client.state).len(), 5);
    client.page(&fx, TEXT, page(history(TEXT, "m", 15, 50), true));
    let shown = ids(&client.state);
    assert_eq!(shown.len(), 50);
    assert_eq!(shown.first().map(String::as_str), Some("m015"));
    assert!(client.state.messages().expect("log").has_more_before);
}

#[test]
fn a_page_that_overlaps_the_cached_rows_should_keep_the_older_ones() {
    let (mut client, fx) = launch(snapshot(history(TEXT, "m", 0, 10)));
    client.page(&fx, TEXT, page(history(TEXT, "m", 8, 50), true));
    let shown = ids(&client.state);
    assert_eq!(shown.len(), 58);
    assert_eq!(shown.first().map(String::as_str), Some("m000"));
}

#[test]
fn a_snapshot_should_bring_back_the_channel_each_server_opens_on() {
    let (client, _) = launch(snapshot(history(TEXT, "m", 0, 2)));
    assert_eq!(client.state.last_channel(SERVER_TWO), Some(TWO_SECOND));
}

// Servers

#[test]
fn a_loaded_server_should_show_its_tree_and_channel_before_its_channels_answer() {
    let mut client = Client::boot();
    let fx = client.at(NOW, Command::SelectServer(SERVER_TWO.into()));
    let tree_at = views(&fx)
        .iter()
        .position(|v| matches!(v, ViewUpdate::Channels(ViewDiff::Reset(rows)) if !rows.is_empty()))
        .expect("the tree in the same batch");
    let channel_at = views(&fx)
        .iter()
        .position(|v| matches!(v, ViewUpdate::ActiveChannel { id, .. } if id == TWO_TEXT))
        .expect("its channel in the same batch");
    assert!(tree_at < channel_at);
    assert_eq!(client.state.active_channel(), Some(TWO_TEXT));
    // The tree is revalidated in the background, the page fetched.
    let (_, _, reason) = requests(&fx)
        .into_iter()
        .find(|(_, r, _)| *r == channels_of(SERVER_TWO))
        .expect("tree refetch");
    assert_eq!(reason, HttpReason::Revalidate);
    assert_eq!(count(&fx, &messages_of(TWO_TEXT)), 1);
    // The tree's answer keeps the open channel and changes no row.
    let two = channels_of(SERVER_TWO);
    let answer = client.respond(request_for(&fx, &two), &two, tree_two());
    assert_eq!(client.state.active_channel(), Some(TWO_TEXT));
    assert!(message_diffs(&answer).is_empty(), "{answer:#?}");
}

#[test]
fn a_server_should_open_on_the_channel_last_opened_there() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectServer(SERVER_TWO.into()));
    client.page(&fx, TWO_TEXT, page(history(TWO_TEXT, "t", 0, 2), false));
    let fx = client.visit(TWO_SECOND, history(TWO_SECOND, "r", 0, 2));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::CacheWrite {
            write: CacheWrite::LastChannel { server_id, channel_id },
            ..
        } if server_id == SERVER_TWO && channel_id == TWO_SECOND
    )));
    client.command(Command::SelectServer(SERVER.into()));
    assert_eq!(client.state.active_channel(), Some(TEXT));
    let fx = client.command(Command::SelectServer(SERVER_TWO.into()));
    assert_eq!(client.state.active_channel(), Some(TWO_SECOND));
    // Warm from before: at once.
    assert_eq!(reset_rows(&fx), Some(vec!["r000".into(), "r001".into()]));
    assert!(!loading(&fx).contains(&true));
}

#[test]
fn a_tree_that_answers_should_open_the_remembered_channel() {
    // No tree loaded for the second server: its answer picks the channel.
    let mut client = Client::boot();
    client.state = {
        let (launched, _) = launch(snapshot(history(TEXT, "m", 0, 2)));
        launched.state
    };
    let fx = client.command(Command::SelectServer(SERVER_TWO.into()));
    let two = channels_of(SERVER_TWO);
    client.respond(request_for(&fx, &two), &two, tree_two());
    assert_eq!(client.state.active_channel(), Some(TWO_SECOND));
}

// Prefetch

#[test]
fn a_prefetch_should_fill_the_warm_cache_without_touching_the_screen() {
    let mut client = Client::boot();
    let fx = client.command(Command::PrefetchChannel(SECOND.into()));
    let found = requests(&fx);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].2, HttpReason::Prefetch);
    let answer = client.page(&fx, SECOND, page(history(SECOND, "b", 0, 3), false));
    assert!(message_diffs(&answer).is_empty());
    assert_eq!(client.state.active_channel(), Some(TEXT));
    assert!(client.state.warm().contains(SECOND));
    let fx = client.command(Command::SelectChannel(SECOND.into()));
    assert_eq!(reset_rows(&fx).map(|r| r.len()), Some(3));
}

#[test]
fn prefetches_should_be_bounded_and_skip_the_open_and_fresh_channels() {
    let mut client = Client::boot_with(tree_many(6));
    let open = client.state.active_channel().expect("open").to_owned();
    assert!(requests(&client.command(Command::PrefetchChannel(open))).is_empty());
    let mut asked = 0;
    for i in 1..5 {
        asked += requests(&client.command(Command::PrefetchChannel(format!("c{i:03}")))).len();
    }
    assert_eq!(asked, 2);
    // The same channel twice is one request.
    let mut client = Client::boot();
    let first = requests(&client.at(NOW, Command::PrefetchChannel(SECOND.into())));
    assert_eq!(first.len(), 1);
    assert!(requests(&client.at(NOW, Command::PrefetchChannel(SECOND.into()))).is_empty());
    let request = messages_of(SECOND);
    client.respond(
        first[0].0,
        &request,
        page(history(SECOND, "b", 0, 3), false),
    );
    // Warm and fresh now: no second fetch.
    assert!(requests(&client.at(NOW + 1_000, Command::PrefetchChannel(SECOND.into()))).is_empty());
}

// The disk window

#[test]
fn a_channel_not_warm_should_show_its_disk_window_while_the_page_loads() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectChannel(SECOND.into()));
    assert!(fx.contains(&Effect::LoadWindow {
        user_id: ME.into(),
        channel_id: SECOND.into(),
    }));
    assert!(loading(&fx).contains(&true));
    let rows: Vec<_> = history(SECOND, "b", 0, 3)
        .into_iter()
        .map(|m| serde_json::from_value(m).expect("message"))
        .collect();
    let shown = client.input(Input::WindowLoaded {
        channel_id: SECOND.into(),
        messages: rows,
    });
    assert_eq!(loading(&shown), vec![false]);
    assert_eq!(reset_rows(&shown).map(|r| r.len()), Some(3));
    // The page revalidates it: only the new row goes in.
    let answer = client.page(&fx, SECOND, page(history(SECOND, "b", 0, 4), false));
    assert!(
        matches!(message_diffs(&answer)[..], [ViewDiff::Insert { at: 3, .. }]),
        "{answer:#?}"
    );
}

#[test]
fn a_disk_window_after_the_page_should_be_ignored() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectChannel(SECOND.into()));
    client.page(&fx, SECOND, page(history(SECOND, "b", 0, 4), false));
    let late = client.input(Input::WindowLoaded {
        channel_id: SECOND.into(),
        messages: vec![serde_json::from_value(message_json(
            SECOND,
            "b000",
            "2026-11-04T10:00:00.000Z",
        ))
        .expect("message")],
    });
    assert!(message_diffs(&late).is_empty());
    assert_eq!(ids(&client.state).len(), 4);
}

// Pins

#[test]
fn pins_seen_before_should_show_at_once_and_be_asked_for_again() {
    use yap_core::store::{ThreadRequest, ThreadResponse};
    let pins_of = |c: &str| {
        ApiRequest::Threads(ThreadRequest::Pins {
            channel_id: c.into(),
        })
    };
    let mut client = Client::boot();
    let fx = client.command(Command::Pins(PinsCommand::Open));
    let pinned = json!([message_json(TEXT, "a000", "2026-11-04T10:00:00.000Z")]);
    let req = pins_of(TEXT);
    let result = req.decode(pinned);
    assert!(matches!(
        result,
        Ok(ApiResponse::Threads(ThreadResponse::Pins(_)))
    ));
    client.input(Input::Http {
        id: request_for(&fx, &req),
        result,
    });
    client.visit(SECOND, history(SECOND, "b", 0, 1));
    client.command(Command::Pins(PinsCommand::Open));
    client.command(Command::SelectChannel(TEXT.into()));
    let fx = client.command(Command::Pins(PinsCommand::Open));
    assert_eq!(count(&fx, &pins_of(TEXT)), 1);
    let panels = views(&fx)
        .into_iter()
        .rev()
        .find_map(|v| match v {
            ViewUpdate::Panels(view) => Some(view.clone()),
            _ => None,
        })
        .expect("panels");
    assert!(!panels.pins_loading);
    assert_eq!(panels.pins_count, 1);
}

// Home and DMs

#[test]
fn the_home_key_should_open_the_last_dm() {
    let mut client = Client::boot();
    // No DM opened yet: Home as before (Friends).
    client.command(Command::SelectHomeLast);
    assert!(client.state.home());
    assert_eq!(client.state.active_channel(), None);
    let fx = client.visit(DM, history(DM, "d", 0, 2));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::CacheWrite {
            write: CacheWrite::LastChannel { server_id, channel_id },
            ..
        } if server_id == "@me" && channel_id == DM
    )));
    client.command(Command::SelectServer(SERVER.into()));
    assert!(!client.state.home());
    let fx = client.command(Command::SelectHomeLast);
    assert!(client.state.home());
    assert_eq!(client.state.active_channel(), Some(DM));
    assert_eq!(reset_rows(&fx).map(|r| r.len()), Some(2));
    // Home itself (the Friends row) still closes the DM.
    client.command(Command::SelectHome);
    assert_eq!(client.state.active_channel(), None);
}

#[test]
fn a_warm_dm_should_always_be_asked_again() {
    let mut client = Client::boot();
    let fx = client.at(NOW, Command::SelectChannel(DM.into()));
    client.page(&fx, DM, page(history(DM, "d", 0, 2), false));
    client.at(NOW + 1_000, Command::SelectChannel(TEXT.into()));
    // A DM's edits and deletes reach only its subscribers.
    let fx = client.at(NOW + 2_000, Command::SelectChannel(DM.into()));
    assert_eq!(reset_rows(&fx).map(|r| r.len()), Some(2));
    assert_eq!(count(&fx, &messages_of(DM)), 1);
}

// Keyboard navigation

fn navigate(client: &mut Client, forward: bool, unread: bool) -> Option<String> {
    client.command(Command::NavigateChannel { forward, unread });
    client.state.active_channel().map(str::to_owned)
}

#[test]
fn alt_up_and_down_should_walk_the_text_channels_in_sidebar_order() {
    let mut client = Client::boot();
    assert_eq!(client.state.active_channel(), Some(TEXT));
    // The voice channel between them is passed over; the walk wraps.
    assert_eq!(navigate(&mut client, true, false).as_deref(), Some(SECOND));
    assert_eq!(navigate(&mut client, true, false).as_deref(), Some(TEXT));
    assert_eq!(navigate(&mut client, false, false).as_deref(), Some(SECOND));
}

#[test]
fn the_unread_keys_should_stop_only_on_unread_channels() {
    let mut client = Client::boot_with(tree_many(4));
    assert_eq!(client.state.active_channel(), Some("c000"));
    // Nothing unread: nothing moves.
    assert_eq!(navigate(&mut client, true, true).as_deref(), Some("c000"));
    client.dispatch(
        "MESSAGE_CREATE",
        message_json("c002", "x1", "2026-11-04T12:00:00.000Z"),
    );
    assert_eq!(navigate(&mut client, true, true).as_deref(), Some("c002"));
    client.dispatch(
        "MESSAGE_CREATE",
        message_json("c001", "x2", "2026-11-04T12:01:00.000Z"),
    );
    assert_eq!(navigate(&mut client, false, true).as_deref(), Some("c001"));
}

#[test]
fn ctrl_alt_up_and_down_should_walk_home_and_the_servers() {
    let mut client = Client::boot();
    client.command(Command::NavigateServer { forward: true });
    assert_eq!(client.state.active_server(), Some(SERVER_TWO));
    assert_eq!(client.state.active_channel(), Some(TWO_TEXT));
    client.command(Command::NavigateServer { forward: true });
    assert!(client.state.home());
    client.command(Command::NavigateServer { forward: true });
    assert!(!client.state.home());
    assert_eq!(client.state.active_server(), Some(SERVER));
    client.command(Command::NavigateServer { forward: false });
    assert!(client.state.home());
}

#[test]
fn alt_down_at_home_should_walk_the_dms() {
    let mut client = Client::boot();
    client.command(Command::SelectHome);
    assert_eq!(navigate(&mut client, true, false).as_deref(), Some(DM));
}

#[test]
fn the_server_already_shown_chosen_again_should_keep_its_open_channel() {
    let mut client = Client::boot();
    let fx = client.command(Command::SelectServer(SERVER.into()));
    assert_eq!(reset_rows(&fx), None, "{fx:#?}");
    assert_eq!(count(&fx, &messages_of(TEXT)), 0);
    assert_eq!(count(&fx, &channels_of(SERVER)), 1);
    assert!(!loading(&fx).contains(&true));
    assert_eq!(client.state.active_channel(), Some(TEXT));
}

#[test]
fn a_lost_open_channel_should_not_come_back_as_warm() {
    let mut client = Client::boot();
    client.visit(SECOND, history(SECOND, "b", 0, 3));
    // SECOND is open; the refetched tree no longer lists it.
    let fx = client.command(Command::SelectServer(SERVER.into()));
    let one = channels_of(SERVER);
    client.respond(
        request_for(&fx, &one),
        &one,
        fixture("channels-response.json"),
    );
    assert_eq!(client.state.active_channel(), Some(TEXT));
    assert!(!client.state.warm().contains(SECOND));
}

#[test]
fn a_prefetch_asked_before_a_fresh_session_should_not_count_as_fresh() {
    let mut client = Client::boot();
    let fx = client.at(NOW, Command::PrefetchChannel(SECOND.into()));
    client.gateway(GatewayEvent::Closed);
    client.gateway(GatewayEvent::Ready);
    client.page(&fx, SECOND, page(history(SECOND, "b", 0, 3), false));
    let fx = client.at(NOW + 1_000, Command::SelectChannel(SECOND.into()));
    assert_eq!(reset_rows(&fx).map(|r| r.len()), Some(3));
    assert_eq!(count(&fx, &messages_of(SECOND)), 1);
}
