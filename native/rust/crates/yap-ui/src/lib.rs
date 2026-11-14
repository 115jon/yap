//! The view layer the desktop and mobile apps share (QML module `Yap.Ui`):
//! the domain objects (`Session`, `Nav`, `Chat`, `Voice`, `Profile`) in
//! [`bridge`], the list models in [`models`], the GUI-thread registry they
//! share in [`gui`], the domain singletons the shared QML reaches (admin,
//! Listen Together, extras, invites, panels, the emoji picker, friends,
//! search, calls, settings, ...), and the core's runtime ([`client`]).
//! What differs per app comes through its [`platform::Platform`].

pub mod admin;
pub mod anim;
pub mod apps;
pub mod board;
pub mod bridge;
pub mod calls;
pub mod client;
pub mod color;
pub mod command_menu;
pub mod extras;
pub mod friends;
pub mod gui;
pub mod invites;
pub mod limits;
pub mod models;
pub mod panels;
pub mod picker;
pub mod platform;
pub mod search;
pub mod settings;
pub mod shop;
