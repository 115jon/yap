//! The Yap wire protocol as Rust types: gateway opcodes and close codes,
//! the frame envelope, typed `d` payloads and the REST DTOs the native client
//! uses. Serde only; no I/O, no async runtime, no Qt.
//!
//! `packages/protocol` is the source of truth for the gateway's opcodes,
//! codes and the payload schemas the Durable Objects check. Three things
//! keep this crate honest: the `opcode_parity` test reads `opcodes.ts`, the
//! fixtures in `packages/protocol/fixtures` are decoded by
//! `tests/fixtures.rs`, and with the `ts` feature these types are written as
//! TypeScript (`packages/protocol/src/generated`, `pnpm protocol:generate`)
//! that the web typechecks its own against
//! (`src/lib/__tests__/wire-types.conformance.ts`). Some web types are the
//! generated ones outright (`Entitlement`); for those this crate is the
//! source.
//!
//! The TypeScript says what native reads: an `Option` or `#[serde(default)]`
//! field is optional there. Fields read with `bool_or_int`, `string_or_list`
//! or `count` name what those take (`BoolOrInt`, ...); fields read with
//! `lenient` (anything unreadable is `None`) and enums with a
//! `#[serde(other)]` fallback name only the values native understands. Types
//! native sends say what it sends: a field skipped when `None` is `x?: T`,
//! and one always written is required.

pub mod collectibles;
pub mod frame;
pub mod game_activity;
pub mod opcode;
pub mod payload;
pub mod profile_board;
pub mod rest;
pub mod tiers;
pub mod voice_app;

pub use frame::Frame;
pub use opcode::{
    GatewayCloseCode, GatewayOp, UnknownCode, DEFAULT_HEARTBEAT_INTERVAL_MS, HEARTBEAT_FRAME,
    RETIRED_BROWSER_SFU_OPS, VOICE_JOIN_UNVERIFIED_CLOSE_CODE, VOICE_MOVED_CLOSE_REASON,
};
