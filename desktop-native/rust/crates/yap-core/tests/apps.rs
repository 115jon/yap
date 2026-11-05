//! Voice apps through the reducer: the soundboard (play, preview, the
//! playing rows, join and leave sounds, upload and delete), Listen Together
//! (the player following the room, drift, the queue commands) and the
//! Wordle stage. The room's broadcasts are the op 106 fixtures of
//! `yap-protocol` (shaped as `realtime/voice-room.ts` sends them).

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_core::store::apps::{
    AppsCommand, AppsRequest, AppsView, AudioCommand, AudioEvent, ClipSource, Trigger,
};
use yap_core::store::{
    reduce, ApiRequest, CallId, Command, Effect, GatewayEvent, Input, Method, RequestId, State,
    ViewUpdate, VoiceCommand, VoiceUpdate,
};
use yap_protocol::voice_app::AppEvent;
use yap_protocol::Frame;

const BASE: &str = "https://api.fixture.invalid";
const SERVER: &str = "srv_fixture";
const TEXT: &str = "chan_fixture_text";
const VOICE: &str = "chan_fixture_voice";
const ME: &str = "user_fixture_me";
const PEER: &str = "user_fixture_alice";
const PID: &str = "p_me";
const NOW: i64 = 1_762_350_000_000;

fn shared(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../packages/protocol/fixtures")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
}

/// An op 106 broadcast from `yap-protocol`'s fixtures, retargeted at the
/// fixture call (server, channel, and the peer as sender).
fn broadcast(name: &str) -> AppEvent {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../yap-protocol/tests/fixtures/voice-app")
        .join(name);
    let text = std::fs::read_to_string(path)
        .expect("fixture")
        .replace("srv_1", SERVER)
        .replace("\"ch_v\"", &format!("\"{VOICE}\""))
        .replace("sb1:6:u_peer:", &format!("sb1:{}:{PEER}:", PEER.len()))
        .replace("u_peer", PEER);
    let frame = Frame::decode(&text).expect("frame");
    AppEvent::decode(&frame.d).expect("an app event")
}

struct Client {
    state: State,
    asked: Vec<(RequestId, ApiRequest)>,
    effects: Vec<Effect>,
    view: AppsView,
    call: Option<CallId>,
}

impl Client {
    fn input(&mut self, input: Input) -> Vec<Effect> {
        let fx = reduce(&mut self.state, input);
        for effect in &fx {
            match effect {
                Effect::Http { id, request, .. } => self.asked.push((*id, request.clone())),
                Effect::View(ViewUpdate::Apps(view)) => self.view = (**view).clone(),
                Effect::VoiceStart { call, .. } => self.call = Some(*call),
                _ => {}
            }
        }
        self.effects.extend(fx.iter().cloned());
        fx
    }

