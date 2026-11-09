//! Animated pictures for `cpp/animatedpicture.cpp`: APNG decorations and
//! profile effects, animated WebP GIFs. The C++ item fetches the bytes
//! through the QML network stack (disk cache) and decodes them here on a
//! worker thread; [`yap_anim::decode`] does the work. Streamed GIFs decode
//! frame by frame on a worker thread through the `yap_gif_*` calls below.

use std::ffi::c_void;
use std::sync::Arc;

use yap_anim::gifstream::{GifInfo, GifStream};
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

// Streamed GIFs (`AnimatedPicture.stream`): the file is scanned once on the
// loader's worker thread (`yap_gif_open`); each item playing it opens its own
// stream (`yap_gif_stream_new`), which a worker thread asks for one frame at a
// time (`yap_gif_stream_frame`). See yap_anim::gifstream.

/// A scanned GIF: its bytes (shared by every stream over it) and its blocks.
pub struct GifFile {
    bytes: Arc<[u8]>,
    info: GifInfo,
}

/// What `yap_gif_open` reports.
#[repr(C)]
pub struct YapGifInfo {
    pub width: u32,
    pub height: u32,
    pub frames: u32,
    /// 0 is forever.
    pub loops: u32,
    /// One pass in ms (each delay clamped as browsers clamp it).
    pub duration_ms: u32,
}

/// Scans a GIF (`len` bytes at `data`, copied): null when it is not one the
/// stream can play (not a GIF, broken from its first frame, or a canvas
/// over yap-anim's limits). Free with [`yap_gif_file_free`]. The handle is
/// immutable: any thread may use it.
///
/// # Safety
/// `data` must point to `len` readable bytes; `out` must be writable.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_open(
    data: *const u8,
    len: usize,
    out: *mut YapGifInfo,
) -> *mut c_void {
    if data.is_null() || len == 0 || out.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: the caller guarantees `len` readable bytes at `data`.
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    let info = match yap_anim::gifstream::scan(bytes) {
        Ok(info) => info,
        Err(error) => {
            eprintln!("[anim] gif: {error}");
            return std::ptr::null_mut();
        }
    };
    let report = YapGifInfo {
        width: info.width,
        height: info.height,
        frames: u32::try_from(info.frames()).unwrap_or(u32::MAX),
        loops: info.loops,
        duration_ms: u32::try_from(info.duration_ms()).unwrap_or(u32::MAX),
    };
    // SAFETY: the caller guarantees `out` is writable.
    unsafe { out.write(report) };
    Box::into_raw(Box::new(GifFile {
        bytes: Arc::from(bytes),
        info,
    }))
    .cast()
}

/// Frame `index`'s delay in ms (clamped), 100 past the last frame.
///
/// # Safety
/// `file` must come from [`yap_gif_open`] and not be freed.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_delay(file: *const c_void, index: u32) -> u32 {
    if file.is_null() {
        return 100;
    }
    // SAFETY: a live `GifFile` from `yap_gif_open`.
    let file = unsafe { &*file.cast::<GifFile>() };
    file.info.delays.get(index as usize).copied().unwrap_or(100)
}

/// # Safety
/// `file` must come from [`yap_gif_open`] and not be used afterwards. Streams
/// opened over it keep their own share of the bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_file_free(file: *mut c_void) {
    if !file.is_null() {
        // SAFETY: ownership returns from `yap_gif_open`'s `Box::into_raw`.
        drop(unsafe { Box::from_raw(file.cast::<GifFile>()) });
    }
}

/// A stream over `file` at its canvas size; frames are scaled into the size
/// each `yap_gif_stream_frame` call asks for. Free with
/// [`yap_gif_stream_free`]. One thread at a time may use a stream.
///
/// # Safety
/// `file` must come from [`yap_gif_open`] and not be freed during this call.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_stream_new(file: *const c_void) -> *mut c_void {
    if file.is_null() {
        return std::ptr::null_mut();
    }
    // SAFETY: a live `GifFile` from `yap_gif_open`.
    let file = unsafe { &*file.cast::<GifFile>() };
    Box::into_raw(Box::new(GifStream::new(
        Arc::clone(&file.bytes),
        &file.info,
    )))
    .cast()
}

/// Composites frame `index` (decoding on from the frame shown, or from the
/// start) and writes it as premultiplied RGBA into `out`, `out_w x out_h`
/// pixels with `stride` bytes a row, scaled when that is not the canvas
/// size. False when the file holds fewer frames (it ended early or broke)
/// or the buffer is too small; `out` is then untouched.
///
/// # Safety
/// `stream` must come from [`yap_gif_stream_new`], not be freed, and not be
/// used from another thread meanwhile; `out` must hold `stride * out_h`
/// writable bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_stream_frame(
    stream: *mut c_void,
    index: u32,
    out: *mut u8,
    out_w: u32,
    out_h: u32,
    stride: usize,
) -> bool {
    if stream.is_null() || out.is_null() || out_w == 0 || out_h == 0 {
        return false;
    }
    // SAFETY: a live `GifStream`, used by one thread (the caller's promise).
    let stream = unsafe { &mut *stream.cast::<GifStream>() };
    match stream.seek(index as usize) {
        Ok(true) => {}
        Ok(false) => return false,
        Err(error) => {
            eprintln!("[anim] gif frame {index}: {error}");
            return false;
        }
    }
    let Some(len) = stride.checked_mul(out_h as usize) else {
        return false;
    };
    // SAFETY: the caller guarantees `stride * out_h` writable bytes.
    let out = unsafe { std::slice::from_raw_parts_mut(out, len) };
    stream.write(out, out_w, out_h, stride)
}

/// # Safety
/// `stream` must come from [`yap_gif_stream_new`] and not be used afterwards.
#[no_mangle]
pub unsafe extern "C" fn yap_gif_stream_free(stream: *mut c_void) {
    if !stream.is_null() {
        // SAFETY: ownership returns from `yap_gif_stream_new`'s `Box::into_raw`.
        drop(unsafe { Box::from_raw(stream.cast::<GifStream>()) });
    }
}
