//! `Shop`: the collectibles shop page, its item detail and the profile
//! editor's pickers ([`yap_core::store::shop`]), as four JSON strings so a
//! detail change never re-parses the page. `Embeds`: a TikTok or Instagram
//! card asking for its media when it shows.

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
        /// `{loaded, loading, error, freshness, counts: {<kind>: n}, tab,
        /// query, collection: {id, name} | null, busy, actionError, picker,
        /// pickerTitle, pickerQuery}`.
        #[qproperty(QString, state)]
        /// The page's rows: `hero`, `heading`, `collections`, `rail`,
        /// `grid`, `status`, `loading` or `empty` (see store/shop.rs).
        #[qproperty(QString, rows)]
        /// The open picker: its copy, sections, draft and preview, or
        /// `null` (store/shop.rs `picker_json`).
        #[qproperty(QString, picker)]
        /// The open detail, or `null`.
        #[qproperty(QString, detail)]
        type Shop = super::ShopRust;

        /// The shop page or a picker opened.
        #[qinvokable]
        fn open(self: &Shop);
        #[qinvokable]
        #[cxx_name = "setTab"]
        fn set_tab(self: &Shop, tab: &QString);
        #[qinvokable]
        #[cxx_name = "setQuery"]
        fn set_query(self: &Shop, query: &QString);
        /// A collection's page; empty for back.
        #[qinvokable]
        #[cxx_name = "openCollection"]
        fn open_collection(self: &Shop, id: &QString);
        #[qinvokable]
        #[cxx_name = "setColumns"]
        fn set_columns(self: &Shop, columns: i32);
        /// An item's detail by its family's base id; empty closes it.
        #[qinvokable]
        #[cxx_name = "openDetail"]
        fn open_detail(self: &Shop, id: &QString);
        #[qinvokable]
        #[cxx_name = "selectVariant"]
        fn select_variant(self: &Shop, id: &QString);
        #[qinvokable]
        fn equip(self: &Shop, id: &QString);
        /// Empties a slot (`avatar_decoration`, `profile_effect`,
        /// `nameplate`, `profile_frame`).
        #[qinvokable]
        fn remove(self: &Shop, kind: &QString);
        /// The profile editor's picker for a kind; empty closes it.
        #[qinvokable]
        #[cxx_name = "openPicker"]
        fn open_picker(self: &Shop, kind: &QString);
        #[qinvokable]
        #[cxx_name = "setPickerQuery"]
        fn set_picker_query(self: &Shop, query: &QString);
        /// The picker's draft: an item id, or `__none`.
        #[qinvokable]
        #[cxx_name = "choosePicker"]
        fn choose_picker(self: &Shop, id: &QString);
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        type Embeds = super::EmbedsRust;

        /// A `tiktok` or `instagram` card showed.
        #[qinvokable]
        fn hydrate(self: &Embeds, provider: &QString, url: &QString);
        /// "Try again" on a card whose lookup failed.
        #[qinvokable]
        fn retry(self: &Embeds, provider: &QString, url: &QString);
    }

    impl cxx_qt::Initialize for Shop {}
}

use core::pin::Pin;
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt_lib::QString;
use yap_core::store::embeds::{EmbedsCommand, Provider};
use yap_core::store::shop::{ShopCommand, ShopView};
use yap_core::store::Command;

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct ShopRust {
    state: QString,
    rows: QString,
    picker: QString,
    detail: QString,
}

#[derive(Default)]
pub struct EmbedsRust;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn send(command: ShopCommand) {
    client::command(Command::Shop(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn optional(value: &QString) -> Option<String> {
    Some(value.to_string()).filter(|v| !v.is_empty())
}

fn apply(mut object: Pin<&mut qobject::Shop>, view: &ShopView) {
    // Each setter only notifies on a change, so an unchanged page is not
    // parsed again by QML.
    object.as_mut().set_state(QString::from(&view.state));
    object.as_mut().set_rows(QString::from(&view.rows));
    object.as_mut().set_picker(QString::from(&view.picker));
    object.set_detail(QString::from(&view.detail));
}

/// The store's shop view changed.
pub fn on_view(view: ShopView) {
    if let Some(object) = gui::object::<qobject::Shop>() {
        apply(object, &view);
    }
    gui::keep_shop(view);
}

impl gui::Slot for qobject::Shop {
    crate::slot_cell!(qobject::Shop);

    fn restore(self: Pin<&mut Self>) {
        let view = gui::shop_view();
        if view.state.is_empty() {
            apply(self, &ShopView::initial());
        } else {
            apply(self, &view);
        }
    }
}

impl cxx_qt::Initialize for qobject::Shop {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for ShopRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Shop>();
    }
}

impl qobject::Shop {
    pub fn open(&self) {
        send(ShopCommand::Open { now_ms: now_ms() });
    }

    pub fn set_tab(&self, tab: &QString) {
        send(ShopCommand::Tab(text(tab)));
    }

    pub fn set_query(&self, query: &QString) {
        send(ShopCommand::Query(text(query)));
    }

    pub fn open_collection(&self, id: &QString) {
        send(ShopCommand::Collection(optional(id)));
    }

    pub fn set_columns(&self, columns: i32) {
        send(ShopCommand::Columns(u32::try_from(columns).unwrap_or(2)));
    }

    pub fn open_detail(&self, id: &QString) {
        send(ShopCommand::Detail(optional(id)));
    }

    pub fn select_variant(&self, id: &QString) {
        send(ShopCommand::Variant(text(id)));
    }

    pub fn equip(&self, id: &QString) {
        send(ShopCommand::Equip(text(id)));
    }

    pub fn remove(&self, kind: &QString) {
        send(ShopCommand::Remove(text(kind)));
    }

    pub fn open_picker(&self, kind: &QString) {
        if optional(kind).is_some() {
            send(ShopCommand::Open { now_ms: now_ms() });
        }
        send(ShopCommand::Picker(optional(kind)));
    }

    pub fn set_picker_query(&self, query: &QString) {
        send(ShopCommand::PickerQuery(text(query)));
    }

    pub fn choose_picker(&self, id: &QString) {
        send(ShopCommand::PickerChoose(text(id)));
    }
}

impl qobject::Embeds {
    pub fn hydrate(&self, provider: &QString, url: &QString) {
        if let Some(provider) = Provider::parse(&text(provider)) {
            client::command(Command::Embeds(EmbedsCommand::Hydrate {
                provider,
                url: text(url),
                now_ms: now_ms(),
            }));
        }
    }

    pub fn retry(&self, provider: &QString, url: &QString) {
        if let Some(provider) = Provider::parse(&text(provider)) {
            client::command(Command::Embeds(EmbedsCommand::Retry {
                provider,
                url: text(url),
                now_ms: now_ms(),
            }));
        }
    }
}
