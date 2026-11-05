//! Yap native client: a Rust core (auth, REST, gateway, state) behind QML
//! singletons. Domain objects (`Session`, `Nav`, `Chat`, `Voice`) live in
//! [`bridge`], list models in [`models`], and the GUI-thread state they
//! share in [`gui`]. The voice settings are [`audio`]; remote video between
//! the call and the UI is [`video`].

mod audio;
mod bridge;
mod client;
mod desktop;
mod gui;
mod media;
mod models;
mod picker;
mod sound;
mod video;
