//! Replacing the Tauri desktop app (`yap_core::tauri`): detection, the
//! shortcut and registry clean-up around its uninstaller, and the import of
//! its preferences. Fixture folders and an in-memory registry only: the
//! real registry, the real install and the real data are never touched.

use std::fs;
use std::path::{Path, PathBuf};

use yap_cache::settings;
use yap_core::install::{MemoryRegistry, Registry, RUN_KEY};
use yap_core::links;
use yap_core::tauri::{
    detect, import, local_storage_dirs, read_local_storage, remove_leftovers, restore_shortcuts,
    save_shortcuts, shortcut_points_into, write_local_storage_fixture, Paths, IMPORT_ATTEMPTS,
    IMPORT_MARKER, UNINSTALL_KEY,
};
use yap_core::tauri::{load_persisted_shortcuts, persist_shortcuts};

/// Every rect on one monitor at `scale`.
fn at(scale: f64) -> impl Fn(i32, i32, i32, i32) -> Option<f64> {
    move |_, _, _, _| Some(scale)
}

struct Fixture {
    _dir: tempfile::TempDir,
    paths: Paths,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("dir");
        let base = dir.path();
        let paths = Paths {
            local_app_data: base.join("Local"),
            roaming_app_data: base.join("Roaming"),
            desktop: base.join("Desktop"),
            programs: base.join("Programs"),
        };
        for p in [
            &paths.local_app_data,
            &paths.roaming_app_data,
            &paths.desktop,
            &paths.programs,
        ] {
            fs::create_dir_all(p).expect("mkdir");
        }
        Self { _dir: dir, paths }
    }

    fn tauri_root(&self) -> PathBuf {
        self.paths.local_app_data.join("Yap")
    }

    fn our_root(&self) -> PathBuf {
        self.paths.local_app_data.join("Yap.App")
    }

    fn native_root(&self) -> PathBuf {
        self.paths.local_app_data.join("Yap.Data")
    }

    fn tauri_data(&self) -> PathBuf {
        self.paths.roaming_app_data.join("Yap")
    }

    /// The Tauri installer's layout (`desktop/installer`).
    fn install_tauri(&self) {
        let root = self.tauri_root();
        fs::create_dir_all(root.join("app-3.13.0")).expect("mkdir");
        fs::write(root.join("Yap.exe"), b"stub").expect("write");
        fs::write(root.join("Update.exe"), b"setup").expect("write");
        fs::write(root.join("current.json"), r#"{"currentVersion":"3.13.0"}"#).expect("write");
    }

    /// A shortcut file whose target is `target` (UTF-16, as a .lnk keeps it).
    fn shortcut(&self, dir: &Path, target: &Path) -> PathBuf {
        let mut bytes = b"L\0\0\0 fake lnk header ".to_vec();
        bytes.extend(
            target
                .to_string_lossy()
                .encode_utf16()
                .flat_map(u16::to_le_bytes),
        );
        let path = dir.join("Yap.lnk");
        fs::write(&path, bytes).expect("write");
        path
    }
}

fn command(exe: &Path) -> String {
    format!("\"{}\" --autostart", exe.display())
}

// Detection

#[test]
fn a_tauri_install_is_found_with_its_uninstall_entry() {
    let fx = Fixture::new();
    fx.install_tauri();
    let mut reg = MemoryRegistry::new();
    reg.write(
        UNINSTALL_KEY,
        "InstallLocation",
        &fx.tauri_root().to_string_lossy(),
    );
    reg.write(UNINSTALL_KEY, "DisplayVersion", "3.13.0");

    let found = detect(&reg, &fx.paths).expect("found");

    assert_eq!(found.root, fx.tauri_root());
    assert_eq!(found.uninstaller, fx.tauri_root().join("Update.exe"));
    assert_eq!(found.version.as_deref(), Some("3.13.0"));
}

#[test]
fn a_tauri_install_without_its_entry_is_still_found() {
    let fx = Fixture::new();
    fx.install_tauri();
    assert!(detect(&MemoryRegistry::new(), &fx.paths).is_some());
}

