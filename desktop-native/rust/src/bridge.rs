//! The QML-facing domain objects, one QML singleton per domain: `Session`,
//! `Nav`, `Chat` and `Voice`. Each holds typed properties and forwards
//! intents to the core as [`Command`]s. The list models are their own
//! singletons in [`crate::models`]: cxx-qt 0.10 has no clean way to expose
//! a Rust-owned child QObject as a property, and a singleton needs no
//! ownership at all.
//!
//! `Session.start()` starts the core. Its view updates arrive in batches;
//! the bridge keeps at most one queued Qt-thread closure, which drains every
//! batch that arrived meanwhile and applies them in order ([`gui::apply`]).

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
        /// Sign-in and the signed-in user.
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
        /// `online`, `idle`, `dnd` or `offline`.
        #[qproperty(QString, me_status, cxx_name = "meStatus")]
        #[qproperty(QString, me_custom_status, cxx_name = "meCustomStatus")]
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
        #[cxx_name = "useToken"]
        fn use_token(self: &Session, token: &QString);

        #[qinvokable]
        #[cxx_name = "signOut"]
        fn sign_out(self: &Session);
    }

    impl cxx_qt::Threading for Session {}

    extern "RustQt" {
        /// Where the user is: server, channel, and the gateway link.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, connected)]
        #[qproperty(QString, active_server_id, cxx_name = "activeServerId")]
        #[qproperty(QString, active_server_name, cxx_name = "activeServerName")]
        #[qproperty(QString, active_channel_id, cxx_name = "activeChannelId")]
        #[qproperty(QString, active_channel_name, cxx_name = "activeChannelName")]
        /// The open voice channel's view (landing or call); empty when none.
        #[qproperty(QString, voice_panel_id, cxx_name = "voicePanelId")]
        #[qproperty(QString, voice_panel_name, cxx_name = "voicePanelName")]
        #[qproperty(i32, voice_panel_count, cxx_name = "voicePanelCount")]
        type Nav = super::NavRust;

        #[qinvokable]
        #[cxx_name = "selectServer"]
        fn select_server(self: &Nav, id: &QString);

        /// Opens a text channel, or joins a voice channel.
        #[qinvokable]
        #[cxx_name = "selectChannel"]
        fn select_channel(self: &Nav, id: &QString);
    }

    impl cxx_qt::Initialize for Nav {}

    extern "RustQt" {
        /// The open channel. Its rows are `MessageListModel`.
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, typing_text, cxx_name = "typingText")]
        // Today's local day number, to compare with a row's `day`.
        #[qproperty(i32, today)]
        // The "New" divider: the message it sits above (empty: none), the
        // banner, its count and when the first unread message was sent.
        #[qproperty(QString, divider_id, cxx_name = "dividerId")]
        #[qproperty(bool, unread_banner, cxx_name = "unreadBanner")]
        #[qproperty(i32, unread_count, cxx_name = "unreadCount")]
        #[qproperty(QString, unread_since_time, cxx_name = "unreadSinceTime")]
        #[qproperty(QString, unread_since_date, cxx_name = "unreadSinceDate")]
        #[qproperty(i32, unread_since_day, cxx_name = "unreadSinceDay")]
        // Older pages: one is loading; there are more to load.
        #[qproperty(bool, older_loading, cxx_name = "olderLoading")]
        #[qproperty(bool, has_older, cxx_name = "hasOlder")]
        // Delete is offered on others' messages too (MANAGE_MESSAGES).
        #[qproperty(bool, can_manage_messages, cxx_name = "canManageMessages")]
        type Chat = super::ChatRust;

        /// Something failed that the user should hear about (a toast).
        #[qsignal]
        fn notice(self: Pin<&mut Chat>, text: QString);

        /// The divider was placed after the list had landed: land again.
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
        #[cxx_name = "editMessage"]
        fn edit_message(self: &Chat, id: &QString, text: &QString);

        #[qinvokable]
        #[cxx_name = "deleteMessage"]
        fn delete_message(self: &Chat, id: &QString);

        /// The composer has text; call at most every 3 s.
        #[qinvokable]
        #[cxx_name = "sendTyping"]
        fn send_typing(self: &Chat);

        #[qinvokable]
        #[cxx_name = "loadOlder"]
        fn load_older(self: &Chat);

        #[qinvokable]
        #[cxx_name = "setAtBottom"]
        fn set_at_bottom(self: &Chat, at_bottom: bool);

        /// The top-most message on screen, reported with the end.
        #[qinvokable]
        #[cxx_name = "setSeenFrom"]
        fn set_seen_from(self: &Chat, message_id: &QString);

        #[qinvokable]
        #[cxx_name = "setFocused"]
        fn set_focused(self: &Chat, focused: bool);

        #[qinvokable]
        #[cxx_name = "markRead"]
        fn mark_read(self: &Chat);

        /// Filters `MentionListModel` by username; an empty query closes it.
        #[qinvokable]
        #[cxx_name = "queryMentions"]
        fn query_mentions(self: &Chat, query: &QString, open: bool);

        /// The newest of our own sent messages (Up to edit), or empty.
        #[qinvokable]
        #[cxx_name = "lastEditableId"]
        fn last_editable_id(self: &Chat) -> QString;

        /// A message's row in `MessageListModel`, or -1.
        #[qinvokable]
        #[cxx_name = "indexOfMessage"]
        fn index_of_message(self: &Chat, id: &QString) -> i32;
    }

    impl cxx_qt::Initialize for Chat {}

    extern "RustQt" {
        /// The call. Its members are `VoiceRosterModel` (and member rows in
        /// `ChannelTreeModel`).
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, channel_id, cxx_name = "channelId")]
        #[qproperty(QString, channel_name, cxx_name = "channelName")]
        #[qproperty(QString, status)]
        // The media path is up.
        #[qproperty(bool, live)]
        #[qproperty(bool, muted)]
        #[qproperty(bool, deafened)]
        /// Why the last call failed; empty once a join starts.
        #[qproperty(QString, error)]
        type Voice = super::VoiceRust;

        #[qinvokable]
        fn join(self: &Voice, channel_id: &QString);

        #[qinvokable]
        fn leave(self: &Voice);

        #[qinvokable]
        #[cxx_name = "requestMute"]
        fn set_muted_from_ui(self: &Voice, muted: bool);

        #[qinvokable]
        #[cxx_name = "requestDeafen"]
        fn set_deafened_from_ui(self: &Voice, deafened: bool);
    }

    impl cxx_qt::Initialize for Voice {}
}

