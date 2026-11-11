//! `CustomSounds`: Settings → Notifications → Custom sounds
//! (CustomSoundsCard.tsx, useCustomSoundsStore.ts). An imported file is
//! checked ([`yap_core::custom_sounds`]), decoded off the GUI thread to
//! learn its length, then copied to
//! `%LOCALAPPDATA%\Yap.Data\custom-sounds\<stem>.<ext>` and listed in
//! `custom-sounds.json`. Nothing is uploaded. [`crate::sound`] plays the
//! file instead of the built-in sound while "Use custom sounds" is on.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
        include!("cxx-qt-lib/qstringlist.h");
        type QStringList = cxx_qt_lib::QStringList;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// `{any, rows: [{id, label, stem, name, seconds, custom, busy,
        /// error}], bulkBusy, bulkLine, bulkFailures: [..]}`.
        #[qproperty(QString, state)]
        type CustomSounds = super::CustomSoundsRust;

        /// One file for one sound (the row's import key).
        #[qinvokable]
        #[cxx_name = "importFile"]
        fn import_file(self: Pin<&mut CustomSounds>, id: &QString, path: &QString);

        /// Several files, each matched to a sound by its name.
        #[qinvokable]
        #[cxx_name = "importFiles"]
        fn import_files(self: Pin<&mut CustomSounds>, paths: &QStringList);

        /// Back to the built-in sound.
        #[qinvokable]
        fn reset(self: Pin<&mut CustomSounds>, id: &QString);

        #[qinvokable]
        #[cxx_name = "resetAll"]
        fn reset_all(self: Pin<&mut CustomSounds>);

        /// Plays the sound as it would play now (custom or built-in),
        /// whatever the switches say.
        #[qinvokable]
        fn preview(self: &CustomSounds, id: &QString);
    }

    impl cxx_qt::Initialize for CustomSounds {}
    impl cxx_qt::Threading for CustomSounds {}
}

use core::pin::Pin;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QList, QString, QStringList};
use serde_json::json;
use yap_cache::settings::app_dir;
use yap_core::custom_sounds::{self as cs, CustomSound, Refusal};

use crate::gui;

const INDEX: &str = "custom-sounds.json";
const FOLDER: &str = "custom-sounds";

#[derive(Default)]
pub struct CustomSoundsRust {
    state: QString,
    /// Sounds being imported, and the last refusal per sound.
    busy: HashSet<String>,
    errors: HashMap<String, String>,
    bulk_busy: bool,
    bulk: Option<(usize, Vec<String>)>,
}

/// The imported files, by sound id, shared with the player.
fn library() -> &'static Mutex<Option<BTreeMap<String, CustomSound>>> {
    static LIBRARY: OnceLock<Mutex<Option<BTreeMap<String, CustomSound>>>> = OnceLock::new();
    LIBRARY.get_or_init(Mutex::default)
}

fn with_library<T>(f: impl FnOnce(&mut BTreeMap<String, CustomSound>) -> T) -> T {
    let mut guard = library().lock().unwrap_or_else(PoisonError::into_inner);
    let map = guard.get_or_insert_with(load_index);
    f(map)
}

fn folder() -> Option<PathBuf> {
    app_dir().map(|dir| dir.join(FOLDER))
}

/// The index on disk. Rows that decode are kept even when others do not;
/// when the file cannot be read at all, it is rebuilt from the custom
/// sounds folder instead of starting empty (the next save would wipe it).
fn load_index() -> BTreeMap<String, CustomSound> {
    let Some(dir) = app_dir() else {
        return BTreeMap::new();
    };
    let Ok(text) = std::fs::read_to_string(dir.join(INDEX)) else {
        // No index (or unreadable): files of a lost index are still there.
        return if dir.join(INDEX).exists() {
            rebuild_from_folder(&dir.join(FOLDER))
        } else {
            BTreeMap::new()
        };
    };
    cs::parse_index(&text).unwrap_or_else(|| rebuild_from_folder(&dir.join(FOLDER)))
}

fn rebuild_from_folder(folder: &Path) -> BTreeMap<String, CustomSound> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return BTreeMap::new();
    };
    let files = entries.filter_map(|entry| {
        let path = entry.ok()?.path();
        let file = path.file_name()?.to_str()?.to_owned();
        cs::id_for_filename(&file)?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())?
            .to_ascii_lowercase();
        let (_, duration) =
            yap_audio::decode_sound(&std::fs::read(&path).ok()?, Some(&ext)).ok()?;
        let meta = std::fs::metadata(&path).ok()?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        Some((file, meta.len(), duration, modified))
    });
    cs::rebuild_index(files)
}