    fn respond(&mut self, request: &ApiRequest, body: Value) -> Vec<Effect> {
        let id = self
            .asked
            .iter()
            .rev()
            .find(|(_, r)| r == request)
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no {request:?} in {:#?}", self.asked));
        let result = request.decode(body);
        self.input(Input::Http { id, result })
    }

    fn apps(&mut self, command: AppsCommand) -> Vec<Effect> {
        self.input(Input::User(Command::Apps(command)))
    }

    fn voice(&mut self, update: VoiceUpdate) -> Vec<Effect> {
        let call = self.call.expect("in a call");
        self.input(Input::Voice { call, update })
    }

    fn event(&mut self, event: AppEvent, now_ms: i64) -> Vec<Effect> {
        self.voice(VoiceUpdate::App {
            event: Box::new(event),
            now_ms,
        })
    }

    /// Signed in (with `sound_settings`), on the fixture server, in its
    /// voice channel with the voice socket ready, the catalog answered.
    fn in_call_with(sound_settings: Value, catalog: Value) -> Self {
        let mut client = Self {
            state: State::new(BASE),
            asked: Vec::new(),
            effects: Vec::new(),
            view: AppsView::default(),
            call: None,
        };
        client.input(Input::User(Command::UseToken("tok_fixture".into())));
        let mut me = shared("user-me.json");
        me["sound_settings"] = sound_settings;
        client.respond(&ApiRequest::Me, me);
        client.respond(&ApiRequest::Servers, shared("servers-list.json"));
        client.respond(
            &ApiRequest::Channels {
                server_id: SERVER.into(),
            },
            shared("channels-response.json"),
        );
        client.respond(
            &ApiRequest::Messages {
                channel_id: TEXT.into(),
                before: None,
            },
            shared("messages-page.json"),
        );
        let session = client.state.session().expect("session").id;
        client.input(Input::Gateway {
            session,
            event: GatewayEvent::Ready,
        });
        client.input(Input::User(Command::JoinVoice(VOICE.into())));
        client.voice(VoiceUpdate::Connected);
        client.voice(VoiceUpdate::VoiceReady {
            participant_id: PID.into(),
        });
        client.respond(&catalog_request(), catalog);
        client
    }

    fn in_call() -> Self {
        Self::in_call_with(Value::Null, json!([]))
    }

    /// Op 106 frames this input sent, as JSON `d`s.
    fn frames(fx: &[Effect]) -> Vec<Value> {
        fx.iter()
            .filter_map(|e| match e {
                Effect::Voice {
                    command: VoiceCommand::SendApp(frame),
                    ..
                } => Some(Frame::decode(frame).expect("frame").d),
                _ => None,
            })
            .collect()
    }

    fn audio(fx: &[Effect]) -> Vec<AudioCommand> {
        fx.iter()
            .filter_map(|e| match e {
                Effect::Audio { command, .. } => Some(command.clone()),
                _ => None,
            })
            .collect()
    }
}

fn catalog_request() -> ApiRequest {
    ApiRequest::Apps(AppsRequest::Catalog {
        server_id: SERVER.into(),
    })
}

fn airhorn() -> Value {
    json!([{
        "id": "snd_9", "server_id": SERVER, "user_id": PEER, "name": "Airhorn",
        "file_url": "/api/soundboard/uploads/snd_9", "emoji": "📯", "volume": 0.5,
    }])
}

// ── Soundboard ────────────────────────────────────────────────────────────

#[test]
fn voice_ready_should_ask_for_the_listen_together_state() {
    let client = Client::in_call();
    let frames = Client::frames(&client.effects);
    assert!(frames.contains(&json!({
        "type": "listen_together.state.request",
        "room_slug": format!("voice-{SERVER}-{VOICE}"),
    })));
}

#[test]
fn a_tone_should_go_out_with_our_playback_id_and_the_send_volume() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::Play("ping".into()));
    assert_eq!(
        Client::frames(&fx),
        vec![json!({
            "type": "soundboard.play", "server_key": SERVER, "user_id": ME,
            "playback_id": format!("sb1:{}:{ME}:ping", ME.len()), "sound_id": "ping",
            "name": "Ping", "volume": 0.8,
        })]
    );
}

#[test]
fn a_server_sound_should_go_out_with_its_upload_url_and_volume() {
    let mut client = Client::in_call_with(Value::Null, airhorn());
    let fx = client.apps(AppsCommand::Play("snd_9".into()));
    let frame = &Client::frames(&fx)[0];
    assert_eq!(
        (&frame["media_url"], &frame["volume"]),
        (&json!("/api/soundboard/uploads/snd_9"), &json!(0.4))
    );
}

#[test]
fn a_peers_sound_should_play_from_its_offset_and_volume() {
    let mut client = Client::in_call();
    // Relayed 400 ms before it arrived.
    let fx = client.event(broadcast("soundboard-play-server.json"), NOW + 400);
    let key = format!("{SERVER}::sb1:{}:{PEER}:snd_9", PEER.len());
    assert_eq!(
        Client::audio(&fx),
        vec![AudioCommand::Play {
            key,
            source: ClipSource::Media("/api/soundboard/uploads/snd_9".into()),
            volume: 0.8,
            start_at: 0.4,
            paused: false,
            max_seconds: None,
            preview: false,
        }]
    );
}

