//! Replacing the Tauri desktop app (desktop/, docs/desktop/launch.md) with
//! this one: find its install, uninstall it with its own uninstaller,
//! remove the per-user values it may leave pointing at it, and import what
//! it kept on this PC (docs/INSTALLER.md, "Replacing the Tauri app").
//!
//! The Tauri app (`desktop/installer`, `InstallEnvironment.cs`,
//! `InstallerLogic.cs`) installs per user to `%LOCALAPPDATA%\Yap`:
//! `Yap.exe` (the root stub), `Update.exe` (a copy of `YapSetup.exe`, the
//! uninstaller), `current.json` and `app-<version>\`. It writes
//! `HKCU\...\Uninstall\Yap` (`QuietUninstallString` `"<root>\Update.exe"
//! --uninstall /S`), `HKCU\Software\115jon\Yap`, `yap://`
//! (`Software\Classes\yap`, to the stub), the Run value `Yap` and the
//! shortcuts `Desktop\Yap.lnk` and `Start Menu\Programs\Yap.lnk` (AUMID
//! `com.115jon.yap`). Its data is `%APPDATA%\Yap`: `desktop-settings.json`,
//! `.window-state.json` and the CEF profile, whose localStorage (Chromium's
//! LevelDB) holds the web UI's theme and sound settings.
//!
//! Everything here is idempotent and only ever removes what points into the
//! Tauri install root. Its uninstaller deletes that root, its own registry
//! keys and values that point into it, and the two `Yap.lnk` shortcuts by
//! name, which are this app's too: [`save_shortcuts`] keeps ours and
//! [`restore_shortcuts`] puts them back. User data (`%APPDATA%\Yap`) is read,
//! never changed. The registry is behind [`Registry`] so tests never touch
//! the real one; the process calls are the app's (`installer.rs`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;
use yap_cache::settings::{self as native, AppearancePrefs, DesktopSettings, WindowState};

use crate::install::{command_is_ours, same_path, Registry, RUN_KEY};
use crate::links;

/// The Tauri app's install folder under `%LOCALAPPDATA%`.
pub const INSTALL_DIR: &str = "Yap";

/// Its uninstaller, a copy of its installer, at the install root.
pub const UNINSTALLER: &str = "Update.exe";

/// What its uninstaller takes for a silent uninstall (`QuietUninstallString`).
pub const UNINSTALL_ARGS: [&str; 2] = ["--uninstall", "/S"];

/// Its version marker at the install root (`InstallRootLayout.cs`).
pub const STATE_FILE: &str = "current.json";

/// Its uninstall entry (`InstallerLogic.CurrentUninstallKeyPath`).
pub const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Yap";

/// Its install-location key (`InstallerLogic.CurrentPublisherKeyPath`).
pub const PUBLISHER_KEY: &str = r"Software\115jon\Yap";

/// Its autostart value (`desktop_settings.rs` `RUN_VALUE_NAME`), the name
/// this app now uses for the default profile too.
pub const RUN_VALUE: &str = "Yap";

/// Its shortcut name on the desktop and in the Start menu, ours too.
pub const SHORTCUT: &str = "Yap.lnk";

/// Its data folder under `%APPDATA%` (`lib.rs` `APP_DATA_DIRECTORY_NAME`).
pub const DATA_DIR: &str = "Yap";

/// Its desktop preferences in [`DATA_DIR`] (`desktop_settings.rs`).
pub const SETTINGS_FILE: &str = "desktop-settings.json";

/// Its window rect in [`DATA_DIR`] (`window_state.rs`), physical pixels.
pub const WINDOW_STATE_FILE: &str = ".window-state.json";

/// Written into this app's data folder once the import ran, so it runs once
/// (`"done": true`), or, while the localStorage could not be read, counting
/// the tries (`"done": false`).
pub const IMPORT_MARKER: &str = "tauri-import.json";

/// How many launches try to read an unreadable localStorage (a log the
/// Tauri app was writing as it was read, say) before the plain files are
/// imported without it.
pub const IMPORT_ATTEMPTS: u64 = 3;

/// Where [`persist_shortcuts`] keeps our shortcuts in the data folder while
/// the Tauri uninstaller runs, so a later launch can put them back if this
/// one ends first.
pub const SAVED_SHORTCUTS_DIR: &str = "tauri-shortcuts";

