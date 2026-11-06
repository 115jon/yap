//! Reducer tests for batch N4: DM calls (CALL_RING / CALL_RINGING /
//! CALL_RING_STOP and ops 36-39), voice moves, sticker reactions and the
//! voice details.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::shell::Sound;
use yap_core::store::calls::{CallsCommand, CallsView, RingStatus};
use yap_core::store::CallId;
use yap_core::store::{
    reduce, ApiRequest, Command, Dispatch, Effect, GatewayEvent, Input, RequestId, State,
    ViewUpdate, VoiceCommand, VoiceMoved, VoiceTarget, VoiceUpdate,
};

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const DM: &str = "dm_fixture_alice";
const ALICE: &str = "user_fixture_alice";

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(&path).expect("fixture")).expect("json")
}

fn user(id: &str, username: &str, display: Option<&str>) -> Value {
    json!({
        "id": id, "username": username, "display_name": display,
        "avatar_url": null, "status": "online", "custom_status": null,
    })
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    view: CallsView,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Calls(view)) => self.view = (**view).clone(),
                _ => {}
            }
        }
        fx
    }

    fn command(&mut self, command: Command) -> Vec<Effect> {
        self.input(Input::User(command))
    }

    fn calls(&mut self, command: CallsCommand) -> Vec<Effect> {
        self.command(Command::Calls(command))
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

    fn dispatch(&mut self, event: &str, data: Value) -> Vec<Effect> {
        let session = self.state.session().expect("session").id;
        let dispatch = Dispatch::decode(event, data).expect("decodes");
        self.input(Input::Gateway {
            session,
            event: GatewayEvent::Dispatch(dispatch),
        })
    }

    fn voice(&mut self, call: CallId, update: VoiceUpdate) -> Vec<Effect> {
        self.input(Input::Voice { call, update })
    }

    fn me(&self) -> String {
        self.state.me().expect("me").id.clone()
    }

    /// Signed in, the fixture server shown, Alice's DM loaded, gateway up.
    fn online() -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            view: CallsView::default(),
        };
        client.command(Command::UseToken("tok_fixture".into()));
        client.respond(&ApiRequest::Me, fixture("user-me.json"));
        client.respond(&ApiRequest::Servers, fixture("servers-list.json"));
        client.respond(
            &ApiRequest::Channels {
                server_id: SERVER.into(),
            },
            fixture("channels-response.json"),
        );
        client.respond(
            &ApiRequest::Messages {
                channel_id: TEXT.into(),
                before: None,
            },
            fixture("messages-page.json"),
        );
        client.respond(
            &ApiRequest::Dms,
            json!([{ "id": DM, "channel_type": "dm", "name": "x", "created_at": null,
                     "recipient": user(ALICE, "alice", Some("Alice")) }]),
        );
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client
    }
}

fn gateway_frames(fx: &[Effect]) -> Vec<Value> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::GatewaySend(frame) => {
                Some(serde_json::from_str(&frame.encode()).expect("json"))
            }
            _ => None,
        })
        .collect()
}

fn voice_start(fx: &[Effect]) -> Option<(CallId, VoiceTarget)> {
    fx.iter().find_map(|e| match e {
        Effect::VoiceStart { call, target, .. } => Some((*call, target.clone())),
        _ => None,
    })
}

fn sounds(fx: &[Effect]) -> Vec<Sound> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::View(ViewUpdate::Sound(s)) => Some(*s),
            _ => None,
        })
        .collect()
}

fn ring(client: &mut Client) -> Vec<Effect> {
    client.dispatch(
        "CALL_RING",
        json!({ "call_id": "call-1", "caller_id": ALICE, "caller_name": "alice",
                "caller_display_name": "Alice", "caller_avatar": null, "channel_id": DM }),
    )
}

#[test]
fn a_ring_should_play_the_ring_toast_and_show_the_incoming_call() {
    let mut c = Client::online();
    let fx = ring(&mut c);
    assert!(sounds(&fx).contains(&Sound::Ring));
    assert!(fx.iter().any(
        |e| matches!(e, Effect::View(ViewUpdate::Toast(t)) if t.title == "Incoming call from Alice")
    ));
    assert_eq!(c.view.status, RingStatus::RingingIncoming);
    assert_eq!(c.view.remote.name, "Alice");
}

