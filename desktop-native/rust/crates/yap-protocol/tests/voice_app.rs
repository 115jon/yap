//! Op 106 voice app events: the broadcasts in `tests/fixtures/voice-app`
//! (shaped as `realtime/voice-room.ts` sends them) decode as the web reads
//! them, our commands are shaped as the web sends them, and the room's own
//! checks accept what we send and drop what it drops.

use std::path::PathBuf;

use serde_json::{json, Value};
use yap_protocol::voice_app::{
    is_date_key, is_playback_owned_by, is_queueable_music, is_resolvable_url, is_station_uuid,
    lt_command, parse_playback_id, playback_id, received_at, sanitize_activity, sanitize_data_url,
    sanitize_inbound_media_url, sanitize_media_url, sanitize_public_https_url, sanitize_soundboard,
    upload_url, AppCommand, AppEvent, AutomaticEvent, CatalogSound, EnqueueMode, LtEvent,
    LtRequester, LtSeed, LtTrack, Sender, SoundboardPlay, WordleProgress,
};
use yap_protocol::{Frame, GatewayOp};

const NOW: i64 = 1_762_350_000_000;

fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/voice-app")
        .join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    let frame = Frame::decode(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(frame.gateway_op(), Some(GatewayOp::VoiceAppEvent), "{name}");
    frame.d
}

fn decode(name: &str) -> AppEvent {
    AppEvent::decode(&fixture(name)).unwrap_or_else(|| panic!("{name}: no event"))
}

fn sender() -> Sender {
    Sender {
        participant_id: "p_me".into(),
        user_id: Some("u_me".into()),
        server_id: Some("srv_1".into()),
        room_slug: "voice-srv_1-ch_v".into(),
    }
}

fn dm_sender() -> Sender {
    Sender {
        server_id: None,
        room_slug: "dm-call-dm_1".into(),
        ..sender()
    }
}

#[test]
fn server_sound_play_should_decode_with_its_media_url_and_stamp() {
    let AppEvent::SoundboardPlay(play) = decode("soundboard-play-server.json") else {
        panic!("not a play");
    };
    assert_eq!(
        *play,
        SoundboardPlay {
            server_key: "srv_1".into(),
            owner_id: "u_peer".into(),
            playback_id: "sb1:6:u_peer:snd_9".into(),
            name: "Airhorn".into(),
            sound_id: Some("snd_9".into()),
            media_url: Some("/api/soundboard/uploads/snd_9".into()),
            volume: Some(0.8),
            sent_at: Some(NOW),
            ..SoundboardPlay::default()
        }
    );
}

#[test]
fn tone_play_should_have_no_media() {
    let AppEvent::SoundboardPlay(play) = decode("soundboard-play-tone.json") else {
        panic!("not a play");
    };
    assert_eq!(
        (play.sound_id.as_deref(), play.media_url.as_deref()),
        (Some("ping"), None)
    );
}

#[test]
fn join_play_should_carry_its_automatic_event_and_cap() {
    let AppEvent::SoundboardPlay(play) = decode("soundboard-play-join.json") else {
        panic!("not a play");
    };
    assert_eq!(
        (play.automatic_event, play.max_duration_seconds),
        (Some(AutomaticEvent::Join), Some(3.0))
    );
}

#[test]
fn play_without_an_owner_should_be_dropped() {
    let mut d = fixture("soundboard-play-server.json");
    d.as_object_mut().unwrap().remove("user_id");
    d.as_object_mut().unwrap().remove("participant_id");
    assert_eq!(AppEvent::decode(&d), None);
}

#[test]
fn play_should_fall_back_to_the_participant_as_owner() {
    let mut d = fixture("soundboard-play-server.json");
    d.as_object_mut().unwrap().remove("user_id");
    let Some(AppEvent::SoundboardPlay(play)) = AppEvent::decode(&d) else {
        panic!("not a play");
    };
    assert_eq!(play.owner_id, "p_peer");
}

#[test]
fn stop_pause_and_volume_should_decode() {
    assert_eq!(
        decode("soundboard-stop.json"),
        AppEvent::SoundboardStop {
            server_key: "srv_1".into(),
            playback_id: Some("sb1:6:u_peer:snd_9".into()),
            owner_id: None,
        }
    );
    assert_eq!(
        decode("soundboard-pause-set.json"),
        AppEvent::SoundboardPauseSet {
            server_key: "srv_1".into(),
            playback_id: "sb1:6:u_peer:snd_9".into(),
            paused: true,
        }
    );
    assert_eq!(
        decode("soundboard-volume-set.json"),
        AppEvent::SoundboardVolumeSet {
            server_key: "srv_1".into(),
            playback_id: "sb1:6:u_peer:snd_9".into(),
            volume: 0.25,
        }
    );
}

