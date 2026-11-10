//! The bridge's I/O on Windows: the marker file, the HTTP requests, the
//! download, the Setup run and the launch. Every decision is `policy`'s.

use std::fs::File;
use std::future::Future;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::MetadataExt;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use super::policy::{
    self, Liveness, Marker, NativeRelease, SwitchAnswer, Trigger, ERROR_BACKOFF, NOT_YET_BACKOFF,
};
use super::{Failure, HEADLESS_ARG};
use crate::app_updates::DownloadEvent;
use crate::authenticode;

/// `1` turns the bridge off for this process.
const DISABLE_ENV: &str = "YAP_NO_NATIVE_HANDOFF";
/// A JSON file that replaces discovery (tests, CI). Ignored by an installed
/// copy (`is_installed_build`): CI runs a copy from elsewhere.
const OVERRIDE_ENV: &str = "YAP_HANDOFF_RELEASE";

/// Not `releases/latest`, which stays the Tauri release (`policy::
/// pick_native_release`).
const RELEASES_URL: &str = "https://api.github.com/repos/115jon/yap/releases?per_page=30";
const SWITCH_URL: &str = "https://yap.115jon.com/api/desktop/native-handoff";

const GITHUB_TIMEOUT: Duration = Duration::from_secs(10);
const SWITCH_TIMEOUT: Duration = Duration::from_secs(5);
/// How long one answer of the switch is used before asking again.
const SWITCH_MEMO: Duration = Duration::from_secs(5 * 60);
/// How long `fetch_update` waits for a check; a slower one keeps running
/// and the next check uses what it found.
const CHECK_BUDGET: Duration = Duration::from_secs(6);
/// How long a signature check with online revocation may take.
const REVOCATION_DEADLINE: Duration = Duration::from_secs(30);
const SETUP_WAIT: Duration = Duration::from_secs(5 * 60);
const POLL: Duration = Duration::from_millis(250);

const MAX_SWITCH_BODY: usize = 4 * 1024;
const MAX_RELEASES_BODY: usize = 8 * 1024 * 1024;
const MAX_LOG_BYTES: u64 = 1024 * 1024;

/// `%TEMP%\yap-native-handoff`: downloads, logs, and one fresh folder per
/// attempt that holds nothing but the Setup it runs.
const WORK_DIR: &str = "yap-native-handoff";
/// The native app's Velopack root under `%LOCALAPPDATA%`.
const NATIVE_ROOT: &str = "Yap.App";
const NATIVE_EXE: &str = "Yap.exe";

const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DETACHED_PROCESS: u32 = 0x0000_0008;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;

// ── Shared state ─────────────────────────────────────────────────────────

struct Shared {
    /// One check at a time; a second waits and reuses what the first found.
    check: tokio::sync::Mutex<()>,
    /// One download at a time (the background one or the install's).
    download: tokio::sync::Mutex<()>,
    /// The marker's read-modify-write.
    marker: Mutex<()>,
    switch_memo: Mutex<Option<(Instant, Result<(), String>)>>,
    /// What `fetch_update` last offered, for `install_update`.
    offered: Mutex<Option<NativeRelease>>,
    /// The last "not now" reason logged, so a 30-minute check does not
    /// repeat it.
    last_reason: Mutex<Option<String>>,
    running: AtomicBool,
}

