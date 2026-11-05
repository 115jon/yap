//! Our screen share in the call machine (SCREEN_SHARE.md, browser path):
//! typed inputs in, effects out.

use serde_json::{json, Value};
use yap_voice::wire::{Participant, RoomState};
use yap_voice::{
    CallMachine, CallTarget, Effect, Input, MediaEvent, MediaState, Notice, Published, RoomEvent,
    ShareCaptured, ShareEncoding, ShareEnd, ShareNotice, SharePublished, ShareQuality,
    ShareRequest, Socket, SourceKind, TicketReplies, UserCommand, VoiceEvent,
};

const ME: &str = "me-1";

fn joined() -> CallMachine {
    let mut m = CallMachine::new(CallTarget {
        server_id: "s1".into(),
        channel_id: "c1".into(),
        me_name: "Jon".into(),
        me_username: "jon".into(),
    });
    m.handle(Input::Start);
    m.handle(Input::Tickets(Ok(TicketReplies {
        room: json!({ "ticket": "room-t", "media_bootstrap": {
            "participant_id": ME, "voice_token": "tok-1", "ice_servers": [],
        }}),
        voice: json!({ "ticket": "voice-t" }),
    })));
    m.handle(Input::Published(Ok(Published {
        session_id: "cap-1".into(),
        sfu_track_name: "my-sfu".into(),
    })));
    m.handle(Input::Room(RoomEvent::Ready(RoomState {
        participant_id: ME.into(),
        participants: Some(vec![Participant {
            id: "ana".into(),
            name: Some("Ana".into()),
            ..Participant::default()
        }]),
        voice_token: String::new(),
    })));
    m.handle(voice_ready());
    m.handle(Input::Media(MediaEvent::State(MediaState::Connected)));
    m
}

fn voice_ready() -> Input {
    Input::Voice(VoiceEvent::Ready {
        tracks: Vec::new(),
        speaking: Vec::new(),
    })
}

fn request(source: &str, audio: bool) -> ShareRequest {
    ShareRequest {
        source_id: source.into(),
        source_name: "Yap Native Share Test".into(),
        kind: if source.starts_with("monitor-") {
            SourceKind::Monitor
        } else {
            SourceKind::Window
        },
        quality: "1080p30".into(),
        audio,
    }
}

fn start(source: &str, audio: bool) -> Input {
    Input::Command(UserCommand::StartShare(request(source, audio)))
}

fn captured(token: u64, has_audio: bool) -> Input {
    Input::ShareCaptured {
        token,
        result: Ok(ShareCaptured { has_audio }),
    }
}

fn published(video: &str, audio: Option<&str>) -> Input {
    Input::SharePublished(Ok(SharePublished {
        video_sfu_track: video.into(),
        audio_sfu_track: audio.map(str::to_owned),
    }))
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

/// `(track_name, sfu_track_name)` of each op 12.
fn announced(fx: &[Effect]) -> Vec<(String, String)> {
    sends(fx, Socket::Voice)
        .iter()
        .filter(|f| f["op"] == 12)
        .map(|f| {
            let t = &f["d"]["tracks"][0];
            (
                t["track_name"].as_str().unwrap_or_default().to_owned(),
                t["sfu_track_name"].as_str().unwrap_or_default().to_owned(),
            )
        })
        .collect()
}

/// The names of each op 13.
fn stopped(fx: &[Effect]) -> Vec<Vec<String>> {
    sends(fx, Socket::Voice)
        .iter()
        .filter(|f| f["op"] == 13)
        .map(|f| {
            f["d"]["track_names"]
                .as_array()
                .map(|names| {
                    names
                        .iter()
                        .filter_map(|n| n.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// `(self_stream, self_stream_audio)` of the last op 15.
fn self_stream(fx: &[Effect]) -> Option<(bool, bool)> {
    sends(fx, Socket::Room)
        .iter()
        .rev()
        .find(|f| f["op"] == 15)
        .map(|f| {
            (
                f["d"]["self_stream"].as_bool().unwrap_or_default(),
                f["d"]["self_stream_audio"].as_bool().unwrap_or_default(),
            )
        })
}

fn notices(fx: &[Effect]) -> Vec<ShareNotice> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::Notify(Notice::Share(n)) => Some(n.clone()),
            _ => None,
        })
        .collect()
}

fn captures(fx: &[Effect]) -> Vec<u64> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::StartShareCapture { token, .. } => Some(*token),
            _ => None,
        })
        .collect()
}

fn releases(fx: &[Effect]) -> Vec<u64> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::ReleaseShareCapture(token) => Some(*token),
            _ => None,
        })
        .collect()
}

fn publishes(fx: &[Effect]) -> Vec<bool> {
    fx.iter()
        .filter_map(|e| match e {
            Effect::PublishShare { audio, .. } => Some(*audio),
            _ => None,
        })
        .collect()
}

/// A joined call sharing `window-7` with audio on `v1` / `a1`.
fn sharing() -> CallMachine {
    let mut m = joined();
    m.handle(start("window-7", true));
    m.handle(captured(1, true));
    m.handle(published("v1", Some("a1")));
    m
}

