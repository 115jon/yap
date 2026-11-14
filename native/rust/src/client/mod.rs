//! The core's runtime and command channel are the shared view layer's
//! (`yap_ui::client`); voice calls, which drive this crate's devices and
//! video, are the desktop's.

pub mod voice;

pub use yap_ui::client::*;
