//! The WGC capture item of a source.

use windows::core::factory;
use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;

use super::enumerate::{hwnd, monitors};
use crate::ids::SourceId;

pub(super) fn capture_item(id: SourceId) -> Result<GraphicsCaptureItem, String> {
    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|e| format!("Windows Graphics Capture is not available: {e}"))?;
    match id {
        SourceId::Monitor(index) => {
            let monitor = monitors()
                .into_iter()
                .nth(index)
                .ok_or_else(|| format!("monitor {index} is gone"))?;
            // SAFETY: a monitor handle from this enumeration; a stale one
            // fails with an error.
            unsafe { interop.CreateForMonitor(HMONITOR(monitor.handle as *mut _)) }
                .map_err(|e| format!("could not capture monitor {index}: {e}"))
        }
        SourceId::Window(value) => {
            // SAFETY: a window handle; a stale or uncapturable one fails with
            // an error.
            unsafe { interop.CreateForWindow(hwnd(value)) }
                .map_err(|e| format!("could not capture the window: {e}"))
        }
    }
}
