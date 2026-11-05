//! The Yap client core, without Qt: configuration, REST, Kova sign-in, a
//! typed state store with a pure reducer ([`store`]), and the runtime that
//! executes its effects ([`runtime`]).
//!
//! The UI talks to it through [`Command`]s in and batches of
//! [`ViewUpdate`]s out: typed scalars and row diffs per list model
//! ([`store::view`]). The gateway and voice calls still live in the app
//! crate and plug in through [`runtime::Realtime`].

pub mod admin;
pub mod api;
pub mod auth;
pub mod collectibles;
pub mod config;
pub mod emoji;
pub mod fixture;
pub mod links;
pub mod markdown;
pub mod picker;
pub mod runtime;
pub mod search;
pub mod settings;
pub mod shell;
pub mod store;
pub mod synth;
pub mod wordle;

pub use api::{Api, ApiError, Token};
pub use runtime::{
    shutdown, spawn, started_at, AudioEvents, AudioHandle, GatewayEvents, GatewayHandle, Realtime,
    Sink, VoiceEvents,
};
pub use store::{
    Command, LocalOffset, Occupant, Phase, UploadFile, ViewDiff, ViewUpdate, VoiceCommand,
    VoiceMember, VoiceShare, VoiceTarget, VoiceUpdate,
};
