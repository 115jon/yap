//! The installer hooks' ownership rules (`yap_core::install`): which
//! scheme, toast identity and autostart values belong to an install root.

use yap_core::install::{
    command_exe, command_is_ours, hook_may_take_scheme, is_released_install, is_run_value,
    is_within, may_register_at_launch, may_take_scheme, velopack_root,
};

const ROOT: &str = r"C:\Users\u\AppData\Local\YapNative.App";
const EXE: &str = r"C:\Users\u\AppData\Local\YapNative.App\current\YapNative.exe";

#[test]
fn command_exe_should_take_the_quoted_first_token() {
    assert_eq!(command_exe(&format!("\"{EXE}\" \"%1\"")), Some(EXE));
}

#[test]
fn command_exe_should_take_a_bare_first_token() {
    assert_eq!(
        command_exe(r"C:\x\YapNative.exe --autostart"),
        Some(r"C:\x\YapNative.exe")
    );
}

#[test]
fn command_exe_should_reject_an_unterminated_quote() {
    assert_eq!(command_exe(r#""C:\x\YapNative.exe --autostart"#), None);
}

#[test]
fn command_exe_should_reject_an_empty_command() {
    assert_eq!(command_exe("   "), None);
}

#[test]
fn is_within_should_ignore_case_and_separators() {
    assert!(is_within(
        "c:/users/u/appdata/local/yapnative.app/current/YapNative.exe",
        &format!("{ROOT}\\")
    ));
}

#[test]
fn is_within_should_reject_a_sibling_with_the_same_prefix() {
    assert!(!is_within(
        r"C:\Users\u\AppData\Local\YapNative.AppX\current\YapNative.exe",
        ROOT
    ));
}

#[test]
fn is_within_should_reject_an_empty_root() {
    assert!(!is_within(EXE, ""));
}

#[test]
fn command_is_ours_should_accept_the_installed_open_command() {
    assert!(command_is_ours(&yap_core::links::open_command(EXE), ROOT));
}

#[test]
fn command_is_ours_should_reject_a_dev_build() {
    assert!(!command_is_ours(
        r#""F:\ct\yapn\rel\YapNative.exe" "%1""#,
        ROOT
    ));
}

#[test]
fn may_take_scheme_should_allow_an_unregistered_scheme() {
    assert!(may_take_scheme(None, ROOT, |_| true));
}

#[test]
fn may_take_scheme_should_keep_a_live_build_elsewhere() {
    assert!(!may_take_scheme(
        Some(r#""F:\ct\yapn\rel\YapNative.exe" "%1""#),
        ROOT,
        |_| true
    ));
}

#[test]
fn may_take_scheme_should_replace_a_missing_executable() {
    assert!(may_take_scheme(
        Some(r#""F:\gone\YapNative.exe" "%1""#),
        ROOT,
        |_| false
    ));
}

#[test]
fn may_take_scheme_should_rewrite_its_own_registration() {
    assert!(may_take_scheme(
        Some(&yap_core::links::open_command(EXE)),
        ROOT,
        |_| true
    ));
}

#[test]
fn is_run_value_should_match_the_default_value() {
    assert!(is_run_value("YapNative"));
}

#[test]
fn is_run_value_should_match_a_profile_value() {
    assert!(is_run_value("YapNative-spike"));
}

#[test]
fn is_run_value_should_reject_the_tauri_app() {
    assert!(!is_run_value("Yap"));
}

#[test]
fn is_run_value_should_reject_an_invalid_profile() {
    assert!(!is_run_value("YapNative-"));
}

// Who registers the scheme: the released install only

/// `Update.exe` next to `current` for the roots listed.
fn updaters(roots: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |path: &str| {
        roots
            .iter()
            .any(|root| path.eq_ignore_ascii_case(&format!(r"{root}\Update.exe")))
    }
}

const TEST_ROOT: &str = r"C:\Users\u\AppData\Local\YapNative.InstallTest";
const TEST_EXE: &str = r"C:\Users\u\AppData\Local\YapNative.InstallTest\current\YapNative.exe";
const DEV_EXE: &str = r"F:\ct\yapn\rel\YapNative.exe";

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
        velopack_root(
            r"C:\Users\u\AppData\Local\YapNative.App\YapNative.exe",
            &has
        ),
        None
    );
    assert_eq!(velopack_root(r"current\YapNative.exe", |_| true), None);
}

#[test]
fn only_the_released_pack_id_should_be_the_released_install() {
    let has = updaters(&[ROOT, TEST_ROOT]);
    assert!(is_released_install(EXE, &has));
    assert!(is_released_install(
        "c:/users/u/appdata/local/yapnative.app/CURRENT/YapNative.exe",
        |p| p.eq_ignore_ascii_case(r"c:\users\u\appdata\local\yapnative.app\Update.exe")
    ));
    assert!(!is_released_install(TEST_EXE, &has));
    assert!(!is_released_install(DEV_EXE, &has));
    assert!(!is_released_install(
        r"F:\ct\yapn\mine\YapNativeMine.exe",
        &has
    ));
    // A folder that only ends in the pack id is not it.
    assert!(!is_released_install(
        r"C:\x\NotYapNative.App\current\YapNative.exe",
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
fn the_released_hook_should_take_the_scheme_back_from_a_live_dev_build() {
    let dev = format!("\"{DEV_EXE}\" \"%1\"");
    assert!(hook_may_take_scheme(ROOT, Some(&dev), |_| true));
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
    // Free, dangling or already its own: taken.
    assert!(hook_may_take_scheme(TEST_ROOT, None, |_| true));
    assert!(hook_may_take_scheme(TEST_ROOT, Some(&dev), |_| false));
    let own = format!("\"{TEST_EXE}\" \"%1\"");
    assert!(hook_may_take_scheme(TEST_ROOT, Some(&own), |_| true));
}