/// The folders that matter, from the caller (known folders on Windows,
/// fixture folders in tests).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// `%LOCALAPPDATA%`.
    pub local_app_data: PathBuf,
    /// `%APPDATA%`.
    pub roaming_app_data: PathBuf,
    /// The user's desktop folder.
    pub desktop: PathBuf,
    /// The Start menu's `Programs` folder.
    pub programs: PathBuf,
}

/// A Tauri install found on this PC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TauriInstall {
    /// `%LOCALAPPDATA%\Yap`.
    pub root: PathBuf,
    /// `<root>\Update.exe`.
    pub uninstaller: PathBuf,
    /// The uninstall entry's `DisplayVersion`, when there is one.
    pub version: Option<String>,
}

/// The Tauri install, if there is one: `%LOCALAPPDATA%\Yap` with its
/// uninstaller and its version marker (`current.json`) or an `app-*`
/// folder. Never a Velopack layout (a `current\` folder: this app under a
/// test pack id of `Yap` would look like that), and never when the
/// uninstall entry names another folder: then the entry is not the one
/// the Tauri installer writes, and nothing is run.
pub fn detect(reg: &dyn Registry, paths: &Paths) -> Option<TauriInstall> {
    let root = paths.local_app_data.join(INSTALL_DIR);
    let uninstaller = root.join(UNINSTALLER);
    if !uninstaller.is_file() || root.join("current").exists() {
        return None;
    }
    let versioned = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("app-") && e.path().is_dir());
    if !root.join(STATE_FILE).is_file() && !versioned {
        return None;
    }
    if let Some(location) = reg.read(UNINSTALL_KEY, "InstallLocation") {
        if !same_path(&location, &root.to_string_lossy()) {
            return None;
        }
    }
    Some(TauriInstall {
        root,
        uninstaller,
        version: reg.read(UNINSTALL_KEY, "DisplayVersion"),
    })
}

/// A shortcut of ours kept across the Tauri uninstall: its path and bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedShortcut {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
}

/// Whether a `.lnk` file's bytes name a path inside `root`: its target is
/// stored as ANSI and as UTF-16, so either form counts. Case-insensitive.
pub fn shortcut_points_into(bytes: &[u8], root: &Path) -> bool {
    let root = root.to_string_lossy().to_ascii_lowercase();
    let root = root.trim_end_matches(['\\', '/']);
    if root.is_empty() {
        return false;
    }
    let lower: Vec<u8> = bytes.iter().map(u8::to_ascii_lowercase).collect();
    let ansi = root.as_bytes();
    let wide: Vec<u8> = root.encode_utf16().flat_map(u16::to_le_bytes).collect();
    let contains = |needle: &[u8]| lower.windows(needle.len()).any(|w| w == needle);
    contains(ansi) || contains(&wide)
}

/// Our `Yap.lnk` shortcuts (pointing into `our_root`), read before the
/// Tauri uninstaller deletes every shortcut of that name.
pub fn save_shortcuts(paths: &Paths, our_root: &Path) -> Vec<SavedShortcut> {
    [&paths.desktop, &paths.programs]
        .iter()
        .map(|dir| dir.join(SHORTCUT))
        .filter_map(|path| {
            let bytes = std::fs::read(&path).ok()?;
            shortcut_points_into(&bytes, our_root).then_some(SavedShortcut { path, bytes })
        })
        .collect()
}

/// Writes `saved` into `dir` (`<n>.lnk` with `<n>.path`), replacing what
/// was there, before the uninstaller runs.
pub fn persist_shortcuts(saved: &[SavedShortcut], dir: &Path) -> std::io::Result<()> {
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    if saved.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    for (i, s) in saved.iter().enumerate() {
        std::fs::write(dir.join(format!("{i}.lnk")), &s.bytes)?;
        std::fs::write(
            dir.join(format!("{i}.path")),
            s.path.to_string_lossy().as_bytes(),
        )?;
    }
    Ok(())
}

/// What [`persist_shortcuts`] kept in `dir`; empty when nothing is there.
pub fn load_persisted_shortcuts(dir: &Path) -> Vec<SavedShortcut> {
    (0..16)
        .map_while(|i| {
            let bytes = std::fs::read(dir.join(format!("{i}.lnk"))).ok()?;
            let path = std::fs::read_to_string(dir.join(format!("{i}.path"))).ok()?;
            Some(SavedShortcut {
                path: PathBuf::from(path),
                bytes,
            })
        })
        .collect()
}

