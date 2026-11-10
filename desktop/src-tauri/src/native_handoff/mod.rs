//! The bridge from this, the last Tauri release (3.14), to the native app
//! (desktop-native, Velopack id `Yap.App`).
//!
//! It stays dormant while the latest GitHub release is a Tauri one. Once the
//! latest is the native app (`v4.x.y` with `Yap.App-win-Setup.exe`) and the
//! remote switch (`GET /api/desktop/native-handoff`, set in the admin
//! console) admits this install, the app's own updater offers "4.x.y":
//! `app_updates::fetch_update` asks [`check`] first and `install_update`
//! runs [`run`], so the launch updater, the update toast and Settings all
//! drive it with no change to the web UI.
//!
//! [`run`] downloads the Setup to `%TEMP%\yap-native-handoff`, checks its
//! SHA-256 against the release's digest and its Authenticode signer, runs
//! it silently while holding it open, checks the installed `Yap.exe` the
//! same way, starts the native app and waits until it stays up; the caller
//! then exits. The native app's first launch imports this app's settings
//! and uninstalls it (desktop-native/rust/src/installer.rs).
//!
//! State is `%APPDATA%\Yap\native-handoff.json` ([`policy::Marker`]); every
//! rule lives in [`policy`], pure and unit-tested. Every step is also
//! appended to `%TEMP%\yap-native-handoff\handoff.log`, which is all the
//! headless run (`--yap-native-handoff`) writes.
//!
//! Off switches: `YAP_NO_NATIVE_HANDOFF=1`, `optOut: true` in the marker,
//! the remote switch, and any build that is not an installed one (a dev run
//! never hands off, apart from the headless flag). `YAP_HANDOFF_RELEASE=<file>`
//! replaces discovery for tests ([`policy::parse_override`]); the digest and
//! signature checks stay on.

pub mod policy;

#[cfg(windows)]
mod windows_impl;

#[cfg(windows)]
pub use windows_impl::*;

/// What `fetch_update` shows for the native app.
pub const UPDATE_NOTES: &str = "Yap is moving to its new app. Update to install it: your settings come with you, and this version is removed afterwards.";

/// Opened once after the automatic attempts are used up.
pub const DOWNLOAD_PAGE: &str = "https://yap.115jon.com/#download";

/// The hidden command-line flag for a headless run.
pub const HEADLESS_ARG: &str = "--yap-native-handoff";

/// A failed attempt, for the updater commands.
#[derive(Debug, Clone)]
pub struct Failure {
    pub message: String,
    /// Open [`DOWNLOAD_PAGE`] now (the automatic attempts just ran out).
    pub open_download_page: bool,
}

#[cfg(not(windows))]
mod stub {
    //! Other platforms have no native app to hand off to.
    use super::policy::{NativeRelease, Trigger};

    pub async fn check(_trigger: Trigger) -> Option<NativeRelease> {
        None
    }

    pub fn set_offered(_release: Option<NativeRelease>) {}

    pub fn take_offered() -> Option<NativeRelease> {
        None
    }
}

#[cfg(not(windows))]
pub use stub::*;