#[test]
fn a_join_clip_should_be_cut_at_three_seconds() {
    let mut client = Client::in_call();
    let fx = client.event(broadcast("soundboard-play-join.json"), NOW);
    let Some(AudioCommand::Play { max_seconds, .. }) = Client::audio(&fx).into_iter().next() else {
        panic!("no play");
    };
    assert_eq!(max_seconds, Some(3.0));
}

#[test]
fn a_play_with_a_bad_media_or_data_url_should_be_dropped() {
    let mut client = Client::in_call();
    for (media, data) in [
        (Some("https://evil.example/media/sounds/x.mp3"), None),
        (Some("/api/soundboard/uploads/a/b"), None),
        (Some("/api/attachments/x"), None),
        (None, Some("data:image/png;base64,AAAA")),
        (None, Some("data:audio/wav;base64,AAA")),
        // A bad URL drops the event even when the other one is fine.
        (
            Some("http://www.myinstants.com/media/sounds/x.mp3"),
            Some("data:audio/wav;base64,AAAA"),
        ),
    ] {
        let AppEvent::SoundboardPlay(mut play) = broadcast("soundboard-play-server.json") else {
            panic!("not a play");
        };
        play.media_url = media.map(str::to_owned);
        play.data_url = data.map(str::to_owned);
        let fx = client.event(AppEvent::SoundboardPlay(play), NOW);
        assert!(Client::audio(&fx).is_empty(), "{media:?} {data:?}");
    }
    // The DM call's capability URL still plays.
    let AppEvent::SoundboardPlay(mut play) = broadcast("soundboard-play-server.json") else {
        panic!("not a play");
    };
    play.media_url = Some(
        "/api/soundboard/uploads/snd_9?cap=sbm1.e30.c2ln&room_slug=dm-call-1&playback_id=pb1"
            .into(),
    );
    let fx = client.event(AppEvent::SoundboardPlay(play), NOW);
    assert_eq!(Client::audio(&fx).len(), 1);
}

#[test]
fn another_servers_sound_should_be_ignored() {
    let mut client = Client::in_call();
    let AppEvent::SoundboardPlay(mut play) = broadcast("soundboard-play-server.json") else {
        panic!("not a play");
    };
    play.server_key = "srv_other".into();
    let fx = client.event(AppEvent::SoundboardPlay(play), NOW);
    assert!(Client::audio(&fx).is_empty());
}

#[test]
fn a_started_row_should_show_and_a_peers_stop_should_end_it() {
    let mut client = Client::in_call();
    client.event(broadcast("soundboard-play-server.json"), NOW);
    let key = format!("{SERVER}::sb1:{}:{PEER}:snd_9", PEER.len());
    client.input(Input::Audio(AudioEvent::Started(key.clone())));
    assert_eq!(client.view.playing.len(), 1);
    let fx = client.event(broadcast("soundboard-stop.json"), NOW);
    assert_eq!(Client::audio(&fx), vec![AudioCommand::Stop(key)]);
    assert!(client.view.playing.is_empty());
}

#[test]
fn pause_and_volume_from_the_room_should_reach_the_player() {
    let mut client = Client::in_call();
    client.event(broadcast("soundboard-play-server.json"), NOW);
    let key = format!("{SERVER}::sb1:{}:{PEER}:snd_9", PEER.len());
    let paused = client.event(broadcast("soundboard-pause-set.json"), NOW);
    let volume = client.event(broadcast("soundboard-volume-set.json"), NOW);
    assert_eq!(
        (Client::audio(&paused), Client::audio(&volume)),
        (
            vec![AudioCommand::Pause {
                key: key.clone(),
                paused: true
            }],
            vec![AudioCommand::Volume { key, volume: 0.25 }]
        )
    );
}

