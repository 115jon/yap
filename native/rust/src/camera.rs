//! Our camera's captured frames, from `cpp/camera.cpp` into the call.
//!
//! The capture (Qt Multimedia's `QCamera`, or the synthetic test pattern
//! under `YAP_FAKE_CAMERA=1`) maps each frame and calls
//! [`yap_camera_frame`] with its planes. `yap_voice` converts it once to
//! I420 while the call's camera is live, and drops it otherwise.
//!
//! Camera backgrounds: the chosen one (a voice setting) and the pictures
//! the store fetched decide the running effect (`yap_voice::background`,
//! segmentation by `yap_segment`).

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use yap_cache::voice::CameraBackground;
use yap_core::store::backgrounds::{cached_path, is_safe_id, BackgroundsCommand};
use yap_voice::background::{
    camera_effect, camera_effect_active, camera_stopped, set_camera_background, Background, Rgba,
    Segmenter, SegmenterFactory,
};
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

// --- Camera backgrounds -------------------------------------------------

/// What the camera background should be, and what is running.
#[derive(Default)]
struct Wanted {
    choice: CameraBackground,
    /// The app's data folder (fetched pictures go in its
    /// `camera-backgrounds`).
    dir: Option<PathBuf>,
    /// Fetched pictures by background id.
    ready: BTreeMap<String, String>,
    /// The key of what was last applied.
    applied: String,
    /// The user just picked a background (a failed fetch is tried again).
    picked: bool,
    /// Why the applied picture could not be used, for the settings row.
    error: Option<String>,
}

fn wanted() -> &'static Mutex<Wanted> {
    static WANTED: OnceLock<Mutex<Wanted>> = OnceLock::new();
    WANTED.get_or_init(Mutex::default)
}

/// The background chosen in the settings (`dir`: the app's data folder).
pub fn set_background_choice(choice: CameraBackground, dir: Option<PathBuf>) {
    {
        let mut w = wanted().lock().unwrap_or_else(PoisonError::into_inner);
        w.choice = choice;
        w.dir = dir;
    }
    update_background();
}

/// The user picked a background (before its choice is saved): a picture
/// whose fetch failed is asked for again.
pub fn background_picked() {
    wanted()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .picked = true;
}

/// The store fetched these pictures.
pub fn set_background_ready(ready: BTreeMap<String, String>) {
    wanted()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .ready = ready;
    update_background();
}

fn segmenter() -> SegmenterFactory {
    Arc::new(|| yap_segment::OrtSegmenter::load().map(|s| Box::new(s) as Box<dyn Segmenter>))
}

