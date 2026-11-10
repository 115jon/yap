//! GUI-thread state, as the desktop's `gui.rs`: the rows every list model
//! shows, the last scalar values for singletons QML creates late, and the
//! live QObjects. The registry API (`Rows`, `Slot`, `register`, `object`,
//! `with_rows`) is the one the desktop's `models.rs` compiles against, so
//! that file is used here unchanged. Only the domain objects this prototype
//! has (`Session`, `Nav`, `Chat`) are wired; the other updates are dropped.

use core::pin::Pin;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use yap_core::store::time::local_day;
use yap_core::store::view::{
    ChannelTreeRow, DmRow, EmojiRow, GifRow, MeView, MemberListRow, MentionRow, MessageRow,
    ServerRow, UnreadView, UploadRow, VoiceMemberRow, VoiceTileRow,
};
use yap_core::ViewUpdate;

use crate::bridge::qobject::{Chat, Nav, Session};
use crate::client;
use crate::models::{self, qobject as m};

/// The rows of each list model (every model `models.rs` defines).
#[derive(Default)]
pub struct Rows {
    pub servers: Vec<ServerRow>,
    pub tree: Vec<ChannelTreeRow>,
    pub messages: Vec<MessageRow>,
    pub roster: Vec<VoiceMemberRow>,
    pub tiles: Vec<VoiceTileRow>,
    pub mentions: Vec<MentionRow>,
    pub dms: Vec<DmRow>,
    pub members: Vec<MemberListRow>,
    pub uploads: Vec<UploadRow>,
    pub gifs: Vec<GifRow>,
    pub emoji: Vec<EmojiRow>,
    pub thread: Vec<MessageRow>,
    pub pins: Vec<MessageRow>,
}

#[derive(Default)]
struct Scalars {
    connected: bool,
    link: (u32, bool),
    server: (String, String),
    channel: (String, String),
    home: bool,
    typing: String,
    unread: UnreadView,
    history: (bool, bool),
    manage_messages: bool,
    read_only: bool,
    page_loading: bool,
}

#[derive(Default)]
pub struct Objects {
    nav: Option<NonNull<Nav>>,
    chat: Option<NonNull<Chat>>,
    servers: Option<NonNull<m::ServerListModel>>,
    tree: Option<NonNull<m::ChannelTreeModel>>,
    messages: Option<NonNull<m::MessageListModel>>,
    roster: Option<NonNull<m::VoiceRosterModel>>,
    tiles: Option<NonNull<m::VoiceTileModel>>,
    mentions: Option<NonNull<m::MentionListModel>>,
    dms: Option<NonNull<m::DmListModel>>,
    members: Option<NonNull<m::MemberListModel>>,
    uploads: Option<NonNull<m::UploadListModel>>,
    gifs: Option<NonNull<m::GifListModel>>,
    emoji: Option<NonNull<m::EmojiGridModel>>,
    thread: Option<NonNull<m::ThreadMessageModel>>,
    pins: Option<NonNull<m::PinnedMessageModel>>,
}

#[derive(Default)]
struct Gui {
    rows: Rows,
    scalars: Scalars,
    objects: Objects,
}

thread_local! {
    static GUI: RefCell<Gui> = RefCell::new(Gui::default());
}

/// A QObject type with a registry slot. `restore` brings a late-created
/// object up to date.
pub(crate) trait Slot: Sized + 'static {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>>;
    fn restore(self: Pin<&mut Self>) {}
}

pub(crate) fn with_rows<T>(f: impl FnOnce(&Rows) -> T) -> T {
    GUI.with(|gui| f(&gui.borrow().rows))
}

pub(crate) fn with_rows_mut<T>(f: impl FnOnce(&mut Rows) -> T) -> T {
    GUI.with(|gui| f(&mut gui.borrow_mut().rows))
}

pub(crate) fn register<T: Slot>(mut this: Pin<&mut T>) {
    // SAFETY: only the address is kept; QObjects never move.
    let ptr = NonNull::from(unsafe { this.as_mut().get_unchecked_mut() });
    GUI.with(|gui| *T::slot(&mut gui.borrow_mut().objects) = Some(ptr));
    this.restore();
}