#[test]
fn stopping_a_peers_sound_should_stay_local() {
    let mut client = Client::in_call();
    client.event(broadcast("soundboard-play-server.json"), NOW);
    let fx = client.apps(AppsCommand::StopPlayback(format!(
        "sb1:{}:{PEER}:snd_9",
        PEER.len()
    )));
    assert!(Client::frames(&fx).is_empty());
    assert_eq!(Client::audio(&fx).len(), 1);
}

#[test]
fn our_audible_clip_should_raise_the_soundshare_flag_until_it_ends() {
    let mut client = Client::in_call();
    let ours = json!({
        "type": "soundboard.play", "participant_id": PID, "user_id": ME,
        "server_key": SERVER, "playback_id": format!("sb1:{}:{ME}:ping", ME.len()),
        "sound_id": "ping", "name": "Ping", "volume": 0.8, "sent_at": NOW,
    });
    client.event(AppEvent::decode(&ours).expect("play"), NOW);
    let key = format!("{SERVER}::sb1:{}:{ME}:ping", ME.len());
    let flag = |fx: &[Effect]| {
        fx.iter().find_map(|e| match e {
            Effect::Voice {
                command: VoiceCommand::SetSoundboardSpeaking(on),
                ..
            } => Some(*on),
            _ => None,
        })
    };
    let started = client.input(Input::Audio(AudioEvent::Started(key.clone())));
    let ended = client.input(Input::Audio(AudioEvent::Ended(key)));
    assert_eq!((flag(&started), flag(&ended)), (Some(true), Some(false)));
}

#[test]
fn a_preview_should_play_only_here() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::Preview("chime".into()));
    assert!(Client::frames(&fx).is_empty());
    assert!(matches!(
        Client::audio(&fx).as_slice(),
        [AudioCommand::Play {
            preview: true,
            source: ClipSource::Tone(_),
            ..
        }]
    ));
}

#[test]
fn a_catalog_update_should_add_the_sound() {
    let mut client = Client::in_call();
    client.event(broadcast("soundboard-catalog-updated.json"), NOW);
    let names: Vec<&str> = client.view.sounds.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Airhorn"]);
}

#[test]
fn the_owner_should_delete_any_sound_and_upload() {
    let client = Client::in_call_with(Value::Null, airhorn());
    assert_eq!(
        (client.view.can_upload, client.view.sounds[0].can_delete),
        (true, true)
    );
}

#[test]
fn delete_should_name_the_sound_in_the_query() {
    let request = AppsRequest::DeleteSound {
        server_id: SERVER.into(),
        sound_id: "snd 9".into(),
    };
    assert_eq!(
        (request.method(), request.path()),
        (
            Method::Delete,
            format!("/api/servers/{SERVER}/soundboard?soundId=snd%209")
        )
    );
}

#[test]
fn a_deleted_sound_should_leave_the_picker() {
    let mut client = Client::in_call_with(Value::Null, airhorn());
    client.apps(AppsCommand::Delete("snd_9".into()));
    let request = ApiRequest::Apps(AppsRequest::DeleteSound {
        server_id: SERVER.into(),
        sound_id: "snd_9".into(),
    });
    client.respond(&request, json!({ "deleted": true }));
    assert!(client.view.sounds.is_empty());
}

