//! Reducer table tests driven by the shared fixtures in
//! `packages/protocol/fixtures`. No network, keyring, runtime or Qt: inputs
//! go into `reduce`, and the test asserts on the state and the effects.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::view::{
    typing_text, ChannelTreeRow, MeView, MessageRole, MessageRow, ServerRow, TreeKind, TreeRole,
    VoicePanelView, VoiceView,
};
use yap_core::store::{
    reduce, ApiRequest, ApiResponse, CacheWrite, CallId, Command, Dispatch, Effect, GatewayEvent,
    Input, Method, Page, Phase, RequestId, SignInEvent, Snapshot, State, UnreadView, ViewDiff,
    ViewUpdate, VoiceCommand, VoicePrefs, VoiceUpdate,
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

/// The UI model updates, without the shell's (toasts, sounds, the badge),
/// which `tests/shell.rs` covers.
fn views(fx: &[Effect]) -> Vec<&ViewUpdate> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(update) if !is_shell(update) => Some(update),
            _ => None,
        })
        .collect()
}

fn is_shell(update: &ViewUpdate) -> bool {
    matches!(
        update,
        ViewUpdate::Toast(_) | ViewUpdate::Sound(_) | ViewUpdate::Badge(_)
    )
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
        ..VoiceMember::default()
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
        before: None,
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
        video: false,
        first: false,
        last: false,
        lit: 0,
        unread: false,
        mentions: 0,
        manage: false,
        category: String::new(),
    };
    // Members have not answered: no Manage channels yet.
    let channel = |id: &str, name: &str, voice, category: &str| ChannelTreeRow {
        kind: TreeKind::Channel,
        voice,
        category: category.into(),
        ..label(id, name)
    };
    assert_eq!(
        views(&fx)[..5],
        [
            &ViewUpdate::Channels(ViewDiff::Reset(vec![
                label("cat_fixture_text", "Text"),
                channel(TEXT, "general", false, "cat_fixture_text"),
                label("cat_fixture_voice", "Voice"),
                channel(VOICE, "lounge", true, "cat_fixture_voice"),
            ])),
            &ViewUpdate::ActiveChannel {
                id: TEXT.into(),
                name: "general".into()
            },
            // The fixture channel's permissions include MANAGE_MESSAGES.
            &ViewUpdate::ManageMessages(true),
            &ViewUpdate::Messages(Box::new(ViewDiff::Reset(Vec::new()))),
            &ViewUpdate::History {
                loading: false,
                more: false
            },
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
    assert!(matches!(views(&fx).first(), Some(ViewUpdate::Messages(_))));
    let Some(ViewDiff::Reset(list)) = message_diffs(&fx).first().copied() else {
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
            attachments: list[0].attachments.clone(),
            reactions: list[0].reactions.clone(),
            embeds: String::new(),
            mine: false,
            nonce: String::new(),
            pending: false,
            failed: false,
            mentioned: false,
            divider: false,
            reply_id: String::new(),
            reply_author: String::new(),
            reply_avatar: String::new(),
            reply_initials: String::new(),
            reply_text: String::new(),
            reply_placeholder: false,
            reply_count: 0,
            pinned: false,
        }
    );
    assert!(list[0].attachments.contains(r#""name":"notes.txt""#));
    assert!(list[0].reactions.contains(r#""count":1"#));
    assert!(fx.contains(&Effect::StartupMark("messages shown")));
    assert_eq!(
        views(&fx)[1..],
        [
            &ViewUpdate::Phase(Phase::Ready),
            &ViewUpdate::Status(String::new()),
            &ViewUpdate::History {
                loading: false,
                more: true
            },
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
        before: None,
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
    assert!(
        fx.iter()
            .all(|e| matches!(e, Effect::View(update) if is_shell(update))),
        "{fx:#?}"
    );
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
    // Only the embeds role changes.
    let Some(ViewDiff::Update { roles, row, .. }) = message_diffs(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(roles.as_slice(), [MessageRole::Embeds]);
    assert!(row.embeds.contains("https://example.com/fixture"));
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

fn send(content: &str, nonce: &str) -> Command {
    Command::Send {
        content: content.into(),
        reply_to_id: None,
        nonce: nonce.into(),
        sent_at: "2026-11-04T12:30:00.000Z".into(),
    }
}

fn post(content: &str, nonce: &str) -> ApiRequest {
    ApiRequest::SendMessage {
        channel_id: TEXT.into(),
        content: content.into(),
        reply_to_id: None,
        nonce: nonce.into(),
        attachment_ids: Vec::new(),
        nsfw_attachment_ids: Vec::new(),
    }
}

/// The page fixture's message, sent by us with `nonce`.
fn echo(id: &str, nonce: &str) -> Value {
    let mut data = message_in(TEXT, id);
    data["author_id"] = json!(ME);
    data["author"] = fixture("user-me.json");
    data["nonce"] = json!(nonce);
    data["created_at"] = json!("2026-11-04T12:30:01.000Z");
    data
}

#[test]
fn send_should_show_a_pending_row_at_once() {
    let mut client = Client::online();
    let fx = client.command(send(
        "  hello from the fixture 
",
        "n1",
    ));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Insert { at: 2, rows }]
            if rows[0].id == "pending-n1" && rows[0].pending && rows[0].mine
    ));
}

#[test]
fn send_should_post_the_trimmed_text_with_its_nonce() {
    let mut client = Client::online();
    let fx = client.command(send(
        "  hello from the fixture 
",
        "n1",
    ));
    request_for(&fx, &post("hello from the fixture", "n1"));
    assert_eq!(
        post("hi", "n1").body(),
        json!({ "content": "hi", "nonce": "n1" })
    );
}

#[test]
fn send_should_carry_reply_to_id() {
    let request = ApiRequest::SendMessage {
        channel_id: TEXT.into(),
        content: "yes".into(),
        reply_to_id: Some("msg_fixture_0001".into()),
        nonce: "n1".into(),
        attachment_ids: Vec::new(),
        nsfw_attachment_ids: Vec::new(),
    };
    assert_eq!(request.body()["reply_to_id"], json!("msg_fixture_0001"));
}

#[test]
fn the_gateway_echo_should_take_the_pending_rows_place() {
    let mut client = Client::online();
    client.command(send("hello", "n1"));
    let echo = Dispatch::decode("MESSAGE_CREATE", echo("msg_sent", "n1")).expect("decodes");
    let fx = client.gateway(GatewayEvent::Dispatch(echo));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Update { at: 2, row, roles }]
            if row.id == "msg_sent" && !row.pending && roles.contains(&MessageRole::Pending)
    ));
}

#[test]
fn the_post_answer_after_the_echo_should_not_add_a_row() {
    let mut client = Client::online();
    let fx = client.command(send("hello", "n1"));
    let id = request_for(&fx, &post("hello", "n1"));
    let echo_frame = Dispatch::decode("MESSAGE_CREATE", echo("msg_sent", "n1")).expect("decodes");
    client.gateway(GatewayEvent::Dispatch(echo_frame));
    client.respond(id, &post("hello", "n1"), echo("msg_sent", "n1"));
    assert_eq!(client.message_count(), 3);
}

#[test]
fn an_empty_send_should_do_nothing() {
    let mut client = Client::online();
    assert!(client.command(send("   ", "n1")).is_empty());
}

#[test]
fn a_failed_send_should_mark_the_row_failed() {
    let mut client = Client::online();
    let fx = client.command(send("hi", "n1"));
    let id = request_for(&fx, &post("hi", "n1"));
    let error = ApiError::Http {
        status: 403,
        message: "missing permission".into(),
    };
    let fx = client.fail(id, error);
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Update { row, roles, .. }]
            if row.failed && !row.pending && roles.contains(&MessageRole::Failed)
    ));
}

