//! `Invites`: Create a server, the invite landing, and the links that lead
//! into the app (deep links from the shell, links clicked in messages).
//! Everything the dialogs show is one JSON `state`
//! ([`yap_core::store::invites::InviteView`]); `landingRequested` opens the
//! landing and `jumpRequested` asks the chat to show a linked message. The
//! shell (`cpp/desktopshell.cpp`) hands deep links to `openArg` and applies
//! `schemeRegistration` to the registry.

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
        /// `{create: {busy, error, done}, createNameError, landing: {code,
        /// step, expired, hasInvite, serverId, serverName, serverIcon,
        /// serverInitial, members, inviter} | null, landingSeq,
        /// jumpChannelId, jumpMessageId, jumpSeq, settingsPanel,
        /// settingsRow, settingsSeq, navSeq}`.
        #[qproperty(QString, state)]
        /// A meeting room a link asked to join (`yap_core::rooms`), until
        /// the join prompt takes it (`clearRoomRequest`); empty otherwise.
        /// Signed in or not: the prompt shows wherever the app is.
        #[qproperty(QString, room_request, cxx_name = "roomRequest")]
        /// The app has a join prompt for meeting rooms: room links are
        /// ours. Off (an app without one), they are not links into the
        /// app and open in the browser as before.
        #[qproperty(bool, rooms)]
        type Invites = super::InvitesRust;

        /// A link asked to join the meeting room in `roomRequest`.
        #[qsignal]
        #[cxx_name = "roomRequested"]
        fn room_requested(self: Pin<&mut Invites>);

        /// The join prompt took `roomRequest`.
        #[qinvokable]
        #[cxx_name = "clearRoomRequest"]
        fn clear_room_request(self: Pin<&mut Invites>);

        /// A link opened the invite landing.
        #[qsignal]
        #[cxx_name = "landingRequested"]
        fn landing_requested(self: Pin<&mut Invites>);

        /// A message link: show `message_id` in `channel_id` (its channel is
        /// already opening).
        #[qsignal]
        #[cxx_name = "jumpRequested"]
        fn jump_requested(self: Pin<&mut Invites>, channel_id: QString, message_id: QString);

        /// A settings link: open Settings on `panel` and land on `row` (none
        /// when empty).
        #[qsignal]
        #[cxx_name = "settingsRequested"]
        fn settings_requested(self: Pin<&mut Invites>, panel: QString, row: QString);

        /// A link led somewhere in the app (an invite, Home, a server, a
        /// channel): what covers the chat closes.
        #[qsignal]
        #[cxx_name = "linkFollowed"]
        fn link_followed(self: Pin<&mut Invites>);

        /// The Create a server dialog opened.
        #[qinvokable]
        #[cxx_name = "startCreate"]
        fn start_create(self: &Invites);

        /// Why a file can't be the new server's icon (the web's copy), or
        /// empty.
        #[qinvokable]
        #[cxx_name = "iconProblem"]
        fn icon_problem(self: &Invites, path: &QString) -> QString;

        /// `icon_path` may be empty.
        #[qinvokable]
        #[cxx_name = "createServer"]
        fn create_server(self: &Invites, name: &QString, icon_path: &QString);

        /// A clicked link: true when it is one of ours (an invite or a
        /// channel on this site, a `yap://` or `yap-native://` link), which
        /// then opens here; false leaves it to the browser.
        #[qinvokable]
        #[cxx_name = "openLink"]
        fn open_link(self: Pin<&mut Invites>, url: &QString) -> bool;

        /// The channel (or DM) a link opens, or empty when it opens none:
        /// for a shell that shows the conversation once it is the open one.
        #[qinvokable]
        #[cxx_name = "linkChannel"]
        fn link_channel(self: &Invites, url: &QString) -> QString;

        /// A command-line argument (first launch or a second instance):
        /// only `yap://` and `yap-native://` links count.
        #[qinvokable]
        #[cxx_name = "openArg"]
        fn open_arg(self: Pin<&mut Invites>, arg: &QString) -> bool;

        /// A bare code or an invite link.
        #[qinvokable]
        #[cxx_name = "openInvite"]
        fn open_invite(self: &Invites, text: &QString) -> bool;

        #[qinvokable]
        fn accept(self: &Invites);

        #[qinvokable]
        fn retry(self: &Invites);

        #[qinvokable]
        #[cxx_name = "closeLanding"]
        fn close_landing(self: &Invites);

        #[qinvokable]
        #[cxx_name = "openServer"]
        fn open_server(self: &Invites, server_id: &QString);

        /// The `yap://` and `yap-native://` registration for `exe` as `key\tname\tdata`
        /// lines under HKCU (the shell writes the ones that differ). Empty
        /// unless `exe` is the released install (`install::PACK_ID`) as the
        /// default profile: no other build takes the schemes at launch.
        #[qinvokable]
        #[cxx_name = "schemeRegistration"]
        fn scheme_registration(self: &Invites, exe: &QString) -> QString;
    }

    impl cxx_qt::Initialize for Invites {}
}

use core::pin::Pin;
use std::cell::RefCell;

use cxx_qt_lib::QString;
use yap_core::links::{self, Link};
use yap_core::store::invites::{create_icon_problem, InviteView};
use yap_core::store::{Command, InviteCommand};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct InvitesRust {
    state: QString,
    room_request: QString,
    rooms: bool,
}

thread_local! {
    /// The last view, for an object QML creates later, and the bumps the
    /// signals were sent for (landing, jump, settings, nav).
    static LAST: RefCell<(InviteView, [u64; 4])> = RefCell::new(Default::default());
}