#[test]
fn an_upload_should_post_the_soundboard_fields_then_announce_the_sound() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::Upload {
        path: "C:/tmp/horn.mp3".into(),
        file_name: "horn.mp3".into(),
        size: 2048,
        content_type: String::new(),
        sound_name: " Horn ".into(),
        emoji: "📯".into(),
        volume: 0.7,
    });
    let Some(Effect::SoundUpload {
        id,
        channel_id,
        content_type,
        fields,
        ..
    }) = fx.iter().find(|e| matches!(e, Effect::SoundUpload { .. }))
    else {
        panic!("no upload");
    };
    assert_eq!(
        (channel_id.as_str(), content_type.as_str(), fields.clone()),
        (
            VOICE,
            "audio/mpeg",
            vec![
                ("purpose".into(), "soundboard".into()),
                ("sound_name".into(), "Horn".into()),
                ("sound_emoji".into(), "📯".into()),
                ("sound_volume".into(), "0.7".into()),
            ]
        )
    );
    let uploaded = yap_core::store::ApiResponse::Uploaded(
        serde_json::from_value(json!({
            "id": "snd_new", "file_url": "/api/soundboard/uploads/snd_new",
            "file_name": "horn.mp3", "file_size": 2048, "content_type": "audio/mpeg",
        }))
        .expect("uploaded"),
    );
    let fx = client.input(Input::Http {
        id: *id,
        result: Ok(uploaded),
    });
    let frame = &Client::frames(&fx)[0];
    assert_eq!(
        (&frame["type"], &frame["sound"]["id"]),
        (&json!("soundboard.catalog-updated"), &json!("snd_new"))
    );
}

#[test]
fn a_non_audio_upload_should_be_refused_before_it_goes() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::Upload {
        path: "C:/tmp/notes.txt".into(),
        file_name: "notes.txt".into(),
        size: 10,
        content_type: "text/plain".into(),
        sound_name: "Notes".into(),
        emoji: String::new(),
        volume: 1.0,
    });
    assert!(!fx.iter().any(|e| matches!(e, Effect::SoundUpload { .. })));
    assert_eq!(
        client.view.sound_error,
        "Only audio files can be uploaded to the soundboard."
    );
}

// ── Join and leave sounds ─────────────────────────────────────────────────

fn chime_triggers() -> Value {
    let chime = json!({ "enabled": true, "sound": {
        "source": "default", "soundId": "chime", "name": "Chime", "volume": 1,
    }});
    json!({ "voiceJoinSoundboard": { "*": chime }, "voiceLeaveSoundboard": { "*": chime } })
}

#[test]
fn the_join_sound_should_go_out_once_the_voice_socket_is_ready() {
    let client = Client::in_call_with(chime_triggers(), json!([]));
    let joins: Vec<Value> = Client::frames(&client.effects)
        .into_iter()
        .filter(|f| f["type"] == "soundboard.play")
        .collect();
    assert_eq!(joins.len(), 1);
    assert_eq!(
        (
            &joins[0]["automatic_event"],
            &joins[0]["max_duration_seconds"],
            &joins[0]["volume"]
        ),
        (&json!("join"), &json!(3.0), &json!(1.0))
    );
}

#[test]
fn the_leave_sound_should_go_before_the_leave() {
    let mut client = Client::in_call_with(chime_triggers(), json!([]));
    let fx = client.input(Input::User(Command::LeaveVoice));
    let commands: Vec<&VoiceCommand> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::Voice { command, .. } => Some(command),
            _ => None,
        })
        .collect();
    assert!(matches!(
        commands.as_slice(),
        [VoiceCommand::SendApp(frame), VoiceCommand::Leave, ..]
            if frame.contains("\"automatic_event\":\"leave\"")
    ));
}

#[test]
fn a_new_join_sound_should_be_saved_for_the_scope() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::SetTrigger {
        trigger: Trigger::Join,
        scope: SERVER.into(),
        enabled: None,
        sound: Some("default:tada".into()),
        clear: false,
    });
    let request = fx
        .iter()
        .find_map(|e| match e {
            Effect::Http {
                request: ApiRequest::Apps(request @ AppsRequest::SaveTrigger { .. }),
                ..
            } => Some(request.clone()),
            _ => None,
        })
        .expect("a save");
    assert_eq!(
        (
            request.method(),
            request.path(),
            request.body()["sound_settings"]["voiceJoinSoundboard"][SERVER].clone()
        ),
        (
            Method::Patch,
            "/api/users/me".into(),
            json!({ "enabled": true, "sound": {
                "source": "default", "soundId": "tada", "name": "Ta-da", "volume": 1.0,
            }})
        )
    );
    assert!(!client.view.joins[SERVER].inherited);
}

