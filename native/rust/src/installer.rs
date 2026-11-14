//! The Velopack side of the installer (docs/INSTALLER.md): `main.cpp`
//! calls [`yap_velopack_startup`] before anything else. Installed, Velopack
//! runs the executable with `--veloapp-install`, `--veloapp-updated` or
//! `--veloapp-uninstall`; the matching hook below runs and the process exits
//! (Velopack's "fast callbacks": 15 to 30 s, then it is killed). Any other
//! launch returns at once and the app starts.
//!
//! The hooks only touch values that belong to this install root; the rules
//! are `yap_core::install`. User data (`%LOCALAPPDATA%\Yap.Data`) is never
//! removed, apart from the default profile's toast icon.
//!
//! Also here, for the C++ shell: the data folder (moved from its old place
//! once, `yap_profile_data_root`), the Run value rule
//! (`yap_run_value_may_change`) and, on the released install's first
//! launches, the replacement of the Tauri desktop app
//! (`yap_replace_tauri_at_launch`, then `yap_tauri_app_started`,
//! `yap_core::tauri`).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use velopack::VelopackApp;
use yap_core::install::{self, Registry};
use yap_core::tauri;

/// On the run Setup starts after installing: exit instead of starting.
const NO_LAUNCH_ARG: &str = "--no-launch";

/// Runs Velopack's startup handling. Returns unless this launch was one of
/// Velopack's hooks (then the process exits) or Velopack restarts the app
/// into a downloaded update. Never unwinds into C++.
#[no_mangle]
pub extern "C" fn yap_velopack_startup() {
    let run = || {
        VelopackApp::build()
            .on_after_install_fast_callback(|version| {
                if let Some(install) = Install::current() {
                    install.log(&format!("after install {version}"));
                    install.after_install();
                }
            })
            // The exe path under `current` is stable across updates, so the
            // Run values stay valid; the schemes are written again in case
            // a dev build took them since the install.
            .on_after_update_fast_callback(|version| {
                if let Some(install) = Install::current() {
                    install.log(&format!("after update {version}"));
                    install.register_schemes();
                }
            })
            .on_before_uninstall_fast_callback(|version| {
                if let Some(install) = Install::current() {
                    install.log(&format!("before uninstall {version}"));
                    install.before_uninstall();
                }
            })
            // Setup always starts the app when it is done. The old
            // installer's /NoLaunch is `Setup.exe --silent -- --no-launch`:
            // Setup passes what follows `--` to this first run only.
            .on_first_run(|_| {
                if std::env::args().skip(1).any(|a| a == NO_LAUNCH_ARG) {
                    std::process::exit(0);
                }
            })
            // A downloaded update installs here only for a copy that
            // updates, with install on quit on and no pin
            // (`yap_core::update::apply_at_start`); otherwise it waits for
            // "Restart now" (src/updater.rs).
            .set_auto_apply_on_startup(crate::updater::apply_at_start())
            .run();
    };
    if std::panic::catch_unwind(run).is_err() {
        eprintln!("[installer] Velopack startup panicked; starting anyway");
    }
}

/// This executable as Velopack installed it: `<root>\current\Yap.exe` next
/// to `<root>\Update.exe`.
struct Install {
    exe: String,
    root: String,
}

impl Install {
    /// `None` unless the layout is Velopack's, so a hook argument on a dev
    /// build's command line changes nothing.
    fn current() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let exe = exe.to_str()?.to_owned();
        match install::velopack_root(&exe, |p| Path::new(p).is_file()) {
            Some(root) => Some(Self { exe, root }),
            None => {
                eprintln!("[installer] not a Velopack install: {exe}");
                None
            }
        }
    }

    /// `yap://` and `yap-native://`, pointing at `current\Yap.exe` (stable
    /// across updates): always for the released install, which owns them
    /// (and so takes them back from a dev build, and `yap://` from the
    /// Tauri app); for a test install only unless another live copy has
    /// them. The released app also rewrites them on every default-profile
    /// start.
    fn register_schemes(&self) {
        let mut reg = Hkcu;
        for line in
            install::register_schemes(&mut reg, &self.root, &self.exe, |p| Path::new(p).exists())
        {
            self.log(&line);
        }
    }

    fn after_install(&self) {
        self.register_schemes();
        // The toast identity, marked as ours so uninstall can tell. A key
        // that is already there (a dev build's) is left as it is; the app
        // sets the icon on its first start.
        let line = install::register_aumid(&mut Hkcu, &self.root);
        self.log(&line);
        // The Tauri app is replaced on the first launch, not here: its
        // uninstaller can take longer than Velopack gives a hook.
    }

    fn before_uninstall(&self) {
        let out = install::unregister(&mut Hkcu, &self.root);
        for line in &out.log {
            self.log(line);
        }
        if out.aumid_removed {
            if let Some(icon) = data_root().map(|d| d.join("toast-icon.png")) {
                match std::fs::remove_file(&icon) {
                    Ok(()) => self.log("toast icon: removed"),
                    Err(e) => self.log(&format!("toast icon: {e}")),
                }
            }
        }
    }

    fn log(&self, line: &str) {
        log_line(&format!("[{}] {line}", self.root));
    }
}