#[test]
fn catalog_update_should_carry_the_sound() {
    assert_eq!(
        decode("soundboard-catalog-updated.json"),
        AppEvent::SoundboardCatalogUpdated {
            server_key: "srv_1".into(),
            sound: CatalogSound {
                id: "snd_9".into(),
                name: "Airhorn".into(),
                file_url: Some("/api/soundboard/uploads/snd_9".into()),
                emoji: Some("📯".into()),
                volume: Some(0.9),
            },
        }
    );
}

#[test]
fn lt_snapshot_should_decode_the_queue_and_anchor() {
    let AppEvent::ListenTogether(LtEvent::Snapshot {
        room_slug,
        snapshot,
    }) = decode("lt-snapshot.json")
    else {
        panic!("not a snapshot");
    };
    assert_eq!(room_slug, "voice-srv_1-ch_v");
    assert_eq!(
        (
            snapshot.paused,
            snapshot.anchor_position_ms,
            snapshot.anchor_updated_at,
            snapshot.queue.len(),
            snapshot.current_entry_id.as_deref(),
            snapshot.current_entry.as_ref().map(|e| e.track.duration()),
        ),
        (
            false,
            12_000.0,
            Some(NOW as f64),
            1,
            Some("e_1"),
            Some(Some(213_000.0))
        )
    );
}

#[test]
fn lt_playback_update_should_decode_as_a_snapshot() {
    let AppEvent::ListenTogether(LtEvent::Snapshot { snapshot, .. }) =
        decode("lt-playback-updated-empty.json")
    else {
        panic!("not a snapshot");
    };
    assert!(snapshot.queue.is_empty() && snapshot.current_entry.is_none());
}

#[test]
fn lt_error_should_keep_its_code() {
    assert_eq!(
        decode("lt-error.json"),
        AppEvent::ListenTogether(LtEvent::Error {
            room_slug: "voice-srv_1-ch_v".into(),
            code: "EMPTY_QUEUE".into(),
            message: "Nothing valid to queue".into(),
        })
    );
}

#[test]
fn activity_events_should_decode() {
    assert_eq!(
        decode("activity-start.json"),
        AppEvent::ActivityStart {
            user_id: "u_peer".into(),
            channel_id: "ch_v".into(),
            activity: "wordle".into(),
            started_at: Some(NOW as f64),
        }
    );
    assert_eq!(
        decode("activity-leave.json"),
        AppEvent::ActivityLeave {
            user_id: "u_peer".into()
        }
    );
}

#[test]
fn an_unknown_activity_should_be_dropped() {
    let mut d = fixture("activity-start.json");
    d["activity"] = json!("chess");
    assert_eq!(AppEvent::decode(&d), None);
}

#[test]
fn wordle_progress_should_decode() {
    assert_eq!(
        decode("wordle-progress.json"),
        AppEvent::WordleProgress {
            channel_id: "ch_v".into(),
            puzzle_date: "2026-11-05".into(),
            progress: WordleProgress {
                user_id: "u_peer".into(),
                guesses: vec!["crane".into(), "sloth".into()],
                streak: 3,
                finished: false,
                missed: false,
            },
        }
    );
}

#[test]
fn a_watch_snapshot_should_dedupe_viewers_and_drop_empty_streams() {
    let d = serde_json::json!({
        "type": "stream.watch.snapshot",
        "watchers_by_streamer": { "u-a": ["u-b", "u-b", ""], "u-c": [] }
    });
    let mut want = std::collections::BTreeMap::new();
    want.insert("u-a".to_owned(), vec!["u-b".to_owned()]);
    assert_eq!(
        AppEvent::decode(&d),
        Some(AppEvent::StreamWatchSnapshot(want))
    );
}

#[test]
fn other_app_events_should_be_ignored() {
    assert_eq!(
        AppEvent::decode(&fixture("stream-watch-snapshot.json")),
        None
    );
}

