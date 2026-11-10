//! Named profiles: `YAP_PROFILE=<name>` or `--profile <name>` runs a second,
//! fully separate copy of the app (own folders, credential, instance lock),
//! so two accounts can run side by side. The name is `[a-z0-9-]{1,32}`;
//! anything else is ignored and the default profile runs.
//!
//! The C++ shell (`cpp/profile.cpp`) applies the same rules to the same
//! inputs, so both halves always agree on the name.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// The environment variable that names the profile.
pub const ENV: &str = "YAP_PROFILE";

/// The command-line switch that names the profile (`--profile <name>` or
/// `--profile=<name>`). It wins over [`ENV`].
pub const ARG: &str = "--profile";

/// The longest profile name.
pub const MAX_LEN: usize = 32;

/// Whether `name` is a usable profile name: 1 to [`MAX_LEN`] characters of
/// `a-z`, `0-9` and `-`.
pub fn is_valid(name: &str) -> bool {
    (1..=MAX_LEN).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// The switch's value in `args` (the first `--profile`), if the switch is
/// there: `Some(None)` when it has no value.
fn arg_value<I, S>(args: I) -> Option<Option<String>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let arg = arg.as_ref();
        if arg == ARG {
            return Some(args.next().map(|v| v.as_ref().to_owned()));
        }
        if let Some(value) = arg
            .strip_prefix(ARG)
            .and_then(|rest| rest.strip_prefix('='))
        {
            return Some(Some(value.to_owned()));
        }
    }
    None
}

/// The profile for these inputs: the command-line switch when present
/// (valid or not: a bad one means the default), else `env`. `None` is the
/// default profile.
pub fn resolve<I, S>(args: I, env: Option<&str>) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let chosen = match arg_value(args) {
        Some(value) => value,
        None => env.map(str::to_owned),
    };
    chosen.filter(|name| is_valid(name))
}

/// This process's profile, read once from its arguments and environment.
pub fn current() -> Option<&'static str> {
    static PROFILE: OnceLock<Option<String>> = OnceLock::new();
    PROFILE
        .get_or_init(|| {
            let env = std::env::var(ENV).ok();
            // Arguments that are not UTF-8 cannot carry a valid name.
            let args: Vec<String> = std::env::args_os()
                .map(|a| a.to_string_lossy().into_owned())
                .collect();
            resolve(args.iter().skip(1), env.as_deref())
        })
        .as_deref()
}

/// The app's data folder under `%LOCALAPPDATA%`: settings, caches, logs,
/// WebView2 and every profile. Not `Yap` (the Tauri app's install root,
/// which its uninstaller deletes) and not `Yap.App` (this app's Velopack
/// install root, which ours deletes): docs/INSTALLER.md.
pub const DATA_DIR: &str = "Yap.Data";

/// The data folder before the rename to Yap (`Yap Native`). [`resolve_root`]
/// moves it to [`DATA_DIR`] once, and reads it in place while it cannot
/// (a running older build holds files in it). The fallback is for one
/// version: drop it once every copy has moved.
pub const LEGACY_DATA_DIR: &str = "YapNative";

/// Where a move that found [`DATA_DIR`] already there parks the old folder
/// while its entries move in one by one; a later start finishes the job.
pub const STAGING_DIR: &str = "YapNative.migrating";

/// Written into [`DATA_DIR`] once the old folder has moved (or there was
/// none): from then on an old folder that an older build makes again
/// belongs to that build and is never moved or read.
pub const MIGRATED_MARKER: &str = ".migrated-from-yapnative";

/// The app folder under `base` (`%LOCALAPPDATA%`): `Yap.Data`, or
/// `Yap.Data\profiles\<name>` for a named profile.
pub fn data_root_in(base: &Path, profile: Option<&str>) -> PathBuf {
    with_profile(base.join(DATA_DIR), profile)
}

/// The same folder in the old layout: `YapNative[\profiles\<name>]`.
pub fn legacy_root_in(base: &Path, profile: Option<&str>) -> PathBuf {
    with_profile(base.join(LEGACY_DATA_DIR), profile)
}

fn with_profile(root: PathBuf, profile: Option<&str>) -> PathBuf {
    match profile {
        Some(name) => root.join("profiles").join(name),
        None => root,
    }
}

