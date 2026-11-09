//! `Search`: the message search panel's side of the core. Everything the
//! panel draws comes as one JSON `state` ([`yap_core::store::SearchView`]);
//! the field reports its text, caret and keys through the invokables.
//! Recent searches are kept in `%LOCALAPPDATA%\Yap.Data\search-recent.json`
//! by scope (the web's localStorage `rm:search-recent:<scope>`), like the
//! emoji picker's prefs: read once, written when the store's list changes.

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
        /// The panel as JSON: `{open, scope, scopeKey, scopeLabel,
        /// placeholder, text, caret, revision, chips, suggestionsOpen,
        /// activeId, groups, idle, status, total, hasMore, loadingMore,
        /// loadMoreFailed, generation, sort, announcement, results,
        /// emptyDescription, filtered, recents}`.
        #[qproperty(QString, state)]
        type Search = super::SearchRust;

        /// The panel opened; `text` is typing handed over (may be empty).
        #[qinvokable]
        fn open(self: &Search, text: &QString, now_ms: f64);

        #[qinvokable]
        fn close(self: &Search);

        /// The field changed: its text and caret (UTF-16).
        #[qinvokable]
        fn edit(self: &Search, text: &QString, caret: i32, now_ms: f64);

        #[qinvokable]
        #[cxx_name = "setCaret"]
        fn set_caret(self: &Search, caret: i32);

        #[qinvokable]
        #[cxx_name = "setFocused"]
        fn set_focused(self: &Search, focused: bool);

        /// Arrow down (1) or up (-1) through the open suggestions.
        #[qinvokable]
        #[cxx_name = "moveActive"]
        fn move_active(self: &Search, delta: i32);

        #[qinvokable]
        fn hover(self: &Search, id: &QString);

        #[qinvokable]
        fn pick(self: &Search, id: &QString, now_ms: f64);

        #[qinvokable]
        #[cxx_name = "removeFilter"]
        fn remove_filter(self: &Search, id: &QString);

        #[qinvokable]
        #[cxx_name = "removeLastFilter"]
        fn remove_last_filter(self: &Search);

        #[qinvokable]
        fn clear(self: &Search);

        #[qinvokable]
        fn submit(self: &Search, now_ms: f64);

        #[qinvokable]
        fn dismiss(self: &Search);

        /// `newest`, `oldest` or `relevant`.
        #[qinvokable]
        #[cxx_name = "setSort"]
        fn set_sort(self: &Search, sort: &QString);

        #[qinvokable]
        #[cxx_name = "loadMore"]
        fn load_more(self: &Search);

        #[qinvokable]
        fn retry(self: &Search);

        /// A result was opened (the panel jumps through `Chat`).
        #[qinvokable]
        fn opened(self: &Search, message_id: &QString);
    }

    impl cxx_qt::Initialize for Search {}
}

use core::pin::Pin;
use std::cell::RefCell;
use std::collections::BTreeMap;

use cxx_qt_lib::QString;
use yap_cache::settings;
use yap_core::search::{decode_recents, RecentSearch, Sort};
use yap_core::store::{Command, SearchCommand, SearchView};

use crate::client;
use crate::gui;

#[derive(Default)]
pub struct SearchRust {
    state: QString,
}

type Recents = BTreeMap<String, Vec<RecentSearch>>;

thread_local! {
    /// The stored recents (loaded on first use) and the last view, GUI
    /// thread only.
    static SEARCH: RefCell<(Option<Recents>, SearchView)> = RefCell::default();
}

fn recents() -> Recents {
    SEARCH.with(|s| {
        s.borrow_mut()
            .0
            .get_or_insert_with(|| {
                settings::app_dir()
                    .map(|dir| settings::load_search_recents(&dir))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(key, list)| (key, decode_recents(list)))
                    .collect()
            })
            .clone()
    })
}