// ── Listen Together ───────────────────────────────────────────────────────

#[test]
fn a_snapshot_should_load_the_stream_at_the_room_position() {
    let mut client = Client::in_call();
    // Anchored at 12 s, 2 s ago.
    let fx = client.event(broadcast("lt-snapshot.json"), NOW + 2_000);
    let loads: Vec<AudioCommand> = Client::audio(&fx)
        .into_iter()
        .filter(|c| matches!(c, AudioCommand::StreamLoad { .. }))
        .collect();
    let [AudioCommand::StreamLoad {
        key,
        path,
        position_ms,
        paused,
    }] = loads.as_slice()
    else {
        panic!("one load: {loads:?}");
    };
    assert_eq!(
        (key.as_str(), *position_ms, *paused),
        ("e_1", 14_000.0, false)
    );
    assert!(path.starts_with("/api/listen-together/stream?videoId=dQw4w9WgXcQ&"));
    assert!(path.contains(&format!("sessionId={PID}")) && path.ends_with("&format=mp4"));
    assert_eq!(
        client.view.listen.now.as_ref().map(|n| n.title.as_str()),
        Some("Never Gonna Give You Up")
    );
}

fn position(client: &mut Client, position_ms: f64, now_ms: i64) -> Vec<AudioCommand> {
    let fx = client.input(Input::Audio(AudioEvent::StreamPosition {
        key: "e_1".into(),
        position_ms,
        now_ms,
    }));
    Client::audio(&fx)
}

#[test]
fn player_position_reports_should_not_change_the_view() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    let before = client.view.listen.clone();
    assert_eq!(before.position_at, NOW);
    for step in 1..=8 {
        let fx = client.input(Input::Audio(AudioEvent::StreamPosition {
            key: "e_1".into(),
            position_ms: 12_000.0 + 250.0 * f64::from(step),
            now_ms: NOW + 250 * i64::from(step),
        }));
        assert!(
            !fx.iter()
                .any(|e| matches!(e, Effect::View(ViewUpdate::Apps(_)))),
            "step {step} re-sent the apps view"
        );
    }
    assert_eq!(client.view.listen, before);
}

#[test]
fn a_stream_that_ended_should_stay_loaded_so_a_seek_reaches_it() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    let fx = client.input(Input::Audio(AudioEvent::StreamEnded("e_1".into())));
    assert!(Client::audio(&fx).is_empty());
    // The room moves back (a new snapshot): the player is told to seek,
    // never to load again (the engine keeps the ended stream, and a seek
    // restarts it).
    position(&mut client, 213_000.0, NOW + 3_000);
    let fx = client.event(broadcast("lt-snapshot.json"), NOW + 4_000);
    let commands = Client::audio(&fx);
    assert!(!commands
        .iter()
        .any(|c| matches!(c, AudioCommand::StreamLoad { .. })));
    assert!(commands
        .iter()
        .any(|c| matches!(c, AudioCommand::StreamSeek(_))));
}

#[test]
fn a_playing_player_more_than_750_ms_off_should_seek() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    // Settled (3 s after the load) and playing: 16 250 expected.
    position(&mut client, 15_800.0, NOW + 4_000);
    let near = position(&mut client, 16_000.0, NOW + 4_250);
    let far = position(&mut client, 17_200.0, NOW + 4_250);
    assert_eq!(
        (near, far),
        (Vec::new(), vec![AudioCommand::StreamSeek(16_250.0)])
    );
}

#[test]
fn a_player_still_buffering_should_not_be_sought_again() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    // Before the settle window ends, and stuck at one position after it.
    let early = position(&mut client, 11_000.0, NOW + 1_000);
    position(&mut client, 12_000.0, NOW + 4_000);
    let stuck = position(&mut client, 12_000.0, NOW + 5_000);
    assert_eq!((early, stuck), (Vec::new(), Vec::new()));
}

#[test]
fn an_empty_queue_should_stop_the_stream() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    let fx = client.event(broadcast("lt-playback-updated-empty.json"), NOW);
    assert!(Client::audio(&fx).contains(&AudioCommand::StreamStop));
}

