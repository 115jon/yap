//! Reducer table tests driven by the shared fixtures in
//! `packages/protocol/fixtures`. No network, keyring, runtime or Qt: inputs
//! go into `reduce`, and the test asserts on the state and the effects.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::{
    ChannelTreeRow, MeView, MessageRole, MessageRow, TreeKind, TreeRole, VoicePanelView, VoiceView,
};
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, CacheWrite, CallId, Command, Dispatch, Effect, GatewayEvent,
    Input, Phase, RequestId, SignInEvent, Snapshot, State, ViewDiff, ViewUpdate, VoiceCommand,
    VoiceUpdate,
};
use yap_core::{ApiError, Token, VoiceMember};
use yap_protocol::GatewayOp;

const BASE: &str = "https://api.fixture.invalid";
const TEXT: &str = "chan_fixture_text";
const VOICE: &str = "chan_fixture_voice";
const SECOND_TEXT: &str = "chan_fixture_text_two";
const SERVER: &str = "srv_fixture";
const SESSION_ENDED: &str = "Your session ended. Sign in again.";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// A dispatch fixture decoded the way the gateway hands it to the store.
fn dispatch(name: &str) -> Dispatch {
    let frame = fixture(name);
    let event = frame["d"]["event"].as_str().expect("event").to_owned();
    Dispatch::decode(&event, frame["d"]["data"].clone()).expect("decodes")
}

/// The message in `dispatch-message-create.json`, moved to `channel`.
fn message_in(channel: &str, id: &str) -> Value {
    let mut data = fixture("dispatch-message-create.json")["d"]["data"].clone();
    data["channel_id"] = json!(channel);
    data["id"] = json!(id);
    data
}

/// The channel fixture plus a second text channel.
fn channels_with_two_text() -> Value {
    let mut body = fixture("channels-response.json");
    let channels = body["channels"].as_array_mut().expect("channels");
    channels.push(json!({
        "id": SECOND_TEXT, "server_id": SERVER, "name": "random",
        "channel_type": "text", "category_id": "cat_fixture_text", "position": 1,
    }));
    body
}

fn requests(fx: &[Effect]) -> Vec<(RequestId, ApiRequest)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Http { id, request, .. } => Some((*id, request.clone())),
            _ => None,
        })
        .collect()
}

fn request_for(fx: &[Effect], wanted: &ApiRequest) -> RequestId {
    requests(fx)
        .into_iter()
        .find(|(_, r)| r == wanted)
        .map(|(id, _)| id)
        .unwrap_or_else(|| panic!("no {wanted:?} in {fx:#?}"))
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
            ViewUpdate::Messages(diff) => Some(diff),
            _ => None,
        })
        .collect()
}

fn tree_diffs(fx: &[Effect]) -> Vec<&ViewDiff<ChannelTreeRow>> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::Channels(diff) => Some(diff),
            _ => None,
        })
        .collect()
}

/// Replays the message diffs of each effect batch onto a row vector, the
/// way the model does.
fn replay_messages(rows: &mut Vec<MessageRow>, fx: &[Effect]) {
    for diff in message_diffs(fx) {
        assert!(diff.clone().apply(rows), "{diff:#?} does not fit {rows:#?}");
    }
}

/// A plain message in the open channel.
fn message(id: &str, author: &str, created_at: &str) -> Dispatch {
    let mut data = message_in(TEXT, id);
    data["author_id"] = json!(author);
    data["author"] = json!({ "id": author, "username": author });
    data["created_at"] = json!(created_at);
    Dispatch::MessageCreate(Box::new(serde_json::from_value(data).expect("message")))
}

fn member(id: &str, speaking: bool) -> VoiceMember {
    VoiceMember {
        id: id.into(),
        user_id: String::new(),
        name: id.to_uppercase(),
        muted: false,
        deafened: false,
        speaking,
        me: id == "p_me",
    }
}

/// Gateway frames sent, as `(op, d)`.
fn frames(fx: &[Effect]) -> Vec<(i64, Value)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::GatewaySend(frame) => Some((frame.op, frame.d.clone())),
            _ => None,
        })
        .collect()
}

fn op(code: GatewayOp) -> i64 {
    i64::from(code.code())
}

fn messages() -> ApiRequest {
    ApiRequest::Messages {
        channel_id: TEXT.into(),
    }
}

fn channels() -> ApiRequest {
    ApiRequest::Channels {
        server_id: SERVER.into(),
    }
}

struct Client {
    state: State,
}

impl Client {
    fn new() -> Self {
        Self {
            state: State::new(BASE),
        }
    }

    fn input(&mut self, input: Input) -> Vec<Effect> {
        reduce(&mut self.state, input)
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    /// Answers request `id` with `body`, decoded as the runner would.
    fn respond(&mut self, id: RequestId, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn fail(&mut self, id: RequestId, error: ApiError) -> Vec<Effect> {
        self.input(Input::Http {
            id,
            result: Err(error),
        })
    }

    fn gateway(&mut self, event: GatewayEvent) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        self.input(Input::Gateway { session, event })
    }

    /// Token → me + servers → channels → first page, with the given channel tree.
    fn signed_in_with(channels_body: Value) -> Self {
        let mut client = Self::new();
        let fx = client.command(Command::UseToken("tok_fixture".into()));
        let me = request_for(&fx, &ApiRequest::Me);
        let servers = request_for(&fx, &ApiRequest::Servers);
        client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
        let fx = client.respond(request_for(&fx, &channels()), &channels(), channels_body);
        client.respond(
            request_for(&fx, &messages()),
            &messages(),
            fixture("messages-page.json"),
        );
        client
    }

    fn signed_in() -> Self {
        Self::signed_in_with(fixture("channels-response.json"))
    }

    /// Signed in with the gateway up.
    fn online() -> Self {
        let mut client = Self::signed_in();
        client.gateway(GatewayEvent::Ready);
        client
    }

    fn message_count(&self) -> usize {
        self.state.messages().map_or(0, |log| log.len())
    }
}

// Boot

#[test]
fn start_without_a_stored_token_should_show_sign_in() {
    let mut client = Client::new();
    let fx = client.input(Input::TokenLoaded(None));
    assert_eq!(views(&fx), vec![&ViewUpdate::Phase(Phase::SignedOut)]);
}

#[test]
fn boot_with_token_should_request_me_and_servers_in_one_batch() {
    let mut client = Client::new();
    let fx = client.input(Input::TokenLoaded(Some(Token::new("tok_fixture"))));
    let kinds: Vec<ApiRequest> = requests(&fx).into_iter().map(|(_, r)| r).collect();
    assert_eq!(kinds, vec![ApiRequest::Me, ApiRequest::Servers]);
    assert_eq!(client.state.phase(), Phase::Loading);
}

#[test]
fn me_should_store_the_token_show_me_and_connect_the_gateway() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("  tok_fixture ".into()));
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    assert!(fx.contains(&Effect::StoreToken(Token::new("tok_fixture"))));
    assert!(fx.contains(&Effect::View(ViewUpdate::Me(MeView {
        id: "user_fixture_me".into(),
        name: "Fixture Me".into(),
        username: "me".into(),
        avatar: format!("{BASE}/api/avatars/user_fixture_me"),
        initials: "FM".into(),
        status: "online".into(),
        custom_status: String::new(),
    }))));
    assert!(fx.iter().any(
        |e| matches!(e, Effect::GatewayConnect { user_id, .. } if user_id == "user_fixture_me")
    ));
}

