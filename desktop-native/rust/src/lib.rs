//! Yap, the native Windows client: a Rust core (auth, REST, gateway, state) behind QML
//! singletons. Domain objects (`Session`, `Nav`, `Chat`, `Voice`) live in
//! [`bridge`], list models in [`models`], and the GUI-thread state they
//! share in [`gui`]. The voice settings are [`audio`]; remote video between
//! the call and the UI is [`video`], our camera's frames into it
//! [`camera`], the share picker's sources and pictures [`share`], and the
//! settings dialog's side of the core [`settings`]; the in-app updater is
//! [`updater`].

mod admin;
mod anim;
mod apps;
mod apps_audio;
mod audio;
mod board;
mod bridge;
mod calls;
mod camera;
mod client;
mod color;
mod command_menu;
#[cfg(windows)]
mod crash;
mod custom_sounds;
// The bridge's toast signal carries the whole toast (title, summary, body
// and the three ids it acts on), one argument over clippy's limit.
#[allow(clippy::too_many_arguments)]
mod desktop;
mod extras;
mod friends;
mod games;
mod gui;
mod hotkeys;
#[cfg(windows)]
mod installer;
mod invites;
mod limits;
mod loopback;
mod media;
mod models;
mod panels;
mod picker;
mod search;
mod settings;
mod share;
mod shop;
mod sound;
mod updater;
mod video;
#[cfg(windows)]
mod webembed;
mod webm;