#[test]
fn retry_should_post_again_with_the_same_nonce() {
    let mut client = Client::online();
    let fx = client.command(send("hi", "n1"));
    client.fail(
        request_for(&fx, &post("hi", "n1")),
        ApiError::Network("offline".into()),
    );
    let fx = client.command(Command::RetrySend("n1".into()));
    request_for(&fx, &post("hi", "n1"));
}

#[test]
fn discarding_a_failed_send_should_remove_its_row() {
    let mut client = Client::online();
    let fx = client.command(send("hi", "n1"));
    client.fail(
        request_for(&fx, &post("hi", "n1")),
        ApiError::Network("offline".into()),
    );
    let fx = client.command(Command::DiscardSend("n1".into()));
    assert_eq!(
        message_diffs(&fx),
        vec![&ViewDiff::Remove { at: 2, count: 1 }]
    );
}

#[test]
fn pending_rows_should_not_reach_the_cache() {
    let mut client = Client::online();
    let fx = client.command(send("hi", "n1"));
    let cached_pending = cache_writes(&fx).iter().any(|w| {
        matches!(w, CacheWrite::Messages { messages, .. } if messages.iter().any(|m| m.id.starts_with("pending-")))
    });
    assert!(!cached_pending);
}

// Unauthorized

#[test]
fn unauthorized_anywhere_should_sign_out() {
    type Step = fn(&mut Client) -> RequestId;
    let steps: [(&str, Step); 3] = [
        ("send", |c| requests(&c.command(send("hi", "n1")))[0].0),
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
            op(GatewayOp::ServerSubscribe),
            op(GatewayOp::ChannelSubscribe)
        ]
    );
    assert!(client.state.connected());
}

#[test]
fn ready_after_a_reconnect_should_refetch_and_resubscribe_without_voice_presence() {
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
                op(GatewayOp::ServerSubscribe),
                json!({ "server_id": "srv_fixture_two" })
            ),
            (
                op(GatewayOp::ChannelSubscribe),
                json!({ "channel_id": TEXT })
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
        muted: true,
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

fn voice_frames(fx: &[Effect]) -> Vec<Value> {
    frames(fx)
        .into_iter()
        .filter(|(code, _)| *code == op(GatewayOp::VoiceStateUpdate))
        .map(|(_, d)| d)
        .collect()
}

fn prefs(client: &Client) -> (bool, bool) {
    let voice = client.state.voice_prefs();
    (voice.muted, voice.deafened)
}

#[test]
fn connecting_should_not_send_voice_state_on_the_gateway() {
    // Mute and deafen go on the room socket only (realtime/VOICE_ROSTER.md).
    let mut client = Client::online();
    client.command(Command::SetVoiceMuted(true));
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
    assert!(voice_frames(&fx).is_empty());
}

#[test]
fn toggles_in_a_joined_call_should_stay_off_the_gateway() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
    let fx = client.command(Command::SetVoiceDeafened(true));
    assert!(voice_frames(&fx).is_empty());
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetDeafened(true)
    }));
}

#[test]
fn a_fresh_room_session_should_announce_presence_again() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Rejoined,
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
fn watching_a_share_should_reach_the_call() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::WatchStream {
        participant_id: "p_ada".into(),
        watch: true,
    });
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetWatched(vec!["p_ada".into()])
    }));
    assert!(client
        .command(Command::WatchStream {
            participant_id: "p_ada".into(),
            watch: true,
        })
        .is_empty());
}

#[test]
fn deafen_should_mute_and_undeafen_should_restore_the_mic() {
    let mut client = Client::online();
    client.command(Command::SetVoiceDeafened(true));
    assert_eq!(prefs(&client), (true, true));
    client.command(Command::SetVoiceDeafened(false));
    assert_eq!(prefs(&client), (false, false));
    // Muted before deafening stays muted after.
    client.command(Command::SetVoiceMuted(true));
    client.command(Command::SetVoiceDeafened(true));
    client.command(Command::SetVoiceDeafened(false));
    assert_eq!(prefs(&client), (true, false));
}

#[test]
fn unmuting_while_deafened_should_undeafen() {
    let mut client = Client::online();
    client.command(Command::SetVoiceDeafened(true));
    client.command(Command::SetVoiceMuted(false));
    assert_eq!(prefs(&client), (false, false));
}

#[test]
fn deafen_in_a_call_should_mute_the_session_too() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::SetVoiceDeafened(true));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetMuted(true),
    }));
}

#[test]
fn voice_prefs_should_be_stored_and_restored_from_the_snapshot() {
    let mut client = Client::online();
    let fx = client.command(Command::SetVoiceDeafened(true));
    let stored = VoicePrefs {
        muted: true,
        deafened: true,
        muted_before_deafen: false,
    };
    assert!(cache_writes(&fx).contains(&&CacheWrite::Voice(stored)));
    let mut cached = snapshot();
    cached.voice = Some(stored);
    let (client, fx) = Client::launch(Some(cached));
    assert_eq!(prefs(&client), (true, true));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView {
            muted: true,
            deafened: true,
            ..
        }))
    )));
}

#[test]
fn our_call_seat_should_show_before_the_gateway_lists_us() {
    let mut client = Client::online();
    client.gateway(GatewayEvent::Dispatch(voice_states(json!({
        VOICE: [occupant("user_ada", "Ada")],
    }))));
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false)]),
    });
    let rows = members_of(&fx);
    let names: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.me)).collect();
    // Ada's row only relights (the path now runs past her to us).
    assert_eq!(names, [("Ada", false), ("P_ME", true)]);
    assert_eq!(rows[1].id, format!("{VOICE}/user_fixture_me"));
    let tree: Vec<_> = client
        .state
        .occupants(VOICE)
        .iter()
        .map(|o| o.name.as_str())
        .collect();
    assert_eq!(tree, ["Ada"]);
}

#[test]
fn a_failed_call_should_say_why_on_the_voice_view_until_the_next_join() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Ended(Some("call admission: 503".into())),
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView { error, .. })) if error == "Couldn't join the call. Try again."
    )));
    // A network failure and a refusal get their own plain lines.
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Ended(Some(
            "call admission: error sending request for url (https://x/api/voice/admission)".into(),
        )),
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView { error, .. }))
            if error == "Couldn't reach Yap. Check your connection and try again."
    )));
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Ended(Some("call admission: HTTP 403 CONNECT_DENIED".into())),
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView { error, .. })) if error == "You can't join this channel."
    )));
    let fx = client.command(Command::JoinVoice(VOICE.into()));
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView { error, .. })) if error.is_empty()
    )));
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
        frames(&fx)[0],
        (
            op(GatewayOp::VoiceChannelJoin),
            json!({ "channel_id": VOICE, "self_mute": false })
        )
    );
    assert_eq!(frames(&fx).len(), 1, "one op 33, no op 15");
}

