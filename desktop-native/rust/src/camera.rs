//! Our camera's captured frames, from `cpp/camera.cpp` into the call.
//!
//! The capture (Qt Multimedia's `QCamera`, or the synthetic test pattern
//! under `YAP_FAKE_CAMERA=1`) maps each frame and calls
//! [`yap_camera_frame`] with its planes. `yap_voice` converts it once to
//! I420 while the call's camera is live, and drops it otherwise.

use yap_voice::{push_camera_frame, CameraFrame, PixelFormat};

fn format(code: i32) -> Option<PixelFormat> {
    Some(match code {
        0 => PixelFormat::I420,
        1 => PixelFormat::Nv12,
        2 => PixelFormat::Nv21,
        3 => PixelFormat::Yuy2,
        4 => PixelFormat::Uyvy,
        5 => PixelFormat::Bgra,
        6 => PixelFormat::Rgba,
        _ => return None,
    })
}

/// A plane as a slice; null or empty is an empty plane.
///
/// # Safety
///
/// A non-null `data` must be valid for reads of `len` bytes for `'a`.
unsafe fn plane<'a>(data: *const u8, len: usize) -> &'a [u8] {
    if data.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: the caller's contract above.
        unsafe { std::slice::from_raw_parts(data, len) }
    }
}

/// One captured frame (format codes as `cpp/camera.cpp` sends them: 0 I420,
/// 1 NV12, 2 NV21, 3 YUY2, 4 UYVY, 5 BGRA, 6 RGBA). Returns whether it went
/// to the call.
///
/// # Safety
///
/// Each non-null plane pointer must be valid for reads of its length for
/// the length of the call. Nothing is kept after it returns.
#[no_mangle]
pub unsafe extern "C" fn yap_camera_frame(
    format_code: i32,
    width: u32,
    height: u32,
    p0: *const u8,
    len0: usize,
    stride0: u32,
    p1: *const u8,
    len1: usize,
    stride1: u32,
    p2: *const u8,
    len2: usize,
    stride2: u32,
) -> bool {
    let Some(format) = format(format_code) else {
        return false;
    };
    // SAFETY: the caller's contract above, plane by plane.
    let planes = unsafe { [plane(p0, len0), plane(p1, len1), plane(p2, len2)] };
    push_camera_frame(&CameraFrame {
        format,
        width,
        height,
        planes,
        strides: [stride0, stride1, stride2],
    })
}
