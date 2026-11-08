//! What the installer's hooks, and the app at launch, may touch
//! (desktop-native/docs/INSTALLER.md).
//!
//! Velopack installs the app under its own root
//! (`%LOCALAPPDATA%\<packId>\current\YapNative.exe`) and runs the executable
//! with `--veloapp-install`, `--veloapp-updated` or `--veloapp-uninstall`
//! around those steps. The hooks only ever change per-user values that
//! belong to that root: the `yap-native://` scheme, the default profile's
//! toast identity and the autostart values. A value another copy of the app
//! wrote (a dev build, a second install, the user's own profile) is left
//! alone, with one exception: the released install ([`PACK_ID`]) owns the
//! scheme and takes it back on install, update and every default-profile
//! launch, and no other copy writes it at launch. This module holds the
//! rules; the registry calls live in the app crate (`installer.rs`) and the
//! shell (`cpp/desktopshell.cpp`).

/// The released app's Velopack pack id: its install root is
/// `%LOCALAPPDATA%\YapNative.App`. Test installs use another id
/// (`YapNative.InstallTest`).
pub const PACK_ID: &str = "YapNative.App";

/// The default profile's toast identity key (`cpp/desktopshell.cpp`).
pub const AUMID_KEY: &str = r"Software\Classes\AppUserModelId\com.115jon.yap.native";

/// The name toasts show for the default profile.
pub const AUMID_DISPLAY_NAME: &str = "Yap Native";

/// The value the install hook adds to [`AUMID_KEY`] when it creates the key:
/// the install root. The uninstall hook deletes the key only when it is
/// there and names the same root, so a key a dev build made survives.
pub const AUMID_OWNER_VALUE: &str = "YapInstallRoot";

/// The per-user autostart key.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The default profile's autostart value; a named profile's is
/// `YapNative-<name>`.
pub const RUN_VALUE: &str = "YapNative";

/// The quoted (or bare) first token of a command line: the executable of
/// `"C:\x\YapNative.exe" "%1"` or `C:\x\YapNative.exe --autostart`. `None`
/// for an empty command or an unterminated quote.
pub fn command_exe(command: &str) -> Option<&str> {
    let command = command.trim_start();
    let exe = match command.strip_prefix('"') {
        Some(rest) => &rest[..rest.find('"')?],
        None => command.split_whitespace().next()?,
    };
    (!exe.is_empty()).then_some(exe)
}

/// Whether `path` is `root` itself or inside it. Windows paths: case and
/// separator (`\` or `/`) do not matter, and a trailing separator on either
/// is ignored. `C:\AppX` is not inside `C:\App`.
pub fn is_within(path: &str, root: &str) -> bool {
    let normalize = |p: &str| {
        p.trim()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let (path, root) = (normalize(path), normalize(root));
    if root.is_empty() {
        return false;
    }
    path == root
        || path
            .strip_prefix(&root)
            .is_some_and(|rest| rest.starts_with('\\'))
}

/// Whether two paths name the same folder, by the rules of [`is_within`].
pub fn same_path(a: &str, b: &str) -> bool {
    is_within(a, b) && is_within(b, a)
}

/// Whether a command line (an open command, a Run value) starts an
/// executable inside `root`.
pub fn command_is_ours(command: &str, root: &str) -> bool {
    command_exe(command).is_some_and(|exe| is_within(exe, root))
}

/// Whether the install hook may write the scheme, given the current open
/// command (`None` when the scheme is not registered): yes when nothing
/// owns it, when it is already ours, or when its executable is gone. A
/// live executable elsewhere keeps it.
pub fn may_take_scheme(current: Option<&str>, root: &str, exists: impl Fn(&str) -> bool) -> bool {
    match current.map(str::trim).filter(|c| !c.is_empty()) {
        None => true,
        Some(command) => match command_exe(command) {
            Some(exe) => is_within(exe, root) || !exists(exe),
            None => true,
        },
    }
}

/// Whether `name` is one of this app's autostart values: `YapNative` or
/// `YapNative-<profile>` (case-insensitive, as registry value names are).
pub fn is_run_value(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = RUN_VALUE.to_ascii_lowercase();
    lower == base
        || lower
            .strip_prefix(&base)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(yap_cache::profile::is_valid)
}

/// The folders of a Windows path, either separator, no empty parts.
fn parts(path: &str) -> Vec<&str> {
    path.split(['\\', '/']).filter(|p| !p.is_empty()).collect()
}

/// Whether the last folder of `root` is [`PACK_ID`].
fn is_released_root(root: &str) -> bool {
    parts(root)
        .last()
        .is_some_and(|name| name.eq_ignore_ascii_case(PACK_ID))
}

/// The install root of a Velopack layout: `<root>` for
/// `<root>\current\<exe>` when `<root>\Update.exe` is a file (asked through
/// `is_file`). `None` for any other path.
pub fn velopack_root(exe: &str, is_file: impl Fn(&str) -> bool) -> Option<String> {
    let parts = parts(exe);
    let [root @ .., current, _exe] = parts.as_slice() else {
        return None;
    };
    if root.is_empty() || !current.eq_ignore_ascii_case("current") {
        return None;
    }
    let root = root.join("\\");
    is_file(&format!("{root}\\Update.exe")).then_some(root)
}

/// Whether `exe` is the released install's executable: a Velopack layout
/// whose root folder is named [`PACK_ID`]. A dev build, a copy and a test
/// install are not.
pub fn is_released_install(exe: &str, is_file: impl Fn(&str) -> bool) -> bool {
    velopack_root(exe, is_file).is_some_and(|root| is_released_root(&root))
}

/// Whether a launch may (re)write the scheme: only the released install,
/// as the default profile. Every other build and profile leaves it to the
/// installed app, so a test run never takes it; a build that needs links
/// for a test registers a test scheme of its own (`links::test_scheme`).
pub fn may_register_at_launch(
    exe: &str,
    default_profile: bool,
    is_file: impl Fn(&str) -> bool,
) -> bool {
    default_profile && is_released_install(exe, is_file)
}

/// Whether the install or update hook of the install at `root` may write
/// the scheme, given the current open command (`None` when unregistered).
/// The released install always does: it owns the scheme, and this is where
/// it takes it back from a dev build that had it. Another install (a test
/// pack id) only as [`may_take_scheme`] allows.
pub fn hook_may_take_scheme(
    root: &str,
    current: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> bool {
    is_released_root(root) || may_take_scheme(current, root, exists)
}