#[test]
fn leaving_before_the_room_answered_should_send_no_presence_leave() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::LeaveVoice);
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::Leave
    }));
    assert!(frames(&fx).is_empty());
    assert!(!fx
        .iter()
        .any(|e| matches!(e, Effect::View(ViewUpdate::Sound(_)))));
}

#[test]
fn leaving_voice_should_stop_the_call_and_clear_presence() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
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
        speaking: false,
        error: String::new(),
        ..VoiceView::default()
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
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Connected,
    });
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
        before: None,
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
        ViewUpdate::Messages(Box::new(ViewDiff::Reset(Vec::new()))),
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
    let ApiResponse::Messages(Page { messages, .. }) = messages()
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
        voice: None,
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
        vec![
            ApiRequest::Me,
            ApiRequest::Servers,
            ApiRequest::Members {
                server_id: SERVER.into()
            },
            channels(),
            messages(),
            ApiRequest::ReadStates,
            ApiRequest::Notifications,
            ApiRequest::Dms,
            ApiRequest::Friends(yap_core::store::FriendsRequest::List),
        ]
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
    let refetched: Vec<_> = requests(&fx)
        .into_iter()
        // Unread state is new each session; other servers' trees feed their pills.
        .filter(|(_, r)| match r {
            ApiRequest::ReadStates | ApiRequest::Notifications => false,
            ApiRequest::Channels { server_id } => server_id == SERVER,
            _ => true,
        })
        .collect();
    assert!(refetched.is_empty(), "{refetched:#?}");
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

// Messaging: read state, paging, replies, edit and delete, typing, mentions

const A0: &str = "2026-11-04T12:00:00.000Z";
const A1: &str = "2026-11-04T12:10:00.000Z";
const A2: &str = "2026-11-04T12:20:00.000Z";
const A3: &str = "2026-11-04T12:30:00.000Z";

/// A message in `channel`, by `author`, at `at`.
fn msg(channel: &str, id: &str, author: &str, at: &str) -> Value {
    let mut data = message_in(channel, id);
    data["author_id"] = json!(author);
    data["author"] = json!({ "id": author, "username": author });
    data["created_at"] = json!(at);
    data["content"] = json!(format!("message {id}"));
    data
}

/// A page, oldest first, as the server sends it.
fn page_of(messages: Vec<Value>, has_more_before: bool) -> Value {
    json!({ "messages": messages, "hasMoreBefore": has_more_before, "hasMoreAfter": false })
}

fn two_message_page() -> Value {
    page_of(
        vec![
            msg(TEXT, "m1", "user_fixture_alice", A0),
            msg(TEXT, "m2", "user_fixture_bob", A2),
        ],
        true,
    )
}

fn read_states(reads: &[(&str, &str)], lasts: &[(&str, &str)]) -> Value {
    let reads: Vec<Value> = reads
        .iter()
        .map(|(c, at)| json!({ "channel_id": c, "last_read_at": at }))
        .collect();
    let lasts: Vec<Value> = lasts
        .iter()
        .map(|(c, at)| json!({ "channel_id": c, "last_message_at": at }))
        .collect();
    json!({ "read_states": reads, "last_messages": lasts })
}

fn mention(id: &str, channel: &str, message: &str) -> Value {
    json!({
        "id": id, "type": "mention", "channel_id": channel, "server_id": SERVER,
        "message_id": message, "is_read": false, "created_at": A3,
        "from_user": { "id": "user_fixture_alice", "username": "alice" },
    })
}

/// The last state of a tree row in these effects.
fn tree_row(fx: &[Effect], id: &str) -> Option<ChannelTreeRow> {
    tree_diffs(fx).into_iter().rev().find_map(|d| match d {
        ViewDiff::Update { row, .. } if row.id == id => Some(row.clone()),
        ViewDiff::Reset(rows) | ViewDiff::Insert { rows, .. } => {
            rows.iter().find(|r| r.id == id).cloned()
        }
        _ => None,
    })
}

fn server_rows(fx: &[Effect]) -> Vec<ServerRow> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::Servers(ViewDiff::Update { row, .. }) => Some(row.clone()),
            _ => None,
        })
        .collect()
}

fn unread_views(fx: &[Effect]) -> Vec<UnreadView> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::Unread(view) => Some(view.clone()),
            _ => None,
        })
        .collect()
}

fn notices(fx: &[Effect]) -> usize {
    views(fx)
        .into_iter()
        .filter(|v| matches!(v, ViewUpdate::Notice(_)))
        .count()
}

fn mark_read(channel: &str) -> ApiRequest {
    ApiRequest::MarkRead {
        channel_id: channel.into(),
    }
}

fn no_notifications() -> Value {
    json!({ "notifications": [], "unread_count": 0 })
}

impl Client {
    /// Sign-in where read states and notifications answer before the first
    /// page, as they usually do. Returns every effect along the way.
    fn boot_with(reads: Value, notifications: Value, page: Value) -> (Self, Vec<Effect>) {
        Self::boot_in(channels_with_two_text(), reads, notifications, page)
    }

    /// [`Self::boot_with`] with this channel tree.
    fn boot_in(
        tree: Value,
        reads: Value,
        notifications: Value,
        page: Value,
    ) -> (Self, Vec<Effect>) {
        let mut client = Self::new();
        let mut all = client.command(Command::UseToken("tok_fixture".into()));
        let me = request_for(&all, &ApiRequest::Me);
        let servers = request_for(&all, &ApiRequest::Servers);
        let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
        let rs = request_for(&fx, &ApiRequest::ReadStates);
        let notes = request_for(&fx, &ApiRequest::Notifications);
        all.extend(fx);
        all.extend(client.respond(rs, &ApiRequest::ReadStates, reads));
        all.extend(client.respond(notes, &ApiRequest::Notifications, notifications));
        let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
        all.extend(fx.clone());
        let fx = client.respond(request_for(&fx, &channels()), &channels(), tree);
        all.extend(fx.clone());
        all.extend(client.respond(request_for(&fx, &messages()), &messages(), page));
        (client, all)
    }

    fn quiet(page: Value) -> (Self, Vec<Effect>) {
        Self::boot_with(read_states(&[], &[]), no_notifications(), page)
    }

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.gateway(GatewayEvent::Dispatch(dispatch))
    }
}

#[test]
fn sign_in_should_ask_for_read_states_and_notifications() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let me = request_for(&fx, &ApiRequest::Me);
    let fx = client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    request_for(&fx, &ApiRequest::ReadStates);
    request_for(&fx, &ApiRequest::Notifications);
    assert_eq!(
        ApiRequest::Notifications.path(),
        "/api/notifications?limit=100&unread=true"
    );
}

