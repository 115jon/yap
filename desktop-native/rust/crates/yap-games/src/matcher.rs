//! Matching a process against the catalog, as arRPC does
//! (`src/process/index.js`), with two rules of our own:
//!
//! - The path is lower-cased with `/` separators; every suffix from the file
//!   name up to the path without its drive is compared, each also with the
//!   first `64`, `.x64`, `x64` or `_64` taken out. An entry matches when it
//!   equals one of them; a `>name` entry only when it equals the file name.
//!   Launchers and entries that need `arguments` were dropped at load.
//! - A shared file name (several apps list it, or it is generic like
//!   `game.exe`) matches only through an entry with its folders.
//! - When the process cannot be opened (anti-cheat, elevated), only its file
//!   name is known: it matches only a name that one app lists, never with
//!   folders, and is not shared.
//!
//! Several matches: the entry with the most folders wins, then the app first
//! in the catalog.

use crate::catalog::{file_part, normalize, Catalog};

/// The strings arRPC compares for a path: its suffixes, then each with the
/// 64-bit markers taken out once.
pub fn compared_strings(path: &str) -> Vec<String> {
    let path = normalize(path);
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    let mut out: Vec<String> = Vec::new();
    // From the file name to everything but the first segment (the drive).
    let longest = segments.len().saturating_sub(1).max(1);
    for count in 1..=longest.min(segments.len()) {
        out.push(segments[segments.len() - count..].join("/"));
    }
    let suffixes = out.clone();
    for suffix in &suffixes {
        for marker in ["64", ".x64", "x64", "_64"] {
            if suffix.contains(marker) {
                out.push(suffix.replacen(marker, "", 1));
            }
        }
    }
    out
}

/// The file names a process name may be filed under (itself and its
/// marker-free forms), lower case.
pub fn file_variants(name: &str) -> Vec<String> {
    let name = normalize(file_part(&normalize(name)));
    let mut out = vec![name.clone()];
    for marker in ["64", ".x64", "x64", "_64"] {
        if name.contains(marker) {
            let stripped = name.replacen(marker, "", 1);
            if !out.contains(&stripped) {
                out.push(stripped);
            }
        }
    }
    out
}

/// Whether a process with this file name could match anything: only then
/// is it worth opening.
pub fn may_match(catalog: &Catalog, name: &str) -> bool {
    file_variants(name).iter().any(|file| catalog.has_file(file))
}

/// The app a full image path matches, if any.
pub fn match_path(catalog: &Catalog, path: &str) -> Option<u32> {
    let compared = compared_strings(path);
    let file = compared.first()?.clone();
    let mut best: Option<(usize, u32)> = None;
    for candidate in &compared {
        for entry in catalog.entries_for(candidate) {
            let hit = if entry.exact {
                entry.path == file
            } else {
                entry.path == candidate.as_str()
            };
            // A shared name needs its folders.
            if !hit || (entry.shared && !entry.folder) {
                continue;
            }
            let depth = entry.path.matches('/').count();
            let better = match best {
                None => true,
                Some((d, app)) => depth > d || (depth == d && entry.app < app),
            };
            if better {
                best = Some((depth, entry.app));
            }
        }
    }
    best.map(|(_, app)| app)
}