#[test]
fn servers_result_should_select_the_first_server_and_request_its_channels() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let (me, servers) = (
        request_for(&fx, &ApiRequest::Me),
        request_for(&fx, &ApiRequest::Servers),
    );
    client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
    assert!(fx.contains(&Effect::View(ViewUpdate::ActiveServer {
        id: SERVER.into(),
        name: "Fixture Server".into(),
    })));
    request_for(&fx, &channels());
    assert_eq!(client.state.servers().len(), 2);
}

#[test]
fn servers_answered_before_me_should_wait_for_me() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let (me, servers) = (
        request_for(&fx, &ApiRequest::Me),
        request_for(&fx, &ApiRequest::Servers),
    );
    let early = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
    assert!(early.is_empty(), "{early:#?}");
    let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let me_at = fx
        .iter()
        .position(|e| matches!(e, Effect::View(ViewUpdate::Me(_))));
    let servers_at = fx
        .iter()
        .position(|e| matches!(e, Effect::View(ViewUpdate::Servers(_))));
    assert!(me_at < servers_at && servers_at.is_some(), "{fx:#?}");
    request_for(&fx, &channels());
}

#[test]
fn channels_result_should_open_the_first_text_channel_and_request_its_page() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    let fx = client.respond(
        request_for(&fx, &channels()),
        &channels(),
        fixture("channels-response.json"),
    );
    let label = |id: &str, name: &str| ChannelTreeRow {
        kind: TreeKind::Label,
        id: id.into(),
        name: name.into(),
        voice: false,
        muted: false,
        speaking: false,
        me: false,
        avatar: String::new(),
        deafened: false,
        live: false,
        first: false,
        last: false,
        lit: 0,
    };
    let channel = |id: &str, name: &str, voice| ChannelTreeRow {
        kind: TreeKind::Channel,
        voice,
        ..label(id, name)
    };
    assert_eq!(
        views(&fx)[..3],
        [
            &ViewUpdate::Channels(ViewDiff::Reset(vec![
                label("cat_fixture_text", "Text"),
                channel(TEXT, "general", false),
                label("cat_fixture_voice", "Voice"),
                channel(VOICE, "lounge", true),
            ])),
            &ViewUpdate::ActiveChannel {
                id: TEXT.into(),
                name: "general".into()
            },
            &ViewUpdate::Messages(ViewDiff::Reset(Vec::new())),
        ]
    );
    request_for(&fx, &messages());
    assert_eq!(client.state.phase(), Phase::Loading);
}

#[test]
fn first_page_should_reset_messages_and_finish_boot() {
    let client = Client::signed_in();
    assert_eq!(client.state.phase(), Phase::Ready);
    assert_eq!(client.state.status(), "");
    assert_eq!(client.message_count(), 2);
}

#[test]
fn first_page_should_project_messages_like_before() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    let fx = client.respond(
        request_for(&fx, &channels()),
        &channels(),
        fixture("channels-response.json"),
    );
    let fx = client.respond(
        request_for(&fx, &messages()),
        &messages(),
        fixture("messages-page.json"),
    );
    let Some(ViewUpdate::Messages(ViewDiff::Reset(list))) = views(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(
        list[0],
        MessageRow {
            id: "msg_fixture_0002".into(),
            author_id: "user_fixture_bob".into(),
            author: "bob".into(),
            initials: "B".into(),
            avatar: String::new(),
            body: "see the attached file".into(),
            html: "see the attached file \
                   <span style=\"color:%EDITED%; font-size:12px\">(edited)</span>"
                .into(),
            created_at: "2026-11-04T12:00:05.000Z".into(),
            // `State::new` formats in UTC.
            time: "12:00 PM".into(),
            date: "Nov 4".into(),
            day: 20_761,
            edited: true,
            grouped: false,
            attachment_names: "notes.txt".into(),
            mine: false,
        }
    );
    assert!(fx.contains(&Effect::StartupMark("messages shown")));
    assert_eq!(
        views(&fx)[1..],
        [
            &ViewUpdate::Phase(Phase::Ready),
            &ViewUpdate::Status(String::new())
        ]
    );
}

#[test]
fn server_without_text_channels_should_clear_the_channel_and_finish_boot() {
    let mut body = fixture("channels-response.json");
    body["channels"]
        .as_array_mut()
        .expect("channels")
        .retain(|c| c["channel_type"] == "voice");
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    let fx = client.respond(request_for(&fx, &channels()), &channels(), body);
    assert!(requests(&fx).is_empty());
    assert_eq!(client.state.phase(), Phase::Ready);
    assert_eq!(client.state.active_channel(), None);
}

// Staleness

#[test]
fn page_answered_after_a_channel_switch_should_be_ignored() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    let old_page = request_for(&fx, &messages());
    let second = ApiRequest::Messages {
        channel_id: SECOND_TEXT.into(),
    };
    let fx = client.command(Command::SelectChannel(SECOND_TEXT.into()));
    let new_page = request_for(&fx, &second);

    let stale = client.respond(old_page, &messages(), fixture("messages-page.json"));
    assert!(stale.is_empty(), "{stale:#?}");
    assert_eq!(client.message_count(), 0);

    client.respond(
        new_page,
        &second,
        json!({ "messages": [message_in(SECOND_TEXT, "m1")] }),
    );
    assert_eq!(client.message_count(), 1);
}

#[test]
fn channels_answered_after_a_server_switch_should_be_ignored() {
    let mut client = Client::signed_in();
    let fx = client.command(Command::SelectServer(SERVER.into()));
    let old = request_for(&fx, &channels());
    let other = ApiRequest::Channels {
        server_id: "srv_fixture_two".into(),
    };
    let fx = client.command(Command::SelectServer("srv_fixture_two".into()));
    request_for(&fx, &other);
    let stale = client.respond(old, &channels(), fixture("channels-response.json"));
    assert!(stale.is_empty(), "{stale:#?}");
}

#[test]
fn a_stale_error_should_be_ignored() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    let old_page = request_for(&fx, &messages());
    client.command(Command::SelectChannel(SECOND_TEXT.into()));
    let fx = client.fail(old_page, ApiError::Unauthorized);
    assert!(fx.is_empty(), "{fx:#?}");
    assert!(client.state.session().is_some());
}

#[test]
fn events_from_a_replaced_session_should_be_ignored() {
    let mut client = Client::signed_in();
    let old = client.state.session().expect("session").id;
    client.command(Command::UseToken("tok_fixture_two".into()));
    let fx = client.input(Input::Gateway {
        session: old,
        event: GatewayEvent::Ready,
    });
    assert!(fx.is_empty(), "{fx:#?}");
}

#[test]
fn a_stale_sign_in_should_be_ignored() {
    let mut client = Client::new();
    let fx = client.command(Command::StartSignIn);
    let Some(Effect::SignInStart(first)) = fx.last().cloned() else {
        panic!("{fx:#?}");
    };
    client.command(Command::StartSignIn);
    let fx = client.input(Input::SignIn {
        attempt: first,
        event: SignInEvent::Approved(Token::new("tok_fixture")),
    });
    assert!(fx.is_empty(), "{fx:#?}");
}

// Dispatches