#[test]
fn a_channel_with_newer_messages_should_be_unread() {
    let reads = read_states(&[(SECOND_TEXT, A0)], &[(SECOND_TEXT, A1)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(client.state.is_unread(SECOND_TEXT));
}

#[test]
fn a_never_read_channel_with_messages_should_be_unread() {
    let reads = read_states(&[], &[(SECOND_TEXT, A1)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(client.state.is_unread(SECOND_TEXT));
}

#[test]
fn the_open_channel_should_never_show_unread() {
    let reads = read_states(&[(TEXT, A0)], &[(TEXT, A2)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(!client.state.is_unread(TEXT));
}

#[test]
fn unread_should_reach_the_tree_row() {
    let reads = read_states(&[(SECOND_TEXT, A0)], &[(SECOND_TEXT, A1)]);
    let (_, fx) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(tree_row(&fx, SECOND_TEXT).is_some_and(|r| r.unread));
}

#[test]
fn read_state_update_should_clear_unread() {
    let reads = read_states(&[(SECOND_TEXT, A0)], &[(SECOND_TEXT, A1)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let fx = client.dispatch(
        "READ_STATE_UPDATE",
        json!({ "channel_id": SECOND_TEXT, "last_read_at": A2 }),
    );
    assert!(tree_row(&fx, SECOND_TEXT).is_some_and(|r| !r.unread));
}

#[test]
fn a_message_in_another_channel_should_make_it_unread() {
    let (mut client, _) = Client::quiet(two_message_page());
    client.dispatch(
        "MESSAGE_CREATE",
        msg(SECOND_TEXT, "x1", "user_fixture_alice", A3),
    );
    assert!(client.state.is_unread(SECOND_TEXT));
}

#[test]
fn a_message_in_the_open_channel_should_mark_it_read() {
    let mut client = Client::online();
    let fx = client.dispatch("MESSAGE_CREATE", msg(TEXT, "x1", "user_fixture_alice", A3));
    request_for(&fx, &mark_read(TEXT));
    assert_eq!(client.state.read_at(TEXT), Some(A3));
}

#[test]
fn mark_read_should_put_an_empty_body() {
    assert_eq!(
        (mark_read(TEXT).method(), mark_read(TEXT).body()),
        (Method::Put, json!({}))
    );
}

#[test]
fn reaching_the_end_focused_should_mark_the_open_channel_read() {
    let reads = read_states(&[(TEXT, A0)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let fx = client.command(Command::SetAtBottom(true));
    request_for(&fx, &mark_read(TEXT));
}

#[test]
fn reaching_the_end_unfocused_should_not_mark_read() {
    let reads = read_states(&[(TEXT, A0)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetFocused(false));
    let fx = client.command(Command::SetAtBottom(true));
    assert!(requests(&fx).is_empty(), "{fx:#?}");
}

#[test]
fn focusing_at_the_end_should_mark_read() {
    let reads = read_states(&[(TEXT, A0)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetFocused(false));
    client.command(Command::SetAtBottom(true));
    let fx = client.command(Command::SetFocused(true));
    request_for(&fx, &mark_read(TEXT));
}

#[test]
fn an_already_read_channel_should_not_be_marked_again() {
    let reads = read_states(&[(TEXT, A3)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let fx = client.command(Command::SetAtBottom(true));
    assert!(requests(&fx).is_empty(), "{fx:#?}");
}

#[test]
fn a_failed_mark_should_roll_back() {
    let reads = read_states(&[(TEXT, A0)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let fx = client.command(Command::SetAtBottom(true));
    client.fail(
        request_for(&fx, &mark_read(TEXT)),
        ApiError::Network("offline".into()),
    );
    assert_eq!(client.state.read_at(TEXT), Some(A0));
}

#[test]
fn opening_with_unread_should_put_the_divider_above_the_first_new_message() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let divider = client.state.divider().expect("divider");
    assert_eq!((divider.message_id.as_str(), divider.count), ("m2", 1));
}

fn page_rows(fx: &[Effect]) -> Vec<MessageRow> {
    match message_diffs(fx).last() {
        Some(ViewDiff::Reset(rows)) => rows.clone(),
        other => panic!("{other:#?}"),
    }
}

#[test]
fn the_divider_row_should_carry_the_flag() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (_, fx) = Client::boot_with(reads, no_notifications(), two_message_page());
    let flagged: Vec<String> = page_rows(&fx)
        .into_iter()
        .filter(|r| r.divider)
        .map(|r| r.id)
        .collect();
    assert_eq!(flagged, vec!["m2"]);
}

#[test]
fn opening_with_unread_should_show_the_banner() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (_, fx) = Client::boot_with(reads, no_notifications(), two_message_page());
    let banner = unread_views(&fx).pop().expect("unread view");
    assert!(banner.banner && banner.count == 1 && banner.since_time == "12:20 PM");
}

#[test]
fn opening_without_unread_should_have_no_divider() {
    let reads = read_states(&[(TEXT, A2)], &[(TEXT, A2)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(client.state.divider().is_none());
}

#[test]
fn reaching_the_end_should_drop_the_banner_but_keep_the_divider() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetAtBottom(true));
    let divider = client.state.divider().expect("divider");
    assert!(!divider.banner && !divider.locked);
}

#[test]
fn returning_to_the_window_at_the_end_should_lock_the_divider() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetAtBottom(true));
    client.command(Command::SetFocused(false));
    client.command(Command::SetFocused(true));
    assert!(client.state.divider().expect("divider").locked);
}

#[test]
fn messages_away_after_reaching_the_end_focused_should_extend_the_divider() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetAtBottom(true));
    client.command(Command::SetFocused(false));
    client.dispatch("MESSAGE_CREATE", msg(TEXT, "m3", "user_fixture_alice", A3));
    let divider = client.state.divider().expect("divider");
    assert_eq!(
        (divider.message_id.as_str(), divider.count, divider.banner),
        ("m2", 2, true)
    );
}

#[test]
fn mark_as_read_should_drop_the_divider() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    let fx = client.command(Command::MarkRead);
    request_for(&fx, &mark_read(TEXT));
    assert!(client.state.divider().is_none());
}

#[test]
fn a_message_while_away_should_move_a_seen_divider_to_it() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetAtBottom(true));
    client.command(Command::SetFocused(false));
    client.command(Command::SetFocused(true));
    client.command(Command::SetFocused(false));
    client.dispatch("MESSAGE_CREATE", msg(TEXT, "m3", "user_fixture_alice", A3));
    let divider = client.state.divider().expect("divider");
    assert_eq!((divider.message_id.as_str(), divider.banner), ("m3", true));
}

#[test]
fn messages_while_away_should_add_to_an_unseen_divider() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let (mut client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    client.command(Command::SetFocused(false));
    client.dispatch("MESSAGE_CREATE", msg(TEXT, "m3", "user_fixture_alice", A3));
    let divider = client.state.divider().expect("divider");
    assert_eq!((divider.message_id.as_str(), divider.count), ("m2", 2));
}

#[test]
fn mention_notifications_should_count_on_the_channel_and_server() {
    let notes = json!({ "notifications": [
        mention("n1", SECOND_TEXT, "x1"),
        mention("n2", SECOND_TEXT, "x2"),
    ] });
    let (client, _) = Client::boot_with(read_states(&[], &[]), notes, two_message_page());
    assert_eq!(
        (
            client.state.channel_mentions(SECOND_TEXT),
            client.state.server_mentions(SERVER)
        ),
        (2, 2)
    );
}

#[test]
fn a_live_mention_should_update_the_server_badge() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.dispatch("NOTIFICATION_CREATE", mention("n1", SECOND_TEXT, "x1"));
    assert!(server_rows(&fx)
        .iter()
        .any(|r| r.id == SERVER && r.mentions == 1));
}

#[test]
fn notifications_read_should_clear_the_count() {
    let notes = json!({ "notifications": [mention("n1", SECOND_TEXT, "x1")] });
    let (mut client, _) = Client::boot_with(read_states(&[], &[]), notes, two_message_page());
    client.dispatch("NOTIFICATIONS_READ", json!({ "ids": ["n1"], "all": false }));
    assert_eq!(client.state.channel_mentions(SECOND_TEXT), 0);
}

#[test]
fn seen_mentions_in_the_open_channel_should_be_marked_read() {
    let notes = json!({ "notifications": [mention("n1", TEXT, "m2")] });
    let (mut client, _) = Client::boot_with(read_states(&[], &[]), notes, two_message_page());
    client.command(Command::SeenFrom("m1".into()));
    let fx = client.command(Command::SetAtBottom(true));
    request_for(
        &fx,
        &ApiRequest::MarkNotificationsRead {
            ids: vec!["n1".into()],
        },
    );
}

// Paging

#[test]
fn load_older_should_ask_for_the_page_before_the_first_message() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    let older = ApiRequest::Messages {
        channel_id: TEXT.into(),
        before: Some(format!("{A0}|m1")),
    };
    request_for(&fx, &older);
    assert_eq!(
        older.path(),
        "/api/channels/chan_fixture_text/messages?limit=50&before=2026-11-04T12%3A00%3A00.000Z%7Cm1"
    );
}

#[test]
fn load_older_should_ask_once_while_in_flight() {
    let (mut client, _) = Client::quiet(two_message_page());
    client.command(Command::LoadOlder);
    assert!(requests(&client.command(Command::LoadOlder)).is_empty());
}

#[test]
fn load_older_should_stop_at_the_start_of_the_channel() {
    let page = page_of(vec![msg(TEXT, "m1", "user_fixture_alice", A0)], false);
    let (mut client, _) = Client::quiet(page);
    assert!(requests(&client.command(Command::LoadOlder)).is_empty());
}

#[test]
fn an_older_page_should_insert_at_the_top_without_a_reset() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    let (id, request) = requests(&fx).pop().expect("older");
    let older = page_of(
        vec![
            msg(TEXT, "o1", "user_fixture_bob", "2026-11-04T11:00:00.000Z"),
            msg(TEXT, "o2", "user_fixture_alice", "2026-11-04T11:59:00.000Z"),
        ],
        false,
    );
    let fx = client.respond(id, &request, older);
    match message_diffs(&fx).as_slice() {
        [ViewDiff::Insert { at: 0, rows }, ViewDiff::Update { at: 2, roles, .. }] => {
            assert!(rows.len() == 2 && roles.contains(&MessageRole::Grouped));
        }
        other => panic!("{other:#?}"),
    }
}

#[test]
fn an_older_page_should_end_paging_when_the_server_says_so() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    let (id, request) = requests(&fx).pop().expect("older");
    client.respond(id, &request, page_of(Vec::new(), false));
    assert!(requests(&client.command(Command::LoadOlder)).is_empty());
}

#[test]
fn a_refetch_should_keep_the_older_pages() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    let (id, request) = requests(&fx).pop().expect("older");
    let older = page_of(
        vec![msg(
            TEXT,
            "o1",
            "user_fixture_bob",
            "2026-11-04T11:00:00.000Z",
        )],
        true,
    );
    client.respond(id, &request, older);
    client.gateway(GatewayEvent::Ready);
    client.gateway(GatewayEvent::Closed);
    let fx = client.gateway(GatewayEvent::Ready);
    client.respond(
        request_for(&fx, &messages()),
        &messages(),
        two_message_page(),
    );
    assert_eq!(client.message_count(), 3);
}

// Replies

fn reply_page() -> Value {
    let mut reply = msg(TEXT, "r1", "user_fixture_alice", A2);
    reply["reply_to_id"] = json!("m1");
    reply["reply_to"] = json!({
        "id": "m1", "content": "first\nline", "author_id": ME,
        "author": { "id": ME, "username": "me", "display_name": "Fixture Me" },
        "attachment_count": 0,
    });
    let mut orphan = msg(TEXT, "r2", "user_fixture_bob", A3);
    orphan["reply_to_id"] = json!("gone");
    page_of(vec![msg(TEXT, "m1", ME, A0), reply, orphan], false)
}

#[test]
fn a_reply_row_should_quote_its_parent_on_one_line() {
    let (_, fx) = Client::quiet(reply_page());
    let r = page_rows(&fx).swap_remove(1);
    assert_eq!(
        (r.reply_id, r.reply_author, r.reply_text),
        ("m1".into(), "Fixture Me".into(), "first line".into())
    );
}

#[test]
fn a_reply_to_a_deleted_message_should_have_no_quote() {
    let (_, fx) = Client::quiet(reply_page());
    assert!(page_rows(&fx)[2].reply_id.is_empty());
}

#[test]
fn a_reply_to_me_should_be_highlighted() {
    let (_, fx) = Client::quiet(reply_page());
    assert!(page_rows(&fx)[1].mentioned);
}

#[test]
fn an_attachment_only_parent_should_read_as_an_attachment() {
    let mut page = reply_page();
    page["messages"][1]["reply_to"]["content"] = json!("");
    page["messages"][1]["reply_to"]["attachment_count"] = json!(1);
    let (_, fx) = Client::quiet(page);
    let r = page_rows(&fx).swap_remove(1);
    assert!(r.reply_placeholder && r.reply_text == "Click to see attachment");
}

#[test]
fn a_reply_send_should_quote_the_parent_at_once() {
    let (mut client, _) = Client::quiet(reply_page());
    let fx = client.command(Command::Send {
        content: "yes".into(),
        reply_to_id: Some("m1".into()),
        nonce: "n1".into(),
        sent_at: A3.into(),
    });
    // Then the parent's reply count moves (an update of its row).
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Insert { rows, .. }, ..] if rows[0].reply_id == "m1"
    ));
}