fn shared() -> &'static Shared {
    static SHARED: OnceLock<Shared> = OnceLock::new();
    SHARED.get_or_init(|| Shared {
        check: tokio::sync::Mutex::new(()),
        download: tokio::sync::Mutex::new(()),
        marker: Mutex::new(()),
        switch_memo: Mutex::new(None),
        offered: Mutex::new(None),
        last_reason: Mutex::new(None),
        running: AtomicBool::new(false),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// A failed step. `counted`: it failed installing or launching, which
/// counts toward the automatic attempts; a refusal of the switch, a network
/// error or an unanswered revocation check is only noted.
#[derive(Debug)]
struct StepError {
    message: String,
    counted: bool,
}

fn counted(message: impl Into<String>) -> StepError {
    StepError {
        message: message.into(),
        counted: true,
    }
}

fn not_counted(message: impl Into<String>) -> StepError {
    StepError {
        message: message.into(),
        counted: false,
    }
}

// ── Paths and the log ────────────────────────────────────────────────────

fn work_dir() -> PathBuf {
    std::env::temp_dir().join(WORK_DIR)
}

fn is_reparse_point(path: &Path) -> bool {
    std::fs::symlink_metadata(path)
        .is_ok_and(|m| m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
}

/// The work folder, made if needed, and refused when it (or `%TEMP%`) is a
/// junction or a symbolic link.
fn ensure_work_dir() -> Result<PathBuf, String> {
    let dir = work_dir();
    if let Some(parent) = dir.parent() {
        if is_reparse_point(parent) {
            return Err(format!("{} is a reparse point", parent.display()));
        }
    }
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    if is_reparse_point(&dir) {
        return Err(format!("{} is a reparse point", dir.display()));
    }
    Ok(dir)
}

/// One line in the app log and in `%TEMP%\yap-native-handoff\handoff.log`
/// (the headless run has no app log).
fn note(line: &str) {
    log::info!("[NativeHandoff] {line}");
    let Ok(dir) = ensure_work_dir() else {
        return;
    };
    let path = dir.join("handoff.log");
    if path.metadata().is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::rename(&path, dir.join("handoff.old.log"));
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(file, "{} [{}] {line}", now_ms(), std::process::id());
    }
}

/// Logs why the bridge does nothing, once per distinct reason.
fn note_reason(reason: &str) {
    let mut last = shared()
        .last_reason
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if last.as_deref() != Some(reason) {
        note(&format!("not now: {reason}"));
        *last = Some(reason.to_owned());
    }
}

fn marker_path() -> Option<PathBuf> {
    crate::windows_app_data_root().map(|d| d.join(policy::MARKER_FILE))
}

struct NativePaths {
    /// `%LOCALAPPDATA%\Yap.App`.
    root: PathBuf,
    /// `<root>\current\Yap.exe`: the app, and what the bridge starts (the
    /// file it verified and holds, not the root stub).
    current_exe: PathBuf,
}

fn native_paths() -> Option<NativePaths> {
    let root = PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join(NATIVE_ROOT);
    Some(NativePaths {
        current_exe: root.join("current").join(NATIVE_EXE),
        root,
    })
}

/// Installed by Velopack: the app under `current\` and its `Update.exe`.
fn native_present(paths: &NativePaths) -> bool {
    paths.current_exe.is_file() && paths.root.join("Update.exe").is_file()
}

// ── Gates ────────────────────────────────────────────────────────────────

fn disabled_by_env() -> bool {
    std::env::var(DISABLE_ENV).is_ok_and(|v| {
        let v = v.trim();
        v == "1" || v.eq_ignore_ascii_case("true")
    })
}

/// Only an installed copy (`%LOCALAPPDATA%\Yap\app-<version>\`) hands off
/// by itself; a dev or test build never does.
fn is_installed_build() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| crate::versioned_windows_install_root_from_exe_path(&exe))
        .is_some()
}

/// `CurrentBuildNumber` (GetVersionEx answers what the manifest declares,
/// not the real build).
fn windows_build() -> Option<u32> {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::ERROR_SUCCESS;
    use windows::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let subkey = HSTRING::from(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
    let name = HSTRING::from("CurrentBuildNumber");
    let mut buffer = [0u16; 32];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    // SAFETY: the buffer and its size are ours.
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            &subkey,
            &name,
            RRF_RT_REG_SZ,
            None,
            Some(buffer.as_mut_ptr().cast()),
            Some(&mut size),
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len]).trim().parse().ok()
}

// ── The marker ───────────────────────────────────────────────────────────

/// A random name part: a GUID, or a hash of the time and the process.
fn random_id() -> String {
    match windows::core::GUID::new() {
        Ok(guid) => format!("{guid:?}"),
        Err(_) => {
            let seed = format!(
                "{}-{}-{:?}",
                now_ms(),
                std::process::id(),
                std::thread::current().id()
            );
            Sha256::digest(seed.as_bytes())
                .iter()
                .take(16)
                .map(|b| format!("{b:02x}"))
                .collect()
        }
    }
}

/// The marker; a missing one is the default. One that is there but cannot
/// be read is an error: the bridge then stays dormant and leaves it alone.
fn read_marker() -> Result<Marker, String> {
    let path = marker_path().ok_or("APPDATA is not set")?;
    match std::fs::read(&path) {
        Ok(bytes) => Marker::parse(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Marker::default()),
        Err(e) => Err(format!("read {}: {e}", path.display())),
    }
}

fn write_marker(marker: &Marker) -> Result<(), String> {
    let path = marker_path().ok_or("APPDATA is not set")?;
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, marker.to_json())
        .and_then(|()| std::fs::rename(&tmp, &path))
        .map_err(|e| format!("write {}: {e}", path.display()))
}

/// Reads the marker (giving it an install id), applies `change`, and
/// writes it back when it changed. Never writes over an unreadable one.
fn with_marker<T>(change: impl FnOnce(&mut Marker) -> T) -> Result<T, String> {
    let _guard = shared().marker.lock().unwrap_or_else(|e| e.into_inner());
    let mut marker = read_marker()?;
    let before = marker.clone();
    marker.ensure_install_id(random_id);
    let out = change(&mut marker);
    if marker != before {
        write_marker(&marker)?;
    }
    Ok(out)
}

// ── HTTP ─────────────────────────────────────────────────────────────────

