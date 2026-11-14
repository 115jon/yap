//! A window's icon for the picker: the window class's icon (a message-free
//! read of class memory, so a busy or hung target is never asked anything),
//! else the executable's icon.

use std::mem::size_of;

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER,
    BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyIcon, GetClassLongPtrW, GetIconInfo, GCLP_HICON, GCLP_HICONSM, HICON, ICONINFO,
};

use super::enumerate::{hwnd, process_image_path, window_pid};
use crate::ids::SourceId;
use crate::pixels::box_resize;
use crate::Image;

/// Biggest icon edge we draw.
const MAX_SIZE: u32 = 512;

pub(super) fn window_icon(source_id: &str, size: u32) -> Option<Image> {
    let SourceId::Window(value) = SourceId::parse(source_id)? else {
        return None;
    };
    if size == 0 || size > MAX_SIZE {
        return None;
    }
    let window = hwnd(value);
    let native = class_icon(window)
        .and_then(render)
        .or_else(|| executable_icon(window))?;
    let bgra = box_resize(&native.bgra, native.width, native.height, size, size)?;
    Some(Image {
        width: size,
        height: size,
        bgra,
    })
}

/// The class's big, else small, icon. Owned by the class: only read it.
fn class_icon(window: HWND) -> Option<HICON> {
    [GCLP_HICON, GCLP_HICONSM].into_iter().find_map(|which| {
        // SAFETY: a plain read of class memory; a dead window gives 0.
        let value = unsafe { GetClassLongPtrW(window, which) };
        (value != 0).then_some(HICON(value as *mut _))
    })
}

/// The icon of the window's executable, rendered and released.
fn executable_icon(window: HWND) -> Option<Image> {
    let path = window_pid(window).and_then(process_image_path)?;
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();
    // SAFETY: `wide` is NUL-terminated; `info` is the struct whose size is
    // passed.
    let got = unsafe {
        SHGetFileInfoW(
            PCWSTR(wide.as_ptr()),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut info),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        )
    };
    if got == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let image = render(info.hIcon);
    // SAFETY: the icon came from `SHGetFileInfoW` with `SHGFI_ICON`, which
    // hands its ownership to us.
    let _ = unsafe { DestroyIcon(info.hIcon) };
    image
}

/// Deletes the bitmaps `GetIconInfo` allocated.
struct Bitmaps(HBITMAP, HBITMAP);

impl Drop for Bitmaps {
    fn drop(&mut self) {
        for bitmap in [self.0, self.1] {
            if !bitmap.is_invalid() {
                // SAFETY: a bitmap we own, deleted once.
                let _ = unsafe { DeleteObject(HGDIOBJ(bitmap.0)) };
            }
        }
    }
}

/// An icon at its own size as straight BGRA. Icons without an alpha channel
/// get theirs from the icon's mask.
fn render(icon: HICON) -> Option<Image> {
    let mut info = ICONINFO::default();
    // SAFETY: a plain query into a live out struct.
    unsafe { GetIconInfo(icon, &mut info) }.ok()?;
    let bitmaps = Bitmaps(info.hbmColor, info.hbmMask);
    if info.hbmColor.is_invalid() {
        // A monochrome icon: not an app icon.
        return None;
    }

    let mut bitmap = BITMAP::default();
    // SAFETY: `bitmap` is the struct whose size is passed.
    let got = unsafe {
        GetObjectW(
            HGDIOBJ(info.hbmColor.0),
            size_of::<BITMAP>() as i32,
            Some(std::ptr::from_mut(&mut bitmap).cast()),
        )
    };
    if got == 0 {
        return None;
    }
    let width = u32::try_from(bitmap.bmWidth).ok().filter(|w| *w > 0)?;
    let height = u32::try_from(bitmap.bmHeight).ok().filter(|h| *h > 0)?;

    let mut bgra = dib_bits(info.hbmColor, width, height)?;
    if bgra.chunks_exact(4).all(|px| px[3] == 0) {
        // No alpha channel: opaque, except where the mask says transparent.
        let mask = dib_bits(info.hbmMask, width, height)?;
        for (px, mask_px) in bgra.chunks_exact_mut(4).zip(mask.chunks_exact(4)) {
            px[3] = if mask_px[0] == 0 { 255 } else { 0 };
        }
    }
    drop(bitmaps);
    Some(Image {
        width,
        height,
        bgra,
    })
}

/// A bitmap's pixels as top-down 32-bit BGRA.
fn dib_bits(bitmap: HBITMAP, width: u32, height: u32) -> Option<Vec<u8>> {
    let mut header = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: i32::try_from(width).ok()?,
            biHeight: -i32::try_from(height).ok()?,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bgra = vec![0u8; width as usize * height as usize * 4];
    // SAFETY: the screen DC is released below; `bgra` holds the `height`
    // rows of `width` pixels the call writes.
    let lines = unsafe {
        let dc = GetDC(None);
        if dc.is_invalid() {
            return None;
        }
        let lines = GetDIBits(
            dc,
            bitmap,
            0,
            height,
            Some(bgra.as_mut_ptr().cast()),
            &mut header,
            DIB_RGB_COLORS,
        );
        ReleaseDC(None, dc);
        lines
    };
    (lines != 0).then_some(bgra)
}