#[test]
fn a_mention_of_my_username_should_highlight_the_row() {
    let mut mentioning = msg(TEXT, "m9", "user_fixture_alice", A3);
    mentioning["content"] = json!("ping @Me please");
    let (_, fx) = Client::quiet(page_of(vec![mentioning], false));
    assert!(page_rows(&fx)[0].mentioned);
}

#[test]
fn my_own_mention_of_everyone_should_not_highlight_my_row() {
    let mut mine = msg(TEXT, "m9", ME, A3);
    mine["content"] = json!("@everyone dinner");
    let (_, fx) = Client::quiet(page_of(vec![mine], false));
    assert!(!page_rows(&fx)[0].mentioned);
}

// Edit and delete

fn own_page() -> Value {
    page_of(
        vec![
            msg(TEXT, "m1", ME, A0),
            msg(TEXT, "m2", "user_fixture_bob", A2),
        ],
        false,
    )
}

fn edit(id: &str, content: &str) -> ApiRequest {
    ApiRequest::EditMessage {
        channel_id: TEXT.into(),
        message_id: id.into(),
        content: content.into(),
    }
}

fn edit_command(id: &str, content: &str) -> Command {
    Command::EditMessage {
        message_id: id.into(),
        content: content.into(),
    }
}

#[test]
fn edit_should_patch_with_the_message_id_in_the_body() {
    let (mut client, _) = Client::quiet(own_page());
    let fx = client.command(edit_command("m1", " fixed "));
    request_for(&fx, &edit("m1", "fixed"));
    assert_eq!(
        (edit("m1", "fixed").method(), edit("m1", "fixed").body()),
        (
            Method::Patch,
            json!({ "message_id": "m1", "content": "fixed" })
        )
    );
}

