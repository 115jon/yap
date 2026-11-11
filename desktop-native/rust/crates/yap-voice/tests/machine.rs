//! `CallMachine` table tests: typed inputs in, effects out. No network, no
//! WebRTC.

use std::time::Duration;

use serde_json::{json, Value};
use yap_protocol::rest::{VoiceAdmission, VoiceAdmissionRequest};
use yap_realtime::StopReason;
use yap_voice::layers::{LAYER_DOWNGRADE_DELAY, LAYER_SWITCH_SETTLE, LAYER_UPGRADE_DELAY};
use yap_voice::wire::{Participant, RoomState, Track};
use yap_voice::{
    CallMachine, CallState, CallTarget, CameraPublished, Effect, Input, Kind, LayerUpdate,
    MediaEvent, MediaState, Member, Notice, Playback, Processing, Published, PullFailure,
    RenderSize, RoomEvent, Sensitivity, Socket, SocketSpec, SpatialLocal, SpatialState, Timer,
    UserCommand, VideoIntent, VoiceEvent, CREDENTIAL_REFRESH, LEAVE_SEND_WINDOW,
    MAX_REJECTED_PULLS, PULL_RETRY_AFTER_MAX, PULL_RETRY_BACKOFF, PULL_RETRY_MAX,
    REMOTE_RECONNECT_GRACE, SILENCE_DELAY_MS,
};

const ME: &str = "me-1";

fn target() -> CallTarget {
    CallTarget {
        server_id: "s1".into(),
        channel_id: "c1".into(),
        me_name: "Jon".into(),
        me_username: "jon".into(),
        ..CallTarget::default()
    }
}

/// The join's `POST /api/voice/admission` reply.
fn admission_json() -> Value {
    json!({
        "access_mode": "authenticated", "room_slug": "voice-s1-c1", "expires_at": 1,
        "participant_id": ME, "room_ticket": "room-t", "voice_ticket": "voice-t",
        "media": {
            "participant_id": ME, "voice_token": "tok-1",
            "ice_servers": [{ "urls": ["turn:t.example:3478"], "username": "u", "credential": "p" }],
        },
    })
}

fn admission_of(value: Value) -> VoiceAdmission {
    serde_json::from_value(value).expect("an admission")
}

fn admission() -> VoiceAdmission {
    admission_of(admission_json())
}

fn published() -> Published {
    Published {
        session_id: "cap-1".into(),
        sfu_track_name: "my-sfu".into(),
    }
}

fn participant(id: &str, name: Option<&str>) -> Participant {
    Participant {
        id: id.into(),
        name: name.map(str::to_owned),
        ..Participant::default()
    }
}

fn room_state(participants: &[Participant]) -> RoomState {
    RoomState {
        participant_id: ME.into(),
        participants: Some(participants.to_vec()),
        voice_token: String::new(),
    }
}

fn track(pid: &str, alias: &str, sfu: &str) -> Track {
    Track {
        participant_id: pid.into(),
        track_name: alias.into(),
        sfu_track_name: sfu.into(),
        session_id: format!("session-{pid}"),
        kind: "audio".into(),
        simulcast_rids: Vec::new(),
    }
}

/// A simulcast camera (`f`, `h`, `q`).
fn camera(pid: &str, sfu: &str) -> Track {
    Track {
        kind: "video".into(),
        simulcast_rids: ["f", "h", "q"].map(str::to_owned).to_vec(),
        ..track(pid, &format!("cam-video-{pid}"), sfu)
    }
}

/// A single-layer share.
fn share(pid: &str, sfu: &str) -> Track {
    Track {
        kind: "video".into(),
        ..track(pid, &format!("screen-video-{pid}"), sfu)
    }
}

fn voice_ready(tracks: Vec<Track>) -> Input {
    Input::Voice(VoiceEvent::Ready {
        tracks,
        speaking: Vec::new(),
    })
}

fn connected() -> Input {
    Input::Media(MediaEvent::State(MediaState::Connected))
}

fn done(sfu: &str, mid: &str) -> Input {
    Input::PullDone {
        sfu_track_name: sfu.into(),
        mid: Some(mid.into()),
    }
}

fn sends(fx: &[Effect], socket: Socket) -> Vec<Value> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Send(s, text) if *s == socket => {
                Some(serde_json::from_str(text).expect("frames are JSON"))
            }
            _ => None,
        })
        .collect()
}

fn ops(fx: &[Effect], socket: Socket) -> Vec<i64> {
    sends(fx, socket)
        .iter()
        .map(|f| f["op"].as_i64().expect("op"))
        .collect()
}

fn pulls(fx: &[Effect]) -> Vec<String> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Pull(r) => Some(r.sfu_track_name.clone()),
            _ => None,
        })
        .collect()
}

fn closes(fx: &[Effect]) -> Vec<Vec<String>> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Close(mids) => Some(mids.clone()),
            _ => None,
        })
        .collect()
}

fn last_roster(fx: &[Effect]) -> Option<Vec<Member>> {
    fx.iter().rev().find_map(|e| match e {
        Effect::Notify(Notice::Roster(r)) => Some(r.clone()),
        _ => None,
    })
}

fn states(fx: &[Effect]) -> Vec<CallState> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Notify(Notice::State(s)) => Some(*s),
            _ => None,
        })
        .collect()
}

/// The credential refreshes armed: (timer, delay).
fn refreshes(fx: &[Effect]) -> Vec<(Timer, Duration)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::ArmTimer { timer, after }
                if matches!(timer, Timer::RefreshCredentials { .. }) =>
            {
                Some((timer.clone(), *after))
            }
            _ => None,
        })
        .collect()
}

fn ends(fx: &[Effect]) -> Vec<Option<String>> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::End(reason) => Some(reason.clone()),
            _ => None,
        })
        .collect()
}

fn grace_timer(fx: &[Effect]) -> Timer {
    fx.iter()
        .find_map(|e| match e {
            Effect::ArmTimer { timer, after } if *after == REMOTE_RECONNECT_GRACE => {
                Some(timer.clone())
            }
            _ => None,
        })
        .expect("a grace timer")
}

fn names(roster: &[Member]) -> Vec<(&str, &str)> {
    roster
        .iter()
        .map(|m| (m.id.as_str(), m.name.as_str()))
        .collect()
}

/// A joined call with `ana` (one pulled mic, mid `m-ana`) in the room.
fn joined() -> CallMachine {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    m.handle(Input::Published(Ok(published())));
    m.handle(Input::Room(RoomEvent::Ready(room_state(&[participant(
        "ana",
        Some("Ana"),
    )]))));
    m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    m.handle(connected());
    m.handle(done("sfu-ana", "m-ana"));
    assert_eq!(m.state(), CallState::Joined);
    m
}

fn json_of(request: &VoiceAdmissionRequest) -> Value {
    serde_json::to_value(request).expect("serializes")
}

#[test]
fn start_should_request_one_admission_for_both_sockets_and_media() {
    let mut m = CallMachine::new(target());
    let fx = m.handle(Input::Start);
    assert_eq!(
        fx,
        vec![
            Effect::Notify(Notice::State(CallState::Connecting)),
            Effect::Notify(Notice::Status("Getting tickets")),
            Effect::RequestAdmission(VoiceAdmissionRequest::join("s1", "c1", "voice-s1-c1")),
        ]
    );
    let Effect::RequestAdmission(request) = &fx[2] else {
        unreachable!()
    };
    assert_eq!(
        json_of(request),
        json!({
            "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
            "sockets": ["room", "voice"], "media": true,
        })
    );
}

/// The effects of a joined admission.
fn admitted() -> Vec<Effect> {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())))
}

fn open_sockets(fx: &[Effect]) -> (&SocketSpec, &SocketSpec) {
    fx.iter()
        .find_map(|e| match e {
            Effect::OpenSockets { room, voice } => Some((&**room, &**voice)),
            _ => None,
        })
        .expect("OpenSockets")
}

#[test]
fn each_socket_should_reconnect_on_an_admission_for_its_own_ticket() {
    let fx = admitted();
    let (room, voice) = open_sockets(&fx);
    assert_eq!(
        json_of(&room.reconnect),
        json!({
            "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
            "sockets": ["room"], "media": false,
        })
    );
    assert_eq!(
        json_of(&voice.reconnect),
        json!({
            "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1",
            "sockets": ["voice"], "media": false, "participantId": ME,
        })
    );
}

#[test]
fn the_admission_should_arm_the_credential_refresh_for_45_minutes() {
    let fx = admitted();
    assert_eq!(CREDENTIAL_REFRESH, Duration::from_secs(45 * 60));
    assert_eq!(refreshes(&fx).len(), 1);
    assert_eq!(refreshes(&fx)[0].1, CREDENTIAL_REFRESH);
}

#[test]
fn an_admission_without_media_should_end_the_call() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let mut held = admission_json();
    held.as_object_mut().expect("object").remove("media");
    let fx = m.handle(Input::Admitted(Ok(admission_of(held))));
    assert_eq!(
        ends(&fx),
        vec![Some("The call admission was incomplete.".into())]
    );
    assert!(!fx.iter().any(|e| matches!(e, Effect::OpenSockets { .. })));
}

#[test]
fn an_admission_without_the_voice_ticket_should_end_the_call() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let mut partial = admission_json();
    partial
        .as_object_mut()
        .expect("object")
        .remove("voice_ticket");
    let fx = m.handle(Input::Admitted(Ok(admission_of(partial))));
    assert_eq!(
        ends(&fx),
        vec![Some("The call admission was incomplete.".into())]
    );
}

#[test]
fn admission_should_open_sockets_then_media() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let fx = m.handle(Input::Admitted(Ok(admission())));
    assert_eq!(fx[0], Effect::Notify(Notice::Status("Joining")));
    let (room, voice) = open_sockets(&fx);
    assert_eq!(room.path, "/api/channels/voice-s1-c1/ws?v=1");
    assert_eq!(room.first_ticket, "room-t");
    assert_eq!(room.identify["op"], 0);
    assert_eq!(room.identify["d"]["supports_voice_state_deltas"], true);
    assert_eq!(voice.path, "/api/channels/voice-s1-c1/voice?v=1");
    assert_eq!(voice.first_ticket, "voice-t");
    assert_eq!(
        voice.identify,
        json!({ "op": 100, "d": { "participant_id": ME, "voice_token": "tok-1" } })
    );
    let media = fx
        .iter()
        .find_map(|e| match e {
            Effect::OpenMedia(media) => Some(media),
            _ => None,
        })
        .expect("OpenMedia");
    assert_eq!(media.stream_id, "cam-me-1");
    assert_eq!(media.ice_servers[0].urls, vec!["turn:t.example:3478"]);
    assert_eq!(media.ice_servers[0].password, "p");
}

#[test]
fn join_should_announce_on_voice_ready_and_pull_only_after_media_connects() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    let fx = m.handle(Input::Published(Ok(published())));
    assert_eq!(names(&last_roster(&fx).expect("roster")), vec![(ME, "Jon")]);

    let fx = m.handle(Input::Room(RoomEvent::Ready(room_state(&[participant(
        "ana",
        Some("Ana"),
    )]))));
    assert_eq!(sends(&fx, Socket::Room)[0]["d"]["self_mute"], false);
    // Our voice state, then our spatial switches (useSpatialAudioSync).
    assert_eq!(ops(&fx, Socket::Room), vec![15, 15]);
    // The room's Ready is `joined` (STATE_MACHINE.md): before media is up.
    assert_eq!(states(&fx), vec![CallState::Joined]);
    assert!(fx.contains(&Effect::Notify(Notice::Connected)));

    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    let announce = &sends(&fx, Socket::Voice)[0];
    assert_eq!(announce["op"], 12);
    assert_eq!(
        announce["d"]["tracks"][0],
        json!({ "participant_id": ME, "track_name": "cam-audio-me-1",
                "sfu_track_name": "my-sfu", "session_id": "cap-1", "kind": "audio" })
    );
    assert!(pulls(&fx).is_empty(), "no pull before media is up");

    let fx = m.handle(connected());
    assert_eq!(
        fx,
        vec![
            Effect::ArmTimer {
                timer: Timer::MediaStable { token: 2 },
                after: yap_voice::REBUILD_STABLE_AFTER,
            },
            Effect::Notify(Notice::Status("Connected")),
            Effect::Pull(yap_voice::Remote {
                participant_id: "ana".into(),
                track_name: "mic-ana".into(),
                sfu_track_name: "sfu-ana".into(),
                session_id: "session-ana".into(),
                kind: Kind::Audio,
                preferred_rid: None,
                fresh: false,
            }),
        ]
    );
}

#[test]
fn a_second_ready_while_joined_should_rejoin_without_a_connect() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Ready(room_state(&[participant(
        "ana",
        Some("Ana"),
    )]))));
    assert!(fx.contains(&Effect::Notify(Notice::Rejoined)));
    assert!(!fx.contains(&Effect::Notify(Notice::Connected)));
    assert_eq!(ops(&fx, Socket::Room), vec![15, 15]);
}

#[test]
fn a_voice_socket_drop_should_not_change_the_phase() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Disconnected));
    assert!(states(&fx).is_empty());
    assert!(fx.contains(&Effect::Notify(Notice::Status("Reconnecting"))));
}

