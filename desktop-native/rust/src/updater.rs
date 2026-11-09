//! `Updater`: the in-app updater (docs/INSTALLER.md, "Updates"). Velopack's
//! `UpdateManager` reads the channel's feed files from a static folder (the
//! compiled `yap_core::update::DEFAULT_FEED_URL`, `updates.json`'s
//! `feedUrl`, or `YAP_UPDATE_URL`), downloads a newer package in the
//! background and leaves it in `<root>\packages`. "Restart now" (never
//! during a call) or the next quit (install on quit) hands it to
//! `Update.exe`, which waits for this process to exit, swaps `current\` and
//! starts the new version.
//!
//! The rules (which copies update, the schedule and its backoff, channels,
//! the pin, the restart prompt) are `yap_core::update`; this module runs
//! them on a thread of its own and shows them to QML. It starts with the
//! core whether or not QML made the object, never blocks the start, and a
//! copy that does not update (a dev build, a portable or named-profile
//! copy, anything under `F:\ct\yapn`) only says why.

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
        /// The installed version (Velopack's), else this build's.
        #[qproperty(QString, version)]
        /// `off`, `idle`, `checking`, `upToDate`, `downloading`, `ready`,
        /// `error` or `applying` (`yap_core::update::View`).
        #[qproperty(QString, phase)]
        /// Download progress, 0-100.
        #[qproperty(i32, percent)]
        /// The version downloading or ready.
        #[qproperty(QString, target)]
        /// The ready update's release notes (plain text); may be empty.
        #[qproperty(QString, notes)]
        /// This version's own release notes (plain text); may be empty.
        #[qproperty(QString, current_notes, cxx_name = "currentNotes")]
        /// Why updates are off, or what failed (one sentence).
        #[qproperty(QString, message)]
        /// The shell's "Restart to update": ready, and no call is up.
        #[qproperty(bool, prompt)]
        #[qproperty(bool, can_restart, cxx_name = "canRestart")]
        #[qproperty(bool, can_check, cxx_name = "canCheck")]
        /// A voice call is up (the restart waits for it).
        #[qproperty(bool, in_call, cxx_name = "inCall")]
        /// The beta channel (stable releases too).
        #[qproperty(bool, beta)]
        #[qproperty(bool, install_on_quit, cxx_name = "installOnQuit")]
        /// The pinned version from `updates.json`, or empty.
        #[qproperty(QString, pinned)]
        type Updater = super::UpdaterRust;

        /// "Check for updates" (and "Try again").
        #[qinvokable]
        fn check(self: &Updater);

        /// Switches the beta channel and saves it.
        #[qinvokable]
        #[cxx_name = "saveBeta"]
        fn save_beta(self: Pin<&mut Updater>, on: bool);

        /// Switches install on quit and saves it.
        #[qinvokable]
        #[cxx_name = "saveInstallOnQuit"]
        fn save_install_on_quit(self: Pin<&mut Updater>, on: bool);

        /// Starts Velopack's updater for the ready update, waiting for this
        /// process to exit; the caller then quits (`DesktopShell.quit()`).
        /// False when nothing is ready, a call is up, or it could not start.
        #[qinvokable]
        #[cxx_name = "restartNow"]
        fn restart_now(self: Pin<&mut Updater>) -> bool;
    }

    impl cxx_qt::Threading for Updater {}
    impl cxx_qt::Initialize for Updater {}
}

use core::pin::Pin;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Mutex, OnceLock, PoisonError};
use std::time::Instant;

use cxx_qt::{CxxQtThread, Threading};
use cxx_qt_lib::QString;
use yap_core::update::{self, Off, Prefs, Timing};

#[derive(Default)]
pub struct UpdaterRust {
    version: QString,
    phase: QString,
    percent: i32,
    target: QString,
    notes: QString,
    current_notes: QString,
    message: QString,
    prompt: bool,
    can_restart: bool,
    can_check: bool,
    in_call: bool,
    beta: bool,
    install_on_quit: bool,
    pinned: QString,
}

/// What the update thread hears.
enum Msg {
    /// "Check for updates".
    Check,
    /// The channel changed: what was found may not apply.
    Reset,
}

struct Shared {
    started: Instant,
    state: update::Updater,
    prefs: Prefs,
    /// The profile's data folder, where `updates.json` is.
    dir: Option<PathBuf>,
    version: String,
    current_notes: String,
    tx: Option<mpsc::Sender<Msg>>,
    qt: Option<CxxQtThread<qobject::Updater>>,
    /// The manager that downloaded the ready update, and its package.
    #[cfg(windows)]
    ready: Option<(velopack::UpdateManager, velopack::VelopackAsset)>,
}

