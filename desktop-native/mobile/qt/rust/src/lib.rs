//! The Qt Quick mobile prototype's Rust side: the mobile app's
//! [`Platform`](yap_ui::platform::Platform) ([`client`]). The QML
//! singletons, the list models and the core's runtime are the desktop's,
//! from the shared `yap_ui` crate (QML module `Yap.Ui`); the phone shell is
//! this crate's QML module, `com.yap.mobile`.

mod client;
// Qt's network loads over the core's rustls (cpp/rustnetwork.cpp).
mod http;
#[path = "../../../common/platform.rs"]
mod platform;
// The FCM token and a notification's actions (cpp/push.cpp).
mod push;
// Voice calls and the call notification's actions (cpp/voice.cpp).
#[cfg(target_os = "android")]
mod voice;
