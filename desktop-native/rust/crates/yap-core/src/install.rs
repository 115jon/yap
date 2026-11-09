//! The app's installed identity, and what the installer's hooks and the app
//! at launch may touch (desktop-native/docs/INSTALLER.md).
//!
//! Velopack installs the app under its own root
//! (`%LOCALAPPDATA%\Yap.App\current\Yap.exe`) and runs the executable with
//! `--veloapp-install`, `--veloapp-updated` or `--veloapp-uninstall` around
//! those steps. The hooks only ever change per-user values that belong to
//! that root: the `yap://` and `yap-native://` schemes, the default
//! profile's toast identity and the autostart values. A value another copy
//! of the app wrote (a dev build, a second install, the user's own profile)
//! is left alone, with one exception: the released install ([`PACK_ID`])
//! owns the schemes and takes them back on install, update and every
//! default-profile launch (from the Tauri app it replaces, too), and no
//! other copy writes them at launch. This module holds the rules and the
//! hooks' registry work behind [`Registry`]; the real registry calls live
//! in the app crate (`installer.rs`) and the shell (`cpp/desktopshell.cpp`).

use std::collections::BTreeMap;

use crate::links;

/// The released app's Velopack pack id: its install root is
/// `%LOCALAPPDATA%\Yap.App`, its uninstall entry `Uninstall\Yap.App`. Not
/// `Yap`: that is the Tauri app's install root and uninstall key (Velopack
/// installs to `%LOCALAPPDATA%\<packId>` and deletes it on uninstall), and
/// the Tauri uninstaller deletes its root too. Test installs use another
/// id (`Yap.InstallTest`).
pub const PACK_ID: &str = "Yap.App";

/// The pack id before the rename (`Yap Native`). Its install is a separate
/// Velopack app; nothing here removes it.
pub const LEGACY_PACK_ID: &str = "YapNative.App";

/// The executable: `<root>\current\Yap.exe`, the name Velopack's stub at
/// the root takes too.
pub const EXE_NAME: &str = "Yap.exe";

/// The executable's name before the rename.
pub const LEGACY_EXE_NAME: &str = "YapNative.exe";

/// The installed app's title: shortcuts, Apps & features, the tray.
pub const TITLE: &str = "Yap";

/// The default profile's AppUserModelID (shortcuts, the taskbar, toasts):
/// the Tauri app's, so pins and toast settings carry over. A named profile
/// appends `.<name>`.
pub const AUMID: &str = "com.115jon.yap";

/// The default profile's toast identity key (`cpp/desktopshell.cpp`).
pub const AUMID_KEY: &str = r"Software\Classes\AppUserModelId\com.115jon.yap";

/// The AUMID before the rename (`com.115jon.yap.native[.<name>]`).
pub const LEGACY_AUMID: &str = "com.115jon.yap.native";

/// The name toasts show for the default profile.
pub const AUMID_DISPLAY_NAME: &str = "Yap";

/// The value the install hook adds to [`AUMID_KEY`] when it creates the key:
/// the install root. The uninstall hook deletes the key only when it is
/// there and names the same root, so a key a dev build made survives.
pub const AUMID_OWNER_VALUE: &str = "YapInstallRoot";

/// The per-user autostart key.
pub const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

/// The default profile's autostart value, the Tauri app's name, which this
/// app takes over; a named profile's is `Yap-<name>`.
pub const RUN_VALUE: &str = "Yap";

/// The autostart value before the rename (`YapNative[-<name>]`).
pub const LEGACY_RUN_VALUE: &str = "YapNative";

/// A per-user registry, the little the hooks and the Tauri replacement
/// need: `REG_SZ` values under `HKEY_CURRENT_USER`, by key path. The app
/// implements it on the real registry (`installer.rs`); tests use
/// [`MemoryRegistry`]. Key paths and value names are case-insensitive, as
/// in the registry; an empty name is the key's default value.
pub trait Registry {
    fn read(&self, key: &str, name: &str) -> Option<String>;
    fn key_exists(&self, key: &str) -> bool;
    /// Every string value of `key` as (name, data).
    fn values(&self, key: &str) -> Vec<(String, String)>;
    /// Writes a value, creating the key.
    fn write(&mut self, key: &str, name: &str, data: &str) -> bool;
    /// Deletes a value; a missing one is fine.
    fn delete_value(&mut self, key: &str, name: &str) -> bool;
    /// Deletes a key with its values and subkeys; a missing one is fine.
    fn delete_tree(&mut self, key: &str) -> bool;
}

/// A [`Registry`] in memory, for tests: never the real one.
#[derive(Debug, Clone, Default)]
pub struct MemoryRegistry {
    /// Lowercased key path -> lowercased value name -> (name, data).
    keys: BTreeMap<String, BTreeMap<String, (String, String)>>,
}