#[test]
fn a_media_drop_should_reconnect_and_its_return_should_restore() {
    let mut m = joined();
    let fx = m.handle(Input::Media(MediaEvent::State(MediaState::Disconnected)));
    assert_eq!(states(&fx), vec![CallState::Reconnecting]);
    let fx = m.handle(connected());
    assert_eq!(states(&fx), vec![CallState::Joined]);
    assert!(fx.contains(&Effect::Notify(Notice::Status("Connected"))));
}

#[test]
fn pull_should_skip_our_own_tracks_and_video_nobody_shows() {
    let mut m = joined();
    let video = camera("bob", "sfu-bob-video");
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bob".into(),
        tracks: vec![
            video,
            track(ME, "cam-audio-me-1", "my-sfu"),
            track("", "mic-bob", "sfu-bob"),
        ],
    }));
    assert_eq!(pulls(&fx), vec!["sfu-bob"]);
}

#[test]
fn deltas_should_keep_names_from_full_states() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: Participant {
            id: "ana".into(),
            self_mute: Some(true),
            self_deaf: Some(true),
            ..Participant::default()
        },
    }));
    let roster = last_roster(&fx).expect("roster");
    let ana = roster.iter().find(|r| r.id == "ana").expect("ana row");
    assert_eq!(
        (ana.name.as_str(), ana.muted, ana.deafened),
        ("Ana", true, true)
    );
}

#[test]
fn delta_after_a_leave_should_be_dropped() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: true,
        participant: participant("ana", None),
    }));
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: participant("ana", None),
    }));
    assert_eq!(names(&m.roster()), vec![(ME, "Jon")]);
}

#[test]
fn delta_for_an_unknown_participant_should_be_dropped() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: participant("zed", None),
    }));
    assert!(!m.roster().iter().any(|r| r.id == "zed"));
}

#[test]
fn a_full_state_should_create_a_row_with_camera_and_share_flags() {
    let mut m = joined();
    let mut zed = participant("zed", Some("Zed"));
    zed.self_video = Some(true);
    zed.self_stream = Some(true);
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: zed,
    }));
    let roster = m.roster();
    let row = roster.iter().find(|r| r.id == "zed").expect("zed row");
    assert_eq!((row.video, row.stream), (true, true));
}

#[test]
fn client_disconnect_should_keep_row_and_pull_when_they_reannounce_in_grace() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    }));
    let timer = grace_timer(&fx);
    assert!(closes(&fx).is_empty());

    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "mic-ana", "sfu-ana")],
    }));
    assert!(closes(&fx).is_empty(), "same SFU name keeps its pull");
    assert!(pulls(&fx).is_empty(), "already pulled");

    let fx = m.handle(Input::Timer(timer));
    assert!(
        closes(&fx).is_empty(),
        "what was offered again stays: {fx:?}"
    );
    assert!(m.roster().iter().any(|r| r.id == "ana"));
}

#[test]
fn grace_should_last_three_seconds() {
    assert_eq!(REMOTE_RECONNECT_GRACE.as_millis(), 3000);
}

#[test]
fn client_disconnect_should_close_pulls_but_keep_the_row_when_grace_expires() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    }));
    let fx = m.handle(Input::Timer(grace_timer(&fx)));
    assert_eq!(closes(&fx), vec![vec!["m-ana".to_owned()]]);
    // Only the room decides who left.
    assert!(m.roster().iter().any(|r| r.id == "ana"));
    assert!(m.tracks().pulled_names().is_empty());
}

#[test]
fn grace_expiry_should_drop_only_tracks_not_offered_again() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "screen-audio-ana", "sfu-ana-sa")],
    }));
    m.handle(done("sfu-ana-sa", "m-sa"));
    let timer = grace_timer(&m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    })));
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "mic-ana", "sfu-ana")],
    }));
    let fx = m.handle(Input::Timer(timer));
    assert_eq!(closes(&fx), vec![vec!["m-sa".to_owned()]]);
    assert_eq!(m.tracks().pulled_names(), vec!["mic-ana"]);
}

#[test]
fn client_disconnect_should_clear_speaking_at_once() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Speaking {
        participant_id: "ana".into(),
        speaking: true,
    }));
    let fx = m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    }));
    let roster = last_roster(&fx).expect("roster");
    assert!(roster.iter().any(|r| r.id == "ana" && !r.speaking));
}

#[test]
fn restarted_grace_should_ignore_the_earlier_timer() {
    let mut m = joined();
    let disconnect = || {
        Input::Voice(VoiceEvent::ClientDisconnect {
            participant_id: "ana".into(),
        })
    };
    let first = grace_timer(&m.handle(disconnect()));
    let second = grace_timer(&m.handle(disconnect()));
    assert!(m.handle(Input::Timer(first)).is_empty());
    assert_eq!(closes(&m.handle(Input::Timer(second))).len(), 1);
}

#[test]
fn room_leave_should_close_pull_and_drop_row_at_once() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: true,
        participant: participant("ana", None),
    }));
    assert_eq!(closes(&fx), vec![vec!["m-ana".to_owned()]]);
    assert_eq!(names(&last_roster(&fx).expect("roster")), vec![(ME, "Jon")]);
}

#[test]
fn stop_tracks_should_close_only_the_named_tracks() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "screen-audio-ana", "sfu-ana-screen")],
    }));
    m.handle(done("sfu-ana-screen", "m-screen"));
    let fx = m.handle(Input::Voice(VoiceEvent::StopTracks {
        participant_id: "ana".into(),
        names: vec!["screen-audio-ana".into()],
    }));
    assert_eq!(closes(&fx), vec![vec!["m-screen".to_owned()]]);
    assert_eq!(m.tracks().pulled_names(), vec!["mic-ana"]);
}

#[test]
fn a_share_offering_its_audio_should_mark_the_roster_until_withdrawn() {
    let mut m = joined();
    let shares_audio = |roster: &[Member]| roster.iter().any(|r| r.id == "ana" && r.screen_audio);
    assert!(!shares_audio(&m.roster()));
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "screen-audio-ana", "sfu-ana-sa")],
    }));
    assert!(shares_audio(&last_roster(&fx).expect("roster")));
    let fx = m.handle(Input::Voice(VoiceEvent::StopTracks {
        participant_id: "ana".into(),
        names: vec!["screen-audio-ana".into()],
    }));
    assert!(!shares_audio(&last_roster(&fx).expect("roster")));
}

#[test]
fn a_share_without_audio_should_not_mark_the_roster() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![Track {
            kind: "video".into(),
            ..track("ana", "screen-video-ana", "sfu-ana-sv")
        }],
    }));
    assert!(m.roster().iter().any(|r| r.id == "ana" && !r.screen_audio));
}

#[test]
fn stop_tracks_without_names_should_close_all_of_the_sender() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::StopTracks {
        participant_id: "ana".into(),
        names: Vec::new(),
    }));
    assert_eq!(closes(&fx), vec![vec!["m-ana".to_owned()]]);
}

#[test]
fn republish_under_a_new_sfu_name_should_replace_the_pull() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "mic-ana", "sfu-ana-2")],
    }));
    assert_eq!(closes(&fx), vec![vec!["m-ana".to_owned()]]);
    assert_eq!(pulls(&fx), vec!["sfu-ana-2"]);
}

#[test]
fn reidentify_should_reannounce_and_keep_still_present_pulls() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bob".into(),
        tracks: vec![track("bob", "mic-bob", "sfu-bob")],
    }));
    m.handle(done("sfu-bob", "m-bob"));

    m.handle(Input::Voice(VoiceEvent::Disconnected));

    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert_eq!(closes(&fx), vec![vec!["m-bob".to_owned()]]);
    assert_eq!(ops(&fx, Socket::Voice), vec![12]);
    assert!(pulls(&fx).is_empty(), "ana's pull is kept, not redone");
    assert!(fx.contains(&Effect::Notify(Notice::Status("Connected"))));
    assert_eq!(m.tracks().pulled_names(), vec!["mic-ana"]);
}

#[test]
fn room_resume_should_rebuild_roster_and_take_the_fresh_token() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::Disconnected));
    assert_eq!(m.state(), CallState::Reconnecting);
    let fx = m.handle(Input::Room(RoomEvent::Resumed(RoomState {
        participant_id: String::new(),
        participants: Some(vec![participant("bob", Some("Bob"))]),
        voice_token: "tok-2".into(),
    })));
    assert_eq!(
        names(&last_roster(&fx).expect("roster")),
        vec![("bob", "Bob"), (ME, "Jon")]
    );
    assert!(fx.contains(&Effect::SetVoiceToken("tok-2".into())));
    assert!(fx.contains(&Effect::SetVoiceIdentify(json!({
        "op": 100, "d": { "participant_id": ME, "voice_token": "tok-2" }
    }))));
    assert_eq!(states(&fx), vec![CallState::Joined]);
}

#[test]
fn fresh_room_participant_after_a_drop_should_end_the_call() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Ready(RoomState {
        participant_id: "someone-new".into(),
        ..room_state(&[])
    })));
    assert_eq!(ends(&fx), vec![Some(yap_voice::CALL_RESET.into())]);
}

#[test]
fn fresh_room_participant_after_a_drop_should_leave_the_room_before_ending() {
    // The room identified a new participant for us: unless it hears the
    // leave, it holds that seat for a resume (a ghost for 120 s).
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Ready(RoomState {
        participant_id: "someone-new".into(),
        ..room_state(&[])
    })));
    assert_eq!(ops(&fx, Socket::Room), vec![11]);
    assert_eq!(ops(&fx, Socket::Voice), vec![11]);
    let leave = fx
        .iter()
        .position(|e| matches!(e, Effect::Send(Socket::Room, _)))
        .expect("op 11");
    let end = fx
        .iter()
        .position(|e| matches!(e, Effect::End(_)))
        .expect("end");
    assert!(leave < end, "the leave goes out before the sockets close");
}

#[test]
fn a_failed_media_connection_should_leave_the_room_before_ending() {
    let mut m = joined();
    for _ in 0..yap_voice::MAX_REBUILDS {
        flap(&mut m);
    }
    let fx = m.handle(failed());
    assert_eq!(ends(&fx), vec![Some("Media connection failed".into())]);
    assert_eq!(ops(&fx, Socket::Room), vec![11]);
}

#[test]
fn a_replaced_session_should_end_without_a_leave() {
    // The room already ended it (4010): nothing to tell it.
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Stopped(StopReason::Replaced {
        code: 4010,
        reason: "Voice moved to another device".into(),
    })));
    assert_eq!(
        ends(&fx),
        vec![Some("You joined this call somewhere else.".into())]
    );
    assert!(ops(&fx, Socket::Room).is_empty());
}

#[test]
fn admission_rejected_should_stop_the_call() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Stopped(
        StopReason::AdmissionRejected {
            reason: String::new(),
        },
    )));
    assert_eq!(ends(&fx), vec![Some("The call refused this join.".into())]);
    assert_eq!(m.state(), CallState::Idle);
}

#[test]
fn session_replaced_by_a_move_should_end_the_call_quietly() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Stopped(StopReason::Replaced {
        code: 4010,
        reason: yap_protocol::VOICE_MOVED_CLOSE_REASON.into(),
    })));
    assert_eq!(ends(&fx), vec![None]);
}

#[test]
fn voice_error_4008_or_4010_should_stop_with_its_message() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Rejected {
        message: "Session replaced".into(),
    }));
    assert_eq!(ends(&fx), vec![Some("Session replaced".into())]);
    assert!(m.handle(connected()).is_empty(), "nothing after the end");
}

#[test]
fn joined_leave_should_stop_media_wait_the_send_window_then_disconnect() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert_eq!(
        fx,
        vec![
            Effect::StopLocalMedia,
            Effect::ArmTimer {
                timer: Timer::Linger,
                after: LEAVE_SEND_WINDOW,
            },
            Effect::Notify(Notice::State(CallState::Leaving)),
        ]
    );
    assert_eq!(LEAVE_SEND_WINDOW.as_millis(), 100);

    let fx = m.handle(Input::Timer(Timer::Linger));
    // Op 11 on the room socket, then the voice socket, then the end.
    let order: Vec<(Socket, i64)> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::Send(socket, text) => {
                let frame: Value = serde_json::from_str(text).expect("JSON");
                Some((*socket, frame["op"].as_i64().expect("op")))
            }
            _ => None,
        })
        .collect();
    assert_eq!(order, vec![(Socket::Room, 11), (Socket::Voice, 11)]);
    assert_eq!(ends(&fx), vec![None]);
    assert_eq!(states(&fx), vec![CallState::Idle]);
}

#[test]
fn leave_before_the_room_is_ready_should_disconnect_at_once() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert_eq!(fx[0], Effect::StopLocalMedia);
    assert_eq!(ops(&fx, Socket::Room), vec![11]);
    assert_eq!(ops(&fx, Socket::Voice), vec![11]);
    assert_eq!(ends(&fx), vec![None]);
}

#[test]
fn leave_should_not_send_stop_tracks() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::Leave));
    let fx = m.handle(Input::Timer(Timer::Linger));
    assert!(!ops(&fx, Socket::Voice).contains(&13));
}

