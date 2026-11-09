//! The in-app updater's rules (desktop-native/docs/INSTALLER.md, "Updates"),
//! without Velopack or Qt: where the feed is, which channel files it reads,
//! whether this copy updates itself at all, when it checks, which releases a
//! pinned version allows, and what the UI shows. The restart prompt waits
//! while a call is up. The app crate (`src/updater.rs`) drives [`Updater`]
//! with Velopack's `UpdateManager` on a thread of its own.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// The release version type (semver), for the app crate.
pub use semver::Version;
use serde::{Deserialize, Serialize};

use crate::install;

/// Where the released app looks for updates when nothing overrides it: a
/// static folder with Velopack's feed files (`releases.<channel>.json`, the
/// `.nupkg` packages). A placeholder: no feed is published there yet. See
/// INSTALLER.md, "Publishing a release".
pub const DEFAULT_FEED_URL: &str = "https://updates.115jon.com/yap/win";

/// Test hook: a feed folder (`file:///F:/feed`, `F:\feed`) or URL that
/// replaces the saved and compiled ones.
pub const ENV_FEED: &str = "YAP_UPDATE_URL";

/// Test hook: seconds between checks (instead of 6 h). Retries and the
/// first check never wait longer than this.
pub const ENV_INTERVAL: &str = "YAP_UPDATE_INTERVAL";

/// Test hook: seconds before the first check (instead of 20 s).
pub const ENV_FIRST: &str = "YAP_UPDATE_DELAY";

/// Test hook: `1` lets a Velopack layout under [`DEV_FOLDER`] or on a named
/// profile update (an end-to-end run in a scratch root). It never makes a
/// dev build, a copy or a portable install update.
pub const ENV_TEST: &str = "YAP_UPDATE_TEST";

/// The preferences file in the profile's data folder.
pub const PREFS_FILE: &str = "updates.json";

/// The agents' build and test tree on the maintainer's PC: nothing started
/// from there updates itself (it would replace a build under test).
pub const DEV_FOLDER: &str = r"F:\ct\yapn";

/// How long after the start the first check waits.
pub const FIRST_CHECK: Duration = Duration::from_secs(20);

/// How often a running app checks again.
pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

/// The first retry after a failed check or download; each further failure
/// doubles it, up to [`RETRY_MAX`].
pub const RETRY_FIRST: Duration = Duration::from_secs(60);

/// The longest wait between retries.
pub const RETRY_MAX: Duration = Duration::from_secs(60 * 60);

/// The update channels. Velopack names a feed file after its channel:
/// stable is `releases.win.json` (vpk's default channel on Windows), beta
/// `releases.beta.json` (`vpk pack --channel beta`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// The Velopack channel this one installs from.
    pub fn velopack_name(self) -> &'static str {
        match self {
            Self::Stable => "win",
            Self::Beta => "beta",
        }
    }

    /// The Velopack channels whose feeds are read, merged: beta takes the
    /// stable releases too, so a beta user is never stuck on an old beta
    /// once a newer stable ships.
    pub fn feeds(self) -> &'static [&'static str] {
        match self {
            Self::Stable => &["win"],
            Self::Beta => &["beta", "win"],
        }
    }
}

/// `updates.json`: what the user chose, and the overrides for testing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
    pub channel: Channel,
    /// A downloaded update installs when Yap quits (and on the next start
    /// if the quit could not install it). Off, only "Restart now" installs.
    pub install_on_quit: bool,
    /// Never update past this version (`"0.2.22"`). Set by hand, for a
    /// release that must stay put; the app never writes it.
    pub pinned_version: Option<String>,
    /// A feed that replaces the compiled one (a test feed); the environment
    /// ([`ENV_FEED`]) still wins.
    pub feed_url: Option<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            channel: Channel::Stable,
            install_on_quit: true,
            pinned_version: None,
            feed_url: None,
        }
    }
}

impl Prefs {
    /// The pinned version, when it parses.
    pub fn pin(&self) -> Option<Version> {
        self.pinned_version.as_deref().and_then(parse_version)
    }
}

