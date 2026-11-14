//! The Qt Quick mobile prototype's Rust side: the mobile app's
//! [`Platform`](yap_ui::platform::Platform) ([`client`]). The QML
//! singletons, the list models and the core's runtime are the desktop's,
//! from the shared `yap_ui` crate (QML module `Yap.Ui`); the phone shell is
//! this crate's QML module, `com.yap.mobile`.

mod client;
// A panic's message for the next launch's crash report (CrashReports.java).
mod crash;
// Qt's network loads over the core's rustls (cpp/rustnetwork.cpp).
mod http;
// Media caps for the players that send no headers (cpp/videoplayer.cpp).
mod mediacap;
mod platform;
// The FCM token and a notification's actions (cpp/push.cpp).
mod push;
// Voice calls and the call notification's actions (cpp/voice.cpp).
#[cfg(target_os = "android")]
mod voice;
// Soundboard clips and UI sounds on the app's speaker (AAudio).
#[cfg(target_os = "android")]
mod sound;
// Remote call video into the phone's video hub (cpp/videohub.cpp).
#[cfg(target_os = "android")]
mod video;
// The phone's camera into the call (cpp/camera.cpp).
#[cfg(target_os = "android")]
mod camera;