fn client(timeout: Duration, https_only: bool) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .https_only(https_only)
        .user_agent(concat!("Yap/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| e.to_string())
}

/// GET `url`: its status and at most `cap` bytes of body.
async fn get_capped(
    url: &str,
    timeout: Duration,
    cap: usize,
    accept: &str,
) -> Result<(u16, Vec<u8>), String> {
    let mut response = client(timeout, true)?
        .get(url)
        .header("Accept", accept)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
        if body.len() + chunk.len() > cap {
            return Err(format!("{url} answered more than {cap} bytes"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok((status, body))
}

/// The remote switch, fail closed, memoised for [`SWITCH_MEMO`] unless
/// `fresh`.
async fn ask_switch(install_id: &str, fresh: bool) -> Result<(), String> {
    if !fresh {
        if let Some((at, answer)) = shared()
            .switch_memo
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
        {
            if at.elapsed() < SWITCH_MEMO {
                return answer.clone();
            }
        }
    }
    let answer = match get_capped(
        SWITCH_URL,
        SWITCH_TIMEOUT,
        MAX_SWITCH_BODY,
        "application/json",
    )
    .await
    {
        Ok((status, body)) => policy::switch_admits(
            SwitchAnswer::Response {
                status,
                body: &body,
            },
            install_id,
        ),
        Err(e) => policy::switch_admits(SwitchAnswer::Unreachable(e), install_id),
    };
    *shared()
        .switch_memo
        .lock()
        .unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), answer.clone()));
    answer
}

/// The switch for this install: the override's when it has one, else the
/// endpoint's.
async fn switch_allows(
    overridden: Option<&policy::Override>,
    install_id: &str,
    fresh: bool,
) -> Result<(), String> {
    match overridden.and_then(|o| o.switch) {
        Some(switch) => policy::admitted(&switch, install_id),
        None => ask_switch(install_id, fresh).await,
    }
}

/// The test override, never for an installed copy.
fn read_override() -> Result<Option<policy::Override>, String> {
    let Some(path) = std::env::var_os(OVERRIDE_ENV) else {
        return Ok(None);
    };
    if is_installed_build() {
        note_reason(&format!("{OVERRIDE_ENV} is ignored by an installed copy"));
        return Ok(None);
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| format!("{OVERRIDE_ENV} {}: {e}", Path::new(&path).display()))?;
    policy::parse_override(&bytes)
        .map(Some)
        .map_err(|e| format!("{OVERRIDE_ENV} is not readable: {e}"))
}

// ── Discovery ────────────────────────────────────────────────────────────

/// Whether the native app may be offered for `trigger`, and which release.
/// `ignore_backoff`: ask GitHub even inside a backoff (the headless run).
async fn discover(trigger: Trigger, ignore_backoff: bool) -> Result<NativeRelease, String> {
    if disabled_by_env() {
        return Err(format!("{DISABLE_ENV} is set"));
    }
    if trigger != Trigger::Support && !is_installed_build() {
        return Err("not an installed build".into());
    }
    let build = windows_build();
    if !policy::windows_build_supported(build) {
        return Err(format!(
            "Windows build {build:?} is below {}",
            policy::MIN_WINDOWS_BUILD
        ));
    }
    let marker_file = marker_path().ok_or("APPDATA is not set")?;
    let marker = with_marker(|m| m.clone())?;
    if !marker_file.is_file() {
        // Without it the install id (the rollout bucket) would change on
        // every check.
        return Err(format!("{} could not be written", marker_file.display()));
    }
    marker.may_run(trigger).map_err(|d| d.to_string())?;

    let overridden = read_override()?;
    switch_allows(overridden.as_ref(), &marker.install_id, false).await?;
    if let Some(o) = overridden {
        return o.release.map_err(|n| format!("{OVERRIDE_ENV}: {n}"));
    }

    let now = now_ms();
    if let Some(release) = marker.fresh_release(now) {
        return Ok(release.clone());
    }
    let stale = marker.release.clone().map(|c| c.release);
    if !ignore_backoff && !marker.check_due(now) {
        return stale.ok_or_else(|| "waiting before asking GitHub again".into());
    }
    match get_capped(
        RELEASES_URL,
        GITHUB_TIMEOUT,
        MAX_RELEASES_BODY,
        "application/vnd.github+json",
    )
    .await
    {
        Ok((200, body)) => match policy::pick_native_release(&body) {
            Ok(release) => {
                note(&format!("found the native app {}", release.version));
                with_marker(|m| m.remember_release(release.clone(), now))?;
                Ok(release)
            }
            Err(not_yet) => {
                with_marker(|m| {
                    m.back_off(now, NOT_YET_BACKOFF);
                    m.release = None;
                })?;
                Err(not_yet.to_string())
            }
        },
        // Rate limits and outages: sooner than a "not yet".
        Ok((status, _)) => {
            with_marker(|m| m.back_off(now, ERROR_BACKOFF))?;
            stale.ok_or_else(|| format!("GitHub answered {status}"))
        }
        Err(e) => {
            with_marker(|m| m.back_off(now, ERROR_BACKOFF))?;
            stale.ok_or_else(|| format!("GitHub unreachable: {e}"))
        }
    }
}

/// For `fetch_update`: the native release to offer `trigger`, or `None`
/// (dormant, not admitted, not yet, or slower than [`CHECK_BUDGET`]; a slow
/// check finishes in the background for the next one). Starts the Setup's
/// download in the background when it finds one.
pub async fn check(trigger: Trigger) -> Option<NativeRelease> {
    let task = tauri::async_runtime::spawn(check_now(trigger));
    match tokio::time::timeout(CHECK_BUDGET, task).await {
        Ok(Ok(found)) => found,
        Ok(Err(e)) => {
            note(&format!("check failed: {e}"));
            None
        }
        Err(_) => {
            note("check is slow; answering without it");
            None
        }
    }
}

async fn check_now(trigger: Trigger) -> Option<NativeRelease> {
    let _one_at_a_time = shared().check.lock().await;
    match discover(trigger, false).await {
        Ok(release) => {
            *shared()
                .last_reason
                .lock()
                .unwrap_or_else(|e| e.into_inner()) = None;
            predownload(&release);
            Some(release)
        }
        Err(reason) => {
            note_reason(&reason);
            None
        }
    }
}

/// What `fetch_update` answered last: the native release it offered, or
/// `None` when it offered the Tauri update (or nothing).
pub fn set_offered(release: Option<NativeRelease>) {
    *shared().offered.lock().unwrap_or_else(|e| e.into_inner()) = release;
}

/// What `fetch_update` offered last; `install_update` takes it.
pub fn take_offered() -> Option<NativeRelease> {
    shared()
        .offered
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
}

/// Downloads and checks the Setup in the background, so the launch
/// updater's install starts at once.
fn predownload(release: &NativeRelease) {
    if native_paths().is_some_and(|p| native_present(&p)) || shared().download.try_lock().is_err() {
        return;
    }
    let release = release.clone();
    tauri::async_runtime::spawn(async move {
        match ensure_downloaded(&release, None).await {
            Ok(path) => note(&format!("Setup ready at {}", path.display())),
            Err(e) => note(&format!("background download failed: {}", e.message)),
        }
    });
}

// ── Download ─────────────────────────────────────────────────────────────

type Progress<'a> = Option<&'a mut (dyn FnMut(DownloadEvent) + Send)>;

fn emit(progress: &mut Progress<'_>, event: DownloadEvent) {
    if let Some(p) = progress.as_mut() {
        p(event);
    }
}

fn setup_name(release: &NativeRelease) -> String {
    format!("Yap.App-win-Setup-{}.exe", release.version)
}

/// The Setup of `release` in the work folder, downloaded unless it is there
/// already with the right size. The download streams into `.partial`, is
/// hashed as it arrives, and is renamed only when its SHA-256 equals the
/// release's digest. [`install`] hashes it again through its held handle.
async fn ensure_downloaded(
    release: &NativeRelease,
    mut progress: Progress<'_>,
) -> Result<PathBuf, StepError> {
    let _one_at_a_time = shared().download.lock().await;
    let dir = ensure_work_dir().map_err(counted)?;
    let path = dir.join(setup_name(release));
    let cached = std::fs::symlink_metadata(&path)
        .is_ok_and(|m| m.is_file() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0);
    if cached && path.metadata().is_ok_and(|m| m.len() == release.size) {
        emit(
            &mut progress,
            DownloadEvent::Started {
                content_length: Some(release.size),
            },
        );
        emit(
            &mut progress,
            DownloadEvent::Progress {
                chunk_length: release.size as usize,
            },
        );
        emit(&mut progress, DownloadEvent::Finished);
        return Ok(path);
    }
    remove_other_setups(&dir, &path);
    let partial = dir.join(format!("{}.partial", setup_name(release)));
    let result = download_to(release, &partial, &mut progress).await;
    if let Err(e) = result {
        let _ = std::fs::remove_file(&partial);
        return Err(e);
    }
    let _ = std::fs::remove_file(&path);
    std::fs::rename(&partial, &path)
        .map_err(|e| counted(format!("place {}: {e}", path.display())))?;
    emit(&mut progress, DownloadEvent::Finished);
    note(&format!("downloaded and checked {}", path.display()));
    Ok(path)
}

async fn download_to(
    release: &NativeRelease,
    partial: &Path,
    progress: &mut Progress<'_>,
) -> Result<(), StepError> {
    // Read and connect timeouts, not a total one: 500 MB on a slow line is fine.
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(60))
        .https_only(release.url.starts_with("https://"))
        .user_agent(concat!("Yap/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| counted(e.to_string()))?;
    let mut response = client
        .get(&release.url)
        .send()
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| not_counted(format!("download: {e}")))?;
    if let Some(length) = response.content_length() {
        if length != release.size {
            return Err(counted(format!(
                "download is {length} bytes, the release says {}",
                release.size
            )));
        }
    }
    emit(
        progress,
        DownloadEvent::Started {
            content_length: Some(release.size),
        },
    );
    let mut file =
        File::create(partial).map_err(|e| counted(format!("create {}: {e}", partial.display())))?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| not_counted(format!("download: {e}")))?
    {
        total += chunk.len() as u64;
        if total > release.size {
            return Err(counted(format!("download is over {} bytes", release.size)));
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .map_err(|e| counted(format!("write {}: {e}", partial.display())))?;
        emit(
            progress,
            DownloadEvent::Progress {
                chunk_length: chunk.len(),
            },
        );
    }
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|e| counted(e.to_string()))?;
    drop(file);
    if total != release.size {
        // The connection ended early.
        return Err(not_counted(format!(
            "download ended at {total} of {} bytes",
            release.size
        )));
    }
    if !policy::digest_matches(&release.sha256, &hasher.finalize()) {
        return Err(counted(
            "the download's SHA-256 does not match the release's digest",
        ));
    }
    Ok(())
}

