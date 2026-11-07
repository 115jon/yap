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
use cxx_qt_lib::{QList, QStringList};
use yap_core::store::time::local_day;
use yap_core::store::view::{
    AdminView, ChannelTreeRow, DmRow, DmView, EmojiRow, GifRow, GifStatusView, MeView,
    MemberListRow, MentionRow, MessageRow, ProfileView, ServerRow, SettingsView, UnreadView,
    UploadRow, VoiceMemberRow, VoicePanelView, VoiceTileRow, VoiceView,
};
use yap_core::store::PanelsView;
use yap_core::ViewUpdate;

use crate::bridge::qobject::{Chat, Nav, Profile, Session, Voice};
use crate::client;
use crate::models::{self, qobject as m};

/// The rows of each list model.
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
    /// The open thread and the pins panel.
    pub thread: Vec<MessageRow>,
    pub pins: Vec<MessageRow>,
}

/// The last scalar values for the singletons that may not exist yet.
#[derive(Default)]
struct Scalars {
    connected: bool,
    server: (String, String),
    channel: (String, String),
    typing: String,
    unread: UnreadView,
    /// (loading, more) of older pages.
    history: (bool, bool),
    manage_messages: bool,
    read_only: bool,
    voice: VoiceView,
    panel: VoicePanelView,
    home: bool,
    dm: DmView,
    dm_unread: i64,
    profile: ProfileView,
    /// (ready, busy) uploads of the open channel.
    uploads: (i64, i64),
    gifs: GifStatusView,
    settings: SettingsView,
    admin: AdminView,
    panels: PanelsView,
    /// (loading, more) of newer pages (a detached list).
    newer: (bool, bool),
    friends: yap_core::store::FriendsView,
    apps: yap_core::store::apps::AppsView,
    shop: yap_core::store::shop::ShopView,
    /// The saved favourites' `provider:id`, as JSON.
    gif_favorites: String,
    extras: yap_core::store::extras::ExtrasView,
    calls: yap_core::store::calls::CallsView,
}