#[test]
fn start_should_capture_then_publish_with_the_preset_caps() {
    let mut m = joined();
    let fx = m.handle(start("window-7", true));
    assert_eq!(captures(&fx), vec![1]);
    // Nothing is announced before the capture lands.
    assert!(announced(&fx).is_empty() && notices(&fx).is_empty());

    let fx = m.handle(captured(1, true));
    assert!(fx.contains(&Effect::PublishShare {
        stream_id: "screen-me-1".into(),
        encoding: ShareEncoding {
            max_bitrate: 8_000_000,
            max_framerate: 30.0,
            detail: false,
        },
        audio: true,
    }));
}

#[test]
fn a_published_share_should_go_live_and_announce_both_tracks() {
    let mut m = joined();
    m.handle(start("window-7", true));
    m.handle(captured(1, true));
    let fx = m.handle(published("v1", Some("a1")));
    assert!(fx.contains(&Effect::SetShareLive(true)));
    assert_eq!(
        announced(&fx),
        vec![
            ("screen-video-me-1".into(), "v1".into()),
            ("screen-audio-me-1".into(), "a1".into()),
        ]
    );
    assert_eq!(self_stream(&fx), Some((true, true)));
    assert!(matches!(notices(&fx)[..], [ShareNotice::Started(ref s)] if s.audio));
}

#[test]
fn a_capture_without_audio_should_never_report_stream_audio() {
    let mut m = joined();
    m.handle(start("window-7", true));
    let fx = m.handle(captured(1, false));
    assert_eq!(publishes(&fx), vec![false]);
    let fx = m.handle(published("v1", None));
    assert_eq!(self_stream(&fx), Some((true, false)));
    assert_eq!(announced(&fx).len(), 1);
}

#[test]
fn stop_should_park_withdraw_both_names_and_release() {
    let mut m = sharing();
    let fx = m.handle(Input::Command(UserCommand::StopShare));
    assert!(fx.contains(&Effect::SetShareLive(false)));
    assert!(fx.contains(&Effect::UnpublishShare));
    assert_eq!(
        stopped(&fx),
        vec![
            vec!["screen-video-me-1".to_owned()],
            vec!["screen-audio-me-1".to_owned()]
        ]
    );
    assert_eq!(releases(&fx), vec![1]);
    assert_eq!(self_stream(&fx), Some((false, false)));
    assert_eq!(
        notices(&fx),
        vec![ShareNotice::Ended {
            reason: ShareEnd::Stopped,
            announced: true,
            error: None
        }]
    );
}

#[test]
fn stop_during_the_capture_should_release_it_unannounced() {
    let mut m = joined();
    m.handle(start("window-7", true));
    let fx = m.handle(Input::Command(UserCommand::StopShare));
    assert!(releases(&fx).is_empty(), "waits for the capture to land");
    let fx = m.handle(captured(1, true));
    assert_eq!(releases(&fx), vec![1]);
    assert!(publishes(&fx).is_empty() && announced(&fx).is_empty());
    assert_eq!(
        notices(&fx),
        vec![ShareNotice::Ended {
            reason: ShareEnd::Stopped,
            announced: false,
            error: None
        }]
    );
}

#[test]
fn the_latest_start_should_win_and_collapse_the_ones_between() {
    let mut m = joined();
    m.handle(start("window-1", false));
    m.handle(start("window-2", false));
    m.handle(start("window-3", false));
    let fx = m.handle(captured(1, false));
    assert_eq!(releases(&fx), vec![1]);
    // Only the last one is captured; the second never runs.
    assert_eq!(captures(&fx), vec![3]);
}

#[test]
fn a_new_source_while_live_should_release_the_old_capture_first_and_keep_the_track() {
    let mut m = sharing();
    let fx = m.handle(start("monitor-0", true));
    let release = fx
        .iter()
        .position(|e| *e == Effect::ReleaseShareCapture(1))
        .expect("old capture released");
    let capture = fx
        .iter()
        .position(|e| matches!(e, Effect::StartShareCapture { token: 2, .. }))
        .expect("new capture");
    assert!(release < capture);
    // Nothing is withdrawn: viewers keep the share's SFU track.
    assert!(stopped(&fx).is_empty() && !fx.contains(&Effect::UnpublishShare));

    m.handle(captured(2, true));
    let fx = m.handle(published("v1", Some("a1")));
    assert!(announced(&fx).is_empty(), "same SFU tracks: nothing new");
    assert!(matches!(notices(&fx)[..], [ShareNotice::Updated(ref s)]
        if s.request.source_id == "monitor-0"));
}

#[test]
fn a_restart_without_audio_should_withdraw_the_audio_name() {
    let mut m = sharing();
    m.handle(start("window-9", false));
    m.handle(captured(2, false));
    let fx = m.handle(published("v1", None));
    assert_eq!(stopped(&fx), vec![vec!["screen-audio-me-1".to_owned()]]);
    assert_eq!(self_stream(&fx), Some((true, false)));
}