#[test]
fn mute_should_disable_the_mic_and_send_voice_state() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::SetMuted(true)));
    assert!(fx.contains(&Effect::SetMicEnabled(false)));
    assert_eq!(sends(&fx, Socket::Room)[0]["op"], 15);
    assert_eq!(sends(&fx, Socket::Room)[0]["d"]["self_mute"], true);
    let roster = last_roster(&fx).expect("roster");
    assert!(roster.iter().any(|r| r.me && r.muted));
}

#[test]
fn deafen_should_silence_playout_and_send_self_deaf() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::SetDeafened(true)));
    assert!(fx.contains(&Effect::SetPlayoutEnabled(false)));
    let sent = &sends(&fx, Socket::Room)[0];
    assert_eq!(
        (sent["op"].clone(), sent["d"]["self_deaf"].clone()),
        (15.into(), true.into())
    );
    assert_eq!(sent["d"]["self_mute"], false);
    let roster = last_roster(&fx).expect("roster");
    assert!(roster.iter().any(|r| r.me && r.deafened && !r.muted));
    let fx = m.handle(Input::Command(UserCommand::SetDeafened(false)));
    assert!(fx.contains(&Effect::SetPlayoutEnabled(true)));
}

#[test]
fn deafen_before_publish_should_apply_once_published() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Command(UserCommand::SetDeafened(true)));
    m.handle(Input::Admitted(Ok(admission())));
    let fx = m.handle(Input::Published(Ok(published())));
    assert!(fx.contains(&Effect::SetPlayoutEnabled(false)));
}

#[test]
fn stale_grace_timer_after_leave_should_be_ignored() {
    let mut m = joined();
    let timer = grace_timer(&m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    })));
    m.handle(Input::Command(UserCommand::Leave));
    assert!(m.handle(Input::Timer(timer.clone())).is_empty());
    m.handle(Input::Timer(Timer::Linger));
    assert!(m.handle(Input::Timer(timer)).is_empty());
}

#[test]
fn pull_finishing_after_its_track_retired_should_close_its_mid() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bob".into(),
        tracks: vec![track("bob", "mic-bob", "sfu-bob")],
    }));
    assert_eq!(pulls(&fx), vec!["sfu-bob"]);
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: true,
        participant: participant("bob", None),
    }));
    let fx = m.handle(done("sfu-bob", "m-bob"));
    assert_eq!(closes(&fx), vec![vec!["m-bob".to_owned()]]);
}

#[test]
fn admission_failure_should_end_with_its_reason() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let fx = m.handle(Input::Admitted(Err(
        "call admission: session expired".into()
    )));
    assert_eq!(
        ends(&fx),
        vec![Some("call admission: session expired".into())]
    );
}

fn failed() -> Input {
    Input::Media(MediaEvent::State(MediaState::Failed))
}

fn rebuilds(fx: &[Effect]) -> usize {
    fx.iter()
        .filter(|e| matches!(e, Effect::RebuildMedia(_)))
        .count()
}

/// The rebuild timer a failure armed, with its backoff.
fn rebuild_timer(fx: &[Effect]) -> Option<(Timer, Duration)> {
    fx.iter().find_map(|e| match e {
        Effect::ArmTimer { timer, after } if matches!(timer, Timer::Rebuild { .. }) => {
            Some((timer.clone(), *after))
        }
        _ => None,
    })
}

/// The stability timer a connect armed.
fn stable_timer(fx: &[Effect]) -> Option<(Timer, Duration)> {
    fx.iter().find_map(|e| match e {
        Effect::ArmTimer { timer, after } if matches!(timer, Timer::MediaStable { .. }) => {
            Some((timer.clone(), *after))
        }
        _ => None,
    })
}

/// One failure, its backoff over, the new session published and connected.
/// Returns the failure's backoff and the connect's effects.
fn flap(m: &mut CallMachine) -> (Duration, Vec<Effect>) {
    let (timer, after) = rebuild_timer(&m.handle(failed())).expect("a rebuild is armed");
    assert_eq!(rebuilds(&m.handle(Input::Timer(timer))), 1);
    m.handle(Input::Published(Ok(published())));
    (after, m.handle(connected()))
}

#[test]
fn media_failure_should_rebuild_the_sfu_session_after_a_backoff_instead_of_ending() {
    let mut m = joined();
    let fx = m.handle(failed());
    assert_eq!(rebuilds(&fx), 0, "not inline");
    assert!(ends(&fx).is_empty());
    assert_eq!(m.state(), CallState::Reconnecting);
    let (timer, after) = rebuild_timer(&fx).expect("armed");
    assert_eq!(after, yap_voice::REBUILD_BACKOFF);
    let fx = m.handle(Input::Timer(timer));
    assert_eq!(rebuilds(&fx), 1);
    assert!(ends(&fx).is_empty());
}

#[test]
fn a_rebuilt_session_should_announce_the_new_capability_and_pull_again() {
    let mut m = joined();
    let (timer, _) = rebuild_timer(&m.handle(failed())).expect("armed");
    m.handle(Input::Timer(timer));
    let fx = m.handle(Input::Published(Ok(Published {
        session_id: "cap-2".into(),
        sfu_track_name: "my-sfu-2".into(),
    })));
    let announced = sends(&fx, Socket::Voice);
    assert_eq!(announced[0]["d"]["tracks"][0]["session_id"], "cap-2");
    assert!(pulls(&fx).is_empty(), "pulls wait for the new connection");
    let fx = m.handle(connected());
    assert_eq!(pulls(&fx), vec!["sfu-ana".to_owned()]);
    assert_eq!(m.state(), CallState::Joined);
}

#[test]
fn a_connection_that_keeps_failing_should_end_the_call_after_the_capped_backoffs() {
    let mut m = joined();
    let waits: Vec<Duration> = (0..yap_voice::MAX_REBUILDS)
        .map(|_| flap(&mut m).0)
        .collect();
    assert_eq!(
        waits,
        [1, 2, 4].map(Duration::from_secs),
        "doubling, one rebuild each"
    );
    let fx = m.handle(failed());
    assert!(rebuild_timer(&fx).is_none());
    assert_eq!(rebuilds(&fx), 0);
    assert_eq!(ends(&fx), vec![Some("Media connection failed".into())]);
}

#[test]
fn a_flapping_link_should_not_get_its_rebuilds_back_on_every_connect() {
    let mut m = joined();
    for _ in 0..yap_voice::MAX_REBUILDS {
        let (_, connect) = flap(&mut m);
        // A connect that drops again before the stability window.
        assert_eq!(
            stable_timer(&connect).expect("armed").1,
            yap_voice::REBUILD_STABLE_AFTER
        );
    }
    let fx = m.handle(failed());
    assert_eq!(ends(&fx), vec![Some("Media connection failed".into())]);
}

#[test]
fn a_connection_that_stayed_up_for_the_stability_window_should_get_its_rebuilds_back() {
    let mut m = joined();
    for _ in 0..yap_voice::MAX_REBUILDS {
        let (_, connect) = flap(&mut m);
        let (timer, after) = stable_timer(&connect).expect("armed");
        assert_eq!(after, Duration::from_secs(60));
        m.handle(Input::Timer(timer));
    }
    // Every window held: the next failure starts from the short backoff.
    let fx = m.handle(failed());
    assert!(ends(&fx).is_empty());
    assert_eq!(
        rebuild_timer(&fx).expect("armed").1,
        yap_voice::REBUILD_BACKOFF
    );
}

#[test]
fn a_stability_timer_that_a_drop_outlived_should_not_reset_the_count() {
    let mut m = joined();
    let (_, connect) = flap(&mut m);
    let (stale, _) = stable_timer(&connect).expect("armed");
    // Drops and returns: the first window is void.
    m.handle(Input::Media(MediaEvent::State(MediaState::Disconnected)));
    m.handle(connected());
    m.handle(Input::Timer(stale));
    let (_, after) = rebuild_timer(&m.handle(failed())).expect("armed");
    assert_eq!(after, Duration::from_secs(2), "one rebuild already spent");
}

#[test]
fn a_second_failure_during_a_rebuild_should_not_rebuild_again_or_end_the_call() {
    let mut m = joined();
    // During the backoff.
    let (timer, _) = rebuild_timer(&m.handle(failed())).expect("armed");
    let fx = m.handle(failed());
    assert!(ends(&fx).is_empty());
    assert!(rebuild_timer(&fx).is_none());
    assert_eq!(rebuilds(&fx), 0);
    // Waiting for the new session.
    assert_eq!(rebuilds(&m.handle(Input::Timer(timer.clone()))), 1);
    let fx = m.handle(failed());
    assert!(ends(&fx).is_empty());
    assert!(rebuild_timer(&fx).is_none());
    assert_eq!(rebuilds(&fx), 0);
    // The timer fires once only.
    assert_eq!(rebuilds(&m.handle(Input::Timer(timer))), 0);
    // The new session counts again as soon as it exists.
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(failed());
    assert!(ends(&fx).is_empty());
    assert_eq!(rebuild_timer(&fx).expect("armed").1, Duration::from_secs(2));
}

#[test]
fn a_failure_before_media_connected_once_should_end_the_call() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    let fx = m.handle(failed());
    assert_eq!(ends(&fx), vec![Some("Media connection failed".into())]);
}

#[test]
fn mute_then_unmute_should_toggle_our_roster_row() {
    let mut m = joined();
    let me_muted = |fx: &[Effect]| {
        last_roster(fx)
            .expect("a roster update")
            .iter()
            .find(|r| r.me)
            .map(|r| r.muted)
    };
    let fx = m.handle(Input::Command(UserCommand::SetMuted(true)));
    assert_eq!(me_muted(&fx), Some(true));
    // Later full states keep our flag.
    let fx = m.handle(Input::Room(RoomEvent::Resumed(room_state(&[participant(
        "ana",
        Some("Ana"),
    )]))));
    assert_eq!(me_muted(&fx), Some(true));
    let fx = m.handle(Input::Command(UserCommand::SetMuted(false)));
    assert_eq!(me_muted(&fx), Some(false));
    assert!(fx.contains(&Effect::SetMicEnabled(true)));
}

fn level(rms: f64, at_ms: u64) -> Input {
    Input::MicLevel { rms, at_ms }
}

fn speaking_sends(fx: &[Effect]) -> Vec<i64> {
    sends(fx, Socket::Voice)
        .iter()
        .filter(|f| f["op"] == 5)
        .map(|f| f["d"]["speaking"].as_i64().expect("speaking"))
        .collect()
}

fn me_speaking(fx: &[Effect]) -> Option<bool> {
    last_roster(fx)?.iter().find(|r| r.me).map(|r| r.speaking)
}

fn gains(fx: &[Effect]) -> Vec<(String, f64)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::SetTrackGain { mid, gain } => Some((mid.clone(), *gain)),
            _ => None,
        })
        .collect()
}

/// A joined call where `ana` is Yap user `u-ana` (mic on mid `m-ana`).
fn joined_with_user() -> CallMachine {
    let mut m = joined();
    let mut ana = participant("ana", Some("Ana"));
    ana.user_id = Some("u-ana".into());
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: ana,
    }));
    m
}

fn playback(output: f64, peers: &[(&str, f64)]) -> Input {
    Input::Command(UserCommand::SetPlayback(Playback {
        output,
        peers: peers.iter().map(|(u, g)| ((*u).to_owned(), *g)).collect(),
        ..Playback::default()
    }))
}

#[test]
fn a_mic_level_should_report_the_meter_position() {
    let mut m = joined();
    let fx = m.handle(level(1.0, 0));
    assert!(fx.contains(&Effect::Notify(Notice::MicLevel(60.0))));
}

#[test]
fn speech_should_light_our_row_and_send_speaking_on() {
    let mut m = joined();
    let fx = m.handle(level(10.0, 0));
    assert_eq!(
        (speaking_sends(&fx), me_speaking(&fx)),
        (vec![1], Some(true))
    );
}

#[test]
fn speaking_should_go_off_after_the_silence_delay() {
    let mut m = joined();
    m.handle(level(10.0, 0));
    assert!(speaking_sends(&m.handle(level(0.1, 50))).is_empty());
    let fx = m.handle(level(0.1, 50 + SILENCE_DELAY_MS));
    assert_eq!(
        (speaking_sends(&fx), me_speaking(&fx)),
        (vec![0], Some(false))
    );
}

#[test]
fn a_level_below_the_manual_threshold_should_not_count_as_speech() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetSensitivity(Sensitivity {
        auto: false,
        db: -10.0,
    })));
    assert!(speaking_sends(&m.handle(level(10.0, 0))).is_empty());
}

#[test]
fn muting_should_turn_speaking_off_at_once() {
    let mut m = joined();
    m.handle(level(10.0, 0));
    let fx = m.handle(Input::Command(UserCommand::SetMuted(true)));
    assert_eq!(
        (speaking_sends(&fx), me_speaking(&fx)),
        (vec![0], Some(false))
    );
}

#[test]
fn a_closed_transmit_gate_should_disable_the_mic_without_announcing_a_mute() {
    let mut m = joined();
    m.handle(level(10.0, 0));
    let fx = m.handle(Input::Command(UserCommand::SetTransmit(false)));
    assert!(fx.contains(&Effect::SetMicEnabled(false)));
    // Speaking goes off at once; nothing goes to the room (op 15).
    assert_eq!(speaking_sends(&fx), vec![0]);
    assert!(sends(&fx, Socket::Room).is_empty());
    // Speech behind the closed gate is not sent.
    assert!(speaking_sends(&m.handle(level(10.0, 100))).is_empty());
    let fx = m.handle(Input::Command(UserCommand::SetTransmit(true)));
    assert!(fx.contains(&Effect::SetMicEnabled(true)));
    assert_eq!(speaking_sends(&m.handle(level(10.0, 200))), vec![1]);
}

