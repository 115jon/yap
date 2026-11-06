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

/// The app folder under `base` (`%LOCALAPPDATA%`): `YapNative`, or
/// `YapNative\profiles\<name>` for a named profile.
pub fn data_root_in(base: &Path, profile: Option<&str>) -> PathBuf {
    let root = base.join("YapNative");
    match profile {
        Some(name) => root.join("profiles").join(name),
        None => root,
    }
}

/// This process's app folder. `None` without `LOCALAPPDATA`.
pub fn data_root() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA").filter(|v| !v.is_empty())?;
    Some(data_root_in(Path::new(&base), current()))
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
        assert_eq!(data_root_in(base, None), base.join("YapNative"));
        assert_eq!(
            data_root_in(base, Some("jon")),
            base.join("YapNative").join("profiles").join("jon")
        );
        assert_eq!(credential_suffix(None), "");
        assert_eq!(credential_suffix(Some("jon")), ":jon");
    }
}
