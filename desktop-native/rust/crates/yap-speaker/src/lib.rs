//! Yap's one speaker. Everything the app plays goes through one shared-mode
//! WASAPI render stream:
//!
//! - the call's mix of remote tracks (`yap-voice`),
//! - the soundboard, its tones and the Listen Together stream (`yap-audio`),
//! - the UI sounds (notifications, join and leave, the ring).
//!
//! Each is an [`Input`] on the [`Bus`]. The render thread mixes 10 ms at a
//! time, bends the sum under full scale ([`bus::soft_clip`]), and hands the
//! frame to the [`Tap`] (the call's echo canceller, as its far end) before
//! writing it to the device. The echo canceller therefore hears exactly
//! what plays, soundboard included.
//!
//! # Device
//!
//! [`Speaker::set_device`] takes an endpoint id (`IMMDevice::GetId`), or
//! `None` for the default communications endpoint, which is what the call
//! always opened and what the device menus list as "Default". With
//! `None`, the speaker follows the Windows default
//! (`IMMNotificationClient::OnDefaultDeviceChanged`). A chosen device that
//! is unplugged falls back to the default and comes back when it is
//! plugged in again. A device that is lost mid-stream
//! (`AUDCLNT_E_DEVICE_INVALIDATED`, or an event that stops firing for a
//! second) is reopened at once. The mix keeps its clock in the meantime,
//! so the call's far end never stalls.
//!
//! # Lifetime
//!
//! The render thread starts when an input has something to play
//! ([`Speaker::wake`]) or something holds the device open ([`Speaker::hold`],
//! the call). It closes the device and ends after 2 s with neither.
//!
//! # Test hooks
//!
//! - `YAP_NULL_SPEAKER=1`: no device for the whole process; the mix is paced
//!   by the clock and the tap still runs.
//! - `YAP_RENDER_TAP=<file.wav>`: records the final mix as it is played, up
//!   to 120 s per render thread (`-2.wav`, `-3.wav`, ... after the first).
//! - `YAP_SPEAKER_AHEAD_MS=<10..=60>`: how far ahead of the device the mix
//!   runs (default 20).

pub mod bus;
pub mod endpoint;
mod engine;
mod voices;
#[cfg(windows)]
mod wasapi;
mod wav;

pub use bus::{Bus, Input};
pub use endpoint::{match_endpoint, Endpoint};
pub use engine::{speaker, Hold, Speaker, Tap, NOTICE_AFTER};
pub use voices::Voices;
#[cfg(windows)]
pub use wasapi::{default_role, opt_out_of_ducking, probe_default, Probe};

/// "Lower other apps during calls" (off by default). Windows turns other
/// apps down, by the amount in Sound settings > Communications (80 % out of
/// the box), while a stream is open on a device opened in the
/// communications role. Off, "Default" opens the console default as
/// Chromium's "Default" does (the Tauri app never ducked); on, the
/// communications default, so Windows ducks. A change applies from the next
/// stream opened (the next call).
static DUCK_OTHERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn set_duck_others(on: bool) {
    DUCK_OTHERS.store(on, std::sync::atomic::Ordering::Relaxed);
}

pub fn duck_others() -> bool {
    DUCK_OTHERS.load(std::sync::atomic::Ordering::Relaxed)
}

/// The mix rate, frames per second.
pub const RATE: u32 = 48_000;

/// One 10 ms frame, in samples per channel (WebRTC's frame).
pub const FRAME: usize = 480;

/// Interleaved stereo.
pub const CHANNELS: usize = 2;

/// Samples in one stereo frame.
pub const SAMPLES: usize = FRAME * CHANNELS;
