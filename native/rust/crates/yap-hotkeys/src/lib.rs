//! Yap's global keybinds and push to talk, without Qt.
//!
//! - [`key`], [`binding`]: keys and buttons, and bindings as text
//!   (`Ctrl+Shift+M`, `RCtrl`, `Mouse4`);
//! - [`settings`]: actions, the saved settings and their defaults, and
//!   conflicts;
//! - [`engine`]: chord matching, push to talk's state machine with its
//!   release delay, and recording a binding (pure, time in ms);
//! - [`script`]: the test command channel's lines;
//! - `hook` (Windows): the low-level keyboard and mouse hook thread;
//! - `elevation` (Windows): whether the foreground window is elevated.

pub mod binding;
pub mod engine;
pub mod key;
pub mod script;
pub mod settings;

#[cfg(windows)]
pub mod elevation;
#[cfg(windows)]
pub mod hook;

pub use binding::Binding;
pub use engine::{Engine, Output, Ptt, Recorded};
pub use key::{Key, Modifier, Mods, MouseButton};
pub use settings::{Action, Conflict, Entry, InputMode, KeybindSettings, SoundLabel};