/// Older Setups (and partial downloads) left in the work folder.
fn remove_other_setups(dir: &Path, keep: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path != keep && name.starts_with("Yap.App-win-Setup-") {
            let _ = std::fs::remove_file(&path);
        }
    }
}

// ── Verification ─────────────────────────────────────────────────────────

/// The SHA-256 of an open file, read from its start through the handle.
fn hash_handle(file: &mut File) -> Result<[u8; 32], String> {
    file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().into())
}

/// The signature of `path` (which the caller holds open), with revocation
/// checked online, against the signer rule. An unanswered revocation check
/// is a failure that is not counted.
fn check_signed(path: &Path) -> Result<authenticode::Signer, StepError> {
    let signer = authenticode::verify_signature_online(path, REVOCATION_DEADLINE).map_err(|e| {
        StepError {
            message: format!("{}: {}", path.display(), e.message),
            counted: !e.transient,
        }
    })?;
    policy::check_signer(
        &signer.subject,
        &signer.issuer,
        signer.timestamped,
        &signer.ekus,
    )
    .map_err(|e| counted(format!("{}: {e}", path.display())))?;
    Ok(signer)
}

/// Opens `path` read-shared only and checks its signer. Returns the handle,
/// to hold until the file has been used.
fn open_signed(path: &Path) -> Result<File, StepError> {
    let file = authenticode::open_read_shared(path).map_err(counted)?;
    check_signed(path)?;
    Ok(file)
}