fn shared() -> &'static Mutex<Shared> {
    static SHARED: OnceLock<Mutex<Shared>> = OnceLock::new();
    SHARED.get_or_init(|| {
        let dir = yap_cache::settings::app_dir();
        let prefs = dir.as_deref().map(update::load_prefs).unwrap_or_default();
        let timing = Timing::new(
            std::env::var(update::ENV_INTERVAL).ok().as_deref(),
            std::env::var(update::ENV_FIRST).ok().as_deref(),
        );
        let started = Instant::now();
        let off = eligibility().err();
        Mutex::new(Shared {
            started,
            state: update::Updater::new(timing, started.elapsed(), off),
            prefs,
            dir,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            current_notes: String::new(),
            tx: None,
            qt: None,
            #[cfg(windows)]
            ready: None,
        })
    })
}

fn with<T>(f: impl FnOnce(&mut Shared) -> T) -> T {
    f(&mut shared().lock().unwrap_or_else(PoisonError::into_inner))
}

/// The install root when this launch updates itself (`update::eligibility`).
fn eligibility() -> Result<String, Off> {
    let exe = std::env::current_exe().map_err(|_| Off::NotInstalled)?;
    let exe = exe.to_str().ok_or(Off::NotInstalled)?.to_owned();
    let launch = update::Launch {
        exe: &exe,
        profile: yap_cache::profile::current(),
        test: std::env::var(update::ENV_TEST).is_ok_and(|v| v == "1"),
    };
    update::eligibility(&launch, |p| std::path::Path::new(p).is_file())
}

/// Whether Velopack may install a downloaded update as this process starts
/// (`installer.rs`, before Qt and before the data folder is resolved).
pub(crate) fn apply_at_start() -> bool {
    let eligible = eligibility().is_ok();
    let prefs = std::env::var_os("LOCALAPPDATA")
        .map(|base| {
            yap_cache::profile::data_root_in(
                std::path::Path::new(&base),
                yap_cache::profile::current(),
            )
        })
        .map(|dir| update::load_prefs(&dir))
        .unwrap_or_default();
    update::apply_at_start(eligible, &prefs)
}

fn log(line: &str) {
    eprintln!("[updater] {line}");
}

/// Puts the shared state on the object.
fn refresh(mut object: Pin<&mut qobject::Updater>) {
    let (view, prefs, version, current_notes) = with(|s| {
        (
            s.state.view(),
            s.prefs.clone(),
            s.version.clone(),
            s.current_notes.clone(),
        )
    });
    object.as_mut().set_version(QString::from(&version));
    object.as_mut().set_phase(QString::from(view.phase));
    object.as_mut().set_percent(i32::from(view.percent));
    object.as_mut().set_target(QString::from(&view.version));
    object.as_mut().set_notes(QString::from(&view.notes));
    object
        .as_mut()
        .set_current_notes(QString::from(&current_notes));
    object.as_mut().set_message(QString::from(&view.message));
    object.as_mut().set_prompt(view.prompt);
    object.as_mut().set_can_restart(view.can_restart);
    object.as_mut().set_can_check(view.can_check);
    object.as_mut().set_in_call(view.in_call);
    object
        .as_mut()
        .set_beta(prefs.channel == update::Channel::Beta);
    object.as_mut().set_install_on_quit(prefs.install_on_quit);
    object.set_pinned(QString::from(
        prefs.pinned_version.as_deref().unwrap_or_default(),
    ));
}

/// Shows the shared state, from any thread.
fn publish() {
    if let Some(qt) = with(|s| s.qt.clone()) {
        let _ = qt.queue(refresh);
    }
}

fn send(msg: Msg) {
    if let Some(tx) = with(|s| s.tx.clone()) {
        let _ = tx.send(msg);
    }
}

fn save_prefs(s: &Shared) {
    if let Some(dir) = &s.dir {
        if let Err(error) = update::save_prefs(dir, &s.prefs) {
            log(&format!("{} not saved: {error}", update::PREFS_FILE));
        }
    }
}

