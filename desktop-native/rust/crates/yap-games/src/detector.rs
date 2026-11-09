//! The detector: one thread that keeps track of which game runs and tells
//! the app when that changes (never per scan).
//!
//! A scan takes a ToolHelp snapshot (pids and executable names, no process
//! opened) and looks only at processes it has not seen. One whose name the
//! catalog lists (or the user added) is opened with
//! `QUERY_LIMITED_INFORMATION | SYNCHRONIZE` for its image path and kept
//! open to notice its exit; a refused open falls back to the name alone
//! ([`crate::matcher::match_name_only`]) and to the poll.
//!
//! Scans run when the foreground window changes (`SetWinEventHook`,
//! out of context, at most one per 2 s), when Steam's `RunningAppID`
//! changes, when a game process exits, and on a poll: every 20 s with no
//! game, 60 s while one runs. With several games, the one most recently in
//! the foreground is reported.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{HANDLE, HWND, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows_sys::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, GetThreadTimes, SetEvent, INFINITE,
};
use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, MsgWaitForMultipleObjects, PeekMessageW, TranslateMessage,
    EVENT_SYSTEM_FOREGROUND, MSG, PM_REMOVE, QS_ALLINPUT, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS,
};

use crate::catalog::{normalize, Catalog};
use crate::steam::SteamWatch;
use crate::win::{self, Owned};
use crate::{matcher, Detected, FakeProcess, Rules, RunningProgram, Source};

/// A foreground change rescans after this long, once.
pub const FOREGROUND_DEBOUNCE: Duration = Duration::from_secs(2);
pub const POLL_IDLE: Duration = Duration::from_secs(20);
pub const POLL_PLAYING: Duration = Duration::from_secs(60);
/// MsgWaitForMultipleObjects takes at most 64 handles (the wake and Steam
/// events go first).
const MAX_WAITS: usize = 60;

/// How the detector runs.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// The profile's data folder (the cached catalog).
    pub data_dir: Option<PathBuf>,
    /// `YAP_TEST_GAME_PROCESSES`: this list stands in for the snapshot.
    pub fake: Option<Vec<FakeProcess>>,
    /// `YAP_TRACE_GAMES=1`: each scan and change in the log.
    pub trace: bool,
}

/// What the detector costs so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub scans: u64,
    /// Processes opened (only ones the catalog or the user names).
    pub opened: u64,
    /// The thread's CPU time (user + kernel), as Windows samples it (in
    /// clock ticks of about 15.6 ms).
    pub cpu: Duration,
    /// Wall time spent inside scans, measured around each one.
    pub busy: Duration,
    /// The thread's CPU cycles (`QueryThreadCycleTime`, exact).
    pub cycles: u64,
    /// The catalog's index on the heap, when loaded.
    pub catalog_bytes: usize,
}

#[derive(Default)]
struct Control {
    rules: Rules,
    rules_changed: bool,
    catalog: Option<Catalog>,
    quit: bool,
    stats: Stats,
}

struct Event(Owned);

// SAFETY: an event handle may be set from any thread.
unsafe impl Sync for Event {}

/// The running detector; dropping it stops the thread.
pub struct Detector {
    control: Arc<Mutex<Control>>,
    wake: Arc<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Detector {
    /// Starts the thread; `on_change` runs on it with each new answer.
    pub fn spawn(
        config: Config,
        rules: Rules,
        on_change: Box<dyn FnMut(Option<Detected>) + Send>,
    ) -> Option<Self> {
        // SAFETY: an unnamed auto-reset event, owned by `Owned`.
        let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, std::ptr::null()) };
        if event.is_null() {
            return None;
        }
        let wake = Arc::new(Event(Owned(event)));
        let control = Arc::new(Mutex::new(Control {
            rules,
            ..Control::default()
        }));
        let thread = std::thread::Builder::new()
            .name("YapGames".into())
            .spawn({
                let control = Arc::clone(&control);
                let wake = Arc::clone(&wake);
                move || Worker::new(config, control, on_change).run(wake.0 .0)
            })
            .ok()?;
        Some(Self {
            control,
            wake,
            thread: Some(thread),
        })
    }

    fn with(&self, f: impl FnOnce(&mut Control)) {
        f(&mut self.control.lock().unwrap_or_else(PoisonError::into_inner));
        // SAFETY: the event lives as long as `self.wake`.
        unsafe { SetEvent(self.wake.0 .0) };
    }

    /// New choices (on/off, hidden, added): applied at once.
    pub fn set_rules(&self, rules: Rules) {
        self.with(|c| {
            c.rules = rules;
            c.rules_changed = true;
        });
    }

    /// A refreshed catalog replaces the loaded one.
    pub fn set_catalog(&self, catalog: Catalog) {
        self.with(|c| c.catalog = Some(catalog));
    }

    pub fn stats(&self) -> Stats {
        self.control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stats
    }
}

