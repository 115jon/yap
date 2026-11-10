//! The Qt Quick mobile prototype's Rust side. Same shape as the desktop app
//! (`desktop-native/rust/src`): the QML singletons `Session`, `Nav` and
//! `Chat` ([`bridge`]) forward intents to yap-core as commands; the core's
//! view updates land on the GUI thread ([`gui`]) and go into the desktop's
//! own list models, compiled here unchanged ([`models`], mirrored from
//! `desktop-native/rust/src/models.rs` by build.rs).

// The desktop's picture decoder behind AnimatedPicture (both unchanged:
// this file and cpp/animatedpicture.cpp).
#[path = "../../../../rust/src/anim.rs"]
#[allow(dead_code)]
mod anim;
mod bridge;
mod client;
mod gui;
// Mirrored from desktop-native/rust/src/models.rs by build.rs (gitignored).
mod models;
#[path = "../../../common/platform.rs"]
mod platform;

// The view layer shared with the desktop (QML module Yap.Ui).
use yap_ui as _;
