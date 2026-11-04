//! The Yap client core, without Qt: configuration, REST, Kova sign-in, a
//! typed state store with a pure reducer ([`store`]), and the runtime that
//! executes its effects ([`runtime`]).
//!
//! The UI talks to it through [`Command`]s in and batches of
//! [`ViewUpdate`]s out: typed scalars and row diffs per list model
//! ([`store::view`]). The gateway and voice calls still live in the app
//! crate and plug in through [`runtime::Realtime`].

pub mod api;
pub mod auth;
pub mod config;
pub mod fixture;
pub mod markdown;
pub mod runtime;
pub mod store;

pub use api::{Api, ApiError, Token};
pub use runtime::{
    shutdown, spawn, started_at, GatewayEvents, GatewayHandle, Realtime, Sink, VoiceEvents,
};
pub use store::{
    Command, LocalOffset, Occupant, Phase, ViewDiff, ViewUpdate, VoiceCommand, VoiceMember,
    VoiceTarget, VoiceUpdate,
};