#[test]
fn a_failed_restart_capture_should_end_the_share() {
    let mut m = sharing();
    m.handle(start("window-9", true));
    let fx = m.handle(Input::ShareCaptured {
        token: 2,
        result: Err("gone".into()),
    });
    assert!(fx.contains(&Effect::UnpublishShare));
    assert_eq!(self_stream(&fx), Some((false, false)));
    assert!(matches!(
        notices(&fx)[..],
        [ShareNotice::Ended {
            reason: ShareEnd::Failed,
            announced: true,
            ..
        }]
    ));
}

#[test]
fn a_failed_first_capture_should_end_unannounced_with_the_reason() {
    let mut m = joined();
    m.handle(start("window-7", true));
    let fx = m.handle(Input::ShareCaptured {
        token: 1,
        result: Err("no such window".into()),
    });
    assert_eq!(
        notices(&fx),
        vec![ShareNotice::Ended {
            reason: ShareEnd::Failed,
            announced: false,
            error: Some("no such window".into())
        }]
    );
    assert!(sends(&fx, Socket::Voice).is_empty());
}

#[test]
fn audio_off_while_live_should_mute_the_track_in_place() {
    let mut m = sharing();
    let fx = m.handle(Input::Command(UserCommand::SetShareAudio(false)));
    assert!(fx.contains(&Effect::SetShareAudioEnabled(false)));
    assert!(captures(&fx).is_empty(), "no restart");
    assert_eq!(self_stream(&fx), Some((true, false)));
}

#[test]
fn rapid_audio_toggles_should_end_at_the_last_value() {
    let mut m = joined();
    m.handle(start("window-7", true));
    m.handle(Input::Command(UserCommand::SetShareAudio(false)));
    m.handle(Input::Command(UserCommand::SetShareAudio(true)));
    m.handle(Input::Command(UserCommand::SetShareAudio(false)));
    m.handle(captured(1, true));
    let fx = m.handle(published("v1", Some("a1")));
    assert!(fx.contains(&Effect::SetShareAudioEnabled(false)));
    assert_eq!(self_stream(&fx), Some((true, false)));
}

#[test]
fn a_quality_change_should_apply_in_place() {
    let mut m = sharing();
    let fx = m.handle(Input::Command(UserCommand::SetShareQuality(
        "720p60".into(),
    )));
    assert!(fx.contains(&Effect::SetShareEncoding(ShareEncoding {
        max_bitrate: 5_000_000,
        max_framerate: 60.0,
        detail: false,
    })));
    assert!(fx.contains(&Effect::SetShareCaptureQuality {
        token: 1,
        quality: ShareQuality::of("720p60"),
    }));
    assert!(captures(&fx).is_empty());
}

#[test]
fn the_source_going_away_should_end_the_share_as_source_ended() {
    let mut m = sharing();
    let fx = m.handle(Input::ShareCaptureEnded { token: 1 });
    assert!(matches!(
        notices(&fx)[..],
        [ShareNotice::Ended {
            reason: ShareEnd::SourceEnded,
            announced: true,
            ..
        }]
    ));
}

#[test]
fn a_dead_capture_should_be_replaced_by_a_newer_start() {
    let mut m = sharing();
    m.handle(start("window-9", false));
    // The old one ends while the new capture is in flight: the new one runs.
    let fx = m.handle(Input::ShareCaptureEnded { token: 1 });
    assert!(notices(&fx).is_empty());
    m.handle(captured(2, false));
    let fx = m.handle(published("v1", None));
    assert!(matches!(notices(&fx)[..], [ShareNotice::Updated(_)]));
}

#[test]
fn a_reidentify_should_announce_the_live_share_again() {
    let mut m = sharing();
    let fx = m.handle(voice_ready());
    let names: Vec<String> = announced(&fx).into_iter().map(|(n, _)| n).collect();
    assert!(names.contains(&"screen-video-me-1".to_owned()));
    assert!(names.contains(&"screen-audio-me-1".to_owned()));
}

#[test]
fn a_share_before_the_mic_is_published_should_wait_for_it() {
    let mut m = CallMachine::new(CallTarget {
        server_id: "s1".into(),
        channel_id: "c1".into(),
        me_name: "Jon".into(),
        me_username: "jon".into(),
    });
    m.handle(Input::Start);
    m.handle(Input::Tickets(Ok(TicketReplies {
        room: json!({ "ticket": "room-t", "media_bootstrap": {
            "participant_id": ME, "voice_token": "tok-1", "ice_servers": [],
        }}),
        voice: json!({ "ticket": "voice-t" }),
    })));
    let fx = m.handle(start("window-7", false));
    assert!(captures(&fx).is_empty());
    let fx = m.handle(Input::Published(Ok(Published {
        session_id: "cap-1".into(),
        sfu_track_name: "my-sfu".into(),
    })));
    assert_eq!(captures(&fx), vec![1]);
}

#[test]
fn leaving_should_release_the_capture_without_withdrawing() {
    let mut m = sharing();
    let fx = m.handle(Input::Command(UserCommand::Leave));
    assert_eq!(releases(&fx), vec![1]);
    assert!(stopped(&fx)
        .iter()
        .all(|names| !names.iter().any(|n| n.starts_with("screen-"))));
}