#[test]
fn playback_ids_should_round_trip_and_name_their_owner() {
    let id = playback_id("u_me", "snd:with:colons");
    assert_eq!(id, "sb1:4:u_me:snd:with:colons");
    assert_eq!(
        parse_playback_id(&id),
        Some(("u_me".into(), "snd:with:colons".into()))
    );
    assert!(is_playback_owned_by(&id, "u_me"));
    assert!(!is_playback_owned_by(&id, "u_m"));
    for bad in [
        "sb1:0:x:y",
        "sb1:01:x:y",
        "sb1:9:u_me:x",
        "sb2:4:u_me:x",
        "sb1:4:u_me:",
    ] {
        assert_eq!(parse_playback_id(bad), None, "{bad}");
    }
}

#[test]
fn received_at_should_ignore_stamps_more_than_five_seconds_off() {
    assert_eq!(received_at(Some(NOW - 1_000), NOW), NOW - 1_000);
    assert_eq!(received_at(Some(NOW + 1_000), NOW), NOW);
    assert_eq!(received_at(Some(NOW - 6_000), NOW), NOW);
    assert_eq!(received_at(None, NOW), NOW);
}

#[test]
fn upload_urls_should_encode_the_id() {
    assert_eq!(upload_url("a b/c"), "/api/soundboard/uploads/a%20b%2Fc");
}

fn our_play(sound_id: &str, media: Option<&str>) -> AppCommand {
    AppCommand::SoundboardPlay(Box::new(SoundboardPlay {
        server_key: "srv_1".into(),
        playback_id: playback_id("u_me", sound_id),
        name: "Sound".into(),
        sound_id: Some(sound_id.into()),
        media_url: media.map(str::to_owned),
        volume: Some(0.8),
        ..SoundboardPlay::default()
    }))
}

#[test]
fn our_play_should_be_shaped_as_the_web_sends_it() {
    let d = our_play("snd_9", Some("/api/soundboard/uploads/snd_9")).payload("u_me");
    assert_eq!(
        d,
        json!({
            "type": "soundboard.play", "server_key": "srv_1", "user_id": "u_me",
            "playback_id": "sb1:4:u_me:snd_9", "sound_id": "snd_9", "name": "Sound",
            "media_url": "/api/soundboard/uploads/snd_9", "volume": 0.8,
        })
    );
}

#[test]
fn the_room_should_accept_our_server_sound_and_tone() {
    for command in [
        our_play("snd_9", Some("/api/soundboard/uploads/snd_9")),
        our_play("ping", None),
    ] {
        let frame = command.checked(&sender(), NOW).expect("accepted");
        assert!(frame.starts_with("{\"op\":106,"), "{frame}");
    }
}

#[test]
fn the_room_should_drop_a_play_owned_by_someone_else() {
    let mut command = our_play("ping", None);
    if let AppCommand::SoundboardPlay(play) = &mut command {
        play.playback_id = playback_id("u_peer", "ping");
    }
    assert!(command.checked(&sender(), NOW).is_err());
}

#[test]
fn the_room_should_drop_an_unknown_tone_and_a_mismatched_upload() {
    assert!(our_play("boing", None).checked(&sender(), NOW).is_err());
    assert!(our_play("snd_9", Some("/api/soundboard/uploads/snd_8"))
        .checked(&sender(), NOW)
        .is_err());
}

#[test]
fn the_room_should_drop_another_servers_key() {
    let mut command = our_play("ping", None);
    if let AppCommand::SoundboardPlay(play) = &mut command {
        play.server_key = "srv_2".into();
    }
    assert!(command.checked(&sender(), NOW).is_err());
}

#[test]
fn a_dm_call_should_take_tones_but_not_volume_changes() {
    let mut command = our_play("ping", None);
    if let AppCommand::SoundboardPlay(play) = &mut command {
        play.server_key = "dm-call".into();
    }
    assert!(command.checked(&dm_sender(), NOW).is_ok());
    let volume = AppCommand::SoundboardVolumeSet {
        server_key: "dm-call".into(),
        playback_id: playback_id("u_me", "ping"),
        volume: 0.5,
    };
    assert!(volume.checked(&dm_sender(), NOW).is_err());
}

#[test]
fn the_sanitized_play_should_clamp_volume_and_duration() {
    let payload = json!({
        "type": "soundboard.play", "server_key": "srv_1", "user_id": "u_me",
        "playback_id": "sb1:4:u_me:ping", "sound_id": "ping", "name": "  Ping  ",
        "volume": 3, "max_duration_seconds": 9, "automatic_event": "leave",
        "current_time": -1,
    });
    let out = sanitize_soundboard("soundboard.play", &payload, &sender()).expect("kept");
    assert_eq!(
        (
            &out["name"],
            &out["volume"],
            &out["max_duration_seconds"],
            out.get("current_time"),
            &out["participant_id"]
        ),
        (
            &json!("Ping"),
            &json!(1.0),
            &json!(3.0),
            None,
            &json!("p_me")
        )
    );
}