/// Starts the update thread, once (with the core). A copy that does not
/// update starts nothing.
pub fn start() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| match eligibility() {
        Err(off) => log(&format!("off: {off:?}")),
        Ok(root) => {
            #[cfg(windows)]
            {
                let (tx, rx) = mpsc::channel();
                with(|s| s.tx = Some(tx));
                let spawned = std::thread::Builder::new()
                    .name("yap-updater".into())
                    .spawn(move || engine::run(&root, &rx));
                if let Err(error) = spawned {
                    log(&format!("no thread: {error}"));
                }
            }
            #[cfg(not(windows))]
            let _ = root;
        }
    });
}

/// A voice call started or ended (the voice view's channel).
pub(crate) fn set_in_call(in_call: bool) {
    let changed = with(|s| {
        let changed = s.state.in_call() != in_call;
        s.state.set_in_call(in_call);
        changed
    });
    if changed {
        publish();
    }
}

/// The quit (`main.cpp`, after the event loop): with install on quit, the
/// ready update goes to Velopack's updater, which waits for this process to
/// exit. Never while Windows is shutting down: the apply could be cut off.
#[no_mangle]
pub extern "C" fn yap_updater_quit(session_ending: bool) {
    let run = || {
        #[cfg(windows)]
        engine::apply_on_quit(session_ending);
        #[cfg(not(windows))]
        let _ = session_ending;
    };
    if std::panic::catch_unwind(run).is_err() {
        log("quit: panicked");
    }
}

impl cxx_qt::Initialize for qobject::Updater {
    fn initialize(self: Pin<&mut Self>) {
        let qt = self.qt_thread();
        with(|s| s.qt = Some(qt));
        refresh(self);
    }
}

impl Drop for UpdaterRust {
    fn drop(&mut self) {
        with(|s| s.qt = None);
    }
}

impl qobject::Updater {
    pub fn check(&self) {
        let due = with(|s| {
            let now = s.started.elapsed();
            s.state.request(now)
        });
        if due {
            send(Msg::Check);
        }
    }

    pub fn save_beta(self: Pin<&mut Self>, on: bool) {
        let channel = if on {
            update::Channel::Beta
        } else {
            update::Channel::Stable
        };
        let changed = with(|s| {
            if s.prefs.channel == channel {
                return false;
            }
            s.prefs.channel = channel;
            save_prefs(s);
            true
        });
        if changed {
            log(&format!("channel {}", channel.velopack_name()));
            send(Msg::Reset);
        }
        refresh(self);
    }

    pub fn save_install_on_quit(self: Pin<&mut Self>, on: bool) {
        with(|s| {
            s.prefs.install_on_quit = on;
            save_prefs(s);
        });
        refresh(self);
    }

    pub fn restart_now(self: Pin<&mut Self>) -> bool {
        #[cfg(windows)]
        let started = engine::apply_and_restart();
        #[cfg(not(windows))]
        let started = false;
        refresh(self);
        started
    }
}

/// The Velopack side, on the update thread.
#[cfg(windows)]
mod engine {
    use std::path::Path;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::time::Duration;

    use velopack::sources::{FileSource, HttpSource, UpdateSource};
    use velopack::{
        HttpOptions, UpdateCheck, UpdateManager, UpdateOptions, VelopackAsset, VelopackAssetFeed,
    };
    use yap_core::update::{self, Feed, FeedEntry};

    use super::{log, publish, with, Msg};

    /// A feed request (the JSON) gives up after this.
    const FEED_TIMEOUT: Duration = Duration::from_secs(30);
    /// A package download gives up after this (Velopack's timeout covers
    /// the whole transfer).
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(30 * 60);
    /// The longest sleep between looks at the schedule.
    const IDLE_WAIT: Duration = Duration::from_secs(60 * 60);

    /// A feed entry, for `yap_core::update::merge_feeds`.
    #[derive(Clone)]
    struct Entry(VelopackAsset);

    impl FeedEntry for Entry {
        fn version(&self) -> &str {
            &self.0.Version
        }
        fn kind(&self) -> &str {
            &self.0.Type
        }
        fn file_name(&self) -> &str {
            &self.0.FileName
        }
    }