#[test]
fn message_create_in_the_open_channel_should_upsert() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-message-create.json",
    )));
    // The fixture reuses an id from the page: replaced in place, not added.
    let Some(ViewDiff::Update { at: 0, row, roles }) = message_diffs(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(row.body, "hello from the fixture");
    assert!(roles.contains(&MessageRole::Body) && !roles.contains(&MessageRole::Id));
    let log = client.state.messages().expect("log");
    assert_eq!(log.len(), 2);
    let message = log.get("msg_fixture_0002").expect("message");
    assert_eq!(message.content, "hello from the fixture");
}

#[test]
fn message_create_in_another_channel_should_not_touch_the_list() {
    let mut client = Client::online();
    let data = message_in(SECOND_TEXT, "msg_elsewhere");
    let create = Dispatch::decode("MESSAGE_CREATE", data).expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(create));
    assert!(fx.is_empty(), "{fx:#?}");
    assert_eq!(client.message_count(), 2);
}

#[test]
fn partial_update_without_content_should_keep_the_content() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-message-create.json",
    )));
    let fx = client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-message-update-partial.json",
    )));
    // Embeds are not shown yet: nothing on screen changes.
    assert!(views(&fx).is_empty(), "{fx:#?}");
    let log = client.state.messages().expect("log");
    let message = log.get("msg_fixture_0002").expect("message");
    assert_eq!(message.content, "hello from the fixture");
    assert_eq!(message.embeds.len(), 1);
}

#[test]
fn partial_update_with_content_should_patch_and_mark_edited() {
    let mut client = Client::online();
    let patch = Dispatch::decode(
        "MESSAGE_UPDATE",
        json!({ "id": "msg_fixture_0001", "channel_id": TEXT, "content": "edited!", "updated_at": "2026-11-04T13:00:00.000Z" }),
    )
    .expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(patch));
    let Some(ViewDiff::Update { at: 1, row, roles }) = message_diffs(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(
        (row.body.as_str(), row.edited, roles.as_slice()),
        (
            "edited!",
            true,
            [MessageRole::Body, MessageRole::Html, MessageRole::Edited].as_slice()
        )
    );
    let log = client.state.messages().expect("log");
    assert_eq!(
        log.get("msg_fixture_0001").expect("message").content,
        "edited!"
    );
}

#[test]
fn full_update_with_author_should_upsert_the_whole_message() {
    let update = Dispatch::decode("MESSAGE_UPDATE", message_in(TEXT, "msg_fixture_0001"));
    assert!(matches!(update, Some(Dispatch::MessageUpdate(_))));
}

#[test]
fn message_delete_in_the_open_channel_should_remove() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-message-delete.json",
    )));
    assert_eq!(
        message_diffs(&fx),
        vec![&ViewDiff::Remove { at: 0, count: 1 }]
    );
    let log = client.state.messages().expect("log");
    assert_eq!(log.len(), 1);
    assert!(
        log.get("msg_fixture_0001").is_some(),
        "index survives removal"
    );
}

#[test]
fn message_delete_in_another_channel_should_be_ignored() {
    let mut client = Client::online();
    let delete = Dispatch::decode(
        "MESSAGE_DELETE",
        json!({ "id": "msg_fixture_0002", "channel_id": SECOND_TEXT }),
    )
    .expect("decodes");
    assert!(client.gateway(GatewayEvent::Dispatch(delete)).is_empty());
    assert_eq!(client.message_count(), 2);
}

#[test]
fn typing_in_the_open_channel_should_show_the_known_name() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-typing-start.json",
    )));
    assert_eq!(
        views(&fx),
        vec![&ViewUpdate::Typing("Alice is typing…".into())]
    );
}

#[test]
fn typing_should_expire_only_after_the_last_event() {
    let mut client = Client::online();
    let typing = || GatewayEvent::Dispatch(dispatch("dispatch-typing-start.json"));
    let expiry = |fx: &[Effect]| {
        fx.iter()
            .find_map(|e| match e {
                Effect::Later { input, .. } => Some((**input).clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no expiry in {fx:#?}"))
    };
    let first = expiry(&client.gateway(typing()));
    let second = expiry(&client.gateway(typing()));
    assert!(client.input(first).is_empty());
    assert_eq!(
        views(&client.input(second)),
        vec![&ViewUpdate::Typing(String::new())]
    );
}

#[test]
fn a_new_message_should_take_its_author_off_the_typing_line() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(dispatch(
        "dispatch-typing-start.json",
    )));
    let data = message_in(TEXT, "msg_new");
    let create = Dispatch::decode("MESSAGE_CREATE", data).expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(create));
    assert!(fx.contains(&Effect::View(ViewUpdate::Typing(String::new()))));
    assert!(client.state.typing().is_empty());
}

#[test]
fn typing_elsewhere_or_by_me_should_be_ignored() {
    let mut client = Client::online();
    for data in [
        json!({ "channel_id": SECOND_TEXT, "user_id": "user_fixture_alice" }),
        json!({ "channel_id": TEXT, "user_id": "user_fixture_me" }),
    ] {
        let typing = Dispatch::decode("TYPING_START", data).expect("decodes");
        assert!(client.gateway(GatewayEvent::Dispatch(typing)).is_empty());
    }
}

#[test]
fn typing_by_an_unknown_user_should_say_someone() {
    let mut client = Client::online();
    let typing = Dispatch::decode(
        "TYPING_START",
        json!({ "channel_id": TEXT, "user_id": "user_unknown" }),
    )
    .expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(typing));
    assert_eq!(
        views(&fx),
        vec![&ViewUpdate::Typing("Someone is typing…".into())]
    );
}

#[test]
fn servers_changed_should_refetch_servers() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(
        Dispatch::decode("GUILD_UPDATE", json!({})).expect("decodes"),
    ));
    request_for(&fx, &ApiRequest::Servers);
}

// Sending

#[test]
fn send_should_post_and_dedupe_with_the_gateway_echo() {
    let mut client = Client::online();
    let fx = client.command(Command::Send("  hello from the fixture \n".into()));
    let post = ApiRequest::SendMessage {
        channel_id: TEXT.into(),
        content: "hello from the fixture".into(),
    };
    let id = request_for(&fx, &post);
    assert_eq!(post.body(), json!({ "content": "hello from the fixture" }));

    let fx = client.respond(id, &post, message_in(TEXT, "msg_sent"));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Insert { at: 2, rows }] if rows[0].id == "msg_sent"
    ));
    let echo = Dispatch::decode("MESSAGE_CREATE", message_in(TEXT, "msg_sent")).expect("decodes");
    client.gateway(GatewayEvent::Dispatch(echo));
    assert_eq!(client.message_count(), 3);
}

#[test]
fn an_empty_send_should_do_nothing() {
    let mut client = Client::online();
    assert!(client.command(Command::Send("   ".into())).is_empty());
}

#[test]
fn a_failed_send_should_show_the_error() {
    let mut client = Client::online();
    let fx = client.command(Command::Send("hi".into()));
    let id = requests(&fx)[0].0;
    let error = ApiError::Http {
        status: 403,
        message: "missing permission".into(),
    };
    let fx = client.fail(id, error);
    assert_eq!(
        views(&fx),
        vec![&ViewUpdate::Status("HTTP 403: missing permission".into())]
    );
}

// Unauthorized