/// The preferences in `dir`, or the defaults when the file is missing or
/// does not parse.
pub fn load_prefs(dir: &Path) -> Prefs {
    std::fs::read_to_string(dir.join(PREFS_FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Writes the preferences through a temporary file and a rename.
pub fn save_prefs(dir: &Path, prefs: &Prefs) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(PREFS_FILE);
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_vec_pretty(prefs).map_err(std::io::Error::other)?;
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &path)
}

// The feed

/// Where the feed files are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Feed {
    /// A web folder, without the trailing slash.
    Http(String),
    /// A folder on disk (a test feed).
    Dir(PathBuf),
}

impl std::fmt::Display for Feed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Http(url) => f.write_str(url),
            Self::Dir(dir) => write!(f, "{}", dir.display()),
        }
    }
}

/// Which setting chose the feed (for the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedOrigin {
    Env,
    Prefs,
    Default,
}

/// A feed location from text: an `http(s)://` folder, a `file://` URL or
/// an absolute Windows path. `None` for anything else.
pub fn parse_feed(text: &str) -> Option<Feed> {
    let text = text.trim();
    let lower = text.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") {
        let url = text.trim_end_matches('/');
        let host = url.split_once("://")?.1;
        return (!host.is_empty()).then(|| Feed::Http(url.to_owned()));
    }
    if lower.starts_with("file:") {
        let rest = &text[5..];
        let path = match rest.strip_prefix("///") {
            Some(local) => percent_decode(local),
            // file://server/share: a UNC path.
            None => format!(r"\\{}", percent_decode(rest.strip_prefix("//")?)),
        };
        return absolute_path(&path).map(Feed::Dir);
    }
    absolute_path(text).map(Feed::Dir)
}

/// `X:\...`, `X:/...` or `\\server\share`, with backslashes.
fn absolute_path(text: &str) -> Option<PathBuf> {
    let path = text.replace('/', "\\");
    let path = path.trim_end_matches('\\');
    let bytes = path.as_bytes();
    let drive =
        bytes.len() >= 3 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && bytes[2] == b'\\';
    let unc = path.starts_with(r"\\") && path.len() > 2;
    (drive || unc).then(|| PathBuf::from(path))
}

/// `%XX` escapes in a `file://` URL's path; anything malformed stays as is.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(byte) = text
                .get(i + 1..i + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
            {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The feed to use: the environment's, else the saved one, else
/// [`DEFAULT_FEED_URL`]. One that does not parse is skipped.
pub fn feed(env: Option<&str>, prefs: Option<&str>) -> (Feed, FeedOrigin) {
    if let Some(feed) = env.and_then(parse_feed) {
        return (feed, FeedOrigin::Env);
    }
    if let Some(feed) = prefs.and_then(parse_feed) {
        return (feed, FeedOrigin::Prefs);
    }
    (Feed::Http(DEFAULT_FEED_URL.to_owned()), FeedOrigin::Default)
}

// Versions and releases

/// A release version: semver, a leading `v` allowed.
pub fn parse_version(text: &str) -> Option<Version> {
    let text = text.trim();
    let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
    Version::parse(text).ok()
}

/// Whether `candidate` is a newer release than `current` (semver order: a
/// pre-release sorts before its release). False if either does not parse.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Whether a pin allows `version`: no pin, or not past it.
pub fn pin_allows(version: &Version, pin: Option<&Version>) -> bool {
    pin.is_none_or(|pin| version <= pin)
}

/// What the rules need of a feed entry; Velopack's `VelopackAsset` and
/// [`FeedAsset`] both have it.
pub trait FeedEntry {
    fn version(&self) -> &str;
    /// `Full` or `Delta`.
    fn kind(&self) -> &str;
    fn file_name(&self) -> &str;
}

/// One entry of a `releases.<channel>.json` as vpk writes it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct FeedAsset {
    #[serde(rename = "PackageId", alias = "packageId")]
    pub package_id: String,
    #[serde(rename = "Version", alias = "version")]
    pub version: String,
    #[serde(rename = "Type", alias = "type")]
    pub kind: String,
    #[serde(rename = "FileName", alias = "fileName")]
    pub file_name: String,
    #[serde(rename = "SHA1", alias = "sha1")]
    pub sha1: String,
    #[serde(rename = "SHA256", alias = "sha256")]
    pub sha256: String,
    #[serde(rename = "Size", alias = "size")]
    pub size: u64,
    #[serde(rename = "NotesMarkdown", alias = "notesMarkdown")]
    pub notes_markdown: String,
}

impl FeedEntry for FeedAsset {
    fn version(&self) -> &str {
        &self.version
    }
    fn kind(&self) -> &str {
        &self.kind
    }
    fn file_name(&self) -> &str {
        &self.file_name
    }
}

/// A whole `releases.<channel>.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct FeedFile {
    #[serde(rename = "Assets", alias = "assets")]
    pub assets: Vec<FeedAsset>,
}