pub(crate) fn unregister<T: Slot>() {
    let _ = GUI.try_with(|gui| {
        if let Ok(mut gui) = gui.try_borrow_mut() {
            *T::slot(&mut gui.objects) = None;
        }
    });
}

/// The live object of a type, if QML has created it.
pub(crate) fn object<T: Slot>() -> Option<Pin<&'static mut T>> {
    let ptr = GUI.with(|gui| *T::slot(&mut gui.borrow_mut().objects))?;
    // SAFETY: registered by the object's constructor and cleared by its
    // destructor, both on this thread; the object never moves, and the
    // borrow lasts one update.
    Some(unsafe { Pin::new_unchecked(&mut *ptr.as_ptr()) })
}

fn with<T: Slot>(f: impl FnOnce(Pin<&mut T>)) {
    if let Some(object) = object::<T>() {
        f(object);
    }
}

fn scalars(f: impl FnOnce(&mut Scalars)) {
    GUI.with(|gui| f(&mut gui.borrow_mut().scalars));
}

fn q(text: &str) -> QString {
    QString::from(text)
}

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn today() -> i32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
    i32::try_from(local_day(now, client::LOCAL)).unwrap_or_default()
}

impl Slot for Nav {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
        &mut objects.nav
    }

    fn restore(mut self: Pin<&mut Self>) {
        let (connected, link, server, channel, home) = GUI.with(|gui| {
            let s = &gui.borrow().scalars;
            (
                s.connected,
                s.link,
                s.server.clone(),
                s.channel.clone(),
                s.home,
            )
        });
        self.as_mut().set_connected(connected);
        self.as_mut().set_link_attempt(int(i64::from(link.0)));
        self.as_mut().set_link_offline(link.1);
        self.as_mut().set_home(home);
        set_server(self.as_mut(), &server.0, &server.1);
        set_channel(self, &channel.0, &channel.1);
    }
}

impl Slot for Chat {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
        &mut objects.chat
    }

    fn restore(mut self: Pin<&mut Self>) {
        let (typing, unread, history, manage, read_only, page_loading) = GUI.with(|gui| {
            let s = &gui.borrow().scalars;
            (
                s.typing.clone(),
                s.unread.clone(),
                s.history,
                s.manage_messages,
                s.read_only,
                s.page_loading,
            )
        });
        self.as_mut().set_typing_text(q(&typing));
        set_unread(self.as_mut(), &unread);
        self.as_mut().set_older_loading(history.0);
        self.as_mut().set_has_older(history.1);
        self.as_mut().set_can_manage_messages(manage);
        self.as_mut().set_read_only(read_only);
        self.as_mut().set_page_loading(page_loading);
        self.set_today(today());
    }
}

macro_rules! model_slots {
    ($($model:ident => $field:ident),* $(,)?) => {$(
        impl Slot for m::$model {
            fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
                &mut objects.$field
            }
        }
    )*};
}

model_slots!(
    ServerListModel => servers,
    ChannelTreeModel => tree,
    MessageListModel => messages,
    VoiceRosterModel => roster,
    VoiceTileModel => tiles,
    MentionListModel => mentions,
    DmListModel => dms,
    MemberListModel => members,
    UploadListModel => uploads,
    GifListModel => gifs,
    EmojiGridModel => emoji,
    ThreadMessageModel => thread,
    PinnedMessageModel => pins,
);

fn set_unread(mut chat: Pin<&mut Chat>, view: &UnreadView) {
    chat.as_mut().set_unread_count(int(view.count));
    chat.as_mut().set_unread_banner(view.banner);
    chat.set_divider_id(q(&view.divider_id));
}

fn set_server(mut nav: Pin<&mut Nav>, id: &str, name: &str) {
    nav.as_mut().set_active_server_name(q(name));
    nav.set_active_server_id(q(id));
}

fn set_channel(mut nav: Pin<&mut Nav>, id: &str, name: &str) {
    nav.as_mut().set_active_channel_name(q(name));
    nav.set_active_channel_id(q(id));
}

