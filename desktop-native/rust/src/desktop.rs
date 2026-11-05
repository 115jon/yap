//! `Desktop`: the shell settings (close to tray, start minimized, open on
//! startup, notifications, sounds) from `%LOCALAPPDATA%\YapNative`, the
//! tray badge and the toasts the core asks for, and the window rect. The OS
//! side (tray, toasts, Run key, single instance) is `DesktopShell` in
//! `cpp/desktopshell.*`; QML wires the two.

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
        #[qproperty(bool, close_to_tray, cxx_name = "closeToTray")]
        #[qproperty(bool, start_minimized, cxx_name = "startMinimized")]
        #[qproperty(bool, open_on_startup, cxx_name = "openOnStartup")]
        #[qproperty(bool, notifications)]
        #[qproperty(bool, sounds)]
        /// Unread notifications (capped at 99); 0 with `badgeDot` or nothing.
        #[qproperty(i32, badge_count, cxx_name = "badgeCount")]
        #[qproperty(bool, badge_dot, cxx_name = "badgeDot")]
        #[qproperty(QString, badge_tooltip, cxx_name = "badgeTooltip")]
        /// This process was started by the Run entry (`--autostart`).
        #[qproperty(bool, autostarted)]
        type Desktop = super::DesktopRust;

        /// A toast to show; a click opens `channel_id`.
        #[qsignal]
        fn toast(
            self: Pin<&mut Desktop>,
            title: QString,
            summary: QString,
            body: QString,
            channel_id: QString,
            server_id: QString,
        );

        /// Changes one setting (`closeToTray`, `startMinimized`,
        /// `openOnStartup`, `notifications`, `sounds`) and saves it.
        #[qinvokable]
        #[cxx_name = "setSetting"]
        fn set_setting(self: Pin<&mut Desktop>, key: &QString, value: bool);

        /// Opens a channel or DM by id (a toast click); `server_id` may be
        /// empty.
        #[qinvokable]
        #[cxx_name = "openChannel"]
        fn open_channel(self: &Desktop, channel_id: &QString, server_id: &QString);

        /// Where the window opens: `x,y,w,h,maximized` from the saved rect
        /// placed on `areas` (`x,y,w,h;…`, the monitors' work areas), or
        /// empty for the default.
        #[qinvokable]
        #[cxx_name = "windowPlacement"]
        fn window_placement(self: &Desktop, areas: &QString, primary: i32) -> QString;

        #[qinvokable]
        #[cxx_name = "saveWindowState"]
        fn save_window_state(
            self: &Desktop,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            maximized: bool,
        );

        /// Start in the tray: an autostart launch with start minimized on.
        #[qinvokable]
        #[cxx_name = "launchToTray"]
        fn launch_to_tray(self: &Desktop) -> bool;
    }

    impl cxx_qt::Initialize for Desktop {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use cxx_qt_lib::QString;
use yap_cache::settings::{self, DesktopSettings, WindowState};
use yap_core::shell::{self, BadgeView, Rect, ToastView};

use crate::client::{self, Command};
use crate::gui;

#[derive(Default)]
pub struct DesktopRust {
    close_to_tray: bool,
    start_minimized: bool,
    open_on_startup: bool,
    notifications: bool,
    sounds: bool,
    badge_count: i32,
    badge_dot: bool,
    badge_tooltip: QString,
    autostarted: bool,
}

struct Shared {
    dir: Option<PathBuf>,
    settings: DesktopSettings,
    /// The badge the core last sent (shown only with notifications on).
    badge: BadgeView,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = settings::app_dir();
        let settings = dir
            .as_deref()
            .map(settings::load_settings)
            .unwrap_or_default();
        Mutex::new(Shared {
            dir,
            settings,
            badge: BadgeView::default(),
        })
    })
}

fn with_shared<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    f(&mut shared().lock().unwrap_or_else(PoisonError::into_inner))
}

/// The settings as saved (read once, then kept current).
pub fn current() -> DesktopSettings {
    with_shared(|s| s.settings)
}

/// What the tray shows: the core's badge, or nothing with notifications
/// off (desktop-native-sync.ts clears it then).
fn shown_badge() -> BadgeView {
    with_shared(|s| {
        if s.settings.desktop_notifications {
            s.badge.clone()
        } else {
            BadgeView::default()
        }
    })
}

fn apply_badge(mut desktop: Pin<&mut qobject::Desktop>) {
    let badge = shown_badge();
    desktop
        .as_mut()
        .set_badge_count(i32::try_from(badge.count).unwrap_or(i32::MAX));
    desktop.as_mut().set_badge_dot(badge.dot);
    desktop.set_badge_tooltip(QString::from(badge.tooltip.as_str()));
}

fn apply_settings(mut desktop: Pin<&mut qobject::Desktop>, s: DesktopSettings) {
    desktop.as_mut().set_close_to_tray(s.close_to_tray);
    desktop.as_mut().set_start_minimized(s.start_minimized);
    desktop.as_mut().set_open_on_startup(s.open_on_startup);
    desktop.as_mut().set_notifications(s.desktop_notifications);
    desktop.set_sounds(s.sounds);
}