#[test]
fn opening_the_gate_while_muted_should_keep_the_mic_off() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetTransmit(false)));
    m.handle(Input::Command(UserCommand::SetMuted(true)));
    let fx = m.handle(Input::Command(UserCommand::SetTransmit(true)));
    assert!(fx.contains(&Effect::SetMicEnabled(false)));
    // Unmuting with the gate closed keeps it off too.
    m.handle(Input::Command(UserCommand::SetTransmit(false)));
    let fx = m.handle(Input::Command(UserCommand::SetMuted(false)));
    assert!(fx.contains(&Effect::SetMicEnabled(false)));
}

#[test]
fn the_same_gate_twice_should_do_nothing() {
    let mut m = joined();
    assert!(m
        .handle(Input::Command(UserCommand::SetTransmit(true)))
        .is_empty());
}

#[test]
fn speech_while_muted_should_not_be_sent() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetMuted(true)));
    assert!(speaking_sends(&m.handle(level(10.0, 0))).is_empty());
}

#[test]
fn a_reidentified_voice_socket_should_hear_that_we_are_speaking() {
    let mut m = joined();
    m.handle(level(10.0, 0));
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert_eq!(speaking_sends(&fx), vec![1]);
}

#[test]
fn a_user_volume_should_set_the_gain_of_their_track() {
    let mut m = joined_with_user();
    let fx = m.handle(playback(1.0, &[("u-ana", 0.5)]));
    assert_eq!(gains(&fx), vec![("m-ana".to_owned(), 0.5)]);
}

#[test]
fn the_output_volume_should_scale_every_track() {
    let mut m = joined_with_user();
    let fx = m.handle(playback(2.0, &[("u-ana", 0.75)]));
    assert_eq!(gains(&fx), vec![("m-ana".to_owned(), 1.5)]);
}

#[test]
fn an_unchanged_gain_should_not_be_set_again() {
    let mut m = joined_with_user();
    m.handle(playback(1.0, &[("u-ana", 0.0)]));
    assert!(gains(&m.handle(level(1.0, 0))).is_empty());
}

#[test]
fn a_volume_should_apply_once_the_room_names_the_user() {
    let mut m = joined();
    m.handle(playback(1.0, &[("u-ana", 0.0)]));
    let mut ana = participant("ana", None);
    ana.user_id = Some("u-ana".into());
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: ana,
    }));
    assert_eq!(gains(&fx), vec![("m-ana".to_owned(), 0.0)]);
}

#[test]
fn a_volume_should_apply_to_a_track_pulled_later() {
    let mut m = joined_with_user();
    m.handle(playback(1.0, &[("u-ana", 1.5)]));
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "mic-ana", "sfu-ana-2")],
    }));
    let fx = m.handle(done("sfu-ana-2", "m-ana-2"));
    assert_eq!(gains(&fx), vec![("m-ana-2".to_owned(), 1.5)]);
}

#[test]
fn processing_should_apply_to_the_published_mic() {
    let mut m = joined();
    let studio = Processing::from_settings(false, false, true, false);
    let fx = m.handle(Input::Command(UserCommand::SetProcessing(studio)));
    assert!(fx.contains(&Effect::SetProcessing(studio)));
}

#[test]
fn processing_chosen_before_the_mic_should_apply_once_it_is_published() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    let off = Processing::from_settings(false, true, false, false);
    assert!(m
        .handle(Input::Command(UserCommand::SetProcessing(off)))
        .is_empty());
    let fx = m.handle(Input::Published(Ok(published())));
    assert!(fx.contains(&Effect::SetProcessing(off)));
}

// Video: render demand, simulcast layers, parking, watching.

fn show(tiles: &[(&str, u32, u32)], stage: Option<&str>) -> Input {
    Input::Command(UserCommand::SetVideo(VideoIntent {
        demand: tiles
            .iter()
            .map(|(alias, width, height)| {
                (
                    (*alias).to_owned(),
                    RenderSize {
                        width: *width,
                        height: *height,
                    },
                )
            })
            .collect(),
        stage: stage.map(str::to_owned),
    }))
}

fn sinks(fx: &[Effect]) -> Vec<(String, Option<String>)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::SetVideoSink { mid, key } => Some((mid.clone(), key.clone())),
            _ => None,
        })
        .collect()
}

fn rids_pulled(fx: &[Effect]) -> Vec<Option<String>> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Pull(r) => Some(r.preferred_rid.clone()),
            _ => None,
        })
        .collect()
}

fn layer_timer(fx: &[Effect]) -> (Timer, std::time::Duration) {
    fx.iter()
        .find_map(|e| match e {
            Effect::ArmTimer {
                timer: timer @ Timer::Layer { .. },
                after,
            } => Some((timer.clone(), *after)),
            _ => None,
        })
        .expect("a layer timer")
}

fn updates(fx: &[Effect]) -> Vec<LayerUpdate> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::UpdateLayer(u) => Some(u.clone()),
            _ => None,
        })
        .collect()
}

const ANA_CAM: &str = "cam-video-ana";

/// Joined with Ana (user `u-ana`) publishing a simulcast camera.
fn with_camera() -> CallMachine {
    let mut m = joined_with_user();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-ana-cam")],
    }));
    assert!(pulls(&fx).is_empty(), "no tile shows it yet");
    m
}

/// `with_camera`, shown in a 640x360 grid tile and pulled on `m-cam`.
fn camera_shown() -> CallMachine {
    let mut m = with_camera();
    m.handle(show(&[(ANA_CAM, 640, 360)], None));
    m.handle(done("sfu-ana-cam", "m-cam"));
    m
}

#[test]
fn a_visible_camera_tile_should_pull_at_its_layer() {
    let mut m = with_camera();
    let fx = m.handle(show(&[(ANA_CAM, 640, 360)], None));
    assert_eq!(pulls(&fx), vec!["sfu-ana-cam"]);
    assert_eq!(rids_pulled(&fx), vec![Some("h".to_owned())]);
}

#[test]
fn a_full_stage_camera_should_pull_the_top_layer() {
    let mut m = with_camera();
    let fx = m.handle(show(&[(ANA_CAM, 1600, 900)], Some("ana")));
    assert_eq!(rids_pulled(&fx), vec![Some("f".to_owned())]);
}

#[test]
fn a_pulled_camera_should_route_its_frames_and_go_live() {
    let mut m = with_camera();
    m.handle(show(&[(ANA_CAM, 640, 360)], None));
    let fx = m.handle(done("sfu-ana-cam", "m-cam"));
    assert_eq!(
        sinks(&fx),
        vec![("m-cam".to_owned(), Some(ANA_CAM.to_owned()))]
    );
    let roster = last_roster(&fx).expect("roster");
    assert!(roster.iter().any(|r| r.id == "ana" && r.camera_live));
    assert!(gains(&fx).is_empty(), "video has no gain");
}

#[test]
fn a_hidden_camera_should_drop_frames_and_keep_its_pull() {
    let mut m = camera_shown();
    let fx = m.handle(show(&[], None));
    assert_eq!(sinks(&fx), vec![("m-cam".to_owned(), None)]);
    assert!(
        closes(&fx).is_empty(),
        "video pulls are never closed to re-pull"
    );
    assert_eq!(layer_timer(&fx).1, LAYER_DOWNGRADE_DELAY);
}

#[test]
fn a_bigger_tile_should_switch_up_after_the_upgrade_delay_then_settle() {
    let mut m = camera_shown();
    let fx = m.handle(show(&[(ANA_CAM, 1920, 1080)], Some("ana")));
    let (timer, after) = layer_timer(&fx);
    assert_eq!(after, LAYER_UPGRADE_DELAY);
    assert!(updates(&fx).is_empty(), "waits for the size to settle");

    let fx = m.handle(Input::Timer(timer));
    assert_eq!(
        updates(&fx),
        vec![LayerUpdate {
            mid: "m-cam".into(),
            sfu_track_name: "sfu-ana-cam".into(),
            session_id: "session-ana".into(),
            rid: "f".into(),
            switch_now: true,
        }]
    );
    let settle = fx
        .iter()
        .find_map(|e| match e {
            Effect::ArmTimer {
                timer: timer @ Timer::LayerSettle { .. },
                after,
            } if *after == LAYER_SWITCH_SETTLE => Some(timer.clone()),
            _ => None,
        })
        .expect("a settle timer");
    let fx = m.handle(Input::Timer(settle));
    let settled = updates(&fx);
    assert_eq!(settled.len(), 1);
    assert!(!settled[0].switch_now);
    assert_eq!(settled[0].rid, "f");
}

#[test]
fn a_resize_within_a_layer_should_not_switch() {
    let mut m = camera_shown();
    let fx = m.handle(show(&[(ANA_CAM, 700, 394)], None));
    assert!(fx.iter().all(|e| !matches!(
        e,
        Effect::ArmTimer {
            timer: Timer::Layer { .. },
            ..
        }
    )));
}

#[test]
fn a_superseded_layer_timer_should_do_nothing() {
    let mut m = camera_shown();
    let (up, _) = layer_timer(&m.handle(show(&[(ANA_CAM, 1920, 1080)], None)));
    m.handle(show(&[(ANA_CAM, 300, 169)], None));
    assert!(updates(&m.handle(Input::Timer(up))).is_empty());
}

#[test]
fn camera_off_should_park_the_pull_and_camera_on_should_show_it_again() {
    let mut m = camera_shown();
    let fx = m.handle(Input::Voice(VoiceEvent::StopTracks {
        participant_id: "ana".into(),
        names: vec![ANA_CAM.into()],
    }));
    assert!(closes(&fx).is_empty(), "parked, not closed");
    assert_eq!(sinks(&fx), vec![("m-cam".to_owned(), None)]);
    assert!(m.roster().iter().any(|r| r.id == "ana" && !r.camera_live));

    // Turned on again: the same SFU track, kept alive with black frames.
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-ana-cam")],
    }));
    assert!(pulls(&fx).is_empty() && closes(&fx).is_empty());
    assert_eq!(
        sinks(&fx),
        vec![("m-cam".to_owned(), Some(ANA_CAM.to_owned()))]
    );
}

#[test]
fn a_camera_on_a_new_sfu_track_should_be_pulled_afresh() {
    let mut m = camera_shown();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-ana-cam-2")],
    }));
    assert_eq!(closes(&fx), vec![vec!["m-cam".to_owned()]]);
    assert_eq!(pulls(&fx), vec!["sfu-ana-cam-2"]);
}

#[test]
fn a_share_should_wait_for_watch_then_pull_and_announce_it() {
    let mut m = joined_with_user();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![share("ana", "sfu-ana-share")],
    }));
    assert!(pulls(&fx).is_empty(), "screen video is opt-in");

    let fx = m.handle(Input::Command(UserCommand::SetWatched(
        ["ana".to_owned()].into(),
    )));
    assert_eq!(pulls(&fx), vec!["sfu-ana-share"]);
    assert_eq!(rids_pulled(&fx), vec![None], "a share has one layer");
    assert_eq!(
        sends(&fx, Socket::Voice),
        vec![json!({ "op": 106, "d": {
            "type": "stream.watch.start", "streamer_user_id": "u-ana",
        }})]
    );
}

#[test]
fn unwatching_a_share_should_hide_it_and_keep_its_pull() {
    let mut m = joined_with_user();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![share("ana", "sfu-ana-share")],
    }));
    m.handle(Input::Command(UserCommand::SetWatched(
        ["ana".to_owned()].into(),
    )));
    m.handle(show(&[("screen-video-ana", 1280, 720)], None));
    let fx = m.handle(done("sfu-ana-share", "m-share"));
    assert_eq!(
        sinks(&fx),
        vec![("m-share".to_owned(), Some("screen-video-ana".to_owned()))]
    );

    let fx = m.handle(Input::Command(UserCommand::SetWatched(Default::default())));
    assert_eq!(sinks(&fx), vec![("m-share".to_owned(), None)]);
    assert!(closes(&fx).is_empty(), "kept until the share ends");
    assert_eq!(
        sends(&fx, Socket::Voice)[0]["d"]["type"],
        "stream.watch.stop"
    );
}

#[test]
fn their_camera_on_stage_should_not_pull_their_share() {
    let mut m = joined_with_user();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![share("ana", "sfu-ana-share")],
    }));
    let fx = m.handle(show(&[("cam-video-ana", 1280, 720)], Some("ana")));
    assert!(pulls(&fx).is_empty());
}

#[test]
fn their_share_tile_on_stage_should_pull_it_unwatched() {
    let mut m = joined_with_user();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![share("ana", "sfu-ana-share")],
    }));
    let fx = m.handle(show(&[("screen-video-ana", 1280, 720)], Some("ana")));
    assert_eq!(pulls(&fx), vec!["sfu-ana-share"]);
}