impl MemoryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    fn norm(key: &str) -> String {
        key.trim_matches('\\').to_ascii_lowercase()
    }

    /// Every key path, lowercased, for assertions.
    pub fn keys(&self) -> Vec<String> {
        self.keys.keys().cloned().collect()
    }
}

impl Registry for MemoryRegistry {
    fn read(&self, key: &str, name: &str) -> Option<String> {
        self.keys
            .get(&Self::norm(key))?
            .get(&name.to_ascii_lowercase())
            .map(|(_, data)| data.clone())
    }

    fn key_exists(&self, key: &str) -> bool {
        let key = Self::norm(key);
        self.keys
            .keys()
            .any(|k| *k == key || k.starts_with(&format!("{key}\\")))
    }

    fn values(&self, key: &str) -> Vec<(String, String)> {
        self.keys
            .get(&Self::norm(key))
            .map(|values| values.values().cloned().collect())
            .unwrap_or_default()
    }

    fn write(&mut self, key: &str, name: &str, data: &str) -> bool {
        self.keys.entry(Self::norm(key)).or_default().insert(
            name.to_ascii_lowercase(),
            (name.to_owned(), data.to_owned()),
        );
        true
    }

    fn delete_value(&mut self, key: &str, name: &str) -> bool {
        if let Some(values) = self.keys.get_mut(&Self::norm(key)) {
            values.remove(&name.to_ascii_lowercase());
        }
        true
    }

    fn delete_tree(&mut self, key: &str) -> bool {
        let key = Self::norm(key);
        let prefix = format!("{key}\\");
        self.keys
            .retain(|k, _| *k != key && !k.starts_with(&prefix));
        true
    }
}

/// The quoted (or bare) first token of a command line: the executable of
/// `"C:\x\Yap.exe" "%1"` or `C:\x\Yap.exe --autostart`. `None` for an empty
/// command or an unterminated quote.
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

/// Whether the install hook may write a scheme, given its current open
/// command (`None` when it is not registered): yes when nothing owns it,
/// when it is already ours, or when its executable is gone. A live
/// executable elsewhere keeps it.
pub fn may_take_scheme(current: Option<&str>, root: &str, exists: impl Fn(&str) -> bool) -> bool {
    match current.map(str::trim).filter(|c| !c.is_empty()) {
        None => true,
        Some(command) => match command_exe(command) {
            Some(exe) => is_within(exe, root) || !exists(exe),
            None => true,
        },
    }
}

/// `base` or `base-<profile>`, case-insensitive (registry value names are).
fn is_named(name: &str, base: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = base.to_ascii_lowercase();
    lower == base
        || lower
            .strip_prefix(&base)
            .and_then(|rest| rest.strip_prefix('-'))
            .is_some_and(yap_cache::profile::is_valid)
}

/// Whether `name` is one of this app's autostart values: `Yap` or
/// `Yap-<profile>`, or, from before the rename, `YapNative` or
/// `YapNative-<profile>`. `Yap` is also the Tauri app's name, so callers
/// check where the command points before touching one.
pub fn is_run_value(name: &str) -> bool {
    is_named(name, RUN_VALUE) || is_named(name, LEGACY_RUN_VALUE)
}

/// The old install's autostart values to remove when the released app
/// starts: `YapNative` and `YapNative-<profile>` whose command runs an
/// executable in `legacy_root` (`%LOCALAPPDATA%\YapNative.App`). Left in
/// place, the older build would start at sign-in beside this one. A value
/// pointing anywhere else (a dev build, the user's own copy) stays.
pub fn legacy_run_values(values: &[(String, String)], legacy_root: &str) -> Vec<String> {
    values
        .iter()
        .filter(|(name, command)| {
            is_named(name, LEGACY_RUN_VALUE) && command_is_ours(command, legacy_root)
        })
        .map(|(name, _)| name.clone())
        .collect()
}

/// Whether a launch may write its Run value over `current` (the value's
/// command now, `None` when there is none): when there is none, when it
/// already starts `exe` or a missing executable, or when this is the
/// released install (it takes the Tauri app's `Yap` over). A dev build or
/// a copy never takes a live app's autostart.
pub fn may_write_run_value(
    current: Option<&str>,
    exe: &str,
    released: bool,
    exists: impl Fn(&str) -> bool,
) -> bool {
    if released {
        return true;
    }
    match current
        .map(str::trim)
        .filter(|c| !c.is_empty())
        .map(command_exe)
    {
        None => true,
        Some(None) => true,
        Some(Some(other)) => same_path(other, exe) || !exists(other),
    }
}

