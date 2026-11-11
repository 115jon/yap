//! Push notifications, the core's side (cpp/push.cpp calls these): the FCM
//! token for `/api/push-tokens`, and a notification's Reply, Mark as read
//! and tap while the app runs. With the app killed the Java side
//! (android/src/dev/yap/qtproto/push) calls the server itself.

use std::ffi::{c_char, CStr, CString};
use std::sync::{Mutex, OnceLock};

use tokio::sync::mpsc::UnboundedSender;
use yap_core::Command;
use yap_ui::client;

/// The build the server sees in `app_variant` (the Expo app sends its own).
const APP_VARIANT: &str = "qt";

/// The newest token, kept for the core's start: commands sent before it
/// are dropped.
static TOKEN: Mutex<Option<String>> = Mutex::new(None);

fn register(token: String) -> Command {
    Command::RegisterPush {
        token,
        platform: std::env::consts::OS.to_owned(),
        // Data-only pushes, drawn by the app (YapMessagingService).
        format: Some("native".to_owned()),
        app_variant: Some(APP_VARIANT.to_owned()),
    }
}

/// The core started ([`yap_ui::platform::Platform::started`]): a token that
/// came first is registered now.
pub fn started(commands: &UnboundedSender<Command>) {
    let token = TOKEN.lock().ok().and_then(|token| token.clone());
    if let Some(token) = token {
        let _ = commands.send(register(token));
    }
}

fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    // SAFETY: the C++ side passes NUL-terminated UTF-8 that outlives the call.
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

/// This device's FCM token (at start and on rotation).
#[no_mangle]
pub extern "C" fn yap_mobile_push_token(token: *const c_char) {
    let token = text(token).trim().to_owned();
    if token.is_empty() {
        return;
    }
    if let Ok(mut kept) = TOKEN.lock() {
        *kept = Some(token.clone());
    }
    client::command(register(token));
}

/// A notification's inline Reply, as the desktop's toast reply.
#[no_mangle]
pub extern "C" fn yap_mobile_notification_reply(
    channel_id: *const c_char,
    message_id: *const c_char,
    content: *const c_char,
) {
    let (nonce, sent_at) = yap_ui::bridge::stamp();
    client::command(Command::ToastReply {
        channel_id: text(channel_id),
        message_id: text(message_id),
        content: text(content),
        nonce,
        sent_at,
    });
}

/// A notification's Mark as read.
#[no_mangle]
pub extern "C" fn yap_mobile_notification_mark_read(channel_id: *const c_char) {
    client::command(Command::ToastMarkRead(text(channel_id)));
}

/// The API's origin, for the Java side's own calls with the app killed.
#[no_mangle]
pub extern "C" fn yap_mobile_api_base() -> *const c_char {
    static BASE: OnceLock<CString> = OnceLock::new();
    BASE.get_or_init(|| {
        CString::new(yap_core::config::get().api_base.clone()).unwrap_or_default()
    })
    .as_ptr()
}