/// One line in `%LOCALAPPDATA%\Yap.Data\logs\installer.log`, next to the
/// app's own logs (Velopack keeps its own in `%LOCALAPPDATA%\velopack`).
fn log_line(line: &str) {
    let Some(dir) = data_root().map(|d| d.join("logs")) else {
        return;
    };
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let _ = std::fs::create_dir_all(&dir);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("installer.log"))
    {
        let _ = writeln!(file, "{secs} {line}");
    }
}

/// The default profile's data root; the hooks never run as a profile and
/// never move the old folder (the app's first start does).
fn data_root() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(yap_cache::profile::data_root_in(Path::new(&base), None))
}

// For the C++ shell

/// Reads a NUL-terminated UTF-16 string, `None` for a null pointer.
///
/// # Safety
/// `text` is null or points at a NUL-terminated UTF-16 string.
unsafe fn from_wide_ptr(text: *const u16) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let mut len = 0;
    // SAFETY: the caller promises a terminator.
    while unsafe { *text.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` units were just read.
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(text, len)
    }))
}

/// This process's data folder (`yap_cache::profile::data_root`: moved from
/// `%LOCALAPPDATA%\YapNative` once, read there while it cannot move) into
/// `out` as UTF-16 with a terminator. Returns its length without the
/// terminator; 0 when there is none or `capacity` is too small. The C++
/// shell (`profile::dataRoot`) asks here, so both halves agree on it.
///
/// # Safety
/// `out` points at `capacity` writable `u16`s.
#[no_mangle]
pub unsafe extern "C" fn yap_profile_data_root(out: *mut u16, capacity: usize) -> usize {
    let Ok(Some(root)) = std::panic::catch_unwind(yap_cache::profile::data_root) else {
        return 0;
    };
    let wide: Vec<u16> = root.as_os_str().to_string_lossy().encode_utf16().collect();
    if out.is_null() || wide.len() + 1 > capacity {
        return 0;
    }
    // SAFETY: `capacity` units are writable and `wide.len() + 1` fit.
    unsafe {
        std::ptr::copy_nonoverlapping(wide.as_ptr(), out, wide.len());
        *out.add(wide.len()) = 0;
    }
    wide.len()
}

/// Whether the shell may write (`enable`) or delete this profile's Run
/// value, whose command is now `current` (null: none), for the executable
/// `exe` (`install::may_write_run_value`, `install::may_delete_run_value`):
/// a dev build never takes or removes another app's autostart, the Tauri
/// app's `Yap` above all; the released install takes `Yap` over.
///
/// # Safety
/// `current` is null or a NUL-terminated UTF-16 string; so is `exe`.
#[no_mangle]
pub unsafe extern "C" fn yap_run_value_may_change(
    current: *const u16,
    exe: *const u16,
    enable: bool,
) -> bool {
    // SAFETY: as the caller promises.
    let (current, exe) = unsafe { (from_wide_ptr(current), from_wide_ptr(exe)) };
    let Some(exe) = exe else {
        return false;
    };
    if enable {
        let released = install::is_released_install(&exe, |p| Path::new(p).is_file());
        install::may_write_run_value(current.as_deref(), &exe, released, |p| {
            Path::new(p).exists()
        })
    } else {
        current.is_none_or(|c| install::may_delete_run_value(&c, &exe))
    }
}

/// The record of the replacement in the data folder, so a failing
/// uninstaller is not run forever.
const REPLACE_STATE: &str = "tauri-replace.json";

/// How often the uninstaller is tried before the app stops trying.
const MAX_UNINSTALL_ATTEMPTS: u64 = 3;

/// How long the uninstaller gets. It waits up to 30 s for the Tauri app to
/// exit (killing it after 5 s) before it deletes anything.
const UNINSTALL_WAIT: Duration = Duration::from_secs(120);

/// On the released install's default-profile launches, before the settings
/// load: imports the Tauri app's preferences once (synchronously, a few
/// file reads), then, if the Tauri app is still installed, arms its
/// uninstall, which runs only after `yap_tauri_app_started` and
/// `tauri::UNINSTALL_DEFER` more (so a launch that crashes early leaves the
/// Tauri app working). Anything else (a dev build, a copy, a test install,
/// a named profile) does nothing. Logged to `installer.log`. Never unwinds
/// into C++.
#[no_mangle]
pub extern "C" fn yap_replace_tauri_at_launch() {
    let run = || {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let exe = exe.to_string_lossy().into_owned();
        if yap_cache::profile::current().is_some()
            || !install::is_released_install(&exe, |p| Path::new(p).is_file())
        {
            return;
        }
        let Some(root) = install::velopack_root(&exe, |p| Path::new(p).is_file()) else {
            return;
        };
        let (Some(paths), Some(data)) = (known_paths(), yap_cache::profile::data_root()) else {
            log_line("[tauri] no known folders; nothing replaced");
            return;
        };
        remove_legacy_run_values(&paths);
        let imported = tauri::import(&paths, &data, &monitor_scale);
        for line in &imported.log {
            log_line(&format!("[tauri] {line}"));
        }
        // Shortcuts a run that ended mid-uninstall saved: put back the ones
        // that are gone.
        let saved_dir = data.join(tauri::SAVED_SHORTCUTS_DIR);
        let saved = tauri::load_persisted_shortcuts(&saved_dir);
        if !saved.is_empty() {
            for line in tauri::restore_shortcuts(&saved) {
                log_line(&format!("[tauri] {line}"));
            }
        }
        if tauri::detect(&Hkcu, &paths).is_none() {
            let _ = std::fs::remove_dir_all(&saved_dir);
            // Nothing installed; clear what may still point at its root.
            for line in
                tauri::remove_leftovers(&mut Hkcu, &paths.local_app_data.join(tauri::INSTALL_DIR))
            {
                log_line(&format!("[tauri] {line}"));
            }
            return;
        }
        let attempts = read_attempts(&data);
        if attempts >= MAX_UNINSTALL_ATTEMPTS {
            log_line(&format!(
                "[tauri] still installed after {attempts} uninstall attempts; not trying again"
            ));
            return;
        }
        // Not now: only once this launch has stayed up
        // (`yap_tauri_app_started`, `tauri::UninstallDeferral`).
        let mut deferral = tauri::UninstallDeferral::default();
        deferral.arm();
        *PENDING_REPLACE.lock().unwrap_or_else(|e| e.into_inner()) = Some(PendingReplace {
            paths,
            root,
            exe,
            saved_dir,
            data,
            deferral,
        });
        log_line(&format!(
            "[tauri] uninstall deferred until the app has been up {} s",
            tauri::UNINSTALL_DEFER.as_secs()
        ));
    };
    if std::panic::catch_unwind(run).is_err() {
        eprintln!("[tauri] replacement check panicked");
    }
}

/// The uninstall `yap_replace_tauri_at_launch` found to do, waiting for
/// `yap_tauri_app_started`.
struct PendingReplace {
    paths: tauri::Paths,
    root: String,
    exe: String,
    saved_dir: PathBuf,
    data: PathBuf,
    deferral: tauri::UninstallDeferral,
}

static PENDING_REPLACE: std::sync::Mutex<Option<PendingReplace>> = std::sync::Mutex::new(None);

/// The shell calls this once Main.qml has its window (shown, or in the tray
/// on a minimized start). If the launch found the Tauri app to replace, a
/// thread waits [`tauri::UNINSTALL_DEFER`] more, then counts the attempt and
/// runs the uninstaller. A launch that crashes before then leaves the Tauri
/// app as it was (the Tauri 3.14 bridge counts that as a failed handoff and
/// keeps working). Never unwinds into C++.
#[no_mangle]
pub extern "C" fn yap_tauri_app_started() {
    let run = || {
        let Some(mut pending) = PENDING_REPLACE
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
        else {
            eprintln!("[tauri] app started: no Tauri replacement armed");
            return;
        };
        eprintln!(
            "[tauri] app started: Tauri uninstall in {} s",
            tauri::UNINSTALL_DEFER.as_secs()
        );
        log_line(&format!(
            "[tauri] app started; uninstalling in {} s",
            tauri::UNINSTALL_DEFER.as_secs()
        ));
        let clock = Instant::now();
        pending.deferral.app_started(clock.elapsed());
        let _ = std::thread::Builder::new()
            .name("tauri-replace".into())
            .spawn(move || {
                loop {
                    if pending.deferral.due(clock.elapsed()) {
                        break;
                    }
                    let Some(wait) = pending.deferral.wait(clock.elapsed()) else {
                        return;
                    };
                    std::thread::sleep(wait.max(Duration::from_millis(100)));
                }
                let PendingReplace {
                    paths,
                    root,
                    exe,
                    saved_dir,
                    data,
                    ..
                } = pending;
                write_attempts(&data, read_attempts(&data) + 1);
                let outcome = std::panic::catch_unwind(|| replace(&paths, &root, &exe, &saved_dir));
                if outcome.is_err() {
                    log_line("[tauri] replacement panicked");
                }
            });
    };
    if std::panic::catch_unwind(run).is_err() {
        eprintln!("[tauri] deferred replacement panicked");
    }
}

/// Uninstalls the Tauri app with its own uninstaller, keeps our shortcuts,
/// clears its leftovers and takes `yap://` back.
fn replace(paths: &tauri::Paths, root: &str, exe: &str, saved_dir: &Path) {
    let mut reg = Hkcu;
    let Some(found) = tauri::detect(&reg, paths) else {
        return;
    };
    log_line(&format!(
        "[tauri] found {} (version {}), uninstalling",
        found.root.display(),
        found.version.as_deref().unwrap_or("?")
    ));
    // On disk before the uninstaller runs: if this process ends first, the
    // next launch puts them back.
    let saved = tauri::save_shortcuts(paths, Path::new(root));
    if let Err(e) = tauri::persist_shortcuts(&saved, saved_dir) {
        log_line(&format!(
            "[tauri] shortcuts not saved ({e}); not uninstalling now"
        ));
        return;
    }
    match run_uninstaller(&found.uninstaller, UNINSTALL_WAIT) {
        Ok(Some(code)) => log_line(&format!("[tauri] uninstaller exited with {code}")),
        Ok(None) => log_line(&format!(
            "[tauri] uninstaller still running after {} s; left to finish",
            UNINSTALL_WAIT.as_secs()
        )),
        Err(e) => log_line(&format!("[tauri] uninstaller did not start: {e}")),
    }
    for line in tauri::restore_shortcuts(&saved) {
        log_line(&format!("[tauri] {line}"));
    }
    for line in tauri::remove_leftovers(&mut reg, &found.root) {
        log_line(&format!("[tauri] {line}"));
    }
    for line in install::register_schemes(&mut reg, root, exe, |p| Path::new(p).exists()) {
        log_line(&format!("[tauri] {line}"));
    }
    let gone = reg.read(tauri::UNINSTALL_KEY, "UninstallString").is_none();
    log_line(&format!("[tauri] uninstall entry gone: {gone}"));
    if gone {
        let _ = std::fs::remove_dir_all(saved_dir);
    }
}

/// The `YapNative[-<profile>]` Run values of the install from before the
/// rename (`%LOCALAPPDATA%\YapNative.App`), so that build no longer starts
/// at sign-in beside this one. Its install itself is left alone.
fn remove_legacy_run_values(paths: &tauri::Paths) {
    let legacy_root = paths.local_app_data.join(install::LEGACY_PACK_ID);
    let mut reg = Hkcu;
    let values = reg.values(install::RUN_KEY);
    for name in install::legacy_run_values(&values, &legacy_root.to_string_lossy()) {
        let ok = reg.delete_value(install::RUN_KEY, &name);
        log_line(&format!("[legacy] run value {name}: removed ({ok})"));
    }
}

/// Runs `<root>\Update.exe --uninstall /S` without a window and waits up to
/// `wait`. `Ok(None)`: still running then.
fn run_uninstaller(uninstaller: &Path, wait: Duration) -> std::io::Result<Option<i32>> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut child = std::process::Command::new(uninstaller)
        .args(tauri::UNINSTALL_ARGS)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()?;
    let start = Instant::now();
    while start.elapsed() < wait {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Ok(None)
}

fn read_attempts(data: &Path) -> u64 {
    std::fs::read_to_string(data.join(REPLACE_STATE))
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| {
            v.get("uninstallAttempts")
                .and_then(serde_json::Value::as_u64)
        })
        .unwrap_or(0)
}

