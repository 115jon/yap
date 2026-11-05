//! The desktop preferences and the main window's last rect: two small JSON
//! files beside the snapshot cache, app-wide rather than per user, read
//! before any session starts (close to tray, start minimized and the
//! window rect are needed at launch). Unlike the snapshot they survive a
//! schema change. The shape follows the desktop app's
//! `desktop-settings.json` and `.window-state.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CacheError;

pub const SETTINGS_FILE: &str = "desktop-settings.json";
pub const WINDOW_STATE_FILE: &str = "window-state.json";

/// The shell's switches, with the desktop app's defaults
/// (`desktop_settings.rs`). Missing fields take their default, unknown ones
/// are ignored, so older and newer builds share the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DesktopSettings {
    /// The `HKCU\…\Run` entry (`YapNative`).
    pub open_on_startup: bool,
    /// An autostart launch stays in the tray.
    pub start_minimized: bool,
    /// The close button hides to the tray instead of quitting.
    pub close_to_tray: bool,
    /// Toasts and the tray badge.
    pub desktop_notifications: bool,
    /// UI sounds (useSoundSettingsStore `soundsEnabled`).
    pub sounds: bool,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            open_on_startup: false,
            start_minimized: false,
            close_to_tray: true,
            desktop_notifications: true,
            sounds: true,
        }
    }
}

/// The main window's last normal rect (Qt's device-independent pixels) and
/// whether it was maximized.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowState {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    #[serde(default)]
    pub maximized: bool,
}

/// `%LOCALAPPDATA%\YapNative`, apart from the installed Yap app's folders.
/// `None` without `LOCALAPPDATA`.
pub fn app_dir() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(base).join("YapNative"))
}

fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes through a temporary file and a rename, so a crash never leaves
/// half a file.
fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), CacheError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// The settings in `dir`, or the defaults when the file is missing or does
/// not parse.
pub fn load_settings(dir: &Path) -> DesktopSettings {
    read(&dir.join(SETTINGS_FILE)).unwrap_or_default()
}

pub fn save_settings(dir: &Path, settings: &DesktopSettings) -> Result<(), CacheError> {
    write(&dir.join(SETTINGS_FILE), settings)
}

/// The saved window rect, if one was saved and it has a size.
pub fn load_window_state(dir: &Path) -> Option<WindowState> {
    read::<WindowState>(&dir.join(WINDOW_STATE_FILE)).filter(|s| s.width > 0 && s.height > 0)
}

pub fn save_window_state(dir: &Path, state: &WindowState) -> Result<(), CacheError> {
    write(&dir.join(WINDOW_STATE_FILE), state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_desktop_app() {
        let s = DesktopSettings::default();
        assert!(s.close_to_tray && s.desktop_notifications && s.sounds);
        assert!(!s.open_on_startup && !s.start_minimized);
    }

    #[test]
    fn a_missing_file_reads_as_defaults() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(load_settings(dir.path()), DesktopSettings::default());
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().expect("dir");
        let settings = DesktopSettings {
            close_to_tray: false,
            sounds: false,
            ..DesktopSettings::default()
        };
        save_settings(dir.path(), &settings).expect("save");
        assert_eq!(load_settings(dir.path()), settings);
    }

    #[test]
    fn missing_fields_take_defaults_and_unknown_ones_are_ignored() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(SETTINGS_FILE),
            r#"{"startMinimized": true, "hardwareAcceleration": false}"#,
        )
        .expect("write");
        let settings = load_settings(dir.path());
        assert!(settings.start_minimized && settings.close_to_tray);
    }

    #[test]
    fn a_corrupt_file_reads_as_defaults() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join(SETTINGS_FILE), "{not json").expect("write");
        assert_eq!(load_settings(dir.path()), DesktopSettings::default());
    }

    #[test]
    fn window_state_round_trips() {
        let dir = tempfile::tempdir().expect("dir");
        let state = WindowState {
            x: -1200,
            y: 40,
            width: 1280,
            height: 800,
            maximized: true,
        };
        save_window_state(dir.path(), &state).expect("save");
        assert_eq!(load_window_state(dir.path()), Some(state));
    }

    #[test]
    fn an_empty_window_rect_is_ignored() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(
            dir.path().join(WINDOW_STATE_FILE),
            r#"{"x":0,"y":0,"width":0,"height":0}"#,
        )
        .expect("write");
        assert_eq!(load_window_state(dir.path()), None);
    }
}