#[test]
fn an_unwatched_share_showing_its_still_should_never_be_pulled() {
    // The still behind Watch stream comes with the gateway roster
    // (`stream_preview_url`); its tile draws no video surface, so the
    // intent never names the share and no transceiver opens or closes.
    let mut m = joined_with_user();
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![share("ana", "sfu-ana-share")],
    }));
    assert!(pulls(&fx).is_empty());
    for intent in [
        show(&[], None),
        show(&[("cam-video-ana", 320, 180)], None),
        show(&[("cam-video-bob", 640, 360)], Some("bob")),
        show(&[], None),
    ] {
        let fx = m.handle(intent);
        assert!(pulls(&fx).is_empty(), "{fx:#?}");
        assert!(closes(&fx).is_empty(), "{fx:#?}");
    }
}

#[test]
fn share_audio_should_be_silent_until_the_share_is_on_stage() {
    let mut m = joined_with_user();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "screen-audio-ana", "sfu-ana-sa")],
    }));
    let fx = m.handle(done("sfu-ana-sa", "m-sa"));
    assert_eq!(gains(&fx), vec![("m-sa".to_owned(), 0.0)]);
    let fx = m.handle(show(&[], Some("ana")));
    assert_eq!(gains(&fx), vec![("m-sa".to_owned(), 1.0)]);
}

#[test]
fn deafen_should_not_touch_video_routes() {
    let mut m = camera_shown();
    let fx = m.handle(Input::Command(UserCommand::SetDeafened(true)));
    assert!(sinks(&fx).is_empty());
}

#[test]
fn a_leaving_call_should_ignore_video_intent() {
    let mut m = with_camera();
    m.handle(Input::Command(UserCommand::Leave));
    assert!(m.handle(show(&[(ANA_CAM, 640, 360)], None)).is_empty());
}

fn camera_published(sfu: &str) -> Input {
    Input::CameraPublished(Ok(CameraPublished {
        sfu_track_name: sfu.into(),
        simulcast_rids: ["f", "h", "q"].map(str::to_owned).to_vec(),
    }))
}

fn camera_notices(fx: &[Effect]) -> Vec<(bool, Option<String>)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Notify(Notice::Camera { on, error }) => Some((*on, error.clone())),
            _ => None,
        })
        .collect()
}

fn publishes_camera(fx: &[Effect]) -> bool {
    fx.iter()
        .any(|e| matches!(e, Effect::PublishCamera { stream_id } if stream_id == "cam-me-1"))
}

/// The op 12 frames for our camera alias.
fn camera_announces(fx: &[Effect]) -> Vec<Value> {
    sends(fx, Socket::Voice)
        .into_iter()
        .filter(|f| f["op"] == 12 && f["d"]["tracks"][0]["track_name"] == "cam-video-me-1")
        .collect()
}

fn self_video(fx: &[Effect]) -> Vec<bool> {
    sends(fx, Socket::Room)
        .iter()
        .filter(|f| f["op"] == 15)
        .map(|f| f["d"]["self_video"].as_bool().expect("self_video"))
        .collect()
}

/// A joined call whose camera is on, on SFU track `cam-sfu-1`.
fn camera_on() -> CallMachine {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetCamera(true)));
    m.handle(camera_published("cam-sfu-1"));
    m
}

#[test]
fn camera_on_should_push_then_announce_its_layers_and_self_video() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::SetCamera(true)));
    assert!(publishes_camera(&fx));
    // Not on until the push lands.
    assert!(camera_notices(&fx).is_empty());

    let fx = m.handle(camera_published("cam-sfu-1"));
    assert!(fx.contains(&Effect::SetCameraLive(true)));
    let announce = camera_announces(&fx);
    assert_eq!(announce.len(), 1);
    let track = &announce[0]["d"]["tracks"][0];
    assert_eq!(
        (track["kind"].as_str(), track["sfu_track_name"].as_str()),
        (Some("video"), Some("cam-sfu-1"))
    );
    assert_eq!(track["simulcast_rids"], json!(["f", "h", "q"]));
    assert_eq!(self_video(&fx), vec![true]);
    assert_eq!(camera_notices(&fx), vec![(true, None)]);
    let me = last_roster(&fx)
        .expect("roster")
        .into_iter()
        .find(|m| m.me)
        .expect("me");
    assert!(me.video);
}

#[test]
fn camera_off_should_park_it_and_say_so() {
    let mut m = camera_on();
    let fx = m.handle(Input::Command(UserCommand::SetCamera(false)));
    assert!(fx.contains(&Effect::SetCameraLive(false)));
    let stops: Vec<Value> = sends(&fx, Socket::Voice)
        .into_iter()
        .filter(|f| f["op"] == 13)
        .collect();
    assert_eq!(stops.len(), 1);
    assert_eq!(stops[0]["d"]["track_names"], json!(["cam-video-me-1"]));
    assert_eq!(self_video(&fx), vec![false]);
    assert_eq!(camera_notices(&fx), vec![(false, None)]);
    // Parked: never closed, so no close effect.
    assert!(closes(&fx).is_empty());
}

#[test]
fn camera_on_again_should_announce_the_kept_track() {
    let mut m = camera_on();
    m.handle(Input::Command(UserCommand::SetCamera(false)));
    let fx = m.handle(Input::Command(UserCommand::SetCamera(true)));
    assert!(publishes_camera(&fx));
    let fx = m.handle(camera_published("cam-sfu-1"));
    let announce = camera_announces(&fx);
    assert_eq!(
        announce[0]["d"]["tracks"][0]["sfu_track_name"],
        json!("cam-sfu-1")
    );
    assert_eq!(self_video(&fx), vec![true]);
}

#[test]
fn a_parked_camera_should_not_be_announced_on_reidentify() {
    let mut m = camera_on();
    m.handle(Input::Command(UserCommand::SetCamera(false)));
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert!(camera_announces(&fx).is_empty());
    // The mic still is.
    assert_eq!(ops(&fx, Socket::Voice).first(), Some(&12));
}

#[test]
fn a_live_camera_should_be_announced_again_on_reidentify() {
    let mut m = camera_on();
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert_eq!(camera_announces(&fx).len(), 1);
}

#[test]
fn camera_off_during_the_push_should_leave_it_parked() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetCamera(true)));
    m.handle(Input::Command(UserCommand::SetCamera(false)));
    let fx = m.handle(camera_published("cam-sfu-1"));
    assert!(camera_announces(&fx).is_empty());
    assert!(!fx.contains(&Effect::SetCameraLive(true)));
    assert!(self_video(&fx).is_empty());
}

#[test]
fn camera_before_the_mic_is_published_should_wait_for_it() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    let fx = m.handle(Input::Command(UserCommand::SetCamera(true)));
    assert!(!publishes_camera(&fx));
    let fx = m.handle(Input::Published(Ok(published())));
    assert!(publishes_camera(&fx));
}

#[test]
fn a_failed_camera_push_should_turn_it_off_with_the_reason() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetCamera(true)));
    let fx = m.handle(Input::CameraPublished(Err("SFU push: HTTP 500".into())));
    assert_eq!(
        camera_notices(&fx),
        vec![(false, Some("SFU push: HTTP 500".into()))]
    );
    assert_eq!(self_video(&fx), vec![false]);
    // A later toggle tries again.
    let fx = m.handle(Input::Command(UserCommand::SetCamera(true)));
    assert!(publishes_camera(&fx));
}

#[test]
fn mute_should_keep_self_video_as_it_is() {
    let mut m = camera_on();
    let fx = m.handle(Input::Command(UserCommand::SetMuted(true)));
    assert_eq!(self_video(&fx), vec![true]);
}

#[test]
fn a_camera_that_lands_during_a_voice_blip_should_be_announced_once_on_ready() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    m.handle(Input::Command(UserCommand::SetCamera(true)));
    let fx = m.handle(camera_published("cam-sfu-1"));
    assert!(sends(&fx, Socket::Voice).is_empty());
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert_eq!(camera_announces(&fx).len(), 1);
}

#[test]
fn camera_on_should_not_announce_the_mic_again() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetCamera(true)));
    let fx = m.handle(camera_published("cam-sfu-1"));
    assert_eq!(ops(&fx, Socket::Voice), vec![12]);
}

/// A joined call, and the credential refresh its admission armed.
fn joined_with_refresh() -> (CallMachine, Timer) {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let armed = refreshes(&m.handle(Input::Admitted(Ok(admission()))));
    m.handle(Input::Published(Ok(published())));
    m.handle(Input::Room(RoomEvent::Ready(room_state(&[]))));
    m.handle(voice_ready(Vec::new()));
    m.handle(connected());
    (m, armed[0].0.clone())
}

#[test]
fn the_refresh_timer_should_send_op_40_on_the_room_and_arm_again() {
    let (mut m, timer) = joined_with_refresh();
    let fx = m.handle(Input::Timer(timer));
    assert_eq!(ops(&fx, Socket::Room), vec![40]);
    assert!(ops(&fx, Socket::Voice).is_empty());
    let again = refreshes(&fx);
    assert_eq!(again.len(), 1);
    assert_eq!(again[0].1, CREDENTIAL_REFRESH);
}

#[test]
fn a_fresh_token_should_go_to_the_sfu_and_restart_the_refresh_from_its_arrival() {
    let (mut m, first) = joined_with_refresh();
    let fx = m.handle(Input::Room(RoomEvent::Credentials {
        voice_token: "tok-2".into(),
    }));
    assert!(fx.contains(&Effect::SetVoiceToken("tok-2".into())));
    assert!(fx.contains(&Effect::SetVoiceIdentify(json!({
        "op": 100, "d": { "participant_id": ME, "voice_token": "tok-2" }
    }))));
    let rearmed = refreshes(&fx);
    assert_eq!(rearmed.len(), 1);
    assert_eq!(rearmed[0].1, CREDENTIAL_REFRESH);
    // The timer armed for the old token no longer refreshes.
    assert!(ops(&m.handle(Input::Timer(first)), Socket::Room).is_empty());
    assert_eq!(
        ops(&m.handle(Input::Timer(rearmed[0].0.clone())), Socket::Room),
        vec![40]
    );
}

#[test]
fn the_same_token_again_should_not_restart_the_refresh() {
    let (mut m, _) = joined_with_refresh();
    let fx = m.handle(Input::Room(RoomEvent::Credentials {
        voice_token: "tok-1".into(),
    }));
    assert!(refreshes(&fx).is_empty());
    assert!(!fx.iter().any(|e| matches!(e, Effect::SetVoiceToken(_))));
}

#[test]
fn a_ready_or_resumed_without_credentials_should_keep_the_admission_token() {
    let (mut m, _) = joined_with_refresh();
    let ready = m.handle(Input::Room(RoomEvent::Ready(room_state(&[]))));
    m.handle(Input::Room(RoomEvent::Disconnected));
    let resumed = m.handle(Input::Room(RoomEvent::Resumed(RoomState {
        participant_id: String::new(),
        participants: Some(Vec::new()),
        voice_token: String::new(),
    })));
    for fx in [ready, resumed] {
        assert!(!fx
            .iter()
            .any(|e| matches!(e, Effect::SetVoiceToken(_) | Effect::SetVoiceIdentify(_))));
        assert!(refreshes(&fx).is_empty());
    }
    assert_eq!(m.state(), CallState::Joined);
}

#[test]
fn the_admission_token_should_identify_the_voice_socket_and_open_media() {
    let fx = admitted();
    let (_, voice) = open_sockets(&fx);
    assert_eq!(voice.identify["d"]["voice_token"], "tok-1");
    let media = fx
        .iter()
        .find_map(|e| match e {
            Effect::OpenMedia(media) => Some(media),
            _ => None,
        })
        .expect("OpenMedia");
    assert_eq!(media.voice_token, "tok-1");
    assert_eq!(media.participant_id, ME);
}

#[test]
fn a_voice_close_1013_should_reidentify_and_reannounce_without_ending() {
    // 1013 reaches the machine as a voice drop (yap-realtime retries it on a
    // fresh bound ticket), then a VoiceReady for the same participant.
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Disconnected));
    assert!(ends(&fx).is_empty());
    assert_eq!(m.state(), CallState::Joined);
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert!(ends(&fx).is_empty());
    let announced = sends(&fx, Socket::Voice);
    assert_eq!(announced[0]["op"], 12);
    assert_eq!(announced[0]["d"]["tracks"][0]["participant_id"], ME);
}

// ── Voice app events (op 106) ─────────────────────────────────────────────

#[test]
fn an_app_event_should_be_handed_to_the_owner_as_it_came() {
    let mut m = joined();
    let d = json!({ "type": "soundboard.stop", "server_key": "srv", "playback_id": "x" });
    let fx = m.handle(Input::Voice(VoiceEvent::App(d.clone())));
    assert_eq!(fx, vec![Effect::Notify(Notice::App(d))]);
}

#[test]
fn voice_ready_should_tell_the_owner_our_participant() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(voice_ready(Vec::new()));
    assert!(fx.contains(&Effect::Notify(Notice::VoiceReady {
        participant_id: ME.into()
    })));
}

#[test]
fn send_app_should_go_out_on_the_voice_socket() {
    let mut m = joined();
    let frame = r#"{"op":106,"d":{"type":"listen_together.state.request"}}"#;
    let fx = m.handle(Input::Command(UserCommand::SendApp(frame.into())));
    assert_eq!(fx, vec![Effect::Send(Socket::Voice, frame.into())]);
}