#[test]
fn stop_pause_and_volume_should_need_our_own_playback() {
    let mine = playback_id("u_me", "snd_9");
    let theirs = playback_id("u_peer", "snd_9");
    let stop = |id: &str| AppCommand::SoundboardStop {
        server_key: "srv_1".into(),
        playback_id: id.into(),
    };
    assert!(stop(&mine).checked(&sender(), NOW).is_ok());
    assert!(stop(&theirs).checked(&sender(), NOW).is_err());
    let pause = AppCommand::SoundboardPauseSet {
        server_key: "srv_1".into(),
        playback_id: mine,
        paused: true,
    };
    assert!(pause.checked(&sender(), NOW).is_ok());
}

#[test]
fn data_urls_should_need_canonical_base64_audio() {
    assert!(sanitize_data_url("data:audio/wav;base64,AAAA", 1024));
    assert!(sanitize_data_url("data:audio/mpeg;base64,AAE=", 1024));
    assert!(!sanitize_data_url("data:audio/mpeg;base64,AAF=", 1024));
    assert!(!sanitize_data_url("data:image/png;base64,AAAA", 1024));
    assert!(!sanitize_data_url("data:audio/wav;base64,AAA", 1024));
    assert!(!sanitize_data_url("data:audio/wav;base64,AAAA", 2));
}

#[test]
fn media_urls_should_be_our_uploads_or_myinstants() {
    assert!(sanitize_media_url("/api/soundboard/uploads/snd_9").is_some());
    assert!(sanitize_media_url("https://www.myinstants.com/media/sounds/x.mp3").is_some());
    for bad in [
        "//evil/x",
        "/api/soundboard/uploads/a/b",
        "/api/soundboard/uploads/x?token=1",
        "/api/attachments/x",
        "http://www.myinstants.com/media/sounds/x.mp3",
    ] {
        assert!(sanitize_media_url(bad).is_none(), "{bad}");
    }
}

#[test]
fn inbound_media_urls_should_allow_only_the_dm_capability_query() {
    for good in [
        "/api/soundboard/uploads/snd_9",
        "https://www.myinstants.com/media/sounds/x.mp3",
        "/api/soundboard/uploads/snd_9?cap=sbm1.eyJhIjoxfQ.c2ln-_&room_slug=dm-call-1&playback_id=pb%3A1",
    ] {
        assert!(sanitize_inbound_media_url(good).is_some(), "{good}");
    }
    for bad in [
        "//evil/x",
        "https://evil.example/media/sounds/x.mp3",
        "https://www.myinstants.com/media/sounds/x.mp3?cap=a",
        "/api/soundboard/uploads/snd_9?token=1",
        "/api/soundboard/uploads/snd_9?cap=a&cap=b",
        "/api/soundboard/uploads/snd_9?cap=a/b",
        "/api/soundboard/uploads/snd_9?cap=a#x",
        "/api/soundboard/uploads/snd_9?cap=",
        "/api/soundboard/uploads/a/b?cap=a",
        "/api/attachments/x?cap=a",
    ] {
        assert!(sanitize_inbound_media_url(bad).is_none(), "{bad}");
    }
}

#[test]
fn activity_start_should_be_stamped_with_the_sender_and_clamped() {
    let command = AppCommand::ActivityStart {
        user_id: "u_spoof".into(),
        channel_id: " ch_v ".into(),
        activity: "wordle".into(),
        started_at: NOW + 3_600_000,
    };
    let out = sanitize_activity(&command.payload("u_me"), "u_me", "p_me", NOW)
        .expect("an activity event")
        .expect("valid");
    assert_eq!(
        out,
        json!({
            "type": "activity.start", "userId": "u_me", "participant_id": "p_me",
            "channelId": "ch_v", "activity": "wordle", "startedAt": NOW + 300_000,
        })
    );
}

#[test]
fn wordle_progress_should_pass_and_lowercase_guesses() {
    let command = AppCommand::WordleProgress {
        channel_id: "ch_v".into(),
        puzzle_date: "2026-11-05".into(),
        name: "Native Spike".into(),
        progress: WordleProgress {
            user_id: "u_me".into(),
            guesses: vec!["CRANE".into()],
            streak: 1,
            finished: false,
            missed: false,
        },
    };
    let out = sanitize_activity(&command.payload("u_me"), "u_me", "p_me", NOW)
        .expect("a wordle event")
        .expect("valid");
    assert_eq!(out["progress"]["guesses"], json!(["crane"]));
    assert!(command.checked(&sender(), NOW).is_ok());
}

