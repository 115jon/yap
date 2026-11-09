//! `Colors`: the colour picker's side of the core (`ColorPopover.qml`).
//! The HSV, RGB and hex maths are `yap_core::color`; the recent colours
//! are kept on this device in `color-recents.json` (yap-cache), app-wide.

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
        /// The last colours picked, most recent first: JSON `["#RRGGBB"]`.
        #[qproperty(QString, recents)]
        type Colors = super::ColorsRust;

        /// `#RRGGBB` for a hue in degrees and saturation and value in 0-1.
        #[qinvokable]
        #[cxx_name = "hsvToHex"]
        fn hsv_to_hex(self: &Colors, h: f64, s: f64, v: f64) -> QString;

        /// The HSV of a hex colour as JSON `[h, s, v]`, or empty when
        /// `text` is not one.
        #[qinvokable]
        #[cxx_name = "hexToHsv"]
        fn hex_to_hsv(self: &Colors, text: &QString) -> QString;

        /// `#RRGGBB` for `#RRGGBB`, `RRGGBB`, `#RGB` or `RGB`, else empty.
        #[qinvokable]
        fn normalize(self: &Colors, text: &QString) -> QString;

        /// A colour the picker settled on goes first in the recents.
        #[qinvokable]
        fn remember(self: Pin<&mut Colors>, text: &QString);
    }

    impl cxx_qt::Initialize for Colors {}
}

use core::pin::Pin;

use cxx_qt_lib::QString;
use yap_cache::settings::{self, ColorPrefs};
use yap_core::color;

#[derive(Default)]
pub struct ColorsRust {
    recents: QString,
}

fn load() -> Vec<String> {
    settings::app_dir()
        .map(|dir| color::clean_recents(&settings::load_colors(&dir).recents))
        .unwrap_or_default()
}

fn json(recents: &[String]) -> QString {
    QString::from(&serde_json::to_string(recents).unwrap_or_else(|_| "[]".into()))
}

impl cxx_qt::Initialize for qobject::Colors {
    fn initialize(self: Pin<&mut Self>) {
        self.set_recents(json(&load()));
    }
}

impl qobject::Colors {
    pub fn hsv_to_hex(&self, h: f64, s: f64, v: f64) -> QString {
        QString::from(&color::hsv_to_hex(h, s, v))
    }

    pub fn hex_to_hsv(&self, text: &QString) -> QString {
        color::hex_to_hsv(&text.to_string())
            .map(|c| QString::from(&format!("[{},{},{}]", c.h, c.s, c.v)))
            .unwrap_or_default()
    }

    pub fn normalize(&self, text: &QString) -> QString {
        yap_core::settings::normalize_hex_color(&text.to_string())
            .map(|c| QString::from(&c))
            .unwrap_or_default()
    }

    pub fn remember(self: Pin<&mut Self>, text: &QString) {
        let mut recents = load();
        if !color::remember(&mut recents, &text.to_string()) {
            return;
        }
        if let Some(dir) = settings::app_dir() {
            let prefs = ColorPrefs {
                recents: recents.clone(),
            };
            if let Err(error) = settings::save_colors(&dir, &prefs) {
                eprintln!("[color] {error}");
            }
        }
        self.set_recents(json(&recents));
    }
}