/// The live QObjects. Each registers from its constructor and clears its
/// slot from its destructor, both on the GUI thread.
#[derive(Default)]
pub struct Objects {
    pub(crate) desktop: Option<NonNull<crate::desktop::qobject::Desktop>>,
    pub(crate) settings: Option<NonNull<crate::settings::qobject::Settings>>,
    pub(crate) admin: Option<NonNull<crate::admin::qobject::Admin>>,
    pub(crate) panels: Option<NonNull<crate::panels::qobject::Panels>>,
    pub(crate) search: Option<NonNull<crate::search::qobject::Search>>,
    pub(crate) friends: Option<NonNull<crate::friends::qobject::Friends>>,
    pub(crate) shop: Option<NonNull<crate::shop::qobject::Shop>>,
    pub(crate) invites: Option<NonNull<crate::invites::qobject::Invites>>,
    pub(crate) apps: Option<NonNull<crate::apps::qobject::Apps>>,
    pub(crate) extras: Option<NonNull<crate::extras::qobject::Extras>>,
    pub(crate) calls: Option<NonNull<crate::calls::qobject::Calls>>,
    pub(crate) command_menu: Option<NonNull<crate::command_menu::qobject::CommandMenu>>,
    pub(crate) custom_sounds: Option<NonNull<crate::custom_sounds::qobject::CustomSounds>>,
    nav: Option<NonNull<Nav>>,
    chat: Option<NonNull<Chat>>,
    voice: Option<NonNull<Voice>>,
    profile: Option<NonNull<Profile>>,
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

/// Keeps the settings view for a `Settings` object QML creates later.
pub(crate) fn keep_settings(view: SettingsView) {
    scalars(|s| s.settings = view);
}

pub(crate) fn settings_view() -> SettingsView {
    GUI.with(|gui| gui.borrow().scalars.settings.clone())
}

/// Keeps the admin view for an `Admin` object QML creates later.
pub(crate) fn keep_admin(view: AdminView) {
    scalars(|s| s.admin = view);
}

pub(crate) fn admin_view() -> AdminView {
    GUI.with(|gui| gui.borrow().scalars.admin.clone())
}

/// Keeps the panels' view for a `Panels` object QML creates later.
pub(crate) fn keep_panels(view: PanelsView) {
    scalars(|s| s.panels = view);
}

pub(crate) fn panels_view() -> PanelsView {
    GUI.with(|gui| gui.borrow().scalars.panels.clone())
}

/// What a message link needs: the open server (`None` at Home or in a
/// DM) and the open channel.
pub(crate) fn link_route() -> (Option<String>, String) {
    GUI.with(|gui| {
        let s = &gui.borrow().scalars;
        let dm = s.home || !s.dm.user_id.is_empty();
        let server = Some(s.server.0.clone()).filter(|id| !dm && !id.is_empty());
        (server, s.channel.0.clone())
    })
}

/// Keeps the friends view for a `Friends` object QML creates later.
/// Keeps the shop view for a `Shop` object QML creates later.
pub(crate) fn keep_shop(view: yap_core::store::shop::ShopView) {
    scalars(|s| s.shop = view);
}

pub(crate) fn shop_view() -> yap_core::store::shop::ShopView {
    GUI.with(|gui| gui.borrow().scalars.shop.clone())
}

pub(crate) fn keep_friends(view: yap_core::store::FriendsView) {
    scalars(|s| s.friends = view);
}

pub(crate) fn friends_view() -> yap_core::store::FriendsView {
    GUI.with(|gui| gui.borrow().scalars.friends.clone())
}

/// Keeps the apps view for an `Apps` object QML creates later.
pub(crate) fn keep_apps(view: yap_core::store::apps::AppsView) {
    scalars(|s| s.apps = view);
}

pub(crate) fn apps_view() -> yap_core::store::apps::AppsView {
    GUI.with(|gui| gui.borrow().scalars.apps.clone())
}

/// Keeps the extras view for an `Extras` object QML creates later.
pub(crate) fn keep_extras(view: yap_core::store::extras::ExtrasView) {
    scalars(|s| s.extras = view);
}

pub(crate) fn extras_view() -> yap_core::store::extras::ExtrasView {
    GUI.with(|gui| gui.borrow().scalars.extras.clone())
}

/// Keeps the calls view for a `Calls` object QML creates later.
pub(crate) fn keep_calls(view: yap_core::store::calls::CallsView) {
    scalars(|s| s.calls = view);
}

pub(crate) fn calls_view() -> yap_core::store::calls::CallsView {
    GUI.with(|gui| gui.borrow().scalars.calls.clone())
}

/// Where the user is, for the quick switcher: the open server (empty at
/// Home), the open channel, and whether Home shows.
pub(crate) fn place() -> (String, String, bool) {
    GUI.with(|gui| {
        let s = &gui.borrow().scalars;
        (s.server.0.clone(), s.channel.0.clone(), s.home)
    })
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
        let (connected, server, channel, panel, home, dm, unread) = GUI.with(|gui| {
            let s = &gui.borrow().scalars;
            (
                s.connected,
                s.server.clone(),
                s.channel.clone(),
                s.panel.clone(),
                s.home,
                s.dm.clone(),
                s.dm_unread,
            )
        });
        self.as_mut().set_connected(connected);
        self.as_mut().set_home(home);
        self.as_mut().set_dm_unread(int(unread));
        set_dm(self.as_mut(), &dm);
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
        let (typing, unread, history, manage, read_only, uploads, gifs, newer, favorites) = GUI
            .with(|gui| {
                let s = &gui.borrow().scalars;
                (
                    s.typing.clone(),
                    s.unread.clone(),
                    s.history,
                    s.manage_messages,
                    s.read_only,
                    s.uploads,
                    s.gifs.clone(),
                    s.newer,
                    s.gif_favorites.clone(),
                )
            });
        self.as_mut().set_gif_favorites(q(if favorites.is_empty() {
            "[]"
        } else {
            &favorites
        }));
        self.as_mut().set_newer_loading(newer.0);
        self.as_mut().set_has_newer(newer.1);
        set_uploads(self.as_mut(), uploads);
        set_gifs(self.as_mut(), &gifs);
        self.as_mut().set_typing_text(q(&typing));
        set_unread(self.as_mut(), &unread);
        self.as_mut().set_older_loading(history.0);
        self.as_mut().set_has_older(history.1);
        self.as_mut().set_can_manage_messages(manage);
        self.as_mut().set_read_only(read_only);
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

impl Slot for Profile {
    fn slot(objects: &mut Objects) -> &mut Option<NonNull<Self>> {
        &mut objects.profile
    }

    fn restore(self: Pin<&mut Self>) {
        let profile = GUI.with(|gui| gui.borrow().scalars.profile.clone());
        set_profile(self, &profile);
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

fn int(n: i64) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

fn string_list(items: &[String]) -> QStringList {
    let mut list = QList::<QString>::default();
    for item in items {
        list.append(q(item));
    }
    QStringList::from(&list)
}

fn set_dm(mut nav: Pin<&mut Nav>, dm: &DmView) {
    nav.as_mut().set_dm_username(q(&dm.username));
    nav.as_mut().set_dm_avatar(q(&dm.avatar));
    nav.as_mut().set_dm_deco(q(&dm.deco));
    nav.set_dm_user_id(q(&dm.user_id));
}

fn set_profile(mut profile: Pin<&mut Profile>, view: &ProfileView) {
    profile.as_mut().set_name(q(&view.name));
    profile.as_mut().set_username(q(&view.username));
    profile.as_mut().set_avatar(q(&view.avatar));
    profile.as_mut().set_initials(q(&view.initials));
    profile.as_mut().set_presence(q(&view.presence));
    profile.as_mut().set_custom_status(q(&view.custom_status));
    profile.as_mut().set_pronouns(q(&view.pronouns));
    profile.as_mut().set_bio(q(&view.bio));
    profile.as_mut().set_banner(q(&view.banner));
    profile.as_mut().set_banner_color(q(&view.banner_color));
    profile
        .as_mut()
        .set_role_colors(string_list(&view.role_colors));
    profile
        .as_mut()
        .set_role_names(string_list(&view.role_names));
    profile.as_mut().set_me(view.me);
    profile.as_mut().set_deco(q(&view.deco));
    profile.as_mut().set_effect(q(&view.effect));
    profile.as_mut().set_frame(q(&view.frame));
    profile.as_mut().set_name_style(q(&view.name_style));
    profile.as_mut().set_platforms(q(&view.platforms));
    profile.as_mut().set_mutual_servers(q(&view.mutual_servers));
    profile.as_mut().set_mutual_friends(q(&view.mutual_friends));
    profile.as_mut().set_member_since(q(&view.member_since));
    profile.as_mut().set_load_error(q(&view.load_error));
    profile.set_user_id(q(&view.user_id));
}

fn set_uploads(mut chat: Pin<&mut Chat>, (ready, busy): (i64, i64)) {
    chat.as_mut().set_uploads_ready(int(ready));
    chat.set_uploads_busy(int(busy));
}

fn set_gifs(mut chat: Pin<&mut Chat>, view: &GifStatusView) {
    chat.as_mut().set_gif_more(view.more);
    chat.as_mut().set_gif_error(q(&view.error));
    chat.as_mut().set_gif_units0(int(view.units[0]));
    chat.as_mut().set_gif_units1(int(view.units[1]));
    chat.as_mut().set_gif_count0(int(view.counts[0]));
    chat.as_mut().set_gif_count1(int(view.counts[1]));
    chat.as_mut().set_gif_browse(q(&view.browse));
    chat.set_gif_loading(view.loading);
}

fn set_unread(mut chat: Pin<&mut Chat>, view: &UnreadView) {
    let int = |n: i64| i32::try_from(n).unwrap_or(i32::MAX);
    chat.as_mut().set_unread_count(int(view.count));
    chat.as_mut().set_unread_since_time(q(&view.since_time));
    chat.as_mut().set_unread_since_date(q(&view.since_date));
    chat.as_mut().set_unread_since_day(int(view.since_day));
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

fn set_panel(mut nav: Pin<&mut Nav>, panel: &VoicePanelView) {
    nav.as_mut().set_voice_panel_name(q(&panel.name));
    nav.as_mut()
        .set_voice_panel_count(i32::try_from(panel.count).unwrap_or(i32::MAX));
    nav.as_mut().set_voice_panel_chat(panel.chat);
    nav.set_voice_panel_id(q(&panel.id));
}

fn set_voice(mut voice: Pin<&mut Voice>, view: &VoiceView) {
    voice.as_mut().set_status(q(&view.status));
    voice.as_mut().set_channel_name(q(&view.channel_name));
    voice.as_mut().set_live(view.live);
    voice.as_mut().set_muted(view.muted);
    voice.as_mut().set_deafened(view.deafened);
    voice.as_mut().set_speaking(view.speaking);
    voice.as_mut().set_error(q(&view.error));
    voice.as_mut().set_camera_wanted(view.camera_wanted);
    voice.as_mut().set_camera_on(view.camera);
    voice.as_mut().set_camera_error(q(&view.camera_error));
    voice.as_mut().set_share_pending(view.share_pending);
    voice.as_mut().set_share_source_id(q(&view.share.source_id));
    voice
        .as_mut()
        .set_share_source_name(q(&view.share.source_name));
    voice.as_mut().set_share_window(view.share.window);
    voice.as_mut().set_share_quality(q(&view.share.quality));
    voice.as_mut().set_share_audio(view.share.audio);
    voice.as_mut().set_share_error(q(&view.share_error));
    voice.as_mut().set_sharing(view.sharing);
    voice.as_mut().set_spatial(q(&view.spatial));
    voice.set_channel_id(q(&view.channel_id));
}

fn set_me(mut session: Pin<&mut Session>, me: &MeView) {
    session.as_mut().set_me_name(q(&me.name));
    session.as_mut().set_me_username(q(&me.username));
    session.as_mut().set_me_avatar(q(&me.avatar));
    session.as_mut().set_me_initials(q(&me.initials));
    session.as_mut().set_me_status(q(&me.status));
    session.as_mut().set_me_custom_status(q(&me.custom_status));
    session.as_mut().set_me_deco(q(&me.deco));
    session.as_mut().set_me_plate(q(&me.plate));
    session.as_mut().set_me_effect(q(&me.effect));
    session.as_mut().set_me_frame(q(&me.frame));
    session.as_mut().set_me_name_style(q(&me.name_style));
    session.as_mut().set_me_display(q(&me.display));
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
        ViewUpdate::TypingPing => with::<Chat>(|chat| chat.typing_pinged()),
        ViewUpdate::PageLoading(loading) => with::<Chat>(|chat| chat.set_page_loading(loading)),
        ViewUpdate::MentionHover(json) => {
            with::<Chat>(|chat| chat.set_hovered_mention(q(&json)));
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
        ViewUpdate::Messages(diff) => models::apply::<m::MessageListModel>(*diff),
        ViewUpdate::Roster(diff) => models::apply::<m::VoiceRosterModel>(diff),
        ViewUpdate::Tiles(diff) => models::apply::<m::VoiceTileModel>(diff),
        ViewUpdate::Mentions(diff) => models::apply::<m::MentionListModel>(diff),
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
        ViewUpdate::Home(home) => {
            with::<Nav>(|nav| nav.set_home(home));
            scalars(|s| s.home = home);
        }
        ViewUpdate::ActiveDm(dm) => {
            with::<Nav>(|nav| set_dm(nav, &dm));
            scalars(|s| s.dm = dm);
        }
        ViewUpdate::Recent(target) => crate::command_menu::record_target(target),
        ViewUpdate::DmUnread(count) => {
            with::<Nav>(|nav| nav.set_dm_unread(int(count)));
            scalars(|s| s.dm_unread = count);
        }
        ViewUpdate::Dms(diff) => models::apply::<m::DmListModel>(diff),
        ViewUpdate::Members(diff) => models::apply::<m::MemberListModel>(diff),
        ViewUpdate::Profile(view) => {
            with::<Profile>(|profile| set_profile(profile, &view));
            scalars(|s| s.profile = *view);
        }
        ViewUpdate::Settings(view) => crate::settings::on_view(*view),
        ViewUpdate::Admin(view) => crate::admin::on_view(*view),
        ViewUpdate::Search(view) => crate::search::on_view(*view),
        ViewUpdate::Friends(view) => crate::friends::on_view(*view),
        ViewUpdate::Shop(view) => crate::shop::on_view(*view),
        ViewUpdate::GifFavorites(keys) => {
            let text = serde_json::to_string(&keys).unwrap_or_else(|_| "[]".into());
            with::<Chat>(|chat| chat.set_gif_favorites(q(&text)));
            scalars(|s| s.gif_favorites = text);
        }
        ViewUpdate::Apps(view) => crate::apps::on_view(*view),
        ViewUpdate::CameraBackgrounds(view) => crate::audio::on_backgrounds(*view),
        ViewUpdate::Extras(view) => crate::extras::on_view(*view),
        ViewUpdate::Invites(view) => crate::invites::on_view(*view),
        ViewUpdate::Toast(toast) => crate::desktop::on_toast(&toast),
        ViewUpdate::Sound(sound) => crate::sound::play(sound),
        ViewUpdate::StopSound(sound) => crate::sound::stop(sound),
        ViewUpdate::Calls(view) => crate::calls::on_view(*view),
        ViewUpdate::Badge(badge) => crate::desktop::on_badge(badge),
        ViewUpdate::Uploads(diff) => models::apply::<m::UploadListModel>(diff),
        ViewUpdate::UploadCounts { ready, busy } => {
            with::<Chat>(|chat| set_uploads(chat, (ready, busy)));
            scalars(|s| s.uploads = (ready, busy));
        }
        ViewUpdate::Gifs(diff) => models::apply::<m::GifListModel>(diff),
        ViewUpdate::GifStatus(view) => {
            with::<Chat>(|chat| set_gifs(chat, &view));
            scalars(|s| s.gifs = view);
        }
        ViewUpdate::MediaAuth(token) => crate::media::set_token(token),
        ViewUpdate::Saved { path, open } => {
            with::<Chat>(|chat| chat.saved(q(&path), open));
        }
        ViewUpdate::ThreadMessages(diff) => models::apply::<m::ThreadMessageModel>(*diff),
        ViewUpdate::PinnedMessages(diff) => models::apply::<m::PinnedMessageModel>(*diff),
        ViewUpdate::Panels(view) => crate::panels::on_view(*view),
        ViewUpdate::JumpTo(id) => with::<Chat>(|chat| chat.jumped(q(&id))),
        ViewUpdate::ScrollRestore(id) => with::<Chat>(|chat| chat.set_restore_id(q(&id))),
        ViewUpdate::MentionNames(names) => with::<Chat>(|chat| chat.set_mention_names(q(&names))),
        ViewUpdate::Newer { loading, more } => {
            with::<Chat>(|mut chat| {
                chat.as_mut().set_has_newer(more);
                chat.set_newer_loading(loading);
            });
            scalars(|s| s.newer = (loading, more));
        }
    }
}