#[test]
fn unauthorized_anywhere_should_sign_out() {
    type Step = fn(&mut Client) -> RequestId;
    let steps: [(&str, Step); 3] = [
        ("send", |c| {
            requests(&c.command(Command::Send("hi".into())))[0].0
        }),
        ("channels", |c| {
            requests(&c.command(Command::SelectServer(SERVER.into())))[0].0
        }),
        ("page", |c| {
            requests(&c.command(Command::SelectChannel(TEXT.into())))[0].0
        }),
    ];
    for (name, step) in steps {
        let mut client = Client::online();
        let id = step(&mut client);
        let fx = client.fail(id, ApiError::Unauthorized);
        assert!(fx.contains(&Effect::GatewayDisconnect), "{name}");
        assert!(fx.contains(&Effect::ClearToken), "{name}");
        assert_eq!(client.state.phase(), Phase::SignedOut, "{name}");
        assert_eq!(client.state.status(), SESSION_ENDED, "{name}");
        assert!(client.state.session().is_none(), "{name}");
    }
}

#[test]
fn unauthorized_me_should_clear_the_stored_token() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let fx = client.fail(request_for(&fx, &ApiRequest::Me), ApiError::Unauthorized);
    assert!(fx.contains(&Effect::ClearToken));
    assert_eq!(client.state.phase(), Phase::SignedOut);
    assert_eq!(client.state.status(), SESSION_ENDED);
}

#[test]
fn gateway_session_expired_should_sign_out_and_clear_the_token() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::SessionExpired);
    assert!(fx.contains(&Effect::ClearToken));
    assert_eq!(client.state.phase(), Phase::SignedOut);
}

#[test]
fn sign_out_should_revoke_and_clear_the_token() {
    let mut client = Client::online();
    let fx = client.command(Command::SignOut);
    assert!(fx.contains(&Effect::Revoke(Token::new("tok_fixture"))));
    assert!(fx.contains(&Effect::ClearToken));
    assert_eq!(client.state.phase(), Phase::SignedOut);
}

// Gateway

#[test]
fn first_ready_should_subscribe_without_refetching() {
    let mut client = Client::signed_in();
    let fx = client.gateway(GatewayEvent::Ready);
    assert!(requests(&fx).is_empty());
    let ops: Vec<i64> = frames(&fx).into_iter().map(|(op, _)| op).collect();
    assert_eq!(
        ops,
        [
            op(GatewayOp::ServerSubscribe),
            op(GatewayOp::ChannelSubscribe)
        ]
    );
    assert!(client.state.connected());
}

#[test]
fn ready_after_a_reconnect_should_refetch_and_resubscribe() {
    let mut client = Client::online();
    client.command(Command::JoinVoice(VOICE.into()));
    client.gateway(GatewayEvent::Closed);
    assert!(!client.state.connected());
    let fx = client.gateway(GatewayEvent::Ready);
    request_for(&fx, &messages());
    assert_eq!(
        frames(&fx),
        vec![
            (
                op(GatewayOp::ServerSubscribe),
                json!({ "server_id": SERVER })
            ),
            (
                op(GatewayOp::ChannelSubscribe),
                json!({ "channel_id": TEXT })
            ),
            (
                op(GatewayOp::VoiceChannelJoin),
                json!({ "channel_id": VOICE, "self_mute": false })
            ),
        ]
    );
}

#[test]
fn resumed_after_a_reconnect_should_resubscribe_without_refetching() {
    let mut client = Client::online();
    client.command(Command::JoinVoice(VOICE.into()));
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Resumed);
    assert!(requests(&fx).is_empty());
    // The server kept the voice presence: no second VoiceChannelJoin.
    let ops: Vec<i64> = frames(&fx).into_iter().map(|(op, _)| op).collect();
    assert_eq!(
        ops,
        [
            op(GatewayOp::ServerSubscribe),
            op(GatewayOp::ChannelSubscribe)
        ]
    );
    assert!(client.state.connected());
}

#[test]
fn frames_should_wait_for_the_gateway() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    let fx = client.command(Command::SelectChannel(SECOND_TEXT.into()));
    assert!(frames(&fx).is_empty());
}

#[test]
fn switching_channels_should_unsubscribe_then_subscribe() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    client.gateway(GatewayEvent::Ready);
    let fx = client.command(Command::SelectChannel(SECOND_TEXT.into()));
    assert_eq!(
        frames(&fx),
        vec![
            (
                op(GatewayOp::ChannelUnsubscribe),
                json!({ "channel_id": TEXT })
            ),
            (
                op(GatewayOp::ChannelSubscribe),
                json!({ "channel_id": SECOND_TEXT })
            ),
        ]
    );
}

// Voice

fn voice_call(fx: &[Effect]) -> CallId {
    fx.iter()
        .find_map(|e| match e {
            Effect::VoiceStart { call, .. } => Some(*call),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no VoiceStart in {fx:#?}"))
}

#[test]
fn joining_from_the_voice_view_should_start_the_call() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(VOICE.into()));
    let fx = client.command(Command::JoinVoice(VOICE.into()));
    let Some(Effect::VoiceStart { target, .. }) =
        fx.iter().find(|e| matches!(e, Effect::VoiceStart { .. }))
    else {
        panic!("{fx:#?}");
    };
    assert_eq!(
        (target.server_id.as_str(), target.channel_id.as_str()),
        (SERVER, VOICE)
    );
    assert_eq!(target.me_name, "Fixture Me");
    assert_eq!(client.state.active_channel(), Some(TEXT));
}

// Voice view and the Talk Tree

fn voice_states(states: Value) -> Dispatch {
    Dispatch::decode("VOICE_CHANNEL_STATES", json!({ "voice_states": states })).expect("decodes")
}

fn voice_state(channel: &str, members: Value) -> Dispatch {
    Dispatch::decode(
        "VOICE_CHANNEL_STATE_UPDATE",
        json!({ "channel_id": channel, "members": members }),
    )
    .expect("decodes")
}

fn occupant(user: &str, name: &str) -> Value {
    json!({
        "user_id": user, "name": name, "avatar_url": format!("/api/avatars/{user}"),
        "self_mute": false, "self_deaf": false, "self_video": false, "self_stream": false,
    })
}

fn members_of(fx: &[Effect]) -> Vec<ChannelTreeRow> {
    tree_diffs(fx)
        .into_iter()
        .flat_map(|d| match d {
            ViewDiff::Reset(rows) | ViewDiff::Insert { rows, .. } => rows.clone(),
            ViewDiff::Update { row, .. } => vec![row.clone()],
            ViewDiff::Remove { .. } => Vec::new(),
        })
        .filter(|r| r.kind == TreeKind::Member)
        .collect()
}

#[test]
fn selecting_a_voice_channel_should_open_its_view_without_joining() {
    let mut client = Client::online();
    let fx = client.command(Command::SelectChannel(VOICE.into()));
    assert!(!fx.iter().any(|e| matches!(e, Effect::VoiceStart { .. })));
    assert!(
        fx.contains(&Effect::View(ViewUpdate::VoicePanel(VoicePanelView {
            id: VOICE.into(),
            name: "lounge".into(),
            count: 0,
        })))
    );
    assert_eq!(client.state.voice_view(), Some(VOICE));
    assert_eq!(client.state.active_channel(), Some(TEXT));
}

#[test]
fn selecting_a_text_channel_should_close_the_voice_view() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(VOICE.into()));
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    assert!(fx.contains(&Effect::View(ViewUpdate::VoicePanel(
        VoicePanelView::default()
    ))));
    assert_eq!(client.state.voice_view(), None);
}

