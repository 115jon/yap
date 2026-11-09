//! The game catalog (`public/desktop/game-catalog.json`, built by the web
//! app's `scripts/lib/game-catalog.mjs` from Discord's detectable list) as a
//! compact index: every executable name, lower case with `/` separators,
//! under its file name, pointing at its app.
//!
//! The file is read in one streaming pass (no copy of the JSON is kept):
//! strings go into one arena, apps and executables into flat arrays, and the
//! file-name keys into a sorted array of hashes. Launchers and entries that
//! need `arguments` (which this client never reads) are dropped. A file name
//! that several apps list, or one of the generic names in [`SHARED_NAMES`],
//! is marked shared: its folderless entries never match on their own.

use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::hash::{Hash, Hasher};
use std::io::Read;

use serde::de::{self, DeserializeSeed, IgnoredAny, MapAccess, SeqAccess, Visitor};
use serde::Deserialize;

/// The catalog version this build reads.
pub const CATALOG_VERSION: u64 = 1;

/// Generic executable names that never identify a game on their own.
pub const SHARED_NAMES: &[&str] = &[
    "game.exe",
    "hl2.exe",
    "nw.exe",
    "dosbox.exe",
    "javaw.exe",
    "java.exe",
    "python.exe",
    "pythonw.exe",
    "electron.exe",
    "node.exe",
    "launcher.exe",
    "client.exe",
    "engine.exe",
    "start.exe",
    "app.exe",
    "main.exe",
    "play.exe",
    "run.exe",
    "setup.exe",
    "autorun.exe",
    "love.exe",
    "renpy.exe",
    "godot.exe",
    "unity.exe",
    "ue4game.exe",
    "ue4game-win64-shipping.exe",
];

/// A string in the arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Span {
    start: u32,
    len: u32,
}

#[derive(Debug, Clone)]
struct App {
    id: u64,
    name: Span,
    /// 16 bytes of the 32-hex icon hash.
    icon: Option<[u8; 16]>,
    /// Into `steam_ids`.
    steam: Span,
}

/// `>name`: the process's file name must be exactly this.
const EXACT: u8 = 1;
/// The name has folders (`folder/game.exe`).
const FOLDER: u8 = 2;
/// Its file name is listed by several apps, or is generic.
const SHARED: u8 = 4;

#[derive(Debug, Clone, Copy)]
struct Exe {
    app: u32,
    /// Lower case, `/` separators, without the `>`.
    path: Span,
    flags: u8,
}

/// One catalog app, as a match reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogApp {
    /// The Discord application id (decimal).
    pub id: String,
    pub name: String,
    /// 32 lowercase hex, when the app has an icon.
    pub icon_hash: Option<String>,
    pub steam_app_ids: Vec<String>,
}

/// The parsed catalog.
#[derive(Default)]
pub struct Catalog {
    etag: String,
    generated_at: String,
    arena: String,
    apps: Vec<App>,
    exes: Vec<Exe>,
    /// (hash of the file-name key, exe index), sorted.
    keys: Vec<(u64, u32)>,
    /// (Steam app id, app index), sorted.
    steam: Vec<(u32, u32)>,
    steam_ids: Vec<u32>,
}

impl fmt::Debug for Catalog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Catalog")
            .field("etag", &self.etag)
            .field("apps", &self.apps.len())
            .field("exes", &self.exes.len())
            .finish()
    }
}

