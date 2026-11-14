//! Media plumbing outside QML: the C entry points `cpp/main.cpp` asks for
//! the session the image loader sends with the API's private media
//! ([`yap_media_auth`]) and for the loopback URLs players use. The session
//! store and the upload file facts are shared with the phone
//! (`yap_ui::media`).

use std::ffi::c_char;

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
    let Some(token) = yap_ui::media::token_for(url) else {
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