/// The Setup, held open: its digest again through the handle (so the bytes
/// that run are the bytes checked), then its signer.
fn open_verified_setup(path: &Path, release: &NativeRelease) -> Result<File, StepError> {
    let mut file = authenticode::open_read_shared(path).map_err(counted)?;
    if !policy::digest_matches(&release.sha256, &hash_handle(&mut file).map_err(counted)?) {
        return Err(counted(format!(
            "{}: SHA-256 does not match the release's digest",
            path.display()
        )));
    }
    let signer = check_signed(path)?;
    note(&format!(
        "Setup verified: {} ({}), issued by {}",
        signer.subject, signer.thumbprint, signer.issuer
    ));
    Ok(file)
}

async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, StepError> + Send + 'static,
) -> Result<T, StepError> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| counted(e.to_string()))?
}

/// A fresh folder for one attempt: a random name that must not exist yet,
/// not a reparse point, holding only the Setup moved into it. Nothing else
/// the Setup could load sits beside it.
fn stage_setup(downloaded: &Path, release: &NativeRelease) -> Result<(PathBuf, PathBuf), String> {
    let work = ensure_work_dir()?;
    let dir = work.join(format!("attempt-{}", random_id()));
    std::fs::create_dir(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    if is_reparse_point(&dir) {
        return Err(format!("{} is a reparse point", dir.display()));
    }
    let setup = dir.join(setup_name(release));
    std::fs::rename(downloaded, &setup)
        .map_err(|e| format!("move the Setup into {}: {e}", dir.display()))?;
    let entries: Vec<_> = std::fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    let only_setup = entries.len() == 1
        && entries[0].path() == setup
        && std::fs::symlink_metadata(&setup)
            .is_ok_and(|m| m.is_file() && m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0);
    if !only_setup {
        return Err(format!("{} holds more than the Setup", dir.display()));
    }
    Ok((dir, setup))
}

/// Runs the verified Setup silently (`--silent` never starts the app) from
/// its own fresh folder and waits for it, holding it open with read sharing
/// only. The folder is removed afterwards.
async fn install(downloaded: PathBuf, release: NativeRelease) -> Result<(), StepError> {
    let (dir, setup) = match stage_setup(&downloaded, &release) {
        Ok(staged) => staged,
        Err(e) => {
            let _ = std::fs::remove_file(&downloaded);
            return Err(counted(e));
        }
    };
    let result = run_setup(&setup, release).await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}

async fn run_setup(setup: &Path, release: NativeRelease) -> Result<(), StepError> {
    let held = {
        let setup = setup.to_path_buf();
        blocking(move || open_verified_setup(&setup, &release)).await?
    };
    let log = work_dir().join("setup.log");
    note(&format!("running {} --silent", setup.display()));
    let mut child = Command::new(setup)
        .arg("--silent")
        .arg("--log")
        .arg(&log)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| counted(format!("start {}: {e}", setup.display())))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| counted(e.to_string()))? {
            break status;
        }
        if started.elapsed() > SETUP_WAIT {
            // Left to finish: killing an installer halfway is worse.
            return Err(counted(format!(
                "Setup still running after {} min (log: {})",
                SETUP_WAIT.as_secs() / 60,
                log.display()
            )));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };
    drop(held);
    match status.code() {
        Some(0) => {
            note("Setup finished");
            Ok(())
        }
        code => Err(counted(format!(
            "Setup exited with {} (log: {})",
            code.map_or("no code".into(), |c| c.to_string()),
            log.display()
        ))),
    }
}