/// Puts back each saved shortcut that is gone. Returns log lines.
pub fn restore_shortcuts(saved: &[SavedShortcut]) -> Vec<String> {
    saved
        .iter()
        .map(|s| {
            if s.path.exists() {
                format!("shortcut {}: kept", s.path.display())
            } else {
                match std::fs::write(&s.path, &s.bytes) {
                    Ok(()) => format!("shortcut {}: restored", s.path.display()),
                    Err(e) => format!("shortcut {}: not restored: {e}", s.path.display()),
                }
            }
        })
        .collect()
}

/// After the uninstaller: the Run value and `yap://` if they still point
/// into the Tauri root (the uninstaller failed, or ran an older build).
/// Values pointing anywhere else, ours above all, stay. Returns log lines.
pub fn remove_leftovers(reg: &mut dyn Registry, tauri_root: &Path) -> Vec<String> {
    let root = tauri_root.to_string_lossy();
    let mut log = Vec::new();
    match reg.read(RUN_KEY, RUN_VALUE) {
        Some(command) if command_is_ours(&command, &root) => {
            let ok = reg.delete_value(RUN_KEY, RUN_VALUE);
            log.push(format!("tauri run value: removed ({ok})"));
        }
        Some(_) => log.push("tauri run value: not the Tauri app's, kept".to_owned()),
        None => log.push("tauri run value: none".to_owned()),
    }
    let key = links::scheme_key(links::SCHEME);
    match reg.read(&crate::install::scheme_command_key(&key), "") {
        Some(command) if command_is_ours(&command, &root) => {
            let ok = reg.delete_tree(&key);
            log.push(format!("tauri yap:// registration: removed ({ok})"));
        }
        Some(_) => log.push("yap:// registration: not the Tauri app's, kept".to_owned()),
        None => log.push("yap:// registration: none".to_owned()),
    }
    log
}

// Import

/// What [`import`] took over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Imported {
    pub desktop_settings: bool,
    pub sounds: bool,
    pub theme: Option<String>,
    pub window_state: bool,
    pub log: Vec<String>,
}

