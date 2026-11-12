//! The mobile app's side of the shared view layer: rustls HTTP, the shared
//! gateway task, voice calls on Android (`voice.rs`), the session token
//! sealed with an Android Keystore key, and the data folder and device name
//! handed to the core.

use std::ffi::{c_char, CStr};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::mpsc::{self, UnboundedReceiver};
use yap_core::auth::TokenStore;
use yap_core::{
    Api, GatewayEvents, GatewayHandle, Realtime, Token, UploadFile, ViewUpdate, VoiceCommand,
    VoiceEvents, VoiceTarget,
};
use yap_ui::client::gateway;
use yap_ui::platform::{self, Platform};

struct MobileRealtime;

impl Realtime for MobileRealtime {
    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle {
        // Signed in: if the last process died in a call (killed in the
        // background), the room hears it left now.
        #[cfg(target_os = "android")]
        crate::voice::signed_in(&api, &user_id);
        let (outgoing, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(api, user_id, rx, events));
        GatewayHandle { outgoing, task }
    }

    #[cfg(target_os = "android")]
    fn spawn_voice(
        &self,
        api: Api,
        http: reqwest::Client,
        target: VoiceTarget,
        commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        crate::voice::spawn(api, http, target, commands, events);
    }

    // The iOS build is a compile and link check without libwebrtc.
    #[cfg(not(target_os = "android"))]
    fn spawn_voice(
        &self,
        _api: Api,
        _http: reqwest::Client,
        _target: VoiceTarget,
        _commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        events.send(yap_core::VoiceUpdate::Ended(Some(
            "Voice is not in this build yet.".into(),
        )));
    }
}

/// The session token on Android: sealed with an AES-GCM key in the Android
/// Keystore (yap-mobile-vault, which also migrates an older build's plain
/// file). The Java side opens the same file with the same key for a
/// notification's Reply with the app killed (push/PushStore.java).
#[cfg(target_os = "android")]
mod keystore {
    use std::ffi::c_void;

    use yap_mobile_vault::{SealError, Sealer, Vault};

    use super::{Token, TokenStore};

    extern "C" {
        /// cpp/keystore.cpp: TokenVault.java's seal (0), open (1) or reset
        /// (2) of `len` bytes at `data`. The result goes to `sink(out, ...)`
        /// before the call returns. [`OK`], [`INVALID`] (the bytes can never
        /// open) or anything else (the Keystore could not be asked).
        fn yap_mobile_keystore(
            op: i32,
            data: *const u8,
            len: usize,
            out: *mut c_void,
            sink: extern "C" fn(*mut c_void, *const u8, usize),
        ) -> i32;
    }

    const SEAL: i32 = 0;
    const OPEN: i32 = 1;
    const RESET: i32 = 2;

    const OK: i32 = 0;
    const INVALID: i32 = 1;

    extern "C" fn sink(out: *mut c_void, data: *const u8, len: usize) {
        if out.is_null() || data.is_null() || len == 0 {
            return;
        }
        // SAFETY: `out` is the Vec `call` passes, alive and not otherwise
        // borrowed during the call; `data` is `len` bytes the C++ side
        // keeps alive until this returns.
        let (out, bytes) = unsafe {
            (
                &mut *out.cast::<Vec<u8>>(),
                std::slice::from_raw_parts(data, len),
            )
        };
        out.extend_from_slice(bytes);
    }

    fn call(op: i32, input: &[u8]) -> Result<Vec<u8>, SealError> {
        let mut out = Vec::new();
        // SAFETY: `input` outlives the call; `sink` gets `out` back only
        // while the call runs.
        let status = unsafe {
            yap_mobile_keystore(
                op,
                input.as_ptr(),
                input.len(),
                std::ptr::from_mut(&mut out).cast(),
                sink,
            )
        };
        match status {
            OK => Ok(out),
            INVALID => Err(SealError::Invalid("see yap-vault in the log".into())),
            _ => Err(SealError::Unavailable("see yap-vault in the log".into())),
        }
    }

    pub struct KeystoreSealer;

    impl Sealer for KeystoreSealer {
        fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, SealError> {
            call(SEAL, plain)
        }

        fn open(&self, sealed: &[u8]) -> Result<Vec<u8>, SealError> {
            call(OPEN, sealed)
        }

