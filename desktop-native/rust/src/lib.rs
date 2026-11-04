//! Yap native client: a Rust core (auth, REST, gateway, state) behind QML
//! singletons. Domain objects (`Session`, `Nav`, `Chat`, `Voice`) live in
//! [`bridge`], list models in [`models`], and the GUI-thread state they
//! share in [`gui`].

mod bridge;
mod client;
mod gui;
mod models;