fn write_attempts(data: &Path, attempts: u64) {
    let _ = std::fs::create_dir_all(data);
    let _ = std::fs::write(
        data.join(REPLACE_STATE),
        serde_json::json!({ "uninstallAttempts": attempts }).to_string(),
    );
}

/// The scale (1.0 at 96 DPI) of the monitor a physical-pixel rect is on,
/// for the window rect import; `None` when it is on no monitor now. The
/// process is per-monitor aware (cpp/app.manifest), so the rect and the
/// DPI are both physical.
fn monitor_scale(x: i32, y: i32, width: i32, height: i32) -> Option<f64> {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{MonitorFromRect, MONITOR_DEFAULTTONULL};
    use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
    let rect = RECT {
        left: x,
        top: y,
        right: x.saturating_add(width),
        bottom: y.saturating_add(height),
    };
    // SAFETY: a valid rect; the handle is not owned.
    let monitor = unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) };
    if monitor.is_null() {
        return None;
    }
    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    // SAFETY: a monitor handle from above and valid out pointers.
    let status = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
    (status == 0 && dpi_x > 0).then(|| f64::from(dpi_x) / 96.0)
}

/// `%LOCALAPPDATA%`, `%APPDATA%`, the desktop and the Start menu's
/// Programs, from the shell's known folders (OneDrive moves the desktop).
fn known_paths() -> Option<tauri::Paths> {
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_LocalAppData, FOLDERID_Programs, FOLDERID_RoamingAppData,
    };
    Some(tauri::Paths {
        local_app_data: known_folder(&FOLDERID_LocalAppData)?,
        roaming_app_data: known_folder(&FOLDERID_RoamingAppData)?,
        desktop: known_folder(&FOLDERID_Desktop)?,
        programs: known_folder(&FOLDERID_Programs)?,
    })
}

