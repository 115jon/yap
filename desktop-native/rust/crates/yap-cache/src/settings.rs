//! The desktop preferences and the main window's last rect: two small JSON
//! files beside the snapshot cache, app-wide rather than per user, read
//! before any session starts (close to tray, start minimized and the
//! window rect are needed at launch). Unlike the snapshot they survive a
//! schema change. The shape follows the desktop app's
//! `desktop-settings.json` and `.window-state.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::CacheError;

pub const SETTINGS_FILE: &str = "desktop-settings.json";
pub const WINDOW_STATE_FILE: &str = "window-state.json";
pub const EMOJI_FILE: &str = "emoji-prefs.json";
pub const APPEARANCE_FILE: &str = "appearance.json";
pub const SEARCH_FILE: &str = "search-recent.json";

/// The theme picked on this device (next-themes' `theme` key on the web):
/// a theme id or `system`. App-wide, like the web's origin storage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearancePrefs {
    pub theme: String,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
        }
    }
}

/// The sound effect switches and volumes (useSoundSettingsStore). The
/// master switch is [`DesktopSettings::sounds`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SoundPrefs {
    /// 0-100, for every UI sound.
    pub sound_volume: u8,
    /// Mentions, replies and DMs.
    pub notifications: bool,
    pub voice_join_leave: bool,
    pub self_connect_disconnect: bool,
    pub mute_deafen: bool,
    pub screen_share: bool,
    pub stream_watcher_activity: bool,
    pub message_received: bool,
    /// 0-100, soundboard playback.
    pub soundboard_volume: u8,
}

impl Default for SoundPrefs {
    fn default() -> Self {
        Self {
            sound_volume: 100,
            notifications: true,
            voice_join_leave: true,
            self_connect_disconnect: true,
            mute_deafen: true,
            screen_share: true,
            stream_watcher_activity: true,
            message_received: true,
            soundboard_volume: 100,
        }
    }
}

/// The emoji picker's memory (localStorage `chat:emoji:recents:v1` and
/// `chat:emoji:skin-tone:v1` on the web): recent picks, most recent first
/// (native emoji or custom `<:name:id>` tokens), and the skin tone 0-5.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EmojiPrefs {
    pub recents: Vec<String>,
    pub skin_tone: u8,
}

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
    /// Which sounds play, and how loud.
    pub sound_settings: SoundPrefs,
}

