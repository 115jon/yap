//! `Games`: game detection (`yap-games`) wired to the core and to
//! Settings > Registered games. The detector starts with the core and runs
//! whether or not QML made this object; what it reports goes to the core as
//! `Command::SetGameActivity` (sent to the gateway only on a change), and
//! into `games.json` in the profile's data folder (last played, hidden and
//! added games, the detection switch).
//!
//! The catalog is the bundled snapshot or the copy a daily refresh keeps
//! (`/desktop/game-catalog.json` on the API origin, with its ETag).
//!
//! While the detector reports Deadlock, `yap_games::deadlock` follows the
//! game's `console.log` on its own thread and its live state rides along
//! in the activity's `game` field (throttled there to the gateway's op 43
//! limit); it stops when the game does. `YAP_TEST_DEADLOCK_LOG=<path>`
//! follows that file instead of the one in the Steam library.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        /// Detect the game being played (on by default).
        #[qproperty(bool, detection)]
        /// What is being played now (detected and not hidden), or empty.
        #[qproperty(QString, playing)]
        /// The games seen and added, most recent first: JSON
        /// `[{key, name, icon, lastPlayed, hidden, added, playing}]`
        /// (`icon` absolute or empty, `lastPlayed` epoch ms or 0).
        #[qproperty(QString, games)]
        /// The programs with a window, after `loadRunning`: JSON
        /// `[{exe, title, added}]`.
        #[qproperty(QString, running)]
        #[qproperty(bool, loading)]
        /// Deadlock's live stats: JSON `{running, status, phase, hero,
        /// launchUrl, launchOption}`; `status` is `waiting`, `live`,
        /// `missing` (no console.log) or `stale` (an earlier run's), empty
        /// while Deadlock is not running.
        #[qproperty(QString, deadlock)]
        type Games = super::GamesRust;

        #[qinvokable]
        #[cxx_name = "enableDetection"]
        fn enable_detection(self: Pin<&mut Games>, on: bool);

        /// Never show `key` (or show it again).
        #[qinvokable]
        #[cxx_name = "setHidden"]
        fn set_hidden(self: Pin<&mut Games>, key: &QString, hidden: bool);

        /// Registers a running program as a game, named `name`.
        #[qinvokable]
        #[cxx_name = "addProgram"]
        fn add_program(self: Pin<&mut Games>, exe: &QString, name: &QString);

        /// Forgets a game added by hand (`exe:<file name>`), or one seen.
        #[qinvokable]
        fn remove(self: Pin<&mut Games>, key: &QString);

        /// Lists the programs with a window ("Add it"), off the GUI thread.
        #[qinvokable]
        #[cxx_name = "loadRunning"]
        fn load_running(self: Pin<&mut Games>);

        /// Starts Deadlock through Steam with `-condebug` (Steam asks
        /// first). `YAP_LOG_EXTERNAL_OPEN=1` only logs it.
        #[qinvokable]
        #[cxx_name = "launchDeadlock"]
        fn launch_deadlock(self: Pin<&mut Games>);
    }

    impl cxx_qt::Threading for Games {}
    impl cxx_qt::Initialize for Games {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock, PoisonError};

use cxx_qt::{CxxQtThread, Threading};
use cxx_qt_lib::QString;
use yap_cache::settings::{self, AddedGame, GamesPrefs};
use yap_core::store::Command;
use yap_games::deadlock::{self, watch::LogStatus};
use yap_games::{Detected, FakeProcess, Rules};
use yap_protocol::game_activity::{DeadlockLiveState, GameActivity};

use crate::client;

#[derive(Default)]
pub struct GamesRust {
    detection: bool,
    playing: QString,
    games: QString,
    running: QString,
    loading: bool,
    deadlock: QString,
}

struct Shared {
    dir: Option<PathBuf>,
    prefs: GamesPrefs,
    current: Option<Detected>,
    fake: Option<Vec<FakeProcess>>,
    #[cfg(windows)]
    detector: Option<yap_games::detector::Detector>,
    qt: Option<CxxQtThread<qobject::Games>>,
    /// The Deadlock run being followed (its `started_at`), its watcher,
    /// what its log says and whether the log is being written.
    watching: Option<i64>,
    watcher: Option<deadlock::watch::Watcher>,
    live: Option<DeadlockLiveState>,
    log_status: Option<LogStatus>,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = settings::app_dir();
        let prefs = dir.as_deref().map(settings::load_games).unwrap_or_default();
        Mutex::new(Shared {
            dir,
            prefs,
            current: None,
            fake: yap_games::test_processes(),
            #[cfg(windows)]
            detector: None,
            qt: None,
            watching: None,
            watcher: None,
            live: None,
            log_status: None,
        })
    })
}