pub fn parse_feed_file(json: &str) -> Result<FeedFile, serde_json::Error> {
    serde_json::from_str(json)
}

/// The channels' feeds as one list: every entry once (by file name), only
/// versions that parse and the pin allows, in feed order.
pub fn merge_feeds<T: FeedEntry + Clone>(feeds: &[Vec<T>], pin: Option<&Version>) -> Vec<T> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for asset in feeds.iter().flatten() {
        let Some(version) = parse_version(asset.version()) else {
            continue;
        };
        if !pin_allows(&version, pin) {
            continue;
        }
        let name = asset.file_name().to_ascii_lowercase();
        if seen.contains(&name) {
            continue;
        }
        seen.push(name);
        out.push(asset.clone());
    }
    out
}

/// The newest full release in `assets` and its version.
pub fn newest_full<T: FeedEntry>(assets: &[T]) -> Option<(&T, Version)> {
    assets
        .iter()
        .filter(|a| a.kind().eq_ignore_ascii_case("full"))
        .filter_map(|a| parse_version(a.version()).map(|v| (a, v)))
        .max_by(|a, b| a.1.cmp(&b.1))
}

// Which copies update

/// Why this copy does not update itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Off {
    /// Not a Velopack install (`<root>\current\Yap.exe` next to
    /// `<root>\Update.exe`): a dev build or a copy.
    NotInstalled,
    /// Velopack's portable layout (`<root>\.portable`).
    Portable,
    /// A named profile (`--profile`, `YAP_PROFILE`): only the default
    /// profile updates the install, which every profile shares.
    NamedProfile,
    /// Started from [`DEV_FOLDER`].
    DevFolder,
}

impl Off {
    /// What Settings says.
    pub fn reason(self) -> &'static str {
        match self {
            Self::NotInstalled => "This copy of Yap isn't installed, so it doesn't update itself.",
            Self::Portable => {
                "Portable copies don't update themselves. Install Yap to get updates."
            }
            Self::NamedProfile => "Only the main profile checks for updates.",
            Self::DevFolder => "Development copies don't update themselves.",
        }
    }
}

/// This launch, as far as updates care.
#[derive(Debug, Clone, Copy)]
pub struct Launch<'a> {
    /// This executable's full path.
    pub exe: &'a str,
    /// The named profile, `None` for the default one.
    pub profile: Option<&'a str>,
    /// [`ENV_TEST`] is set.
    pub test: bool,
}

/// The install root when this launch updates itself, else why not. The
/// layout rule is the installer hooks' (`install::velopack_root`).
pub fn eligibility(launch: &Launch<'_>, is_file: impl Fn(&str) -> bool) -> Result<String, Off> {
    let root = install::velopack_root(launch.exe, &is_file).ok_or(Off::NotInstalled)?;
    if is_file(&format!(r"{root}\.portable")) {
        return Err(Off::Portable);
    }
    if launch.test {
        return Ok(root);
    }
    if launch.profile.is_some() {
        return Err(Off::NamedProfile);
    }
    if install::is_within(launch.exe, DEV_FOLDER) {
        return Err(Off::DevFolder);
    }
    Ok(root)
}