/// Why a catalog file was refused.
#[derive(Debug)]
pub struct CatalogError(pub String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CatalogError {}

fn key_hash(key: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

/// The file name of a normalized path (`a/b.exe` -> `b.exe`).
pub(crate) fn file_part(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Lower case with `/` separators, as arRPC compares paths.
pub fn normalize(path: &str) -> String {
    path.to_lowercase().replace('\\', "/")
}

fn hex16(hash: &str) -> Option<[u8; 16]> {
    if hash.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(hash.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    // Only lowercase hex round-trips to the same path.
    hash.bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        .then_some(out)
}

impl Catalog {
    /// Reads a catalog file (the JSON, from any reader).
    pub fn from_reader(reader: impl Read) -> Result<Self, CatalogError> {
        let reader = std::io::BufReader::with_capacity(64 * 1024, reader);
        let mut de = serde_json::Deserializer::from_reader(reader);
        let mut catalog = Catalog::default();
        (&mut catalog)
            .deserialize(&mut de)
            .map_err(|e| CatalogError(e.to_string()))?;
        de.end().map_err(|e| CatalogError(e.to_string()))?;
        catalog.finish()?;
        Ok(catalog)
    }

    pub fn from_slice(json: &[u8]) -> Result<Self, CatalogError> {
        Self::from_reader(json)
    }

    /// Reads a gzipped catalog (the bundled snapshot).
    pub fn from_gzip(gz: &[u8]) -> Result<Self, CatalogError> {
        Self::from_reader(flate2::read::GzDecoder::new(gz))
    }

    /// The `etag` field: which build of the catalog this is.
    pub fn etag(&self) -> &str {
        &self.etag
    }

    /// When it was built (ISO 8601), as the file says.
    pub fn generated_at(&self) -> &str {
        &self.generated_at
    }

    pub fn len(&self) -> usize {
        self.apps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.apps.is_empty()
    }

    /// Bytes the index holds on the heap (capacities, not lengths).
    pub fn heap_bytes(&self) -> usize {
        self.arena.capacity()
            + self.apps.capacity() * std::mem::size_of::<App>()
            + self.exes.capacity() * std::mem::size_of::<Exe>()
            + self.keys.capacity() * std::mem::size_of::<(u64, u32)>()
            + self.steam.capacity() * std::mem::size_of::<(u32, u32)>()
            + self.steam_ids.capacity() * std::mem::size_of::<u32>()
            + self.etag.capacity()
            + self.generated_at.capacity()
    }

    fn text(&self, span: Span) -> &str {
        &self.arena[span.start as usize..(span.start + span.len) as usize]
    }

    fn push_text(&mut self, text: &str) -> Span {
        let start = u32::try_from(self.arena.len()).unwrap_or(u32::MAX);
        self.arena.push_str(text);
        Span {
            start,
            len: u32::try_from(text.len()).unwrap_or(0),
        }
    }

    /// One app from the file. Apps without a usable id, name or executable
    /// are skipped (the builder already drops them).
    fn add(&mut self, raw: RawApp) {
        let Ok(id) = raw.id.parse::<u64>() else {
            return;
        };
        let name = raw.name.trim();
        if name.is_empty() {
            return;
        }
        let usable: Vec<(String, u8)> = raw
            .executables
            .iter()
            .filter(|exe| !exe.is_launcher && exe.arguments.as_deref().is_none_or(str::is_empty))
            .filter_map(|exe| {
                let path = normalize(exe.name.trim());
                let (path, exact) = match path.strip_prefix('>') {
                    Some(rest) => (rest.to_owned(), EXACT),
                    None => (path, 0),
                };
                let path = path.trim_matches('/').to_owned();
                if path.is_empty() {
                    return None;
                }
                let folder = if path.contains('/') { FOLDER } else { 0 };
                Some((path, exact | folder))
            })
            .collect();
        if usable.is_empty() {
            return;
        }
        let app = u32::try_from(self.apps.len()).unwrap_or(u32::MAX);
        let name = self.push_text(name);
        let steam_start = u32::try_from(self.steam_ids.len()).unwrap_or(u32::MAX);
        for steam in raw
            .steam_app_ids
            .iter()
            .filter_map(|s| s.parse::<u32>().ok())
        {
            self.steam_ids.push(steam);
            self.steam.push((steam, app));
        }
        let steam_len = u32::try_from(self.steam_ids.len()).unwrap_or(0) - steam_start;
        self.apps.push(App {
            id,
            name,
            icon: raw.icon_hash.as_deref().and_then(hex16),
            steam: Span {
                start: steam_start,
                len: steam_len,
            },
        });
        for (path, flags) in usable {
            let span = self.push_text(&path);
            let index = u32::try_from(self.exes.len()).unwrap_or(u32::MAX);
            self.keys.push((key_hash(file_part(&path)), index));
            self.exes.push(Exe {
                app,
                path: span,
                flags,
            });
        }
    }

    /// Sorts the lookups and marks shared file names.
    fn finish(&mut self) -> Result<(), CatalogError> {
        if self.apps.is_empty() {
            return Err(CatalogError("no apps".into()));
        }
        self.keys.sort_unstable();
        self.steam.sort_unstable();
        // Shared: listed by more than one app (under the same file name).
        let mut i = 0;
        while i < self.keys.len() {
            let hash = self.keys[i].0;
            let mut j = i;
            while j < self.keys.len() && self.keys[j].0 == hash {
                j += 1;
            }
            // Several names can share a hash; group by the real name.
            let group: Vec<u32> = self.keys[i..j].iter().map(|(_, e)| *e).collect();
            for &exe in &group {
                let file = file_part(self.text(self.exes[exe as usize].path)).to_owned();
                let mut apps: Vec<u32> = group
                    .iter()
                    .filter(|&&other| file_part(self.text(self.exes[other as usize].path)) == file)
                    .map(|&other| self.exes[other as usize].app)
                    .collect();
                apps.sort_unstable();
                apps.dedup();
                if apps.len() > 1 || SHARED_NAMES.contains(&file.as_str()) {
                    self.exes[exe as usize].flags |= SHARED;
                }
            }
            i = j;
        }
        self.arena.shrink_to_fit();
        self.apps.shrink_to_fit();
        self.exes.shrink_to_fit();
        self.keys.shrink_to_fit();
        self.steam.shrink_to_fit();
        self.steam_ids.shrink_to_fit();
        Ok(())
    }

    /// The executables filed under a file name.
    fn under<'a>(&'a self, file: &'a str) -> impl Iterator<Item = &'a Exe> + 'a {
        let hash = key_hash(file);
        let start = self.keys.partition_point(|(h, _)| *h < hash);
        self.keys[start..]
            .iter()
            .take_while(move |(h, _)| *h == hash)
            .map(|(_, exe)| &self.exes[*exe as usize])
            .filter(move |exe| file_part(self.text(exe.path)) == file)
    }

    /// Whether any entry is filed under this (lower-case) file name.
    pub fn has_file(&self, file: &str) -> bool {
        self.under(file).next().is_some()
    }

    /// The app at `index`.
    pub fn app(&self, index: u32) -> Option<CatalogApp> {
        let app = self.apps.get(index as usize)?;
        let steam =
            &self.steam_ids[app.steam.start as usize..(app.steam.start + app.steam.len) as usize];
        Some(CatalogApp {
            id: app.id.to_string(),
            name: self.text(app.name).to_owned(),
            icon_hash: app
                .icon
                .map(|bytes| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()),
            steam_app_ids: steam.iter().map(u32::to_string).collect(),
        })
    }

    /// The app that lists this Steam app id, if one does.
    pub fn app_for_steam(&self, steam_id: u32) -> Option<u32> {
        let at = self.steam.partition_point(|(id, _)| *id < steam_id);
        self.steam
            .get(at)
            .filter(|(id, _)| *id == steam_id)
            .map(|(_, app)| *app)
    }

    /// Entries a full path may match: (exe, the compared string's length
    /// in segments), for [`crate::matcher`].
    pub(crate) fn entries_for<'a>(
        &'a self,
        compared: &'a str,
    ) -> impl Iterator<Item = Entry<'a>> + 'a {
        self.under(file_part(compared)).map(move |exe| Entry {
            app: exe.app,
            path: self.text(exe.path),
            exact: exe.flags & EXACT != 0,
            folder: exe.flags & FOLDER != 0,
            shared: exe.flags & SHARED != 0,
        })
    }
}

/// One executable entry, as the matcher sees it.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Entry<'a> {
    pub app: u32,
    pub path: &'a str,
    pub exact: bool,
    pub folder: bool,
    pub shared: bool,
}

// Streaming deserialization: the top-level object, then each app.

#[derive(Deserialize)]
struct RawExe {
    name: String,
    #[serde(default)]
    is_launcher: bool,
    #[serde(default)]
    arguments: Option<String>,
}

#[derive(Deserialize)]
struct RawApp {
    id: String,
    name: String,
    #[serde(default)]
    icon_hash: Option<String>,
    #[serde(default)]
    executables: Vec<RawExe>,
    #[serde(default)]
    steam_app_ids: Vec<String>,
}

impl<'de> DeserializeSeed<'de> for &mut Catalog {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_map(TopVisitor(self))
    }
}