#[test]
fn nothing_is_found_without_an_install() {
    let fx = Fixture::new();
    assert_eq!(detect(&MemoryRegistry::new(), &fx.paths), None);
    // A folder with only data in it, no uninstaller.
    fs::create_dir_all(fx.tauri_root().join("logs")).expect("mkdir");
    assert_eq!(detect(&MemoryRegistry::new(), &fx.paths), None);
}

#[test]
fn a_velopack_layout_named_yap_is_never_taken_for_the_tauri_app() {
    let fx = Fixture::new();
    let root = fx.tauri_root();
    fs::create_dir_all(root.join("current")).expect("mkdir");
    fs::write(root.join("Update.exe"), b"velopack").expect("write");
    fs::write(root.join("current.json"), b"{}").expect("write");
    assert_eq!(detect(&MemoryRegistry::new(), &fx.paths), None);
}

#[test]
fn an_uninstall_entry_naming_another_folder_is_not_trusted() {
    let fx = Fixture::new();
    fx.install_tauri();
    let mut reg = MemoryRegistry::new();
    reg.write(UNINSTALL_KEY, "InstallLocation", r"D:\Elsewhere\Yap");
    assert_eq!(detect(&reg, &fx.paths), None);
}

// Shortcuts

#[test]
fn a_shortcut_is_ours_when_it_names_our_root_in_either_encoding() {
    let root = Path::new(r"C:\Users\u\AppData\Local\Yap.App");
    let wide: Vec<u8> = r"c:\users\u\appdata\local\yap.app\current\Yap.exe"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    assert!(shortcut_points_into(&wide, root));
    assert!(shortcut_points_into(
        br"xx C:\USERS\U\APPDATA\LOCAL\YAP.APP\Yap.exe xx",
        root
    ));
    // The Tauri app's own shortcut: its root, not ours.
    assert!(!shortcut_points_into(
        br"C:\Users\u\AppData\Local\Yap\Yap.exe",
        root
    ));
}

#[test]
fn our_shortcuts_come_back_after_the_tauri_uninstaller_deletes_them() {
    let fx = Fixture::new();
    let exe = fx.our_root().join("current").join("Yap.exe");
    let desktop = fx.shortcut(&fx.paths.desktop, &exe);
    let start = fx.shortcut(&fx.paths.programs, &exe);
    let bytes = fs::read(&desktop).expect("read");

    let saved = save_shortcuts(&fx.paths, &fx.our_root());
    assert_eq!(saved.len(), 2);
    // What the Tauri uninstaller does: both Yap.lnk, by name.
    fs::remove_file(&desktop).expect("rm");
    fs::remove_file(&start).expect("rm");

    let log = restore_shortcuts(&saved);

    assert_eq!(fs::read(&desktop).expect("read"), bytes);
    assert!(start.is_file());
    assert!(log.iter().all(|l| l.ends_with("restored")), "{log:?}");
    // Run again: nothing to do.
    assert!(restore_shortcuts(&saved)
        .iter()
        .all(|l| l.ends_with("kept")));
}

#[test]
fn the_tauri_apps_own_shortcut_is_not_saved() {
    let fx = Fixture::new();
    fx.shortcut(&fx.paths.desktop, &fx.tauri_root().join("Yap.exe"));
    assert!(save_shortcuts(&fx.paths, &fx.our_root()).is_empty());
}

// Registry leftovers

#[test]
fn leftovers_pointing_into_the_tauri_root_are_removed() {
    let fx = Fixture::new();
    let stub = fx.tauri_root().join("Yap.exe");
    let mut reg = MemoryRegistry::new();
    reg.write(RUN_KEY, "Yap", &command(&stub));
    for v in links::scheme_values(links::SCHEME, &stub.to_string_lossy()) {
        reg.write(&v.key, &v.name, &v.data);
    }

    let log = remove_leftovers(&mut reg, &fx.tauri_root());

    assert_eq!(reg.read(RUN_KEY, "Yap"), None);
    assert!(!reg.key_exists(links::SCHEME_KEY));
    assert_eq!(log.len(), 2);
}

