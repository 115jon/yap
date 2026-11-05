//! `CallMachine` table tests: typed inputs in, effects out. No network, no
//! WebRTC.

use serde_json::{json, Value};
use yap_realtime::StopReason;
use yap_voice::wire::{Participant, RoomState, Track};
use yap_voice::{
    CallMachine, CallState, CallTarget, Effect, Input, MediaEvent, MediaState, Member, Notice,
    Playback, Processing, Published, RoomEvent, Sensitivity, Socket, TicketReplies, Timer,
    UserCommand, VoiceEvent, LEAVE_LINGER, REANNOUNCE_GRACE, SILENCE_DELAY_MS,
};

const ME: &str = "me-1";

fn target() -> CallTarget {
    CallTarget {
        server_id: "s1".into(),
        channel_id: "c1".into(),
        me_name: "Jon".into(),
        me_username: "jon".into(),
    }
}

fn tickets() -> TicketReplies {
    TicketReplies {
        room: json!({ "ticket": "room-t", "media_bootstrap": {
            "participant_id": ME, "voice_token": "tok-1",
            "ice_servers": [{ "urls": ["turn:t.example:3478"], "username": "u", "credential": "p" }],
        }}),
        voice: json!({ "ticket": "voice-t" }),
    }
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
            Effect::ArmTimer { timer, after } if *after == REANNOUNCE_GRACE => Some(timer.clone()),
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
    m.handle(Input::Tickets(Ok(tickets())));
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

#[test]
fn start_should_fetch_both_tickets() {
    let mut m = CallMachine::new(target());
    let fx = m.handle(Input::Start);
    assert_eq!(
        fx,
        vec![
            Effect::Notify(Notice::State(CallState::Connecting)),
            Effect::Notify(Notice::Status("Getting tickets")),
            Effect::FetchTickets {
                room: json!({ "audience": "room", "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1" }),
                voice: json!({ "audience": "voice", "channelId": "c1", "serverId": "s1", "roomSlug": "voice-s1-c1" }),
            },
        ]
    );
}

#[test]
fn tickets_should_open_sockets_then_media() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let fx = m.handle(Input::Tickets(Ok(tickets())));
    assert_eq!(fx[0], Effect::Notify(Notice::Status("Joining")));
    let Effect::OpenSockets { room, voice } = &fx[1] else {
        panic!("expected OpenSockets, got {:?}", fx[1]);
    };
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
    let Effect::OpenMedia(media) = &fx[2] else {
        panic!("expected OpenMedia, got {:?}", fx[2]);
    };
    assert_eq!(media.stream_id, "cam-me-1");
    assert_eq!(media.ice_servers[0].urls, vec!["turn:t.example:3478"]);
    assert_eq!(media.ice_servers[0].password, "p");
}

#[test]
fn join_should_announce_on_voice_ready_and_pull_only_after_media_connects() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Tickets(Ok(tickets())));
    let fx = m.handle(Input::Published(Ok(published())));
    assert_eq!(names(&last_roster(&fx).expect("roster")), vec![(ME, "Jon")]);

    let fx = m.handle(Input::Room(RoomEvent::Ready(room_state(&[participant(
        "ana",
        Some("Ana"),
    )]))));
    assert_eq!(sends(&fx, Socket::Room)[0]["d"]["self_mute"], false);
    assert_eq!(ops(&fx, Socket::Room), vec![15]);

    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    let announce = &sends(&fx, Socket::Voice)[0];
    assert_eq!(announce["op"], 12);
    assert_eq!(
        announce["d"]["tracks"][0],
        json!({ "participant_id": ME, "track_name": "cam-audio-me-1",
                "sfu_track_name": "my-sfu", "session_id": "cap-1", "kind": "audio" })
    );
    assert!(pulls(&fx).is_empty(), "no pull before media is up");
    assert_eq!(m.state(), CallState::Connecting);

    let fx = m.handle(connected());
    assert_eq!(
        fx,
        vec![
            Effect::Notify(Notice::Connected),
            Effect::Notify(Notice::Status("Connected")),
            Effect::Notify(Notice::State(CallState::Joined)),
            Effect::Pull(yap_voice::Remote {
                participant_id: "ana".into(),
                track_name: "mic-ana".into(),
                sfu_track_name: "sfu-ana".into(),
                session_id: "session-ana".into(),
            }),
        ]
    );
}

#[test]
fn pull_should_skip_our_own_and_non_audio_tracks() {
    let mut m = joined();
    let mut video = track("bob", "cam-bob", "sfu-bob-video");
    video.kind = "video".into();
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
fn delta_should_readd_a_dropped_row_with_its_known_name() {
    let mut m = joined();
    m.handle(Input::Room(RoomEvent::VoiceState {
        leave: true,
        participant: participant("ana", None),
    }));
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: participant("ana", None),
    }));
    assert_eq!(
        names(&last_roster(&fx).expect("roster")),
        vec![("ana", "Ana"), (ME, "Jon")]
    );
}