fn save_index(map: &BTreeMap<String, CustomSound>) {
    let (Some(dir), Ok(text)) = (app_dir(), serde_json::to_string_pretty(map)) else {
        return;
    };
    let tmp = dir.join(format!("{INDEX}.tmp"));
    if std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&tmp, text).is_ok() {
        let _ = std::fs::rename(&tmp, dir.join(INDEX));
    }
}

/// The imported file for a sound, if there is one.
pub(crate) fn file_for(id: &str) -> Option<PathBuf> {
    let name = with_library(|map| map.get(id).map(|s| s.file.clone()))?;
    folder().map(|dir| dir.join(name))
}

fn content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("mp3") => "audio/mpeg",
        Some("wav") => "audio/wav",
        Some("ogg" | "oga") => "audio/ogg",
        Some("opus") => "audio/opus",
        Some("webm") => "audio/webm",
        Some("m4a" | "aac") => "audio/mp4",
        Some("flac") => "audio/flac",
        _ => "",
    }
}

/// Checks, decodes and copies one file; the refusal's sentence otherwise.
/// Blocking: runs on a worker thread.
fn import(id: &str, path: &Path) -> Result<CustomSound, Refusal> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    cs::check_file(&name, content_type(path), size)?;
    let bytes = std::fs::read(path).map_err(|_| cs::decode_refusal(&name))?;
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("bin")
        .to_ascii_lowercase();
    let (_, seconds) =
        yap_audio::decode_sound(&bytes, Some(&ext)).map_err(|_| cs::decode_refusal(&name))?;
    cs::check_duration(id, &name, seconds)?;
    let dir = folder().ok_or_else(|| cs::decode_refusal(&name))?;
    let file = format!("{}.{ext}", cs::file_stem(id));
    std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::write(dir.join(&file), &bytes))
        .map_err(|_| cs::decode_refusal(&name))?;
    crate::sound::forget_custom(id);
    Ok(CustomSound {
        name,
        file,
        size,
        duration: seconds,
        updated_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX)),
    })
}

fn remember(id: &str, sound: CustomSound) {
    with_library(|map| {
        // An earlier file with another extension goes.
        if let Some(old) = map.get(id).filter(|old| old.file != sound.file) {
            if let Some(dir) = folder() {
                let _ = std::fs::remove_file(dir.join(&old.file));
            }
        }
        map.insert(id.to_owned(), sound);
        save_index(map);
    });
}

fn forget(id: &str) {
    with_library(|map| {
        if let Some(old) = map.remove(id) {
            if let Some(dir) = folder() {
                let _ = std::fs::remove_file(dir.join(old.file));
            }
            save_index(map);
        }
    });
    crate::sound::forget_custom(id);
}

fn render(rust: &CustomSoundsRust) -> QString {
    let rows: Vec<serde_json::Value> = cs::SOUND_IDS
        .iter()
        .map(|id| {
            let sound = with_library(|map| map.get(*id).cloned());
            json!({
                "id": id,
                "label": cs::label(id),
                "stem": cs::file_stem(id),
                "name": sound.as_ref().map(|s| s.name.clone()).unwrap_or_default(),
                "seconds": sound.as_ref().map(|s| cs::seconds_label(s.duration)).unwrap_or_default(),
                "custom": sound.is_some(),
                "busy": rust.busy.contains(*id),
                "error": rust.errors.get(*id).cloned().unwrap_or_default(),
            })
        })
        .collect();
    let any = with_library(|map| !map.is_empty());
    let state = json!({
        "any": any,
        "rows": rows,
        "bulkBusy": rust.bulk_busy,
        "bulkLine": rust.bulk.as_ref().map(|(n, _)| cs::imported_line(*n)).unwrap_or_default(),
        "bulkFailures": rust.bulk.as_ref().map(|(_, f)| f.clone()).unwrap_or_default(),
    });
    QString::from(&state.to_string())
}

fn refresh(mut object: Pin<&mut qobject::CustomSounds>) {
    let state = render(object.as_ref().rust());
    object.as_mut().set_state(state);
}

impl gui::Slot for qobject::CustomSounds {
    yap_ui::slot_cell!(qobject::CustomSounds);

    fn restore(self: Pin<&mut Self>) {
        refresh(self);
    }
}

impl cxx_qt::Initialize for qobject::CustomSounds {
    fn initialize(self: Pin<&mut Self>) {
        gui::register(self);
    }
}