#[test]
fn our_run_value_and_scheme_are_never_removed_as_leftovers() {
    let fx = Fixture::new();
    let ours = fx.our_root().join("current").join("Yap.exe");
    let mut reg = MemoryRegistry::new();
    reg.write(RUN_KEY, "Yap", &command(&ours));
    reg.write(RUN_KEY, "OtherApp", r#""C:\Yap\Yap.exe""#);
    for v in links::scheme_registration(&ours.to_string_lossy()) {
        reg.write(&v.key, &v.name, &v.data);
    }
    let before = reg.keys();

    remove_leftovers(&mut reg, &fx.tauri_root());
    remove_leftovers(&mut reg, &fx.tauri_root());

    assert_eq!(reg.keys(), before);
    assert_eq!(reg.read(RUN_KEY, "Yap"), Some(command(&ours)));
    assert!(reg.read(RUN_KEY, "OtherApp").is_some());
}

// localStorage

#[test]
fn local_storage_reads_back_what_chromium_writes() {
    let dir = tempfile::tempdir().expect("dir");
    let db = dir.path().join("leveldb");
    write_local_storage_fixture(
        &db,
        "http://tauri.localhost",
        &[("theme", "miku-dark"), ("wide", "voix \u{2603} snow")],
    )
    .expect("fixture");
    // A LOCK file a running browser holds is skipped.
    fs::write(db.join("LOCK"), b"").expect("write");

    let origins = read_local_storage(&db).expect("read");

    let items = &origins["http://tauri.localhost"];
    assert_eq!(items["theme"], "miku-dark");
    assert_eq!(items["wide"], "voix \u{2603} snow");
    // Read in memory: the folder is as it was.
    let mut names: Vec<String> = fs::read_dir(&db)
        .expect("ls")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert!(names.contains(&"CURRENT".to_owned()), "{names:?}");
}

// Import

const SOUNDS: &str = r#"{"state":{"currentUser":"u1","userSettings":{
    "u0":{"soundsEnabled":true,"soundVolume":100},
    "u1":{"soundsEnabled":false,"voiceJoinLeave":false,"calls":false,
          "soundVolume":42.4,"soundboardVolume":180,"customSounds":true}}},
    "version":1}"#;

fn tauri_data(fx: &Fixture) {
    let data = fx.tauri_data();
    fs::create_dir_all(&data).expect("mkdir");
    fs::write(
        data.join("desktop-settings.json"),
        r#"{"openOnStartup":true,"startMinimized":true,"closeToTray":false,
            "hardwareAcceleration":false,"desktopNotifications":false,
            "gameCaptureHook":false}"#,
    )
    .expect("write");
    fs::write(
        data.join(".window-state.json"),
        r#"{"main":{"x":300,"y":150,"width":2400,"height":1500,"maximized":true,"fullscreen":false}}"#,
    )
    .expect("write");
    let storage = &local_storage_dirs(&fx.paths)[0];
    write_local_storage_fixture(
        storage,
        "http://tauri.localhost",
        &[
            ("theme", "spiderman-dark"),
            ("sound-settings-storage", SOUNDS),
            ("rm-voice-switch-skip-confirm", "true"),
        ],
    )
    .expect("fixture");
    // Another origin (an embed) with a theme of its own is ignored.
    write_local_storage_fixture(
        &local_storage_dirs(&fx.paths)[1],
        "https://www.youtube.com",
        &[("theme", "light")],
    )
    .expect("fixture");
}

#[test]
fn the_tauri_preferences_are_imported_once() {
    let fx = Fixture::new();
    tauri_data(&fx);
    let root = fx.native_root();

    let imported = import(&fx.paths, &root, &at(1.5));

    assert!(imported.desktop_settings && imported.sounds, "{imported:?}");
    assert_eq!(imported.theme.as_deref(), Some("spiderman-dark"));
    assert!(imported.window_state);
    let s = settings::load_settings(&root);
    assert!(s.open_on_startup && s.start_minimized);
    assert!(!s.close_to_tray && !s.desktop_notifications && !s.game_capture_hook);
    assert!(!s.sounds);
    assert!(!s.sound_settings.voice_join_leave && !s.sound_settings.calls);
    assert!(s.sound_settings.mute_deafen);
    assert!(s.sound_settings.custom_sounds);
    assert_eq!(s.sound_settings.sound_volume, 42);
    assert_eq!(s.sound_settings.soundboard_volume, 100);
    assert!(s.skip_voice_switch_confirm && !s.skip_start_call_confirm);
    assert_eq!(settings::load_appearance(&root).theme, "spiderman-dark");
    let w = settings::load_window_state(&root).expect("window");
    assert_eq!(
        (w.x, w.y, w.width, w.height, w.maximized),
        (200, 100, 1600, 1000, true)
    );
    assert!(root.join(IMPORT_MARKER).is_file());

    // Once: a second run changes nothing, even after a change here.
    let mut changed = s;
    changed.close_to_tray = true;
    settings::save_settings(&root, &changed).expect("save");
    let again = import(&fx.paths, &root, &at(1.0));
    assert_eq!(again.log, vec!["import: already done".to_owned()]);
    assert!(settings::load_settings(&root).close_to_tray);
}