fn with<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    f(&mut shared().lock().unwrap_or_else(PoisonError::into_inner))
}

fn rules(prefs: &GamesPrefs) -> Rules {
    Rules {
        enabled: prefs.detection,
        hidden: prefs.hidden.clone(),
        added: prefs
            .added
            .iter()
            .map(|game| (game.exe.to_lowercase(), game.name.clone()))
            .collect(),
    }
}

fn save(s: &Shared) {
    if let Some(dir) = &s.dir {
        if let Err(err) = settings::save_games(dir, &s.prefs) {
            eprintln!("[games] could not save games.json: {err}");
        }
    }
}

/// What goes to the core: the detected game, with Deadlock's live state.
fn activity(s: &Shared) -> Option<Box<GameActivity>> {
    let game = s.current.as_ref()?;
    let live = s.live.as_ref().filter(|_| s.watching.is_some());
    Some(Box::new(deadlock::with_live(game.activity(), live)))
}

/// Tells the core (under the lock, so two threads never reorder updates).
fn send_activity(s: &Shared) {
    client::command(Command::SetGameActivity(activity(s)));
}

/// Follows Deadlock's log for the run that started at `started` (epoch ms).
fn watch_deadlock(started: i64) -> Option<deadlock::watch::Watcher> {
    let Some(path) = deadlock::locate::console_log() else {
        eprintln!("[games] Deadlock's install was not found: no live stats");
        return None;
    };
    let config = deadlock::watch::WatchConfig {
        path,
        game_started_at: started,
        trace: std::env::var("YAP_TRACE_GAMES").is_ok_and(|v| v == "1"),
    };
    deadlock::watch::Watcher::spawn(
        config,
        Box::new(move |live| {
            let qt = with(|s| {
                if s.watching != Some(started) {
                    return None;
                }
                s.live = live;
                send_activity(s);
                s.qt.clone()
            });
            if let Some(qt) = qt {
                let _ = qt.queue(refresh);
            }
        }),
        Box::new(move |status| {
            eprintln!("[games] Deadlock log {}", status.as_str());
            let qt = with(|s| {
                if s.watching != Some(started) {
                    return None;
                }
                s.log_status = Some(status);
                s.qt.clone()
            });
            if let Some(qt) = qt {
                let _ = qt.queue(refresh);
            }
        }),
    )
}

/// Starts or stops following Deadlock's log with what the detector says.
fn follow_deadlock(detected: Option<&Detected>) {
    let run = detected
        .filter(|game| deadlock::is_deadlock(game))
        .map(|game| game.started_at);
    let (stopped, start) = with(|s| {
        if s.watching == run {
            return (None, None);
        }
        s.watching = run;
        s.live = None;
        s.log_status = run.map(|_| LogStatus::Waiting);
        (s.watcher.take(), run)
    });
    // Joined outside the lock: its callbacks take it.
    drop(stopped);
    if let Some(started) = start {
        let watcher = watch_deadlock(started);
        with(|s| {
            if s.watching == Some(started) {
                s.watcher = watcher;
            }
        });
    }
}

fn deadlock_json(s: &Shared) -> String {
    let live = s.live.as_ref();
    serde_json::json!({
        "running": s.watching.is_some(),
        "status": s.log_status.map_or("", LogStatus::as_str),
        "phase": live.map_or("", |l| l.phase.as_str()),
        "hero": live.and_then(|l| l.hero.as_deref()).unwrap_or_default(),
        "launchUrl": deadlock::LAUNCH_URL,
        "launchOption": deadlock::LAUNCH_OPTION,
    })
    .to_string()
}

/// The detector heard of a change (on its thread).
fn on_change(detected: Option<Detected>) {
    let now = yap_games::refresh::now_ms();
    follow_deadlock(detected.as_ref());
    let qt = with(|s| {
        if let Some(game) = &detected {
            let icon = game.icon_path().unwrap_or_default();
            s.prefs.played(&game.key, &game.name, &icon, now);
        }
        // The game that stopped was played until now.
        if let Some(previous) = s.current.take() {
            if let Some(entry) = s.prefs.played.iter_mut().find(|g| g.key == previous.key) {
                entry.last_played = now;
            }
        }
        s.current.clone_from(&detected);
        save(s);
        send_activity(s);
        s.qt.clone()
    });
    if let Some(qt) = qt {
        let _ = qt.queue(refresh);
    }
}

