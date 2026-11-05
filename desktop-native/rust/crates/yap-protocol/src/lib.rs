//! The Yap wire protocol as Rust types: gateway opcodes and close codes,
//! the frame envelope, typed `d` payloads and the REST DTOs the native client
//! uses. Serde only; no I/O, no async runtime, no Qt.
//!
//! `packages/protocol` is the source of truth. Two things keep this crate
//! honest: the `opcode_parity` test reads `opcodes.ts`, and the fixtures in
//! `packages/protocol/fixtures` are decoded by `tests/fixtures.rs`.

pub mod collectibles;
pub mod frame;
pub mod opcode;
pub mod payload;
pub mod rest;
pub mod voice_app;

pub use frame::Frame;
pub use opcode::{
    GatewayCloseCode, GatewayOp, UnknownCode, DEFAULT_HEARTBEAT_INTERVAL_MS, HEARTBEAT_FRAME,
    RETIRED_BROWSER_SFU_OPS, VOICE_JOIN_UNVERIFIED_CLOSE_CODE, VOICE_MOVED_CLOSE_REASON,
};