/// The core's badge changed.
pub(crate) fn on_badge(badge: BadgeView) {
    with_shared(|s| s.badge = badge);
    if let Some(desktop) = gui::object::<qobject::Desktop>() {
        apply_badge(desktop);
    }
}

/// The core asks for a toast; dropped with notifications off.
pub(crate) fn on_toast(toast: &ToastView) {
    if !current().desktop_notifications {
        return;
    }
    if let Some(desktop) = gui::object::<qobject::Desktop>() {
        desktop.toast(
            QString::from(toast.title.as_str()),
            QString::from(toast.summary.as_str()),
            QString::from(toast.body.as_str()),
            QString::from(toast.channel_id.as_str()),
            QString::from(toast.server_id.as_str()),
        );
    }
}

impl gui::Slot for qobject::Desktop {
    fn slot(objects: &mut gui::Objects) -> &mut Option<std::ptr::NonNull<Self>> {
        &mut objects.desktop
    }

    fn restore(mut self: Pin<&mut Self>) {
        apply_settings(self.as_mut(), current());
        self.as_mut().set_autostarted(autostarted());
        apply_badge(self);
    }
}

impl cxx_qt::Initialize for qobject::Desktop {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for DesktopRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::Desktop>();
    }
}

/// `--autostart` on this process's command line (lossy: a non-Unicode
/// argument must not panic `std::env::args`).
fn autostarted() -> bool {
    shell::launched_by_autostart(std::env::args_os().map(|a| a.to_string_lossy().into_owned()))
}

fn parse_rects(text: &str) -> Vec<Rect> {
    text.split(';')
        .filter_map(|part| {
            let n: Vec<i32> = part
                .split(',')
                .map(|v| v.trim().parse().ok())
                .collect::<Option<_>>()?;
            match n[..] {
                [x, y, width, height] => Some(Rect {
                    x,
                    y,
                    width,
                    height,
                }),
                _ => None,
            }
        })
        .collect()
}

impl qobject::Desktop {
    pub fn set_setting(mut self: Pin<&mut Self>, key: &QString, value: bool) {
        let key = key.to_string();
        let (next, dir) = with_shared(|s| {
            let mut next = s.settings;
            match key.as_str() {
                "closeToTray" => next.close_to_tray = value,
                "startMinimized" => next.start_minimized = value,
                "openOnStartup" => next.open_on_startup = value,
                "notifications" => next.desktop_notifications = value,
                "sounds" => next.sounds = value,
                _ => eprintln!("[desktop] unknown setting {key}"),
            }
            s.settings = next;
            (next, s.dir.clone())
        });
        if let Some(dir) = dir {
            if let Err(error) = settings::save_settings(&dir, &next) {
                eprintln!("[desktop] settings not saved: {error}");
            }
        }
        apply_settings(self.as_mut(), next);
        apply_badge(self);
    }

    pub fn open_channel(&self, channel_id: &QString, server_id: &QString) {
        let server_id = server_id.to_string();
        client::command(Command::OpenChannel {
            channel_id: channel_id.to_string(),
            server_id: (!server_id.is_empty()).then_some(server_id),
        });
    }

    pub fn window_placement(&self, areas: &QString, primary: i32) -> QString {
        let Some(saved) = with_shared(|s| s.dir.clone())
            .as_deref()
            .and_then(settings::load_window_state)
        else {
            return QString::default();
        };
        let areas = parse_rects(&areas.to_string());
        let rect = Rect {
            x: saved.x,
            y: saved.y,
            width: saved.width,
            height: saved.height,
        };
        let primary = usize::try_from(primary).unwrap_or_default();
        match shell::place(rect, &areas, primary) {
            Some(r) => QString::from(
                format!(
                    "{},{},{},{},{}",
                    r.x,
                    r.y,
                    r.width,
                    r.height,
                    u8::from(saved.maximized)
                )
                .as_str(),
            ),
            None => QString::default(),
        }
    }

    pub fn save_window_state(&self, x: i32, y: i32, width: i32, height: i32, maximized: bool) {
        let state = WindowState {
            x,
            y,
            width,
            height,
            maximized,
        };
        if let Some(dir) = with_shared(|s| s.dir.clone()) {
            if let Err(error) = settings::save_window_state(&dir, &state) {
                eprintln!("[desktop] window state not saved: {error}");
            }
        }
    }

    pub fn launch_to_tray(&self) -> bool {
        shell::launch_to_tray(autostarted(), current().start_minimized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_work_areas() {
        assert_eq!(
            parse_rects("0,0,1920,1040;1920,-200,2560,1400"),
            [
                Rect {
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1040
                },
                Rect {
                    x: 1920,
                    y: -200,
                    width: 2560,
                    height: 1400
                }
            ]
        );
    }

    #[test]
    fn skips_malformed_areas() {
        assert!(parse_rects("1,2,3;x,1,2,3").is_empty());
    }
}