#[test]
fn edit_should_show_the_new_text_at_once() {
    let (mut client, _) = Client::quiet(own_page());
    let fx = client.command(edit_command("m1", "fixed"));
    assert!(matches!(
        message_diffs(&fx)[..],
        [ViewDiff::Update { at: 0, row, .. }] if row.body == "fixed" && row.edited
    ));
}

#[test]
fn a_failed_edit_should_restore_the_text_and_say_so() {
    let (mut client, _) = Client::quiet(own_page());
    let fx = client.command(edit_command("m1", "fixed"));
    let fx = client.fail(
        request_for(&fx, &edit("m1", "fixed")),
        ApiError::Network("offline".into()),
    );
    let restored = client
        .state
        .messages()
        .and_then(|l| l.get("m1"))
        .map(|m| m.content.clone());
    assert_eq!((restored.as_deref(), notices(&fx)), (Some("message m1"), 1));
}

#[test]
fn editing_someone_elses_message_should_do_nothing() {
    let (mut client, _) = Client::quiet(own_page());
    let fx = client.command(edit_command("m2", "mine now"));
    assert!(fx.is_empty(), "{fx:#?}");
}

fn delete(id: &str) -> ApiRequest {
    ApiRequest::DeleteMessage {
        channel_id: TEXT.into(),
        message_id: id.into(),
    }
}

#[test]
fn delete_should_remove_the_row_and_send_the_id_in_the_body() {
    let (mut client, _) = Client::quiet(own_page());
    let fx = client.command(Command::DeleteMessage("m1".into()));
    request_for(&fx, &delete("m1"));
    assert_eq!(
        (message_diffs(&fx)[0].clone(), delete("m1").body()),
        (
            ViewDiff::Remove { at: 0, count: 1 },
            json!({ "message_id": "m1" })
        )
    );
}

#[test]
fn a_failed_delete_should_put_the_row_back() {
    let (mut client, boot) = Client::quiet(own_page());
    let mut rows = page_rows(&boot);
    let fx = client.command(Command::DeleteMessage("m1".into()));
    replay_messages(&mut rows, &fx);
    let fx = client.fail(
        request_for(&fx, &delete("m1")),
        ApiError::Network("offline".into()),
    );
    replay_messages(&mut rows, &fx);
    let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["m1", "m2"]);
}

// Typing

#[test]
fn typing_should_post_to_the_open_channel() {
    let mut client = Client::online();
    let fx = client.command(Command::Typing);
    request_for(
        &fx,
        &ApiRequest::Typing {
            channel_id: TEXT.into(),
        },
    );
}

fn typing(user: &str) -> Value {
    json!({ "channel_id": TEXT, "user_id": user })
}

fn expiry(fx: &[Effect]) -> Input {
    fx.iter()
        .find_map(|e| match e {
            Effect::Later { input, .. } => Some((**input).clone()),
            _ => None,
        })
        .expect("expiry")
}

#[test]
fn typing_should_expire_per_person() {
    let mut client = Client::online();
    let alice = expiry(&client.dispatch("TYPING_START", typing("user_fixture_alice")));
    client.dispatch("TYPING_START", typing("user_fixture_bob"));
    client.input(alice);
    let left: Vec<&str> = client
        .state
        .typing()
        .iter()
        .map(|t| t.user_id.as_str())
        .collect();
    assert_eq!(left, vec!["user_fixture_bob"]);
}

#[test]
fn an_older_expiry_should_not_clear_a_fresh_typer() {
    let mut client = Client::online();
    let first = expiry(&client.dispatch("TYPING_START", typing("user_fixture_alice")));
    client.dispatch("TYPING_START", typing("user_fixture_alice"));
    client.input(first);
    assert_eq!(client.state.typing().len(), 1);
}

#[test]
fn typing_line_should_name_up_to_three() {
    assert_eq!(
        typing_text(&["Alice", "Bob", "Cara"]),
        "Alice, Bob, Cara are typing…"
    );
}

#[test]
fn a_message_should_take_its_author_off_the_typing_line() {
    let mut client = Client::online();
    client.dispatch("TYPING_START", typing("user_fixture_alice"));
    client.dispatch("MESSAGE_CREATE", msg(TEXT, "m9", "user_fixture_alice", A3));
    assert!(client.state.typing().is_empty());
}

// Mention suggestions

#[test]
fn mention_query_should_list_matching_members() {
    let mut client = Client::new();
    let fx = client.command(Command::UseToken("tok_fixture".into()));
    let me = request_for(&fx, &ApiRequest::Me);
    let servers = request_for(&fx, &ApiRequest::Servers);
    client.respond(me, &ApiRequest::Me, fixture("user-me.json"));
    let fx = client.respond(servers, &ApiRequest::Servers, fixture("servers-list.json"));
    let members = ApiRequest::Members {
        server_id: SERVER.into(),
    };
    let body = json!([
        { "user": { "id": "u1", "username": "alice", "display_name": "Alice" } },
        { "user": { "id": "u2", "username": "alicia" } },
        { "user": { "id": "u3", "username": "bob" } },
    ]);
    client.respond(request_for(&fx, &members), &members, body);
    let fx = client.command(Command::MentionQuery(Some("ALI".into())));
    let names: Vec<String> = views(&fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::Mentions(ViewDiff::Insert { rows, .. }) => Some(rows.clone()),
            _ => None,
        })
        .flatten()
        .map(|r| r.username)
        .collect();
    assert_eq!(names, vec!["alice", "alicia"]);
}

// Review fixes: cached boot unread, paging state, seen mentions, delete
// permissions, timestamps

const FIXTURE_NEWEST: &str = "2026-11-04T12:00:05.000Z";

fn marks(fx: &[Effect]) -> usize {
    requests(fx)
        .iter()
        .filter(|(_, r)| matches!(r, ApiRequest::MarkRead { .. }))
        .count()
}

fn relands(fx: &[Effect]) -> usize {
    views(fx)
        .into_iter()
        .filter(|v| matches!(v, ViewUpdate::Reland))
        .count()
}

