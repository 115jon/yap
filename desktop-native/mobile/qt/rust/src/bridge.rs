//! The QML singletons `Session`, `Nav` and `Chat`: the desktop's names and
//! signatures (`desktop-native/rust/src/bridge.rs`) for the part this
//! prototype covers, so the shared QML binds to them unchanged. Each
//! forwards intents to the core as a [`Command`]. Splitting the desktop's
//! bridge into this shared part and its desktop-only rest is the refactor
//! that would let both apps compile one file (README, "Crate audit").

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    /// Which top-level screen shows.
    #[qenum(Session)]
    enum Phase {
        Boot,
        SignedOut,
        DeviceCode,
        Loading,
        Ready,
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(Phase, phase)]
        #[qproperty(QString, status)]
        #[qproperty(QString, user_code, cxx_name = "userCode")]
        #[qproperty(QString, verification_url, cxx_name = "verificationUrl")]
        #[qproperty(QString, me_id, cxx_name = "meId")]
        #[qproperty(QString, me_name, cxx_name = "meName")]
        #[qproperty(QString, me_username, cxx_name = "meUsername")]
        #[qproperty(QString, me_avatar, cxx_name = "meAvatar")]
        #[qproperty(QString, me_initials, cxx_name = "meInitials")]
        type Session = super::SessionRust;

        /// Starts the core (once).
        #[qinvokable]
        fn start(self: Pin<&mut Session>);

        #[qinvokable]
        #[cxx_name = "startSignIn"]
        fn start_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "cancelSignIn"]
        fn cancel_sign_in(self: &Session);

        #[qinvokable]
        #[cxx_name = "signOut"]
        fn sign_out(self: &Session);

        /// The app came to the foreground (`true`) or went to the background:
        /// the core keeps its gateway and timers as for a shown window.
        #[qinvokable]
        #[cxx_name = "setVisible"]
        fn set_visible(self: &Session, visible: bool);
    }

    impl cxx_qt::Threading for Session {}

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, connected)]
        #[qproperty(i32, link_attempt, cxx_name = "linkAttempt")]
        #[qproperty(bool, link_offline, cxx_name = "linkOffline")]
        #[qproperty(QString, active_server_id, cxx_name = "activeServerId")]
        #[qproperty(QString, active_server_name, cxx_name = "activeServerName")]
        #[qproperty(QString, active_channel_id, cxx_name = "activeChannelId")]
        #[qproperty(QString, active_channel_name, cxx_name = "activeChannelName")]
        #[qproperty(bool, home)]
        type Nav = super::NavRust;

        #[qinvokable]
        #[cxx_name = "selectServer"]
        fn select_server(self: &Nav, id: &QString);

        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: &Nav, id: &QString);

        #[qinvokable]
        #[cxx_name = "selectHome"]
        fn select_home(self: &Nav);

        #[qinvokable]
        #[cxx_name = "prefetchChannel"]
        fn prefetch_channel(self: &Nav, id: &QString);

        #[qinvokable]
        #[cxx_name = "retryLink"]
        fn retry_link(self: &Nav);
    }

    impl cxx_qt::Initialize for Nav {}

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, typing_text, cxx_name = "typingText")]
        #[qproperty(i32, today)]
        #[qproperty(QString, divider_id, cxx_name = "dividerId")]
        #[qproperty(bool, unread_banner, cxx_name = "unreadBanner")]
        #[qproperty(i32, unread_count, cxx_name = "unreadCount")]
        #[qproperty(bool, older_loading, cxx_name = "olderLoading")]
        #[qproperty(bool, has_older, cxx_name = "hasOlder")]
        #[qproperty(bool, can_manage_messages, cxx_name = "canManageMessages")]
        #[qproperty(bool, read_only, cxx_name = "readOnly")]
        #[qproperty(bool, page_loading, cxx_name = "pageLoading")]
        type Chat = super::ChatRust;

        /// A short note for the toast line ("Copied", a failed send).
        #[qsignal]
        fn notice(self: Pin<&mut Chat>, text: QString);

        #[qsignal]
        fn reland(self: Pin<&mut Chat>);

        /// Posts to the open channel; `reply_to_id` may be empty.
        #[qinvokable]
        #[cxx_name = "sendMessage"]
        fn send_message(self: &Chat, text: &QString, reply_to_id: &QString);

        #[qinvokable]
        #[cxx_name = "retrySend"]
        fn retry_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "discardSend"]
        fn discard_send(self: &Chat, nonce: &QString);

        #[qinvokable]
        #[cxx_name = "toggleReaction"]
        fn toggle_reaction(self: &Chat, message_id: &QString, emoji: &QString);

        #[qinvokable]
        #[cxx_name = "sendTyping"]
        fn send_typing(self: &Chat);

        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: &Chat);

        #[qinvokable]
        #[cxx_name = "setAtBottom"]
        fn set_at_bottom(self: &Chat, at_bottom: bool);

        #[qinvokable]
        #[cxx_name = "setFocused"]
        fn set_focused(self: &Chat, focused: bool);

        #[qinvokable]
        #[cxx_name = "markRead"]
        fn mark_read(self: &Chat);

        #[qinvokable]
        #[cxx_name = "toggleCategory"]
        fn toggle_category(self: &Chat, category_id: &QString);
    }

    impl cxx_qt::Initialize for Chat {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt::Threading;
use cxx_qt_lib::QString;
use yap_core::store::time::format_rfc3339;
use yap_core::{Command, Phase, ViewUpdate};

use crate::{client, gui};

impl Default for qobject::Phase {
    fn default() -> Self {
        Self::Boot
    }
}

impl From<Phase> for qobject::Phase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Boot => Self::Boot,
            Phase::SignedOut => Self::SignedOut,
            Phase::DeviceCode => Self::DeviceCode,
            Phase::Loading => Self::Loading,
            Phase::Ready => Self::Ready,
        }
    }
}

