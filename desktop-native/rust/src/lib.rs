//! Yap, the native Windows client: a Rust core (auth, REST, gateway, state) behind QML
//! singletons. The view layer it shares with the mobile app is `yap_ui`
//! (domain objects, list models, the GUI-thread registry, the core's
//! runtime); this crate is the desktop's [`Platform`](yap_ui::platform::Platform)
//! ([`native`]) and its desktop-only objects. The voice settings are
//! [`audio`]; remote video between the call and the UI is [`video`], our
//! camera's frames into it [`camera`], the share picker's sources and
//! pictures [`share`]; the in-app updater is [`updater`].

mod apps_audio;
mod audio;
mod camera;
mod client;
#[cfg(windows)]
mod crash;
mod custom_sounds;
// The bridge's toast signal carries the whole toast (title, summary, body
// and the three ids it acts on), one argument over clippy's limit.
#[allow(clippy::too_many_arguments)]
mod desktop;
mod games;
mod hotkeys;
#[cfg(windows)]
mod installer;
mod loopback;
mod media;
mod native;
mod share;
mod sound;
mod updater;
mod video;
#[cfg(windows)]
mod webembed;
mod webm;

// The shared view layer's modules this crate reaches, under their old paths.
use yap_ui::{apps, bridge, gui, platform};