/// What [`migrate`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Migration {
    /// The marker was there: nothing to do.
    AlreadyDone,
    /// No old folder: a fresh start (the marker is written).
    NothingToMove,
    /// The old folder was renamed to the new one, every profile with it.
    Renamed,
    /// The new folder existed (an installer log, say): the old one's entries
    /// moved in one by one. `conflicts` are files both had: the old copy
    /// sits beside the new one as `<stem>.legacy.<ext>`. `left` could not
    /// move and stay in [`STAGING_DIR`].
    Merged {
        conflicts: Vec<PathBuf>,
        left: Vec<PathBuf>,
    },
    /// The old folder could not be moved (files open in it, usually a
    /// running older build): nothing moved, and this run reads the old
    /// folder. The next start tries again.
    Locked(String),
}

impl Migration {
    /// Whether this run should use the old folder.
    pub fn uses_legacy(&self) -> bool {
        matches!(self, Migration::Locked(_))
    }
}

/// Moves `base\YapNative` (every profile in it) to `base\Yap.Data`, once.
/// Every step is a rename on the same volume, so nothing is copied and
/// nothing is deleted: a folder that cannot move stays where it was, and an
/// interrupted move resumes on the next start. Idempotent.
pub fn migrate(base: &Path) -> Migration {
    let new = base.join(DATA_DIR);
    let old = base.join(LEGACY_DATA_DIR);
    let staging = base.join(STAGING_DIR);
    if new.join(MIGRATED_MARKER).exists() {
        return Migration::AlreadyDone;
    }
    let outcome = if staging.is_dir() {
        // A merge that did not finish (the process ended mid-way).
        merge_staging(&staging, &new)
    } else if !old.is_dir() {
        Migration::NothingToMove
    } else if !new.exists() {
        match std::fs::rename(&old, &new) {
            Ok(()) => Migration::Renamed,
            // Another process (a second profile starting) moved it first.
            Err(_) if !old.exists() && new.is_dir() => Migration::Renamed,
            Err(e) => return Migration::Locked(e.to_string()),
        }
    } else {
        // One rename takes the whole old folder or nothing, so a running
        // older build (which keeps files open in it) stops it here.
        match std::fs::rename(&old, &staging) {
            Ok(()) => merge_staging(&staging, &new),
            Err(_) if !old.exists() => Migration::AlreadyDone,
            Err(e) => return Migration::Locked(e.to_string()),
        }
    };
    // Not while anything is still in the staging folder: the marker would
    // stop the next start from finishing the move.
    let unfinished = matches!(&outcome, Migration::Merged { left, .. } if !left.is_empty());
    if !unfinished && std::fs::create_dir_all(&new).is_ok() {
        let _ = std::fs::write(new.join(MIGRATED_MARKER), format!("{outcome:?}\n"));
    }
    outcome
}

fn merge_staging(staging: &Path, new: &Path) -> Migration {
    let mut conflicts = Vec::new();
    let mut left = Vec::new();
    let _ = std::fs::create_dir_all(new);
    merge_dir(staging, new, &mut conflicts, &mut left);
    // Empty folders only: a file left behind keeps its folder.
    remove_empty_dirs(staging);
    Migration::Merged { conflicts, left }
}

/// Moves every entry of `from` into `to`: a missing one by a rename,
/// folders on both sides entry by entry, and a file on both sides as
/// `<stem>.legacy.<ext>` next to the one already there.
fn merge_dir(from: &Path, to: &Path, conflicts: &mut Vec<PathBuf>, left: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(from) else {
        left.push(from.to_path_buf());
        return;
    };
    for entry in entries.flatten() {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if !target.exists() {
            if std::fs::rename(&source, &target).is_err() {
                left.push(source);
            }
            continue;
        }
        if source.is_dir() && target.is_dir() {
            merge_dir(&source, &target, conflicts, left);
            continue;
        }
        let aside = legacy_name(&target);
        if !aside.exists() && std::fs::rename(&source, &aside).is_ok() {
            conflicts.push(aside);
        } else {
            left.push(source);
        }
    }
}

/// `installer.log` -> `installer.legacy.log`; `notes` -> `notes.legacy`.
fn legacy_name(path: &Path) -> PathBuf {
    let name = match (path.file_stem(), path.extension()) {
        (Some(stem), Some(ext)) => format!(
            "{}.legacy.{}",
            stem.to_string_lossy(),
            ext.to_string_lossy()
        ),
        _ => format!(
            "{}.legacy",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
    };
    path.with_file_name(name)
}

fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    // Fails, and keeps the folder, unless it is empty.
    let _ = std::fs::remove_dir(dir);
}

