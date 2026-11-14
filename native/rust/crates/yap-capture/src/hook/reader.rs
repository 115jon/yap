//! Frames from the hook: the shared texture the OBS hook copies each present
//! into, opened on our own D3D11 device, read back to the CPU and pushed to
//! `yap-voice` like a WGC frame (the desktop app's `dx11.rs` hands the
//! texture to its GPU encoder instead; the native share encodes from BGRA).
//!
//! An FP16 back buffer (a game rendering scRGB, as HDR games on Windows do)
//! goes through the WGC path's tone map first (`win/hdr.rs`), with the
//! display the game's window is mostly on.

use std::time::{Duration, Instant};

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use yap_voice::{push_screen_frame, ScreenFrame, ShareQuality};

use crate::hook::obs_ipc::{FrameMetadata, IpcError, ObsIpcChannel};
use crate::tonemap::ToneMap;
use crate::win::gpu::{texture_format, texture_size, Gpu, Staging};
use crate::win::hdr::{wanted, ColorTarget, HdrConverter};

/// How often an FP16 game's display is looked at again (HDR switched,
/// the window moved).
const COLOR_EVERY: Duration = Duration::from_secs(2);

/// How long one frame waits for the hook's texture lock before it is
/// skipped (the desktop app's 100 ms: a torn frame is worse than none).
const TEXTURE_LOCK_MS: u32 = 100;

/// How a texture's pixels are laid out, by `DXGI_FORMAT`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Layout {
    /// B8G8R8A8 / B8G8R8X8, plain or sRGB: pushed as is.
    Bgra,
    /// R8G8B8A8, plain or sRGB: red and blue swapped first.
    Rgba,
    /// R16G16B16A16_FLOAT (scRGB): tone-mapped to BGRA8 on the GPU first.
    Scrgb,
}

/// The layout of a `DXGI_FORMAT`, or `None` for one this reader cannot turn
/// into BGRA (10-bit back buffers: HDR10 or SDR, which the hook does not
/// say).
pub(crate) fn layout_of(format: u32) -> Option<Layout> {
    match format {
        // DXGI_FORMAT_B8G8R8A8_UNORM(_SRGB), B8G8R8X8_UNORM(_SRGB),
        // B8G8R8A8_TYPELESS, B8G8R8X8_TYPELESS.
        87 | 88 | 90..=93 => Some(Layout::Bgra),
        // DXGI_FORMAT_R8G8B8A8_TYPELESS, _UNORM, _UNORM_SRGB.
        27..=29 => Some(Layout::Rgba),
        // DXGI_FORMAT_R16G16B16A16_FLOAT.
        10 => Some(Layout::Scrgb),
        _ => None,
    }
}

/// The tone map of an FP16 game on its window's display: the display's
/// own when it is in HDR, the identity (scRGB 1.0 is white) when not.
fn game_tone_map(window: isize) -> ToneMap {
    wanted(ColorTarget::Window(window))
        .0
        .unwrap_or_else(|| ToneMap::from_nits(80.0, 80.0))
}

/// `rows` (RGBA, `stride` bytes apart) as tightly packed BGRA.
pub(crate) fn rgba_to_bgra(rows: &[u8], stride: u32, width: u32, height: u32) -> Vec<u8> {
    let row_bytes = width as usize * 4;
    let mut out = Vec::with_capacity(row_bytes * height as usize);
    for y in 0..height as usize {
        let start = y * stride as usize;
        let Some(row) = rows.get(start..start + row_bytes) else {
            break;
        };
        for px in row.chunks_exact(4) {
            out.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
        }
    }
    out
}

/// Why reading stopped for good.
#[derive(Debug)]
pub(crate) enum ReadError {
    Ipc(IpcError),
    Gpu(String),
    /// The back buffer is not 8-bit RGB or FP16 (`DXGI_FORMAT` value).
    Format(u32),
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ipc(e) => write!(f, "hook IPC: {e:?}"),
            Self::Gpu(e) => write!(f, "{e}"),
            Self::Format(format) => write!(f, "unsupported back buffer format {format}"),
        }
    }
}

