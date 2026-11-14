//! The phone's own camera in a call: Camera2's frames from the Android
//! shim (android/src/dev/yap/qtproto/voice/CameraShim.java, through
//! cpp/camera.cpp) into the call's camera ([`yap_voice::push_camera_frame`]),
//! and what the call sends them as.
//!
//! Frames: libwebrtc's `Camera2Capturer` (libwebrtc.jar) captures 720p30
//! onto a texture; the shim reads each one back as I420 (`toI420`) on its
//! camera thread and hands the planes over with the turn that makes them
//! upright. The call turns and converts it once; the self-view shows the
//! same frame under [`yap_voice::LOCAL_CAMERA_KEY`]
//! ([`yap_voice::set_camera_preview`], on from the first call).
//!
//! Encodings: one layer, [`yap_voice::layers::phone_camera`], from the
//! device's heat and whether its network is metered (cpp/videohub.cpp
//! reports both, [`set_limits`]); the call takes a change in place. At
//! `THERMAL_STATUS_SEVERE` there is none: the shell turns the camera off
//! (CameraLink.qml), and the last encoding stands meanwhile.

use std::sync::OnceLock;

use tokio::sync::watch;
use yap_voice::layers::{phone_camera, PHONE_CAMERA};
use yap_voice::{push_camera_frame, CameraEncoding, CameraFrame, PixelFormat};

fn encodings() -> &'static watch::Sender<Vec<CameraEncoding>> {
    static ENCODINGS: OnceLock<watch::Sender<Vec<CameraEncoding>>> = OnceLock::new();
    ENCODINGS.get_or_init(|| watch::channel(vec![PHONE_CAMERA]).0)
}

/// The call's camera encodings (`Host::camera_encodings`).
pub fn subscribe() -> watch::Receiver<Vec<CameraEncoding>> {
    encodings().subscribe()
}

/// The device's heat (Android's `THERMAL_STATUS_*`, below 0 unknown) and
/// whether its network is metered, each time one changes.
pub fn set_limits(thermal: i32, metered: bool) {
    let Some(encoding) = phone_camera(thermal, metered) else {
        return;
    };
    encodings().send_if_modified(|current| {
        let next = vec![encoding];
        let changed = *current != next;
        if changed {
            eprintln!(
                "[camera] sending {} at {:?} fps, {:?} bps",
                if encoding.scale_down_by > 1.0 {
                    "360p"
                } else {
                    "720p"
                },
                encoding.max_framerate,
                encoding.max_bitrate
            );
            *current = next;
        }
        changed
    });
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

/// cpp/camera.cpp, on the shim's camera thread: one I420 frame, `rotation`
/// degrees clockwise from upright. Returns whether it went to the call.
///
/// # Safety
/// Each non-null plane pointer is valid for reads of its length for the
/// length of the call; nothing is kept after it returns.
#[no_mangle]
pub unsafe extern "C" fn yap_mobile_camera_frame(
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
    rotation: u16,
) -> bool {
    // SAFETY: the caller's contract above, plane by plane.
    let planes = unsafe { [plane(y, y_len), plane(u, u_len), plane(v, v_len)] };
    push_camera_frame(&CameraFrame {
        format: PixelFormat::I420,
        width,
        height,
        planes,
        strides: [stride_y, stride_u, stride_v],
        rotation,
    })
}