/// Starts detection and the catalog refresh, once (with the core).
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        #[cfg(windows)]
        with(|s| {
            let config = yap_games::detector::Config {
                data_dir: s.dir.clone(),
                fake: s.fake.clone(),
                trace: std::env::var("YAP_TRACE_GAMES").is_ok_and(|v| v == "1"),
            };
            s.detector =
                yap_games::detector::Detector::spawn(config, rules(&s.prefs), Box::new(on_change));
            if s.detector.is_none() {
                eprintln!("[games] could not start detection");
            }
            if let (Some(dir), None) = (s.dir.clone(), &s.fake) {
                yap_games::refresh::spawn(
                    dir,
                    Box::new(fetch_catalog),
                    Box::new(|catalog| {
                        with(|s| {
                            if let Some(detector) = &s.detector {
                                detector.set_catalog(catalog);
                            }
                        });
                    }),
                );
            }
        });
    });
}

/// The catalog from the API origin (`YAP_API_URL`), with the last ETag.
#[cfg(windows)]
fn fetch_catalog(etag: &str) -> Result<yap_games::refresh::Fetched, String> {
    use yap_games::refresh::{Fetched, CATALOG_PATH, FETCH_TIMEOUT, MAX_BODY_BYTES};
    let url = format!("{}{CATALOG_PATH}", yap_core::config::get().api_base);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(async {
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .user_agent(concat!("YapNative/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| e.to_string())?;
        let mut request = client.get(&url);
        if !etag.is_empty() {
            request = request.header(reqwest::header::IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|e| e.to_string())?;
        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(Fetched::NotModified);
        }
        let response = response.error_for_status().map_err(|e| e.to_string())?;
        if response
            .content_length()
            .is_some_and(|len| len > MAX_BODY_BYTES as u64)
        {
            return Err("catalog too large".into());
        }
        let etag = response
            .headers()
            .get(reqwest::header::ETAG)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        Ok(Fetched::Body {
            bytes: bytes.to_vec(),
            etag,
        })
    })
}

#[cfg(not(windows))]
fn fetch_catalog(_etag: &str) -> Result<yap_games::refresh::Fetched, String> {
    Err("no detection here".into())
}

/// The prefs changed: saved, and the detector follows.
fn changed(s: &mut Shared) {
    save(s);
    #[cfg(windows)]
    if let Some(detector) = &s.detector {
        detector.set_rules(rules(&s.prefs));
    }
}

fn games_json(s: &Shared) -> String {
    let base = &yap_core::config::get().api_base;
    let playing = s.current.as_ref().map(|g| g.key.as_str());
    let icon = |path: &str| {
        if path.is_empty() {
            String::new()
        } else {
            format!("{base}{path}")
        }
    };
    let mut rows: Vec<serde_json::Value> = s
        .prefs
        .played
        .iter()
        .map(|game| {
            serde_json::json!({
                "key": game.key,
                "name": game.name,
                "icon": icon(&game.icon),
                "lastPlayed": game.last_played,
                "hidden": s.prefs.is_hidden(&game.key),
                "added": s.prefs.added.iter().any(|a| format!("exe:{}", a.exe) == game.key),
                "playing": playing == Some(game.key.as_str()),
            })
        })
        .collect();
    // Added games not seen running yet.
    for added in &s.prefs.added {
        let key = format!("exe:{}", added.exe);
        if !s.prefs.played.iter().any(|g| g.key == key) {
            rows.push(serde_json::json!({
                "key": key,
                "name": added.name,
                "icon": "",
                "lastPlayed": 0,
                "hidden": s.prefs.is_hidden(&key),
                "added": true,
                "playing": false,
            }));
        }
    }
    serde_json::Value::Array(rows).to_string()
}

/// Puts the shared state on the object.
fn refresh(mut object: Pin<&mut qobject::Games>) {
    let (detection, playing, games, live) = with(|s| {
        let playing = s
            .current
            .as_ref()
            .filter(|g| !s.prefs.is_hidden(&g.key))
            .map(|g| g.name.clone())
            .unwrap_or_default();
        (s.prefs.detection, playing, games_json(s), deadlock_json(s))
    });
    object.as_mut().set_detection(detection);
    object.as_mut().set_playing(QString::from(&playing));
    object.as_mut().set_deadlock(QString::from(&live));
    object.set_games(QString::from(&games));
}

impl cxx_qt::Initialize for qobject::Games {
    fn initialize(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        with(|s| s.qt = Some(qt));
        refresh(self);
    }
}

impl Drop for GamesRust {
    fn drop(&mut self) {
        with(|s| s.qt = None);
    }
}

impl qobject::Games {
    pub fn enable_detection(self: Pin<&mut Self>, on: bool) {
        with(|s| {
            s.prefs.detection = on;
            changed(s);
        });
        refresh(self);
    }

    pub fn set_hidden(self: Pin<&mut Self>, key: &QString, hidden: bool) {
        let key = key.to_string();
        with(|s| {
            s.prefs.set_hidden(&key, hidden);
            changed(s);
        });
        refresh(self);
    }

    pub fn add_program(mut self: Pin<&mut Self>, exe: &QString, name: &QString) {
        let exe = exe.to_string().trim().to_lowercase();
        let name: String = name.to_string().trim().chars().take(128).collect();
        if exe.is_empty() || exe.contains(['/', '\\']) {
            return;
        }
        let name = if name.is_empty() {
            exe.rsplit_once('.')
                .map_or(exe.as_str(), |(stem, _)| stem)
                .to_owned()
        } else {
            name
        };
        with(|s| {
            s.prefs.added.retain(|a| a.exe != exe);
            s.prefs.added.push(AddedGame { exe, name });
            changed(s);
        });
        self.as_mut().mark_running();
        refresh(self);
    }

    pub fn remove(self: Pin<&mut Self>, key: &QString) {
        let key = key.to_string();
        with(|s| {
            if let Some(exe) = key.strip_prefix("exe:") {
                s.prefs.added.retain(|a| a.exe != exe);
            }
            s.prefs.played.retain(|g| g.key != key);
            s.prefs.set_hidden(&key, false);
            changed(s);
        });
        refresh(self);
    }

    pub fn load_running(mut self: Pin<&mut Self>) {
        if *self.loading() {
            return;
        }
        self.as_mut().set_loading(true);
        let qt = self.qt_thread();
        let fake = with(|s| s.fake.clone());
        std::thread::spawn(move || {
            #[cfg(windows)]
            let programs = yap_games::detector::running_programs(fake.as_deref());
            #[cfg(not(windows))]
            let programs: Vec<yap_games::RunningProgram> = {
                let _ = fake;
                Vec::new()
            };
            let _ = qt.queue(move |mut object| {
                let list: Vec<serde_json::Value> = programs
                    .iter()
                    .map(|p| serde_json::json!({ "exe": p.exe, "title": p.title }))
                    .collect();
                object
                    .as_mut()
                    .set_running(QString::from(&serde_json::Value::Array(list).to_string()));
                object.as_mut().mark_running();
                object.set_loading(false);
            });
        });
    }

    pub fn launch_deadlock(self: Pin<&mut Self>) {
        let url = deadlock::LAUNCH_URL;
        if std::env::var("YAP_LOG_EXTERNAL_OPEN").is_ok_and(|v| v != "0" && !v.is_empty()) {
            eprintln!("[external] open {url}");
            return;
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::UI::Shell::ShellExecuteW;
            let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
            let (verb, file) = (wide("open"), wide(url));
            // SAFETY: both strings are NUL-terminated and outlive the call;
            // 1 is SW_SHOWNORMAL. The URL is our constant, never input.
            unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    file.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                );
            }
        }
    }

    /// `added` on each listed program, from the prefs.
    fn mark_running(self: Pin<&mut Self>) {
        let list: Vec<serde_json::Value> =
            serde_json::from_str(&self.running().to_string()).unwrap_or_default();
        let list: Vec<serde_json::Value> = with(|s| {
            list.into_iter()
                .map(|mut row| {
                    let exe = row["exe"].as_str().unwrap_or_default().to_lowercase();
                    row["added"] = serde_json::json!(s.prefs.added.iter().any(|a| a.exe == exe));
                    row
                })
                .collect()
        });
        self.set_running(QString::from(&serde_json::Value::Array(list).to_string()));
    }
}
