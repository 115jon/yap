//! The non-Windows stubs: nothing to list, nothing to capture.

use yap_voice::{CaptureEnded, ShareCaptured, ShareQuality, ShareRequest};

use crate::{Image, Source};

/// A capture; never exists off Windows.
pub struct Capture(());

pub(crate) fn sources() -> Vec<Source> {
    Vec::new()
}

pub(crate) fn thumbnail(_source_id: &str, _max_width: u32) -> Option<Image> {
    None
}

pub(crate) fn window_icon(_source_id: &str, _size: u32) -> Option<Image> {
    None
}

pub(crate) fn start(
    _request: &ShareRequest,
    _ended: CaptureEnded,
) -> Result<(Capture, ShareCaptured), String> {
    Err("screen capture is only available on Windows".into())
}

pub(crate) fn set_quality(_capture: &Capture, _quality: ShareQuality) {}
