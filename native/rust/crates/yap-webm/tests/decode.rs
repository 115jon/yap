//! Decoding through libvpx (feature `libvpx`, with `webrtc.lib` linked by
//! the build script). The fixtures were made with ffmpeg 8.1 (libvpx):
//!
//! - `alpha-vp9.webm`: 64x32, 6 frames at 10 fps, VP9 with alpha_mode 1;
//!   opaque red on the left half, fully transparent on the right.
//! - `opaque-vp8.webm`: 64x48, 6 frames at 10 fps, `testsrc2`, keyframes
//!   every 3 frames.
//! - `yuv444-vp9.webm`, `gbr-vp9.webm`: 32x32, 2 frames of `0x3366cc`, VP9
//!   profile 1 as YUV 4:4:4 and as RGB (what ffmpeg picks for RGB input).
#![cfg(feature = "libvpx")]

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use yap_webm::{Event, Frame, Options, Player, Sink};

const ALPHA_VP9: &[u8] = include_bytes!("fixtures/alpha-vp9.webm");
const OPAQUE_VP8: &[u8] = include_bytes!("fixtures/opaque-vp8.webm");
const YUV444_VP9: &[u8] = include_bytes!("fixtures/yuv444-vp9.webm");
const GBR_VP9: &[u8] = include_bytes!("fixtures/gbr-vp9.webm");

#[derive(Debug, Clone)]
struct Shot {
    pts: u64,
    width: u32,
    height: u32,
    bgra: Option<Vec<u8>>,
}

#[derive(Default, Clone)]
struct Log {
    shots: Arc<Mutex<Vec<Shot>>>,
    events: Arc<Mutex<Vec<Event>>>,
}

impl Sink for Log {
    fn frame(&self, frame: &Frame<'_>, pts: u64) {
        let shot = match frame {
            Frame::I420 { width, height, .. } => Shot {
                pts,
                width: *width,
                height: *height,
                bgra: None,
            },
            Frame::Bgra {
                width,
                height,
                data,
                ..
            } => Shot {
                pts,
                width: *width,
                height: *height,
                bgra: Some(data.to_vec()),
            },
        };
        self.shots.lock().unwrap().push(shot);
    }
    fn event(&self, event: Event) {
        self.events.lock().unwrap().push(event);
    }
}

fn run(bytes: &'static [u8], options: Options, until: impl Fn(&Log) -> bool) -> (Player, Log) {
    let log = Log::default();
    let player = Player::spawn(move || Ok(Arc::from(bytes)), log.clone(), options).unwrap();
    let start = Instant::now();
    while !until(&log) {
        assert!(start.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(5));
    }
    (player, log)
}

fn ended(log: &Log) -> bool {
    log.events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(e, Event::Ended | Event::Failed(_)))
}

#[test]
fn a_playing_vp8_should_show_every_frame_then_end() {
    let options = Options {
        playing: true,
        ..Options::default()
    };
    let (_player, log) = run(OPAQUE_VP8, options, ended);
    let pts: Vec<u64> = log.shots.lock().unwrap().iter().map(|s| s.pts).collect();
    assert_eq!(pts, vec![0, 100, 200, 300, 400, 500]);
}

#[test]
fn a_paused_player_should_hold_its_first_frame() {
    let (_player, log) = run(OPAQUE_VP8, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(log.shots.lock().unwrap().len(), 1);
}

#[test]
fn the_alpha_side_stream_should_become_transparency() {
    let (_player, log) = run(ALPHA_VP9, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    let shot = log.shots.lock().unwrap()[0].clone();
    let bgra = shot.bgra.expect("alpha frames come out as BGRA");
    let alpha_at = |x: usize| bgra[(16 * shot.width as usize + x) * 4 + 3];
    assert_eq!((alpha_at(8), alpha_at(56)), (255, 0));
}

#[test]
fn transparent_pixels_should_be_premultiplied() {
    let (_player, log) = run(ALPHA_VP9, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    let shot = log.shots.lock().unwrap()[0].clone();
    let bgra = shot.bgra.unwrap();
    let at = (16 * shot.width as usize + 56) * 4;
    assert_eq!(&bgra[at..at + 4], &[0, 0, 0, 0]);
}

#[test]
fn a_looping_player_should_start_over() {
    let options = Options {
        playing: true,
        looping: true,
        ..Options::default()
    };
    let (_player, log) = run(OPAQUE_VP8, options, |l| l.shots.lock().unwrap().len() >= 8);
    let pts: Vec<u64> = log.shots.lock().unwrap().iter().map(|s| s.pts).collect();
    assert_eq!(&pts[5..8], &[500, 0, 100]);
}

#[test]
fn a_seek_should_show_the_frame_at_that_time() {
    let (player, log) = run(OPAQUE_VP8, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    player.seek(420);
    let start = Instant::now();
    while log.shots.lock().unwrap().len() < 2 {
        assert!(start.elapsed() < Duration::from_secs(5));
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(log.shots.lock().unwrap()[1].pts, 400);
}

#[test]
fn a_small_box_should_scale_the_picture_down() {
    let options = Options {
        max_width: 32,
        max_height: 32,
        ..Options::default()
    };
    let (_player, log) = run(OPAQUE_VP8, options, |l| !l.shots.lock().unwrap().is_empty());
    let shot = log.shots.lock().unwrap()[0].clone();
    assert_eq!((shot.width, shot.height), (32, 24));
}

#[test]
fn loaded_should_report_the_track() {
    let (_player, log) = run(ALPHA_VP9, Options::default(), |l| {
        !l.events.lock().unwrap().is_empty()
    });
    assert_eq!(
        log.events.lock().unwrap()[0],
        Event::Loaded {
            duration_ms: 600,
            width: 64,
            height: 32,
            alpha: true
        }
    );
}

#[test]
fn a_vp9_444_stream_should_come_out_as_i420() {
    let (_player, log) = run(YUV444_VP9, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    let shot = log.shots.lock().unwrap()[0].clone();
    assert_eq!(
        (shot.width, shot.height, shot.bgra.is_none()),
        (32, 32, true)
    );
}

#[test]
fn a_vp9_rgb_stream_should_keep_its_colour() {
    // color=0x3366cc: B 0xcc, G 0x66, R 0x33 (lossy, so within a few steps).
    let (_player, log) = run(GBR_VP9, Options::default(), |l| {
        !l.shots.lock().unwrap().is_empty()
    });
    let shot = log.shots.lock().unwrap()[0].clone();
    let px = &shot.bgra.expect("RGB comes out as BGRA")[..4];
    let near = |a: u8, b: u8| a.abs_diff(b) <= 6;
    assert!(
        near(px[0], 0xcc) && near(px[1], 0x66) && near(px[2], 0x33) && px[3] == 255,
        "{px:?}"
    );
}
