//! The mobile app's side of the shared view layer: rustls HTTP, the shared
//! gateway task (no voice yet), the session token in an app-private file,
//! and the data folder and device name handed to the core.

use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::{self, UnboundedReceiver};
use yap_core::auth::TokenStore;
use yap_core::{
    Api, GatewayEvents, GatewayHandle, Realtime, Token, ViewUpdate, VoiceCommand, VoiceEvents,
    VoiceTarget, VoiceUpdate,
};
use yap_ui::client::gateway;
use yap_ui::platform::{self, Platform};

struct MobileRealtime;

impl Realtime for MobileRealtime {
    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle {
        let (outgoing, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(api, user_id, rx, events));
        GatewayHandle { outgoing, task }
    }

    fn spawn_voice(
        &self,
        _api: Api,
        _http: reqwest::Client,
        _target: VoiceTarget,
        _commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        events.send(VoiceUpdate::Ended(Some(
            "Voice is not in the mobile prototype yet.".into(),
        )));
    }
}

/// The session token in the app's private data folder (other apps cannot
/// read it; Auto Backup is off in the manifest). Prototype only: a release
/// build wraps it with an Android Keystore key through a small Java shim,
/// as the Compose baseline's notes say.
struct FileVault(PathBuf);

impl TokenStore for FileVault {
    fn load(&self) -> Option<Token> {
        let text = std::fs::read_to_string(&self.0).ok()?;
        let text = text.trim();
        (!text.is_empty()).then(|| Token::new(text))
    }

    fn store(&self, token: &Token) {
        if let Err(e) = std::fs::write(&self.0, token.as_str()) {
            eprintln!("[session] could not keep the token: {e}");
        }
    }

    fn clear(&self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// The phone's platform. The desktop-only view updates (toasts, sounds,
/// the tray badge, ...) are dropped: the phone has no such surfaces yet.
struct Mobile {
    data: PathBuf,
}

impl Platform for Mobile {
    fn http_client(&self) -> reqwest::Client {
        crate::platform::http_client()
    }

    fn realtime(&self) -> Box<dyn Realtime> {
        Box::new(MobileRealtime)
    }

    fn token_store(&self) -> Arc<dyn TokenStore> {
        Arc::new(FileVault(self.data.join("session.token")))
    }

    fn on_update(&self, update: ViewUpdate) -> bool {
        // Known and not shown here yet; anything else is logged as unhandled.
        matches!(
            update,
            ViewUpdate::CameraBackgrounds(_)
                | ViewUpdate::Toast(_)
                | ViewUpdate::Sound(_)
                | ViewUpdate::StopSound(_)
                | ViewUpdate::Badge(_)
                | ViewUpdate::MediaAuth(_)
        )
    }
}

fn text(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    // SAFETY: main.cpp passes NUL-terminated UTF-8 that outlives the call.
    let text = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    (!text.is_empty()).then_some(text)
}

/// Before QML loads (main.cpp): the app's private data folder (the snapshot
/// cache, settings, the session file) and the device's name for Kova's
/// approval page. Both go to the core as values, not through the
/// environment the desktop reads. Installs the mobile [`Platform`].
#[no_mangle]
pub extern "C" fn yap_mobile_init(data_dir: *const c_char, device_name: *const c_char) {
    crate::platform::stderr_to_logcat();
    let data = text(data_dir).map_or_else(|| PathBuf::from("."), PathBuf::from);
    yap_cache::profile::set_base(data.clone());
    let mut config = yap_core::config::Config::from_env();
    config.platform = std::env::consts::OS.to_owned();
    config.device_name = text(device_name);
    let _ = yap_core::config::set(config);
    platform::install(Mobile { data });
}

/// Commits the snapshot cache's pending write (the app going to the
/// background may be the last chance).
#[no_mangle]
pub extern "C" fn yap_mobile_flush() {
    if std::panic::catch_unwind(yap_core::shutdown).is_err() {
        eprintln!("[quit] flush panicked");
    }
}