impl Drop for Detector {
    fn drop(&mut self) {
        self.with(|c| c.quit = true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// A process seen by a scan.
struct Tracked {
    name: String,
    hit: Option<Hit>,
    wait: Option<Owned>,
    first_seen: i64,
}

#[derive(Debug, Clone)]
struct Hit {
    key: String,
    name: String,
    app: Option<u32>,
    source: Source,
}

thread_local! {
    /// Foreground order by pid (higher: more recent), and when a
    /// foreground change asked for a scan.
    static FOREGROUND: RefCell<(u64, HashMap<u32, u64>, Option<Instant>)> =
        RefCell::new((0, HashMap::new(), None));
}

fn note_foreground(pid: u32) {
    if pid == 0 {
        return;
    }
    FOREGROUND.with(|fg| {
        let mut fg = fg.borrow_mut();
        let current = fg.1.get(&pid).copied();
        if current.is_some() && current == Some(fg.0) {
            return;
        }
        fg.0 += 1;
        let seq = fg.0;
        fg.1.insert(pid, seq);
    });
}

unsafe extern "system" fn on_foreground(
    _hook: HWINEVENTHOOK,
    _event: u32,
    window: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    note_foreground(win::window_pid(window));
    FOREGROUND.with(|fg| {
        let mut fg = fg.borrow_mut();
        if fg.2.is_none() {
            fg.2 = Some(Instant::now() + FOREGROUND_DEBOUNCE);
        }
    });
}

fn foreground_seq(pid: u32) -> u64 {
    FOREGROUND.with(|fg| fg.borrow().1.get(&pid).copied().unwrap_or(0))
}

struct Worker {
    config: Config,
    control: Arc<Mutex<Control>>,
    on_change: Box<dyn FnMut(Option<Detected>) + Send>,
    rules: Rules,
    catalog: Option<Catalog>,
    tracked: HashMap<u32, Tracked>,
    reported: Option<Detected>,
    windir: String,
    own_pid: u32,
    steam: Option<SteamWatch>,
    /// The running Steam app and when it was first seen.
    steam_app: (u32, i64),
    /// The next scan matches every process again (new rules or catalog),
    /// keeping when each was first seen.
    reclassify: bool,
    stats: Stats,
}

impl Worker {
    fn new(
        config: Config,
        control: Arc<Mutex<Control>>,
        on_change: Box<dyn FnMut(Option<Detected>) + Send>,
    ) -> Self {
        // The fake process list stands alone: no Steam either.
        let steam = if config.fake.is_some() {
            None
        } else {
            SteamWatch::open()
        };
        Self {
            config,
            control,
            on_change,
            rules: Rules::default(),
            catalog: None,
            tracked: HashMap::new(),
            reported: None,
            windir: win::windows_dir(),
            own_pid: win::own_pid(),
            steam,
            steam_app: (0, 0),
            reclassify: false,
            stats: Stats::default(),
        }
    }

    fn run(mut self, wake: HANDLE) {
        // SAFETY: an out-of-context hook delivers to this thread's queue,
        // which the loop below pumps; it is removed before the thread ends.
        let hook = unsafe {
            SetWinEventHook(
                EVENT_SYSTEM_FOREGROUND,
                EVENT_SYSTEM_FOREGROUND,
                std::ptr::null_mut(),
                Some(on_foreground),
                0,
                0,
                WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
            )
        };
        if !self.take_control() {
            self.scan();
            loop {
                match self.wait(wake) {
                    Woke::Control => {
                        if self.take_control() {
                            break;
                        }
                        self.scan();
                    }
                    Woke::Steam => {
                        if let Some(steam) = &self.steam {
                            steam.arm();
                        }
                        self.scan();
                    }
                    Woke::Exited | Woke::Timeout => self.scan(),
                    Woke::Messages => {}
                }
            }
        }
        if !hook.is_null() {
            // SAFETY: the hook set above.
            unsafe { UnhookWinEvent(hook) };
        }
    }

    /// Picks up new rules and catalogs; `true` to quit.
    fn take_control(&mut self) -> bool {
        let mut control = self.control.lock().unwrap_or_else(PoisonError::into_inner);
        if control.quit {
            return true;
        }
        if let Some(catalog) = control.catalog.take() {
            if self.catalog.is_some() || self.rules.enabled {
                self.catalog = Some(catalog);
            }
            // What was matched against the old one is matched again.
            self.reclassify = true;
        }
        if std::mem::take(&mut control.rules_changed) || control.rules != self.rules {
            let added_changed = control.rules.added != self.rules.added;
            self.rules = control.rules.clone();
            if added_changed {
                self.reclassify = true;
            }
        }
        if !self.rules.enabled {
            // Off: nothing kept, the catalog freed.
            self.tracked.clear();
            self.catalog = None;
        }
        control.stats = self.stats;
        false
    }

    fn timeout(&self) -> u32 {
        if !self.rules.enabled {
            return INFINITE;
        }
        let poll = if self.reported.is_some() {
            POLL_PLAYING
        } else {
            POLL_IDLE
        };
        let due = FOREGROUND.with(|fg| fg.borrow().2);
        let wait = match due {
            Some(at) => at.saturating_duration_since(Instant::now()).min(poll),
            None => poll,
        };
        u32::try_from(wait.as_millis()).unwrap_or(INFINITE - 1)
    }

    fn wait(&mut self, wake: HANDLE) -> Woke {
        let mut handles: Vec<HANDLE> = vec![wake];
        let steam_at = self
            .steam
            .as_ref()
            .filter(|_| self.rules.enabled)
            .map(|steam| {
                handles.push(steam.event());
                handles.len() - 1
            });
        if self.rules.enabled {
            handles.extend(
                self.tracked
                    .values()
                    .filter(|t| t.hit.is_some())
                    .filter_map(|t| t.wait.as_ref().map(|w| w.0))
                    .take(MAX_WAITS),
            );
        }
        // SAFETY: every handle is open until the next scan, after this call.
        let result = unsafe {
            MsgWaitForMultipleObjects(
                handles.len() as u32,
                handles.as_ptr(),
                0,
                self.timeout(),
                QS_ALLINPUT,
            )
        };
        let index = result.wrapping_sub(WAIT_OBJECT_0) as usize;
        if result == WAIT_TIMEOUT {
            // A foreground change's scan, or the poll.
            FOREGROUND.with(|fg| fg.borrow_mut().2 = None);
            Woke::Timeout
        } else if index == handles.len() {
            self.pump();
            Woke::Messages
        } else if index == 0 {
            Woke::Control
        } else if Some(index) == steam_at {
            Woke::Steam
        } else if index < handles.len() {
            Woke::Exited
        } else {
            // WAIT_FAILED: a handle went bad; scanning drops dead ones.
            std::thread::sleep(Duration::from_millis(500));
            Woke::Exited
        }
    }

    fn pump(&mut self) {
        // SAFETY: a standard message pump on this thread's queue.
        unsafe {
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    }

    fn processes(&self) -> Vec<(u32, String)> {
        match &self.config.fake {
            Some(fake) => fake.iter().map(|p| (p.pid, p.name.clone())).collect(),
            None => win::snapshot(),
        }
    }

    fn catalog(&mut self) -> Option<&Catalog> {
        if self.catalog.is_none() {
            let started = Instant::now();
            match crate::load_catalog(self.config.data_dir.as_deref()) {
                Ok(catalog) => {
                    eprintln!(
                        "[games] catalog {} loaded: {} apps, {} KiB, {} ms",
                        catalog.etag(),
                        catalog.len(),
                        catalog.heap_bytes() / 1024,
                        started.elapsed().as_millis()
                    );
                    self.stats.catalog_bytes = catalog.heap_bytes();
                    self.catalog = Some(catalog);
                }
                Err(err) => eprintln!("[games] no catalog: {err}"),
            }
        }
        self.catalog.as_ref()
    }

    fn open(&self, pid: u32) -> win::Opened {
        match &self.config.fake {
            Some(fake) => win::Opened {
                path: fake
                    .iter()
                    .find(|p| p.pid == pid)
                    .and_then(|p| p.path.clone()),
                wait: None,
            },
            None => win::open(pid),
        }
    }

    /// What a new process is, opening it only when its name could match.
    fn classify(&mut self, pid: u32, name: &str) -> (Option<Hit>, Option<Owned>) {
        if let Some((_, shown)) = self.rules.added.iter().find(|(exe, _)| exe == name) {
            let hit = Hit {
                key: format!("exe:{name}"),
                name: shown.clone(),
                app: None,
                source: Source::Added,
            };
            self.stats.opened += 1;
            let wait = self.open(pid).wait;
            if wait.as_ref().is_some_and(Owned::exited) {
                return (None, None);
            }
            return (Some(hit), wait);
        }
        let windir = self.windir.clone();
        let Some(catalog) = self.catalog() else {
            return (None, None);
        };
        if !matcher::may_match(catalog, name) {
            return (None, None);
        }
        let opened = self.open(pid);
        self.stats.opened += 1;
        // Gone already (still listed while a handle is open): not a game.
        if opened.wait.as_ref().is_some_and(Owned::exited) {
            return (None, None);
        }
        let catalog = self.catalog.as_ref().expect("loaded above");
        let app = match &opened.path {
            Some(path) => {
                let path = normalize(path);
                if crate::is_under(&path, &windir) {
                    None
                } else {
                    matcher::match_path(catalog, &path)
                }
            }
            None => matcher::match_name_only(catalog, name),
        };
        let hit = app.and_then(|index| {
            let app = catalog.app(index)?;
            Some(Hit {
                key: format!("app:{}", app.id),
                name: app.name,
                app: Some(index),
                source: Source::Catalog,
            })
        });
        let wait = if hit.is_some() { opened.wait } else { None };
        (hit, wait)
    }

    fn scan(&mut self) {
        if !self.rules.enabled {
            self.report(None);
            return;
        }
        let started = Instant::now();
        let now = crate::refresh::now_ms();
        note_foreground(win::foreground_pid());
        let list = self.processes();
        let mut alive = HashSet::with_capacity(list.len());
        let mut fresh = 0usize;
        let reclassify = std::mem::take(&mut self.reclassify);
        for (pid, name) in list {
            if pid == 0 || pid == 4 || pid == self.own_pid || crate::is_ignored(&name) {
                continue;
            }
            let name = name.to_lowercase();
            alive.insert(pid);
            let mut first_seen = now;
            if let Some(tracked) = self.tracked.get_mut(&pid).filter(|t| t.name == name) {
                // Exited but still listed: no longer a game, and its handle
                // goes so it stops waking the wait.
                if tracked.wait.as_ref().is_some_and(Owned::exited) {
                    tracked.wait = None;
                    tracked.hit = None;
                    continue;
                }
                if !reclassify {
                    continue;
                }
                // Matched again, but it started when it started.
                first_seen = tracked.first_seen;
                tracked.wait = None;
            } else {
                fresh += 1;
            }
            let (hit, wait) = self.classify(pid, &name);
            self.tracked.insert(
                pid,
                Tracked {
                    name,
                    hit,
                    wait,
                    first_seen,
                },
            );
        }
        self.tracked.retain(|pid, _| alive.contains(pid));
        FOREGROUND.with(|fg| fg.borrow_mut().1.retain(|pid, _| alive.contains(pid)));
        let detected = self.pick(now);
        self.stats.scans += 1;
        self.stats.cpu = thread_cpu();
        self.stats.cycles = thread_cycles();
        self.stats.busy += started.elapsed();
        if self.config.trace {
            eprintln!(
                "[games] scan {}: {} processes ({} new), {} opened so far, {} us",
                self.stats.scans,
                alive.len(),
                fresh,
                self.stats.opened,
                started.elapsed().as_micros()
            );
        }
        self.control
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stats = self.stats;
        self.report(detected);
    }

    /// The game to report: the running one most recently in the foreground
    /// (then the newest), else what Steam runs.
    fn pick(&mut self, now: i64) -> Option<Detected> {
        let hidden = |key: &str| self.rules.hidden.iter().any(|k| k == key);
        let best = self
            .tracked
            .iter()
            .filter_map(|(pid, t)| t.hit.as_ref().map(|hit| (*pid, t, hit)))
            .filter(|(_, _, hit)| !hidden(&hit.key))
            .max_by_key(|(pid, t, _)| {
                (foreground_seq(*pid), t.first_seen, std::cmp::Reverse(*pid))
            });
        let steam_running = self.steam.as_ref().map_or(0, SteamWatch::running_app);
        if steam_running != self.steam_app.0 {
            self.steam_app = (steam_running, now);
        }
        let catalog = self.catalog.as_ref();
        let steam_id_for = |app: Option<u32>| -> Option<String> {
            let ids = catalog?.app(app?)?.steam_app_ids;
            let running = steam_running.to_string();
            if steam_running != 0 && ids.contains(&running) {
                Some(running)
            } else if ids.len() == 1 {
                ids.into_iter().next()
            } else {
                None
            }
        };
        if let Some((_, tracked, hit)) = best {
            let app = hit.app.and_then(|index| catalog?.app(index));
            return Some(Detected {
                key: hit.key.clone(),
                name: hit.name.clone(),
                app_id: app.as_ref().map(|a| a.id.clone()),
                icon_hash: app.and_then(|a| a.icon_hash),
                steam_app_id: steam_id_for(hit.app),
                started_at: tracked.first_seen,
                source: hit.source,
            });
        }
        if steam_running == 0 {
            return None;
        }
        let since = self.steam_app.1;
        let in_catalog = self
            .catalog
            .as_ref()
            .and_then(|c| c.app_for_steam(steam_running).and_then(|i| c.app(i)));
        let detected = match in_catalog {
            Some(app) => Detected {
                key: format!("app:{}", app.id),
                name: app.name,
                app_id: Some(app.id),
                icon_hash: app.icon_hash,
                steam_app_id: Some(steam_running.to_string()),
                started_at: since,
                source: Source::Catalog,
            },
            None => Detected {
                key: format!("steam:{steam_running}"),
                name: self.steam.as_ref()?.manifest_name(steam_running)?,
                app_id: None,
                icon_hash: None,
                steam_app_id: Some(steam_running.to_string()),
                started_at: since,
                source: Source::Catalog,
            },
        };
        (!hidden(&detected.key)).then_some(detected)
    }

    fn report(&mut self, detected: Option<Detected>) {
        if detected == self.reported {
            return;
        }
        if self.config.trace {
            match &detected {
                Some(d) => eprintln!("[games] playing {} ({})", d.key, d.name),
                None => eprintln!("[games] not playing"),
            }
        } else {
            eprintln!(
                "[games] {}",
                if detected.is_some() {
                    "game running"
                } else {
                    "no game running"
                }
            );
        }
        self.reported = detected.clone();
        (self.on_change)(detected);
    }
}

enum Woke {
    Control,
    Steam,
    Exited,
    Timeout,
    Messages,
}

/// This thread's CPU cycles so far.
fn thread_cycles() -> u64 {
    let mut cycles = 0u64;
    // SAFETY: the pseudo handle needs no closing; `cycles` outlives the call.
    unsafe {
        windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime(
            GetCurrentThread(),
            &mut cycles,
        )
    };
    cycles
}

/// This thread's CPU time so far.
fn thread_cpu() -> Duration {
    use windows_sys::Win32::Foundation::FILETIME;
    let mut times = [FILETIME {
        dwLowDateTime: 0,
        dwHighDateTime: 0,
    }; 4];
    let [created, exited, kernel, user] = &mut times;
    // SAFETY: the pseudo handle needs no closing; the out params are valid.
    let ok = unsafe { GetThreadTimes(GetCurrentThread(), created, exited, kernel, user) } != 0;
    if !ok {
        return Duration::ZERO;
    }
    let ticks = |t: &FILETIME| (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime);
    Duration::from_nanos((ticks(&times[2]) + ticks(&times[3])) * 100)
}

/// Programs with a window, for "Add it": the executable and its window's
/// title, ours and the shell's left out. With the test switch, the fake
/// processes (titled by their name) instead, so no real window shows.
pub fn running_programs(fake: Option<&[FakeProcess]>) -> Vec<RunningProgram> {
    if let Some(fake) = fake {
        return fake
            .iter()
            .map(|p| RunningProgram {
                exe: p.name.clone(),
                title: p
                    .name
                    .rsplit_once('.')
                    .map_or(p.name.as_str(), |(stem, _)| stem)
                    .to_owned(),
            })
            .collect();
    }
    const SHELL: &[&str] = &[
        "explorer.exe",
        "applicationframehost.exe",
        "textinputhost.exe",
        "systemsettings.exe",
        "shellexperiencehost.exe",
        "searchhost.exe",
        "startmenuexperiencehost.exe",
    ];
    let names: HashMap<u32, String> = win::snapshot().into_iter().collect();
    let own = win::own_pid();
    let mut seen = HashSet::new();
    win::app_windows()
        .into_iter()
        .filter(|(pid, _)| *pid != own)
        .filter_map(|(pid, title)| {
            let exe = names.get(&pid)?.clone();
            let lower = exe.to_lowercase();
            if SHELL.contains(&lower.as_str()) || crate::is_ignored(&lower) || !seen.insert(lower) {
                return None;
            }
            Some(RunningProgram { exe, title })
        })
        .collect()
}
