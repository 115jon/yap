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
        type Chat = super::ChatRust;

        #[qinvokable]
        #[cxx_name = "sendMessage"]
        fn send_message(self: &Chat, text: &QString);
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
        type Voice = super::VoiceRust;

        #[qinvokable]
        fn join(self: &Voice, channel_id: &QString);

        #[qinvokable]
        fn leave(self: &Voice);

        #[qinvokable]
        #[cxx_name = "setMuted"]
        fn set_muted_from_ui(self: &Voice, muted: bool);
    }

    impl cxx_qt::Initialize for Voice {}
}

use core::pin::Pin;
use std::sync::{Arc, Mutex, PoisonError};

use cxx_qt::Threading;
use cxx_qt_lib::QString;
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
}

#[derive(Default)]
pub struct NavRust {
    connected: bool,
    active_server_id: QString,
    active_server_name: QString,
    active_channel_id: QString,
    active_channel_name: QString,
}

#[derive(Default)]
pub struct ChatRust {
    typing_text: QString,
    today: i32,
}

#[derive(Default)]
pub struct VoiceRust {
    channel_id: QString,
    channel_name: QString,
    status: QString,
    live: bool,
    muted: bool,
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

impl qobject::Chat {
    pub fn send_message(&self, text: &QString) {
        client::command(Command::Send(text.to_string()));
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