#[test]
fn voice_channel_states_should_put_occupants_under_every_voice_channel() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [occupant("user_ada", "Ada"), occupant("user_bob", "Bob")],
    }))));
    let rows = members_of(&fx);
    let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Ada", "Bob"]);
    assert_eq!(rows[0].id, format!("{VOICE}/user_ada"));
    assert_eq!(rows[0].avatar, format!("{BASE}/api/avatars/user_ada"));
    assert_eq!((rows[0].first, rows[0].last), (true, false));
    assert_eq!((rows[1].first, rows[1].last), (false, true));
    // Not in the call: no lit path.
    assert!(rows.iter().all(|r| r.lit == 0 && !r.me));
    assert_eq!(client.state.occupants(VOICE).len(), 2);
}

#[test]
fn a_voice_state_delta_should_keep_names_and_avatars() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [occupant("user_ada", "Ada")],
    }))));
    let fx = client.gateway(GatewayEvent::Dispatch(voice_state(
        VOICE,
        json!([{ "user_id": "user_ada", "self_mute": true, "self_deaf": true }]),
    )));
    let ada = &client.state.occupants(VOICE)[0];
    assert_eq!(
        (ada.name.as_str(), ada.muted, ada.deafened),
        ("Ada", true, true)
    );
    assert_eq!(ada.avatar_url, "/api/avatars/user_ada");
    assert_eq!(
        tree_diffs(&fx),
        vec![&ViewDiff::Update {
            at: 4,
            row: members_of(&fx)[0].clone(),
            roles: vec![TreeRole::Muted, TreeRole::Deafened],
        }]
    );
}

#[test]
fn an_empty_voice_state_should_clear_the_channel() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [occupant("user_ada", "Ada")],
    }))));
    let fx = client.gateway(GatewayEvent::Dispatch(voice_state(VOICE, json!([]))));
    assert!(client.state.occupants(VOICE).is_empty());
    assert_eq!(tree_diffs(&fx), vec![&ViewDiff::Remove { at: 4, count: 1 }]);
}

#[test]
fn the_path_should_run_down_to_you_and_speaking_should_come_from_the_call() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let mut bob = member("p_bob", true);
    bob.user_id = "user_bob".into();
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), bob]),
    });
    let fx = client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [
            occupant("user_bob", "Bob"),
            occupant("user_fixture_me", "Fixture Me"),
            occupant("user_cy", "Cy"),
        ],
    }))));
    let rows = members_of(&fx);
    let lit: Vec<_> = rows
        .iter()
        .map(|r| (r.name.as_str(), r.lit, r.speaking, r.me))
        .collect();
    assert_eq!(
        lit,
        [
            ("Bob", 1, true, false),
            ("Fixture Me", 2, false, true),
            ("Cy", 0, false, false),
        ]
    );
}

#[test]
fn the_call_roster_should_stand_in_until_the_gateway_lists_the_channel() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), member("p_ana", true)]),
    });
    let rows = members_of(&fx);
    let names: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.speaking)).collect();
    assert_eq!(names, [("P_ME", false), ("P_ANA", true)]);
    assert_eq!(rows[0].id, format!("{VOICE}/user_fixture_me"));
}

#[test]
fn the_voice_view_roster_should_count_who_is_in_the_channel() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(VOICE.into()));
    let fx = client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [occupant("user_ada", "Ada")],
    }))));
    assert!(
        fx.contains(&Effect::View(ViewUpdate::VoicePanel(VoicePanelView {
            id: VOICE.into(),
            name: "lounge".into(),
            count: 1,
        })))
    );
    let roster: Vec<_> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Roster(ViewDiff::Insert { rows, .. })) => Some(rows.clone()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(roster.len(), 1);
    assert_eq!(
        (roster[0].id.as_str(), roster[0].initials.as_str()),
        ("user_ada", "A")
    );
}

#[test]
fn deafen_should_be_kept_and_applied_when_joining() {
    let mut client = Client::online();
    let fx = client.command(Command::SetVoiceDeafened(true));
    assert!(fx.contains(&Effect::View(ViewUpdate::Voice(VoiceView {
        deafened: true,
        ..VoiceView::default()
    }))));
    let fx = client.command(Command::JoinVoice(VOICE.into()));
    let call = voice_call(&fx);
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetDeafened(true),
    }));
    assert!(client.state.voice().is_some_and(|c| c.deafened));
}

#[test]
fn deafen_in_a_call_should_reach_the_voice_session() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::SetVoiceDeafened(true));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetDeafened(true),
    }));
}

#[test]
fn switching_servers_should_close_the_voice_view() {
    let mut client = Client::online();
    client.command(Command::SelectChannel(VOICE.into()));
    let fx = client.command(Command::SelectServer("srv_other".into()));
    assert!(fx.contains(&Effect::View(ViewUpdate::VoicePanel(
        VoicePanelView::default()
    ))));
}

#[test]
fn joining_twice_should_keep_the_call() {
    let mut client = Client::online();
    client.command(Command::JoinVoice(VOICE.into()));
    assert!(client.command(Command::JoinVoice(VOICE.into())).is_empty());
}

#[test]
fn voice_connected_should_assert_presence() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
    assert_eq!(
        frames(&fx),
        vec![(
            op(GatewayOp::VoiceChannelJoin),
            json!({ "channel_id": VOICE, "self_mute": false })
        )]
    );
}

#[test]
fn leaving_voice_should_stop_the_call_and_clear_presence() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::LeaveVoice);
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::Leave
    }));
    assert_eq!(
        frames(&fx),
        vec![(
            op(GatewayOp::VoiceChannelLeave),
            json!({ "channel_id": VOICE })
        )]
    );
    assert!(client.state.voice().is_none());
}

#[test]
fn mute_should_reach_the_call_and_the_ui() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::SetVoiceMuted(true));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetMuted(true)
    }));
    assert!(matches!(
        views(&fx)[..],
        [ViewUpdate::Voice(VoiceView { muted: true, .. })]
    ));
}

// View diffs

/// Online in TEXT with a page of `(id, author, created_at)` (oldest first),
/// and a row vector that mirrors the model.
fn chat(rows: &[(&str, &str, &str)]) -> (Client, Vec<MessageRow>) {
    let mut client = Client::online();
    let fx = client.command(Command::SelectChannel(TEXT.into()));
    let page: Vec<Value> = rows
        .iter()
        .map(|(id, author, at)| {
            let mut m = message_in(TEXT, id);
            m["author_id"] = json!(author);
            m["author"] = json!({ "id": author, "username": author });
            m["created_at"] = json!(at);
            m
        })
        .collect();
    let fx = client.respond(
        request_for(&fx, &messages()),
        &messages(),
        json!({ "messages": page }),
    );
    let mut model = Vec::new();
    replay_messages(&mut model, &fx);
    (client, model)
}

fn grouped(rows: &[MessageRow]) -> Vec<bool> {
    rows.iter().map(|r| r.grouped).collect()
}

const T0: &str = "2026-11-04T12:00:00.000Z";
const T1: &str = "2026-11-04T12:01:00.000Z";
const T2: &str = "2026-11-04T12:02:00.000Z";
const LATER: &str = "2026-11-04T13:00:00.000Z";

#[test]
fn page_rows_should_group_by_author_within_the_window() {
    let (_, model) = chat(&[
        ("m1", "alice", T0),
        ("m2", "alice", T1),
        ("m3", "bob", T2),
        ("m4", "bob", LATER),
    ]);
    assert_eq!(grouped(&model), [false, true, false, false]);
}

