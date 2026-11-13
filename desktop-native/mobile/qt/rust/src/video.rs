//! Remote call video on Android: the call's decoded frames into the
//! phone's video hub (cpp/videohub.cpp), and what the phone lets a call
//! pull ([`yap_voice::demand`], shared with the desktop).
//!
//! Frames: the call's media engine hands each routed frame to [`PhoneOutput`]
//! on a media thread. The hub answers at once when nothing shows the key;
//! otherwise it lends a buffer of the right size and the frame is written
//! into it as RGBA (libyuv), once: the phone's Qt has no shader tools, so
//! the conversion is not done on the GPU (yet).
//!
//! Limits: cpp/videohub.cpp reports the device's heat, whether its network
//! is metered and whether the app is in the background
//! ([`yap_mobile_video_limits`]); [`Limits::phone`] turns those into what
//! the call may pull. A watched share is pulled only while a tile shows it
//! ([`Limits::watch_needs_tile`]): left behind the call page (Back, a
//! profile over it) or past the tile limit, it is released after the
//! usual delay and pulled again, still watched, when its tile is back.

use std::ffi::{c_char, c_void};

use yap_voice::demand::{self, Limits};
use yap_voice::{I420Frame, VideoOutput};

extern "C" {
    /// cpp/videohub.cpp, from a media thread: if something shows `key`,
    /// calls `fill(context, pixels, stride)` once with a `width` x `height`
    /// RGBA buffer to write the frame into, then keeps it as the key's
    /// newest frame (turned `rotation` degrees clockwise for display).
    fn yap_mobile_video_frame(
        key: *const c_char,
        key_len: usize,
        width: u32,
        height: u32,
        rotation: u16,
        fill: extern "C" fn(*mut c_void, *mut u8, usize, u32) -> bool,
        context: *mut c_void,
    );
}

/// The frame `fill` writes; `context` points at it for the call.
extern "C" fn fill(context: *mut c_void, pixels: *mut u8, len: usize, stride: u32) -> bool {
    if context.is_null() || pixels.is_null() {
        return false;
    }
    // SAFETY: `context` is the `I420Frame` `PhoneOutput::frame` passes,
    // alive for the hub's call that calls this; `pixels` is `len` bytes the
    // hub owns and lends only for this call.
    let (frame, dst) = unsafe {
        (
            &*context.cast::<I420Frame<'_>>(),
            std::slice::from_raw_parts_mut(pixels, len),
        )
    };
    frame.write_rgba(dst, stride)
}

/// Hands frames to the phone's video hub.
struct PhoneOutput;

impl VideoOutput for PhoneOutput {
    fn frame(&self, key: &str, frame: &I420Frame<'_>) {
        // SAFETY: `key` outlives the call; `fill` gets `frame` back only
        // while the call runs.
        unsafe {
            yap_mobile_video_frame(
                key.as_ptr().cast(),
                key.len(),
                frame.width,
                frame.height,
                frame.rotation,
                fill,
                std::ptr::from_ref(frame).cast_mut().cast(),
            );
        }
    }
}

/// Installs [`PhoneOutput`] as where call video goes (once).
pub fn install_output() {
    static OUTPUT: std::sync::Once = std::sync::Once::new();
    OUTPUT.call_once(|| {
        yap_voice::LibwebrtcEngine::set_video_output(std::sync::Arc::new(PhoneOutput));
    });
}

/// cpp/videohub.cpp: the device's thermal status (Android's
/// `THERMAL_STATUS_*`, -1 unknown), whether the network is metered and
/// whether the app is in the background, each time one changes.
#[no_mangle]
pub extern "C" fn yap_mobile_video_limits(thermal: i32, metered: bool, background: bool) {
    let limits = Limits {
        watch_needs_tile: true,
        ..Limits::phone(thermal, metered, background)
    };
    eprintln!(
        "[video] limits: thermal {thermal}, metered {metered}, background {background}: cap {:?}{}",
        limits.cap,
        if limits.paused { ", paused" } else { "" }
    );
    demand::set_limits(limits);
}