/// The fixture page's newer message is unread.
fn fixture_unread() -> Value {
    read_states(
        &[(TEXT, "2026-11-04T12:00:02.000Z")],
        &[(TEXT, FIXTURE_NEWEST)],
    )
}

#[test]
fn cached_boot_should_not_mark_read_before_the_read_states_answer() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    // The cached rows landed at the end of a focused window.
    let mut fx = client.command(Command::SetAtBottom(true));
    fx.extend(client.respond(
        request_for(&boot, &messages()),
        &messages(),
        fixture("messages-page.json"),
    ));
    assert_eq!(marks(&fx), 0, "{fx:#?}");
}

#[test]
fn cached_boot_read_states_after_the_page_should_place_the_divider_and_land_again() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    client.command(Command::SetAtBottom(true));
    client.respond(
        request_for(&boot, &messages()),
        &messages(),
        fixture("messages-page.json"),
    );
    let fx = client.respond(
        request_for(&boot, &ApiRequest::ReadStates),
        &ApiRequest::ReadStates,
        fixture_unread(),
    );
    let divider = client.state.divider().expect("divider");
    assert_eq!(
        (divider.message_id.as_str(), relands(&fx), marks(&fx)),
        ("msg_fixture_0002", 1, 0)
    );
    let row = message_diffs(&fx).into_iter().find_map(|d| match d {
        ViewDiff::Update { row, .. } if row.divider => Some(row.id.clone()),
        _ => None,
    });
    assert_eq!(row.as_deref(), Some("msg_fixture_0002"));
    // Once the list reports the end again, it is read.
    assert_eq!(marks(&client.command(Command::SetAtBottom(true))), 1);
}

#[test]
fn cached_boot_read_states_before_the_page_should_place_the_divider_in_the_cached_rows() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    client.command(Command::SetAtBottom(true));
    let fx = client.respond(
        request_for(&boot, &ApiRequest::ReadStates),
        &ApiRequest::ReadStates,
        fixture_unread(),
    );
    assert_eq!((relands(&fx), marks(&fx)), (1, 0));
    let fx = client.respond(
        request_for(&boot, &messages()),
        &messages(),
        fixture("messages-page.json"),
    );
    let divider = client.state.divider().expect("divider");
    assert_eq!(
        (divider.message_id.as_str(), relands(&fx), marks(&fx)),
        ("msg_fixture_0002", 0, 0)
    );
}

#[test]
fn a_cached_session_should_not_ask_for_read_states_again_when_me_answers() {
    let (mut client, boot) = Client::launch(Some(snapshot()));
    let fx = client.respond(
        request_for(&boot, &ApiRequest::Me),
        &ApiRequest::Me,
        fixture("user-me.json"),
    );
    let asked = requests(&fx)
        .iter()
        .any(|(_, r)| matches!(r, ApiRequest::ReadStates | ApiRequest::Notifications));
    assert!(!asked, "{fx:#?}");
}

fn history(fx: &[Effect]) -> Vec<(bool, bool)> {
    views(fx)
        .into_iter()
        .filter_map(|v| match v {
            ViewUpdate::History { loading, more } => Some((*loading, *more)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_failed_older_page_should_end_loading_and_allow_a_retry() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    assert_eq!(history(&fx), vec![(true, true)]);
    let (id, _) = requests(&fx).pop().expect("older");
    let fx = client.fail(id, ApiError::Network("offline".into()));
    assert_eq!(history(&fx), vec![(false, true)]);
    assert_eq!(requests(&client.command(Command::LoadOlder)).len(), 1);
}

#[test]
fn an_older_page_with_nothing_new_should_stop_paging() {
    let (mut client, _) = Client::quiet(two_message_page());
    let fx = client.command(Command::LoadOlder);
    let (id, request) = requests(&fx).pop().expect("older");
    // The server says there are more, but sent nothing new.
    let fx = client.respond(id, &request, page_of(Vec::new(), true));
    assert_eq!(history(&fx), vec![(false, false)]);
    assert!(requests(&client.command(Command::LoadOlder)).is_empty());
}

fn notifications_marked(fx: &[Effect]) -> Vec<String> {
    requests(fx)
        .into_iter()
        .find_map(|(_, r)| match r {
            ApiRequest::MarkNotificationsRead { ids } => Some(ids),
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn reaching_the_end_should_not_mark_mentions_above_what_was_on_screen() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let notes = json!({ "notifications": [mention("n1", TEXT, "m1"), mention("n2", TEXT, "m2")] });
    let (mut client, _) = Client::boot_with(reads, notes, two_message_page());
    client.command(Command::SeenFrom("m2".into()));
    let fx = client.command(Command::SetAtBottom(true));
    assert_eq!(notifications_marked(&fx), vec!["n2".to_owned()]);
    assert_eq!(client.state.channel_mentions(TEXT), 1);
}

#[test]
fn reaching_the_end_should_mark_mentions_from_the_top_row_on_screen() {
    let reads = read_states(&[(TEXT, A1)], &[(TEXT, A2)]);
    let notes = json!({ "notifications": [mention("n1", TEXT, "m1"), mention("n2", TEXT, "m2")] });
    let (mut client, _) = Client::boot_with(reads, notes, two_message_page());
    client.command(Command::SeenFrom("m1".into()));
    let fx = client.command(Command::SetAtBottom(true));
    assert_eq!(
        notifications_marked(&fx),
        vec!["n1".to_owned(), "n2".to_owned()]
    );
}

#[test]
fn a_live_mention_at_the_end_should_be_marked_read() {
    let (mut client, _) = Client::quiet(two_message_page());
    client.command(Command::SeenFrom("m2".into()));
    client.command(Command::SetAtBottom(true));
    client.dispatch("MESSAGE_CREATE", msg(TEXT, "m3", "user_fixture_alice", A3));
    let fx = client.dispatch("NOTIFICATION_CREATE", mention("n3", TEXT, "m3"));
    assert_eq!(notifications_marked(&fx), vec!["n3".to_owned()]);
}

/// The two-text-channel tree with no permissions on any channel.
fn tree_without_manage() -> Value {
    let mut tree = channels_with_two_text();
    for channel in tree["channels"].as_array_mut().expect("channels") {
        channel
            .as_object_mut()
            .expect("channel")
            .remove("permissions");
    }
    tree
}

#[test]
fn delete_should_skip_others_messages_without_manage_messages() {
    let (mut client, boot) = Client::boot_in(
        tree_without_manage(),
        read_states(&[], &[]),
        no_notifications(),
        own_page(),
    );
    assert!(views(&boot).contains(&&ViewUpdate::ManageMessages(false)));
    let fx = client.command(Command::DeleteMessage("m2".into()));
    assert!(fx.is_empty(), "{fx:#?}");
}

#[test]
fn delete_should_take_others_messages_with_manage_messages() {
    let (mut client, boot) = Client::quiet(own_page());
    assert!(views(&boot).contains(&&ViewUpdate::ManageMessages(true)));
    let fx = client.command(Command::DeleteMessage("m2".into()));
    request_for(&fx, &delete("m2"));
}

#[test]
fn administrator_should_be_able_to_delete_others_messages() {
    let mut tree = tree_without_manage();
    tree["channels"][0]["permissions"] = json!(1);
    let (mut client, _) =
        Client::boot_in(tree, read_states(&[], &[]), no_notifications(), own_page());
    let fx = client.command(Command::DeleteMessage("m2".into()));
    request_for(&fx, &delete("m2"));
}

#[test]
fn read_marks_with_a_space_separator_should_compare_as_instants() {
    // As strings, "2026-11-04T12:20…" sorts after "2026-11-04 12:25…".
    let reads = read_states(&[(TEXT, "2026-11-04 12:25:00")], &[(TEXT, A2)]);
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(client.state.divider().is_none());
}

#[test]
fn read_marks_with_an_offset_should_compare_as_instants() {
    // 12:25 UTC: the newest message, at 12:20 UTC, is read.
    let reads = read_states(
        &[(SECOND_TEXT, "2026-11-04T07:25:00-05:00")],
        &[(SECOND_TEXT, A2)],
    );
    let (client, _) = Client::boot_with(reads, no_notifications(), two_message_page());
    assert!(!client.state.is_unread(SECOND_TEXT));
}

#[test]
fn our_speaking_bit_should_reach_the_voice_view() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", true), member("p_ana", false)]),
    });
    assert!(fx.iter().any(|e| matches!(
        e,
        Effect::View(ViewUpdate::Voice(VoiceView { speaking: true, .. }))
    )));
}

fn tiles_of(fx: &[Effect]) -> Vec<yap_core::store::view::VoiceTileRow> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Tiles(ViewDiff::Insert { rows, .. })) => Some(rows.clone()),
            Effect::View(ViewUpdate::Tiles(ViewDiff::Reset(rows))) => Some(rows.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[test]
fn the_call_stage_should_get_a_tile_per_person_and_per_share() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let mut ana = member("p_ana", false);
    ana.user_id = "user_ana".into();
    ana.video = true;
    ana.camera_live = true;
    ana.stream = true;
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), ana]),
    });
    let tiles = tiles_of(&fx);
    let summary: Vec<_> = tiles
        .iter()
        .map(|t| (t.kind.as_str(), t.video, t.live, t.video_key.as_str()))
        .collect();
    assert_eq!(
        summary,
        [
            ("person", false, false, "local-camera"),
            ("person", true, true, "cam-video-p_ana"),
            ("screen", true, false, "screen-video-p_ana"),
        ]
    );
    assert!(!tiles[2].watching);
}

