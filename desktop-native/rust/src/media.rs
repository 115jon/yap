//! Media plumbing outside QML: the session the image loader sends with
//! the API's private media (`cpp/main.cpp` asks [`yap_media_auth`]), and
//! what the composer knows about a local file before it uploads it.

use std::ffi::c_char;
use std::sync::{PoisonError, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use yap_core::{Token, UploadFile};

static MEDIA_TOKEN: RwLock<Option<Token>> = RwLock::new(None);

/// The session for media requests; `None` after sign-out.
pub fn set_token(token: Option<Token>) {
    *MEDIA_TOKEN.write().unwrap_or_else(PoisonError::into_inner) = token;
}

/// `Bearer <session>` for `url` when it is the API's own private media
/// (attachments, custom emoji); `None` for anything else, which never gets
/// the session.
pub fn bearer_for(url: &str) -> Option<String> {
    if !yap_core::api::media_needs_auth(&yap_core::config::get().api_base, url) {
        return None;
    }
    let guard = MEDIA_TOKEN.read().unwrap_or_else(PoisonError::into_inner);
    guard.as_ref().map(|t| format!("Bearer {}", t.as_str()))
}

/// The loopback URL a player uses for `url` (UTF-8, `len` bytes): its
/// length, written to `out` if it fits in `cap`; 0 when `url` is not the
/// API's private media (it plays as it is). A call that writes it holds the
/// URL's token for the caller, who gives it back with
/// [`yap_media_release_local_url`]; one that does not fit (retry with
/// `cap` at least the length) holds nothing. See [`crate::loopback`].
///
/// # Safety
/// `url` must point to `len` readable bytes and `out` to `cap` writable
/// bytes (either may be null when its length is 0).
#[no_mangle]
pub unsafe extern "C" fn yap_media_local_url(
    url: *const c_char,
    len: usize,
    out: *mut c_char,
    cap: usize,
) -> usize {
    if url.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `url`.
    let bytes = unsafe { std::slice::from_raw_parts(url.cast::<u8>(), len) };
    let Ok(url) = std::str::from_utf8(bytes) else {
        return 0;
    };
    let Some(local) = crate::loopback::local_url(url) else {
        return 0;
    };
    if !out.is_null() && local.len() <= cap {
        // SAFETY: `cap` writable bytes at `out`, and `local.len() <= cap`.
        unsafe { std::ptr::copy_nonoverlapping(local.as_ptr(), out.cast::<u8>(), local.len()) };
        crate::loopback::hold(url);
    }
    local.len()
}

/// Gives back a hold taken by [`yap_media_local_url`] for `url` (UTF-8,
/// `len` bytes): the token stops working with its last holder.
///
/// # Safety
/// `url` must point to `len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn yap_media_release_local_url(url: *const c_char, len: usize) {
    if url.is_null() || len == 0 {
        return;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `url`.
    let bytes = unsafe { std::slice::from_raw_parts(url.cast::<u8>(), len) };
    if let Ok(url) = std::str::from_utf8(bytes) {
        crate::loopback::release(url);
    }
}

/// The bearer for an image URL the Qt loader is about to fetch: its length
/// when `url` is the API's own attachment or custom emoji (written to
/// `out` if it fits in `cap`), else 0. Anything else never gets it.
///
/// # Safety
/// `url` must point to `len` readable bytes and `out` to `cap` writable
/// bytes (either may be null when its length is 0).
#[no_mangle]
pub unsafe extern "C" fn yap_media_auth(
    url: *const c_char,
    len: usize,
    out: *mut c_char,
    cap: usize,
) -> usize {
    if url.is_null() || len == 0 {
        return 0;
    }
    // SAFETY: the caller guarantees `len` readable bytes at `url`.
    let bytes = unsafe { std::slice::from_raw_parts(url.cast::<u8>(), len) };
    let Ok(url) = std::str::from_utf8(bytes) else {
        return 0;
    };
    if !yap_core::api::media_needs_auth(&yap_core::config::get().api_base, url) {
        return 0;
    }
    let guard = MEDIA_TOKEN.read().unwrap_or_else(PoisonError::into_inner);
    let Some(token) = guard.as_ref() else {
        return 0;
    };
    let token = token.as_str().as_bytes();
    if !out.is_null() && token.len() <= cap {
        // SAFETY: the caller guarantees `cap` writable bytes at `out`, and
        // `token.len() <= cap`; the regions cannot overlap (ours is a heap
        // string).
        unsafe { std::ptr::copy_nonoverlapping(token.as_ptr(), out.cast::<u8>(), token.len()) };
    }
    token.len()
}

/// The MIME type the browser would send for a file name (the common ones;
/// the server stores whatever it gets, `application/octet-stream` else).
pub fn content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "ogv" => "video/ogg",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "ogg" | "oga" => "audio/ogg",
        "flac" => "audio/flac",
        "m4a" => "audio/mp4",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "7z" => "application/x-7z-compressed",
        "json" => "application/json",
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        _ => "application/octet-stream",
    }
}

/// Reads what an upload needs to know about each local file; folders and
/// files that cannot be read are skipped.
pub fn upload_files(paths: &[String]) -> Vec<UploadFile> {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    paths
        .iter()
        .enumerate()
        .filter_map(|(i, path)| {
            let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
            let name = std::path::Path::new(path)
                .file_name()?
                .to_string_lossy()
                .into_owned();
            Some(UploadFile {
                local_id: format!("upload-{stamp:x}-{i}"),
                path: path.clone(),
                content_type: content_type(&name).to_owned(),
                name,
                size: meta.len(),
            })
        })
        .collect()
}