#[test]
fn send_app_while_the_voice_socket_reconnects_should_queue_and_flush_on_ready() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    let first = r#"{"op":106,"d":{"type":"soundboard.play","n":1}}"#;
    let second = r#"{"op":106,"d":{"type":"soundboard.play","n":2}}"#;
    for frame in [first, second] {
        let fx = m.handle(Input::Command(UserCommand::SendApp(frame.into())));
        assert!(sends(&fx, Socket::Voice).is_empty());
    }
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    let frames: Vec<&Effect> = fx
        .iter()
        .filter(|e| matches!(e, Effect::Send(Socket::Voice, f) if f.contains("soundboard.play")))
        .collect();
    assert_eq!(
        frames,
        vec![
            &Effect::Send(Socket::Voice, first.into()),
            &Effect::Send(Socket::Voice, second.into())
        ]
    );
    // Sent once: a later ready has nothing left to flush.
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert!(!fx
        .iter()
        .any(|e| matches!(e, Effect::Send(Socket::Voice, f) if f.contains("soundboard.play"))));
}

#[test]
fn the_app_queue_should_hold_32_and_drop_on_leave() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Disconnected));
    for n in 0..40 {
        m.handle(Input::Command(UserCommand::SendApp(format!(
            r#"{{"op":106,"d":{{"type":"soundboard.stop","n":{n}}}}}"#
        ))));
    }
    let fx = m.handle(voice_ready(Vec::new()));
    let queued: Vec<i64> = sends(&fx, Socket::Voice)
        .iter()
        .filter(|frame| frame["op"] == 106)
        .filter_map(|frame| frame["d"]["n"].as_i64())
        .collect();
    // The newest 32 survive, in order.
    assert_eq!(queued, (8..40).collect::<Vec<i64>>());

    m.handle(Input::Voice(VoiceEvent::Disconnected));
    m.handle(Input::Command(UserCommand::SendApp(
        r#"{"op":106,"d":{"type":"soundboard.stop","n":99}}"#.into(),
    )));
    m.handle(Input::Command(UserCommand::Leave));
    let fx = m.handle(voice_ready(Vec::new()));
    assert!(sends(&fx, Socket::Voice)
        .iter()
        .all(|frame| frame["op"] != 106));
}

#[test]
fn our_soundboard_clip_should_set_the_soundshare_flag() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::SetSoundboardSpeaking(true)));
    assert_eq!(
        sends(&fx, Socket::Voice),
        vec![json!({ "op": 5, "d": { "speaking": 2 } })]
    );
    let fx = m.handle(Input::Command(UserCommand::SetSoundboardSpeaking(false)));
    assert_eq!(
        sends(&fx, Socket::Voice),
        vec![json!({ "op": 5, "d": { "speaking": 0 } })]
    );
}

#[test]
fn a_repeated_soundboard_flag_should_send_nothing() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetSoundboardSpeaking(true)));
    let fx = m.handle(Input::Command(UserCommand::SetSoundboardSpeaking(true)));
    assert!(sends(&fx, Socket::Voice).is_empty());
}

// ---- Batch N4: DM calls, moves, self-healing, release, standby, stats ----

fn dm_target() -> CallTarget {
    CallTarget {
        channel_id: "dm-1".into(),
        me_name: "Jon".into(),
        me_username: "jon".into(),
        room_slug: Some(yap_voice::dm_call_slug("u-b", "u-a")),
        ..CallTarget::default()
    }
}

#[test]
fn dm_call_slug_should_sort_the_two_user_ids() {
    assert_eq!(yap_voice::dm_call_slug("u-b", "u-a"), "dm-call-u-a-u-b");
    assert_eq!(yap_voice::dm_call_slug("u-a", "u-b"), "dm-call-u-a-u-b");
}

#[test]
fn a_dm_call_should_ask_one_admission_for_its_room_without_a_server() {
    let mut m = CallMachine::new(dm_target());
    let fx = m.handle(Input::Start);
    let request = fx
        .iter()
        .find_map(|e| match e {
            Effect::RequestAdmission(r) => Some(json_of(r)),
            _ => None,
        })
        .expect("an admission");
    assert_eq!(
        request,
        json!({
            "channelId": "dm-1", "roomSlug": "dm-call-u-a-u-b",
            "sockets": ["room", "voice"], "media": true,
        })
    );
}

#[test]
fn a_dm_call_should_open_its_sockets_under_the_dm_room() {
    let mut m = CallMachine::new(dm_target());
    m.handle(Input::Start);
    let fx = m.handle(Input::Admitted(Ok(admission())));
    let room = fx
        .iter()
        .find_map(|e| match e {
            Effect::OpenSockets { room, .. } => Some(room.path.clone()),
            _ => None,
        })
        .expect("sockets");
    assert_eq!(room, "/api/channels/dm-call-u-a-u-b/ws?v=1");
}

#[test]
fn a_join_after_a_move_should_carry_the_grant() {
    let mut m = CallMachine::new(CallTarget {
        move_grant: Some("grant-1".into()),
        ..target()
    });
    let fx = m.handle(Input::Start);
    let request = fx
        .iter()
        .find_map(|e| match e {
            Effect::RequestAdmission(r) => Some(json_of(r)),
            _ => None,
        })
        .expect("an admission");
    assert_eq!(request["moveGrant"], "grant-1");
}

#[test]
fn a_voice_move_should_report_the_target_then_end_quietly_on_4010() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::Moved(
        serde_json::from_value(json!({
            "channel_id": "c2", "server_id": "s1", "channel_name": "Lounge",
            "mover_name": "Mod", "grant": "g", "grant_expires_at": 5.0,
        }))
        .expect("a move"),
    )));
    let moved = fx.iter().find_map(|e| match e {
        Effect::Notify(Notice::Moved(to)) => Some(to.channel_id.clone()),
        _ => None,
    });
    assert_eq!(moved.as_deref(), Some("c2"));
    let fx = m.handle(Input::Room(RoomEvent::Stopped(StopReason::Replaced {
        code: 4010,
        reason: "Session replaced".into(),
    })));
    assert_eq!(ends(&fx), vec![None]);
}

fn not_found(sfu: &str) -> Input {
    Input::PullNotFound {
        sfu_track_name: sfu.into(),
    }
}

/// A joined call where bo's mic was offered and is being pulled.
fn pulling_bo() -> CallMachine {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bo".into(),
        tracks: vec![track("bo", "cam-audio-bo", "sfu-bo")],
    }));
    m
}

#[test]
fn a_pull_the_sfu_keeps_not_finding_should_send_track_unavailable() {
    let mut m = pulling_bo();
    let fx = m.handle(not_found("sfu-bo"));
    let sent = sends(&fx, Socket::Voice);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["op"], 109);
    assert_eq!(
        sent[0]["d"],
        json!({
            "participant_id": "bo", "track_name": "cam-audio-bo",
            "sfu_track_name": "sfu-bo", "session_id": "session-bo",
        })
    );
}

#[test]
fn track_unavailable_should_stop_after_three_reports_until_a_pull_works() {
    let mut m = pulling_bo();
    let mut reports = 0;
    for _ in 0..5 {
        let fx = m.handle(not_found("sfu-bo"));
        reports += ops(&fx, Socket::Voice)
            .iter()
            .filter(|op| **op == 109)
            .count();
        m.handle(Input::PullDone {
            sfu_track_name: "sfu-bo".into(),
            mid: None,
        });
    }
    assert_eq!(reports, 3);
}

fn republish_request(track: &str, sfu: Option<&str>) -> Input {
    Input::Voice(VoiceEvent::Republish(
        serde_json::from_value(json!({ "track_name": track, "sfu_track_name": sfu }))
            .expect("a request"),
    ))
}

fn republishes(fx: &[Effect]) -> Vec<yap_voice::Republish> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Republish(kind) => Some(*kind),
            _ => None,
        })
        .collect()
}

#[test]
fn a_republish_request_for_our_mic_should_push_it_again_once_per_cooldown() {
    let mut m = joined();
    let fx = m.handle(republish_request("cam-audio-me-1", Some("my-sfu")));
    assert_eq!(republishes(&fx), vec![yap_voice::Republish::Mic]);
    let fx = m.handle(republish_request("cam-audio-me-1", Some("my-sfu")));
    assert!(republishes(&fx).is_empty(), "rate-limited");
    m.handle(Input::Timer(Timer::RepublishCooldown {
        track: "cam-audio-me-1".into(),
    }));
    let fx = m.handle(republish_request("cam-audio-me-1", None));
    assert_eq!(republishes(&fx), vec![yap_voice::Republish::Mic]);
}

#[test]
fn a_stale_or_foreign_republish_request_should_be_ignored() {
    let mut m = joined();
    let fx = m.handle(republish_request("cam-audio-me-1", Some("old-sfu")));
    assert!(republishes(&fx).is_empty());
    let fx = m.handle(republish_request("cam-audio-ana", None));
    assert!(republishes(&fx).is_empty());
}

#[test]
fn a_landed_republish_should_announce_the_new_sfu_track() {
    let mut m = joined();
    let fx = m.handle(Input::Republished(Ok((
        yap_voice::Republish::Mic,
        "my-sfu-2".into(),
    ))));
    let sent = sends(&fx, Socket::Voice);
    assert_eq!(sent[0]["op"], 12);
    assert_eq!(sent[0]["d"]["tracks"][0]["sfu_track_name"], "my-sfu-2");
}

fn shown(alias: &str) -> VideoIntent {
    VideoIntent {
        demand: [(
            alias.to_owned(),
            RenderSize {
                width: 320,
                height: 180,
            },
        )]
        .into_iter()
        .collect(),
        stage: None,
    }
}

fn release_timer(fx: &[Effect]) -> Option<Timer> {
    fx.iter().find_map(|e| match e {
        Effect::ArmTimer { timer, after } if *after == yap_voice::VIDEO_RELEASE_DELAY => {
            Some(timer.clone())
        }
        _ => None,
    })
}

#[test]
fn a_camera_nobody_shows_should_be_released_and_pulled_fresh_later() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetVideo(shown(
        "cam-video-ana",
    ))));
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-cam")],
    }));
    m.handle(done("sfu-cam", "m-cam"));
    let fx = m.handle(Input::Command(
        UserCommand::SetVideo(VideoIntent::default()),
    ));
    let timer = release_timer(&fx).expect("a release timer");
    let fx = m.handle(Input::Timer(timer));
    assert_eq!(closes(&fx), vec![vec!["m-cam".to_owned()]]);
    let fx = m.handle(Input::Command(UserCommand::SetVideo(shown(
        "cam-video-ana",
    ))));
    let fresh: Vec<bool> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::Pull(r) => Some(r.fresh),
            _ => None,
        })
        .collect();
    assert_eq!(fresh, vec![true]);
}

#[test]
fn a_camera_shown_again_before_the_delay_should_keep_its_pull() {
    let mut m = joined();
    m.handle(Input::Command(UserCommand::SetVideo(shown(
        "cam-video-ana",
    ))));
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-cam")],
    }));
    m.handle(done("sfu-cam", "m-cam"));
    let fx = m.handle(Input::Command(
        UserCommand::SetVideo(VideoIntent::default()),
    ));
    let timer = release_timer(&fx).expect("a release timer");
    m.handle(Input::Command(UserCommand::SetVideo(shown(
        "cam-video-ana",
    ))));
    let fx = m.handle(Input::Timer(timer));
    assert!(closes(&fx).is_empty());
}

fn standby_machine() -> (CallMachine, Vec<Effect>) {
    let mut m = CallMachine::new(CallTarget {
        standby: true,
        ..target()
    });
    m.handle(Input::Start);
    let fx = m.handle(Input::Admitted(Ok(admission())));
    (m, fx)
}

#[test]
fn a_standby_should_identify_hidden_and_hold_the_voice_identify() {
    let (_m, fx) = standby_machine();
    let (room, voice) = fx
        .iter()
        .find_map(|e| match e {
            Effect::OpenSockets { room, voice } => {
                Some((room.identify.clone(), voice.identify.clone()))
            }
            _ => None,
        })
        .expect("sockets");
    assert_eq!(room["d"]["standby"], true);
    assert_eq!(voice, Value::Null);
}

#[test]
fn a_standby_ready_should_not_join_until_promoted() {
    let (mut m, _) = standby_machine();
    let fx = m.handle(Input::Published(Ok(published())));
    assert!(fx.contains(&Effect::SetMicEnabled(false)));
    let fx = m.handle(Input::Room(RoomEvent::Ready(room_state(&[]))));
    assert!(
        sends(&fx, Socket::Room).is_empty(),
        "no voice state while hidden"
    );
    assert_eq!(m.state(), CallState::Connecting);
    let fx = m.handle(Input::Command(UserCommand::Promote));
    let room = sends(&fx, Socket::Room);
    assert_eq!(room[0]["d"]["promote_standby"], true);
    assert!(fx
        .iter()
        .any(|e| matches!(e, Effect::SetVoiceIdentify(v) if v["op"] == 100)));
    assert!(fx.contains(&Effect::PromoteMedia));
    assert!(fx.contains(&Effect::Notify(Notice::Connected)));
    assert_eq!(m.state(), CallState::Joined);
}