/// Whether turning autostart off may delete the value `current`: only when
/// it starts this very executable. Another app's (the Tauri app's `Yap`,
/// another copy's) stays.
pub fn may_delete_run_value(current: &str, exe: &str) -> bool {
    command_exe(current).is_some_and(|other| same_path(other, exe))
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

/// Whether a launch may (re)write the schemes: only the released install,
/// as the default profile. Every other build and profile leaves them to
/// the installed app, so a test run never takes them; a build that needs
/// links for a test registers a test scheme of its own
/// (`links::test_scheme`).
pub fn may_register_at_launch(
    exe: &str,
    default_profile: bool,
    is_file: impl Fn(&str) -> bool,
) -> bool {
    default_profile && is_released_install(exe, is_file)
}

/// Whether the install or update hook of the install at `root` may write a
/// scheme, given its current open command (`None` when unregistered). The
/// released install always does: it owns the schemes, and this is where it
/// takes them back from a dev build, or `yap://` from the Tauri app. Another
/// install (a test pack id) only as [`may_take_scheme`] allows.
pub fn hook_may_take_scheme(
    root: &str,
    current: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> bool {
    is_released_root(root) || may_take_scheme(current, root, exists)
}

/// `<scheme key>\shell\open\command`, where a scheme's open command is.
pub fn scheme_command_key(scheme_key: &str) -> String {
    format!(r"{scheme_key}\shell\open\command")
}

/// The install and update hooks' scheme step: each of [`links::SCHEMES`]
/// pointed at `exe` (`<root>\current\Yap.exe`) where
/// [`hook_may_take_scheme`] allows. Returns a log line per scheme.
pub fn register_schemes(
    reg: &mut dyn Registry,
    root: &str,
    exe: &str,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut log = Vec::new();
    for scheme in links::SCHEMES {
        let key = links::scheme_key(scheme);
        let current = reg.read(&scheme_command_key(&key), "");
        if hook_may_take_scheme(root, current.as_deref(), &exists) {
            let written = links::scheme_values(scheme, exe)
                .iter()
                .filter(|v| reg.write(&v.key, &v.name, &v.data))
                .count();
            log.push(format!("scheme {scheme}: {written} value(s) written"));
        } else {
            log.push(format!(
                "scheme {scheme}: owned by another copy, left alone"
            ));
        }
    }
    log
}

/// The install hook's toast identity step: [`AUMID_KEY`] with its name and
/// an owner marker naming `root`, unless the key is there already (a dev
/// build's; the app sets the icon on its first start).
pub fn register_aumid(reg: &mut dyn Registry, root: &str) -> String {
    if reg.key_exists(AUMID_KEY) {
        return "aumid: already registered, left alone".to_owned();
    }
    let ok = reg.write(AUMID_KEY, "DisplayName", AUMID_DISPLAY_NAME)
        && reg.write(AUMID_KEY, AUMID_OWNER_VALUE, root);
    format!("aumid: created ({ok})")
}

/// What the uninstall hook removed, for the log and the caller.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Uninstalled {
    pub log: Vec<String>,
    /// The AUMID key was ours and went: the default toast icon goes too.
    pub aumid_removed: bool,
}

/// The uninstall hook's registry work: each scheme whose open command runs
/// an executable in `root`, the AUMID key when its marker names `root`, and
/// every Run value of ours ([`is_run_value`]) whose command runs an
/// executable in `root`. Nothing that points elsewhere (the Tauri app,
/// another copy) is touched.
pub fn unregister(reg: &mut dyn Registry, root: &str) -> Uninstalled {
    let mut out = Uninstalled::default();
    for scheme in links::SCHEMES {
        let key = links::scheme_key(scheme);
        match reg.read(&scheme_command_key(&key), "") {
            Some(command) if command_is_ours(&command, root) => {
                let ok = reg.delete_tree(&key);
                out.log.push(format!("scheme {scheme}: removed ({ok})"));
            }
            Some(_) => out
                .log
                .push(format!("scheme {scheme}: owned by another copy, kept")),
            None => out.log.push(format!("scheme {scheme}: not registered")),
        }
    }

    let owner = reg.read(AUMID_KEY, AUMID_OWNER_VALUE);
    if owner.is_some_and(|o| same_path(&o, root)) {
        let ok = reg.delete_tree(AUMID_KEY);
        out.log.push(format!("aumid: removed ({ok})"));
        out.aumid_removed = true;
    } else {
        out.log
            .push("aumid: not created by this install, kept".to_owned());
    }

    for (name, command) in reg.values(RUN_KEY) {
        if is_run_value(&name) && command_is_ours(&command, root) {
            let ok = reg.delete_value(RUN_KEY, &name);
            out.log.push(format!("run value {name}: removed ({ok})"));
        }
    }
    out
}