/// Whether Velopack may install an already downloaded update as the app
/// starts (`VelopackApp::set_auto_apply_on_startup`): only for a copy that
/// updates, with install on quit on (the start finishes what a quit could
/// not), and never with a pin (the pin is checked against the feed).
pub fn apply_at_start(eligible: bool, prefs: &Prefs) -> bool {
    eligible && prefs.install_on_quit && prefs.pinned_version.is_none()
}

/// What Velopack passes the restarted app: the profile, so a test run on a
/// named profile comes back on it.
pub fn restart_args(profile: Option<&str>) -> Vec<String> {
    profile
        .map(|name| vec!["--profile".to_owned(), name.to_owned()])
        .unwrap_or_default()
}

// When to check

/// The check timing: the first check, the interval, and the retries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    pub first: Duration,
    pub every: Duration,
    pub retry_first: Duration,
    pub retry_max: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self::new(None, None)
    }
}

impl Timing {
    /// The defaults, or [`ENV_INTERVAL`] / [`ENV_FIRST`] (whole seconds,
    /// at least 1). No wait is ever longer than the interval.
    pub fn new(every_secs: Option<&str>, first_secs: Option<&str>) -> Self {
        let secs = |text: Option<&str>| {
            text.and_then(|t| t.trim().parse::<u64>().ok())
                .filter(|&s| s >= 1)
                .map(Duration::from_secs)
        };
        let every = secs(every_secs).unwrap_or(CHECK_EVERY);
        let first = secs(first_secs).unwrap_or(FIRST_CHECK).min(every);
        Self {
            first,
            every,
            retry_first: RETRY_FIRST.min(every),
            retry_max: RETRY_MAX.min(every),
        }
    }

    /// The wait after `failures` failures in a row (1 or more): doubling
    /// from the first retry, capped.
    pub fn backoff(&self, failures: u32) -> Duration {
        let doublings = failures.saturating_sub(1).min(20);
        self.retry_first
            .saturating_mul(1 << doublings)
            .min(self.retry_max)
    }
}

// The state the UI shows

/// What the updater is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// This copy does not update.
    Off(Off),
    /// No check has finished yet.
    Idle,
    Checking,
    UpToDate,
    Downloading {
        version: String,
        percent: u8,
    },
    /// Downloaded; installs on restart (or quit).
    Ready,
    /// The last check or download failed; it retries by itself.
    Failed {
        message: String,
    },
    /// Velopack's updater was started; the app is on its way out.
    Applying,
}

/// The downloaded update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub version: String,
    pub notes: String,
}

/// What QML shows (`Updater`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct View {
    /// `off`, `idle`, `checking`, `upToDate`, `downloading`, `ready`,
    /// `error` or `applying`.
    pub phase: &'static str,
    pub percent: u8,
    /// The version being downloaded or ready.
    pub version: String,
    /// The ready update's release notes (Markdown), may be empty.
    pub notes: String,
    /// Why updates are off, or what failed.
    pub message: String,
    /// The shell's "Restart to update": an update is ready and no call is
    /// up.
    pub prompt: bool,
    /// "Restart now" may run (ready, no call).
    pub can_restart: bool,
    /// A check may start now (not off, not busy, not applying).
    pub can_check: bool,
    pub in_call: bool,
}

/// The updater's state and schedule. Time is the caller's clock: the time
/// since the app started.
#[derive(Debug, Clone)]
pub struct Updater {
    timing: Timing,
    phase: Phase,
    ready: Option<Ready>,
    /// When the next check is due; `None` while one runs, or never.
    next: Option<Duration>,
    failures: u32,
    in_call: bool,
}