use core::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cxx_qt::Threading;
use cxx_qt_lib::QString;
use yap_core::store::time::format_rfc3339;
use yap_core::{Phase, ViewUpdate};

use crate::client::{self, Command};
use crate::gui;

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
    me_status: QString,
    me_custom_status: QString,
}

impl Drop for SessionRust {
    fn drop(&mut self) {
        client::shutdown();
    }
}

#[derive(Default)]
pub struct NavRust {
    connected: bool,
    active_server_id: QString,
    active_server_name: QString,
    active_channel_id: QString,
    active_channel_name: QString,
    voice_panel_id: QString,
    voice_panel_name: QString,
    voice_panel_count: i32,
}

#[derive(Default)]
pub struct ChatRust {
    typing_text: QString,
    today: i32,
    divider_id: QString,
    unread_banner: bool,
    unread_count: i32,
    unread_since_time: QString,
    unread_since_date: QString,
    unread_since_day: i32,
    older_loading: bool,
    has_older: bool,
    can_manage_messages: bool,
}

#[derive(Default)]
pub struct VoiceRust {
    channel_id: QString,
    channel_name: QString,
    status: QString,
    live: bool,
    muted: bool,
    deafened: bool,
    error: QString,
}

/// View updates waiting for the Qt thread, and whether a closure that will
/// drain them is already queued.
#[derive(Default)]
struct Pending {
    updates: Vec<ViewUpdate>,
    queued: bool,
}