fn set_me(mut session: Pin<&mut Session>, me: &MeView) {
    session.as_mut().set_me_name(q(&me.name));
    session.as_mut().set_me_username(q(&me.username));
    session.as_mut().set_me_avatar(q(&me.avatar));
    session.as_mut().set_me_initials(q(&me.initials));
    session.set_me_id(q(&me.id));
}

/// Applies one drained batch, in order, on the GUI thread.
pub(crate) fn apply(mut session: Pin<&mut Session>, updates: Vec<ViewUpdate>) {
    for update in updates {
        apply_one(session.as_mut(), update);
    }
    with::<Chat>(|chat| chat.set_today(today()));
}

fn apply_one(mut session: Pin<&mut Session>, update: ViewUpdate) {
    match update {
        ViewUpdate::Phase(phase) => session.set_phase(phase.into()),
        ViewUpdate::Status(text) => session.set_status(q(&text)),
        ViewUpdate::DeviceCode { user_code, url } => {
            session.as_mut().set_user_code(q(&user_code));
            session.set_verification_url(q(&url));
        }
        ViewUpdate::Me(me) => set_me(session, &me),
        ViewUpdate::Connected(connected) => {
            scalars(|s| s.connected = connected);
            with::<Nav>(|nav| nav.set_connected(connected));
        }
        ViewUpdate::Link { attempt, offline } => {
            scalars(|s| s.link = (attempt, offline));
            with::<Nav>(|mut nav| {
                nav.as_mut().set_link_attempt(int(i64::from(attempt)));
                nav.set_link_offline(offline);
            });
        }
        ViewUpdate::ActiveServer { id, name } => {
            with::<Nav>(|nav| set_server(nav, &id, &name));
            scalars(|s| s.server = (id, name));
        }
        ViewUpdate::ActiveChannel { id, name } => {
            with::<Nav>(|nav| set_channel(nav, &id, &name));
            scalars(|s| s.channel = (id, name));
        }
        ViewUpdate::Home(home) => {
            with::<Nav>(|nav| nav.set_home(home));
            scalars(|s| s.home = home);
        }
        ViewUpdate::Typing(text) => {
            with::<Chat>(|chat| chat.set_typing_text(q(&text)));
            scalars(|s| s.typing = text);
        }
        ViewUpdate::PageLoading(loading) => {
            with::<Chat>(|chat| chat.set_page_loading(loading));
            scalars(|s| s.page_loading = loading);
        }
        ViewUpdate::Unread(view) => {
            with::<Chat>(|chat| set_unread(chat, &view));
            scalars(|s| s.unread = view);
        }
        ViewUpdate::Notice(text) => with::<Chat>(|chat| chat.notice(q(&text))),
        ViewUpdate::Reland => with::<Chat>(|chat| chat.reland()),
        ViewUpdate::History { loading, more } => {
            with::<Chat>(|mut chat| {
                chat.as_mut().set_has_older(more);
                chat.set_older_loading(loading);
            });
            scalars(|s| s.history = (loading, more));
        }
        ViewUpdate::ManageMessages(manage) => {
            with::<Chat>(|chat| chat.set_can_manage_messages(manage));
            scalars(|s| s.manage_messages = manage);
        }
        ViewUpdate::ReadOnly(read_only) => {
            with::<Chat>(|chat| chat.set_read_only(read_only));
            scalars(|s| s.read_only = read_only);
        }
        ViewUpdate::Servers(diff) => models::apply::<m::ServerListModel>(diff),
        ViewUpdate::Channels(diff) => models::apply::<m::ChannelTreeModel>(diff),
        ViewUpdate::Messages(diff) => models::apply::<m::MessageListModel>(*diff),
        ViewUpdate::Members(diff) => models::apply::<m::MemberListModel>(diff),
        ViewUpdate::Mentions(diff) => models::apply::<m::MentionListModel>(diff),
        ViewUpdate::Dms(diff) => models::apply::<m::DmListModel>(diff),
        // Voice, uploads, GIFs, settings, profile and the other desktop
        // panels are not in the prototype.
        _ => {}
    }
}