impl Updater {
    /// A fresh updater at `now`: the first check after [`Timing::first`],
    /// or none when this copy does not update.
    pub fn new(timing: Timing, now: Duration, off: Option<Off>) -> Self {
        let (phase, next) = match off {
            Some(off) => (Phase::Off(off), None),
            None => (Phase::Idle, Some(now + timing.first)),
        };
        Self {
            timing,
            phase,
            ready: None,
            next,
            failures: 0,
            in_call: false,
        }
    }

    pub fn phase(&self) -> &Phase {
        &self.phase
    }

    pub fn ready(&self) -> Option<&Ready> {
        self.ready.as_ref()
    }

    pub fn failures(&self) -> u32 {
        self.failures
    }

    /// When the next check is due (`None`: none planned).
    pub fn next_check(&self) -> Option<Duration> {
        self.next
    }

    fn busy(&self) -> bool {
        matches!(
            self.phase,
            Phase::Checking | Phase::Downloading { .. } | Phase::Applying
        ) || self.next.is_none()
    }

    /// How long until the next check (zero when due); `None` when none is
    /// planned.
    pub fn until_due(&self, now: Duration) -> Option<Duration> {
        self.next.map(|next| next.saturating_sub(now))
    }

    pub fn due(&self, now: Duration) -> bool {
        self.next.is_some_and(|next| now >= next)
    }

    /// "Check for updates": due now, unless updates are off or one runs.
    /// Returns whether a check is now due.
    pub fn request(&mut self, now: Duration) -> bool {
        if matches!(self.phase, Phase::Off(_) | Phase::Applying) || self.busy() {
            return false;
        }
        self.next = Some(now);
        true
    }

    /// A check starts: nothing more is planned until it ends. A ready
    /// update stays shown while a newer one is looked for.
    pub fn begin(&mut self) {
        self.next = None;
        if self.ready.is_none() {
            self.phase = Phase::Checking;
        }
    }

    fn settle(&mut self, now: Duration) {
        self.failures = 0;
        self.next = Some(now + self.timing.every);
    }

    /// The feed has nothing newer.
    pub fn up_to_date(&mut self, now: Duration) {
        self.settle(now);
        self.phase = if self.ready.is_some() {
            Phase::Ready
        } else {
            Phase::UpToDate
        };
    }

    /// Download progress for `version`, 0-100.
    pub fn downloading(&mut self, version: &str, percent: i16) {
        if self.ready.as_ref().is_some_and(|r| r.version == version) {
            return;
        }
        let percent = u8::try_from(percent.clamp(0, 100)).unwrap_or(0);
        self.phase = Phase::Downloading {
            version: version.to_owned(),
            percent,
        };
    }

    /// `version` is downloaded (or was already, from an earlier run).
    pub fn downloaded(&mut self, version: &str, notes: &str, now: Duration) {
        self.settle(now);
        self.ready = Some(Ready {
            version: version.to_owned(),
            notes: notes.trim().to_owned(),
        });
        self.phase = Phase::Ready;
    }

    /// The check or the download failed: retried after the backoff. A
    /// ready update stays ready.
    pub fn failed(&mut self, message: &str, now: Duration) {
        self.failures = self.failures.saturating_add(1);
        self.next = Some(now + self.timing.backoff(self.failures));
        self.phase = if self.ready.is_some() {
            Phase::Ready
        } else {
            Phase::Failed {
                message: message.to_owned(),
            }
        };
    }

    /// The channel or the pin changed: what was found may not apply any
    /// more, so check again soon.
    pub fn reset(&mut self, now: Duration) {
        if matches!(self.phase, Phase::Off(_) | Phase::Applying) {
            return;
        }
        if !matches!(self.phase, Phase::Checking | Phase::Downloading { .. }) {
            self.phase = Phase::Idle;
            self.next = Some(now);
        }
        self.ready = None;
        self.failures = 0;
    }

    /// A voice call started or ended.
    pub fn set_in_call(&mut self, in_call: bool) {
        self.in_call = in_call;
    }

    pub fn in_call(&self) -> bool {
        self.in_call
    }

