//! The core's runtime on mobile: the desktop gateway task (unchanged), the
//! device's time zone through Qt (as the desktop's `client::LOCAL`), the
//! session token in an app-private file, and the one command channel the
//! QML singletons send through.

#[path = "../../../../rust/crates/yap-ui/src/client/gateway.rs"]
mod gateway;

use std::ffi::{c_char, CStr};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use cxx_qt_lib::{QDateTime, QTimeZone};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_core::auth::TokenStore;
use yap_core::{
    Api, Command, GatewayEvents, GatewayHandle, LocalOffset, Realtime, Sink, Token, VoiceCommand,
    VoiceEvents, VoiceTarget, VoiceUpdate,
};

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

/// The desktop's `system_offset`: the UTC offset at an instant, so DST
/// applies per date.
fn system_offset(utc_ms: i64) -> i32 {
    thread_local! {
        static ZONE: cxx::UniquePtr<QTimeZone> = QTimeZone::system_time_zone();
    }
    ZONE.with(|zone| match zone.as_ref() {
        Some(zone) => QDateTime::from_msecs_since_epoch(utc_ms, zone).offset_from_utc(),
        None => 0,
    })
}

pub const LOCAL: LocalOffset = LocalOffset(system_offset);

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

static COMMANDS: OnceLock<UnboundedSender<Command>> = OnceLock::new();
static DATA: OnceLock<PathBuf> = OnceLock::new();

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
/// environment the desktop reads.
#[no_mangle]
pub extern "C" fn yap_mobile_init(data_dir: *const c_char, device_name: *const c_char) {
    crate::platform::stderr_to_logcat();
    let data = text(data_dir).map_or_else(|| PathBuf::from("."), PathBuf::from);
    yap_cache::profile::set_base(data.clone());
    let _ = DATA.set(data);
    let mut config = yap_core::config::Config::from_env();
    config.platform = std::env::consts::OS.to_owned();
    config.device_name = text(device_name);
    let _ = yap_core::config::set(config);
}

/// Starts the core once; later calls are ignored.
pub fn start(sink: Sink) -> bool {
    if COMMANDS.get().is_some() {
        return false;
    }
    let data = DATA.get().cloned().unwrap_or_else(|| PathBuf::from("."));
    let tx = yap_core::spawn(
        sink,
        crate::platform::http_client(),
        Box::new(MobileRealtime),
        Arc::new(FileVault(data.join("session.token"))),
        LOCAL,
    );
    let _ = tx.send(Command::Start);
    COMMANDS.set(tx).is_ok()
}

/// Sends a command to the core (dropped before `start`).
pub fn command(command: Command) {
    if let Some(tx) = COMMANDS.get() {
        let _ = tx.send(command);
    }
}

/// Commits the snapshot cache's pending write (the app going to the
/// background may be the last chance).
#[no_mangle]
pub extern "C" fn yap_mobile_flush() {
    if std::panic::catch_unwind(yap_core::shutdown).is_err() {
        eprintln!("[quit] flush panicked");
    }
}