fn save(recents: &Recents) {
    let Some(dir) = settings::app_dir() else {
        return;
    };
    let stored = recents
        .iter()
        .map(|(key, list)| {
            let values = list
                .iter()
                .filter_map(|r| serde_json::to_value(r).ok())
                .collect();
            (key.clone(), values)
        })
        .collect();
    if let Err(error) = settings::save_search_recents(&dir, &stored) {
        eprintln!("[search] {error}");
    }
}

fn send(command: SearchCommand) {
    client::command(Command::Search(command));
}

fn text(value: &QString) -> String {
    value.to_string()
}

fn ms(now_ms: f64) -> i64 {
    now_ms as i64
}

fn units(n: i32) -> usize {
    usize::try_from(n).unwrap_or_default()
}

fn apply(object: Pin<&mut qobject::Search>, view: &SearchView) {
    let state = serde_json::to_string(view).unwrap_or_else(|_| "{}".into());
    object.set_state(QString::from(&state));
}

/// The store's search view changed: show it, and store this scope's
/// recents when they moved.
pub(crate) fn on_view(view: SearchView) {
    if !view.scope_key.is_empty() {
        let mut all = recents();
        let stored = all.get(&view.scope_key).map_or(&[][..], Vec::as_slice);
        if stored != view.recents.as_slice() {
            all.insert(view.scope_key.clone(), view.recents.clone());
            save(&all);
            SEARCH.with(|s| s.borrow_mut().0 = Some(all));
        }
    }
    if let Some(object) = gui::object::<qobject::Search>() {
        apply(object, &view);
    }
    SEARCH.with(|s| s.borrow_mut().1 = view);
}

impl gui::Slot for qobject::Search {
    fn slot(objects: &mut gui::Objects) -> &mut Option<std::ptr::NonNull<Self>> {
        &mut objects.search
    }

    fn restore(self: Pin<&mut Self>) {
        let view = SEARCH.with(|s| s.borrow().1.clone());
        apply(self, &view);
    }
}

impl cxx_qt::Initialize for qobject::Search {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for SearchRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Search>();
    }
}

impl qobject::Search {
    pub fn open(&self, text_: &QString, now_ms: f64) {
        send(SearchCommand::Open {
            text: text(text_),
            now_ms: ms(now_ms),
            recents: recents(),
        });
    }

    pub fn close(&self) {
        send(SearchCommand::Close);
    }

    pub fn edit(&self, value: &QString, caret: i32, now_ms: f64) {
        send(SearchCommand::Edit {
            text: text(value),
            caret: units(caret),
            now_ms: ms(now_ms),
        });
    }

    pub fn set_caret(&self, caret: i32) {
        send(SearchCommand::Caret(units(caret)));
    }

    pub fn set_focused(&self, focused: bool) {
        send(SearchCommand::Focus(focused));
    }

    pub fn move_active(&self, delta: i32) {
        send(SearchCommand::Move(delta));
    }

    pub fn hover(&self, id: &QString) {
        send(SearchCommand::Hover(text(id)));
    }

    pub fn pick(&self, id: &QString, now_ms: f64) {
        send(SearchCommand::Pick {
            id: text(id),
            now_ms: ms(now_ms),
        });
    }

    pub fn remove_filter(&self, id: &QString) {
        send(SearchCommand::RemoveFilter(text(id)));
    }

    pub fn remove_last_filter(&self) {
        send(SearchCommand::RemoveLastFilter);
    }

    pub fn clear(&self) {
        send(SearchCommand::Clear);
    }

    pub fn submit(&self, now_ms: f64) {
        send(SearchCommand::Submit { now_ms: ms(now_ms) });
    }

    pub fn dismiss(&self) {
        send(SearchCommand::Dismiss);
    }

    pub fn set_sort(&self, sort: &QString) {
        if let Some(sort) = Sort::parse(&text(sort)) {
            send(SearchCommand::SetSort(sort));
        }
    }

    pub fn load_more(&self) {
        send(SearchCommand::LoadMore);
    }

    pub fn retry(&self) {
        send(SearchCommand::Retry);
    }

    pub fn opened(&self, message_id: &QString) {
        send(SearchCommand::Opened(text(message_id)));
    }
}
