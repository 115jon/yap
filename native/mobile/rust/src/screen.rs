//! The phone's screen share: MediaProjection's frames from the Android shim
//! (android/src/dev/yap/qtproto/voice/ScreenShim.java, through
//! cpp/screen.cpp) into the call's share ([`yap_voice::push_screen_i420`]),
//! and the device's sound with them when asked.
//!
//! The user consents first, in the system's own dialog, from the share key
//! (ShareLink.qml); the call then starts the capture through
//! [`PhoneScreen`], which hands the consent to libwebrtc's
//! `ScreenCapturerAndroid` (libwebrtc.jar) once the call's foreground
//! service carries the mediaProjection type. The virtual display is sized
//! to fit the preset's box whichever way up the screen is, each frame is
//! read back as I420 on the capture's thread, and the share sends it as
//! one VP8 layer. Android's own Stop (the cast notification, the status
//! bar's chip) ends the projection: the capture reports its source gone.
//!
//! Sound: AudioPlaybackCapture (API 29+) of what other apps play as media,
//! games or unknown, never Yap's own: the call plays as voice
//! communication, which other apps cannot capture, and Yap's uid is left
//! out besides, so nobody hears the call back through the share.
//!
//! The preset follows the device's heat and network
//! ([`yap_voice::layers::phone_share`]): a live share steps between
//! [`PHONE_SHARE`] and [`PHONE_SHARE_LOW`] in place; at
//! `THERMAL_STATUS_SEVERE` the shell stops it (ShareLink.qml).

use std::sync::{Mutex, PoisonError};

use yap_core::Command;
use yap_ui::client;
use yap_voice::layers::{phone_share, PHONE_SHARE, PHONE_SHARE_LOW};
use yap_voice::{
    push_screen_audio, push_screen_i420, CaptureEnded, ScreenI420, ShareCaptured, ShareCapturer,
    ShareQuality, ShareRequest,
};

extern "C" {
    /// cpp/screen.cpp, blocking (the driver calls it off the runtime):
    /// starts the capture with the consent the user gave last. Returns the
    /// capture's number shifted left once, with bit 0 set when the
    /// device's sound is captured too; below 0 it did not start (`-1` no
    /// consent, `-2` no mediaProjection service, `-3` the capture failed).
    fn yap_mobile_screen_start(long_side: u32, short_side: u32, fps: u32, audio: bool) -> i64;
    /// Stops capture `capture` (nothing when another one runs).
    fn yap_mobile_screen_stop(capture: i64);
    /// A new box and pacing for capture `capture`.
    fn yap_mobile_screen_format(capture: i64, long_side: u32, short_side: u32, fps: u32);
}

/// The running capture: its number, the box its frames are sent within,
/// and what to call when Android ends it.
struct Running {
    capture: i64,
    quality: ShareQuality,
    ended: CaptureEnded,
}

static RUNNING: Mutex<Option<Running>> = Mutex::new(None);

/// The device's last heat and metering ([`set_limits`]).
static LIMITS: Mutex<(i32, bool)> = Mutex::new((-1, false));

fn running() -> std::sync::MutexGuard<'static, Option<Running>> {
    RUNNING.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The preset the device allows now (the low one when it allows none: the
/// shell is stopping the share then).
fn allowed() -> &'static str {
    let (thermal, metered) = *LIMITS.lock().unwrap_or_else(PoisonError::into_inner);
    phone_share(thermal, metered).unwrap_or(PHONE_SHARE_LOW)
}

/// A phone preset's box as long side and short side.
fn sides(quality: ShareQuality) -> (u32, u32) {
    (
        quality.width.max(quality.height),
        quality.width.min(quality.height),
    )
}

/// The call's screen capture on the phone (`Host::capturer`).
#[derive(Debug, Clone, Copy, Default)]
pub struct PhoneScreen;

/// A started capture; dropping it stops it.
pub struct Capture(i64);

impl Drop for Capture {
    fn drop(&mut self) {
        {
            let mut running = running();
            if running.as_ref().is_some_and(|r| r.capture == self.0) {
                *running = None;
            }
        }
        // SAFETY: a plain C function, callable from any thread.
        unsafe { yap_mobile_screen_stop(self.0) };
    }
}

impl ShareCapturer for PhoneScreen {
    type Capture = Capture;

