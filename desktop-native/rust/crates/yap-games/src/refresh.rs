//! Keeping the catalog current: the web app serves it at
//! `/desktop/game-catalog.json`; the app asks at most once a day, with the
//! last ETag (`If-None-Match`), and keeps a good copy in the profile's data
//! folder ([`crate::CACHE_FILE`]), as `yap-capture`'s `remote_policy` does
//! for the capture policy. A failed fetch or a file that does not parse
//! (a page, an older format) keeps what is there; the bundled snapshot is
//! always the floor.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::catalog::Catalog;

/// The catalog's path on the web app (`public/desktop/`).
pub const CATALOG_PATH: &str = "/desktop/game-catalog.json";
/// Keeps the fetch off the startup path.
pub const FIRST_CHECK_DELAY: Duration = Duration::from_secs(45);
/// How often the thread looks whether a day has passed.
const CHECK_EVERY: Duration = Duration::from_secs(60 * 60);
/// At most one fetch per day.
pub const REFRESH_INTERVAL_MS: i64 = 24 * 60 * 60 * 1000;
/// What a fetch may return at most (the catalog is about 2 MB).
pub const MAX_BODY_BYTES: usize = 8 * 1024 * 1024;
pub const FETCH_TIMEOUT: Duration = Duration::from_secs(30);
const META_FILE: &str = "game-catalog.meta.json";

/// When the catalog was last checked, and the ETag that answer carried.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RefreshMeta {
    pub checked_at: i64,
    pub http_etag: String,
}

/// What one fetch got.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// 304: what we have is current.
    NotModified,
    /// 200, with its ETag header (empty when none).
    Body { bytes: Vec<u8>, etag: String },
}

/// Fetches the catalog given the last ETag (the app's HTTP client; this
/// crate has none).
pub type Fetch = Box<dyn Fn(&str) -> Result<Fetched, String> + Send>;

fn meta_path(dir: &Path) -> PathBuf {
    dir.join(META_FILE)
}