struct TopVisitor<'a>(&'a mut Catalog);

impl<'de> Visitor<'de> for TopVisitor<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a game catalog object")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let mut version = None;
        let mut apps = false;
        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "version" => version = Some(map.next_value::<u64>()?),
                "etag" => self.0.etag = map.next_value()?,
                "generated_at" => self.0.generated_at = map.next_value()?,
                "apps" => {
                    if version.is_some_and(|v| v != CATALOG_VERSION) {
                        return Err(de::Error::custom("unsupported catalog version"));
                    }
                    map.next_value_seed(AppsSeed(self.0))?;
                    apps = true;
                }
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        if version != Some(CATALOG_VERSION) {
            return Err(de::Error::custom("unsupported catalog version"));
        }
        if !apps {
            return Err(de::Error::missing_field("apps"));
        }
        Ok(())
    }
}

struct AppsSeed<'a>(&'a mut Catalog);

impl<'de> DeserializeSeed<'de> for AppsSeed<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> Visitor<'de> for AppsSeed<'_> {
    type Value = ();

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a list of apps")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        // One malformed app is skipped, as the builder would have dropped it.
        while let Some(RawAppLenient(app)) = seq.next_element()? {
            if let Some(app) = app {
                self.0.add(app);
            }
        }
        Ok(())
    }
}