#[test]
fn a_new_message_should_insert_at_the_end_grouped_with_the_last() {
    let (mut client, mut model) = chat(&[("m1", "alice", T0)]);
    let fx = client.gateway(GatewayEvent::Dispatch(message("m2", "alice", T1)));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Insert { at: 1, rows }] if rows.len() == 1 && rows[0].grouped
    ));
    replay_messages(&mut model, &fx);
    assert_eq!(grouped(&model), [false, true]);
}

#[test]
fn removing_a_group_head_should_regroup_the_next_row() {
    let (mut client, mut model) =
        chat(&[("m1", "bob", T0), ("m2", "alice", T1), ("m3", "alice", T2)]);
    let delete = Dispatch::decode("MESSAGE_DELETE", json!({ "id": "m2", "channel_id": TEXT }))
        .expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(delete));
    let Some(ViewDiff::Update { at: 1, roles, .. }) = message_diffs(&fx).get(1).copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(roles, &[MessageRole::Grouped]);
    replay_messages(&mut model, &fx);
    assert_eq!((model.len(), grouped(&model)), (2, vec![false, false]));
}

#[test]
fn removing_a_row_between_two_by_one_author_should_join_them() {
    let (mut client, mut model) =
        chat(&[("m1", "alice", T0), ("m2", "bob", T1), ("m3", "alice", T2)]);
    let delete = Dispatch::decode("MESSAGE_DELETE", json!({ "id": "m2", "channel_id": TEXT }))
        .expect("decodes");
    replay_messages(&mut model, &client.gateway(GatewayEvent::Dispatch(delete)));
    assert_eq!(grouped(&model), [false, true]);
}

#[test]
fn removing_the_last_row_should_emit_only_the_removal() {
    let (mut client, _) = chat(&[("m1", "alice", T0), ("m2", "alice", T1)]);
    let delete = Dispatch::decode("MESSAGE_DELETE", json!({ "id": "m2", "channel_id": TEXT }))
        .expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(delete));
    assert_eq!(
        message_diffs(&fx),
        vec![&ViewDiff::Remove { at: 1, count: 1 }]
    );
}

#[test]
fn an_edit_should_update_only_the_changed_roles() {
    let (mut client, mut model) = chat(&[("m1", "alice", T0), ("m2", "alice", T1)]);
    let patch = Dispatch::decode(
        "MESSAGE_UPDATE",
        json!({ "id": "m1", "channel_id": TEXT, "content": "fixed" }),
    )
    .expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(patch));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Update { at: 0, roles, .. }] if roles == &[MessageRole::Body, MessageRole::Html]
    ));
    replay_messages(&mut model, &fx);
    assert_eq!(model[0].body, "fixed");
}

#[test]
fn own_messages_should_be_marked_mine() {
    let (mut client, _) = chat(&[]);
    let fx = client.gateway(GatewayEvent::Dispatch(message("m1", "user_fixture_me", T0)));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Insert { rows, .. }] if rows[0].mine
    ));
}

#[test]
fn switching_channels_should_reset_the_messages() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    let fx = client.command(Command::SelectChannel(SECOND_TEXT.into()));
    assert_eq!(message_diffs(&fx), vec![&ViewDiff::Reset(Vec::new())]);
}

#[test]
fn joining_voice_should_put_the_roster_under_the_voice_channel() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), member("p_two", false)]),
    });
    // Label, general, label, lounge: members go in at 4.
    let Some(ViewDiff::Insert { at: 4, rows }) = tree_diffs(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    let names: Vec<(TreeKind, &str, bool)> =
        rows.iter().map(|r| (r.kind, r.id.as_str(), r.me)).collect();
    assert_eq!(
        names,
        [
            (TreeKind::Member, "chan_fixture_voice/user_fixture_me", true),
            (TreeKind::Member, "chan_fixture_voice/p_two", false)
        ]
    );
}

#[test]
fn a_speaking_flip_should_update_one_tree_row_and_one_roster_row() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let roster =
        |speaking| VoiceUpdate::Roster(vec![member("p_me", false), member("p_two", speaking)]);
    client.input(Input::Voice {
        call,
        update: roster(false),
    });
    let fx = client.input(Input::Voice {
        call,
        update: roster(true),
    });
    assert!(matches!(
        tree_diffs(&fx)[..],
        [ViewDiff::Update { at: 5, roles, .. }] if roles == &[TreeRole::Speaking]
    ));
    let roster_updates = views(&fx)
        .into_iter()
        .filter(|v| matches!(v, ViewUpdate::Roster(ViewDiff::Update { at: 1, .. })))
        .count();
    assert_eq!(roster_updates, 1);
}

#[test]
fn leaving_voice_should_remove_the_member_rows() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), member("p_two", false)]),
    });
    let fx = client.command(Command::LeaveVoice);
    assert_eq!(tree_diffs(&fx), vec![&ViewDiff::Remove { at: 4, count: 2 }]);
}

#[test]
fn voice_view_should_name_the_channel_and_go_live_when_connected() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Status("Connected".into()),
    });
    assert!(fx.contains(&Effect::View(ViewUpdate::Voice(VoiceView {
        channel_id: VOICE.into(),
        channel_name: "lounge".into(),
        status: "Connected".into(),
        live: true,
        muted: false,
        deafened: false,
    }))));
}

#[test]
fn updates_from_an_old_call_should_be_ignored() {
    let mut client = Client::online();
    let old = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.command(Command::LeaveVoice);
    client.command(Command::JoinVoice(VOICE.into()));
    let fx = client.input(Input::Voice {
        call: old,
        update: VoiceUpdate::Ended(Some("boom".into())),
    });
    assert!(fx.is_empty(), "{fx:#?}");
    assert!(client.state.voice().is_some());
}

#[test]
fn a_call_that_ends_with_an_error_should_say_why() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Ended(Some("room socket: closed".into())),
    });
    assert!(fx.contains(&Effect::VoiceForget(call)));
    assert_eq!(client.state.status(), "Left the call: room socket: closed");
    assert_eq!(
        frames(&fx),
        vec![(
            op(GatewayOp::VoiceChannelLeave),
            json!({ "channel_id": VOICE })
        )]
    );
}

// Review fixes

#[test]
fn a_servers_refetch_should_keep_the_open_channel() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    client.gateway(GatewayEvent::Ready);
    let fx = client.command(Command::SelectChannel(SECOND_TEXT.into()));
    let second = ApiRequest::Messages {
        channel_id: SECOND_TEXT.into(),
    };
    client.respond(
        request_for(&fx, &second),
        &second,
        json!({ "messages": [] }),
    );

    let changed = Dispatch::decode("GUILD_UPDATE", json!({})).expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(changed));
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    let fx = client.respond(
        request_for(&fx, &channels()),
        &channels(),
        channels_with_two_text(),
    );
    assert!(message_diffs(&fx).is_empty(), "{fx:#?}");
    assert!(requests(&fx).is_empty(), "{fx:#?}");
    assert_eq!(client.state.active_channel(), Some(SECOND_TEXT));
    assert_eq!(client.state.active_server(), Some(SERVER));
}

#[test]
fn a_refetch_without_the_open_channel_should_fall_back_to_the_first_text() {
    let mut client = Client::signed_in_with(channels_with_two_text());
    client.command(Command::SelectChannel(SECOND_TEXT.into()));
    let fx = client.command(Command::SelectServer(SERVER.into()));
    let fx = client.respond(
        request_for(&fx, &channels()),
        &channels(),
        fixture("channels-response.json"),
    );
    request_for(&fx, &messages());
    assert_eq!(client.state.active_channel(), Some(TEXT));
}