#[test]
fn delta_for_an_unknown_participant_should_show_someone() {
    let mut m = joined();
    let fx = m.handle(Input::Room(RoomEvent::VoiceState {
        leave: false,
        participant: participant("zed", None),
    }));
    let roster = last_roster(&fx).expect("roster");
    assert!(roster.iter().any(|r| r.id == "zed" && r.name == "Someone"));
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
    assert!(fx.is_empty(), "a cancelled grace does nothing: {fx:?}");
    assert!(m.roster().iter().any(|r| r.id == "ana"));
}

#[test]
fn client_disconnect_should_close_pull_and_drop_row_when_grace_expires() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::ClientDisconnect {
        participant_id: "ana".into(),
    }));
    let fx = m.handle(Input::Timer(grace_timer(&fx)));
    assert_eq!(closes(&fx), vec![vec!["m-ana".to_owned()]]);
    assert_eq!(names(&last_roster(&fx).expect("roster")), vec![(ME, "Jon")]);
    assert!(m.tracks().pulled_names().is_empty());
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
    assert_eq!(m.tracks().pulled_names(), vec!["sfu-ana"]);
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

    let fx = m.handle(Input::Voice(VoiceEvent::Disconnected));
    assert_eq!(states(&fx), vec![CallState::Reconnecting]);

    let fx = m.handle(voice_ready(vec![track("ana", "mic-ana", "sfu-ana")]));
    assert_eq!(closes(&fx), vec![vec!["m-bob".to_owned()]]);
    assert_eq!(ops(&fx, Socket::Voice), vec![12]);
    assert!(pulls(&fx).is_empty(), "ana's pull is kept, not redone");
    assert_eq!(states(&fx), vec![CallState::Joined]);
    assert_eq!(m.tracks().pulled_names(), vec!["sfu-ana"]);
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
    assert_eq!(
        ends(&fx),
        vec![Some("The call connection was reset. Join again.".into())]
    );
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
fn session_replaced_by_a_move_should_stop_the_call() {
    let mut m = joined();
    let fx = m.handle(Input::Voice(VoiceEvent::Stopped(StopReason::Replaced {
        code: 4010,
        reason: yap_protocol::VOICE_MOVED_CLOSE_REASON.into(),
    })));
    assert_eq!(
        ends(&fx),
        vec![Some("You were moved to another voice channel.".into())]
    );
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
fn leave_should_send_op13_op11_op11_then_close() {
    let mut m = joined();
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert_eq!(ops(&fx, Socket::Voice), vec![13, 11]);
    assert_eq!(
        sends(&fx, Socket::Voice)[0]["d"],
        json!({ "track_names": ["cam-audio-me-1"] })
    );
    assert_eq!(ops(&fx, Socket::Room), vec![11]);
    assert!(fx.contains(&Effect::ArmTimer {
        timer: Timer::Linger,
        after: LEAVE_LINGER,
    }));
    assert_eq!(m.state(), CallState::Leaving);
    assert!(ends(&fx).is_empty());

    let fx = m.handle(Input::Timer(Timer::Linger));
    assert_eq!(ends(&fx), vec![None]);
    assert_eq!(states(&fx), vec![CallState::Idle]);
}

#[test]
fn leave_before_voice_ready_should_skip_op13() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Tickets(Ok(tickets())));
    m.handle(Input::Published(Ok(published())));
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert_eq!(ops(&fx, Socket::Voice), vec![11]);
    assert_eq!(ops(&fx, Socket::Room), vec![11]);
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
    m.handle(Input::Tickets(Ok(tickets())));
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
fn ticket_failure_should_end_with_its_reason() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    let fx = m.handle(Input::Tickets(Err("room ticket: session expired".into())));
    assert_eq!(ends(&fx), vec![Some("room ticket: session expired".into())]);
}

#[test]
fn media_failure_should_end_the_call() {
    let mut m = joined();
    let fx = m.handle(Input::Media(MediaEvent::State(MediaState::Failed)));
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
    let studio = Processing::from_settings(false, false, true);
    let fx = m.handle(Input::Command(UserCommand::SetProcessing(studio)));
    assert!(fx.contains(&Effect::SetProcessing(studio)));
}

#[test]
fn processing_chosen_before_the_mic_should_apply_once_it_is_published() {
    let mut m = CallMachine::new(target());
    m.handle(Input::Start);
    m.handle(Input::Tickets(Ok(tickets())));
    let off = Processing::from_settings(false, true, false);
    assert!(m
        .handle(Input::Command(UserCommand::SetProcessing(off)))
        .is_empty());
    let fx = m.handle(Input::Published(Ok(published())));
    assert!(fx.contains(&Effect::SetProcessing(off)));
}
