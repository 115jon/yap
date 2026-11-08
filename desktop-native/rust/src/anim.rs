//! Animated pictures for `cpp/animatedpicture.cpp`: APNG decorations and
//! profile effects, animated WebP GIFs. The C++ item fetches the bytes
//! through the QML network stack (disk cache) and decodes them here on a
//! worker thread; [`yap_anim::decode`] does the work.

use std::ffi::c_void;

use yap_anim::Animation;

/// What `yap_anim_info` reports.
#[repr(C)]
pub struct YapAnimInfo {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// 0 is forever.
    pub loops: u32,
}

/// Decodes `len` bytes at `data` to fit `max_w x max_h` (0: no bound),
/// its first `max_frames` frames (0: every frame). Null when the bytes are
/// not a PNG/APNG or WebP. Free with [`yap_anim_free`].
///
/// # Safety
/// `data` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_anim_decode(
    data: *const u8,
    len: usize,
    max_w: u32,
    max_h: u32,
    max_frames: u32,
) -> *mut c_void {
    if data.is_null() || len == 0 {
        return std::ptr::null_mut();
    }
    // SAFETY: the caller guarantees `len` readable bytes at `data`.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let limit = match max_frames {
        0 => yap_anim::MAX_FRAMES,
        n => usize::try_from(n).unwrap_or(yap_anim::MAX_FRAMES),
    };
    match yap_anim::decode_frames(bytes, max_w, max_h, limit) {
        Ok(anim) => Box::into_raw(Box::new(anim)).cast(),
        Err(error) => {
            eprintln!("[anim] {error}");
            std::ptr::null_mut()
        }
    }
}

/// # Safety
/// `anim` must come from [`yap_anim_decode`] and not be freed; `out` must be
/// writable.
#[no_mangle]
pub unsafe extern "C" fn yap_anim_info(anim: *const c_void, out: *mut YapAnimInfo) {
    if anim.is_null() || out.is_null() {
        return;
    }
    // SAFETY: a live `Animation` from `yap_anim_decode`.
    let anim = unsafe { &*anim.cast::<Animation>() };
    let info = YapAnimInfo {
        width: anim.width,
        height: anim.height,
        frames: u32::try_from(anim.frames.len()).unwrap_or(u32::MAX),
        loops: anim.loops,
    };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { out.write(info) };
}

/// Frame `index`'s premultiplied RGBA (`width * height * 4` bytes, valid
/// until [`yap_anim_free`]) and its delay; null past the last frame.
///
/// # Safety
/// `anim` must come from [`yap_anim_decode`] and not be freed; `delay_ms`
/// must be writable or null.
#[no_mangle]
pub unsafe extern "C" fn yap_anim_frame(
    anim: *const c_void,
    index: u32,
    delay_ms: *mut u32,
) -> *const u8 {
    if anim.is_null() {
        return std::ptr::null();
    }
    // SAFETY: a live `Animation` from `yap_anim_decode`.
    let anim = unsafe { &*anim.cast::<Animation>() };
    let Some(frame) = anim.frames.get(index as usize) else {
        return std::ptr::null();
    };
    if !delay_ms.is_null() {
        // SAFETY: the caller guarantees a writable `u32`.
        unsafe { delay_ms.write(frame.delay_ms) };
    }
    frame.rgba.as_ptr()
}

/// Gives frame `index`'s pixels back once the caller has copied them, so a
/// long animation is never held twice (here and as `QImage`s). The pointer
/// [`yap_anim_frame`] returned for it is dead after this.
///
/// # Safety
/// `anim` must come from [`yap_anim_decode`], not be freed, and not be used
/// from another thread meanwhile.
#[no_mangle]
pub unsafe extern "C" fn yap_anim_release_frame(anim: *mut c_void, index: u32) {
    if anim.is_null() {
        return;
    }
    // SAFETY: a live `Animation` from `yap_anim_decode`, used by one thread.
    let anim = unsafe { &mut *anim.cast::<Animation>() };
    if let Some(frame) = anim.frames.get_mut(index as usize) {
        frame.rgba = Vec::new();
    }
}

/// # Safety
/// `anim` must come from [`yap_anim_decode`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn yap_anim_free(anim: *mut c_void) {
    if !anim.is_null() {
        // SAFETY: ownership returns from `yap_anim_decode`'s `Box::into_raw`.
        drop(unsafe { Box::from_raw(anim.cast::<Animation>()) });
    }
}