fn known_folder(id: &windows_sys::core::GUID) -> Option<PathBuf> {
    use windows_sys::Win32::Foundation::S_OK;
    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderPath;
    let mut path: windows_sys::core::PWSTR = std::ptr::null_mut();
    // SAFETY: `id` is a valid GUID; `path` receives a CoTaskMem string.
    let status = unsafe { SHGetKnownFolderPath(id, 0, std::ptr::null_mut(), &mut path) };
    // SAFETY: on success `path` is a NUL-terminated string.
    let text = (status == S_OK)
        .then(|| unsafe { from_wide_ptr(path) })
        .flatten();
    // SAFETY: freed once, also on failure (it may be set).
    unsafe { CoTaskMemFree(path.cast()) };
    text.map(PathBuf::from)
}

/// The real per-user registry (`HKEY_CURRENT_USER`), for the hooks and the
/// Tauri replacement. Tests use `install::MemoryRegistry` instead.
struct Hkcu;

impl Registry for Hkcu {
    fn read(&self, key: &str, name: &str) -> Option<String> {
        reg::read(key, name)
    }
    fn key_exists(&self, key: &str) -> bool {
        reg::key_exists(key)
    }
    fn values(&self, key: &str) -> Vec<(String, String)> {
        reg::values(key)
    }
    fn write(&mut self, key: &str, name: &str, data: &str) -> bool {
        reg::write(key, name, data)
    }
    fn delete_value(&mut self, key: &str, name: &str) -> bool {
        reg::delete_value(key, name)
    }
    fn delete_tree(&mut self, key: &str) -> bool {
        reg::delete_tree(key)
    }
}

