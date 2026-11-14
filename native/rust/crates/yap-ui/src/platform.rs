//! What the view layer asks of the app it runs in. The domain objects, the
//! registry and the core's runtime are the same on every platform; the
//! TLS stack, the token store, realtime (gateway, voice, the local player),
//! files, screen share and video, and the view updates only a desktop shows
//! (toasts, sounds, the tray badge) are the app's. The app installs its
//! [`Platform`] once, before QML loads (the desktop's `yap_desktop_init`,
//! the mobile app's `yap_mobile_init`).

use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc::UnboundedSender;
use yap_core::auth::TokenStore;
use yap_core::{Command, Realtime, UploadFile, ViewUpdate};

pub trait Platform: Send + Sync + 'static {
    /// The core's HTTP client, on the app's TLS backend.
    fn http_client(&self) -> reqwest::Client;

    /// Gateway, voice calls and the local player.
    fn realtime(&self) -> Box<dyn Realtime>;

    /// Where the session token is kept.
    fn token_store(&self) -> Arc<dyn TokenStore>;

    /// The core started; `commands` is its command channel (what reports
    /// into the store from outside the UI starts here).
    fn started(&self, commands: &UnboundedSender<Command>) {
        let _ = commands;
    }

    /// The core is about to stop ([`crate::client::shutdown`]): a call in
    /// progress is left now, hooks are released.
    fn shutting_down(&self) {}

    /// A view update the shared registry does not handle, in order, on the
    /// GUI thread. `false`: the app has no handler for it either (the
    /// registry logs the variant once, so a new core update is not lost
    /// silently).
    fn on_update(&self, update: ViewUpdate) -> bool {
        let _ = update;
        false
    }

    /// We joined (`true`) or left a call (GUI thread).
    fn in_call(&self, in_call: bool) {
        let _ = in_call;
    }

    /// What an upload needs to know about each local file; unreadable
    /// paths are skipped.
    fn upload_files(&self, paths: &[String]) -> Vec<UploadFile> {
        let _ = paths;
        Vec::new()
    }

    /// The screen share picker's sources, as JSON (on a worker thread).
    fn share_sources_json(&self) -> String {
        "[]".into()
    }

    /// A video surface for a tile: its id, which `report_video` names.
    fn new_video_surface(&self) -> i32 {
        0
    }

    /// What a surface shows (`key`, a participant's camera or share) and
    /// its size on screen.
    fn report_video(&self, surface: i32, key: &str, width: u32, height: u32) {
        let _ = (surface, key, width, height);
    }

    /// The participant on the call stage (empty: none).
    fn set_video_stage(&self, participant_id: &str) {
        let _ = participant_id;
    }

    /// Screen share lists only our own windows (`YAP_SHARE_ONLY_OURS=1`,
    /// tests).
    fn share_only_ours(&self) -> bool {
        false
    }

    /// The link schemes' registration lines for `exe`, or empty when this
    /// copy registers none (`Invites.schemeRegistration`).
    fn scheme_registration(&self, exe: &str) -> String {
        let _ = exe;
        String::new()
    }

    /// The `Apps` object exists: the app sends its local sound settings.
    fn apps_created(&self) {}

    /// The local player's output level, 0 to 1 (Listen Together's meter).
    fn player_level(&self) -> f64 {
        0.0
    }

    /// Whether the system has a network, when the app can ask (`None`:
    /// it cannot). A phone app in the background can have its own network
    /// blocked while the device's is up: a host name that does not resolve
    /// there is not "no network" (the gateway's strip says Reconnecting).
    fn network_up(&self) -> Option<bool> {
        None
    }
}

static PLATFORM: OnceLock<Box<dyn Platform>> = OnceLock::new();

/// Installs the app's platform; `false` if one was installed already.
pub fn install(platform: impl Platform) -> bool {
    PLATFORM.set(Box::new(platform)).is_ok()
}

/// The installed platform.
///
/// # Panics
/// Before [`install`]: the app installs it before QML loads.
pub fn platform() -> &'static dyn Platform {
    installed().expect("the app installs its Platform before QML loads")
}

/// The installed platform, if there is one yet.
pub fn installed() -> Option<&'static dyn Platform> {
    PLATFORM.get().map(AsRef::as_ref)
}
