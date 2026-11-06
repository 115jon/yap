//! Screen share capture for the native Yap client (Windows).
//!
//! Three jobs, all handed to or used by `yap-voice`:
//!
//! - the picker: [`sources`] lists what can be shared, [`thumbnail`] and
//!   [`window_icon`] draw it;
//! - [`Capturer`], the [`yap_voice::ShareCapturer`] that captures a picked
//!   monitor or window with Windows Graphics Capture and pushes its BGRA
//!   frames to [`yap_voice::push_screen_frame`];
//! - the source's audio (WASAPI loopback: the system mix for a monitor, the
//!   window's process tree for a window) pushed to
//!   [`yap_voice::push_screen_audio`].
//!
//! Off Windows every call is a stub (no sources, no images, every start
//! fails).
//!
//! Two environment switches exist for testing without exposing anything
//! private:
//!
//! - `YAP_SHARE_ONLY_OURS=1`: thumbnails and captures are refused for every
//!   source except windows of this process and windows titled
//!   [`TEST_WINDOW_TITLE`]. Monitors are always refused. Listing stays on.
//! - `YAP_FAKE_SHARE_AUDIO=1`: share audio is a synthetic 440 Hz sine
//!   instead of any loopback.

#![cfg_attr(not(windows), allow(dead_code))]

mod cache;
mod filter;
#[cfg(windows)]
pub mod hook;
mod ids;
mod pace;
mod pcm;
mod pixels;
mod stop;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;

#[cfg(not(windows))]
mod other;
#[cfg(not(windows))]
use other as platform;

pub use platform::Capture;
pub use yap_voice::SourceKind;

use yap_voice::{CaptureEnded, ShareCaptured, ShareCapturer, ShareQuality, ShareRequest};

/// The title that marks a window as ours to the test-safety mode.
pub const TEST_WINDOW_TITLE: &str = "Yap Native Share Test";

/// Why [`only_ours`] mode turned a source down.
pub const ONLY_OURS_REFUSAL: &str = "not allowed in YAP_SHARE_ONLY_OURS mode";

/// One shareable source for the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// `monitor-<index>` or `window-<hwnd as isize>`.
    pub id: String,
    pub name: String,
    pub kind: SourceKind,
    /// The owning executable's name; empty for monitors.
    pub app_name: String,
    /// A window of this process, or one titled [`TEST_WINDOW_TITLE`].
    pub ours: bool,
}

/// A BGRA image, tightly packed (stride is `width * 4`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

/// What can be shared: monitors first (index order), then windows (Z order,
/// our own windows last).
pub fn sources() -> Vec<Source> {
    platform::sources()
}

/// A one-shot capture of a source, box-downscaled to at most `max_width`
/// wide with its aspect kept. Cached for 4 s per `(source_id, max_width)`.
/// Blocks (up to 600 ms for the capture): call it from a worker thread.
/// `None` when the source cannot be captured in time or the test-safety
/// mode refuses it.
pub fn thumbnail(source_id: &str, max_width: u32) -> Option<Image> {
    platform::thumbnail(source_id, max_width)
}

/// A window's icon at `size` x `size`, straight (not premultiplied) alpha.
/// Windows only; monitors have none.
pub fn window_icon(source_id: &str, size: u32) -> Option<Image> {
    platform::window_icon(source_id, size)
}

static GAME_CAPTURE_HOOK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// The "Game capture hook" setting (on by default): a shared window is
/// also tried with the OBS-signed hook (see [`hook`]); off, always WGC.
pub fn set_game_capture_hook(on: bool) {
    GAME_CAPTURE_HOOK.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Whether the hook is tried for a shared window.
pub fn game_capture_hook() -> bool {
    GAME_CAPTURE_HOOK.load(std::sync::atomic::Ordering::Relaxed)
}

/// Where the hook keeps its files (the profile's data folder). Without it
/// nothing is staged or remembered.
pub fn set_data_dir(dir: std::path::PathBuf) {
    #[cfg(windows)]
    hook::set_data_dir(dir);
    #[cfg(not(windows))]
    let _ = dir;
}

/// `YAP_SHARE_ONLY_OURS=1`: only our own windows may be captured.
pub fn only_ours() -> bool {
    env_flag("YAP_SHARE_ONLY_OURS")
}

/// `YAP_FAKE_SHARE_AUDIO=1`: share audio is a synthetic sine.
pub fn fake_audio() -> bool {
    env_flag("YAP_FAKE_SHARE_AUDIO")
}

fn env_flag(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| value == "1")
}

/// Captures a picked source for `yap-voice`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Capturer;

impl ShareCapturer for Capturer {
    type Capture = Capture;

    fn start(
        &self,
        request: &ShareRequest,
        ended: CaptureEnded,
    ) -> Result<(Capture, ShareCaptured), String> {
        platform::start(request, ended)
    }

    fn set_quality(&self, capture: &Capture, quality: ShareQuality) {
        platform::set_quality(capture, quality);
    }
}