#[test]
fn watching_should_mark_the_share_tile() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let mut ana = member("p_ana", false);
    ana.user_id = "user_ana".into();
    ana.stream = true;
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false), ana]),
    });
    let fx = client.command(Command::WatchStream {
        participant_id: "p_ana".into(),
        watch: true,
    });
    assert!(views(&fx).iter().any(|v| matches!(
        v,
        ViewUpdate::Tiles(ViewDiff::Update { row, .. }) if row.kind == "screen" && row.watching
    )));
}

fn last_voice_view(fx: &[Effect]) -> VoiceView {
    fx.iter()
        .rev()
        .find_map(|e| match e {
            Effect::View(ViewUpdate::Voice(v)) => Some(v.clone()),
            _ => None,
        })
        .expect("a voice view")
}

#[test]
fn camera_toggle_should_reach_the_call_and_show_once_it_is_on() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::SetVoiceCamera(true));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetCamera(true),
    }));
    let view = last_voice_view(&fx);
    assert_eq!((view.camera_wanted, view.camera), (true, false));

    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::Camera {
            on: true,
            error: None,
        },
    });
    assert!(last_voice_view(&fx).camera);
}

#[test]
fn a_camera_capture_failure_should_turn_it_off_with_the_reason() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.command(Command::SetVoiceCamera(true));
    let fx = client.command(Command::VoiceCameraFailed("busy".into()));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::SetCamera(false),
    }));
    let view = last_voice_view(&fx);
    assert_eq!(
        (view.camera_wanted, view.camera_error.as_str()),
        (false, "busy")
    );
    // Trying again clears it.
    let fx = client.command(Command::SetVoiceCamera(true));
    assert_eq!(last_voice_view(&fx).camera_error, "");
}

fn share(audio: bool) -> yap_core::VoiceShare {
    yap_core::VoiceShare {
        source_id: "window-7".into(),
        source_name: "Yap Native Share Test".into(),
        window: true,
        quality: "1080p30".into(),
        audio,
    }
}

fn sounds(fx: &[Effect]) -> Vec<yap_core::shell::Sound> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Sound(s)) => Some(*s),
            _ => None,
        })
        .collect()
}

#[test]
fn go_live_should_reach_the_call_and_wait_for_it() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::StartVoiceShare(share(true)));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::StartShare(share(true)),
    }));
    let view = last_voice_view(&fx);
    assert_eq!((view.share_pending, view.sharing), (true, false));
}

#[test]
fn a_started_share_should_play_the_start_sound_and_show_our_stream_tile() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::Roster(vec![member("p_me", false)]),
    });
    client.command(Command::StartVoiceShare(share(true)));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::ShareStarted(share(true)),
    });
    assert_eq!(sounds(&fx), vec![yap_core::shell::Sound::ScreenShareStart]);
    let view = last_voice_view(&fx);
    assert!(view.sharing && !view.share_pending && view.share.audio);
    let tile = tiles_of(&fx)
        .into_iter()
        .find(|t| t.kind == "screen")
        .expect("our share tile");
    assert_eq!(
        (tile.me, tile.live, tile.video_key.as_str()),
        (true, true, "local-screen")
    );
}

#[test]
fn stopping_a_live_share_should_play_the_stop_sound_and_clear_it() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.command(Command::StartVoiceShare(share(false)));
    client.input(Input::Voice {
        call,
        update: VoiceUpdate::ShareStarted(share(false)),
    });
    let fx = client.command(Command::StopVoiceShare);
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::StopShare,
    }));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::ShareEnded {
            stop_sound: true,
            error: None,
        },
    });
    assert_eq!(sounds(&fx), vec![yap_core::shell::Sound::ScreenShareStop]);
    assert!(!last_voice_view(&fx).sharing);
}

#[test]
fn a_failed_share_should_keep_the_reason_until_the_next_one() {
    let mut client = Client::online();
    let call = voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    client.command(Command::StartVoiceShare(share(false)));
    let fx = client.input(Input::Voice {
        call,
        update: VoiceUpdate::ShareEnded {
            stop_sound: false,
            error: Some("gone".into()),
        },
    });
    let view = last_voice_view(&fx);
    assert_eq!(
        (view.share_pending, view.share_error.as_str()),
        (false, "gone")
    );
    let fx = client.command(Command::StartVoiceShare(share(false)));
    assert_eq!(last_voice_view(&fx).share_error, "");
}

#[test]
fn share_audio_outside_a_share_should_do_nothing() {
    let mut client = Client::online();
    voice_call(&client.command(Command::JoinVoice(VOICE.into())));
    let fx = client.command(Command::SetVoiceShareAudio(false));
    assert!(!fx.iter().any(|e| matches!(e, Effect::Voice { .. })));
}