#[test]
fn the_import_never_overwrites_this_apps_own_settings() {
    let fx = Fixture::new();
    tauri_data(&fx);
    let root = fx.native_root();
    let mine = settings::DesktopSettings {
        close_to_tray: true,
        sounds: false,
        ..Default::default()
    };
    settings::save_settings(&root, &mine).expect("save");
    settings::save_appearance(
        &root,
        &settings::AppearancePrefs {
            theme: "miku-light".into(),
        },
    )
    .expect("save");

    let imported = import(&fx.paths, &root, &at(1.0));

    assert!(!imported.desktop_settings && !imported.sounds);
    assert_eq!(imported.theme, None);
    assert!(imported.window_state);
    assert_eq!(settings::load_settings(&root), mine);
    assert_eq!(settings::load_appearance(&root).theme, "miku-light");
}

#[test]
fn an_unreadable_local_storage_is_tried_again_before_the_rest_imports_without_it() {
    let fx = Fixture::new();
    let data = fx.tauri_data();
    fs::create_dir_all(&data).expect("mkdir");
    fs::write(
        data.join("desktop-settings.json"),
        r#"{"closeToTray":false}"#,
    )
    .expect("write");
    // A localStorage folder that is not a database.
    let storage = &local_storage_dirs(&fx.paths)[0];
    fs::create_dir_all(storage).expect("mkdir");
    fs::write(storage.join("CURRENT"), b"MANIFEST-000999\n").expect("write");
    let root = fx.native_root();

    // The first tries import nothing and leave the import open.
    for _ in 1..IMPORT_ATTEMPTS {
        let imported = import(&fx.paths, &root, &at(1.0));
        assert!(!imported.desktop_settings, "{imported:?}");
        assert!(!root.join(settings::SETTINGS_FILE).exists());
        assert!(
            imported.log.iter().any(|l| l.contains("tries again")),
            "{imported:?}"
        );
        let marker: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(root.join(IMPORT_MARKER)).expect("marker"))
                .expect("json");
        assert_eq!(marker["done"], false);
    }

    // The last one imports the plain files without it, and closes.
    let imported = import(&fx.paths, &root, &at(1.0));
    assert!(
        imported.desktop_settings && !imported.sounds,
        "{imported:?}"
    );
    assert_eq!(imported.theme, None);
    assert!(!settings::load_settings(&root).close_to_tray);
    let again = import(&fx.paths, &root, &at(1.0));
    assert_eq!(again.log, vec!["import: already done".to_owned()]);
}

