//! What the installer's hooks may touch (desktop-native/docs/INSTALLER.md).
//!
//! Velopack installs the app under its own root
//! (`%LOCALAPPDATA%\<packId>\current\YapNative.exe`) and runs the executable
//! with `--veloapp-install` or `--veloapp-uninstall` around those steps. The
//! hooks only ever change per-user values that belong to that root: the
//! `yap-native://` scheme, the default profile's toast identity and the
//! autostart values. A value another copy of the app wrote (a dev build, a
//! second install, the user's own profile) is left alone. This module holds
//! the rules; the registry calls live in the app crate (`installer.rs`).

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
