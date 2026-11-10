//! yap-core behind a UniFFI boundary, for the Android and iOS prototype
//! (desktop-native/mobile/README.md).
//!
//! The store, reducer, effect runner, REST, Kova device sign-in, snapshot
//! cache and gateway are the desktop crates, unchanged. This crate only:
//!
//! - maps a few [`Command`]s to methods on [`Core`];
//! - maps the [`ViewUpdate`]s the prototype draws to UniFFI records, one
//!   batch per [`Listener::on_updates`] call, in order (the same row diffs
//!   the QML list models apply);
//! - stores the session through the app's [`TokenVault`] instead of the OS
//!   keyring, and asks the app's [`LocalTime`] for UTC offsets (the desktop
//!   asks `QTimeZone`);
//! - runs the desktop gateway task (`src/client/gateway.rs`, included by
//!   path) and has no voice yet.

use std::path::PathBuf;
use std::sync::{Arc, Once, OnceLock};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use yap_core::auth::TokenStore;
use yap_core::store::view::{ChannelTreeRow, MessageRow, ServerRow, TreeKind};
use yap_core::{
    Api, Command, GatewayEvents, GatewayHandle, LocalOffset, Phase as CorePhase, Realtime, Token,
    ViewDiff, ViewUpdate, VoiceCommand, VoiceEvents, VoiceTarget, VoiceUpdate,
};

// The desktop gateway task, unchanged: it depends on yap-core and
// yap-realtime only.
#[path = "../../../rust/src/client/gateway.rs"]
mod gateway;

uniffi::setup_scaffolding!();

// ---- What the views receive ------------------------------------------------

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Boot,
    SignedOut,
    DeviceCode,
    Loading,
    Ready,
}

impl From<CorePhase> for Phase {
    fn from(phase: CorePhase) -> Self {
        match phase {
            CorePhase::Boot => Self::Boot,
            CorePhase::SignedOut => Self::SignedOut,
            CorePhase::DeviceCode => Self::DeviceCode,
            CorePhase::Loading => Self::Loading,
            CorePhase::Ready => Self::Ready,
        }
    }
}

/// One key on the server rail.
#[derive(uniffi::Record, Clone, Debug)]
pub struct Server {
    pub id: String,
    pub name: String,
    pub initials: String,
    /// Absolute URL, or empty.
    pub icon: String,
    pub unread: bool,
    pub mentions: i64,
}

impl From<ServerRow> for Server {
    fn from(row: ServerRow) -> Self {
        Self {
            id: row.id,
            name: row.name,
            initials: row.initials,
            icon: row.icon,
            unread: row.unread,
            mentions: row.mentions,
        }
    }
}

#[derive(uniffi::Enum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TreeItemKind {
    Category,
    Channel,
    /// Someone in a voice channel, under it.
    Member,
}

/// One row of the flattened channel tree (categories, channels, the people
/// in voice channels), in sidebar order.
#[derive(uniffi::Record, Clone, Debug)]
pub struct TreeItem {
    pub kind: TreeItemKind,
    pub id: String,
    pub name: String,
    pub voice: bool,
    pub unread: bool,
    pub mentions: i64,
    /// Under (or is) a collapsed category.
    pub folded: bool,
    /// Members: the avatar URL, or empty.
    pub avatar: String,
    pub speaking: bool,
}

impl From<ChannelTreeRow> for TreeItem {
    fn from(row: ChannelTreeRow) -> Self {
        Self {
            kind: match row.kind {
                TreeKind::Label => TreeItemKind::Category,
                TreeKind::Channel => TreeItemKind::Channel,
                TreeKind::Member => TreeItemKind::Member,
            },
            id: row.id,
            name: row.name,
            voice: row.voice,
            unread: row.unread,
            mentions: row.mentions,
            folded: row.folded,
            avatar: row.avatar,
            speaking: row.speaking,
        }
    }
}

/// One message, ready to draw. `body` is the Markdown source: the core's
/// `html` is Qt rich text, which the native views do not use.
#[derive(uniffi::Record, Clone, Debug)]
pub struct Message {
    pub id: String,
    pub author: String,
    /// `#rrggbb`, or empty.
    pub author_color: String,
    pub initials: String,
    pub avatar: String,
    pub body: String,
    /// `h:mm AM`, local.
    pub time: String,
    /// `MMM d`, local.
    pub date: String,
    pub edited: bool,
    /// Same author as the row above, within the group window.
    pub grouped: bool,
    pub mine: bool,
    pub pending: bool,
    pub failed: bool,
    pub mentioned: bool,
    /// The "New" divider goes above this row.
    pub divider: bool,
    pub reply_author: String,
    pub reply_text: String,
    /// The attachments as JSON (`[]` when none).
    pub attachments: String,
}

