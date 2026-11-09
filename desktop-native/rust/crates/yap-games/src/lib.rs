//! Game detection for Yap: which game is running, from the process
//! list and Steam, matched against the game catalog (`catalog`, `matcher`),
//! watched on one background thread (`detector`, Windows only).
//!
//! What it reads: process ids and executable names from a ToolHelp snapshot
//! (which opens no process), and, only for a process whose name the catalog
//! lists, its image path through `PROCESS_QUERY_LIMITED_INFORMATION`. It
//! never reads another process's memory or command line, and asks for
//! nothing beyond `PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE`.
//!
//! No Qt and no network: the app hands it catalog bytes and takes its events.

pub mod catalog;
#[cfg(windows)]
pub mod detector;
pub mod matcher;
pub mod refresh;
#[cfg(windows)]
mod steam;
#[cfg(windows)]
mod win;

use std::path::{Path, PathBuf};

use serde::Deserialize;

pub use catalog::{Catalog, CatalogApp, CatalogError};

/// The catalog snapshot this build ships (`assets/games`, packed by
/// `tools/pack-game-catalog.mjs`): the web's JSON, gzipped.
pub static BUNDLED_CATALOG: &[u8] = include_bytes!("../../../../assets/games/game-catalog.json.gz");
const BUNDLED_META: &str = include_str!("../../../../assets/games/game-catalog.meta.json");

/// What `game-catalog.meta.json` says of the bundled snapshot.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct BundledMeta {
    pub etag: String,
    pub generated_at: String,
    pub count: usize,
}

pub fn bundled_meta() -> BundledMeta {
    serde_json::from_str(BUNDLED_META).unwrap_or_default()
}

/// The cached catalog in the profile's data folder (a refresh writes it).
pub const CACHE_FILE: &str = "game-catalog.json";

/// The catalog to use: the cached copy when it parses and is not older than
/// the bundled one, else the bundled snapshot.
pub fn load_catalog(data_dir: Option<&Path>) -> Result<Catalog, CatalogError> {
    let bundled_at = bundled_meta().generated_at;
    if let Some(path) = data_dir.map(|dir| dir.join(CACHE_FILE)) {
        if let Ok(file) = std::fs::File::open(&path) {
            match Catalog::from_reader(file) {
                Ok(cached) if cached.generated_at() >= bundled_at.as_str() => return Ok(cached),
                Ok(_) => {}
                Err(err) => eprintln!("[games] ignoring the cached catalog: {err}"),
            }
        }
    }
    Catalog::from_gzip(BUNDLED_CATALOG)
}

/// Which kind of game a detection is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// A catalog game, from its process or from Steam.
    Catalog,
    /// A program the user added by hand.
    Added,
}

/// The game the detector reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Detected {
    /// `app:<catalog id>`, `exe:<file name>` (added by hand) or
    /// `steam:<id>` (Steam knows it, the catalog does not).
    pub key: String,
    pub name: String,
    pub app_id: Option<String>,
    pub icon_hash: Option<String>,
    pub steam_app_id: Option<String>,
    /// When detection first saw it running (epoch ms).
    pub started_at: i64,
    pub source: Source,
}

impl Detected {
    /// Our `/api/game-icons/{app id}/{hash}.png` path, when it has an icon.
    pub fn icon_path(&self) -> Option<String> {
        yap_protocol::game_activity::game_icon_path(
            self.app_id.as_deref()?,
            self.icon_hash.as_deref()?,
        )
    }

    /// The activity as op 43 carries it (`detected` for catalog games,
    /// `manual` for added ones).
    pub fn activity(&self) -> yap_protocol::game_activity::GameActivity {
        let name: String = self.name.trim().chars().take(128).collect();
        yap_protocol::game_activity::GameActivity {
            app_id: self.app_id.clone().filter(|id| id.len() <= 20),
            steam_app_id: self.steam_app_id.clone().filter(|id| id.len() <= 10),
            icon_url: self.icon_path(),
            source: match self.source {
                Source::Catalog => "detected",
                Source::Added => "manual",
            }
            .into(),
            ..yap_protocol::game_activity::GameActivity::playing(name, self.started_at.max(0))
        }
    }
}

/// What the user chose (Settings > Registered games), as the detector reads
/// it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rules {
    pub enabled: bool,
    /// Game keys never reported.
    pub hidden: Vec<String>,
    /// (executable file name, lower case; shown name).
    pub added: Vec<(String, String)>,
}

/// A program with a window, for "Add it".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunningProgram {
    /// The executable's file name, as Windows has it.
    pub exe: String,
    /// The window's title, or empty.
    pub title: String,
}

/// One process of `YAP_TEST_GAME_PROCESSES`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeProcess {
    pub pid: u32,
    pub name: String,
    /// `None`: the process cannot be opened (anti-cheat, elevated).
    pub path: Option<String>,
}