#[test]
fn accept_should_send_op_37_and_the_accepted_stop_should_join_the_dm_room() {
    let mut c = Client::online();
    ring(&mut c);
    let fx = c.calls(CallsCommand::Accept { video: false });
    assert_eq!(
        gateway_frames(&fx)[0],
        json!({ "op": 37, "d": { "call_id": "call-1" } })
    );
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-1", "reason": "accepted" }),
    );
    assert!(sounds(&fx).contains(&Sound::CallConnect));
    let (_, target) = voice_start(&fx).expect("joins");
    let mut ids = [c.me(), ALICE.to_owned()];
    ids.sort();
    assert_eq!(
        target.room_slug,
        Some(format!("dm-call-{}-{}", ids[0], ids[1]))
    );
    assert_eq!(target.server_id, "");
    assert_eq!(target.channel_id, DM);
    assert_eq!(c.view.status, RingStatus::Active);
}

#[test]
fn decline_should_send_op_38_and_end_the_call() {
    let mut c = Client::online();
    ring(&mut c);
    let fx = c.calls(CallsCommand::Decline);
    assert_eq!(
        gateway_frames(&fx)[0],
        json!({ "op": 38, "d": { "call_id": "call-1" } })
    );
    assert!(fx.contains(&Effect::View(ViewUpdate::StopSound(Sound::Ring))));
    assert_eq!(c.view.status, RingStatus::Idle);
    assert_eq!(c.view.end_reason, "declined");
}

#[test]
fn start_should_ring_the_dm_recipient_with_op_36() {
    let mut c = Client::online();
    let fx = c.calls(CallsCommand::Start {
        channel_id: DM.into(),
    });
    assert_eq!(
        gateway_frames(&fx)[0],
        json!({ "op": 36, "d": { "target_user_id": ALICE, "channel_id": DM } })
    );
}

#[test]
fn a_declined_outgoing_ring_should_keep_the_caller_in_the_call_room() {
    let mut c = Client::online();
    let fx = c.dispatch(
        "CALL_RINGING",
        json!({ "call_id": "call-2", "callee_id": ALICE, "callee_display_name": "Alice", "channel_id": DM }),
    );
    assert!(sounds(&fx).contains(&Sound::OutgoingRing));
    assert_eq!(c.view.status, RingStatus::RingingOutgoing);
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-2", "reason": "declined" }),
    );
    assert!(voice_start(&fx).is_some(), "the caller stays, in the room");
    assert_eq!(c.view.status, RingStatus::Active);
}

#[test]
fn cancel_should_send_op_39() {
    let mut c = Client::online();
    c.dispatch(
        "CALL_RINGING",
        json!({ "call_id": "call-2", "callee_id": ALICE, "channel_id": DM }),
    );
    let fx = c.calls(CallsCommand::Cancel);
    assert_eq!(
        gateway_frames(&fx)[0],
        json!({ "op": 39, "d": { "call_id": "call-2" } })
    );
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-2", "reason": "cancelled" }),
    );
    assert!(sounds(&fx).contains(&Sound::CallEnd));
    assert_eq!(c.view.status, RingStatus::Idle);
}

#[test]
fn a_ring_while_in_another_active_call_should_be_declined_at_once() {
    let mut c = Client::online();
    ring(&mut c);
    c.calls(CallsCommand::Accept { video: false });
    c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-1", "reason": "accepted" }),
    );
    let fx = c.dispatch(
        "CALL_RING",
        json!({ "call_id": "call-9", "caller_id": "someone", "channel_id": "dm-other" }),
    );
    assert_eq!(
        gateway_frames(&fx)[0],
        json!({ "op": 38, "d": { "call_id": "call-9" } })
    );
    assert!(sounds(&fx).is_empty());
}

#[test]
fn a_busy_stop_should_end_the_ring_silently() {
    let mut c = Client::online();
    c.dispatch(
        "CALL_RINGING",
        json!({ "call_id": "call-2", "callee_id": ALICE, "channel_id": DM }),
    );
    c.calls(CallsCommand::Cancel);
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": null, "reason": "busy" }),
    );
    assert!(!sounds(&fx).contains(&Sound::CallEnd));
    assert_eq!(c.view.end_reason, "busy");
}

#[test]
fn the_peer_going_offline_should_end_a_ring() {
    let mut c = Client::online();
    ring(&mut c);
    c.dispatch(
        "PRESENCE_UPDATE",
        json!({ "user_id": ALICE, "status": "offline", "custom_status": null }),
    );
    assert_eq!(c.view.status, RingStatus::Idle);
    assert_eq!(c.view.end_reason, "disconnected");
}

#[test]
fn a_dm_call_should_play_no_voice_connect_sound_and_send_op_33_for_the_dm() {
    let mut c = Client::online();
    ring(&mut c);
    c.calls(CallsCommand::Accept { video: false });
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-1", "reason": "accepted" }),
    );
    let (call, _) = voice_start(&fx).expect("joins");
    let fx = c.voice(call, VoiceUpdate::Connected);
    assert!(!sounds(&fx).contains(&Sound::Connect));
    assert_eq!(gateway_frames(&fx)[0]["op"], 33);
    assert_eq!(gateway_frames(&fx)[0]["d"]["channel_id"], DM);
}

