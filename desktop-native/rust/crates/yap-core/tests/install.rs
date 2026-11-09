//! The installed identity and the installer hooks' ownership rules
//! (`yap_core::install`): which schemes, toast identity and autostart
//! values belong to an install root. The hooks run against
//! `MemoryRegistry`, never the real registry.

use yap_core::install::{
    command_exe, command_is_ours, hook_may_take_scheme, is_released_install, is_run_value,
    is_within, legacy_run_values, may_delete_run_value, may_register_at_launch, may_take_scheme,
    may_write_run_value, register_aumid, register_schemes, scheme_command_key, unregister,
    velopack_root, MemoryRegistry, Registry, AUMID, AUMID_DISPLAY_NAME, AUMID_KEY,
    AUMID_OWNER_VALUE, EXE_NAME, LEGACY_AUMID, LEGACY_EXE_NAME, LEGACY_PACK_ID, LEGACY_RUN_VALUE,
    PACK_ID, RUN_KEY, RUN_VALUE, TITLE,
};
use yap_core::links::{self, open_command, scheme_registration, ALIAS_SCHEME_KEY, SCHEME_KEY};

const ROOT: &str = r"C:\Users\u\AppData\Local\Yap.App";
const EXE: &str = r"C:\Users\u\AppData\Local\Yap.App\current\Yap.exe";
const TAURI_STUB: &str = r"C:\Users\u\AppData\Local\Yap\Yap.exe";

// The identity

#[test]
fn the_identity_should_be_yap_beside_the_tauri_install() {
    assert_eq!(PACK_ID, "Yap.App");
    assert_eq!(EXE_NAME, "Yap.exe");
    assert_eq!(TITLE, "Yap");
    assert_eq!(AUMID, "com.115jon.yap");
    assert_eq!(
        AUMID_KEY,
        format!(r"Software\Classes\AppUserModelId\{AUMID}")
    );
    assert_eq!(AUMID_DISPLAY_NAME, "Yap");
    assert_eq!(RUN_VALUE, "Yap");
    assert_eq!(links::SCHEME, "yap");
    assert_eq!(links::ALIAS_SCHEME, "yap-native");
    assert_eq!(SCHEME_KEY, r"Software\Classes\yap");
    assert_eq!(ALIAS_SCHEME_KEY, r"Software\Classes\yap-native");
    // Velopack installs to %LOCALAPPDATA%\<packId>; the Tauri app lives in
    // %LOCALAPPDATA%\Yap and the data root is %LOCALAPPDATA%\Yap.Data:
    // three different folders, none inside another.
    let tauri = r"C:\Users\u\AppData\Local\Yap";
    let data = r"C:\Users\u\AppData\Local\Yap.Data";
    for (a, b) in [(ROOT, tauri), (ROOT, data), (tauri, data)] {
        assert!(!is_within(a, b) && !is_within(b, a), "{a} / {b}");
    }
    assert_eq!(yap_cache::profile::DATA_DIR, "Yap.Data");
    assert_ne!(PACK_ID, yap_cache::profile::DATA_DIR);
}

#[test]
fn the_old_identity_should_be_kept_for_the_migration() {
    assert_eq!(LEGACY_PACK_ID, "YapNative.App");
    assert_eq!(LEGACY_EXE_NAME, "YapNative.exe");
    assert_eq!(LEGACY_AUMID, "com.115jon.yap.native");
    assert_eq!(LEGACY_RUN_VALUE, "YapNative");
    assert_eq!(yap_cache::profile::LEGACY_DATA_DIR, "YapNative");
}

// Command lines and paths

#[test]
fn command_exe_should_take_the_quoted_first_token() {
    assert_eq!(command_exe(&format!("\"{EXE}\" \"%1\"")), Some(EXE));
}

#[test]
fn command_exe_should_take_a_bare_first_token() {
    assert_eq!(
        command_exe(r"C:\x\Yap.exe --autostart"),
        Some(r"C:\x\Yap.exe")
    );
}