/// Imports the Tauri app's preferences into this app's data folder
/// `native_root` (the default profile's), once ([`IMPORT_MARKER`]), and
/// never over a file this app already has:
///
/// - `desktop-settings.json`: open on startup, start minimized, close to
///   tray, notifications, the game capture hook; with the sound switches
///   and volumes and the voice confirm choices from localStorage
///   (`sound-settings-storage`, `rm-*-skip-confirm`);
/// - `appearance.json`: localStorage `theme` (next-themes);
/// - `window-state.json`: `.window-state.json`'s main window, from physical
///   pixels to device-independent ones at `scale_at(x, y, width, height)`,
///   the scale of the monitor the rect is on (`None`: on none, and the rect
///   is not imported).
///
/// A localStorage that is there but cannot be read is tried again on the
/// next launches ([`IMPORT_ATTEMPTS`]) before the rest imports without it.
/// The session is not imported: the app signs in again.
pub fn import(
    paths: &Paths,
    native_root: &Path,
    scale_at: &dyn Fn(i32, i32, i32, i32) -> Option<f64>,
) -> Imported {
    let mut out = Imported::default();
    let marker = native_root.join(IMPORT_MARKER);
    let previous = read_json(&marker);
    if marker.exists()
        && previous
            .as_ref()
            .and_then(|v| v.get("done"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    {
        out.log.push("import: already done".to_owned());
        return out;
    }
    let data = paths.roaming_app_data.join(DATA_DIR);
    if !data.is_dir() {
        out.log.push(format!("import: no {}", data.display()));
        return out;
    }
    let items = match find_local_storage(paths) {
        Storage::Read(dir, items) => {
            out.log.push(format!(
                "import: localStorage {} ({} keys)",
                dir.display(),
                items.len()
            ));
            items
        }
        Storage::Missing => {
            out.log.push("import: no localStorage".to_owned());
            BTreeMap::new()
        }
        Storage::Unreadable => {
            let tries = previous
                .as_ref()
                .and_then(|v| v.get("attempts"))
                .and_then(Value::as_u64)
                .unwrap_or(0)
                + 1;
            if tries < IMPORT_ATTEMPTS {
                out.log.push(format!(
                    "import: localStorage unreadable (try {tries} of {IMPORT_ATTEMPTS}); next launch tries again"
                ));
                if std::fs::create_dir_all(native_root).is_ok() {
                    let pending = serde_json::json!({ "done": false, "attempts": tries });
                    let _ = std::fs::write(&marker, pending.to_string());
                }
                return out;
            }
            out.log.push(format!(
                "import: localStorage unreadable after {tries} tries; importing without it"
            ));
            BTreeMap::new()
        }
    };

    if !native_root.join(native::SETTINGS_FILE).exists() {
        let mut settings = DesktopSettings::default();
        let file = read_json(&data.join(SETTINGS_FILE));
        if let Some(file) = &file {
            apply_desktop_settings(&mut settings, file);
            out.desktop_settings = true;
        }
        if let Some(raw) = items.get("sound-settings-storage") {
            out.sounds = apply_sound_settings(&mut settings, raw);
        }
        for (key, flag) in [
            (
                "rm-voice-switch-skip-confirm",
                &mut settings.skip_voice_switch_confirm,
            ),
            (
                "rm-start-call-skip-confirm",
                &mut settings.skip_start_call_confirm,
            ),
        ] {
            if items.get(key).is_some_and(|v| v == "true") {
                *flag = true;
            }
        }
        if out.desktop_settings || out.sounds {
            match native::save_settings(native_root, &settings) {
                Ok(()) => out.log.push(format!(
                    "import: desktop settings (file {}, sounds {})",
                    out.desktop_settings, out.sounds
                )),
                Err(e) => {
                    out.log
                        .push(format!("import: desktop settings failed: {e}"));
                    out.desktop_settings = false;
                    out.sounds = false;
                }
            }
        }
    } else {
        out.log
            .push("import: desktop settings kept (already set here)".to_owned());
    }

    if !native_root.join(native::APPEARANCE_FILE).exists() {
        let theme = items
            .get("theme")
            .map(|t| t.trim().to_owned())
            .filter(|t| crate::settings::is_app_theme(t));
        if let Some(theme) = theme {
            let prefs = AppearancePrefs {
                theme: theme.clone(),
            };
            match native::save_appearance(native_root, &prefs) {
                Ok(()) => {
                    out.log.push(format!("import: theme {theme}"));
                    out.theme = Some(theme);
                }
                Err(e) => out.log.push(format!("import: theme failed: {e}")),
            }
        }
    } else {
        out.log
            .push("import: theme kept (already set here)".to_owned());
    }

    if !native_root.join(native::WINDOW_STATE_FILE).exists() {
        if let Some(state) =
            read_json(&data.join(WINDOW_STATE_FILE)).and_then(|v| window_state(&v, scale_at))
        {
            match native::save_window_state(native_root, &state) {
                Ok(()) => {
                    out.window_state = true;
                    out.log.push("import: window state".to_owned());
                }
                Err(e) => out.log.push(format!("import: window state failed: {e}")),
            }
        }
    } else {
        out.log
            .push("import: window state kept (already set here)".to_owned());
    }

    let summary = serde_json::json!({
        "done": true,
        "from": data.to_string_lossy(),
        "desktopSettings": out.desktop_settings,
        "sounds": out.sounds,
        "theme": out.theme,
        "windowState": out.window_state,
    });
    if std::fs::create_dir_all(native_root).is_ok() {
        let _ = std::fs::write(&marker, summary.to_string());
    }
    out
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn apply_desktop_settings(settings: &mut DesktopSettings, file: &Value) {
    let flag = |name: &str| file.get(name).and_then(Value::as_bool);
    if let Some(v) = flag("openOnStartup") {
        settings.open_on_startup = v;
    }
    if let Some(v) = flag("startMinimized") {
        settings.start_minimized = v;
    }
    if let Some(v) = flag("closeToTray") {
        settings.close_to_tray = v;
    }
    if let Some(v) = flag("desktopNotifications") {
        settings.desktop_notifications = v;
    }
    if let Some(v) = flag("gameCaptureHook") {
        settings.game_capture_hook = v;
    }
}

/// The zustand store (`useSoundSettingsStore`, version 1): the signed-in
/// user's settings, `{ state: { currentUser, userSettings: { <id>: ... } } }`.
fn apply_sound_settings(settings: &mut DesktopSettings, raw: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    let state = value.get("state").unwrap_or(&value);
    let Some(users) = state.get("userSettings").and_then(Value::as_object) else {
        return false;
    };
    let current = state.get("currentUser").and_then(Value::as_str);
    let Some(user) = current
        .and_then(|id| users.get(id))
        .or_else(|| (users.len() == 1).then(|| users.values().next()).flatten())
    else {
        return false;
    };
    let flag = |name: &str| user.get(name).and_then(Value::as_bool);
    let volume = |name: &str| {
        user.get(name)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite())
            .map(|v| v.round().clamp(0.0, 100.0) as u8)
    };
    let s = &mut settings.sound_settings;
    if let Some(v) = flag("soundsEnabled") {
        settings.sounds = v;
    }
    for (name, field) in [
        ("notifications", &mut s.notifications),
        ("voiceJoinLeave", &mut s.voice_join_leave),
        ("selfConnectDisconnect", &mut s.self_connect_disconnect),
        ("muteDeafen", &mut s.mute_deafen),
        ("screenShare", &mut s.screen_share),
        ("streamWatcherActivity", &mut s.stream_watcher_activity),
        ("messageReceived", &mut s.message_received),
        ("calls", &mut s.calls),
        ("customSounds", &mut s.custom_sounds),
    ] {
        if let Some(v) = flag(name) {
            *field = v;
        }
    }
    if let Some(v) = volume("soundVolume") {
        s.sound_volume = v;
    }
    if let Some(v) = volume("soundboardVolume") {
        s.soundboard_volume = v;
    }
    true
}

/// `{"main": {x, y, width, height, maximized, fullscreen}}` in physical
/// pixels, as device-independent pixels at the scale of the monitor it is
/// on; `None` when it is on no monitor now.
fn window_state(
    value: &Value,
    scale_at: &dyn Fn(i32, i32, i32, i32) -> Option<f64>,
) -> Option<WindowState> {
    let main = value.get("main")?;
    let num = |name: &str| {
        main.get(name)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && v.abs() < 1e7)
            .map(|v| v.round() as i32)
    };
    let (x, y, width, height) = (num("x")?, num("y")?, num("width")?, num("height")?);
    let scale = scale_at(x, y, width, height).filter(|s| s.is_finite() && *s >= 0.5)?;
    let px = |v: i32| (f64::from(v) / scale).round() as i32;
    let state = WindowState {
        x: px(x),
        y: px(y),
        width: px(width),
        height: px(height),
        maximized: main
            .get("maximized")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    (state.width > 0 && state.height > 0).then_some(state)
}

// localStorage

/// Where the Tauri app's web UI may have kept its localStorage, best first:
/// its CEF profile under `%APPDATA%\Yap`, then WebView2's under
/// `%LOCALAPPDATA%\com.115jon.yap` (builds before CEF).
pub fn local_storage_dirs(paths: &Paths) -> Vec<PathBuf> {
    let data = paths.roaming_app_data.join(DATA_DIR);
    let webview = paths
        .local_app_data
        .join("com.115jon.yap")
        .join("EBWebView");
    let leveldb = |p: PathBuf| p.join("Local Storage").join("leveldb");
    vec![
        leveldb(data.join("cef").join("Default")),
        leveldb(data.join("cef")),
        leveldb(data.join("Default")),
        leveldb(data.clone()),
        leveldb(webview.join("Default")),
    ]
}

/// What [`find_local_storage`] found.
enum Storage {
    /// A database that has the app's keys, as the chosen origin's items.
    Read(PathBuf, BTreeMap<String, String>),
    /// No database, or only ones without the app's keys.
    Missing,
    /// A database is there, but none could be read.
    Unreadable,
}

/// The first readable localStorage among [`local_storage_dirs`] that has
/// the app's keys.
fn find_local_storage(paths: &Paths) -> Storage {
    let mut seen = false;
    let mut opened = false;
    for dir in local_storage_dirs(paths) {
        if !dir.join("CURRENT").is_file() {
            continue;
        }
        seen = true;
        let Ok(origins) = read_local_storage(&dir) else {
            continue;
        };
        opened = true;
        if let Some(items) = pick_origin(origins) {
            return Storage::Read(dir, items);
        }
    }
    if seen && !opened {
        Storage::Unreadable
    } else {
        Storage::Missing
    }
}

/// The keys that say an origin is the app's.
const APP_KEYS: [&str; 3] = [
    "theme",
    "sound-settings-storage",
    "rm-voice-switch-skip-confirm",
];

/// The app's origin: the one with the most [`APP_KEYS`], `tauri.localhost`
/// winning a tie. `None` when no origin has any.
fn pick_origin(
    origins: BTreeMap<String, BTreeMap<String, String>>,
) -> Option<BTreeMap<String, String>> {
    origins
        .into_iter()
        .map(|(origin, items)| {
            let hits = APP_KEYS.iter().filter(|k| items.contains_key(**k)).count();
            let tauri = origin.contains("tauri.localhost");
            ((hits, tauri), items)
        })
        .filter(|((hits, _), _)| *hits > 0)
        .max_by_key(|(rank, _)| *rank)
        .map(|(_, items)| items)
}

/// One Chromium localStorage string: a format byte, then UTF-16LE (0) or
/// Latin-1 (1).
fn storage_string(bytes: &[u8]) -> Option<String> {
    let (format, rest) = bytes.split_first()?;
    match format {
        0 => {
            if rest.len() % 2 != 0 {
                return None;
            }
            let units: Vec<u16> = rest
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16(&units).ok()
        }
        1 => Some(rest.iter().map(|&b| char::from(b)).collect()),
        _ => None,
    }
}

/// Every origin's localStorage items in a Chromium `Local Storage\leveldb`
/// folder: keys `_<origin>\0<key>` with [`storage_string`] keys and values.
/// The files are read into memory (the `LOCK` file skipped, so a running
/// browser does not matter), and the database is opened there: nothing on
/// disk is written.
pub fn read_local_storage(
    dir: &Path,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, String> {
    use rusty_leveldb::env::Env;
    use rusty_leveldb::{LdbIterator, MemEnv, Options, DB};
    use std::io::Write;

    const NAME: &str = "localstorage";
    let env = MemEnv::new();
    let entries = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name == "LOCK" || !entry.path().is_file() {
            continue;
        }
        let bytes = std::fs::read(entry.path()).map_err(|e| e.to_string())?;
        let path = Path::new(NAME).join(&name);
        let mut file = env.open_writable_file(&path).map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    let options = Options {
        env: std::rc::Rc::new(Box::new(env)),
        create_if_missing: false,
        ..Options::default()
    };
    let mut db = DB::open(NAME, options).map_err(|e| e.to_string())?;
    let mut iter = db.new_iter().map_err(|e| e.to_string())?;
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    while let Some((key, value)) = iter.next() {
        let Some(rest) = key.strip_prefix(b"_") else {
            continue;
        };
        let Some(split) = rest.iter().position(|&b| b == 0) else {
            continue;
        };
        let origin = String::from_utf8_lossy(&rest[..split]).into_owned();
        let (Some(key), Some(value)) = (storage_string(&rest[split + 1..]), storage_string(&value))
        else {
            continue;
        };
        out.entry(origin).or_default().insert(key, value);
    }
    Ok(out)
}

/// A Chromium localStorage database in `dir` holding `items` for `origin`
/// (keys and values Latin-1 when they fit, else UTF-16), for tests and
/// fixtures. Built in memory and copied out, file by file.
pub fn write_local_storage_fixture(
    dir: &Path,
    origin: &str,
    items: &[(&str, &str)],
) -> Result<(), String> {
    use rusty_leveldb::env::Env;
    use rusty_leveldb::{MemEnv, Options, DB};
    use std::io::Read;

    fn encode(text: &str) -> Vec<u8> {
        if text.chars().all(|c| (c as u32) < 256) {
            std::iter::once(1u8)
                .chain(text.chars().map(|c| c as u8))
                .collect()
        } else {
            std::iter::once(0u8)
                .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
                .collect()
        }
    }

    const NAME: &str = "fixture";
    let env = std::rc::Rc::new(Box::new(MemEnv::new()) as Box<dyn Env>);
    let options = Options {
        env: env.clone(),
        ..Options::default()
    };
    let mut db = DB::open(NAME, options).map_err(|e| e.to_string())?;
    db.put(b"VERSION", b"1").map_err(|e| e.to_string())?;
    for (key, value) in items {
        let mut k = format!("_{origin}").into_bytes();
        k.push(0);
        k.extend(encode(key));
        db.put(&k, &encode(value)).map_err(|e| e.to_string())?;
    }
    // A table file, as Chromium's compacted databases have, besides the log.
    db.flush().map_err(|e| e.to_string())?;
    db.close().map_err(|e| e.to_string())?;
    drop(db);

    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    for path in env.children(Path::new(NAME)).map_err(|e| e.to_string())? {
        let Some(name) = path.file_name() else {
            continue;
        };
        let full = Path::new(NAME).join(name);
        let mut bytes = Vec::new();
        env.open_sequential_file(&full)
            .and_then(|mut f| f.read_to_end(&mut bytes).map_err(Into::into))
            .map_err(|e| e.to_string())?;
        std::fs::write(dir.join(name), bytes).map_err(|e| e.to_string())?;
    }
    Ok(())
}
