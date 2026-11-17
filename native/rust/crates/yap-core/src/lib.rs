//! The Yap client core, without Qt: configuration, REST, Kova sign-in, a
//! typed state store with a pure reducer ([`store`]), and the runtime that
//! executes its effects (`runtime`).
//!
//! The UI talks to it through [`Command`]s in and batches of
//! [`ViewUpdate`]s out: typed scalars and row diffs per list model
//! ([`store::view`]). The gateway and voice calls still live in the app
//! crate and plug in through `runtime::Realtime`.
//!
//! Everything that does I/O (REST, sign-in, the keyring, the runtime, the
//! SQLite cache) is behind the `runtime` feature, which every app build
//! turns on. Without it the crate is the pure half, which builds for
//! `wasm32-unknown-unknown` (packages/rust/yap-core-wasm, checked in CI).

pub mod account_deletion;
pub mod admin;
pub mod api;
#[cfg(feature = "runtime")]
pub mod auth;
pub mod badges;
#[cfg(feature = "runtime")]
pub mod chunked;
pub mod collectibles;
pub mod color;
pub mod command_menu;
pub mod config;
pub mod connections;
pub mod custom_sounds;
pub mod dm_permission;
pub mod emoji;
pub mod entitlements;
pub mod faults;
pub mod fixture;
pub mod gif_favorite;
#[cfg(feature = "desktop")]
pub mod install;
pub mod limits;
pub mod links;
pub mod markdown;
pub mod notification_levels;
pub mod picker;
pub mod profile_board;
pub mod rooms;
#[cfg(feature = "runtime")]
pub mod runtime;
pub mod search;
pub mod settings;
pub mod shell;
pub mod site_config;
pub mod store;
pub mod synth;
#[cfg(feature = "desktop")]
pub mod tauri;
#[cfg(feature = "desktop")]
pub mod update;
pub mod wordle;

#[cfg(feature = "runtime")]
pub use api::Api;
pub use api::{ApiError, Token};
#[cfg(feature = "runtime")]
pub use runtime::{
    shutdown, spawn, started_at, AudioEvents, AudioHandle, GatewayEvents, GatewayHandle, Realtime,
    Sink, VoiceEvents,
};
pub use store::{
    Command, LocalOffset, Occupant, Phase, UploadFile, ViewDiff, ViewUpdate, VoiceCommand,
    VoiceMember, VoiceMoved, VoiceShare, VoiceTarget, VoiceUpdate,
};