    /// The channel's feeds merged (beta reads stable too) and cut at the
    /// pin; downloads go to the same folder with a longer timeout.
    struct ChannelSource {
        feeds: &'static [&'static str],
        pin: Option<update::Version>,
        list: Box<dyn UpdateSource>,
        fetch: Box<dyn UpdateSource>,
    }

    impl UpdateSource for ChannelSource {
        fn get_release_feed(
            &self,
            _channel: &str,
            app: &velopack::bundle::Manifest,
            staged_user_id: &str,
        ) -> Result<VelopackAssetFeed, velopack::Error> {
            let mut feeds = Vec::new();
            let mut error = None;
            for channel in self.feeds {
                match self.list.get_release_feed(channel, app, staged_user_id) {
                    Ok(feed) => feeds.push(feed.Assets.into_iter().map(Entry).collect::<Vec<_>>()),
                    Err(e) => {
                        log(&format!("feed {channel}: {e}"));
                        error = Some(e);
                    }
                }
            }
            if feeds.is_empty() {
                return Err(error.unwrap_or_else(|| velopack::Error::Other("no feed".into())));
            }
            let assets = update::merge_feeds(&feeds, self.pin.as_ref());
            Ok(VelopackAssetFeed {
                Assets: assets.into_iter().map(|e| e.0).collect(),
            })
        }

        fn download_release_entry(
            &self,
            asset: &VelopackAsset,
            local_file: &Path,
            progress_sender: Option<mpsc::Sender<i16>>,
        ) -> Result<(), velopack::Error> {
            self.fetch
                .download_release_entry(asset, local_file, progress_sender)
        }
    }

    fn http(url: &str, timeout: Duration) -> Box<dyn UpdateSource> {
        let options = HttpOptions {
            Headers: vec![velopack::HttpHeader {
                Name: "User-Agent".into(),
                Value: concat!("YapNative/", env!("CARGO_PKG_VERSION")).into(),
            }],
            TimeoutMilliseconds: u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX),
        };
        Box::new(HttpSource::new_with_options(url, options))
    }

    /// A manager for the current preferences.
    fn manager() -> Result<UpdateManager, velopack::Error> {
        let prefs = with(|s| s.prefs.clone());
        let env = std::env::var(update::ENV_FEED).ok();
        let (feed, origin) = update::feed(env.as_deref(), prefs.feed_url.as_deref());
        let (list, fetch): (Box<dyn UpdateSource>, Box<dyn UpdateSource>) = match &feed {
            Feed::Http(url) => (http(url, FEED_TIMEOUT), http(url, DOWNLOAD_TIMEOUT)),
            Feed::Dir(dir) => (
                Box::new(FileSource::new(dir)),
                Box::new(FileSource::new(dir)),
            ),
        };
        log(&format!(
            "feed {feed} ({origin:?}), channel {}, pin {}",
            prefs.channel.velopack_name(),
            prefs.pinned_version.as_deref().unwrap_or("none")
        ));
        let source = ChannelSource {
            feeds: prefs.channel.feeds(),
            pin: prefs.pin(),
            list,
            fetch,
        };
        let options = UpdateOptions {
            AllowVersionDowngrade: false,
            ExplicitChannel: Some(prefs.channel.velopack_name().to_owned()),
            MaximumDeltasBeforeFallback: 0,
        };
        // Located from this executable (`<root>\current\`), as the hooks.
        UpdateManager::new(source, Some(options), None)
    }

    /// The update thread: wait for the schedule or a request, check, repeat.
    pub(super) fn run(root: &str, rx: &Receiver<Msg>) {
        log(&format!("on, install root {root}"));
        // The installed version and its notes, once.
        match manager() {
            Ok(um) => {
                let version = um.get_current_version_as_string();
                let notes = update::plain_notes(
                    &um.get_current_release_notes_markdown().unwrap_or_default(),
                );
                with(|s| {
                    s.version = version;
                    s.current_notes = notes;
                });
                publish();
            }
            Err(error) => log(&format!("no manager: {error}")),
        }
        loop {
            let wait = with(|s| s.state.until_due(s.started.elapsed()))
                .unwrap_or(IDLE_WAIT)
                .min(IDLE_WAIT);
            match rx.recv_timeout(wait) {
                Ok(Msg::Check) | Err(RecvTimeoutError::Timeout) => {}
                Ok(Msg::Reset) => {
                    with(|s| {
                        let now = s.started.elapsed();
                        s.state.reset(now);
                        s.ready = None;
                    });
                    publish();
                }
                Err(RecvTimeoutError::Disconnected) => return,
            }
            if !with(|s| s.state.due(s.started.elapsed())) {
                continue;
            }
            with(|s| s.state.begin());
            publish();
            let outcome = std::panic::catch_unwind(cycle);
            if outcome.is_err() {
                fail("the update check panicked");
            }
            publish();
        }
    }

    /// Logs Velopack's error as it is; Settings gets a plain sentence.
    fn fail(message: &str) {
        log(&format!("failed: {}", update::short_error(message)));
        let shown = update::friendly_error(message);
        with(|s| {
            let now = s.started.elapsed();
            s.state.failed(shown, now);
        });
    }

    /// One check, and the download when the feed has something newer.
    fn cycle() {
        let um = match manager() {
            Ok(um) => um,
            Err(error) => return fail(&error.to_string()),
        };
        let check = um.check_for_updates();
        let info = match check {
            Ok(UpdateCheck::UpdateAvailable(info)) => info,
            Ok(UpdateCheck::NoUpdateAvailable | UpdateCheck::RemoteIsEmpty) => {
                log(&format!(
                    "up to date ({})",
                    um.get_current_version_as_string()
                ));
                with(|s| {
                    let now = s.started.elapsed();
                    s.state.up_to_date(now);
                });
                return;
            }
            Err(error) => {
                // Offline: an update downloaded by an earlier run still
                // installs.
                if !pending(&um) {
                    fail(&error.to_string());
                }
                return;
            }
        };
        let asset = info.TargetFullRelease.clone();
        let version = asset.Version.clone();
        log(&format!(
            "found {version} ({} delta(s)), downloading",
            info.DeltasToTarget.len()
        ));
        with(|s| s.state.downloading(&version, 0));
        publish();

        let (tx, progress) = mpsc::channel::<i16>();
        let forward_version = version.clone();
        let forwarder = std::thread::Builder::new()
            .name("yap-updater-progress".into())
            .spawn(move || {
                for percent in progress {
                    with(|s| s.state.downloading(&forward_version, percent));
                    publish();
                }
            });
        if forwarder.is_err() {
            log("no progress thread");
        }
        let result = um.download_updates(&info, Some(tx));
        match result {
            Ok(()) => {
                log(&format!("{version} downloaded"));
                let notes = update::plain_notes(&asset.NotesMarkdown);
                with(|s| {
                    let now = s.started.elapsed();
                    s.state.downloaded(&version, &notes, now);
                    s.ready = Some((um.clone(), asset));
                });
            }
            Err(error) => fail(&error.to_string()),
        }
    }

    /// A package an earlier run downloaded, newer than this version and
    /// allowed by the pin: ready.
    fn pending(um: &UpdateManager) -> bool {
        let Some(asset) = um.get_update_pending_restart() else {
            return false;
        };
        let pin = with(|s| s.prefs.pin());
        let allowed = update::parse_version(&asset.Version)
            .is_some_and(|v| update::pin_allows(&v, pin.as_ref()));
        if !allowed {
            return false;
        }
        log(&format!("{} was downloaded before", asset.Version));
        let (version, notes) = (
            asset.Version.clone(),
            update::plain_notes(&asset.NotesMarkdown),
        );
        with(|s| {
            let now = s.started.elapsed();
            s.state.downloaded(&version, &notes, now);
            s.ready = Some((um.clone(), asset));
        });
        true
    }

    /// "Restart now": Velopack's updater waits for this process, applies
    /// the package and starts the new version (with this profile).
    pub(super) fn apply_and_restart() -> bool {
        let ready = with(|s| {
            if s.state.can_restart() {
                s.ready.clone()
            } else {
                None
            }
        });
        let Some((um, asset)) = ready else {
            log("restart: nothing ready, or a call is up");
            return false;
        };
        let args = update::restart_args(yap_cache::profile::current());
        match um.wait_exit_then_apply_updates(&asset, false, true, &args) {
            Ok(()) => {
                log(&format!("restart: applying {} after exit", asset.Version));
                with(|s| s.state.applying());
                true
            }
            Err(error) => {
                fail(&format!("could not start Update.exe: {error}"));
                publish();
                false
            }
        }
    }

    /// The quit: with install on quit, the same apply, silent and without a
    /// restart.
    pub(super) fn apply_on_quit(session_ending: bool) {
        let ready = with(|s| {
            if s.state.apply_on_quit(s.prefs.install_on_quit) {
                s.ready.clone()
            } else {
                None
            }
        });
        let Some((um, asset)) = ready else {
            return;
        };
        if session_ending {
            log("quit: Windows is shutting down; the update waits for the next start");
            return;
        }
        let none: [&str; 0] = [];
        match um.wait_exit_then_apply_updates(&asset, true, false, none) {
            Ok(()) => {
                log(&format!("quit: applying {} after exit", asset.Version));
                with(|s| s.state.applying());
            }
            Err(error) => log(&format!("quit: could not start the update: {error}")),
        }
    }
}