#[test]
fn command_exe_should_reject_an_unterminated_quote() {
    assert_eq!(command_exe(r#""C:\x\Yap.exe --autostart"#), None);
}

#[test]
fn command_exe_should_reject_an_empty_command() {
    assert_eq!(command_exe("   "), None);
}

#[test]
fn is_within_should_ignore_case_and_separators() {
    assert!(is_within(
        "c:/users/u/appdata/local/yap.app/current/Yap.exe",
        &format!("{ROOT}\\")
    ));
}

#[test]
fn is_within_should_reject_a_sibling_with_the_same_prefix() {
    assert!(!is_within(
        r"C:\Users\u\AppData\Local\Yap.AppX\current\Yap.exe",
        ROOT
    ));
    // The Tauri root is a prefix of ours in name only.
    assert!(!is_within(EXE, r"C:\Users\u\AppData\Local\Yap"));
}

#[test]
fn is_within_should_reject_an_empty_root() {
    assert!(!is_within(EXE, ""));
}

#[test]
fn command_is_ours_should_accept_the_installed_open_command() {
    assert!(command_is_ours(&open_command(EXE), ROOT));
}

#[test]
fn command_is_ours_should_reject_a_dev_build_and_the_tauri_app() {
    assert!(!command_is_ours(r#""F:\ct\yapn\rel\Yap.exe" "%1""#, ROOT));
    assert!(!command_is_ours(&open_command(TAURI_STUB), ROOT));
}

#[test]
fn may_take_scheme_should_allow_an_unregistered_scheme() {
    assert!(may_take_scheme(None, ROOT, |_| true));
}

#[test]
fn may_take_scheme_should_keep_a_live_build_elsewhere() {
    assert!(!may_take_scheme(
        Some(r#""F:\ct\yapn\rel\Yap.exe" "%1""#),
        ROOT,
        |_| true
    ));
}

#[test]
fn may_take_scheme_should_replace_a_missing_executable() {
    assert!(may_take_scheme(
        Some(r#""F:\gone\Yap.exe" "%1""#),
        ROOT,
        |_| false
    ));
}

#[test]
fn may_take_scheme_should_rewrite_its_own_registration() {
    assert!(may_take_scheme(Some(&open_command(EXE)), ROOT, |_| true));
}

// Run values

#[test]
fn is_run_value_should_match_ours_and_the_names_before_the_rename() {
    for name in [
        "Yap",
        "yap",
        "Yap-spike",
        "YapNative",
        "YapNative-spike",
        "yapnative-jon",
    ] {
        assert!(is_run_value(name), "{name}");
    }
}

#[test]
fn is_run_value_should_reject_other_names() {
    for name in [
        "Yap-",
        "YapNative-",
        "Yap-a_b",
        "YapX",
        "Yapper",
        "Discord",
        "",
    ] {
        assert!(!is_run_value(name), "{name}");
    }
}

#[test]
fn the_released_install_should_take_the_tauri_apps_run_value() {
    let tauri = format!("\"{TAURI_STUB}\" --autostart");
    assert!(may_write_run_value(Some(&tauri), EXE, true, |_| true));
}

#[test]
fn a_dev_build_should_never_take_a_live_apps_run_value() {
    let dev = r"F:\ct\yapn\rel\Yap.exe";
    let tauri = format!("\"{TAURI_STUB}\" --autostart");
    assert!(!may_write_run_value(Some(&tauri), dev, false, |_| true));
    // Free, its own, or pointing at a missing executable: fine.
    assert!(may_write_run_value(None, dev, false, |_| true));
    assert!(may_write_run_value(Some(""), dev, false, |_| true));
    assert!(may_write_run_value(
        Some(&format!("\"{dev}\" --autostart")),
        dev,
        false,
        |_| true
    ));
    assert!(may_write_run_value(Some(&tauri), dev, false, |_| false));
}

#[test]
fn autostart_off_should_only_delete_its_own_value() {
    assert!(may_delete_run_value(&format!("\"{EXE}\" --autostart"), EXE));
    assert!(may_delete_run_value(
        &format!("\"{}\" --autostart", EXE.to_lowercase()),
        EXE
    ));
    assert!(!may_delete_run_value(
        &format!("\"{TAURI_STUB}\" --autostart"),
        EXE
    ));
    assert!(!may_delete_run_value("", EXE));
}

// Who registers the schemes: the released install only

/// `Update.exe` next to `current` for the roots listed.
fn updaters(roots: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |path: &str| {
        roots
            .iter()
            .any(|root| path.eq_ignore_ascii_case(&format!(r"{root}\Update.exe")))
    }
}

const TEST_ROOT: &str = r"C:\Users\u\AppData\Local\Yap.InstallTest";
const TEST_EXE: &str = r"C:\Users\u\AppData\Local\Yap.InstallTest\current\Yap.exe";
const DEV_EXE: &str = r"F:\ct\yapn\rel\Yap.exe";
const OLD_ROOT: &str = r"C:\Users\u\AppData\Local\YapNative.App";
const OLD_EXE: &str = r"C:\Users\u\AppData\Local\YapNative.App\current\YapNative.exe";

#[test]
fn velopack_root_should_need_current_and_update_exe() {
    let has = updaters(&[ROOT, TEST_ROOT]);
    assert_eq!(velopack_root(EXE, &has).as_deref(), Some(ROOT));
    assert_eq!(velopack_root(TEST_EXE, &has).as_deref(), Some(TEST_ROOT));
    // No Update.exe: a copy of the layout, not an install.
    assert_eq!(velopack_root(EXE, |_| false), None);
    // Not under `current`.
    assert_eq!(velopack_root(DEV_EXE, &has), None);
    assert_eq!(
        velopack_root(r"C:\Users\u\AppData\Local\Yap.App\Yap.exe", &has),
        None
    );
    assert_eq!(velopack_root(r"current\Yap.exe", |_| true), None);
    // The Tauri layout (stub at the root, Update.exe beside it) is not one.
    assert_eq!(velopack_root(TAURI_STUB, |_| true), None);
}

#[test]
fn only_the_released_pack_id_should_be_the_released_install() {
    let has = updaters(&[ROOT, TEST_ROOT, OLD_ROOT]);
    assert!(is_released_install(EXE, &has));
    assert!(is_released_install(
        "c:/users/u/appdata/local/yap.app/CURRENT/Yap.exe",
        |p| p.eq_ignore_ascii_case(r"c:\users\u\appdata\local\yap.app\Update.exe")
    ));
    assert!(!is_released_install(TEST_EXE, &has));
    assert!(!is_released_install(DEV_EXE, &has));
    // The install from before the rename is another app now.
    assert!(!is_released_install(OLD_EXE, &has));
    assert!(!is_released_install(
        r"F:\ct\yapn\mine\YapNativeMine.exe",
        &has
    ));
    // A folder that only ends in the pack id is not it.
    assert!(!is_released_install(
        r"C:\x\NotYap.App\current\Yap.exe",
        |_| true
    ));
}

#[test]
fn launch_registration_should_be_the_released_default_profile_only() {
    let has = updaters(&[ROOT, TEST_ROOT]);
    assert!(may_register_at_launch(EXE, true, &has));
    // A named profile of the installed app.
    assert!(!may_register_at_launch(EXE, false, &has));
    // A dev build, a profile build, a copy, a test install: never.
    assert!(!may_register_at_launch(DEV_EXE, true, &has));
    assert!(!may_register_at_launch(DEV_EXE, false, &has));
    assert!(!may_register_at_launch(TEST_EXE, true, &has));
    assert!(!may_register_at_launch(EXE, true, |_| false));
}

#[test]
fn the_released_hook_should_take_the_schemes_back_from_a_dev_build_or_the_tauri_app() {
    let dev = format!("\"{DEV_EXE}\" \"%1\"");
    assert!(hook_may_take_scheme(ROOT, Some(&dev), |_| true));
    assert!(hook_may_take_scheme(
        ROOT,
        Some(&open_command(TAURI_STUB)),
        |_| true
    ));
    assert!(hook_may_take_scheme(ROOT, None, |_| true));
    assert!(hook_may_take_scheme(
        &format!("{ROOT}\\"),
        Some(&dev),
        |_| true
    ));
}

#[test]
fn a_test_install_hook_should_leave_a_live_copy_alone() {
    let dev = format!("\"{DEV_EXE}\" \"%1\"");
    let released = format!("\"{EXE}\" \"%1\"");
    assert!(!hook_may_take_scheme(TEST_ROOT, Some(&dev), |_| true));
    assert!(!hook_may_take_scheme(TEST_ROOT, Some(&released), |_| true));
    assert!(!hook_may_take_scheme(
        TEST_ROOT,
        Some(&open_command(TAURI_STUB)),
        |_| true
    ));
    // Free, dangling or already its own: taken.
    assert!(hook_may_take_scheme(TEST_ROOT, None, |_| true));
    assert!(hook_may_take_scheme(TEST_ROOT, Some(&dev), |_| false));
    let own = format!("\"{TEST_EXE}\" \"%1\"");
    assert!(hook_may_take_scheme(TEST_ROOT, Some(&own), |_| true));
}

// The hooks, on a registry in memory

fn command_of(reg: &MemoryRegistry, key: &str) -> Option<String> {
    reg.read(&scheme_command_key(key), "")
}

#[test]
fn the_install_hook_should_take_yap_from_the_tauri_app_and_add_the_alias() {
    let mut reg = MemoryRegistry::new();
    for v in links::scheme_values(links::SCHEME, TAURI_STUB) {
        reg.write(&v.key, &v.name, &v.data);
    }

    let log = register_schemes(&mut reg, ROOT, EXE, |_| true);

    assert_eq!(command_of(&reg, SCHEME_KEY), Some(open_command(EXE)));
    assert_eq!(command_of(&reg, ALIAS_SCHEME_KEY), Some(open_command(EXE)));
    assert_eq!(log.len(), 2, "{log:?}");
    // Again: the same values, nothing else.
    let keys = reg.keys();
    register_schemes(&mut reg, ROOT, EXE, |_| true);
    assert_eq!(reg.keys(), keys);
}

#[test]
fn a_test_install_hook_should_not_take_yap_from_the_live_tauri_app() {
    let mut reg = MemoryRegistry::new();
    for v in links::scheme_values(links::SCHEME, TAURI_STUB) {
        reg.write(&v.key, &v.name, &v.data);
    }
    register_schemes(&mut reg, TEST_ROOT, TEST_EXE, |_| true);
    assert_eq!(command_of(&reg, SCHEME_KEY), Some(open_command(TAURI_STUB)));
    // The alias was free.
    assert_eq!(
        command_of(&reg, ALIAS_SCHEME_KEY),
        Some(open_command(TEST_EXE))
    );
}

#[test]
fn the_aumid_key_should_be_created_once_with_its_owner() {
    let mut reg = MemoryRegistry::new();
    assert_eq!(register_aumid(&mut reg, ROOT), "aumid: created (true)");
    assert_eq!(reg.read(AUMID_KEY, "DisplayName").as_deref(), Some("Yap"));
    assert_eq!(
        reg.read(AUMID_KEY, AUMID_OWNER_VALUE).as_deref(),
        Some(ROOT)
    );
    // A key that is there already is left as it is.
    let mut dev = MemoryRegistry::new();
    dev.write(AUMID_KEY, "DisplayName", "Yap (dev)");
    assert!(register_aumid(&mut dev, ROOT).contains("left alone"));
    assert_eq!(dev.read(AUMID_KEY, AUMID_OWNER_VALUE), None);
}

#[test]
fn the_uninstall_hook_should_remove_only_what_points_into_its_root() {
    let mut reg = MemoryRegistry::new();
    for v in scheme_registration(EXE) {
        reg.write(&v.key, &v.name, &v.data);
    }
    register_aumid(&mut reg, ROOT);
    let ours = format!("\"{EXE}\" --autostart");
    reg.write(RUN_KEY, "Yap", &ours);
    reg.write(RUN_KEY, "Yap-spike", &format!("{ours} --profile spike"));
    reg.write(
        RUN_KEY,
        "YapNative-jon",
        &format!("\"{EXE}\" --autostart --profile jon"),
    );
    // Not ours: the old install's value, a dev build's, another app's.
    reg.write(RUN_KEY, "YapNative", &format!("\"{OLD_EXE}\" --autostart"));
    reg.write(RUN_KEY, "Yap-dev", &format!("\"{DEV_EXE}\" --autostart"));
    reg.write(RUN_KEY, "Discord", &format!("\"{EXE}\""));

    let out = unregister(&mut reg, ROOT);

    assert!(out.aumid_removed);
    assert!(!reg.key_exists(SCHEME_KEY));
    assert!(!reg.key_exists(ALIAS_SCHEME_KEY));
    assert!(!reg.key_exists(AUMID_KEY));
    for gone in ["Yap", "Yap-spike", "YapNative-jon"] {
        assert_eq!(reg.read(RUN_KEY, gone), None, "{gone}");
    }
    for kept in ["YapNative", "Yap-dev", "Discord"] {
        assert!(reg.read(RUN_KEY, kept).is_some(), "{kept}");
    }
}

#[test]
fn the_uninstall_hook_should_keep_the_tauri_apps_registrations() {
    let mut reg = MemoryRegistry::new();
    for v in links::scheme_values(links::SCHEME, TAURI_STUB) {
        reg.write(&v.key, &v.name, &v.data);
    }
    reg.write(RUN_KEY, "Yap", &format!("\"{TAURI_STUB}\" --autostart"));
    reg.write(AUMID_KEY, "DisplayName", "Yap");

    let out = unregister(&mut reg, ROOT);

    assert!(!out.aumid_removed);
    assert_eq!(command_of(&reg, SCHEME_KEY), Some(open_command(TAURI_STUB)));
    assert!(reg.read(RUN_KEY, "Yap").is_some());
    assert!(reg.key_exists(AUMID_KEY));
}

#[test]
fn only_the_old_installs_run_values_are_removed_at_launch() {
    let ours = format!("\"{EXE}\" --autostart");
    let old = format!("\"{OLD_EXE}\" --autostart");
    let values = vec![
        ("YapNative".to_owned(), old.clone()),
        ("YapNative-jon".to_owned(), format!("{old} --profile jon")),
        // A dev build's old-named value, and today's names: kept.
        (
            "YapNative-spike".to_owned(),
            format!("\"{DEV_EXE}\" --autostart"),
        ),
        ("Yap".to_owned(), ours),
        ("Yap-jon".to_owned(), old.clone()),
        ("Discord".to_owned(), old),
    ];
    assert_eq!(
        legacy_run_values(&values, OLD_ROOT),
        vec!["YapNative".to_owned(), "YapNative-jon".to_owned()]
    );
    assert!(legacy_run_values(&values, ROOT).is_empty());
}
