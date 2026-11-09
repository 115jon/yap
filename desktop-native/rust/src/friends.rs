//! `Friends`: relationships for the Friends view, the Home badges and the
//! profile card. Counts come as properties (the rail and the Friends row
//! bind them); everything else is one JSON `state`
//! ([`yap_core::store::FriendsView`]).

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
        /// Incoming friend requests (the Pending tab and the Friends row).
        #[qproperty(i32, pending)]
        /// The Home key's count: unread DMs past the rail's three, plus
        /// incoming requests.
        #[qproperty(i32, home_badge, cxx_name = "homeBadge")]
        /// `{loading, online, all, incoming, sent, blocked, pending,
        /// homeBadge, add: {kind, message, sent}, acts: {<userId>: {busy,
        /// error, failed, done}}, card: {userId, relation}}`. Rows are
        /// `{userId, name, username, avatar, initials, presence, statusLine,
        /// kind}`.
        #[qproperty(QString, state)]
        type Friends = super::FriendsRust;

        /// The Friends view opened: the list loads (with its loading rows).
        #[qinvokable]
        fn load(self: &Friends);

        /// A profile card opened: the list loads quietly.
        #[qinvokable]
        fn refresh(self: &Friends);

        #[qinvokable]
        #[cxx_name = "sendRequest"]
        fn send_request(self: &Friends, username: &QString);

        /// The add field changed: its error goes.
        #[qinvokable]
        #[cxx_name = "clearAdd"]
        fn clear_add(self: &Friends);

        /// `add`, `accept`, `decline`, `cancel`, `remove`, `block` or
        /// `unblock` on one person; `name` completes the error copy.
        #[qinvokable]
        fn act(
            self: &Friends,
            action: &QString,
            user_id: &QString,
            username: &QString,
            name: &QString,
        );

        /// Forget a person's last outcome.
        #[qinvokable]
        fn clear(self: &Friends, user_id: &QString);
    }

    impl cxx_qt::Initialize for Friends {}
}

use core::pin::Pin;

use cxx_qt_lib::QString;
use yap_core::store::{Command, FriendAction, FriendsCommand, FriendsView};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct FriendsRust {
    pending: i32,
    home_badge: i32,
    state: QString,
}

fn send(command: FriendsCommand) {
    client::command(Command::Friends(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn apply(mut object: Pin<&mut qobject::Friends>, view: &FriendsView) {
    object.as_mut().set_pending(int(view.pending));
    object.as_mut().set_home_badge(int(view.home_badge));
    let state = serde_json::to_string(view).unwrap_or_else(|_| "{}".into());
    object.set_state(QString::from(&state));
}

/// The store's friends view changed.
pub(crate) fn on_view(view: FriendsView) {
    if let Some(object) = gui::object::<qobject::Friends>() {
        apply(object, &view);
    }
    gui::keep_friends(view);
}

impl gui::Slot for qobject::Friends {
    fn slot(objects: &mut gui::Objects) -> &mut Option<std::ptr::NonNull<Self>> {
        &mut objects.friends
    }

    fn restore(self: Pin<&mut Self>) {
        apply(self, &gui::friends_view());
    }
}

impl cxx_qt::Initialize for qobject::Friends {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for FriendsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Friends>();
    }
}

impl qobject::Friends {
    pub fn load(&self) {
        send(FriendsCommand::Load);
    }

    pub fn refresh(&self) {
        send(FriendsCommand::Refresh);
    }

    pub fn send_request(&self, username: &QString) {
        send(FriendsCommand::SendRequest(text(username)));
    }

    pub fn clear_add(&self) {
        send(FriendsCommand::ClearAdd);
    }

    pub fn act(&self, action: &QString, user_id: &QString, username: &QString, name: &QString) {
        let Some(action) = FriendAction::parse(&text(action)) else {
            return;
        };
        send(FriendsCommand::Act {
            action,
            user_id: text(user_id),
            username: text(username),
            name: text(name),
        });
    }

    pub fn clear(&self, user_id: &QString) {
        send(FriendsCommand::Clear(text(user_id)));
    }
}