#[test]
fn seek_and_pause_should_name_the_current_entry() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    let seek = client.apps(AppsCommand::LtSeek(61_000.0));
    let pause = client.apps(AppsCommand::LtPause(true));
    assert_eq!(
        (Client::frames(&seek), Client::frames(&pause)),
        (
            vec![json!({
                "type": "listen_together.seek", "room_slug": format!("voice-{SERVER}-{VOICE}"),
                "positionMs": 61_000.0, "entryId": "e_1",
            })],
            vec![json!({
                "type": "listen_together.pause", "room_slug": format!("voice-{SERVER}-{VOICE}"),
                "paused": true, "entryId": "e_1",
            })]
        )
    );
}

#[test]
fn a_search_should_carry_the_voice_session_and_queue_its_result() {
    let mut client = Client::in_call();
    client.apps(AppsCommand::LtSearch("never gonna".into()));
    let (_, request) = client.asked.last().cloned().expect("a search");
    assert_eq!(
        request.path(),
        format!(
            "/api/listen-together/search?q=never%20gonna&filter=track&roomSlug=voice-{SERVER}-{VOICE}&serverId={SERVER}&channelId={VOICE}&sessionId={PID}"
        )
    );
    client.respond(
        &request,
        json!({ "filter": "track", "cursor": null, "results": [{
            "kind": "track", "id": "youtube:dQw4w9WgXcQ", "provider": "youtube",
            "videoId": "dQw4w9WgXcQ", "title": "Never Gonna Give You Up",
            "artist": "Rick Astley", "durationMs": 213000,
            "canonicalUrl": "https://www.youtube.com/watch?v=dQw4w9WgXcQ", "sourceLabel": "YouTube",
        }]}),
    );
    let fx = client.apps(AppsCommand::LtEnqueue {
        index: 0,
        mode: "append".into(),
    });
    let frame = &Client::frames(&fx)[0];
    assert_eq!(
        (
            &frame["type"],
            &frame["entries"][0]["track"]["kind"],
            &frame["entries"][0]["requester"]["userId"]
        ),
        (
            &json!("listen_together.enqueue"),
            &json!("music"),
            &json!(ME)
        )
    );
}

#[test]
fn a_link_should_resolve_instead_of_search() {
    let mut client = Client::in_call();
    client.apps(AppsCommand::LtSearch("https://youtu.be/dQw4w9WgXcQ".into()));
    let (_, request) = client.asked.last().cloned().expect("a resolve");
    assert_eq!(
        (request.method(), request.body()["url"].clone()),
        (Method::Post, json!("https://youtu.be/dQw4w9WgXcQ"))
    );
}

// ── Activities and Wordle ─────────────────────────────────────────────────

#[test]
fn starting_wordle_should_tell_the_call_and_open_our_stage() {
    let mut client = Client::in_call();
    let fx = client.apps(AppsCommand::StartActivity {
        activity: "wordle".into(),
        now_ms: NOW,
    });
    assert_eq!(
        Client::frames(&fx),
        vec![json!({
            "type": "activity.start", "userId": ME, "channelId": VOICE,
            "activity": "wordle", "startedAt": NOW,
        })]
    );
    assert_eq!(client.view.activity, "wordle");
}

#[test]
fn a_peers_activity_should_be_listed_until_they_leave() {
    let mut client = Client::in_call();
    client.event(broadcast("activity-start.json"), NOW);
    assert_eq!(
        client.view.others.get(PEER).map(String::as_str),
        Some("wordle")
    );
    client.event(broadcast("activity-leave.json"), NOW);
    assert!(client.view.others.is_empty());
}

fn wordle_open(client: &mut Client) {
    client.apps(AppsCommand::WordleOpen {
        date: "2026-11-05".into(),
    });
    client.respond(
        &ApiRequest::Apps(AppsRequest::Wordle {
            date: "2026-11-05".into(),
        }),
        json!({ "id": 1600, "print_date": "2026-11-05", "solution": "slate", "editor": null, "source": "nyt" }),
    );
}

