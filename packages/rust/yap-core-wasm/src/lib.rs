//! yap-core without its `runtime` feature, for the web.
//!
//! Building this crate for `wasm32-unknown-unknown` (CI's rust-shared job)
//! is the check that the pure half of yap-core (the store's reducer, state,
//! view and effects, and the rules modules) needs no I/O crate. Code that
//! talks to the network, the disk or the keyring sits behind yap-core's
//! `runtime` feature.
//!
//! Compiling is not running: `std::time::Instant::now`, `std::fs` and
//! `std::env` build for this target but fail or panic when called, so an
//! export that reaches them needs a browser stand-in first.

pub use yap_core;