/// `HKEY_CURRENT_USER` string values, the little the hooks need.
mod reg {
    use std::ptr;

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegEnumValueW, RegGetValueW,
        RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE,
        REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn from_wide(buffer: &[u16]) -> String {
        let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
        String::from_utf16_lossy(&buffer[..end])
    }

    /// An open key, closed on drop.
    struct Key(HKEY);

    impl Key {
        fn open(path: &str, access: u32) -> Option<Self> {
            let path = wide(path);
            let mut key: HKEY = ptr::null_mut();
            // SAFETY: `path` is NUL-terminated and outlives the call; `key`
            // is a valid out pointer.
            let status =
                unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) };
            (status == ERROR_SUCCESS).then_some(Self(key))
        }
    }

    impl Drop for Key {
        fn drop(&mut self) {
            // SAFETY: the handle came from a successful open or create.
            unsafe { RegCloseKey(self.0) };
        }
    }

    pub fn key_exists(path: &str) -> bool {
        Key::open(path, KEY_QUERY_VALUE).is_some()
    }

    /// The `REG_SZ` value `name` (empty: the default value) of `path`.
    pub fn read(path: &str, name: &str) -> Option<String> {
        let path = wide(path);
        let name = wide(name);
        let mut size: u32 = 0;
        // SAFETY: NUL-terminated strings; a null buffer asks for the size.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; size as usize / 2 + 1];
        let mut size = (buffer.len() * 2) as u32;
        // SAFETY: `buffer` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                name.as_ptr(),
                RRF_RT_REG_SZ,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then(|| from_wide(&buffer))
    }

    /// Writes a `REG_SZ`, creating the key.
    pub fn write(path: &str, name: &str, data: &str) -> bool {
        let wpath = wide(path);
        let mut key: HKEY = ptr::null_mut();
        // SAFETY: NUL-terminated path, valid out pointer.
        let status = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                wpath.as_ptr(),
                0,
                ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                ptr::null(),
                &mut key,
                ptr::null_mut(),
            )
        };
        if status != ERROR_SUCCESS {
            return false;
        }
        let key = Key(key);
        let name = wide(name);
        let data = wide(data);
        // SAFETY: `data` is `data.len() * 2` bytes, NUL included.
        let status = unsafe {
            RegSetValueExW(
                key.0,
                if name.len() == 1 {
                    ptr::null()
                } else {
                    name.as_ptr()
                },
                0,
                REG_SZ,
                data.as_ptr().cast(),
                (data.len() * 2) as u32,
            )
        };
        status == ERROR_SUCCESS
    }

    /// Deletes `path` with its values and subkeys; a missing key is fine.
    pub fn delete_tree(path: &str) -> bool {
        let path = wide(path);
        // SAFETY: NUL-terminated path.
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, path.as_ptr()) };
        status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND
    }

    pub fn delete_value(path: &str, name: &str) -> bool {
        let Some(key) = Key::open(path, KEY_SET_VALUE) else {
            return false;
        };
        let name = wide(name);
        // SAFETY: open key, NUL-terminated name.
        let status = unsafe { RegDeleteValueW(key.0, name.as_ptr()) };
        status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND
    }

    /// Every `REG_SZ` value of `path` as (name, data).
    pub fn values(path: &str) -> Vec<(String, String)> {
        let Some(key) = Key::open(path, KEY_QUERY_VALUE) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut name = vec![0u16; 16_384];
        let mut data = vec![0u16; 8_192];
        for index in 0.. {
            let mut name_len = name.len() as u32;
            let mut data_len = (data.len() * 2) as u32;
            let mut kind: u32 = 0;
            // SAFETY: the buffers hold the lengths passed.
            let status = unsafe {
                RegEnumValueW(
                    key.0,
                    index,
                    name.as_mut_ptr(),
                    &mut name_len,
                    ptr::null(),
                    &mut kind,
                    data.as_mut_ptr().cast(),
                    &mut data_len,
                )
            };
            if status != ERROR_SUCCESS {
                // ERROR_NO_MORE_ITEMS, or a value too long to be ours.
                if status == windows_sys::Win32::Foundation::ERROR_MORE_DATA {
                    continue;
                }
                break;
            }
            if kind == REG_SZ {
                let len = (data_len as usize / 2).min(data.len());
                out.push((
                    String::from_utf16_lossy(&name[..name_len as usize]),
                    from_wide(&data[..len]),
                ));
            }
        }
        out
    }
}