impl From<MessageRow> for Message {
    fn from(row: MessageRow) -> Self {
        Self {
            id: row.id,
            author: row.author,
            author_color: row.author_color,
            initials: row.initials,
            avatar: row.avatar,
            body: row.body,
            time: row.time,
            date: row.date,
            edited: row.edited,
            grouped: row.grouped,
            mine: row.mine,
            pending: row.pending,
            failed: row.failed,
            mentioned: row.mentioned,
            divider: row.divider,
            reply_author: row.reply_author,
            reply_text: row.reply_text,
            attachments: row.attachments,
        }
    }
}

/// The core's row diffs, per list. UniFFI has no generic enums, so one
/// enum per row type; the views apply them as QML's list models do.
macro_rules! diff_enum {
    ($name:ident, $row:ty, $core:ty) => {
        #[derive(uniffi::Enum, Clone, Debug)]
        pub enum $name {
            Reset { rows: Vec<$row> },
            Insert { at: u32, rows: Vec<$row> },
            Update { at: u32, row: $row },
            Remove { at: u32, count: u32 },
        }

        impl From<ViewDiff<$core>> for $name {
            fn from(diff: ViewDiff<$core>) -> Self {
                let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
                match diff {
                    ViewDiff::Reset(rows) => Self::Reset {
                        rows: rows.into_iter().map(Into::into).collect(),
                    },
                    ViewDiff::Insert { at, rows } => Self::Insert {
                        at: n(at),
                        rows: rows.into_iter().map(Into::into).collect(),
                    },
                    ViewDiff::Update { at, row, .. } => Self::Update {
                        at: n(at),
                        row: row.into(),
                    },
                    ViewDiff::Remove { at, count } => Self::Remove {
                        at: n(at),
                        count: n(count),
                    },
                }
            }
        }
    };
}

diff_enum!(ServerDiff, Server, ServerRow);
diff_enum!(TreeDiff, TreeItem, ChannelTreeRow);
diff_enum!(MessageDiff, Message, MessageRow);

/// The view updates the prototype draws; the core sends many more, which
/// are dropped here.
#[derive(uniffi::Enum, Clone, Debug)]
pub enum Update {
    /// Signed out, showing a device code, loading, or ready.
    PhaseChanged {
        phase: Phase,
    },
    /// Device sign-in: show `user_code`, open `url` in the browser.
    DeviceCode {
        user_code: String,
        url: String,
    },
    /// The status line (sign-in errors, "Connecting…").
    Status {
        text: String,
    },
    Connected {
        connected: bool,
    },
    Me {
        id: String,
        name: String,
        username: String,
        avatar: String,
    },
    ActiveServer {
        id: String,
        name: String,
    },
    ActiveChannel {
        id: String,
        name: String,
    },
    Servers {
        diff: ServerDiff,
    },
    Channels {
        diff: TreeDiff,
    },
    Messages {
        diff: MessageDiff,
    },
    /// The open channel's first page is loading.
    PageLoading {
        loading: bool,
    },
    /// Older pages: one is loading; there are more.
    History {
        loading: bool,
        more: bool,
    },
    Typing {
        text: String,
    },
}

fn map_update(update: ViewUpdate) -> Option<Update> {
    Some(match update {
        ViewUpdate::Phase(phase) => Update::PhaseChanged {
            phase: phase.into(),
        },
        ViewUpdate::DeviceCode { user_code, url } => Update::DeviceCode { user_code, url },
        ViewUpdate::Status(text) => Update::Status { text },
        ViewUpdate::Connected(connected) => Update::Connected { connected },
        ViewUpdate::Me(me) => Update::Me {
            id: me.id,
            name: me.name,
            username: me.username,
            avatar: me.avatar,
        },
        ViewUpdate::ActiveServer { id, name } => Update::ActiveServer { id, name },
        ViewUpdate::ActiveChannel { id, name } => Update::ActiveChannel { id, name },
        ViewUpdate::Servers(diff) => Update::Servers { diff: diff.into() },
        ViewUpdate::Channels(diff) => Update::Channels { diff: diff.into() },
        ViewUpdate::Messages(diff) => Update::Messages {
            diff: (*diff).into(),
        },
        ViewUpdate::PageLoading(loading) => Update::PageLoading { loading },
        ViewUpdate::History { loading, more } => Update::History { loading, more },
        ViewUpdate::Typing(text) => Update::Typing { text },
        _ => return None,
    })
}

// ---- What the app provides -------------------------------------------------

/// Receives the core's view updates, one batch at a time, in order, on a
/// core thread (the app hops to its UI thread).
#[uniffi::export(with_foreign)]
pub trait Listener: Send + Sync {
    fn on_updates(&self, updates: Vec<Update>);
}

/// Where the session token lives between launches (the desktop keeps it in
/// the OS credential store). Called on a blocking thread, in order.
#[uniffi::export(with_foreign)]
pub trait TokenVault: Send + Sync {
    fn load(&self) -> Option<String>;
    fn store(&self, token: String);
    fn clear(&self);
}

struct Vault(Arc<dyn TokenVault>);