impl Drop for CustomSoundsRust {
    fn drop(&mut self) {
        gui::unregister::<qobject::CustomSounds>();
    }
}

/// Importing a file is a clear sign the person wants to hear it: the
/// switch turns on (CustomSoundsCard.tsx `onImported`).
fn switch_on() {
    if let Some(mut desktop) = gui::object::<crate::desktop::qobject::Desktop>() {
        if !*desktop.custom_sounds() {
            desktop
                .as_mut()
                .set_setting(&QString::from("customSounds"), true);
        }
    }
}

type Outcome = (String, Result<CustomSound, Refusal>);

impl qobject::CustomSounds {
    pub fn import_file(mut self: Pin<&mut Self>, id: &QString, path: &QString) {
        let (id, path) = (id.to_string(), PathBuf::from(path.to_string()));
        let Some(&id) = cs::SOUND_IDS.iter().find(|known| **known == id) else {
            return;
        };
        // A bulk import owns every sound it matched until it is done.
        if self.rust().bulk_busy || !self.as_mut().rust_mut().busy.insert(id.to_owned()) {
            return;
        }
        self.as_mut().rust_mut().errors.remove(id);
        refresh(self.as_mut());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let outcome = import(id, &path);
            let _ = qt.queue(move |mut object| {
                object.as_mut().rust_mut().busy.remove(id);
                match outcome {
                    Ok(sound) => {
                        remember(id, sound);
                        switch_on();
                    }
                    Err(refusal) => {
                        object
                            .as_mut()
                            .rust_mut()
                            .errors
                            .insert(id.to_owned(), refusal.message().to_owned());
                    }
                }
                refresh(object);
            });
        });
    }

    pub fn import_files(mut self: Pin<&mut Self>, paths: &QStringList) {
        let paths: Vec<PathBuf> = QList::<QString>::from(paths)
            .iter()
            .map(|p| PathBuf::from(p.to_string()))
            .collect();
        // Each file's sound, found here so the sounds are marked busy
        // before the worker starts.
        let jobs: Vec<(PathBuf, Result<&'static str, Refusal>)> = paths
            .into_iter()
            .map(|path| {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let id = cs::id_for_filename(&name).ok_or_else(|| cs::unmatched(&name));
                (path, id)
            })
            .collect();
        let ids: HashSet<&'static str> = jobs
            .iter()
            .filter_map(|(_, id)| id.as_ref().ok().copied())
            .collect();
        {
            let mut rust = self.as_mut().rust_mut();
            // One bulk import at a time, and never over a row's own import.
            if jobs.is_empty()
                || rust.bulk_busy
                || rust.busy.iter().any(|busy| ids.contains(busy.as_str()))
            {
                return;
            }
            rust.bulk_busy = true;
            rust.busy.extend(ids.iter().map(|id| (*id).to_owned()));
        }
        refresh(self.as_mut());
        let qt = self.qt_thread();
        std::thread::spawn(move || {
            let outcomes: Vec<Result<Outcome, Refusal>> = jobs
                .into_iter()
                .map(|(path, id)| {
                    let id = id?;
                    Ok((id.to_owned(), import(id, &path)))
                })
                .collect();
            let _ = qt.queue(move |mut object| {
                let mut imported = 0;
                let mut failures = Vec::new();
                for outcome in outcomes {
                    match outcome {
                        Ok((id, Ok(sound))) => {
                            remember(&id, sound);
                            imported += 1;
                        }
                        Ok((_, Err(refusal))) | Err(refusal) => {
                            failures.push(refusal.message().to_owned());
                        }
                    }
                }
                if imported > 0 {
                    switch_on();
                }
                let mut rust = object.as_mut().rust_mut();
                rust.bulk_busy = false;
                rust.busy.retain(|busy| !ids.contains(busy.as_str()));
                rust.bulk = Some((imported, failures));
                refresh(object);
            });
        });
    }

    pub fn reset(mut self: Pin<&mut Self>, id: &QString) {
        let id = id.to_string();
        forget(&id);
        self.as_mut().rust_mut().errors.remove(&id);
        refresh(self);
    }

    pub fn reset_all(mut self: Pin<&mut Self>) {
        for id in cs::SOUND_IDS {
            forget(id);
        }
        if let Some(dir) = folder() {
            let _ = std::fs::remove_dir(dir);
        }
        let mut rust = self.as_mut().rust_mut();
        rust.errors.clear();
        rust.bulk = None;
        refresh(self);
    }

    pub fn preview(&self, id: &QString) {
        crate::sound::preview(&id.to_string());
    }
}
