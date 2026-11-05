//! Yap native client: a Rust core (auth, REST, gateway, state) behind QML
//! singletons. Domain objects (`Session`, `Nav`, `Chat`, `Voice`) live in
//! [`bridge`], list models in [`models`], and the GUI-thread state they
//! share in [`gui`]. The voice settings are [`audio`]; remote video between
//! the call and the UI is [`video`], our camera's frames into it
//! [`camera`], the share picker's sources and pictures [`share`], and the
//! settings dialog's side of the core [`settings`].

mod admin;
mod anim;
mod apps;
mod apps_audio;
mod audio;
mod bridge;
mod camera;
mod client;
mod desktop;
mod friends;
mod gui;
mod invites;
mod media;
mod models;
mod panels;
mod picker;
mod search;
mod settings;
mod share;
mod shop;
mod sound;
mod video;
#[cfg(windows)]
mod webembed;