#[test]
fn wordle_progress_with_a_bad_guess_or_date_should_be_rejected() {
    let base = json!({
        "type": "wordle.progress", "channel_id": "ch_v", "puzzle_date": "2026-11-05",
        "progress": { "guesses": ["crane"], "streak": 0, "finished": false, "missed": false },
    });
    let mut bad_guess = base.clone();
    bad_guess["progress"]["guesses"] = json!(["cran3"]);
    let mut bad_date = base.clone();
    bad_date["puzzle_date"] = json!("2026-02-30");
    let mut bad_streak = base;
    bad_streak["progress"]["streak"] = json!(1.5);
    for input in [bad_guess, bad_date, bad_streak] {
        assert_eq!(
            sanitize_activity(&input, "u_me", "p_me", NOW),
            Some(Err("Invalid wordle.progress payload"))
        );
    }
}

#[test]
fn date_keys_should_be_real_days() {
    assert!(is_date_key("2024-02-29"));
    assert!(!is_date_key("2023-02-29"));
    assert!(!is_date_key("2026-13-01"));
    assert!(!is_date_key("2026-1-01"));
}

fn track() -> LtTrack {
    LtTrack {
        kind: "music".into(),
        id: "youtube:dQw4w9WgXcQ".into(),
        provider: "youtube".into(),
        video_id: Some("dQw4w9WgXcQ".into()),
        title: "Never Gonna Give You Up".into(),
        duration_ms: Some(213_000.0),
        canonical_url: "https://www.youtube.com/watch?v=dQw4w9WgXcQ".into(),
        source_label: "YouTube".into(),
        ..LtTrack::default()
    }
}

#[test]
fn lt_commands_should_be_shaped_as_the_web_sends_them() {
    let room = "voice-srv_1-ch_v".to_owned();
    let enqueue = AppCommand::LtEnqueue {
        room_slug: room.clone(),
        mode: EnqueueMode::Append,
        entries: vec![LtSeed {
            track: track(),
            requester: LtRequester {
                user_id: "u_me".into(),
                display_name: "Native Spike".into(),
                ..LtRequester::default()
            },
            import_batch_id: None,
            import_batch_label: None,
        }],
    };
    let d = enqueue.payload("u_me");
    assert_eq!(
        (
            &d["type"],
            &d["mode"],
            &d["entries"][0]["track"]["videoId"],
            &d["entries"][0]["requester"]["displayName"]
        ),
        (
            &json!("listen_together.enqueue"),
            &json!("append"),
            &json!("dQw4w9WgXcQ"),
            &json!("Native Spike")
        )
    );
    let seek = AppCommand::LtSeek {
        room_slug: room,
        position_ms: 61_234.7,
        entry_id: "e_1".into(),
    };
    assert_eq!(
        seek.payload("u_me"),
        json!({
            "type": "listen_together.seek", "room_slug": "voice-srv_1-ch_v",
            "positionMs": 61_234.0, "entryId": "e_1",
        })
    );
}

#[test]
fn the_room_should_reject_pause_without_an_entry() {
    let room = "voice-srv_1-ch_v";
    assert_eq!(
        lt_command(
            &json!({ "type": "listen_together.pause", "paused": true }),
            room
        ),
        None
    );
    assert_eq!(
        lt_command(
            &json!({ "type": "listen_together.seek", "entryId": "e" }),
            room
        ),
        None
    );
    assert!(lt_command(&json!({ "type": "listen_together.clear" }), room).is_some());
    assert_eq!(
        lt_command(
            &json!({ "type": "listen_together.enqueue", "mode": "x" }),
            room
        )
        .map(|c| c["mode"].clone()),
        Some(json!("append"))
    );
}

#[test]
fn queueable_music_should_need_a_video_id_title_and_duration() {
    assert!(is_queueable_music(&track()));
    let mut no_duration = track();
    no_duration.duration_ms = None;
    let mut bad_id = track();
    bad_id.video_id = Some("x".into());
    assert!(!is_queueable_music(&no_duration));
    assert!(!is_queueable_music(&bad_id));
}

