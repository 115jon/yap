//! A one-shot WGC capture for the picker's thumbnails.
//!
//! WGC is DWM-composed and sends the target no `WM_PRINT`, so a thumbnail of
//! a busy game window does not stall its render thread the way GDI capture
//! does. Everything here is throwaway: its own device, a one-buffer pool and
//! a session that lives for a single frame.

use std::sync::mpsc;
use std::time::Duration;

use windows::core::IInspectable;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
use windows::Graphics::DirectX::DirectXPixelFormat;

use super::gpu::{frame_texture, texture_size, Gpu, Staging};
use crate::Image;

/// The first frame of `item`, cursor off, on `gpu`, or an error if none
/// arrives within `timeout`.
pub(super) fn capture(
    item: &GraphicsCaptureItem,
    gpu: &Gpu,
    timeout: Duration,
) -> Result<Image, String> {
    let size = item.Size().map_err(|e| e.to_string())?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &gpu.winrt,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(|e| e.to_string())?;
    let session = pool.CreateCaptureSession(item).map_err(|e| e.to_string())?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    let _ = session.SetIsBorderRequired(false);

    let (arrived, frame_ready) = mpsc::sync_channel::<()>(1);
    let token = pool
        .FrameArrived(
            &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |_, _| {
                // Only the first matters.
                let _ = arrived.try_send(());
                Ok(())
            }),
        )
        .map_err(|e| e.to_string())?;

    let image = session
        .StartCapture()
        .map_err(|e| e.to_string())
        .and_then(|()| {
            frame_ready
                .recv_timeout(timeout)
                .map_err(|_| "no frame in time".to_owned())
        })
        .and_then(|()| read_frame(&pool, gpu));

    // Stop the callbacks before anything they could touch goes away.
    let _ = pool.RemoveFrameArrived(token);
    let _ = session.Close();
    let _ = pool.Close();
    image
}

fn read_frame(pool: &Direct3D11CaptureFramePool, gpu: &Gpu) -> Result<Image, String> {
    let frame = pool.TryGetNextFrame().map_err(|e| e.to_string())?;
    let content = frame.ContentSize().map_err(|e| e.to_string())?;
    let texture = frame_texture(&frame).map_err(|e| e.to_string())?;
    let (texture_width, texture_height) = texture_size(&texture);
    let width = u32::try_from(content.Width).unwrap_or(0).min(texture_width);
    let height = u32::try_from(content.Height)
        .unwrap_or(0)
        .min(texture_height);
    let staging = Staging::new(&gpu.device, width, height)?;
    staging.read(&gpu.context, &texture, |rows, pitch| {
        let row_bytes = width as usize * 4;
        let mut bgra = Vec::with_capacity(row_bytes * height as usize);
        for row in 0..height as usize {
            let start = row * pitch as usize;
            bgra.extend_from_slice(&rows[start..start + row_bytes]);
        }
        Image {
            width,
            height,
            bgra,
        }
    })
}
