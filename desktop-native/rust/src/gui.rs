//! GUI-thread state: the rows every list model shows, the last scalar
//! values (for singletons QML creates late), and the live QObjects. Only the
//! GUI thread touches it (a `thread_local`), so `data()` never locks.
//!
//! Borrows of the state are short and never held across a call into Qt:
//! Qt calls back (`data()`, bindings, handlers) synchronously.

use core::pin::Pin;
use std::cell::RefCell;
use std::ptr::NonNull;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use yap_core::store::time::local_day;
use yap_core::store::view::{
    ChannelTreeRow, MeView, MessageRow, ServerRow, VoiceMemberRow, VoicePanelView, VoiceView,
};
use yap_core::ViewUpdate;

use crate::bridge::qobject::{Chat, Nav, Session, Voice};
use crate::client;
use crate::models::{self, qobject as m};

/// The rows of each list model.
#[derive(Default)]
pub struct Rows {
    pub servers: Vec<ServerRow>,
    pub tree: Vec<ChannelTreeRow>,
    pub messages: Vec<MessageRow>,
    pub roster: Vec<VoiceMemberRow>,
}

/// The last scalar values for the singletons that may not exist yet.
#[derive(Default)]
struct Scalars {
    connected: bool,
    server: (String, String),
    channel: (String, String),
    typing: String,
    voice: VoiceView,
    panel: VoicePanelView,
}

/// The live QObjects. Each registers from its constructor and clears its
/// slot from its destructor, both on the GUI thread.
#[derive(Default)]
pub struct Objects {
    nav: Option<NonNull<Nav>>,
    chat: Option<NonNull<Chat>>,
    voice: Option<NonNull<Voice>>,
    servers: Option<NonNull<m::ServerListModel>>,
    tree: Option<NonNull<m::ChannelTreeModel>>,
    messages: Option<NonNull<m::MessageListModel>>,
    roster: Option<NonNull<m::VoiceRosterModel>>,
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
    // At thread exit the state may already be gone; nothing to clear then.
    let _ = GUI.try_with(|gui| {
        if let Ok(mut gui) = gui.try_borrow_mut() {
            *T::slot(&mut gui.objects) = None;
        }
    });
}

/// The live object of a type, if QML has created it.
pub(crate) fn object<T: Slot>() -> Option<Pin<&'static mut T>> {
    let ptr = GUI.with(|gui| *T::slot(&mut gui.borrow_mut().objects))?;
    // SAFETY: the pointer was registered by the object's constructor and is
    // cleared by its destructor, both on this thread, which is the only one
    // that calls this. The object never moves. The `'static` borrow is only
    // used for the duration of one update.
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
        let (connected, server, channel, panel) = GUI.with(|gui| {
            let s = &gui.borrow().scalars;
            (
                s.connected,
                s.server.clone(),
                s.channel.clone(),
                s.panel.clone(),
            )
        });
        self.as_mut().set_connected(connected);
        set_server(self.as_mut(), &server.0, &server.1);
        set_panel(self.as_mut(), &panel);
        set_channel(self, &channel.0, &channel.1);
    }
}

impl Slot for Chat {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
        &mut objects.chat
    }

    fn restore(mut self: Pin<&mut Self>) {
        let typing = GUI.with(|gui| gui.borrow().scalars.typing.clone());
        self.as_mut().set_typing_text(q(&typing));
        self.set_today(today());
    }
}

impl Slot for Voice {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
        &mut objects.voice
    }

    fn restore(self: Pin<&mut Self>) {
        let voice = GUI.with(|gui| gui.borrow().scalars.voice.clone());
        set_voice(self, &voice);
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
);

fn set_server(mut nav: Pin<&mut Nav>, id: &str, name: &str) {
    nav.as_mut().set_active_server_name(q(name));
    nav.set_active_server_id(q(id));
}

fn set_channel(mut nav: Pin<&mut Nav>, id: &str, name: &str) {
    nav.as_mut().set_active_channel_name(q(name));
    nav.set_active_channel_id(q(id));
}

fn set_panel(mut nav: Pin<&mut Nav>, panel: &VoicePanelView) {
    nav.as_mut().set_voice_panel_name(q(&panel.name));
    nav.as_mut()
        .set_voice_panel_count(i32::try_from(panel.count).unwrap_or(i32::MAX));
    nav.set_voice_panel_id(q(&panel.id));
}

fn set_voice(mut voice: Pin<&mut Voice>, view: &VoiceView) {
    voice.as_mut().set_status(q(&view.status));
    voice.as_mut().set_channel_name(q(&view.channel_name));
    voice.as_mut().set_live(view.live);
    voice.as_mut().set_muted(view.muted);
    voice.as_mut().set_deafened(view.deafened);
    voice.as_mut().set_error(q(&view.error));
    voice.set_channel_id(q(&view.channel_id));
}

fn set_me(mut session: Pin<&mut Session>, me: &MeView) {
    session.as_mut().set_me_name(q(&me.name));
    session.as_mut().set_me_username(q(&me.username));
    session.as_mut().set_me_avatar(q(&me.avatar));
    session.as_mut().set_me_initials(q(&me.initials));
    session.as_mut().set_me_status(q(&me.status));
    session.as_mut().set_me_custom_status(q(&me.custom_status));
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
        ViewUpdate::ActiveServer { id, name } => {
            with::<Nav>(|nav| set_server(nav, &id, &name));
            scalars(|s| s.server = (id, name));
        }
        ViewUpdate::ActiveChannel { id, name } => {
            with::<Nav>(|nav| set_channel(nav, &id, &name));
            scalars(|s| s.channel = (id, name));
        }
        ViewUpdate::Typing(text) => {
            with::<Chat>(|chat| chat.set_typing_text(q(&text)));
            scalars(|s| s.typing = text);
        }
        ViewUpdate::Voice(view) => {
            with::<Voice>(|voice| set_voice(voice, &view));
            scalars(|s| s.voice = view);
        }
        ViewUpdate::VoicePanel(panel) => {
            with::<Nav>(|nav| set_panel(nav, &panel));
            scalars(|s| s.panel = panel);
        }
        ViewUpdate::Servers(diff) => models::apply::<m::ServerListModel>(diff),
        ViewUpdate::Channels(diff) => models::apply::<m::ChannelTreeModel>(diff),
        ViewUpdate::Messages(diff) => models::apply::<m::MessageListModel>(diff),
        ViewUpdate::Roster(diff) => models::apply::<m::VoiceRosterModel>(diff),
    }
}
