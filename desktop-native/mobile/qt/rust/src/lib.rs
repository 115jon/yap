//! The Qt Quick mobile prototype's Rust side: the mobile app's
//! [`Platform`](yap_ui::platform::Platform) ([`client`]). The QML
//! singletons, the list models and the core's runtime are the desktop's,
//! from the shared `yap_ui` crate (QML module `Yap.Ui`); the phone shell is
//! this crate's QML module, `com.yap.mobile`.

mod client;
#[path = "../../../common/platform.rs"]
mod platform;
