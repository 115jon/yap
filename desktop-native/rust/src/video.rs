//! Remote video between the call and the UI.
//!
//! Frames: the call's media engine hands decoded I420 planes to
//! [`QtOutput`] on a media thread; it passes them straight to
//! `cpp/videohub.cpp`, which copies them once into a `QVideoFrame` and hands
//! that to the `VideoOutput`s showing the key, on the GUI thread. A frame
//! the GPU decoded comes as its D3D11 texture instead, which the
//! `VideoOutput` samples as it is (zero copy).
//!
//! Demand: each `VideoSurface` in QML reports the device-pixel size it shows
//! a key at, and withdraws when it goes. The largest size per key, with the
//! stage, is the call's [`VideoIntent`]: it decides what is pulled, at which
//! simulcast layer, and which frames are routed at all.

use std::collections::{BTreeMap, HashMap};
use std::ffi::c_char;
use std::sync::{Mutex, OnceLock, PoisonError};

use tokio::sync::watch;
use yap_voice::{I420Frame, RenderSize, VideoIntent, VideoOutput};

extern "C" {
    fn yap_video_frame(
        key: *const c_char,
        key_len: usize,
        width: u32,
        height: u32,
        rotation: u16,
        y: *const u8,
        stride_y: u32,
        u: *const u8,
        stride_u: u32,
        v: *const u8,
        stride_v: u32,
    );
    fn yap_video_native_frame(
        key: *const c_char,
        key_len: usize,
        buffer: *const std::ffi::c_void,
        rotation: u16,
    ) -> bool;
}

// `cargo test` links the crate without cpp/videohub.cpp, which CMake builds
// into Yap.exe only; the test binary gets these stand-ins instead.
#[cfg(test)]
mod hub_stand_ins {
    use std::ffi::c_char;

    #[no_mangle]
    extern "C" fn yap_video_frame(
        _key: *const c_char,
        _key_len: usize,
        _width: u32,
        _height: u32,
        _rotation: u16,
        _y: *const u8,
        _stride_y: u32,
        _u: *const u8,
        _stride_u: u32,
        _v: *const u8,
        _stride_v: u32,
    ) {
    }

    #[no_mangle]
    extern "C" fn yap_video_native_frame(
        _key: *const c_char,
        _key_len: usize,
        _buffer: *const std::ffi::c_void,
        _rotation: u16,
    ) -> bool {
        false
    }
}

/// Hands frames to the Qt video hub.
pub struct QtOutput;

impl VideoOutput for QtOutput {
    fn frame(&self, key: &str, frame: &I420Frame<'_>) {
        let chroma_rows = frame.height.div_ceil(2) as usize;
        let fits = |plane: &[u8], stride: u32, rows: usize| plane.len() >= stride as usize * rows;
        if !fits(frame.y, frame.stride_y, frame.height as usize)
            || !fits(frame.u, frame.stride_u, chroma_rows)
            || !fits(frame.v, frame.stride_v, chroma_rows)
        {
            return;
        }
        // SAFETY: every pointer is valid for the plane lengths checked above
        // for the length of the call; the C++ side copies before returning.
        unsafe {
            yap_video_frame(
                key.as_ptr().cast(),
                key.len(),
                frame.width,
                frame.height,
                frame.rotation,
                frame.y.as_ptr(),
                frame.stride_y,
                frame.u.as_ptr(),
                frame.stride_u,
                frame.v.as_ptr(),
                frame.stride_v,
            );
        }
    }

    fn native_frame(&self, key: &str, buffer: *const std::ffi::c_void, rotation: u16) -> bool {
        // SAFETY: `buffer` is valid for the call (the trait's contract); the
        // C++ side takes its own reference before returning.
        unsafe { yap_video_native_frame(key.as_ptr().cast(), key.len(), buffer, rotation) }
    }
}

/// Installs [`QtOutput`] as where call video and our own composited
/// camera go (once; later calls do nothing).
pub fn install_output() {
    static OUTPUT: std::sync::Once = std::sync::Once::new();
    OUTPUT.call_once(|| {
        yap_voice::LibwebrtcEngine::set_video_output(std::sync::Arc::new(QtOutput));
    });
}

/// What the visible surfaces show, by surface id.
#[derive(Default)]
struct Surfaces {
    shown: HashMap<i32, (String, RenderSize)>,
    stage: Option<String>,
    next_id: i32,
}

impl Surfaces {
    fn intent(&self) -> VideoIntent {
        let mut demand: BTreeMap<String, RenderSize> = BTreeMap::new();
        for (key, size) in self.shown.values() {
            demand
                .entry(key.clone())
                .and_modify(|largest| *largest = largest.larger(*size))
                .or_insert(*size);
        }
        VideoIntent {
            demand,
            stage: self.stage.clone(),
        }
    }
}

fn surfaces() -> &'static Mutex<Surfaces> {
    static SURFACES: OnceLock<Mutex<Surfaces>> = OnceLock::new();
    SURFACES.get_or_init(Mutex::default)
}

fn intents() -> &'static watch::Sender<VideoIntent> {
    static INTENT: OnceLock<watch::Sender<VideoIntent>> = OnceLock::new();
    INTENT.get_or_init(|| watch::channel(VideoIntent::default()).0)
}

/// The call's video intent, as it changes.
pub fn subscribe() -> watch::Receiver<VideoIntent> {
    intents().subscribe()
}

fn update(change: impl FnOnce(&mut Surfaces)) {
    let mut surfaces = surfaces().lock().unwrap_or_else(PoisonError::into_inner);
    change(&mut surfaces);
    let intent = surfaces.intent();
    intents().send_if_modified(|current| {
        let changed = *current != intent;
        *current = intent;
        changed
    });
}

/// A fresh id for a surface to report under.
pub fn new_surface() -> i32 {
    let mut surfaces = surfaces().lock().unwrap_or_else(PoisonError::into_inner);
    surfaces.next_id += 1;
    surfaces.next_id
}

/// Surface `id` shows `key` at `width` x `height` device pixels; an empty
/// key or size withdraws it.
pub fn report(id: i32, key: &str, width: u32, height: u32) {
    update(|s| {
        if key.is_empty() || width == 0 || height == 0 {
            s.shown.remove(&id);
        } else {
            s.shown
                .insert(id, (key.to_owned(), RenderSize { width, height }));
        }
    });
}

/// The participant focused on stage (empty: none).
pub fn set_stage(participant_id: &str) {
    update(|s| s.stage = Some(participant_id.to_owned()).filter(|p| !p.is_empty()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intent_should_keep_the_largest_size_per_key() {
        let mut s = Surfaces::default();
        let size = |width, height| RenderSize { width, height };
        s.shown.insert(1, ("cam-video-a".into(), size(320, 180)));
        s.shown.insert(2, ("cam-video-a".into(), size(1280, 720)));
        s.shown.insert(3, ("cam-video-b".into(), size(640, 360)));
        let intent = s.intent();
        assert_eq!(intent.demand["cam-video-a"], size(1280, 720));
        assert_eq!(intent.demand["cam-video-b"], size(640, 360));
    }
}