    fn start(
        &self,
        request: &ShareRequest,
        ended: CaptureEnded,
    ) -> Result<(Capture, ShareCaptured), String> {
        // Started at what the device allows now; the call hears of a lower
        // preset right after, and its sender follows.
        let preset = if request.quality == PHONE_SHARE {
            allowed()
        } else {
            request.quality.as_str()
        };
        let quality = ShareQuality::of(preset);
        let (long, short) = sides(quality);
        // SAFETY: a plain C function; it blocks this (blocking) thread.
        let code = unsafe { yap_mobile_screen_start(long, short, quality.fps, request.audio) };
        if code < 0 {
            return Err(match code {
                -1 => "denied",
                -2 => "service",
                _ => "unknown",
            }
            .into());
        }
        let capture = code >> 1;
        let has_audio = code & 1 == 1;
        *running() = Some(Running {
            capture,
            quality,
            ended,
        });
        if preset != request.quality {
            client::command(Command::SetVoiceShareQuality(preset.to_owned()));
        }
        Ok((Capture(capture), ShareCaptured { has_audio }))
    }

    fn set_quality(&self, capture: &Capture, quality: ShareQuality) {
        {
            let mut running = running();
            match running.as_mut() {
                Some(r) if r.capture == capture.0 => r.quality = quality,
                _ => return,
            }
        }
        let (long, short) = sides(quality);
        // SAFETY: a plain C function, callable from any thread.
        unsafe { yap_mobile_screen_format(capture.0, long, short, quality.fps) };
    }
}

/// The device's heat (Android's `THERMAL_STATUS_*`, below 0 unknown) and
/// whether its network is metered, each time one changes: a live phone
/// share moves to the preset they allow.
pub fn set_limits(thermal: i32, metered: bool) {
    *LIMITS.lock().unwrap_or_else(PoisonError::into_inner) = (thermal, metered);
    let Some(preset) = phone_share(thermal, metered) else {
        return;
    };
    let current = running().as_ref().map(|r| r.quality);
    if current.is_some_and(|q| q != ShareQuality::of(preset)) {
        eprintln!("[share] sending the screen as {preset}");
        client::command(Command::SetVoiceShareQuality(preset.to_owned()));
    }
}

/// A plane as a slice; null or empty is an empty plane.
///
/// # Safety
/// A non-null `data` must be valid for reads of `len` bytes for `'a`.
unsafe fn plane<'a>(data: *const u8, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller's contract above.
        unsafe { std::slice::from_raw_parts(data, len) }
    }
}

/// cpp/screen.cpp, on the capture's thread: one I420 frame of the screen.
/// Returns whether it went to the call.
///
/// # Safety
/// Each non-null plane pointer is valid for reads of its length for the
/// length of the call; nothing is kept after it returns.
#[no_mangle]
pub unsafe extern "C" fn yap_mobile_screen_frame(
    y: *const u8,
    y_len: usize,
    stride_y: u32,
    u: *const u8,
    u_len: usize,
    stride_u: u32,
    v: *const u8,
    v_len: usize,
    stride_v: u32,
    width: u32,
    height: u32,
) -> bool {
    let Some(quality) = running().as_ref().map(|r| r.quality) else {
        return false;
    };
    // The box turned the way the frame is.
    let (long, short) = sides(quality);
    let quality = if height > width {
        ShareQuality {
            width: short,
            height: long,
            ..quality
        }
    } else {
        ShareQuality {
            width: long,
            height: short,
            ..quality
        }
    };
    // SAFETY: the caller's contract above, plane by plane.
    let planes = unsafe { [plane(y, y_len), plane(u, u_len), plane(v, v_len)] };
    push_screen_i420(&ScreenI420 {
        planes,
        strides: [stride_y, stride_u, stride_v],
        width,
        height,
        quality,
    })
}

/// cpp/screen.cpp, on the shim's audio thread: interleaved 48 kHz stereo
/// samples of the device's sound.
///
/// # Safety
/// A non-null `samples` is valid for reads of `count` samples for the
/// length of the call.
#[no_mangle]
pub unsafe extern "C" fn yap_mobile_screen_audio(samples: *const i16, count: usize) {
    if samples.is_null() || count == 0 {
        return;
    }
    // SAFETY: the caller's contract above.
    push_screen_audio(unsafe { std::slice::from_raw_parts(samples, count) });
}

/// cpp/screen.cpp: Android ended capture `capture` (the user stopped it
/// from the system's notification or chip, or the system did).
#[no_mangle]
pub extern "C" fn yap_mobile_screen_ended(capture: i64) {
    let running = running();
    if let Some(r) = running.as_ref().filter(|r| r.capture == capture) {
        (r.ended)();
    }
}

/// The call notification's Stop sharing.
#[no_mangle]
pub extern "C" fn yap_mobile_share_stop() {
    client::command(Command::StopVoiceShare);
}