#[test]
fn resolvable_urls_should_be_youtube_or_spotify_links() {
    assert!(is_resolvable_url(
        "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ));
    assert!(is_resolvable_url("https://youtu.be/dQw4w9WgXcQ"));
    assert!(is_resolvable_url(
        "https://open.spotify.com/playlist/37i9dQZF1DXcBWIGoYBM5M"
    ));
    assert!(!is_resolvable_url("https://www.youtube.com/"));
    assert!(!is_resolvable_url("never gonna give you up"));
    assert!(!is_resolvable_url("https://example.com/watch?v=1"));
}

// ── Radio, collections and recently played ────────────────────────────────

const STATION: &str = "96062a7b-0601-11e8-ae97-52543be04c81";

#[test]
fn station_uuids_should_follow_the_rooms_pattern() {
    assert!(is_station_uuid(STATION));
    assert!(is_station_uuid(&STATION.to_ascii_uppercase()));
    for bad in [
        "",
        "96062a7b-0601-01e8-ae97-52543be04c81",
        "96062a7b-0601-11e8-7e97-52543be04c81",
        "96062a7b06011-1e8-ae97-52543be04c81",
        "96062a7b-0601-11e8-ae97-52543be04c8g",
        "96062a7b-0601-11e8-ae97-52543be04c811",
    ] {
        assert!(!is_station_uuid(bad), "{bad}");
    }
}

#[test]
fn radio_urls_should_be_public_https_only() {
    assert_eq!(
        sanitize_public_https_url("https://stream.example.org/live.mp3").as_deref(),
        Some("https://stream.example.org/live.mp3")
    );
    assert!(sanitize_public_https_url("https://8.8.8.8/a").is_some());
    for bad in [
        "http://stream.example.org/live.mp3",
        "https://user:pw@stream.example.org/live.mp3",
        "https://localhost/a",
        "https://radio.localhost/a",
        "https://box.local/a",
        "https://10.0.0.2/a",
        "https://127.0.0.1/a",
        "https://169.254.1.1/a",
        "https://172.20.1.1/a",
        "https://192.168.1.1/a",
        "https://0.1.2.3/a",
        "https://[::1]/a",
        "ftp://stream.example.org/a",
        "not a url",
    ] {
        assert!(sanitize_public_https_url(bad).is_none(), "{bad}");
    }
}

#[test]
fn a_radio_seed_should_carry_the_station_as_the_web_sends_it() {
    let seed = LtSeed::radio(
        STATION,
        "Lofi Radio",
        LtRequester {
            user_id: "u_me".into(),
            display_name: "Native Spike".into(),
            ..LtRequester::default()
        },
    );
    let d = AppCommand::LtEnqueue {
        room_slug: "voice-srv_1-ch_v".into(),
        mode: EnqueueMode::Append,
        entries: vec![seed],
    }
    .payload("u_me");
    let track = &d["entries"][0]["track"];
    assert_eq!(
        (
            &track["kind"],
            &track["provider"],
            &track["station_uuid"],
            &track["title"]
        ),
        (
            &json!("radio"),
            &json!("radio"),
            &json!(STATION),
            &json!("Lofi Radio")
        )
    );
    assert_eq!(d["entries"][0]["importBatchId"], Value::Null);
}

#[test]
fn recently_played_should_decode_and_drop_bad_entries() {
    let mut d = fixture("lt-snapshot.json");
    let entry = d["snapshot"]["queue"][0].clone();
    d["snapshot"]["recentlyPlayed"] = json!([
        { "historyId": "h_1", "playedAt": 1_762_349_000_000_i64, "entry": entry },
        { "historyId": "h_2" },
        "junk",
    ]);
    let Some(AppEvent::ListenTogether(LtEvent::Snapshot { snapshot, .. })) = AppEvent::decode(&d)
    else {
        panic!("no snapshot");
    };
    assert_eq!(snapshot.recently_played.len(), 1);
    assert_eq!(
        snapshot.recently_played[0].entry.track.title,
        "Never Gonna Give You Up"
    );
}

#[test]
fn data_urls_should_encode_as_the_room_expects() {
    use yap_protocol::voice_app::data_url;
    assert_eq!(
        data_url("audio/wav", b"hello"),
        "data:audio/wav;base64,aGVsbG8="
    );
    assert_eq!(data_url("audio/wav", b"hi"), "data:audio/wav;base64,aGk=");
    assert_eq!(data_url("audio/wav", b"abc"), "data:audio/wav;base64,YWJj");
    let bytes: Vec<u8> = (0..=255).collect();
    assert!(sanitize_data_url(&data_url("audio/mpeg", &bytes), 1024));
}