#[derive(Default)]
pub struct SessionRust {
    phase: qobject::Phase,
    status: QString,
    user_code: QString,
    verification_url: QString,
    me_id: QString,
    me_name: QString,
    me_username: QString,
    me_avatar: QString,
    me_initials: QString,
}

#[derive(Default)]
pub struct NavRust {
    connected: bool,
    link_attempt: i32,
    link_offline: bool,
    active_server_id: QString,
    active_server_name: QString,
    active_channel_id: QString,
    active_channel_name: QString,
    home: bool,
}

#[derive(Default)]
pub struct ChatRust {
    typing_text: QString,
    today: i32,
    divider_id: QString,
    unread_banner: bool,
    unread_count: i32,
    older_loading: bool,
    has_older: bool,
    can_manage_messages: bool,
    read_only: bool,
    page_loading: bool,
}

/// View updates waiting for the Qt thread, and whether a closure that will
/// drain them is already queued (the desktop's batching, unchanged).
#[derive(Default)]
struct Pending {
    updates: Vec<ViewUpdate>,
    queued: bool,
}

impl qobject::Session {
    pub fn start(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        let pending = Arc::new(Mutex::new(Pending::default()));
        let sink: yap_core::Sink = Box::new(move |batch| {
            let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
            waiting.updates.extend(batch);
            if std::mem::replace(&mut waiting.queued, true) {
                return;
            }
            drop(waiting);
            let drain = Arc::clone(&pending);
            let queued = qt.queue(move |session| {
                let updates = {
                    let mut waiting = drain.lock().unwrap_or_else(PoisonError::into_inner);
                    waiting.queued = false;
                    std::mem::take(&mut waiting.updates)
                };
                gui::apply(session, updates);
            });
            if queued.is_err() {
                let mut waiting = pending.lock().unwrap_or_else(PoisonError::into_inner);
                waiting.queued = false;
                waiting.updates.clear();
            }
        });
        client::start(sink);
    }

    pub fn start_sign_in(&self) {
        client::command(Command::StartSignIn);
    }

    pub fn cancel_sign_in(&self) {
        client::command(Command::CancelSignIn);
    }

    pub fn sign_out(&self) {
        client::command(Command::SignOut);
    }

    pub fn set_visible(&self, visible: bool) {
        client::command(Command::WindowShown(visible));
    }
}

impl qobject::Nav {
    pub fn select_server(&self, id: &QString) {
        client::command(Command::SelectServer(id.to_string()));
    }

    pub fn select_channel(&self, id: &QString) {
        client::command(Command::SelectChannel(id.to_string()));
    }

    pub fn select_home(&self) {
        client::command(Command::SelectHome);
    }

    pub fn prefetch_channel(&self, id: &QString) {
        client::command(Command::PrefetchChannel(id.to_string()));
    }

    pub fn retry_link(&self) {
        client::command(Command::RetryGateway);
    }
}

/// A nonce no other send from this process shares (the desktop's form).
fn stamp() -> (String, String) {
    static SENT: AtomicU64 = AtomicU64::new(0);
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let n = SENT.fetch_add(1, Ordering::Relaxed);
    let ms = i64::try_from(now.as_millis()).unwrap_or(i64::MAX);
    (
        format!("mobile-{:x}-{n:x}", now.as_nanos()),
        format_rfc3339(ms),
    )
}

fn optional(text: &QString) -> Option<String> {
    let text = text.to_string();
    (!text.is_empty()).then_some(text)
}

impl qobject::Chat {
    pub fn send_message(&self, text: &QString, reply_to_id: &QString) {
        let (nonce, sent_at) = stamp();
        client::command(Command::Send {
            content: text.to_string(),
            reply_to_id: optional(reply_to_id),
            nonce,
            sent_at,
        });
    }

    pub fn retry_send(&self, nonce: &QString) {
        client::command(Command::RetrySend(nonce.to_string()));
    }

    pub fn discard_send(&self, nonce: &QString) {
        client::command(Command::DiscardSend(nonce.to_string()));
    }

    pub fn toggle_reaction(&self, message_id: &QString, emoji: &QString) {
        client::command(Command::ToggleReaction {
            message_id: message_id.to_string(),
            emoji: emoji.to_string(),
        });
    }

    pub fn send_typing(&self) {
        client::command(Command::Typing);
    }

    pub fn load_older(&self) {
        client::command(Command::LoadOlder);
    }

    pub fn set_at_bottom(&self, at_bottom: bool) {
        client::command(Command::SetAtBottom(at_bottom));
    }

    pub fn set_focused(&self, focused: bool) {
        client::command(Command::SetFocused(focused));
    }

    pub fn mark_read(&self) {
        client::command(Command::MarkRead);
    }

    pub fn toggle_category(&self, category_id: &QString) {
        client::command(Command::ToggleCategory(category_id.to_string()));
    }
}

macro_rules! registered {
    ($($object:ident => $rust:ident),* $(,)?) => {$(
        impl cxx_qt::Initialize for qobject::$object {
            fn initialize(self: Pin<&mut Self>) {
                gui::register(self);
            }
        }

        impl Drop for $rust {
            fn drop(&mut self) {
                gui::unregister::<qobject::$object>();
            }
        }
    )*};
}

registered!(Nav => NavRust, Chat => ChatRust);
