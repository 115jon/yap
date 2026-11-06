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
        /// The sound effect switches and volumes (NotificationsPanel.tsx).
        #[qproperty(i32, sound_volume, cxx_name = "soundVolume")]
        #[qproperty(bool, sound_notifications, cxx_name = "soundNotifications")]
        #[qproperty(bool, sound_voice_join_leave, cxx_name = "soundVoiceJoinLeave")]
        #[qproperty(bool, sound_connect, cxx_name = "soundConnect")]
        #[qproperty(bool, sound_mute, cxx_name = "soundMute")]
        #[qproperty(bool, sound_screen_share, cxx_name = "soundScreenShare")]
        #[qproperty(bool, sound_stream_viewers, cxx_name = "soundStreamViewers")]
        #[qproperty(i32, soundboard_volume, cxx_name = "soundboardVolume")]
        /// Imported sound files play instead of the built-in ones.
        #[qproperty(bool, custom_sounds, cxx_name = "customSounds")]
        /// "Don't ask again" on the voice switch confirm.
        #[qproperty(bool, skip_voice_switch, cxx_name = "skipVoiceSwitch")]
        /// "Don't ask again" on the start call confirm.
        #[qproperty(bool, skip_start_call, cxx_name = "skipStartCall")]
        /// Ring and call tones (the `calls` sound category).
        #[qproperty(bool, sound_calls, cxx_name = "soundCalls")]
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
        /// `openOnStartup`, `notifications`, `sounds`, or a sound category:
        /// `soundNotifications`, `soundVoiceJoinLeave`, `soundConnect`,
        /// `soundMute`, `soundScreenShare`, `soundStreamViewers`) and saves it.
        #[qinvokable]
        #[cxx_name = "setSetting"]
        fn set_setting(self: Pin<&mut Desktop>, key: &QString, value: bool);

        /// `soundVolume` or `soundboardVolume`, 0-100, saved.
        #[qinvokable]
        #[cxx_name = "setVolume"]
        fn set_volume(self: Pin<&mut Desktop>, key: &QString, value: i32);

        /// Plays the notification chime whatever the switches say (Test).
        #[qinvokable]
        #[cxx_name = "playTestSound"]
        fn play_test_sound(self: &Desktop);

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
    sound_volume: i32,
    sound_notifications: bool,
    sound_voice_join_leave: bool,
    sound_connect: bool,
    sound_mute: bool,
    sound_screen_share: bool,
    sound_stream_viewers: bool,
    soundboard_volume: i32,
    custom_sounds: bool,
    skip_voice_switch: bool,
    skip_start_call: bool,
    sound_calls: bool,
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
    let sound = s.sound_settings;
    desktop
        .as_mut()
        .set_sound_volume(i32::from(sound.sound_volume));
    desktop
        .as_mut()
        .set_sound_notifications(sound.notifications);
    desktop
        .as_mut()
        .set_sound_voice_join_leave(sound.voice_join_leave);
    desktop
        .as_mut()
        .set_sound_connect(sound.self_connect_disconnect);
    desktop.as_mut().set_sound_mute(sound.mute_deafen);
    desktop.as_mut().set_sound_screen_share(sound.screen_share);
    desktop
        .as_mut()
        .set_sound_stream_viewers(sound.stream_watcher_activity);
    desktop
        .as_mut()
        .set_soundboard_volume(i32::from(sound.soundboard_volume));
    desktop.as_mut().set_custom_sounds(sound.custom_sounds);
    desktop
        .as_mut()
        .set_skip_voice_switch(s.skip_voice_switch_confirm);
    desktop
        .as_mut()
        .set_skip_start_call(s.skip_start_call_confirm);
    desktop.as_mut().set_sound_calls(sound.calls);
    desktop.set_sounds(s.sounds);
}

/// Saves `next` as the settings and shows it.
fn store(mut desktop: Pin<&mut qobject::Desktop>, next: DesktopSettings) {
    let dir = with_shared(|s| {
        s.settings = next;
        s.dir.clone()
    });
    if let Some(dir) = dir {
        if let Err(error) = settings::save_settings(&dir, &next) {
            eprintln!("[desktop] settings not saved: {error}");
        }
    }
    apply_settings(desktop.as_mut(), next);
    apply_badge(desktop);
    crate::apps::send_local_settings();
    crate::apps_audio::set_soundboard_volume(next.sound_settings.soundboard_volume);
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
    pub fn set_setting(self: Pin<&mut Self>, key: &QString, value: bool) {
        let key = key.to_string();
        let mut next = current();
        let sound = &mut next.sound_settings;
        let field = match key.as_str() {
            "closeToTray" => &mut next.close_to_tray,
            "startMinimized" => &mut next.start_minimized,
            "openOnStartup" => &mut next.open_on_startup,
            "notifications" => &mut next.desktop_notifications,
            "sounds" => &mut next.sounds,
            "soundNotifications" => &mut sound.notifications,
            "soundVoiceJoinLeave" => &mut sound.voice_join_leave,
            "soundConnect" => &mut sound.self_connect_disconnect,
            "soundMute" => &mut sound.mute_deafen,
            "soundScreenShare" => &mut sound.screen_share,
            "soundStreamViewers" => &mut sound.stream_watcher_activity,
            "customSounds" => &mut sound.custom_sounds,
            "skipVoiceSwitch" => &mut next.skip_voice_switch_confirm,
            "skipStartCall" => &mut next.skip_start_call_confirm,
            "soundCalls" => &mut sound.calls,
            _ => {
                eprintln!("[desktop] unknown setting {key}");
                return;
            }
        };
        *field = value;
        store(self, next);
    }

    pub fn set_volume(self: Pin<&mut Self>, key: &QString, value: i32) {
        let volume = u8::try_from(value.clamp(0, 100)).unwrap_or(100);
        let mut next = current();
        match key.to_string().as_str() {
            "soundVolume" => next.sound_settings.sound_volume = volume,
            "soundboardVolume" => next.sound_settings.soundboard_volume = volume,
            other => {
                eprintln!("[desktop] unknown volume {other}");
                return;
            }
        }
        store(self, next);
    }

    pub fn play_test_sound(&self) {
        crate::sound::play_test(yap_core::shell::Sound::Notification);
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