// ── Launch ───────────────────────────────────────────────────────────────

/// Starts `exe` on its own: no console, its own process group, out of this
/// process's job when that is allowed.
fn spawn_detached(exe: &Path, args: &[String]) -> Result<Child, String> {
    let spawn = |flags: u32| {
        let mut command = Command::new(exe);
        command.args(args);
        if let Some(dir) = exe.parent() {
            command.current_dir(dir);
        }
        command
            .creation_flags(flags)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
    };
    let base = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    spawn(base | CREATE_BREAKAWAY_FROM_JOB)
        .or_else(|_| spawn(base))
        .map_err(|e| format!("start {}: {e}", exe.display()))
}

fn same_path(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
    norm(a) == norm(b)
}

/// Whether a process runs from `exe` (any of this user's we can query).
fn process_running(exe: &Path) -> bool {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let Some(file_name) = exe.file_name().map(|n| n.to_string_lossy().to_lowercase()) else {
        return false;
    };
    // SAFETY: a snapshot handle we close below.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return false;
    };
    let fresh = || PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut entry = fresh();
    let mut found = false;
    // SAFETY: `entry` is sized for the call.
    let mut walk = unsafe { Process32FirstW(snapshot, &mut entry) };
    while walk.is_ok() && !found {
        let len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
        if name == file_name {
            // SAFETY: a process handle we close at once.
            unsafe {
                if let Ok(process) = OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION,
                    false,
                    entry.th32ProcessID,
                ) {
                    let mut buffer = [0u16; 1024];
                    let mut size = buffer.len() as u32;
                    if QueryFullProcessImageNameW(
                        process,
                        PROCESS_NAME_WIN32,
                        PWSTR(buffer.as_mut_ptr()),
                        &mut size,
                    )
                    .is_ok()
                    {
                        let image = String::from_utf16_lossy(&buffer[..size as usize]);
                        found = same_path(Path::new(&image), exe);
                    }
                    let _ = CloseHandle(process);
                }
            }
        }
        entry = fresh();
        // SAFETY: as above.
        walk = unsafe { Process32NextW(snapshot, &mut entry) };
    }
    // SAFETY: the snapshot from above.
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found
}

/// Verifies `current\Yap.exe`, starts that same file (held open until a
/// process runs from it) and waits [`policy::LIVENESS_WINDOW`]: alive only
/// if it still runs at the end ([`policy::judge_liveness`]).
async fn launch_verified(paths: &NativePaths, args: Vec<String>) -> Result<(), StepError> {
    let exe = paths.current_exe.clone();
    let held = {
        let exe = exe.clone();
        blocking(move || open_signed(&exe)).await?
    };
    note(&format!("starting {}", exe.display()));
    let mut child = spawn_detached(&exe, &args).map_err(counted)?;
    let started = Instant::now();
    let mut held = Some(held);
    loop {
        tokio::time::sleep(POLL).await;
        let exit = child
            .try_wait()
            .ok()
            .flatten()
            .map(|status| status.code().unwrap_or(-1));
        let native_running = process_running(&exe);
        if native_running {
            held.take();
        }
        match policy::judge_liveness(started.elapsed(), exit, native_running) {
            Liveness::Waiting => {}
            Liveness::Alive => {
                note(&format!(
                    "the native app is up after {} ms",
                    started.elapsed().as_millis()
                ));
                return Ok(());
            }
            Liveness::Dead(reason) => return Err(counted(reason)),
        }
    }
}

// ── The attempt ──────────────────────────────────────────────────────────

/// Clears the "running" flag however the attempt ends.
struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        shared().running.store(false, Ordering::Release);
    }
}

fn failure(message: impl Into<String>) -> Failure {
    Failure {
        message: message.into(),
        open_download_page: false,
    }
}

