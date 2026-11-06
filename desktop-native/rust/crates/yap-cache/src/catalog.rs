//! The collectibles catalog between launches (collectibles-catalog-client.ts
//! keeps it in memory with a freshness window; the native app also keeps
//! the last copy on disk, so the shop opens at once).
//!
//! `catalog.json` next to the snapshot cache holds the catalog as the API
//! served it and when it came. The store shows it straight away and asks
//! the server again only once it is past the web's window: 30 minutes, or
//! 10 when the server said its copy was stale. The API sends no `ETag` or
//! `Cache-Control` for the route, so the time is the only freshness signal.
//! The catalog is the same for every account.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::{read, write};
use crate::CacheError;

/// The web's `FRESH_RECHECK_MS`.
pub const FRESH_MS: i64 = 30 * 60 * 1000;
/// The web's `STALE_RECHECK_MS` (the server's copy was stale).
pub const STALE_MS: i64 = 10 * 60 * 1000;

const FILE: &str = "catalog.json";

/// What `catalog.json` holds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredCatalog {
    /// When it was fetched, epoch milliseconds.
    pub fetched_at_ms: i64,
    /// The `catalog` object of `GET /api/collectibles/catalog`.
    pub catalog: serde_json::Value,
}

/// `%LOCALAPPDATA%\YapNative\cache\catalog.json`.
pub fn default_path() -> Option<PathBuf> {
    Some(crate::default_path()?.parent()?.join(FILE))
}

/// The stored copy, if there is a readable one.
pub fn load(path: &Path) -> Option<StoredCatalog> {
    read::<StoredCatalog>(path).filter(|c| c.catalog.is_object())
}

pub fn save(path: &Path, catalog: &StoredCatalog) -> Result<(), CacheError> {
    write(path, catalog)
}

/// `shouldReuse`: whether a copy fetched at `fetched_at_ms` can be shown
/// without asking again at `now_ms`.
pub fn reusable(stale: bool, fetched_at_ms: i64, now_ms: i64) -> bool {
    let age = now_ms - fetched_at_ms;
    (0..if stale { STALE_MS } else { FRESH_MS }).contains(&age)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_copy_should_be_reused_for_thirty_minutes() {
        assert!(reusable(false, 0, FRESH_MS - 1) && !reusable(false, 0, FRESH_MS));
    }

    #[test]
    fn a_stale_copy_should_be_rechecked_after_ten_minutes() {
        assert!(!reusable(true, 0, STALE_MS));
    }

    #[test]
    fn a_copy_from_the_future_should_be_rechecked() {
        assert!(!reusable(false, 10_000, 0));
    }

    #[test]
    fn a_saved_copy_should_load_back() {
        let dir = std::env::temp_dir().join(format!("yap-cache-catalog-{}", std::process::id()));
        let path = dir.join(FILE);
        let stored = StoredCatalog {
            fetched_at_ms: 42,
            catalog: serde_json::json!({ "items": [] }),
        };
        save(&path, &stored).expect("save");
        let loaded = load(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(loaded, Some(stored));
    }

    #[test]
    fn a_file_without_a_catalog_object_should_not_load() {
        let dir =
            std::env::temp_dir().join(format!("yap-cache-catalog-bad-{}", std::process::id()));
        let path = dir.join(FILE);
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, r#"{"fetched_at_ms": 1, "catalog": []}"#);
        let loaded = load(&path);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(loaded.is_none());
    }
}