/// The open shared texture and what reads it.
struct Opened {
    handle: u64,
    texture: ID3D11Texture2D,
    staging: Staging,
    layout: Layout,
    size: (u32, u32),
}

/// An FP16 game's tone map, and when its display was last looked at.
struct Scrgb {
    converter: HdrConverter,
    map: ToneMap,
    checked: Instant,
}

pub(crate) struct HookReader {
    ipc: ObsIpcChannel,
    gpu: Gpu,
    /// The game's window (its display, for an FP16 back buffer).
    window: isize,
    opened: Option<Opened>,
    scrgb: Option<Scrgb>,
    /// The last frame's centre pixel, BGRA (logs and the probe).
    center: [u8; 4],
}

/// The BGRA pixel at the centre of `rows`.
fn center_of(rows: &[u8], stride: u32, width: u32, height: u32, layout: Layout) -> [u8; 4] {
    let at = (height / 2) as usize * stride as usize + (width / 2) as usize * 4;
    match rows.get(at..at + 4) {
        Some(&[a, g, b, alpha]) if layout == Layout::Rgba => [b, g, a, alpha],
        Some(&[b, g, r, alpha]) => [b, g, r, alpha],
        _ => [0; 4],
    }
}

// SAFETY: the reader lives on one thread (the hook's); the device is
// multithread-protected, and the channel's handles are owned by it alone.
unsafe impl Send for HookReader {}

impl HookReader {
    pub(crate) fn new(ipc: ObsIpcChannel, window: isize) -> Result<Self, String> {
        Ok(Self {
            ipc,
            gpu: Gpu::new()?,
            window,
            opened: None,
            scrgb: None,
            center: [0; 4],
        })
    }

    pub(crate) fn center(&self) -> [u8; 4] {
        self.center
    }

    pub(crate) fn channel(&mut self) -> &mut ObsIpcChannel {
        &mut self.ipc
    }

    /// Waits a moment for the next present and pushes it. `Ok(true)` when a
    /// frame went out, `Ok(false)` when there was none (yet).
    pub(crate) fn next_frame(&mut self, quality: ShareQuality) -> Result<bool, ReadError> {
        let meta = match self.ipc.next_metadata().map_err(ReadError::Ipc)? {
            Some(meta) if meta.shared_handle != 0 => meta,
            _ => return Ok(false),
        };
        if self.opened.is_none() || self.ipc.handle_changed(&meta) {
            self.opened = None;
            if !self.ipc.acquire_texture_lock(TEXTURE_LOCK_MS) {
                return Ok(false);
            }
            let opened = self.open(&meta);
            self.ipc.release_texture_lock();
            self.opened = Some(opened?);
            self.ipc.mark_handle_opened(&meta);
        }
        let Some(opened) = self.opened.as_ref() else {
            return Ok(false);
        };
        if !self.ipc.acquire_texture_lock(TEXTURE_LOCK_MS) {
            return Ok(false);
        }
        let (width, height) = opened.size;
        let layout = opened.layout;
        if layout == Layout::Scrgb {
            let converted = match self.scrgb.as_mut() {
                Some(scrgb) => {
                    if scrgb.checked.elapsed() >= COLOR_EVERY {
                        scrgb.map = game_tone_map(self.window);
                        scrgb.checked = Instant::now();
                    }
                    scrgb
                        .converter
                        .convert(
                            &self.gpu.device,
                            &self.gpu.context,
                            &opened.texture,
                            (width, height),
                            scrgb.map,
                        )
                        .cloned()
                }
                None => Err("no tone map".to_owned()),
            };
            let converted = match converted {
                Ok(converted) => converted,
                Err(error) => {
                    self.ipc.release_texture_lock();
                    return Err(ReadError::Gpu(error));
                }
            };
            // The game's texture is free again once the copy is queued.
            opened.staging.copy(&self.gpu.context, &converted);
            self.ipc.release_texture_lock();
            let read = opened.staging.map(&self.gpu.context, |rows, stride| {
                push_screen_frame(&ScreenFrame {
                    bgra: rows,
                    stride,
                    width,
                    height,
                    quality,
                });
                center_of(rows, stride, width, height, Layout::Bgra)
            });
            self.center = read.map_err(ReadError::Gpu)?;
            return Ok(true);
        }
        let read = opened
            .staging
            .read(&self.gpu.context, &opened.texture, |rows, stride| {
                let center = center_of(rows, stride, width, height, layout);
                match layout {
                    Layout::Bgra | Layout::Scrgb => push_screen_frame(&ScreenFrame {
                        bgra: rows,
                        stride,
                        width,
                        height,
                        quality,
                    }),
                    Layout::Rgba => {
                        let bgra = rgba_to_bgra(rows, stride, width, height);
                        push_screen_frame(&ScreenFrame {
                            bgra: &bgra,
                            stride: width * 4,
                            width,
                            height,
                            quality,
                        })
                    }
                };
                center
            });
        self.ipc.release_texture_lock();
        let center = read.map_err(ReadError::Gpu)?;
        self.center = center;
        Ok(true)
    }