fn q(text: &str) -> QString {
    QString::from(text)
}

fn send(command: InviteCommand) {
    client::command(Command::Invites(command));
}

fn state_json(view: &InviteView) -> String {
    serde_json::to_string(view).unwrap_or_else(|_| "{}".into())
}

/// The store's view changed: the JSON, then the signals for new bumps.
pub fn on_view(view: InviteView) {
    let [landing, jump, settings, nav] = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let seqs = [
            view.landing_seq,
            view.jump_seq,
            view.settings_seq,
            view.nav_seq,
        ];
        let bumped = std::array::from_fn(|i| seqs[i] > last.1[i]);
        *last = (view.clone(), seqs);
        bumped
    });
    let Some(mut object) = gui::object::<qobject::Invites>() else {
        return;
    };
    object.as_mut().set_state(q(&state_json(&view)));
    // First, so a landing or settings the same link opens stays open.
    if nav {
        object.as_mut().link_followed();
    }
    if landing {
        object.as_mut().landing_requested();
    }
    if jump {
        object
            .as_mut()
            .jump_requested(q(&view.jump_channel_id), q(&view.jump_message_id));
    }
    if settings {
        object
            .as_mut()
            .settings_requested(q(&view.settings_panel), q(&view.settings_row));
    }
}

impl gui::Slot for qobject::Invites {
    crate::slot_cell!(qobject::Invites);

    fn restore(self: Pin<&mut Self>) {
        let view = LAST.with(|last| last.borrow().0.clone());
        self.set_state(q(&state_json(&view)));
    }
}

impl cxx_qt::Initialize for qobject::Invites {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for InvitesRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Invites>();
    }
}

fn open(link: Option<Link>) -> bool {
    match link {
        Some(link) => {
            send(InviteCommand::OpenLink(link));
            true
        }
        None => false,
    }
}

/// The run's test scheme (`YAP_TEST_SCHEME=yap-native-<name>`), read once:
/// deep links in it read as `yap://` ones, so an end-to-end test can
/// point a scheme of its own at a build. The app never registers it.
fn test_scheme() -> Option<&'static str> {
    static SCHEME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    SCHEME
        .get_or_init(|| {
            std::env::var("YAP_TEST_SCHEME")
                .ok()
                .and_then(|v| links::test_scheme(&v))
        })
        .as_deref()
}

fn web_origin() -> &'static str {
    &yap_core::config::get().api_base
}

impl qobject::Invites {
    pub fn start_create(&self) {
        send(InviteCommand::StartCreate);
    }

    pub fn icon_problem(&self, path: &QString) -> QString {
        let path = path.to_string();
        let size = std::fs::metadata(&path).ok().map(|m| m.len());
        create_icon_problem(crate::admin::icon_content_type(&path), size)
            .map(q)
            .unwrap_or_default()
    }

    pub fn create_server(&self, name: &QString, icon_path: &QString) {
        let path = icon_path.to_string();
        let icon = crate::admin::icon_content_type(&path).map(|t| (path.clone(), t.to_owned()));
        send(InviteCommand::CreateServer {
            name: name.to_string(),
            icon,
        });
    }

    pub fn open_link(self: Pin<&mut Self>, url: &QString) -> bool {
        let link = links::parse_link(&url.to_string(), web_origin());
        self.follow(link)
    }

    /// A room link goes to the join prompt; any other to the store.
    fn follow(self: Pin<&mut Self>, link: Option<Link>) -> bool {
        match link {
            Some(Link::Room(_)) if !*self.rooms() => false,
            Some(Link::Room(slug)) => {
                let mut this = self;
                this.as_mut().set_room_request(q(&slug));
                this.room_requested();
                true
            }
            other => open(other),
        }
    }

    pub fn clear_room_request(self: Pin<&mut Self>) {
        self.set_room_request(QString::default());
    }

    pub fn link_channel(&self, url: &QString) -> QString {
        match links::parse_link(&url.to_string(), web_origin()) {
            Some(Link::Channel { channel_id, .. }) => q(&channel_id),
            _ => QString::default(),
        }
    }

    pub fn open_arg(self: Pin<&mut Self>, arg: &QString) -> bool {
        let arg = arg.to_string();
        // A room's https link handed to the app opens it too; no other
        // site link does from the command line.
        let link = links::deep_link_arg_with(&arg, test_scheme()).or_else(|| {
            links::parse_link(&arg, web_origin()).filter(|l| matches!(l, Link::Room(_)))
        });
        match &link {
            Some(link) => eprintln!("[link] deep link: {}", link.kind()),
            // A flag or a path says nothing; a URL we refused does.
            None if arg.contains("://") => eprintln!("[link] deep link refused"),
            None => {}
        }
        self.follow(link)
    }

    pub fn open_invite(&self, text: &QString) -> bool {
        open(links::invite_code(&text.to_string(), web_origin()).map(Link::Invite))
    }

    pub fn accept(&self) {
        send(InviteCommand::Accept);
    }

    pub fn retry(&self) {
        send(InviteCommand::Retry);
    }

    pub fn close_landing(&self) {
        send(InviteCommand::CloseLanding);
    }

    pub fn open_server(&self, server_id: &QString) {
        send(InviteCommand::OpenServer(server_id.to_string()));
    }

    pub fn scheme_registration(&self, exe: &QString) -> QString {
        q(&crate::platform::platform().scheme_registration(&exe.to_string()))
    }
}
