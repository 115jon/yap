//! `Panels`: the thread and pins side panels ([`yap_core::store::PanelsView`]).
//! Their rows are `ThreadMessageModel` and `PinnedMessageModel`
//! ([`crate::models`]); this object carries the scalars and the intents.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Pin and unpin are offered in the open channel.
        #[qproperty(bool, can_pin, cxx_name = "canPin")]
        /// The open thread's root message, empty when none is open.
        #[qproperty(QString, thread_root_id, cxx_name = "threadRootId")]
        #[qproperty(bool, thread_loading, cxx_name = "threadLoading")]
        /// The load's error copy, empty otherwise.
        #[qproperty(QString, thread_error, cxx_name = "threadError")]
        /// Replies shown under the root.
        #[qproperty(i32, thread_replies, cxx_name = "threadReplies")]
        #[qproperty(bool, pins_loading, cxx_name = "pinsLoading")]
        #[qproperty(i32, pins_count, cxx_name = "pinsCount")]
        /// The member list's Threads tab.
        #[qproperty(bool, threads_loading, cxx_name = "threadsLoading")]
        #[qproperty(QString, threads_error, cxx_name = "threadsError")]
        /// Its rows as JSON: `[{id, author, initials, avatar, content,
        /// replies, lastReply}]`.
        #[qproperty(QString, threads)]
        type Panels = super::PanelsRust;

        /// Shows the thread of a message in the open channel.
        #[qinvokable]
        #[cxx_name = "openThread"]
        fn open_thread(self: &Panels, root_id: &QString);

        #[qinvokable]
        #[cxx_name = "closeThread"]
        fn close_thread(self: &Panels);

        /// Replies in the open thread.
        #[qinvokable]
        #[cxx_name = "sendReply"]
        fn send_reply(self: &Panels, text: &QString);

        /// The pins panel opened, or its channel changed.
        #[qinvokable]
        #[cxx_name = "openPins"]
        fn open_pins(self: &Panels);

        /// Pin or unpin a message (after the confirm, or with Shift).
        #[qinvokable]
        #[cxx_name = "setPinned"]
        fn set_pinned(self: &Panels, message_id: &QString, pinned: bool);

        /// The Threads tab showed: the open channel's threads load afresh.
        #[qinvokable]
        #[cxx_name = "listThreads"]
        fn list_threads(self: &Panels, now_ms: f64);
    }

    impl cxx_qt::Initialize for Panels {}
}

use core::pin::Pin;

use cxx_qt_lib::QString;
use yap_core::store::{Command, PanelsView, PinsCommand, ThreadCommand};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct PanelsRust {
    can_pin: bool,
    thread_root_id: QString,
    thread_loading: bool,
    thread_error: QString,
    thread_replies: i32,
    pins_loading: bool,
    pins_count: i32,
    threads_loading: bool,
    threads_error: QString,
    threads: QString,
}

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn apply(mut object: Pin<&mut qobject::Panels>, view: &PanelsView) {
    object.as_mut().set_can_pin(view.can_pin);
    object
        .as_mut()
        .set_thread_root_id(QString::from(view.thread_root_id.as_str()));
    object.as_mut().set_thread_loading(view.thread_loading);
    object
        .as_mut()
        .set_thread_error(QString::from(view.thread_error.as_str()));
    object.as_mut().set_thread_replies(int(view.thread_replies));
    object.as_mut().set_pins_loading(view.pins_loading);
    object.as_mut().set_pins_count(int(view.pins_count));
    object.as_mut().set_threads_loading(view.threads_loading);
    object
        .as_mut()
        .set_threads_error(QString::from(view.threads_error.as_str()));
    let rows = serde_json::to_string(&view.threads).unwrap_or_else(|_| "[]".to_owned());
    object.set_threads(QString::from(rows.as_str()));
}

/// The store's panels view changed.
pub(crate) fn on_view(view: PanelsView) {
    if let Some(object) = gui::object::<qobject::Panels>() {
        apply(object, &view);
    }
    gui::keep_panels(view);
}

impl gui::Slot for qobject::Panels {
    fn slot(objects: &mut gui::Objects) -> &mut Option<std::ptr::NonNull<Self>> {
        &mut objects.panels
    }

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::panels_view());
    }
}

impl cxx_qt::Initialize for qobject::Panels {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for PanelsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Panels>();
    }
}

impl qobject::Panels {
    pub fn open_thread(&self, root_id: &QString) {
        client::command(Command::Thread(ThreadCommand::Open(root_id.to_string())));
    }

    pub fn close_thread(&self) {
        client::command(Command::Thread(ThreadCommand::Close));
    }

    pub fn send_reply(&self, text: &QString) {
        let (nonce, sent_at) = crate::bridge::stamp();
        client::command(Command::Thread(ThreadCommand::Send {
            content: text.to_string(),
            nonce,
            sent_at,
        }));
    }

    pub fn open_pins(&self) {
        client::command(Command::Pins(PinsCommand::Open));
    }

    pub fn list_threads(&self, now_ms: f64) {
        // `Date.now()`: whole epoch milliseconds.
        client::command(Command::Thread(ThreadCommand::List {
            now_ms: now_ms as i64,
        }));
    }

    pub fn set_pinned(&self, message_id: &QString, pinned: bool) {
        client::command(Command::Pins(PinsCommand::SetPinned {
            message_id: message_id.to_string(),
            pinned,
        }));
    }
}
