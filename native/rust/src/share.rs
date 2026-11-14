//! The screen share picker's side of `yap-capture`: the sources as JSON for
//! QML, and their thumbnails and window icons for `cpp/shareprovider.cpp`
//! (`image://share/...`). The share itself runs in the call
//! (`client::voice`, with `yap_capture::Capturer`).

use std::ffi::{c_char, c_void};

use serde_json::{json, Value};

/// The sources (`get_screen_sources`): `[{id, name, kind, appName, ours}]`.
/// In the test mode (`YAP_SHARE_ONLY_OURS=1`) other apps' windows keep
/// their place but not their titles, so a screenshot of the picker shows
/// nothing of them.
pub fn sources_json() -> String {
    let redact = yap_capture::only_ours();
    let list: Vec<Value> = yap_capture::sources()
        .into_iter()
        .map(|s| {
            let hidden = redact && !s.ours && s.kind == yap_capture::SourceKind::Window;
            json!({
                "id": s.id,
                "name": if hidden { "Another app (hidden in test mode)".to_owned() } else { s.name },
                "kind": s.kind.as_str(),
                "appName": if hidden { String::new() } else { s.app_name },
                "ours": s.ours,
            })
        })
        .collect();
    Value::Array(list).to_string()
}

/// Whether the test mode keeps `id`'s pictures back (anything not ours).
fn withheld(id: &str) -> bool {
    yap_capture::only_ours() && !yap_capture::sources().iter().any(|s| s.id == id && s.ours)
}

type Sink = extern "C" fn(ctx: *mut c_void, bgra: *const u8, width: u32, height: u32, stride: u32);

/// A source's thumbnail (`icon` false, at most `size` wide) or its window's
/// icon (`size` square), handed to `sink` once. `false` when there is none.
/// Blocks (a capture snapshot); the provider calls it off the GUI thread.
///
/// # Safety
///
/// `id` must be valid for reads of `id_len` bytes; `ctx` is passed back to
/// `sink` untouched, and the pixels it gets live only for that call.
#[no_mangle]
pub unsafe extern "C" fn yap_share_image(
    id: *const c_char,
    id_len: usize,
    icon: bool,
    size: u32,
    sink: Sink,
    ctx: *mut c_void,
) -> bool {
    if id.is_null() {
        return false;
    }
    // SAFETY: the caller's contract above.
    let bytes = unsafe { std::slice::from_raw_parts(id.cast::<u8>(), id_len) };
    let Ok(id) = std::str::from_utf8(bytes) else {
        return false;
    };
    if withheld(id) {
        return false;
    }
    let image = if icon {
        yap_capture::window_icon(id, size.clamp(8, 256))
    } else {
        yap_capture::thumbnail(id, size.clamp(64, 1920))
    };
    let Some(image) = image else {
        return false;
    };
    let rows = image.width as usize * image.height as usize * 4;
    if image.width == 0 || image.height == 0 || image.bgra.len() < rows {
        return false;
    }
    sink(
        ctx,
        image.bgra.as_ptr(),
        image.width,
        image.height,
        image.width * 4,
    );
    true
}