#[test]
fn sign_out_should_clear_the_shell_scalars_and_messages() {
    let mut client = Client::online();
    let fx = client.command(Command::SignOut);
    let cleared = [
        ViewUpdate::Me(MeView::default()),
        ViewUpdate::ActiveServer {
            id: String::new(),
            name: String::new(),
        },
        ViewUpdate::ActiveChannel {
            id: String::new(),
            name: String::new(),
        },
        ViewUpdate::Messages(ViewDiff::Reset(Vec::new())),
    ];
    for update in cleared {
        assert!(
            fx.contains(&Effect::View(update.clone())),
            "{update:?} in {fx:#?}"
        );
    }
}

#[test]
fn a_token_during_device_sign_in_should_cancel_it() {
    let mut client = Client::new();
    let fx = client.command(Command::StartSignIn);
    let Some(Effect::SignInStart(attempt)) = fx.last().cloned() else {
        panic!("{fx:#?}");
    };
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    assert_eq!(fx.first(), Some(&Effect::SignInCancel));
    let late = client.input(Input::SignIn {
        attempt,
        event: SignInEvent::Approved(Token::new("tok_other")),
    });
    assert!(late.is_empty(), "{late:#?}");
}

#[test]
fn categories_sharing_a_rank_should_get_one_label_each() {
    let body = json!({
        "categories": [
            { "id": "cat_a", "name": "A", "rank": 0 },
            { "id": "cat_b", "name": "B", "rank": 0 },
        ],
        "channels": [
            { "id": "a1", "name": "a1", "channel_type": "text", "category_id": "cat_a", "position": 0 },
            { "id": "b1", "name": "b1", "channel_type": "text", "category_id": "cat_b", "position": 1 },
            { "id": "a2", "name": "a2", "channel_type": "text", "category_id": "cat_a", "position": 2 },
        ],
    });
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    let fx = client.respond(request_for(&fx, &channels()), &channels(), body);
    let Some(ViewDiff::Reset(rows)) = tree_diffs(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["cat_a", "a1", "a2", "cat_b", "b1"]);
}

// Cached boot (snapshot cache)

const ME: &str = "user_fixture_me";

/// The last session as the cache returns it: the fixture user on the
/// fixture server's text channel, with the fixture page.
fn snapshot() -> Snapshot {
    let ApiResponse::Messages(messages) = messages()
        .decode(fixture("messages-page.json"))
        .expect("page")
    else {
        panic!("not a page");
    };
    Snapshot {
        me: serde_json::from_value(fixture("user-me.json")).expect("me"),
        servers: serde_json::from_value(fixture("servers-list.json")).expect("servers"),
        channels: vec![(
            SERVER.into(),
            serde_json::from_value(fixture("channels-response.json")).expect("channels"),
        )],
        active_server: Some(SERVER.into()),
        active_channel: Some(TEXT.into()),
        messages,
        written_at_ms: 0,
    }
}

fn cache_writes(fx: &[Effect]) -> Vec<&CacheWrite> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::CacheWrite { user_id, write } if user_id == ME => Some(write),
            _ => None,
        })
        .collect()
}

fn cleared(fx: &[Effect], user: &str) -> bool {
    fx.contains(&Effect::CacheClear {
        user_id: user.into(),
    })
}

impl Client {
    /// `Start`, then the token and the snapshot reads; returns the effects
    /// of the read that completed the launch.
    fn launch(snapshot: Option<Snapshot>) -> (Self, Vec<Effect>) {
        let mut client = Self::new();
        client.command(Command::Start);
        let token = client.input(Input::TokenLoaded(Some(Token::new("tok_fixture"))));
        assert!(token.is_empty(), "waits for the snapshot: {token:#?}");
        let fx = client.input(Input::SnapshotLoaded(snapshot.map(Box::new)));
        (client, fx)
    }

    /// Answers the cached boot's requests with the fixtures, `page` for the page.
    fn revalidate(&mut self, boot: &[Effect], page: Value) -> Vec<Effect> {
        let mut fx = self.respond(
            request_for(boot, &ApiRequest::Me),
            &ApiRequest::Me,
            fixture("user-me.json"),
        );
        fx.extend(self.respond(
            request_for(boot, &ApiRequest::Servers),
            &ApiRequest::Servers,
            fixture("servers-list.json"),
        ));
        fx.extend(self.respond(
            request_for(boot, &channels()),
            &channels(),
            fixture("channels-response.json"),
        ));
        fx.extend(self.respond(request_for(boot, &messages()), &messages(), page));
        fx
    }
}

#[test]
fn start_should_read_the_token_and_the_snapshot_in_parallel() {
    let mut client = Client::new();
    assert_eq!(
        client.command(Command::Start),
        vec![Effect::LoadToken, Effect::LoadSnapshot]
    );
}

#[test]
fn a_snapshot_read_first_should_wait_for_the_token() {
    let mut client = Client::new();
    client.command(Command::Start);
    let fx = client.input(Input::SnapshotLoaded(Some(Box::new(snapshot()))));
    assert!(fx.is_empty(), "{fx:#?}");
    let fx = client.input(Input::TokenLoaded(Some(Token::new("tok_fixture"))));
    assert_eq!(message_diffs(&fx).len(), 1, "{fx:#?}");
}

#[test]
fn cached_boot_should_show_the_snapshot_before_any_http_result() {
    let (client, fx) = Client::launch(Some(snapshot()));
    let rows = match message_diffs(&fx).as_slice() {
        [ViewDiff::Reset(rows)] => rows.clone(),
        other => panic!("{other:#?}"),
    };
    assert_eq!(rows.len(), 2);
    assert!(fx.contains(&Effect::View(ViewUpdate::Phase(Phase::Ready))));
    assert!(fx.contains(&Effect::View(ViewUpdate::ActiveChannel {
        id: TEXT.into(),
        name: "general".into(),
    })));
    assert!(fx.contains(&Effect::StartupMark("cached content")));
    assert_eq!(client.state.phase(), Phase::Ready);
    assert!(client.state.refreshing());
    assert_eq!(client.state.status(), "Refreshing…");
}

#[test]
fn cached_boot_should_revalidate_everything_in_one_batch() {
    let (_, fx) = Client::launch(Some(snapshot()));
    let kinds: Vec<ApiRequest> = requests(&fx).into_iter().map(|(_, r)| r).collect();
    assert_eq!(
        kinds,
        vec![ApiRequest::Me, ApiRequest::Servers, channels(), messages()]
    );
}

#[test]
fn cached_boot_should_connect_the_gateway_as_the_cached_user_at_once() {
    let (_, fx) = Client::launch(Some(snapshot()));
    assert!(fx
        .iter()
        .any(|e| matches!(e, Effect::GatewayConnect { user_id, .. } if user_id == ME)));
}

#[test]
fn revalidation_that_matches_the_cache_should_change_no_rows() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.revalidate(&boot, fixture("messages-page.json"));
    assert!(message_diffs(&fx).is_empty(), "{:#?}", message_diffs(&fx));
    assert!(tree_diffs(&fx).is_empty(), "{:#?}", tree_diffs(&fx));
    assert!(
        !views(&fx)
            .iter()
            .any(|v| matches!(v, ViewUpdate::Servers(_))),
        "{fx:#?}"
    );
}

#[test]
fn revalidation_should_not_refetch_what_it_already_requested() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.revalidate(&boot, fixture("messages-page.json"));
    assert!(requests(&fx).is_empty(), "{:#?}", requests(&fx));
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::GatewayConnect { .. })),
        "the gateway is already up: {fx:#?}"
    );
}