impl TokenStore for Vault {
    fn load(&self) -> Option<Token> {
        self.0
            .load()
            .filter(|t| !t.is_empty())
            .map(|t| Token::new(&t))
    }
    fn store(&self, token: &Token) {
        self.0.store(token.as_str().to_owned());
    }
    fn clear(&self) {
        self.0.clear();
    }
}

// ---- Realtime: the desktop gateway, no voice yet ---------------------------

struct MobileRealtime;

impl Realtime for MobileRealtime {
    fn spawn_gateway(&self, api: Api, user_id: String, events: GatewayEvents) -> GatewayHandle {
        let (outgoing, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(gateway::run(api, user_id, rx, events));
        GatewayHandle { outgoing, task }
    }

    fn spawn_voice(
        &self,
        _api: Api,
        _http: reqwest::Client,
        _target: VoiceTarget,
        _commands: UnboundedReceiver<VoiceCommand>,
        events: VoiceEvents,
    ) {
        events.send(VoiceUpdate::Ended(Some(
            "Voice is not in the mobile prototype yet.".into(),
        )));
    }
}

// ---- Local time ------------------------------------------------------------

/// The device's time zone. Per instant, so DST applies per date, as on the
/// desktop (`LocalOffset`'s contract).
#[uniffi::export(with_foreign)]
pub trait LocalTime: Send + Sync {
    /// The UTC offset in seconds at `utc_ms` (ms since the epoch).
    fn offset_secs(&self, utc_ms: i64) -> i32;
}

/// `LocalOffset` is a plain `fn`, so the app's clock lives in a static.
static LOCAL_TIME: OnceLock<Arc<dyn LocalTime>> = OnceLock::new();

fn utc_offset(utc_ms: i64) -> i32 {
    LOCAL_TIME
        .get()
        .map_or(0, |clock| clock.offset_secs(utc_ms))
}

#[path = "../../common/platform.rs"]
mod platform;
use platform::{http_client, stderr_to_logcat};

// ---- The core --------------------------------------------------------------

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    #[error("the core is already running in this process")]
    AlreadyRunning,
}

/// The running core. One per process (the store and cache are global).
#[derive(uniffi::Object)]
pub struct Core {
    commands: UnboundedSender<Command>,
}

static STARTED: Once = Once::new();

#[uniffi::export]
impl Core {
    /// Starts the core. `data_dir` holds the snapshot cache and settings
    /// (Android: `filesDir`); `device_name` is what Kova's approval page
    /// calls this device (Android: `Build.MODEL`).
    #[uniffi::constructor]
    pub fn start(
        data_dir: String,
        device_name: String,
        local_time: Arc<dyn LocalTime>,
        listener: Arc<dyn Listener>,
        vault: Arc<dyn TokenVault>,
    ) -> Result<Arc<Self>, CoreError> {
        let mut first = false;
        STARTED.call_once(|| first = true);
        if !first {
            return Err(CoreError::AlreadyRunning);
        }
        // The data folder and the device go to the core as values (the
        // desktop reads %LOCALAPPDATA% and COMPUTERNAME instead).
        stderr_to_logcat();
        yap_cache::profile::set_base(PathBuf::from(data_dir));
        let mut config = yap_core::config::Config::from_env();
        config.platform = std::env::consts::OS.to_owned();
        config.device_name = (!device_name.is_empty()).then_some(device_name);
        let _ = yap_core::config::set(config);
        let _ = LOCAL_TIME.set(local_time);

        let sink: yap_core::Sink = Box::new(move |batch: Vec<ViewUpdate>| {
            let updates: Vec<Update> = batch.into_iter().filter_map(map_update).collect();
            if !updates.is_empty() {
                listener.on_updates(updates);
            }
        });
        let commands = yap_core::spawn(
            sink,
            http_client(),
            Box::new(MobileRealtime),
            Arc::new(Vault(vault)),
            LocalOffset(utc_offset),
        );
        let core = Arc::new(Self { commands });
        core.send(Command::Start);
        Ok(core)
    }

    pub fn start_sign_in(&self) {
        self.send(Command::StartSignIn);
    }

    pub fn cancel_sign_in(&self) {
        self.send(Command::CancelSignIn);
    }

    pub fn sign_out(&self) {
        self.send(Command::SignOut);
    }

    /// The app came to the foreground (`true`) or went to the background.
    pub fn set_visible(&self, visible: bool) {
        self.send(Command::WindowShown(visible));
    }

    pub fn retry_gateway(&self) {
        self.send(Command::RetryGateway);
    }

    pub fn select_server(&self, id: String) {
        self.send(Command::SelectServer(id));
    }

    pub fn select_home(&self) {
        self.send(Command::SelectHome);
    }

    pub fn select_channel(&self, id: String) {
        self.send(Command::SelectChannel(id));
    }

    pub fn load_older(&self) {
        self.send(Command::LoadOlder);
    }
}

impl Core {
    fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

/// Flushes the snapshot cache's pending write (call from `onStop`).
#[uniffi::export]
pub fn flush_cache() {
    yap_core::shutdown();
}
