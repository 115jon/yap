//! The installer hooks' ownership rules (`yap_core::install`): which
//! scheme, toast identity and autostart values belong to an install root.

use yap_core::install::{command_exe, command_is_ours, is_run_value, is_within, may_take_scheme};

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