/// An app, or `None` when the entry does not have the shape of one.
struct RawAppLenient(Option<RawApp>);

impl<'de> Deserialize<'de> for RawAppLenient {
    fn deserialize<D: de::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        Ok(Self(serde_json::from_value(value).ok()))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) const FIXTURE: &str = include_str!("../tests/fixture-catalog.json");

    #[test]
    fn the_fixture_parses_into_an_index() {
        let catalog = Catalog::from_slice(FIXTURE.as_bytes()).expect("parses");
        assert_eq!(catalog.etag(), "fixture0001");
        // The app with only a launcher and the bad one are dropped.
        assert_eq!(catalog.len(), 9);
        assert!(catalog.has_file("overwatch.exe"));
        assert!(catalog.has_file("wow.exe"));
        assert!(!catalog.has_file("battle.net.exe"), "launchers are dropped");
        assert!(!catalog.has_file("argued.exe"), "arguments are dropped");
    }

    #[test]
    fn apps_keep_their_id_icon_and_steam_ids() {
        let catalog = Catalog::from_slice(FIXTURE.as_bytes()).expect("parses");
        let index = catalog.app_for_steam(570).expect("dota by steam id");
        let dota = catalog.app(index).expect("app");
        assert_eq!(dota.id, "356875988589740042");
        assert_eq!(dota.name, "Dota 2");
        assert_eq!(
            dota.icon_hash.as_deref(),
            Some("6b4b3fa4c83555d3008de69d33a60588")
        );
        assert_eq!(dota.steam_app_ids, vec!["570".to_owned()]);
        assert!(catalog.app_for_steam(1).is_none());
    }

    #[test]
    fn other_versions_and_shapes_are_refused() {
        for raw in [
            r#"{"version":2,"apps":[{"id":"1","name":"A","executables":[{"name":"a.exe"}]}]}"#,
            r#"{"apps":[{"id":"1","name":"A","executables":[{"name":"a.exe"}]}]}"#,
            r#"{"version":1}"#,
            r#"{"version":1,"apps":[]}"#,
            "<!doctype html><html></html>",
            "",
        ] {
            assert!(
                Catalog::from_slice(raw.as_bytes()).is_err(),
                "accepted {raw:?}"
            );
        }
    }

    #[test]
    fn the_bundled_snapshot_loads_under_budget() {
        let catalog = Catalog::from_gzip(crate::BUNDLED_CATALOG).expect("bundled");
        assert!(catalog.len() > 10_000, "{}", catalog.len());
        assert!(
            catalog.heap_bytes() < 3 * 1024 * 1024,
            "{}",
            catalog.heap_bytes()
        );
        assert_eq!(catalog.etag(), crate::bundled_meta().etag);
    }
}