#[test]
fn a_stale_standby_should_republish_its_mic_on_promotion() {
    let (mut m, _) = standby_machine();
    m.handle(Input::Published(Ok(published())));
    m.handle(Input::Timer(Timer::StandbyStale));
    let fx = m.handle(Input::Command(UserCommand::Promote));
    assert_eq!(republishes(&fx), vec![yap_voice::Republish::Mic]);
}

fn stats(at_ms: u64, mic: u64, inbound: u64) -> Input {
    Input::Stats {
        stats: yap_voice::MediaStats {
            mic_bytes_sent: mic,
            audio_bytes_received: inbound,
            audio_packets_received: inbound / 100,
            round_trip: Some(0.04),
            server: "203.0.113.5:3478 udp".into(),
            inbound_streams: 1,
            ..yap_voice::MediaStats::default()
        },
        at_ms,
    }
}

fn voice_stats(fx: &[Effect]) -> Option<yap_voice::VoiceStats> {
    fx.iter().find_map(|e| match e {
        Effect::Notify(Notice::Stats(s)) => Some(s.clone()),
        _ => None,
    })
}

#[test]
fn stats_should_report_bitrate_round_trip_and_the_server() {
    let mut m = joined();
    m.handle(stats(0, 0, 0));
    let fx = m.handle(stats(5_000, 25_000, 10_000));
    let s = voice_stats(&fx).expect("stats");
    assert!((s.bitrate_kbps - 40.0).abs() < 1e-9, "{}", s.bitrate_kbps);
    assert_eq!(s.rtt_ms, Some(40.0));
    assert_eq!(s.server, "203.0.113.5:3478 udp");
    assert_eq!(s.room, "voice-s1-c1");
    assert_eq!(s.quality, "good");
    assert_eq!(s.samples.len(), 1);
    assert_eq!(s.samples[0].at_ms, 5_000);
}

#[test]
fn remote_tracks_should_count_live_pulls_not_inbound_rtp_entries() {
    let mut m = joined();
    // One pull, and no inbound-rtp entry yet (no packet arrived).
    let none_yet = |at_ms| Input::Stats {
        stats: yap_voice::MediaStats {
            round_trip: Some(0.04),
            ..yap_voice::MediaStats::default()
        },
        at_ms,
    };
    m.handle(none_yet(0));
    let s = voice_stats(&m.handle(none_yet(5_000))).expect("stats");
    assert_eq!(s.remote_tracks, 1);
    // Their reconnect grace runs out: the row stays, the pull goes.
    let fx = m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    }));
    m.handle(Input::Timer(grace_timer(&fx)));
    assert!(m.roster().iter().any(|r| r.id == "ana"));
    let s = voice_stats(&m.handle(stats(10_000, 0, 0))).expect("stats");
    assert_eq!(s.remote_tracks, 0);
}

#[test]
fn a_silent_unmuted_mic_should_stall_after_30_s_and_republish() {
    let mut m = joined();
    m.handle(stats(0, 100, 0));
    let mut stalled = false;
    let mut kinds = Vec::new();
    for tick in 1..=6 {
        let fx = m.handle(stats(tick * 5_000, 100, 0));
        stalled |= fx.contains(&Effect::Notify(Notice::MicStalled(true)));
        kinds.extend(republishes(&fx));
    }
    assert!(stalled);
    assert_eq!(kinds, vec![yap_voice::Republish::Mic]);
}

#[test]
fn inbound_audio_should_stall_only_while_someone_speaks() {
    let mut m = joined();
    m.handle(stats(0, 0, 500));
    let mut stalled = false;
    for tick in 1..=7 {
        m.handle(Input::Voice(VoiceEvent::Speaking {
            participant_id: "ana".into(),
            speaking: true,
        }));
        let fx = m.handle(stats(tick * 5_000, tick * 1_000, 500));
        stalled |= fx.contains(&Effect::Notify(Notice::InboundAudioStalled(true)));
    }
    assert!(stalled);
    let fx = m.handle(stats(40_000, 9_000, 900));
    assert!(fx.contains(&Effect::Notify(Notice::InboundAudioStalled(false))));
}

fn device_stats(at_ms: u64, mic_failed: bool, speaker_failed: bool) -> Input {
    let Input::Stats { mut stats, at_ms } = stats(at_ms, at_ms, at_ms) else {
        unreachable!("stats() builds Input::Stats");
    };
    stats.mic_device_failed = mic_failed;
    stats.speaker_device_failed = speaker_failed;
    Input::Stats { stats, at_ms }
}

#[test]
fn a_failing_mic_device_should_raise_the_mic_banner_until_it_works() {
    let mut m = joined();
    m.handle(device_stats(0, false, false));
    let raised = m.handle(device_stats(5_000, true, false));
    let cleared = m.handle(device_stats(10_000, false, false));
    assert!(
        raised.contains(&Effect::Notify(Notice::MicStalled(true)))
            && cleared.contains(&Effect::Notify(Notice::MicStalled(false)))
    );
}

#[test]
fn a_failing_speaker_device_should_raise_the_inbound_banner_once() {
    let mut m = joined();
    m.handle(device_stats(0, false, false));
    let first = m.handle(device_stats(5_000, false, true));
    let again = m.handle(device_stats(10_000, false, true));
    let raised = |fx: &[Effect]| fx.contains(&Effect::Notify(Notice::InboundAudioStalled(true)));
    assert!(raised(&first) && !raised(&again));
}

// Spatial audio: the room's placement, our switches, pans and distance.

fn pans(fx: &[Effect]) -> Vec<(String, f64)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::SetTrackPan { mid, pan } => Some((mid.clone(), *pan)),
            _ => None,
        })
        .collect()
}

fn spatial_room(enabled: bool) -> Input {
    Input::Room(RoomEvent::Spatial(SpatialState {
        enabled,
        ..SpatialState::default()
    }))
}

fn spatial_local(enabled: bool, high_fidelity: bool) -> Input {
    Input::Command(UserCommand::SetSpatial(SpatialLocal {
        enabled,
        high_fidelity,
        user_id: "u-me".into(),
    }))
}

#[test]
fn ready_should_send_our_spatial_switches_with_the_rooms_state() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Admitted(Ok(admission())));
    m.handle(Input::Published(Ok(published())));
    m.handle(spatial_room(true));
    let fx = m.handle(Input::Room(RoomEvent::Ready(room_state(&[]))));
    let spatial = &sends(&fx, Socket::Room)[1]["d"];
    assert_eq!(
        (
            &spatial["spatial_audio_enabled"],
            &spatial["spatial_audio_high_fidelity"],
            &spatial["spatial_audio_state"]["enabled"],
            &spatial["spatial_audio_state"]["placementMode"],
        ),
        (&json!(false), &json!(false), &json!(false), &json!("arc"))
    );
}

#[test]
fn turning_spatial_audio_on_should_go_out_in_op_15() {
    let mut m = joined();
    let fx = m.handle(spatial_local(true, true));
    let spatial = &sends(&fx, Socket::Room)[0]["d"];
    assert_eq!(
        (
            &spatial["spatial_audio_enabled"],
            &spatial["spatial_audio_high_fidelity"],
            &spatial["spatial_audio_state"]["enabled"],
        ),
        (&json!(true), &json!(true), &json!(true))
    );
}

#[test]
fn the_same_switches_again_should_send_nothing() {
    let mut m = joined();
    m.handle(spatial_local(true, true));
    assert!(sends(&m.handle(spatial_local(true, true)), Socket::Room).is_empty());
}

#[test]
fn hearing_the_room_spatially_should_pan_and_attenuate_the_voice() {
    let mut m = joined_with_user();
    m.handle(spatial_room(true));
    let fx = m.handle(spatial_local(true, true));
    // We sit first on the default arc (left), Ana second (centre): right
    // of us, and some metres away.
    let pan = pans(&fx);
    assert_eq!(pan.len(), 1);
    assert!(pan[0].0 == "m-ana" && pan[0].1 > 0.3, "{pan:?}");
    let gain = gains(&fx);
    assert!(gain[0].1 < 1.0 && gain[0].1 >= 0.32, "{gain:?}");
}

#[test]
fn spatial_audio_should_need_high_fidelity() {
    let mut m = joined_with_user();
    m.handle(spatial_local(true, false));
    assert!(pans(&m.handle(spatial_room(true))).is_empty());
}

#[test]
fn deafening_should_centre_the_voices_again() {
    let mut m = joined_with_user();
    m.handle(spatial_local(true, true));
    m.handle(spatial_room(true));
    let fx = m.handle(Input::Command(UserCommand::SetDeafened(true)));
    assert_eq!(pans(&fx), vec![("m-ana".to_owned(), 0.0)]);
}

#[test]
fn share_audio_should_never_be_panned() {
    let mut m = joined_with_user();
    m.handle(spatial_local(true, true));
    m.handle(spatial_room(true));
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![track("ana", "screen-audio-ana", "sfu-ana-share")],
    }));
    let fx = m.handle(done("sfu-ana-share", "m-ana-share"));
    assert!(pans(&fx).iter().all(|(mid, _)| mid != "m-ana-share"));
}

#[test]
fn a_new_placement_should_be_sent_for_the_room() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::SetSpatialRoom(SpatialState {
        room_size: 80.0,
        ..SpatialState::default()
    })));
    assert_eq!(
        sends(&fx, Socket::Room)[0]["d"]["spatial_audio_state"]["roomSize"],
        json!(80.0)
    );
}

fn last_map(fx: &[Effect]) -> Option<yap_voice::SpatialMap> {
    fx.iter().rev().find_map(|e| match e {
        Effect::Notify(Notice::Spatial(map)) => Some(map.clone()),
        _ => None,
    })
}

#[test]
fn a_panel_edit_should_be_heard_at_once_without_waiting_for_the_echo() {
    // useVoiceChannel.ts updateSharedSpatialAudioState dispatches the state
    // locally and sends it; the mix follows the local state.
    let mut m = joined_with_user();
    m.handle(spatial_local(true, true));
    m.handle(spatial_room(true));
    let fx = m.handle(Input::Command(UserCommand::SetSpatialRoom(SpatialState {
        enabled: true,
        placement: yap_voice::spatial::Placement::Line,
        // Close enough that the pan is not clamped at the edge either way.
        distance: 10.0,
        ..SpatialState::default()
    })));
    let pan = pans(&fx);
    assert!(
        pan.len() == 1 && pan[0].1 > 0.0 && pan[0].1 < 1.0,
        "{pan:?}"
    );
    let map = last_map(&fx).expect("map");
    assert_eq!(map.state.placement, yap_voice::spatial::Placement::Line);
    assert_eq!(
        sends(&fx, Socket::Room)[0]["d"]["spatial_audio_state"]["placementMode"],
        json!("line")
    );
}

#[test]
fn reset_should_turn_the_room_off_as_the_web_does() {
    // SpatialAudioPanel.tsx Reset commits the defaults, `enabled: false`.
    let mut m = joined();
    m.handle(spatial_local(true, true));
    let fx = m.handle(Input::Command(UserCommand::SetSpatialRoom(
        SpatialState::default(),
    )));
    let d = &sends(&fx, Socket::Room)[0]["d"];
    assert_eq!(
        (
            &d["spatial_audio_enabled"],
            &d["spatial_audio_state"]["enabled"]
        ),
        (&json!(true), &json!(false))
    );
}

#[test]
fn the_map_should_seat_us_first_and_carry_each_peers_switches() {
    let mut m = joined_with_user();
    m.handle(spatial_local(true, true));
    let mut ana = participant("ana", None);
    ana.spatial_enabled = Some(true);
    ana.spatial_high_fidelity = Some(false);
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: ana,
    }));
    let map = last_map(&fx).expect("map");
    let ids: Vec<(&str, bool)> = map
        .seats
        .iter()
        .map(|s| (s.user_id.as_str(), s.me))
        .collect();
    assert_eq!(ids, [("u-me", true), ("u-ana", false)]);
    assert!(map.seats[1].enabled && !map.seats[1].high_fidelity);
    // The seats are the mix's: the default arc puts us left of Ana.
    assert!(map.seats[0].position.x < map.seats[1].position.x);
}

#[test]
fn an_unchanged_map_should_not_be_reported_again() {
    let mut m = joined_with_user();
    m.handle(spatial_local(true, true));
    assert!(last_map(&m.handle(level(0.0, 0))).is_none());
}

fn pull_failed(sfu: &str, failure: PullFailure) -> Input {
    Input::PullFailed {
        sfu_track_name: sfu.into(),
        failure,
    }
}

fn pull_none(sfu: &str) -> Input {
    Input::PullDone {
        sfu_track_name: sfu.into(),
        mid: None,
    }
}

/// The pull retry timer a failure armed, with its wait.
fn retry_timer(fx: &[Effect]) -> Option<(Timer, Duration)> {
    fx.iter().find_map(|e| match e {
        Effect::ArmTimer { timer, after } if matches!(timer, Timer::PullRetry { .. }) => {
            Some((timer.clone(), *after))
        }
        _ => None,
    })
}

/// The in-flight pull of Ana's camera fails with `failure`, reported as
/// the driver does (the reason, then the failed done); the reason's
/// effects and the done's.
fn camera_pull_fails(m: &mut CallMachine, failure: PullFailure) -> (Vec<Effect>, Vec<Effect>) {
    let failed = m.handle(pull_failed("sfu-ana-cam", failure));
    let done = m.handle(pull_none("sfu-ana-cam"));
    (failed, done)
}