/// One handoff attempt: download (streaming `on_event`), verify, install,
/// verify the installed app, `before_launch` (the native share's shutdown),
/// start it and wait until it stays up. On `Ok` the caller exits this app.
/// A failure is noted in the marker (and counted when it failed installing
/// or launching) and this app keeps running.
pub async fn run<F, Fut>(
    release: NativeRelease,
    trigger: Trigger,
    mut on_event: impl FnMut(DownloadEvent) + Send,
    before_launch: F,
) -> Result<(), Failure>
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = ()> + Send,
{
    if shared().running.swap(true, Ordering::AcqRel) {
        return Err(failure("a handoff is already running"));
    }
    let _guard = RunningGuard;
    with_marker(|m| m.may_run(trigger))
        .map_err(failure)?
        .map_err(|dormant| failure(dormant.to_string()))?;
    with_marker(|m| m.start_attempt(&release.version, now_ms())).map_err(failure)?;
    note(&format!(
        "attempt ({trigger:?}) to hand off to {}",
        release.version
    ));

    match attempt(&release, &mut on_event, before_launch).await {
        Ok(()) => {
            let _ = with_marker(|m| m.record_success(&release.version));
            note("handed off");
            Ok(())
        }
        Err(e) => {
            let open_download_page =
                with_marker(|m| m.record_failure(&e.message, trigger, e.counted)).unwrap_or(false);
            note(&format!(
                "attempt failed ({}): {}",
                if e.counted { "counted" } else { "not counted" },
                e.message
            ));
            Err(Failure {
                message: e.message,
                open_download_page,
            })
        }
    }
}

async fn attempt<F, Fut>(
    release: &NativeRelease,
    on_event: &mut (dyn FnMut(DownloadEvent) + Send),
    before_launch: F,
) -> Result<(), StepError>
where
    F: FnOnce() -> Fut + Send,
    Fut: Future<Output = ()> + Send,
{
    // The switch once more, unmemoised: turning it off stops an install
    // that has not started yet.
    let install_id = with_marker(|m| m.install_id.clone()).map_err(not_counted)?;
    let overridden = read_override().map_err(not_counted)?;
    switch_allows(overridden.as_ref(), &install_id, true)
        .await
        .map_err(not_counted)?;

    let paths = native_paths().ok_or_else(|| counted("LOCALAPPDATA is not set"))?;
    if native_present(&paths) {
        note("the native app is installed already; skipping Setup");
        on_event(DownloadEvent::Started {
            content_length: None,
        });
        on_event(DownloadEvent::Finished);
    } else {
        let setup = ensure_downloaded(release, Some(&mut *on_event)).await?;
        install(setup, release.clone()).await?;
        if !native_present(&paths) {
            return Err(counted(format!(
                "Setup succeeded but {} is missing",
                paths.current_exe.display()
            )));
        }
    }
    before_launch().await;
    launch_verified(&paths, Vec::new()).await
}

// ── Entry points outside the updater ─────────────────────────────────────

/// Before any window: once the handoff is done and the native app is still
/// installed, start it (verified, as [`launch_verified`]) and return true
/// once it runs, so this app exits. Never for a dev build, with the bridge
/// off, without a readable marker, or after [`policy::MAX_REDIRECT_FAILURES`]
/// failed redirects in a row.
pub fn launch_native_if_done() -> bool {
    if disabled_by_env() || !is_installed_build() {
        return false;
    }
    let Some(path) = marker_path() else {
        return false;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return false;
    };
    let Ok(marker) = Marker::parse(&bytes) else {
        note("the marker is unreadable; staying");
        return false;
    };
    let Some(paths) = native_paths() else {
        return false;
    };
    if marker.launch_action(native_present(&paths)) != policy::LaunchAction::LaunchNative {
        return false;
    }
    // A `yap://` link this launch was opened with goes to the native app,
    // which owns the scheme.
    let links: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| {
            let a = a.to_ascii_lowercase();
            a.starts_with("yap://") || a.starts_with("yap-native://")
        })
        .collect();
    let _ = rustls::crypto::ring::default_provider().install_default();
    let outcome = tauri::async_runtime::block_on(launch_verified(&paths, links));
    match outcome {
        Ok(()) => {
            let _ = with_marker(|m| m.record_redirect(true));
            note("handed off before; the native app runs instead");
            true
        }
        Err(e) => {
            if e.counted {
                let _ = with_marker(|m| m.record_redirect(false));
            }
            note(&format!(
                "handed off before, but the redirect failed: {}",
                e.message
            ));
            false
        }
    }
}

/// Whether this launch is the headless run.
pub fn headless_requested() -> bool {
    std::env::args().skip(1).any(|a| a == HEADLESS_ARG)
}