#[test]
fn hang_up_should_leave_the_room_play_the_end_tone_and_end_the_call() {
    let mut c = Client::online();
    ring(&mut c);
    c.calls(CallsCommand::Accept { video: false });
    let fx = c.dispatch(
        "CALL_RING_STOP",
        json!({ "call_id": "call-1", "reason": "accepted" }),
    );
    let (call, _) = voice_start(&fx).expect("joins");
    let fx = c.calls(CallsCommand::Leave);
    assert!(sounds(&fx).contains(&Sound::CallEnd));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::Leave
    }));
    assert_eq!(c.view.status, RingStatus::Idle);
    assert_eq!(c.view.end_reason, "left");
}

#[test]
fn a_move_should_leave_and_join_the_target_with_its_grant() {
    let mut c = Client::online();
    let voice = c
        .state
        .channels(SERVER)
        .expect("tree")
        .channels
        .iter()
        .find(|ch| ch.channel_type == yap_protocol::rest::ChannelType::Voice)
        .expect("a voice channel")
        .id
        .clone();
    let fx = c.command(Command::JoinVoice(voice.clone()));
    let (call, _) = voice_start(&fx).expect("joins");
    let fx = c.voice(
        call,
        VoiceUpdate::Moved(VoiceMoved {
            server_id: SERVER.into(),
            channel_id: "chan_target".into(),
            channel_name: "Lounge".into(),
            mover_name: "Mod".into(),
            grant: Some("grant-1".into()),
        }),
    );
    assert!(fx.contains(&Effect::View(ViewUpdate::Notice(
        "Mod moved you to Lounge".into()
    ))));
    assert!(fx.contains(&Effect::Voice {
        call,
        command: VoiceCommand::Leave
    }));
    let (next, target) = voice_start(&fx).expect("joins the target");
    assert_ne!(next, call);
    assert_eq!(target.channel_id, "chan_target");
    assert_eq!(target.move_grant.as_deref(), Some("grant-1"));
    let fx = c.voice(next, VoiceUpdate::Connected);
    assert_eq!(gateway_frames(&fx)[0]["d"]["move_grant"], "grant-1");
}

#[test]
fn move_should_post_the_voice_move_route() {
    let mut c = Client::online();
    let fx = c.calls(CallsCommand::Move {
        user_id: ALICE.into(),
        from_channel_id: "a".into(),
        to_channel_id: "b".into(),
    });
    let request = fx
        .iter()
        .find_map(|e| match e {
            Effect::Http { request, .. } => Some(request.clone()),
            _ => None,
        })
        .expect("a request");
    assert_eq!(request.path(), "/api/voice/move");
    assert_eq!(
        request.body(),
        json!({ "server_id": SERVER, "user_id": ALICE, "from_channel_id": "a", "to_channel_id": "b" })
    );
}

#[test]
fn a_sticker_should_land_in_the_view_with_a_new_seq() {
    let mut c = Client::online();
    let voice = c
        .state
        .channels(SERVER)
        .expect("tree")
        .channels
        .iter()
        .find(|ch| ch.channel_type == yap_protocol::rest::ChannelType::Voice)
        .expect("a voice channel")
        .id
        .clone();
    let fx = c.command(Command::JoinVoice(voice));
    let (call, _) = voice_start(&fx).expect("joins");
    let sticker = yap_protocol::voice_app::AppEvent::decode(&json!({
        "type": "reaction.sticker", "url": "https://static.klipy.com/a.webp",
        "contentType": "image/webp", "displayMode": "rain", "participant_id": "p", "user_id": ALICE,
    }))
    .expect("decodes");
    c.voice(
        call,
        VoiceUpdate::App {
            event: Box::new(sticker),
            now_ms: 0,
        },
    );
    assert_eq!(c.view.stickers.len(), 1);
    assert_eq!(c.view.stickers[0].display_mode, "rain");
    assert_eq!(c.view.stickers[0].seq, 1);
    let fx = c.calls(CallsCommand::Sticker {
        url: "https://static.klipy.com/b.gif".into(),
        content_type: String::new(),
        display_mode: "nope".into(),
    });
    let frame = fx
        .iter()
        .find_map(|e| match e {
            Effect::Voice {
                command: VoiceCommand::SendApp(frame),
                ..
            } => Some(serde_json::from_str::<Value>(frame).expect("json")),
            _ => None,
        })
        .expect("sent");
    assert_eq!(
        frame["d"],
        json!({ "type": "reaction.sticker", "url": "https://static.klipy.com/b.gif",
                "contentType": "image/gif", "displayMode": "single" })
    );
}