/// The app a file name alone matches (the process could not be opened):
/// only a name one app lists without folders, and not a shared one.
pub fn match_name_only(catalog: &Catalog, name: &str) -> Option<u32> {
    let mut found: Option<u32> = None;
    for file in file_variants(name) {
        for entry in catalog.entries_for(&file) {
            if entry.folder || entry.shared {
                return None;
            }
            match found {
                Some(app) if app != entry.app => return None,
                _ => found = Some(entry.app),
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    fn catalog() -> Catalog {
        Catalog::from_slice(crate::catalog::tests::FIXTURE.as_bytes()).expect("fixture")
    }

    fn name_of(catalog: &Catalog, app: Option<u32>) -> Option<String> {
        app.and_then(|a| catalog.app(a)).map(|a| a.name)
    }

    #[test]
    fn suffixes_run_from_the_file_name_to_the_path_without_its_drive() {
        let compared = compared_strings(r"C:\Games\Blizzard\Overwatch.exe");
        assert_eq!(
            &compared[..3],
            ["overwatch.exe", "blizzard/overwatch.exe", "games/blizzard/overwatch.exe"]
        );
        assert!(!compared.iter().any(|c| c.starts_with("c:")));
    }

    #[test]
    fn a_plain_name_matches_anywhere() {
        let c = catalog();
        let app = match_path(&c, r"D:\Stuff\Overwatch\Overwatch.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Overwatch"));
    }

    #[test]
    fn a_folder_entry_needs_its_folders() {
        let c = catalog();
        let wow = match_path(&c, r"C:\Program Files\World of Warcraft\_retail_\Wow.exe");
        assert_eq!(name_of(&c, wow).as_deref(), Some("World of Warcraft"));
        assert!(match_path(&c, r"C:\Other\Wow.exe").is_none());
    }

    #[test]
    fn sixty_four_bit_markers_are_stripped() {
        let c = catalog();
        // Listed as `_retail_/wow.exe`: wow-64 has no match, Wow64 does.
        let app = match_path(&c, r"C:\WoW\_retail_\Wow64.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("World of Warcraft"));
        let app = match_path(&c, r"C:\x\Celeste_64.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Celeste"));
        let app = match_path(&c, r"C:\x\celeste.x64.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Celeste"));
        assert!(file_variants("Celeste_64.EXE").contains(&"celeste.exe".to_owned()));
    }

    #[test]
    fn an_exact_entry_matches_only_the_file_name() {
        let c = catalog();
        // `>hl2.exe` with folders listed elsewhere: the file name is the
        // shared hl2.exe, so only the folder entry identifies a game.
        let app = match_path(&c, r"C:\Steam\steamapps\common\Half-Life 2\hl2.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Half-Life 2"));
        // `>launch.exe` is exact: `launch.exe` matches, `launch64.exe`
        // (whose stripped form is launch.exe) does not.
        let app = match_path(&c, r"C:\Games\Exact\launch.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Exact Game"));
        assert!(match_path(&c, r"C:\Games\Exact\launch64.exe").is_none());
    }

    #[test]
    fn launchers_and_argument_entries_never_match() {
        let c = catalog();
        assert!(match_path(&c, r"C:\Battle.net\Battle.net.exe").is_none());
        assert!(match_path(&c, r"C:\Games\argued.exe").is_none());
    }

    #[test]
    fn a_shared_name_needs_its_folder_path() {
        let c = catalog();
        // game.exe is listed bare by two apps: neither matches bare.
        assert!(match_path(&c, r"C:\Some\Where\game.exe").is_none());
        // One of them also lists `sunless sea/game.exe`.
        let app = match_path(&c, r"D:\Steam\common\Sunless Sea\game.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Sunless Sea"));
        // nw.exe is generic even when one app lists it.
        assert!(match_path(&c, r"C:\x\nw.exe").is_none());
    }

    #[test]
    fn the_deepest_entry_wins() {
        let c = catalog();
        let app = match_path(&c, r"D:\Steam\common\Sunless Sea\game.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Sunless Sea"));
    }

    #[test]
    fn a_name_alone_matches_only_a_unique_folderless_name() {
        let c = catalog();
        let app = match_name_only(&c, "Overwatch.exe");
        assert_eq!(name_of(&c, app).as_deref(), Some("Overwatch"));
        // wow.exe is listed with folders.
        assert!(match_name_only(&c, "Wow.exe").is_none());
        // game.exe is shared.
        assert!(match_name_only(&c, "game.exe").is_none());
        assert!(match_name_only(&c, "unknown.exe").is_none());
        assert!(may_match(&c, "OVERWATCH.EXE"));
        assert!(!may_match(&c, "notepad.exe"));
    }
}