#[test]
fn revalidation_should_clear_the_refreshing_status() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    client.revalidate(&boot, fixture("messages-page.json"));
    assert!(!client.state.refreshing());
    assert_eq!(client.state.status(), "");
}

#[test]
fn revalidation_should_update_only_the_changed_row() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let mut page = fixture("messages-page.json");
    page["messages"][0]["content"] = json!("edited while away");
    let fx = client.revalidate(&boot, page);
    match message_diffs(&fx).as_slice() {
        [ViewDiff::Update { at: 0, roles, .. }] => {
            assert!(roles.contains(&MessageRole::Body), "{roles:?}");
        }
        other => panic!("{other:#?}"),
    }
}

#[test]
fn revalidation_should_insert_new_messages_after_the_cached_ones() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let mut rows = match message_diffs(&boot).as_slice() {
        [ViewDiff::Reset(rows)] => rows.clone(),
        other => panic!("{other:#?}"),
    };
    let mut page = fixture("messages-page.json");
    let messages = page["messages"].as_array_mut().expect("messages");
    messages.push(message_in(TEXT, "msg_fixture_new"));
    let fx = client.revalidate(&boot, page);
    assert!(
        !message_diffs(&fx)
            .iter()
            .any(|d| matches!(d, ViewDiff::Reset(_))),
        "{fx:#?}"
    );
    replay_messages(&mut rows, &fx);
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["msg_fixture_0002", "msg_fixture_0001", "msg_fixture_new"]
    );
}

#[test]
fn unauthorized_revalidation_should_sign_out_and_clear_the_snapshot() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.fail(request_for(&boot, &ApiRequest::Me), ApiError::Unauthorized);
    assert!(cleared(&fx, ME), "{fx:#?}");
    assert!(fx.contains(&Effect::ClearToken));
    assert!(fx.contains(&Effect::GatewayDisconnect));
    assert_eq!(client.state.phase(), Phase::SignedOut);
}

#[test]
fn unauthorized_page_during_revalidation_should_clear_the_snapshot() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.fail(request_for(&boot, &messages()), ApiError::Unauthorized);
    assert!(cleared(&fx, ME), "{fx:#?}");
    assert_eq!(client.state.phase(), Phase::SignedOut);
}

#[test]
fn a_network_error_on_me_should_keep_the_cached_view() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.fail(
        request_for(&boot, &ApiRequest::Me),
        ApiError::Network("offline".into()),
    );
    assert!(!cleared(&fx, ME), "{fx:#?}");
    assert_eq!(client.state.phase(), Phase::Ready);
    assert_eq!(client.state.status(), "offline");
    assert_eq!(client.message_count(), 2);
}

#[test]
fn no_stored_token_should_sign_out_and_clear_the_snapshot() {
    let mut client = Client::new();
    client.command(Command::Start);
    client.input(Input::SnapshotLoaded(Some(Box::new(snapshot()))));
    let fx = client.input(Input::TokenLoaded(None));
    assert!(cleared(&fx, ME), "{fx:#?}");
    assert_eq!(client.state.phase(), Phase::SignedOut);
}

#[test]
fn no_snapshot_should_boot_from_the_network_as_before() {
    let (client, fx) = Client::launch(None);
    let kinds: Vec<ApiRequest> = requests(&fx).into_iter().map(|(_, r)| r).collect();
    assert_eq!(kinds, vec![ApiRequest::Me, ApiRequest::Servers]);
    assert_eq!(client.state.phase(), Phase::Loading);
}

#[test]
fn sign_out_should_clear_the_snapshot() {
    let mut client = Client::online();
    let fx = client.command(Command::SignOut);
    assert!(cleared(&fx, ME), "{fx:#?}");
}

#[test]
fn gateway_session_expired_should_clear_the_snapshot() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::SessionExpired);
    assert!(cleared(&fx, ME), "{fx:#?}");
}

#[test]
fn me_for_another_user_should_drop_the_cache_and_start_over() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let mut other = fixture("user-me.json");
    other["id"] = json!("user_fixture_other");
    let fx = client.respond(request_for(&boot, &ApiRequest::Me), &ApiRequest::Me, other);
    assert!(cleared(&fx, ME), "{fx:#?}");
    assert!(fx.contains(&Effect::GatewayDisconnect));
    let kinds: Vec<ApiRequest> = requests(&fx).into_iter().map(|(_, r)| r).collect();
    assert_eq!(kinds, vec![ApiRequest::Me, ApiRequest::Servers]);
    assert_eq!(client.state.phase(), Phase::Loading);
    assert_eq!(client.message_count(), 0);
}

#[test]
fn results_for_the_dropped_cached_session_should_be_ignored() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let mut other = fixture("user-me.json");
    other["id"] = json!("user_fixture_other");
    client.respond(request_for(&boot, &ApiRequest::Me), &ApiRequest::Me, other);
    let fx = client.respond(
        request_for(&boot, &messages()),
        &messages(),
        fixture("messages-page.json"),
    );
    assert!(fx.is_empty(), "{fx:#?}");
}

#[test]
fn a_token_pasted_while_launching_should_win_over_the_stored_one() {
    let mut client = Client::new();
    client.command(Command::Start);
    client.command(Command::UseToken("tok_pasted".into()));
    let fx = client.input(Input::TokenLoaded(Some(Token::new("tok_fixture"))));
    let fx2 = client.input(Input::SnapshotLoaded(Some(Box::new(snapshot()))));
    assert!(fx.is_empty() && fx2.is_empty(), "{fx:#?} {fx2:#?}");
}

#[test]
fn a_network_boot_should_write_through_every_part_of_the_snapshot() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let mut all = client.respond(
        request_for(&fx, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let fx = client.respond(
        request_for(&fx, &ApiRequest::Servers),
        &ApiRequest::Servers,
        fixture("servers-list.json"),
    );
    all.extend(fx.clone());
    let fx = client.respond(
        request_for(&fx, &channels()),
        &channels(),
        fixture("channels-response.json"),
    );
    all.extend(fx.clone());
    all.extend(client.respond(
        request_for(&fx, &messages()),
        &messages(),
        fixture("messages-page.json"),
    ));
    let writes = cache_writes(&all);
    assert!(writes
        .iter()
        .any(|w| matches!(w, CacheWrite::Me(me) if me.id == ME)));
    assert!(writes
        .iter()
        .any(|w| matches!(w, CacheWrite::Servers(list) if list.len() == 2)));
    assert!(writes
        .iter()
        .any(|w| matches!(w, CacheWrite::Channels { server_id, .. } if server_id == SERVER)));
    assert!(writes.contains(&&CacheWrite::Route {
        server_id: Some(SERVER.into()),
        channel_id: Some(TEXT.into()),
    }));
    assert!(writes.iter().any(|w| matches!(
        w,
        CacheWrite::Messages { channel_id, messages } if channel_id == TEXT && messages.len() == 2
    )));
}

#[test]
fn a_new_message_should_write_the_channel_window() {
    let mut client = Client::online();
    let fx = client.gateway(GatewayEvent::Dispatch(message(
        "msg_new",
        "user_fixture_alice",
        "2026-11-01T12:10:00.000Z",
    )));
    assert!(cache_writes(&fx).iter().any(|w| matches!(
        w,
        CacheWrite::Messages { messages, .. } if messages.len() == 3
    )));
}