impl Default for DesktopSettings {
    fn default() -> Self {
        Self {
            open_on_startup: false,
            start_minimized: false,
            close_to_tray: true,
            desktop_notifications: true,
            sounds: true,
            sound_settings: SoundPrefs::default(),
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

pub(crate) fn read<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes through a temporary file and a rename, so a crash never leaves
/// half a file.
pub(crate) fn write<T: Serialize>(path: &Path, value: &T) -> Result<(), CacheError> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// A plain `*.json` name in the app folder (no folders, no `..`).
fn app_file_name(file: &str) -> bool {
    file.ends_with(".json")
        && file.len() > 5
        && file
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
        && !file.contains("..")
}

/// One of the voice apps' own files (custom sounds, Daily Word boards), as
/// JSON; `None` when missing, unreadable or not a plain file name.
pub fn load_app_file(dir: &Path, file: &str) -> Option<serde_json::Value> {
    app_file_name(file).then(|| read(&dir.join(file))).flatten()
}

/// Replaces one of those files with `contents`, which must be JSON.
pub fn save_app_file(dir: &Path, file: &str, contents: &str) -> Result<(), CacheError> {
    if !app_file_name(file) {
        return Err(CacheError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not an app file name",
        )));
    }
    let value: serde_json::Value = serde_json::from_str(contents)?;
    write(&dir.join(file), &value)
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

/// The theme picked on this device; the default when missing or unreadable.
pub fn load_appearance(dir: &Path) -> AppearancePrefs {
    read(&dir.join(APPEARANCE_FILE)).unwrap_or_default()
}

pub fn save_appearance(dir: &Path, prefs: &AppearancePrefs) -> Result<(), CacheError> {
    write(&dir.join(APPEARANCE_FILE), prefs)
}

/// The picker's memory in `dir`; empty when missing or unreadable.
pub fn load_emoji(dir: &Path) -> EmojiPrefs {
    read(&dir.join(EMOJI_FILE)).unwrap_or_default()
}

pub fn save_emoji(dir: &Path, prefs: &EmojiPrefs) -> Result<(), CacheError> {
    write(&dir.join(EMOJI_FILE), prefs)
}

/// Recent message searches by scope key (`s:<server>`, `c:<channel>`;
/// localStorage `rm:search-recent:<scope>` on the web), each entry as the
/// web stores it. Empty when missing or unreadable.
pub fn load_search_recents(dir: &Path) -> BTreeMap<String, Vec<serde_json::Value>> {
    read(&dir.join(SEARCH_FILE)).unwrap_or_default()
}

pub fn save_search_recents(
    dir: &Path,
    recents: &BTreeMap<String, Vec<serde_json::Value>>,
) -> Result<(), CacheError> {
    write(&dir.join(SEARCH_FILE), recents)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_files_should_round_trip_and_refuse_other_paths() {
        let dir = tempfile::tempdir().expect("dir");
        save_app_file(dir.path(), "wordle.json", r#"{"k":[1]}"#).expect("save");
        assert_eq!(
            load_app_file(dir.path(), "wordle.json"),
            Some(serde_json::json!({"k": [1]}))
        );
        assert!(save_app_file(dir.path(), "../x.json", "{}").is_err());
        assert!(save_app_file(dir.path(), "a/b.json", "{}").is_err());
        assert!(save_app_file(dir.path(), "x.json", "not json").is_err());
        assert!(load_app_file(dir.path(), "missing.json").is_none());
    }

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
    fn sound_prefs_default_to_every_sound_at_full_volume() {
        let s = SoundPrefs::default();
        assert_eq!((s.sound_volume, s.soundboard_volume), (100, 100));
    }

    #[test]
    fn sound_prefs_round_trip_inside_the_settings_file() {
        let dir = tempfile::tempdir().expect("dir");
        let settings = DesktopSettings {
            sound_settings: SoundPrefs {
                sound_volume: 40,
                mute_deafen: false,
                ..SoundPrefs::default()
            },
            ..DesktopSettings::default()
        };
        save_settings(dir.path(), &settings).expect("save");
        assert_eq!(load_settings(dir.path()), settings);
    }

    #[test]
    fn an_older_settings_file_gets_default_sound_prefs() {
        let dir = tempfile::tempdir().expect("dir");
        std::fs::write(dir.path().join(SETTINGS_FILE), r#"{"sounds": false}"#).expect("write");
        assert_eq!(
            load_settings(dir.path()).sound_settings,
            SoundPrefs::default()
        );
    }

    #[test]
    fn appearance_defaults_to_midnight() {
        let dir = tempfile::tempdir().expect("dir");
        assert_eq!(load_appearance(dir.path()).theme, "dark");
    }

    #[test]
    fn appearance_round_trips() {
        let dir = tempfile::tempdir().expect("dir");
        let prefs = AppearancePrefs {
            theme: "miku-light".into(),
        };
        save_appearance(dir.path(), &prefs).expect("save");
        assert_eq!(load_appearance(dir.path()), prefs);
    }

    #[test]
    fn emoji_prefs_round_trip() {
        let dir = tempfile::tempdir().expect("dir");
        let prefs = EmojiPrefs {
            recents: vec!["🔥".into(), "<:yap:1>".into()],
            skin_tone: 3,
        };
        save_emoji(dir.path(), &prefs).expect("save");
        assert_eq!(load_emoji(dir.path()), prefs);
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
