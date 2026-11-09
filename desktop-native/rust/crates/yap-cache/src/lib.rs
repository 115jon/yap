//! The local snapshot cache: what the last session showed, per user, so the
//! next launch can put it on screen before the network answers.
//!
//! One SQLite file ([`default_path`]) holds, per user id: `me`, the server
//! list, the channel tree of each server seen, the last active server and
//! channel, the newest [`MESSAGE_WINDOW`] messages of each text channel
//! opened, and the mic and deafen settings. Tokens and tickets are never
//! stored.
//!
//! It is a cache: a schema version mismatch or a file that does not open
//! drops it and starts over. [`Cache`] is blocking; [`Writer`] owns it on a
//! dedicated thread and coalesces writes (write-behind).

pub mod catalog;
mod db;
pub mod profile;
pub mod settings;
pub mod voice;
mod writer;

use std::path::PathBuf;

pub use db::{Batch, Cache, SCHEMA_VERSION};
pub use writer::Writer;
pub use yap_protocol::rest::{ChannelsResponse, Message, Server, User};

/// The newest messages kept per channel.
pub const MESSAGE_WINDOW: usize = 100;

/// Channel windows kept per user; the least recently written go first.
pub const CHANNEL_WINDOWS: usize = 32;

/// Why a cache call failed. Callers treat every error as "no cache".
#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("cache database: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("cache file: {0}")]
    Io(#[from] std::io::Error),
    #[error("cache encoding: {0}")]
    Json(#[from] serde_json::Error),
}

/// Mic and deafen as the user left them (the web's useVoiceSettingsStore):
/// deafening also mutes and remembers whether the mic was muted before, so
/// undeafening can put it back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VoicePrefs {
    pub muted: bool,
    pub deafened: bool,
    pub muted_before_deafen: bool,
}

/// One user's last session, as read at boot.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub me: User,
    /// In list order.
    pub servers: Vec<Server>,
    /// Channel trees by server id, for every server whose tree was stored.
    pub channels: Vec<(String, ChannelsResponse)>,
    pub active_server: Option<String>,
    pub active_channel: Option<String>,
    /// The active channel's newest window, oldest first.
    pub messages: Vec<Message>,
    /// When the snapshot was last written, epoch milliseconds.
    pub written_at_ms: i64,
    /// `None` until the user changed mic or deafen once.
    pub voice: Option<VoicePrefs>,
    /// Where each channel was left: its top message on screen, or
    /// `BOTTOM` (the web's persisted `scrollPositions`).
    pub scroll_positions: Vec<(String, String)>,
    /// The text channel each server opens on, and the last DM under `@me`
    /// (the web's `lastActiveChannels:<userId>`).
    pub last_channels: Vec<(String, String)>,
    /// The sidebar's collapsed categories (ids), across servers.
    pub collapsed_categories: Vec<String>,
}

/// One replacement for part of a user's snapshot. Each replaces what was
/// stored under the same key, so the writer keeps only the newest per key.
#[derive(Debug, Clone, PartialEq)]
pub enum Write {
    Me(Box<User>),
    Servers(Vec<Server>),
    Channels {
        server_id: String,
        tree: ChannelsResponse,
    },
    /// The last active server and channel.
    Route {
        server_id: Option<String>,
        channel_id: Option<String>,
    },
    /// A channel's newest window, oldest first (trimmed to [`MESSAGE_WINDOW`]).
    Messages {
        channel_id: String,
        messages: Vec<Message>,
    },
    /// Mic and deafen.
    Voice(VoicePrefs),
    /// Where a channel was left.
    Scroll {
        channel_id: String,
        message_id: String,
    },
    /// The channel a server (or `@me`) opens on next.
    LastChannel {
        server_id: String,
        channel_id: String,
    },
    /// The collapsed categories, the whole set.
    Collapsed(Vec<String>),
}

/// `%LOCALAPPDATA%\YapNative\cache\snapshot.db` (under the profile's folder
/// for a named profile, see [`profile`]), apart from the installed Yap app's
/// folders. `None` without `LOCALAPPDATA`.
pub fn default_path() -> Option<PathBuf> {
    Some(profile::data_root()?.join("cache").join("snapshot.db"))
}