impl qobject::Session {
    pub fn start(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        let pending = Arc::new(Mutex::new(Pending::default()));
        let sink: client::Sink = Box::new(move |batch| {
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
            // Fails only once the Session is gone (shutdown): nothing will
            // drain, so drop what is waiting instead of growing forever.
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

    pub fn use_token(&self, token: &QString) {
        client::command(Command::UseToken(token.to_string()));
    }

    pub fn sign_out(&self) {
        client::command(Command::SignOut);
    }
}

impl qobject::Nav {
    pub fn select_server(&self, id: &QString) {
        client::command(Command::SelectServer(id.to_string()));
    }

    pub fn select_channel(&self, id: &QString) {
        client::command(Command::SelectChannel(id.to_string()));
    }
}

/// A nonce no other send from this process shares: the wall clock in
/// nanoseconds and a counter.
fn next_nonce(now: Duration) -> String {
    static SENT: AtomicU64 = AtomicU64::new(0);
    let n = SENT.fetch_add(1, Ordering::Relaxed);
    format!("native-{:x}-{n:x}", now.as_nanos())
}

impl qobject::Chat {
    pub fn send_message(&self, text: &QString, reply_to_id: &QString) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let ms = i64::try_from(now.as_millis()).unwrap_or(i64::MAX);
        let reply = reply_to_id.to_string();
        client::command(Command::Send {
            content: text.to_string(),
            reply_to_id: (!reply.is_empty()).then_some(reply),
            nonce: next_nonce(now),
            sent_at: format_rfc3339(ms),
        });
    }

    pub fn retry_send(&self, nonce: &QString) {
        client::command(Command::RetrySend(nonce.to_string()));
    }

    pub fn discard_send(&self, nonce: &QString) {
        client::command(Command::DiscardSend(nonce.to_string()));
    }

    pub fn edit_message(&self, id: &QString, text: &QString) {
        client::command(Command::EditMessage {
            message_id: id.to_string(),
            content: text.to_string(),
        });
    }

    pub fn delete_message(&self, id: &QString) {
        client::command(Command::DeleteMessage(id.to_string()));
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

    pub fn set_seen_from(&self, message_id: &QString) {
        client::command(Command::SeenFrom(message_id.to_string()));
    }

    pub fn set_focused(&self, focused: bool) {
        client::command(Command::SetFocused(focused));
    }

    pub fn mark_read(&self) {
        client::command(Command::MarkRead);
    }

    pub fn query_mentions(&self, query: &QString, open: bool) {
        client::command(Command::MentionQuery(open.then(|| query.to_string())));
    }

    pub fn last_editable_id(&self) -> QString {
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .rev()
                .find(|r| r.mine && !r.pending && !r.failed)
                .map(|r| QString::from(r.id.as_str()))
                .unwrap_or_default()
        })
    }

    pub fn index_of_message(&self, id: &QString) -> i32 {
        let id = id.to_string();
        gui::with_rows(|rows| {
            rows.messages
                .iter()
                .position(|r| r.id == id)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        })
    }
}

impl qobject::Voice {
    pub fn join(&self, channel_id: &QString) {
        client::command(Command::JoinVoice(channel_id.to_string()));
    }

    pub fn leave(&self) {
        client::command(Command::LeaveVoice);
    }

    pub fn set_muted_from_ui(&self, muted: bool) {
        client::command(Command::SetVoiceMuted(muted));
    }

    pub fn set_deafened_from_ui(&self, deafened: bool) {
        client::command(Command::SetVoiceDeafened(deafened));
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

registered!(Nav => NavRust, Chat => ChatRust, Voice => VoiceRust);