/// A picture as RGBA, at most 2560 on its long side (a cover of the
/// effect's 1280x720 needs no more).
fn decode(path: &str) -> Result<Rgba, String> {
    // The fetched file has no extension: the format comes from its bytes.
    let image = image::ImageReader::open(path)
        .and_then(|reader| reader.with_guessed_format())
        .map_err(|e| e.to_string())?
        .decode()
        .map_err(|e| e.to_string())?;
    let image = if image.width() > 2560 || image.height() > 2560 {
        image.thumbnail(2560, 2560)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Ok(Rgba {
        width: rgba.width(),
        height: rgba.height(),
        pixels: rgba.into_raw(),
    })
}

/// Where background `id`'s picture is: fetched this run, or kept on disk
/// from an earlier one. `None` for an id that is not safe as a file name.
fn picture(w: &Wanted, id: &str) -> Option<String> {
    if !is_safe_id(id) {
        return None;
    }
    w.ready.get(id).cloned().or_else(|| {
        let kept = cached_path(w.dir.as_ref()?, id)?;
        kept.is_file().then(|| kept.to_string_lossy().into_owned())
    })
}

fn update_background() {
    let mut w = wanted().lock().unwrap_or_else(PoisonError::into_inner);
    let picked = std::mem::take(&mut w.picked);
    let key = match &w.choice {
        CameraBackground::None => "none".to_owned(),
        CameraBackground::Blur { strength } => format!("blur:{strength}"),
        CameraBackground::Image { id } => match picture(&w, id) {
            Some(path) => format!("image:{id}:{path}"),
            None => format!("fetching:{id}"),
        },
    };
    if key == w.applied {
        // Unchanged. Only a pick asks again for a picture that is not in
        // (a failed fetch waits otherwise; the list's arrival sends one
        // that waited for it).
        if let (true, CameraBackground::Image { id }) = (picked, &w.choice) {
            if key.starts_with("fetching:") {
                fetch(&w, id, true);
            }
        }
        return;
    }
    w.applied.clone_from(&key);
    w.error = None;
    match w.choice.clone() {
        CameraBackground::None => set_camera_background(None),
        CameraBackground::Blur { strength } => {
            crate::video::install_output();
            set_camera_background(Some((
                Background::Blur {
                    strong: strength != "light",
                },
                segmenter(),
            )));
        }
        CameraBackground::Image { id } => {
            // The plain camera until the picture is in (as the web shows it
            // while it loads).
            set_camera_background(None);
            match picture(&w, &id) {
                Some(path) => {
                    crate::video::install_output();
                    let spawned = std::thread::Builder::new()
                        .name("yap-background-image".into())
                        .spawn(move || apply_picture(&id, &path));
                    if let Err(e) = spawned {
                        eprintln!("[camera] background image thread: {e}");
                    }
                }
                None if !is_safe_id(&id) => {
                    eprintln!("[camera] background id {id:?} refused");
                    w.error = Some("unsafe background id".into());
                }
                None => fetch(&w, &id, picked),
            }
        }
    }
}

/// Decodes background `id`'s picture and runs it, unless the choice moved
/// on meanwhile. A picture that does not decode (a format this build
/// cannot read, a broken file) is kept as the effect's error, so the
/// settings row says it could not be applied, as the web does.
fn apply_picture(id: &str, path: &str) {
    let decoded = decode(path);
    let mut w = wanted().lock().unwrap_or_else(PoisonError::into_inner);
    if !w.applied.starts_with(&format!("image:{id}:")) {
        return;
    }
    match decoded {
        Ok(rgba) => {
            drop(w);
            set_camera_background(Some((Background::Image(Arc::new(rgba)), segmenter())));
        }
        Err(why) => {
            eprintln!("[camera] background image {id}: {why}");
            w.error = Some(why);
        }
    }
}

/// Asks the store for background `id`'s picture (loading the list first
/// if it has not: a choice kept from the last run comes before it).
/// `retry`: the user picked it, so a failed fetch runs again at once.
fn fetch(w: &Wanted, id: &str, retry: bool) {
    let Some(dest) = w.dir.as_ref().and_then(|dir| cached_path(dir, id)) else {
        return;
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    crate::client::command(yap_core::Command::Backgrounds(BackgroundsCommand::Load));
    crate::client::command(yap_core::Command::Backgrounds(BackgroundsCommand::Fetch {
        id: id.to_owned(),
        dest: dest.to_string_lossy().into_owned(),
        now_ms,
        retry,
    }));
}

/// How the effect is doing, as JSON for the settings row: whether the
/// segmenter can run here, its backend, error and per-frame costs. A
/// picture that would not decode is the error too.
pub fn background_effect_info() -> String {
    let stats = camera_effect().map(|e| e.stats()).unwrap_or_default();
    let picture_error = wanted()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .error
        .clone();
    let error = if stats.error.is_empty() {
        picture_error.unwrap_or_default()
    } else {
        stats.error
    };
    serde_json::json!({
        "available": yap_segment::available(),
        "active": camera_effect_active(),
        "backend": stats.backend,
        "error": error,
        "segmentMs": stats.segment_ms,
        "frameMs": stats.frame_ms,
        "segmentations": stats.segmentations,
    })
    .to_string()
}

/// For `cpp/camera.cpp`: a background is on, so frames come here for the
/// preview too (and the preview shows the composited ones).
#[no_mangle]
pub extern "C" fn yap_camera_effect_active() -> bool {
    camera_effect_active()
}

/// For `cpp/camera.cpp`: the capture closed. The effect lets go of its
/// segmenter (the ONNX session and its GPU memory) and its frame buffers
/// until the next frame.
#[no_mangle]
pub extern "C" fn yap_camera_closed() {
    camera_stopped();
}
