//! `Limits`: the upload limits and the too-big cards for QML
//! ([`yap_core::limits`]). Pure: nothing goes through the core, and a file
//! is only stat'ed, never read. A card is JSON (`limits.rs` `card`), or
//! empty when the file fits.

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
        type Limits = super::LimitsRust;

        /// The card for `path` against `kind`'s limit (`avatar`, `banner`,
        /// `sound`, `gif`, `vibe`, `background`), or empty when it fits or
        /// cannot be read (the upload then says why).
        #[qinvokable]
        fn check(self: &Limits, kind: &QString, path: &QString) -> QString;

        /// The drop overlay's card while files are dragged over the chat:
        /// `files` is JSON `[{name, size}]` (sizes from `Files.localFiles`).
        /// Empty when every file fits.
        #[qinvokable]
        #[cxx_name = "dropCard"]
        fn drop_card(self: &Limits, files: &QString) -> QString;

        /// The tray's one card for `n` too-big files with `m` others.
        #[qinvokable]
        #[cxx_name = "traySummary"]
        fn tray_summary(self: &Limits, n: i32, m: i32) -> QString;

        /// `kind`'s limit as words: `25 MB`.
        #[qinvokable]
        #[cxx_name = "limitText"]
        fn limit_text(self: &Limits, kind: &QString) -> QString;
    }
}

use cxx_qt_lib::QString;
use yap_core::limits::{self, Kind};

#[derive(Default)]
pub struct LimitsRust;

#[derive(serde::Deserialize)]
struct Dragged {
    #[serde(default)]
    name: String,
    #[serde(default)]
    size: u64,
}

impl qobject::Limits {
    pub fn check(&self, kind: &QString, path: &QString) -> QString {
        let Some(kind) = Kind::from_key(&kind.to_string()) else {
            return QString::default();
        };
        let path = path.to_string();
        let Ok(meta) = std::fs::metadata(&path) else {
            return QString::default();
        };
        let is_gif = std::path::Path::new(&path)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
        limits::picker_card(kind, meta.len(), is_gif)
            .map(|card| QString::from(&card.to_string()))
            .unwrap_or_default()
    }

    pub fn drop_card(&self, files: &QString) -> QString {
        let files: Vec<Dragged> = serde_json::from_str(&files.to_string()).unwrap_or_default();
        let files: Vec<(String, u64)> = files.into_iter().map(|f| (f.name, f.size)).collect();
        limits::drop_card(&files)
            .map(|card| QString::from(&card.to_string()))
            .unwrap_or_default()
    }

    pub fn tray_summary(&self, n: i32, m: i32) -> QString {
        let count = |v: i32| usize::try_from(v).unwrap_or(0);
        QString::from(&limits::tray_summary(count(n), count(m)).to_string())
    }

    pub fn limit_text(&self, kind: &QString) -> QString {
        Kind::from_key(&kind.to_string())
            .map(|kind| QString::from(&limits::format_limit(kind.limit())))
            .unwrap_or_default()
    }
}