    fn open(&mut self, meta: &FrameMetadata) -> Result<Opened, ReadError> {
        let handle = HANDLE(meta.shared_handle as usize as *mut core::ffi::c_void);
        // SAFETY: a legacy DXGI shared handle the hook published for this
        // texture; opening it duplicates nothing we must close.
        let mut texture: Option<ID3D11Texture2D> = None;
        // SAFETY: the out pointer is a live `Option` of the asked interface.
        unsafe { self.gpu.device.OpenSharedResource(handle, &mut texture) }
            .map_err(|e| ReadError::Gpu(format!("could not open the hook's texture: {e}")))?;
        let texture =
            texture.ok_or_else(|| ReadError::Gpu("the hook's texture did not open".into()))?;
        let format = texture_format(&texture);
        let layout = layout_of(format).ok_or(ReadError::Format(format))?;
        let (tw, th) = texture_size(&texture);
        let size = (meta.width.min(tw), meta.height.min(th));
        // An FP16 texture is read after its tone map, as BGRA8.
        let staging_format = if layout == Layout::Scrgb {
            if self.scrgb.is_none() {
                self.scrgb = Some(Scrgb {
                    converter: HdrConverter::new(&self.gpu.device).map_err(ReadError::Gpu)?,
                    map: game_tone_map(self.window),
                    checked: Instant::now(),
                });
            }
            DXGI_FORMAT_B8G8R8A8_UNORM.0 as u32
        } else {
            format
        };
        let staging = Staging::with_format(&self.gpu.device, size.0, size.1, staging_format)
            .map_err(ReadError::Gpu)?;
        eprintln!(
            "[hook] reading {}x{} (format {format}{}) from pid {}",
            size.0,
            size.1,
            match (&self.scrgb, layout) {
                (Some(scrgb), Layout::Scrgb) => format!(
                    ", scRGB tone-mapped: scale {:.3}, peak {:.2}, knee {:.2}",
                    scrgb.map.scale, scrgb.map.peak, scrgb.map.knee
                ),
                _ => String::new(),
            },
            self.ipc.target_pid()
        );
        Ok(Opened {
            handle: meta.shared_handle,
            texture,
            staging,
            layout,
            size,
        })
    }

    /// Stops the channel (signals the hook to stop capturing).
    pub(crate) fn stop(&mut self) {
        self.opened = None;
        self.ipc.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_formats_should_push_as_they_are() {
        assert_eq!(layout_of(87), Some(Layout::Bgra));
        assert_eq!(layout_of(91), Some(Layout::Bgra));
    }

    #[test]
    fn rgba_formats_should_be_swizzled() {
        assert_eq!(layout_of(28), Some(Layout::Rgba));
    }

    #[test]
    fn ten_bit_back_buffers_should_be_refused() {
        assert_eq!(layout_of(24), None);
    }

    #[test]
    fn fp16_back_buffers_should_be_tone_mapped() {
        assert_eq!(layout_of(10), Some(Layout::Scrgb));
    }

    #[test]
    fn rgba_to_bgra_should_swap_red_and_blue_and_drop_the_padding() {
        let rows = [1, 2, 3, 4, 9, 9, 5, 6, 7, 8, 9, 9];
        assert_eq!(rgba_to_bgra(&rows, 6, 1, 2), [3, 2, 1, 4, 7, 6, 5, 8]);
    }
}