        fn reset(&self) {
            let _ = call(RESET, &[]);
        }
    }

    pub struct SessionVault(pub Vault<KeystoreSealer>);

    impl TokenStore for SessionVault {
        fn load(&self) -> Option<Token> {
            self.0.load().map(|token| Token::new(&token))
        }

        fn store(&self, token: &Token) {
            self.0.store(token.as_str());
        }

        fn clear(&self) {
            self.0.clear();
        }
    }
}

/// The iOS build is a compile and link check: the token in a plain file in
/// the app's container until it has the Keychain.
#[cfg(not(target_os = "android"))]
struct FileVault(PathBuf);

#[cfg(not(target_os = "android"))]
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
        #[cfg(target_os = "android")]
        let store = keystore::SessionVault(yap_mobile_vault::Vault::in_dir(
            keystore::KeystoreSealer,
            &self.data,
        ));
        #[cfg(not(target_os = "android"))]
        let store = FileVault(self.data.join(yap_mobile_vault::LEGACY_FILE));
        Arc::new(store)
    }

    fn started(&self, commands: &tokio::sync::mpsc::UnboundedSender<yap_core::Command>) {
        crate::push::started(commands);
    }

    /// Android's own word on the network (cpp/native.cpp): in the
    /// background the app's network is blocked while the device's is up.
    #[cfg(target_os = "android")]
    fn network_up(&self) -> Option<bool> {
        extern "C" {
            /// NetworkState.get: 1 up (blocked for this app or not),
            /// 0 none, -1 unknown.
            fn yap_mobile_network_state() -> i32;
        }
        // SAFETY: no arguments; the C++ side attaches the calling thread.
        match unsafe { yap_mobile_network_state() } {
            1 => Some(true),
            0 => Some(false),
            _ => None,
        }
    }

    /// The photo picker's copies in the app cache (`Native.pickMedia`):
    /// named after the picked item, with the extension its type implies.
    fn upload_files(&self, paths: &[String]) -> Vec<UploadFile> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        paths
            .iter()
            .enumerate()
            .filter_map(|(i, path)| {
                let meta = std::fs::metadata(path).ok().filter(|m| m.is_file())?;
                let name = Path::new(path).file_name()?.to_string_lossy().into_owned();
                Some(UploadFile {
                    local_id: format!("upload-{stamp:x}-{i}"),
                    path: path.clone(),
                    content_type: content_type(&name).to_owned(),
                    name,
                    size: meta.len(),
                })
            })
            .collect()
    }

    fn on_update(&self, update: ViewUpdate) -> bool {
        // The session for private media (attachments, custom emoji).
        if let ViewUpdate::MediaAuth(token) = update {
            crate::http::set_token(token);
            return true;
        }
        // Known and not shown here yet; anything else is logged as unhandled.
        matches!(
            update,
            ViewUpdate::CameraBackgrounds(_)
                | ViewUpdate::Toast(_)
                | ViewUpdate::Sound(_)
                | ViewUpdate::StopSound(_)
                | ViewUpdate::Badge(_)
        )
    }
}

/// The MIME type for a picked file's name: the picker offers photos and
/// videos (the server stores `application/octet-stream` for anything else).
fn content_type(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "3gp" => "video/3gpp",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        _ => "application/octet-stream",
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
    seed_theme();
    let mut config = yap_core::config::Config::from_env();
    config.platform = std::env::consts::OS.to_owned();
    config.device_name = text(device_name);
    // No hidden standby session on a voice landing: a phone joins when
    // asked (and has no landing).
    config.voice_standby = false;
    let _ = yap_core::config::set(config);
    platform::install(Mobile { data });
}

/// A phone follows the system's light or dark mode until a theme is picked
/// in Settings: before the first, this device's appearance file says
/// `system` (the core's default is the desktop's `dark`).
fn seed_theme() {
    use yap_cache::settings::{self as prefs, AppearancePrefs, APPEARANCE_FILE};
    let Some(dir) = prefs::app_dir() else {
        return;
    };
    if dir.join(APPEARANCE_FILE).exists() {
        return;
    }
    let seed = AppearancePrefs {
        theme: "system".into(),
    };
    if let Err(error) = prefs::save_appearance(&dir, &seed) {
        eprintln!("[settings] theme not seeded: {error}");
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