    /// "Restart now" may run: ready, and never during a call.
    pub fn can_restart(&self) -> bool {
        self.ready.is_some() && !self.in_call && !matches!(self.phase, Phase::Applying)
    }

    /// Whether the quit installs the ready update. A quit during a call
    /// still does: the user ended the call by quitting.
    pub fn apply_on_quit(&self, install_on_quit: bool) -> bool {
        install_on_quit && self.ready.is_some() && !matches!(self.phase, Phase::Applying)
    }

    /// Velopack's updater was started (restart or quit): nothing more runs.
    pub fn applying(&mut self) {
        self.phase = Phase::Applying;
        self.next = None;
    }

    pub fn view(&self) -> View {
        let ready = self.ready.clone();
        let (phase, percent, version, message) = match &self.phase {
            Phase::Off(off) => ("off", 0, String::new(), off.reason().to_owned()),
            Phase::Idle => ("idle", 0, String::new(), String::new()),
            Phase::Checking => ("checking", 0, String::new(), String::new()),
            Phase::UpToDate => ("upToDate", 0, String::new(), String::new()),
            Phase::Downloading { version, percent } => {
                ("downloading", *percent, version.clone(), String::new())
            }
            Phase::Ready => (
                "ready",
                100,
                ready
                    .as_ref()
                    .map(|r| r.version.clone())
                    .unwrap_or_default(),
                String::new(),
            ),
            Phase::Failed { message } => ("error", 0, String::new(), message.clone()),
            Phase::Applying => (
                "applying",
                100,
                ready
                    .as_ref()
                    .map(|r| r.version.clone())
                    .unwrap_or_default(),
                String::new(),
            ),
        };
        let can_restart = self.can_restart();
        View {
            phase,
            percent,
            version,
            notes: ready.map(|r| r.notes).unwrap_or_default(),
            message,
            prompt: can_restart,
            can_restart,
            can_check: !self.busy() && !matches!(self.phase, Phase::Off(_)),
            in_call: self.in_call,
        }
    }
}

/// What Settings says about a failed check or download, from Velopack's
/// error text (which goes to the log as it is): one plain sentence.
pub fn friendly_error(text: &str) -> &'static str {
    let lower = text.to_ascii_lowercase();
    let has = |needles: &[&str]| needles.iter().any(|n| lower.contains(n));
    if has(&[
        "checksum",
        "size did not match",
        "invalid package",
        "zip error",
    ]) {
        "The download was damaged."
    } else if has(&["404", "not found", "cannot find", "no feed", "json error"]) {
        "The update feed wasn't found."
    } else if has(&[
        "network",
        "http",
        "dns",
        "timed out",
        "timeout",
        "connection",
        "os error 100",
    ]) {
        "Yap couldn't reach the update server."
    } else if has(&["not properly installed", "not installed"]) {
        "This copy of Yap isn't installed properly."
    } else if has(&[
        "io error",
        "access is denied",
        "being used by another process",
    ]) {
        "The update couldn't be saved."
    } else {
        "Something went wrong."
    }
}

/// The release notes as plain text: Markdown headings and emphasis
/// marks dropped, list items as bullets.
pub fn plain_notes(markdown: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in markdown.trim().lines() {
        let trimmed = line.trim_end();
        let text = trimmed.trim_start();
        let indent = &trimmed[..trimmed.len() - text.len()];
        let text = if text.starts_with('#') {
            text.trim_start_matches('#').trim_start().to_owned()
        } else if let Some(item) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
            format!("{indent}• {item}")
        } else {
            trimmed.to_owned()
        };
        out.push(text.replace("**", "").replace('`', ""));
    }
    out.join("\n")
}

/// A short, single-line error for the log (Velopack's errors can carry a
/// whole HTTP response).
pub fn short_error(text: &str) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or("Unknown error");
    let mut out: String = line.chars().take(160).collect();
    if line.chars().count() > 160 {
        out.push('…');
    }
    out
}