/// The first fake pid: far above what Windows hands out.
pub const FAKE_PID_BASE: u32 = 0x7F00_0000;

/// `YAP_TEST_GAME_PROCESSES`: `name.exe|C:/path/name.exe;...` replaces the
/// process snapshot (tests). An entry without `|path` is one that cannot be
/// opened.
pub fn parse_test_processes(spec: &str) -> Vec<FakeProcess> {
    spec.split(';')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .enumerate()
        .filter_map(|(i, entry)| {
            let (name, path) = match entry.split_once('|') {
                Some((name, path)) => (name.trim(), Some(path.trim()).filter(|p| !p.is_empty())),
                None => (entry, None),
            };
            let name = if name.is_empty() {
                catalog::file_part(&catalog::normalize(path?)).to_owned()
            } else {
                name.to_owned()
            };
            Some(FakeProcess {
                pid: FAKE_PID_BASE + u32::try_from(i).ok()?,
                name,
                path: path.map(str::to_owned),
            })
        })
        .collect()
}

/// The fake process list, when the test switch is set.
pub fn test_processes() -> Option<Vec<FakeProcess>> {
    std::env::var("YAP_TEST_GAME_PROCESSES")
        .ok()
        .map(|spec| parse_test_processes(&spec))
}

/// Launchers, stores and their helpers: never a game, whatever the
/// catalog says.
pub const IGNORED_PROCESSES: &[&str] = &[
    "steam.exe",
    "steamwebhelper.exe",
    "steamservice.exe",
    "gameoverlayui.exe",
    "epicgameslauncher.exe",
    "epicwebhelper.exe",
    "battle.net.exe",
    "eadesktop.exe",
    "eabackgroundservice.exe",
    "origin.exe",
    "ubisoftconnect.exe",
    "upc.exe",
    "uplaywebcore.exe",
    "galaxyclient.exe",
    "galaxyclient helper.exe",
    "riotclientservices.exe",
    "riotclientux.exe",
    "riotclientuxrender.exe",
    "yapnative.exe",
    "yap.exe",
];

/// Whether a process name is one never reported.
pub fn is_ignored(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    IGNORED_PROCESSES.contains(&name.as_str())
}

/// Whether a normalized image path is under the Windows folder
/// (`windir`, normalized too).
pub fn is_under(path: &str, folder: &str) -> bool {
    let folder = folder.trim_end_matches('/');
    !folder.is_empty()
        && path.len() > folder.len()
        && path.starts_with(folder)
        && path.as_bytes()[folder.len()] == b'/'
}

/// The data folder as a path, for the callers that have one.
pub fn cache_path(dir: &Path) -> PathBuf {
    dir.join(CACHE_FILE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_test_switch_parses_names_paths_and_closed_processes() {
        let list = parse_test_processes(
            " celeste.exe|C:/Games/Celeste/Celeste.exe ; Locked.exe ;;|D:/x/Only.exe",
        );
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].name, "celeste.exe");
        assert_eq!(
            list[0].path.as_deref(),
            Some("C:/Games/Celeste/Celeste.exe")
        );
        assert_eq!(list[0].pid, FAKE_PID_BASE);
        assert_eq!(
            (list[1].name.as_str(), list[1].path.as_deref()),
            ("Locked.exe", None)
        );
        assert_eq!(list[2].name, "only.exe");
    }

    #[test]
    fn launchers_and_the_windows_folder_are_ignored() {
        assert!(is_ignored("Steam.exe") && is_ignored("EpicGamesLauncher.exe"));
        assert!(!is_ignored("celeste.exe"));
        assert!(is_under("c:/windows/system32/a.exe", "c:/windows"));
        assert!(!is_under("c:/windowsapps/a.exe", "c:/windows"));
        assert!(!is_under("c:/windows", "c:/windows"));
    }

    #[test]
    fn a_detection_becomes_a_valid_activity() {
        let detected = Detected {
            key: "app:356875221078245376".into(),
            name: " Overwatch ".into(),
            app_id: Some("356875221078245376".into()),
            icon_hash: Some("a60bb76ba4d4acafbd4cb9aad6e61739".into()),
            steam_app_id: Some("2357570".into()),
            started_at: 1_700_000_000_000,
            source: Source::Catalog,
        };
        let activity = detected.activity();
        assert!(activity.is_valid(), "{activity:?}");
        assert_eq!(activity.name, "Overwatch");
        assert_eq!(activity.source, "detected");
        assert_eq!(
            activity.icon_url.as_deref(),
            Some("/api/game-icons/356875221078245376/a60bb76ba4d4acafbd4cb9aad6e61739.png")
        );
        let added = Detected {
            key: "exe:mygame.exe".into(),
            app_id: None,
            icon_hash: None,
            steam_app_id: None,
            source: Source::Added,
            ..detected
        };
        let activity = added.activity();
        assert!(activity.is_valid() && activity.icon_url.is_none());
        assert_eq!(activity.source, "manual");
    }
}
