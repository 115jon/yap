//! `Emoji`: the emoji picker's state on the GUI thread (the query, the
//! skin tone, the recents) and its rows in `EmojiGridModel`. The grid is
//! pure data (`yap_core::picker`), so it never goes through the core.

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
        /// 0 default, 1 light .. 5 dark.
        #[qproperty(i32, skin_tone, cxx_name = "skinTone")]
        /// The hover bar's three quick reactions, JSON `[{e, u, s, n}]`.
        #[qproperty(QString, quick)]
        type Emoji = super::EmojiRust;

        /// Fills `EmojiGridModel`: results for `query`, else every section.
        #[qinvokable]
        fn search(self: Pin<&mut Emoji>, query: &QString);

        #[qinvokable]
        #[cxx_name = "setTone"]
        fn set_tone(self: Pin<&mut Emoji>, tone: i32);

        /// A pick or a reaction add: moves `value` to the front of the recents.
        #[qinvokable]
        fn remember(self: Pin<&mut Emoji>, value: &QString);

        /// The grid row where a section starts, or -1.
        #[qinvokable]
        #[cxx_name = "sectionRow"]
        fn section_row(self: &Emoji, section: &QString) -> i32;
    }

    impl cxx_qt::Initialize for Emoji {}
}

use core::pin::Pin;
use std::cell::RefCell;

use cxx_qt_lib::QString;
use yap_cache::settings::{self, EmojiPrefs};
use yap_core::store::view::ViewDiff;

use crate::gui;
use crate::models::{self, qobject as m};

#[derive(Default)]
pub struct EmojiRust {
    skin_tone: i32,
    quick: QString,
}

/// The query on screen and the saved prefs, GUI thread only.
#[derive(Default)]
struct Picker {
    query: String,
    prefs: Option<EmojiPrefs>,
}

thread_local! {
    static PICKER: RefCell<Picker> = RefCell::new(Picker::default());
}

fn prefs() -> EmojiPrefs {
    PICKER.with(|p| {
        p.borrow_mut()
            .prefs
            .get_or_insert_with(|| {
                settings::app_dir()
                    .map(|dir| settings::load_emoji(&dir))
                    .unwrap_or_default()
            })
            .clone()
    })
}

fn save(prefs: EmojiPrefs) {
    if let Some(dir) = settings::app_dir() {
        if let Err(error) = settings::save_emoji(&dir, &prefs) {
            eprintln!("[emoji] {error}");
        }
    }
    PICKER.with(|p| p.borrow_mut().prefs = Some(prefs));
}

fn api_base() -> &'static str {
    &yap_core::config::get().api_base
}

fn quick(prefs: &EmojiPrefs) -> QString {
    QString::from(&yap_core::picker::quick_reactions(
        &prefs.recents,
        prefs.skin_tone,
        api_base(),
    ))
}

impl cxx_qt::Initialize for qobject::Emoji {
    fn initialize(mut self: Pin<&mut Self>) {
        let prefs = prefs();
        self.as_mut().set_skin_tone(i32::from(prefs.skin_tone));
        self.set_quick(quick(&prefs));
    }
}

/// Rebuilds the grid for the current query and tone.
fn refill() {
    let prefs = prefs();
    let query = PICKER.with(|p| p.borrow().query.clone());
    let rows = yap_core::picker::rows(&query, prefs.skin_tone, &prefs.recents, api_base());
    models::apply::<m::EmojiGridModel>(ViewDiff::Reset(rows));
}

impl qobject::Emoji {
    pub fn search(self: Pin<&mut Self>, query: &QString) {
        PICKER.with(|p| p.borrow_mut().query = query.to_string());
        refill();
    }

    pub fn set_tone(mut self: Pin<&mut Self>, tone: i32) {
        let mut prefs = prefs();
        prefs.skin_tone = u8::try_from(tone.clamp(0, 5)).unwrap_or(0);
        self.as_mut().set_skin_tone(i32::from(prefs.skin_tone));
        self.as_mut().set_quick(quick(&prefs));
        save(prefs);
        refill();
    }

    pub fn remember(mut self: Pin<&mut Self>, value: &QString) {
        let mut prefs = prefs();
        yap_core::picker::push_recent(&mut prefs.recents, &value.to_string());
        self.as_mut().set_quick(quick(&prefs));
        save(prefs);
    }

    pub fn section_row(&self, section: &QString) -> i32 {
        let section = section.to_string();
        gui::with_rows(|rows| {
            rows.emoji
                .iter()
                .position(|r| r.kind == "header" && r.section == section)
                .and_then(|i| i32::try_from(i).ok())
                .unwrap_or(-1)
        })
    }
}