#[test]
fn the_puzzle_should_load_for_new_yorks_date() {
    let request = AppsRequest::Wordle {
        date: "2026-11-05".into(),
    };
    assert_eq!(request.path(), "/api/wordle/today?date=2026-11-05");
}

#[test]
fn a_guess_should_be_marked_and_relayed() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    for letter in "crane".chars() {
        client.apps(AppsCommand::WordleKey(letter));
    }
    let fx = client.apps(AppsCommand::WordleEnter);
    assert_eq!(
        Client::frames(&fx),
        vec![json!({
            "type": "wordle.progress", "channel_id": VOICE, "puzzle_date": "2026-11-05",
            "progress": {
                "userId": ME, "name": "Fixture Me", "avatar": null, "guesses": ["crane"],
                "streak": 0, "finished": false, "missed": false,
            },
        })]
    );
    let marks: Vec<&str> = client.view.wordle.board[0].iter().map(|t| t.mark).collect();
    assert_eq!(marks, ["absent", "absent", "correct", "absent", "correct"]);
}

#[test]
fn a_word_off_the_list_should_shake_and_not_count() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    for letter in "qqqqq".chars() {
        client.apps(AppsCommand::WordleKey(letter));
    }
    let fx = client.apps(AppsCommand::WordleEnter);
    assert!(Client::frames(&fx).is_empty());
    assert_eq!(
        (
            client.view.wordle.notice.as_str(),
            client.view.wordle.invalid
        ),
        ("Not in word list.", 1)
    );
}

#[test]
fn hard_mode_should_refuse_a_guess_that_drops_a_green() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    client.apps(AppsCommand::WordleHardMode(true));
    for word in ["crane", "pilot"] {
        for letter in word.chars() {
            client.apps(AppsCommand::WordleKey(letter));
        }
        client.apps(AppsCommand::WordleEnter);
    }
    assert_eq!(client.view.wordle.notice, "A must stay in position 3.");
}

#[test]
fn a_solve_should_finish_with_a_streak() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    for letter in "slate".chars() {
        client.apps(AppsCommand::WordleKey(letter));
    }
    let fx = client.apps(AppsCommand::WordleEnter);
    let frame = &Client::frames(&fx)[0];
    assert_eq!(
        (
            &frame["progress"]["finished"],
            &frame["progress"]["streak"],
            client.view.wordle.status
        ),
        (&json!(true), &json!(1), "solved")
    );
}

#[test]
fn a_peers_progress_should_show_as_marks_only() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    let mut progress = broadcast("wordle-progress.json");
    if let AppEvent::WordleProgress { puzzle_date, .. } = &mut progress {
        "2026-11-05".clone_into(puzzle_date);
    }
    client.event(progress, NOW);
    let peer = &client.view.wordle.players[0];
    assert_eq!(
        (peer.user_id.as_str(), peer.rows[0].clone()),
        (
            PEER,
            vec!["absent", "absent", "correct", "absent", "correct"]
        )
    );
}

#[test]
fn hints_should_unlock_in_order() {
    let mut client = Client::in_call();
    wordle_open(&mut client);
    client.apps(AppsCommand::WordleHint);
    client.apps(AppsCommand::WordleHint);
    let values: Vec<&str> = client
        .view
        .wordle
        .hints
        .iter()
        .map(|h| h.value.as_str())
        .collect();
    assert_eq!(values, ["S", "2"]);
}

#[test]
fn leaving_the_call_should_stop_every_clip_and_the_stream() {
    let mut client = Client::in_call();
    client.event(broadcast("lt-snapshot.json"), NOW);
    let fx = client.input(Input::User(Command::LeaveVoice));
    let audio = Client::audio(&fx);
    assert!(audio.contains(&AudioCommand::StopAll) && audio.contains(&AudioCommand::StreamStop));
}
