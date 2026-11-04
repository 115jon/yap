//! Reducer table tests driven by the shared fixtures in
//! `packages/protocol/fixtures`. No network, keyring, runtime or Qt: inputs
//! go into `reduce`, and the test asserts on the state and the effects.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::{
    reduce, ApiRequest, CallId, Command, Dispatch, Effect, GatewayEvent, Input, Phase, RequestId,
    SignInEvent, State, Ui, VoiceCommand, VoiceUpdate,
};
use yap_core::{ApiError, Token};
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

fn uis(fx: &[Effect]) -> Vec<&Ui> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Ui(ui) => Some(ui),
            _ => None,
        })
        .collect()
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
fn start_should_load_the_stored_token() {
    let mut client = Client::new();
    assert_eq!(client.command(Command::Start), vec![Effect::LoadToken]);
}

#[test]
fn start_without_a_stored_token_should_show_sign_in() {
    let mut client = Client::new();
    let fx = client.input(Input::TokenLoaded(None));
    assert_eq!(uis(&fx), vec![&Ui::Phase("signedOut")]);
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
    assert!(fx.contains(&Effect::Ui(Ui::Me(json!({
        "id": "user_fixture_me",
        "name": "Fixture Me",
        "username": "me",
        "avatar": format!("{BASE}/api/avatars/user_fixture_me"),
    })))));
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
    assert!(fx.contains(&Effect::Ui(Ui::ActiveServer(SERVER.into()))));
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
    let me_at = fx.iter().position(|e| matches!(e, Effect::Ui(Ui::Me(_))));
    let servers_at = fx
        .iter()
        .position(|e| matches!(e, Effect::Ui(Ui::Servers(_))));
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
    assert_eq!(
        uis(&fx)[..3],
        [
            &Ui::Channels(json!([
                { "id": TEXT, "name": "general", "type": "text", "category": "Text" },
                { "id": VOICE, "name": "lounge", "type": "voice", "category": "Voice" },
            ])),
            &Ui::ActiveChannel {
                id: TEXT.into(),
                name: "general".into()
            },
            &Ui::MessagesReset(json!([])),
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
    let Some(Ui::MessagesReset(list)) = uis(&fx).first().copied() else {
        panic!("{fx:#?}");
    };
    assert_eq!(
        list[0],
        json!({
            "id": "msg_fixture_0002",
            "channelId": TEXT,
            "authorId": "user_fixture_bob",
            "author": "bob",
            "avatar": "",
            "content": "see the attached file",
            "createdAt": "2026-11-04T12:00:05.000Z",
            "edited": true,
            "attachments": [{
                "name": "notes.txt",
                "url": format!("{BASE}/api/media/fixtures/notes.txt"),
                "contentType": "text/plain",
            }],
        })
    );
    assert!(fx.contains(&Effect::StartupMark("messages shown")));
    assert_eq!(
        uis(&fx)[1..],
        [&Ui::Phase("ready"), &Ui::Status(String::new())]
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
    assert!(
        matches!(uis(&fx)[..], [Ui::MessageUpsert(m)] if m["content"] == "hello from the fixture")
    );
    // The fixture reuses an id from the page: replaced in place, not added.
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
    assert_eq!(
        uis(&fx),
        vec![&Ui::MessageUpsert(
            json!({ "id": "msg_fixture_0002", "partial": true })
        )]
    );
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
    assert_eq!(
        uis(&fx),
        vec![&Ui::MessageUpsert(
            json!({ "id": "msg_fixture_0001", "partial": true, "content": "edited!", "edited": true })
        )]
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
        uis(&fx),
        vec![&Ui::MessageRemoved("msg_fixture_0002".into())]
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
    assert_eq!(uis(&fx), vec![&Ui::Typing("Alice".into())]);
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
    assert_eq!(uis(&fx), vec![&Ui::Typing("Someone".into())]);
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
    assert!(matches!(uis(&fx)[..], [Ui::MessageUpsert(m)] if m["id"] == "msg_sent"));
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
        uis(&fx),
        vec![&Ui::Status("HTTP 403: missing permission".into())]
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
fn selecting_a_voice_channel_should_join_it() {
    let mut client = Client::online();
    let fx = client.command(Command::SelectChannel(VOICE.into()));
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
    assert!(matches!(uis(&fx)[..], [Ui::Voice { muted: true, .. }]));
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