/// `--yap-native-handoff`: the whole flow with no window, for CI and
/// support. Any build, inside a backoff too; the switch, the marker's
/// `optOut`, the digest and the signature still apply. Exit code: 0 handed
/// off, 1 the attempt failed, 2 nothing to hand off to (see handoff.log).
pub fn run_headless() -> i32 {
    let _ = rustls::crypto::ring::default_provider().install_default();
    note("headless run");
    tauri::async_runtime::block_on(async {
        let release = match discover(Trigger::Support, true).await {
            Ok(release) => release,
            Err(reason) => {
                note(&format!("nothing to hand off to: {reason}"));
                return 2;
            }
        };
        match run(
            release,
            Trigger::Support,
            |_: DownloadEvent| {},
            || async {},
        )
        .await
        {
            Ok(()) => 0,
            Err(_) => 1,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("yap-handoff-test-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn release_for(bytes: &[u8]) -> NativeRelease {
        NativeRelease {
            version: "4.0.0".into(),
            url: "https://github.com/115jon/yap/releases/download/v4.0.0/Yap.App-win-Setup.exe"
                .into(),
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect(),
            size: bytes.len() as u64,
        }
    }

    #[test]
    fn a_setup_whose_bytes_changed_is_refused_before_its_signature() {
        let dir = scratch("digest");
        let path = dir.join("Yap.App-win-Setup-4.0.0.exe");
        std::fs::write(&path, b"MZ the bytes the release named").unwrap();
        let release = release_for(b"MZ the bytes the release named");
        std::fs::write(&path, b"MZ the bytes someone swapped in").unwrap();
        let err = open_verified_setup(&path, &release).unwrap_err();
        assert!(err.message.contains("SHA-256"), "{err:?}");
        assert!(err.counted);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unsigned_setup_with_the_right_digest_is_refused() {
        let dir = scratch("unsigned");
        let path = dir.join("Yap.App-win-Setup-4.0.0.exe");
        let bytes = b"MZ not signed at all";
        std::fs::write(&path, bytes).unwrap();
        let err = open_verified_setup(&path, &release_for(bytes)).unwrap_err();
        assert!(err.message.contains("Authenticode"), "{err:?}");
        assert!(err.counted, "a bad signature counts");
        assert!(open_signed(&path).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Against a real signed Setup when one is given (a published
    /// `Yap.App-win-Setup.exe`, never run): `YAP_HANDOFF_TEST_SETUP=<path>`.
    /// Checks revocation online, so it needs the network.
    #[test]
    fn a_published_setup_verifies_when_given() {
        let Some(path) = std::env::var_os("YAP_HANDOFF_TEST_SETUP").map(PathBuf::from) else {
            eprintln!("skipped: YAP_HANDOFF_TEST_SETUP is not set");
            return;
        };
        let bytes = std::fs::read(&path).unwrap();
        let file = open_verified_setup(&path, &release_for(&bytes)).expect("the Setup verifies");
        let signer = authenticode::verify_signature(&path, &file).unwrap();
        eprintln!("{signer:?}");
        assert_eq!(signer.subject, policy::SIGNER_NAME);
        assert!(signer.issuer.starts_with(policy::ISSUER_PREFIX));
        assert!(signer.timestamped);
        assert!(signer.ekus.iter().any(|e| e == policy::IDENTITY_EKU));
        // Held: it cannot be replaced while the bridge would run it.
        assert!(std::fs::OpenOptions::new().write(true).open(&path).is_err());
    }

    #[test]
    fn a_setup_is_staged_alone_in_a_fresh_folder() {
        let dir = scratch("stage");
        let bytes = b"MZ staged";
        let release = release_for(bytes);
        let downloaded = dir.join("download.exe");
        std::fs::write(&downloaded, bytes).unwrap();
        let (attempt, setup) = stage_setup(&downloaded, &release).unwrap();
        assert!(!downloaded.exists(), "moved, not copied");
        assert!(attempt.starts_with(work_dir()));
        assert_eq!(std::fs::read(&setup).unwrap(), bytes);
        assert_eq!(std::fs::read_dir(&attempt).unwrap().count(), 1);
        let again = dir.join("download2.exe");
        std::fs::write(&again, bytes).unwrap();
        let (second, _) = stage_setup(&again, &release).unwrap();
        assert_ne!(attempt, second, "a new folder per attempt");
        let _ = std::fs::remove_dir_all(&attempt);
        let _ = std::fs::remove_dir_all(&second);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_junction_is_a_reparse_point() {
        let dir = scratch("junction");
        let target = dir.join("target");
        let link = dir.join("link");
        std::fs::create_dir_all(&target).unwrap();
        let made = Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .stdout(Stdio::null())
            .status()
            .is_ok_and(|s| s.success());
        if made {
            assert!(is_reparse_point(&link));
        } else {
            eprintln!("skipped: could not make a junction");
        }
        assert!(!is_reparse_point(&target));
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_windows_build_is_read() {
        let build = windows_build().expect("CurrentBuildNumber");
        assert!(build >= 10240, "{build}");
    }

    #[test]
    fn this_process_is_found_by_its_path_and_a_missing_one_is_not() {
        let exe = std::env::current_exe().unwrap();
        assert!(process_running(&exe));
        let upper = PathBuf::from(exe.to_string_lossy().to_uppercase());
        assert!(process_running(&upper), "paths compare without case");
        assert!(!process_running(
            &exe.with_file_name("no-such-yap-process.exe")
        ));
    }

    #[test]
    fn a_test_build_is_not_an_installed_one() {
        assert!(!is_installed_build());
    }
}
