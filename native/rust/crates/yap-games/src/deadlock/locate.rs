//! Where Deadlock keeps its `console.log`: the Steam library holding app
//! 1422450 (`libraryfolders.vdf`, then `appmanifest_1422450.acf`'s
//! `installdir`), then `game/citadel/console.log` under the install.

use std::path::{Path, PathBuf};

use crate::vdf::{steam_libraries, vdf_values};

use super::STEAM_APP_ID;

/// `console.log` under an install folder.
pub fn console_log_in(install: &Path) -> PathBuf {
    install.join("game").join("citadel").join("console.log")
}

/// Deadlock's install folder in the Steam libraries under `steam`.
pub fn install_dir(steam: &Path) -> Option<PathBuf> {
    steam_libraries(steam).into_iter().find_map(|library| {
        let manifest =
            std::fs::read_to_string(library.join(format!("appmanifest_{STEAM_APP_ID}.acf"))).ok()?;
        let dir = vdf_values(&manifest, "installdir")
            .next()
            .filter(|dir| !dir.is_empty() && !dir.contains(['/', '\\']) && dir != "..")
            .unwrap_or_else(|| "Deadlock".into());
        Some(library.join("common").join(dir))
    })
}

/// The log's path on this computer: `YAP_TEST_DEADLOCK_LOG` when set (tests),
/// else from Steam's own folder.
pub fn console_log() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("YAP_TEST_DEADLOCK_LOG").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(path));
    }
    #[cfg(windows)]
    {
        install_dir(&crate::steam::steam_path()?).map(|dir| console_log_in(&dir))
    }
    #[cfg(not(windows))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_is_found_in_whichever_library_has_it() {
        let root = tempfile::tempdir().unwrap();
        let steam = root.path().join("Steam");
        let other = root.path().join("Library2");
        std::fs::create_dir_all(steam.join("steamapps")).unwrap();
        std::fs::create_dir_all(other.join("steamapps")).unwrap();
        let escaped = other.to_string_lossy().replace('\\', "\\\\");
        std::fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"0\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n\t\"1\"\n\t{{\n\t\t\"path\"\t\t\"{escaped}\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"1422450\"\t\t\"39264596005\"\n\t\t}}\n\t}}\n}}\n",
                steam.to_string_lossy().replace('\\', "\\\\")
            ),
        )
        .unwrap();
        assert_eq!(install_dir(&steam), None);
        std::fs::write(
            other.join("steamapps/appmanifest_1422450.acf"),
            "\"AppState\"\n{\n\t\"appid\"\t\t\"1422450\"\n\t\"installdir\"\t\t\"Deadlock\"\n}\n",
        )
        .unwrap();
        let install = install_dir(&steam).unwrap();
        assert_eq!(install, other.join("steamapps").join("common").join("Deadlock"));
        assert!(console_log_in(&install).ends_with("game/citadel/console.log"));
    }
}