#[test]
fn a_readable_local_storage_after_a_failed_try_still_imports_everything() {
    let fx = Fixture::new();
    tauri_data(&fx);
    let root = fx.native_root();
    // An earlier launch could not read it.
    fs::create_dir_all(&root).expect("mkdir");
    fs::write(root.join(IMPORT_MARKER), r#"{"done":false,"attempts":1}"#).expect("write");

    let imported = import(&fx.paths, &root, &at(1.0));

    assert!(imported.sounds && imported.desktop_settings);
    assert_eq!(imported.theme.as_deref(), Some("spiderman-dark"));
}

#[test]
fn a_window_rect_on_no_monitor_is_not_imported() {
    let fx = Fixture::new();
    tauri_data(&fx);
    let root = fx.native_root();
    let mut asked = std::cell::RefCell::new(Vec::new());
    let scale_at = |x: i32, y: i32, w: i32, h: i32| {
        asked.borrow_mut().push((x, y, w, h));
        None
    };

    let imported = import(&fx.paths, &root, &scale_at);

    assert!(!imported.window_state);
    assert!(settings::load_window_state(&root).is_none());
    // Asked with the physical rect as saved.
    assert_eq!(asked.get_mut().as_slice(), &[(300, 150, 2400, 1500)]);
    // The rest still imports.
    assert!(imported.desktop_settings && imported.sounds);
}

#[test]
fn saved_shortcuts_survive_on_disk_until_put_back() {
    let fx = Fixture::new();
    let exe = fx.our_root().join("current").join("Yap.exe");
    let desktop = fx.shortcut(&fx.paths.desktop, &exe);
    let start = fx.shortcut(&fx.paths.programs, &exe);
    let saved = save_shortcuts(&fx.paths, &fx.our_root());
    let dir = fx.native_root().join("tauri-shortcuts");

    persist_shortcuts(&saved, &dir).expect("persist");
    // The process ends mid-uninstall; the uninstaller deletes both.
    fs::remove_file(&desktop).expect("rm");
    fs::remove_file(&start).expect("rm");

    // The next launch reads them back and restores them.
    let loaded = load_persisted_shortcuts(&dir);
    assert_eq!(loaded, saved);
    restore_shortcuts(&loaded);
    assert!(desktop.is_file() && start.is_file());

    // Persisting nothing clears the folder.
    persist_shortcuts(&[], &dir).expect("persist");
    assert!(!dir.exists());
    assert!(load_persisted_shortcuts(&dir).is_empty());
}

#[test]
fn no_tauri_data_means_no_import_and_no_marker() {
    let fx = Fixture::new();
    let root = fx.native_root();
    let imported = import(&fx.paths, &root, &at(1.0));
    assert!(!imported.desktop_settings && !imported.window_state);
    assert!(!root.join(IMPORT_MARKER).exists());
    // The Tauri data is read, never changed.
    tauri_data(&fx);
    let before = fs::read(fx.tauri_data().join("desktop-settings.json")).expect("read");
    import(&fx.paths, &root, &at(1.0));
    assert_eq!(
        fs::read(fx.tauri_data().join("desktop-settings.json")).expect("read"),
        before
    );
}

// ── The deferred uninstall ──────────────────────────────────────────────

mod deferral {
    use std::time::Duration;

    use yap_core::tauri::{UninstallDeferral, UNINSTALL_DEFER};

    const S: fn(u64) -> Duration = Duration::from_secs;

    #[test]
    fn it_waits_for_the_interface_and_then_the_whole_delay() {
        assert!(
            UNINSTALL_DEFER > S(8),
            "longer than the Tauri bridge's liveness window"
        );
        let mut d = UninstallDeferral::default();
        d.arm();
        // Armed at launch, but the interface has not loaded: never.
        assert!(!d.due(S(600)));
        assert_eq!(d.wait(S(600)), None);
        d.app_started(S(3));
        assert!(!d.due(S(3)));
        assert!(!d.due(S(3) + UNINSTALL_DEFER - Duration::from_millis(1)));
        assert_eq!(d.wait(S(13)), Some(UNINSTALL_DEFER - S(10)));
        assert!(d.due(S(3) + UNINSTALL_DEFER));
    }

    #[test]
    fn it_fires_once() {
        let mut d = UninstallDeferral::default();
        d.arm();
        d.app_started(S(0));
        assert!(d.due(UNINSTALL_DEFER));
        assert!(!d.due(UNINSTALL_DEFER * 2));
        assert_eq!(d.wait(UNINSTALL_DEFER * 2), None);
    }

    #[test]
    fn a_later_start_signal_does_not_move_the_clock() {
        let mut d = UninstallDeferral::default();
        d.arm();
        d.app_started(S(1));
        d.app_started(S(100));
        assert!(d.due(S(1) + UNINSTALL_DEFER));
    }

    #[test]
    fn nothing_runs_when_it_was_never_armed() {
        // No Tauri install, or its attempts were used up.
        let mut d = UninstallDeferral::default();
        d.app_started(S(0));
        assert!(!d.due(S(3600)));
        assert_eq!(d.wait(S(3600)), None);
    }
}