pub fn load_meta(dir: &Path) -> RefreshMeta {
    std::fs::read(meta_path(dir))
        .ok()
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

/// Whether a day has passed since the last check (or the clock went back).
pub fn due(meta: &RefreshMeta, now_ms: i64) -> bool {
    now_ms - meta.checked_at >= REFRESH_INTERVAL_MS || now_ms < meta.checked_at
}

/// One check: fetch with the last ETag, keep a good body. Returns the new
/// catalog when one replaced the cache. The check time is saved either
/// way, so a broken answer is not asked again for a day.
pub fn check(dir: &Path, fetch: &Fetch, now_ms: i64) -> Option<Catalog> {
    let mut meta = load_meta(dir);
    let outcome = fetch(&meta.http_etag);
    meta.checked_at = now_ms;
    let fresh = match outcome {
        Ok(Fetched::NotModified) => {
            eprintln!("[games] catalog unchanged");
            None
        }
        Ok(Fetched::Body { bytes, etag }) => match accept(&bytes) {
            Ok(catalog) => match write_atomic(&dir.join(crate::CACHE_FILE), &bytes) {
                Ok(()) => {
                    eprintln!(
                        "[games] catalog {} ({} apps) from {CATALOG_PATH}",
                        catalog.etag(),
                        catalog.len()
                    );
                    meta.http_etag = etag;
                    Some(catalog)
                }
                Err(err) => {
                    eprintln!("[games] could not cache the catalog: {err}");
                    None
                }
            },
            Err(err) => {
                eprintln!("[games] keeping the catalog: {err}");
                None
            }
        },
        Err(err) => {
            eprintln!("[games] keeping the catalog: {err}");
            None
        }
    };
    if let Ok(raw) = serde_json::to_vec(&meta) {
        let _ = write_atomic(&meta_path(dir), &raw);
    }
    fresh
}

/// A fetched body, if it is a catalog this build reads.
pub fn accept(bytes: &[u8]) -> Result<Catalog, String> {
    if bytes.len() > MAX_BODY_BYTES {
        return Err(format!("catalog is {} bytes", bytes.len()));
    }
    Catalog::from_slice(bytes).map_err(|e| e.to_string())
}

/// Checks after a delay and then hourly (fetching at most daily), on a
/// background thread; `on_new` gets each catalog that replaced the cache.
pub fn spawn(dir: PathBuf, fetch: Fetch, on_new: Box<dyn Fn(Catalog) + Send>) {
    let spawned = std::thread::Builder::new()
        .name("YapGameCatalog".into())
        .spawn(move || {
            std::thread::sleep(FIRST_CHECK_DELAY);
            loop {
                let now = now_ms();
                if due(&load_meta(&dir), now) {
                    if let Some(catalog) = check(&dir, &fetch, now) {
                        on_new(catalog);
                    }
                }
                std::thread::sleep(CHECK_EVERY);
            }
        });
    if let Err(err) = spawned {
        eprintln!("[games] could not start the catalog refresh: {err}");
    }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    const FIXTURE: &str = include_str!("../tests/fixture-catalog.json");

    fn fetcher(answer: Result<Fetched, String>, seen: Arc<Mutex<Vec<String>>>) -> Fetch {
        Box::new(move |etag: &str| {
            seen.lock().unwrap().push(etag.to_owned());
            answer.clone()
        })
    }

    #[test]
    fn a_day_must_pass_between_checks() {
        let meta = RefreshMeta {
            checked_at: 1_000,
            http_etag: String::new(),
        };
        assert!(!due(&meta, 1_000 + REFRESH_INTERVAL_MS - 1));
        assert!(due(&meta, 1_000 + REFRESH_INTERVAL_MS));
        assert!(due(&meta, 10), "a clock that went back checks again");
        assert!(due(&RefreshMeta::default(), REFRESH_INTERVAL_MS));
    }

    #[test]
    fn a_good_body_is_cached_with_its_etag_and_sent_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let body = Fetched::Body {
            bytes: FIXTURE.as_bytes().to_vec(),
            etag: "\"abc\"".into(),
        };
        let fresh = check(dir.path(), &fetcher(Ok(body), seen.clone()), 5);
        assert_eq!(fresh.expect("new catalog").etag(), "fixture0001");
        assert!(dir.path().join(crate::CACHE_FILE).exists());
        assert_eq!(load_meta(dir.path()).http_etag, "\"abc\"");
        // A 304 keeps the cache and the ETag, and marks the check.
        let fresh = check(dir.path(), &fetcher(Ok(Fetched::NotModified), seen.clone()), 9);
        assert!(fresh.is_none());
        assert_eq!(seen.lock().unwrap().as_slice(), ["", "\"abc\""]);
        let meta = load_meta(dir.path());
        assert_eq!((meta.checked_at, meta.http_etag.as_str()), (9, "\"abc\""));
    }

    #[test]
    fn a_bad_body_or_a_failure_keeps_the_cache() {
        let dir = tempfile::tempdir().unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        let good = Fetched::Body {
            bytes: FIXTURE.as_bytes().to_vec(),
            etag: "\"one\"".into(),
        };
        check(dir.path(), &fetcher(Ok(good), seen.clone()), 1);
        let page = Fetched::Body {
            bytes: b"<!doctype html><title>Yap</title>".to_vec(),
            etag: "\"page\"".into(),
        };
        assert!(check(dir.path(), &fetcher(Ok(page), seen.clone()), 2).is_none());
        assert!(check(dir.path(), &fetcher(Err("offline".into()), seen), 3).is_none());
        let cached = std::fs::read(dir.path().join(crate::CACHE_FILE)).unwrap();
        assert_eq!(cached, FIXTURE.as_bytes());
        let meta = load_meta(dir.path());
        assert_eq!((meta.checked_at, meta.http_etag.as_str()), (3, "\"one\""));
    }

    #[test]
    fn an_older_cache_loses_to_the_bundled_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        // The fixture is dated before the bundled snapshot.
        std::fs::write(dir.path().join(crate::CACHE_FILE), FIXTURE).unwrap();
        let catalog = crate::load_catalog(Some(dir.path())).unwrap();
        assert_eq!(catalog.etag(), crate::bundled_meta().etag);
        // A newer one wins.
        let newer = FIXTURE.replace("2026-11-08T00:00:00.000Z", "2099-01-01T00:00:00.000Z");
        std::fs::write(dir.path().join(crate::CACHE_FILE), newer).unwrap();
        let catalog = crate::load_catalog(Some(dir.path())).unwrap();
        assert_eq!(catalog.etag(), "fixture0001");
    }
}