/// The data folder of `profile` under `base`, after [`migrate`]: the new
/// one, or the old one while the move is blocked. A move is logged to
/// `<root>\logs\migration.log`.
pub fn resolve_root(base: &Path, profile: Option<&str>) -> (PathBuf, Migration) {
    let migration = migrate(base);
    let root = if migration.uses_legacy() {
        legacy_root_in(base, profile)
    } else {
        data_root_in(base, profile)
    };
    if migration != Migration::AlreadyDone {
        log_migration(&root, &migration);
    }
    (root, migration)
}

/// One line in `<root>\logs\migration.log` (kept, never rotated).
fn log_migration(root: &Path, migration: &Migration) {
    use std::io::Write;
    let dir = root.join("logs");
    let _ = std::fs::create_dir_all(&dir);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("migration.log"))
    {
        let _ = writeln!(
            file,
            "{secs} data folder: {migration:?} -> {}",
            root.display()
        );
    }
}

/// The folder the app's data lives under, when the app names it ([`set_base`]).
static BASE: OnceLock<PathBuf> = OnceLock::new();

/// [`data_root`], once resolved.
static ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Names the folder the app's data lives under, in place of
/// `%LOCALAPPDATA%`: a phone app's private data folder. Call it before
/// anything reads [`data_root`] (it resolves once); `false`, and ignored,
/// if a base was already set or the root has already resolved.
pub fn set_base(base: PathBuf) -> bool {
    ROOT.get().is_none() && BASE.set(base).is_ok()
}

/// This process's app folder, resolved (and the old folder moved) once,
/// for the Rust side and the C++ shell alike (`yap_profile_data_root`).
/// Under the app's base ([`set_base`]), else `%LOCALAPPDATA%`; `None`
/// without either.
pub fn data_root() -> Option<PathBuf> {
    ROOT.get_or_init(|| {
        let base = match BASE.get() {
            Some(base) => base.clone(),
            None => PathBuf::from(std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?),
        };
        Some(resolve_root(&base, current()).0)
    })
    .clone()
}

/// The suffix the credential's service name takes: `:<name>`, or nothing for
/// the default profile.
pub fn credential_suffix(profile: Option<&str>) -> String {
    profile.map(|name| format!(":{name}")).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_lowercase_letters_digits_and_dashes() {
        for ok in ["jon", "a", "test-2", "0", "a-b-c", &"x".repeat(32)] {
            assert!(is_valid(ok), "{ok} should be valid");
        }
        for bad in [
            "",
            "Jon",
            "jon_2",
            "jon.2",
            "a b",
            "..",
            "a/b",
            "a\\b",
            "jön",
            &"x".repeat(33),
        ] {
            assert!(!is_valid(bad), "{bad:?} should be invalid");
        }
    }

    #[test]
    fn the_switch_beats_the_environment_and_bad_names_fall_back() {
        let none: [&str; 0] = [];
        assert_eq!(resolve(none, None), None);
        assert_eq!(resolve(none, Some("jon")).as_deref(), Some("jon"));
        assert_eq!(resolve(none, Some("Bad Name")), None);
        assert_eq!(resolve(["--profile", "a"], Some("b")).as_deref(), Some("a"));
        assert_eq!(resolve(["--profile=a"], Some("b")).as_deref(), Some("a"));
        // A bad switch is the default, not the environment's profile.
        assert_eq!(resolve(["--profile", "A"], Some("b")), None);
        assert_eq!(resolve(["--profile"], Some("b")), None);
        assert_eq!(
            resolve(["--autostart", "--profile", "x"], None).as_deref(),
            Some("x")
        );
        assert_eq!(resolve(["--profiles", "x"], None), None);
    }

    #[test]
    fn a_named_profile_has_its_own_folder_and_credential() {
        let base = Path::new("base");
        assert_eq!(data_root_in(base, None), base.join("Yap.Data"));
        assert_eq!(
            data_root_in(base, Some("jon")),
            base.join("Yap.Data").join("profiles").join("jon")
        );
        assert_eq!(legacy_root_in(base, None), base.join("YapNative"));
        assert_eq!(
            legacy_root_in(base, Some("jon")),
            base.join("YapNative").join("profiles").join("jon")
        );
        assert_eq!(credential_suffix(None), "");
        assert_eq!(credential_suffix(Some("jon")), ":jon");
    }
}