/// `with_camera`, shown in a tile, its pull in flight.
fn camera_pulling() -> CallMachine {
    let mut m = with_camera();
    let fx = m.handle(show(&[(ANA_CAM, 640, 360)], None));
    assert_eq!(pulls(&fx), vec!["sfu-ana-cam"]);
    m
}

#[test]
fn a_failed_pull_should_wait_its_backoff_instead_of_pulling_again_on_the_next_input() {
    let mut m = camera_pulling();
    let (failed, done) = camera_pull_fails(&mut m, PullFailure::Rejected);
    assert!(pulls(&failed).is_empty() && pulls(&done).is_empty());
    let (timer, after) = retry_timer(&failed).expect("a retry is armed");
    assert_eq!(after, PULL_RETRY_BACKOFF);
    // Mic level ticks and stats used to pull it again at once, each time.
    for at in 0..20 {
        assert!(pulls(&m.handle(level(0.0, at * 50))).is_empty());
    }
    assert!(pulls(&m.handle(Input::Timer(Timer::Stats))).is_empty());
    assert_eq!(pulls(&m.handle(Input::Timer(timer))), vec!["sfu-ana-cam"]);
}

#[test]
fn failed_pulls_in_a_row_should_double_their_wait_up_to_the_cap() {
    let mut m = camera_pulling();
    let mut waits = Vec::new();
    for _ in 0..7 {
        let (failed, _) = camera_pull_fails(&mut m, PullFailure::Transient);
        let (timer, after) = retry_timer(&failed).expect("a retry is armed");
        waits.push(after);
        assert_eq!(pulls(&m.handle(Input::Timer(timer))), vec!["sfu-ana-cam"]);
    }
    assert_eq!(waits, [1, 2, 4, 8, 16, 30, 30].map(Duration::from_secs));
    assert_eq!(PULL_RETRY_MAX, Duration::from_secs(30));
}

/// The pulls `fx` asks for, with whether each runs on its own session.
fn fresh_pulls(fx: &[Effect]) -> Vec<(String, bool)> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Pull(r) => Some((r.sfu_track_name.clone(), r.fresh)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_pull_the_sfu_keeps_refusing_should_slow_to_the_cap_but_never_stop() {
    let mut m = camera_pulling();
    let mut waits = Vec::new();
    for _ in 0..MAX_REJECTED_PULLS + 2 {
        let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
        let (timer, after) = retry_timer(&failed).expect("always retried");
        waits.push(after);
        assert_eq!(pulls(&m.handle(Input::Timer(timer))), vec!["sfu-ana-cam"]);
    }
    assert_eq!(waits, [1, 2, 30, 30, 30].map(Duration::from_secs));
    // Between tries, nothing pulls it.
    let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
    assert!(pulls(&m.handle(Input::Timer(Timer::Stats))).is_empty());
    assert!(pulls(&m.handle(show(&[(ANA_CAM, 1280, 720)], None))).is_empty());
    let _ = failed;
    // Ana's camera pushed again (a new SFU track): pulled at once.
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-ana-cam-2")],
    }));
    assert_eq!(pulls(&fx), vec!["sfu-ana-cam-2"]);
}

#[test]
fn only_refusals_in_a_row_should_reach_the_cap() {
    let mut m = camera_pulling();
    let mut waits = Vec::new();
    for failure in [
        PullFailure::Rejected,
        PullFailure::Rejected,
        PullFailure::Transient,
        PullFailure::Rejected,
    ] {
        let (failed, _) = camera_pull_fails(&mut m, failure);
        let (timer, after) = retry_timer(&failed).expect("retried");
        waits.push(after);
        m.handle(Input::Timer(timer));
    }
    // Doubling by all failures in a row; the refusal count started over.
    assert_eq!(waits, [1, 2, 4, 8].map(Duration::from_secs));
}

#[test]
fn a_refused_video_pull_should_be_tried_next_on_its_own_session() {
    let mut m = camera_pulling();
    let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
    let (timer, _) = retry_timer(&failed).expect("armed");
    assert_eq!(
        fresh_pulls(&m.handle(Input::Timer(timer))),
        vec![("sfu-ana-cam".to_owned(), true)]
    );
    // Every later video pull too, but not audio.
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bo".into(),
        tracks: vec![
            camera("bo", "sfu-bo-cam"),
            track("bo", "cam-audio-bo", "sfu-bo"),
        ],
    }));
    let mut later = fresh_pulls(&m.handle(show(
        &[(ANA_CAM, 640, 360), ("cam-video-bo", 640, 360)],
        None,
    )));
    later.sort();
    assert!(
        later.contains(&("sfu-bo-cam".to_owned(), true)),
        "{later:?}"
    );
    assert!(!later.contains(&("sfu-bo".to_owned(), true)), "{later:?}");
}

#[test]
fn a_rebuild_should_pull_video_on_our_session_again() {
    let mut m = camera_pulling();
    camera_pull_fails(&mut m, PullFailure::Rejected);
    let (timer, _) = rebuild_timer(&m.handle(failed())).expect("armed");
    m.handle(Input::Timer(timer));
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(connected());
    assert!(
        fresh_pulls(&fx).contains(&("sfu-ana-cam".to_owned(), false)),
        "{:?}",
        fresh_pulls(&fx)
    );
}

#[test]
fn a_transient_video_failure_should_stay_on_our_session() {
    let mut m = camera_pulling();
    let (failed, _) = camera_pull_fails(&mut m, PullFailure::Transient);
    let (timer, _) = retry_timer(&failed).expect("armed");
    assert_eq!(
        fresh_pulls(&m.handle(Input::Timer(timer))),
        vec![("sfu-ana-cam".to_owned(), false)]
    );
}

#[test]
fn a_refused_audio_pull_should_keep_being_retried() {
    let mut m = joined();
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bo".into(),
        tracks: vec![track("bo", "cam-audio-bo", "sfu-bo")],
    }));
    for _ in 0..MAX_REJECTED_PULLS + 3 {
        let failed = m.handle(pull_failed("sfu-bo", PullFailure::Rejected));
        m.handle(pull_none("sfu-bo"));
        let (timer, _) = retry_timer(&failed).expect("never given up");
        assert_eq!(
            fresh_pulls(&m.handle(Input::Timer(timer))),
            vec![("sfu-bo".to_owned(), false)]
        );
    }
}

#[test]
fn a_fresh_voice_token_should_end_the_waits() {
    let mut m = camera_pulling();
    for _ in 0..MAX_REJECTED_PULLS {
        let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
        let (timer, _) = retry_timer(&failed).expect("armed");
        m.handle(Input::Timer(timer));
    }
    camera_pull_fails(&mut m, PullFailure::Rejected);
    let fx = m.handle(Input::Room(RoomEvent::Credentials {
        voice_token: "tok-2".into(),
    }));
    assert_eq!(pulls(&fx), vec!["sfu-ana-cam"]);
}

#[test]
fn a_retry_after_should_hold_every_new_pull() {
    let mut m = camera_pulling();
    let failed = m.handle(pull_failed(
        "sfu-ana-cam",
        PullFailure::RateLimited {
            retry_after: Some(Duration::from_secs(20)),
        },
    ));
    m.handle(pull_none("sfu-ana-cam"));
    let resume = failed
        .iter()
        .find_map(|e| match e {
            Effect::ArmTimer { timer, after } if matches!(timer, Timer::PullsResume { .. }) => {
                Some((timer.clone(), *after))
            }
            _ => None,
        })
        .expect("a resume timer");
    assert_eq!(resume.1, Duration::from_secs(20));
    // Bo's mic offered meanwhile: held too.
    let fx = m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "bo".into(),
        tracks: vec![track("bo", "cam-audio-bo", "sfu-bo")],
    }));
    assert!(pulls(&fx).is_empty());
    let (retry, _) = retry_timer(&failed).expect("armed");
    assert!(pulls(&m.handle(Input::Timer(retry))).is_empty());
    let mut after = pulls(&m.handle(Input::Timer(resume.0)));
    after.sort();
    assert_eq!(after, vec!["sfu-ana-cam", "sfu-bo"]);
}

#[test]
fn a_not_found_or_rate_limited_pull_should_keep_retrying() {
    for failure in [
        PullFailure::NotFound,
        PullFailure::RateLimited { retry_after: None },
    ] {
        let mut m = camera_pulling();
        for _ in 0..MAX_REJECTED_PULLS + 2 {
            let (failed, _) = camera_pull_fails(&mut m, failure);
            let (timer, _) = retry_timer(&failed).expect("still retried");
            m.handle(Input::Timer(timer));
        }
    }
}

#[test]
fn a_rate_limited_pull_should_honour_retry_after() {
    let mut m = camera_pulling();
    let (failed, _) = camera_pull_fails(
        &mut m,
        PullFailure::RateLimited {
            retry_after: Some(Duration::from_secs(12)),
        },
    );
    assert_eq!(
        retry_timer(&failed).map(|(_, after)| after),
        Some(Duration::from_secs(12))
    );
    // Without a Retry-After, the backoff; a longer backoff wins; a huge
    // Retry-After is capped.
    let limited = |after: Option<u64>| PullFailure::RateLimited {
        retry_after: after.map(Duration::from_secs),
    };
    assert_eq!(limited(None).backoff(2), Duration::from_secs(2));
    assert_eq!(limited(Some(1)).backoff(4), Duration::from_secs(8));
    assert_eq!(limited(Some(3600)).backoff(1), PULL_RETRY_AFTER_MAX);
}

#[test]
fn a_pull_that_works_should_start_its_count_over() {
    let mut m = camera_pulling();
    for _ in 0..MAX_REJECTED_PULLS - 1 {
        let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
        let (timer, _) = retry_timer(&failed).expect("armed");
        m.handle(Input::Timer(timer));
    }
    m.handle(done("sfu-ana-cam", "m-cam"));
    // Its publisher withdraws and pushes again under another SFU track;
    // that pull failing is the first in a row again.
    m.handle(Input::Voice(VoiceEvent::Video {
        participant_id: "ana".into(),
        tracks: vec![camera("ana", "sfu-ana-cam-2")],
    }));
    let failed = m.handle(pull_failed("sfu-ana-cam-2", PullFailure::Rejected));
    assert_eq!(
        retry_timer(&failed).map(|(_, after)| after),
        Some(PULL_RETRY_BACKOFF)
    );
}

#[test]
fn a_failed_pull_should_leave_the_call_and_its_share_alone() {
    let mut m = camera_pulling();
    for _ in 0..10 {
        let (failed, done) = camera_pull_fails(&mut m, PullFailure::Rejected);
        for fx in [&failed, &done] {
            assert!(ends(fx).is_empty());
            assert_eq!(rebuilds(fx), 0);
            assert!(closes(fx).is_empty());
            assert!(!fx.iter().any(|e| matches!(
                e,
                Effect::UnpublishShare
                    | Effect::SetShareLive(false)
                    | Effect::Notify(Notice::Share(_))
            )));
        }
        if let Some((timer, _)) = retry_timer(&failed) {
            m.handle(Input::Timer(timer));
        }
    }
    assert_eq!(m.state(), CallState::Joined);
}

#[test]
fn a_rebuild_should_forget_the_pull_backoffs() {
    let mut m = camera_pulling();
    for _ in 0..MAX_REJECTED_PULLS {
        let (failed, _) = camera_pull_fails(&mut m, PullFailure::Rejected);
        if let Some((timer, _)) = retry_timer(&failed) {
            m.handle(Input::Timer(timer));
        }
    }
    let (timer, _) = rebuild_timer(&m.handle(failed())).expect("armed");
    m.handle(Input::Timer(timer));
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(connected());
    assert!(
        pulls(&fx).contains(&"sfu-ana-cam".to_owned()),
        "{:?}",
        pulls(&fx)
    );
}

#[test]
fn a_stale_retry_timer_should_not_end_a_newer_backoff() {
    let mut m = camera_pulling();
    let (failed, _) = camera_pull_fails(&mut m, PullFailure::Transient);
    let (first, _) = retry_timer(&failed).expect("armed");
    assert_eq!(pulls(&m.handle(Input::Timer(first.clone()))).len(), 1);
    camera_pull_fails(&mut m, PullFailure::Transient);
    assert!(pulls(&m.handle(Input::Timer(first))).is_empty());
}

fn unheard(fx: &[Effect]) -> bool {
    fx.contains(&Effect::Notify(Notice::LeaveUnheard))
}

#[test]
fn a_leave_while_the_room_socket_is_down_should_ask_for_the_beacon() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::Disconnected));
    assert!(unheard(&m.handle(Input::Command(UserCommand::Leave))));
}

#[test]
fn a_leave_mid_rebuild_with_the_room_up_should_rely_on_op_11() {
    let mut m = joined();
    let (timer, _) = rebuild_timer(&m.handle(failed())).expect("armed");
    m.handle(Input::Timer(timer));
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert!(!unheard(&fx));
    assert_eq!(m.state(), CallState::Leaving);
}

#[test]
fn a_failure_while_the_room_socket_is_down_should_ask_for_the_beacon() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::Disconnected));
    let fx = m.handle(Input::Media(MediaEvent::Closed));
    assert!(unheard(&fx));
    assert_eq!(ends(&fx).len(), 1);
}
