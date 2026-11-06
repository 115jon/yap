//! The Velopack side of the installer (docs/INSTALLER.md): `main.cpp`
//! calls [`yap_velopack_startup`] before anything else. Installed, Velopack
//! runs the executable with `--veloapp-install`, `--veloapp-updated` or
//! `--veloapp-uninstall`; the matching hook below runs and the process exits
//! (Velopack's "fast callbacks": 15 to 30 s, then it is killed). Any other
//! launch returns at once and the app starts.
//!
//! The hooks only touch values that belong to this install root; the rules
//! are `yap_core::install`. User data (`%LOCALAPPDATA%\YapNative`) is never
//! removed, apart from the default profile's toast icon.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use velopack::VelopackApp;
use yap_core::install::{
    command_is_ours, is_run_value, may_take_scheme, same_path, AUMID_DISPLAY_NAME, AUMID_KEY,
    AUMID_OWNER_VALUE, RUN_KEY,
};
use yap_core::links::{scheme_registration, SCHEME_KEY};

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
            // Nothing changes between versions yet: the exe path under
            // `current` is stable, so the scheme and Run values stay valid.
            .on_after_update_fast_callback(|version| {
                if let Some(install) = Install::current() {
                    install.log(&format!("after update {version}"));
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
            .run();
    };
    if std::panic::catch_unwind(run).is_err() {
        eprintln!("[installer] Velopack startup panicked; starting anyway");
    }
}

/// This executable as Velopack installed it: `<root>\current\YapNative.exe`
/// next to `<root>\Update.exe`.
struct Install {
    exe: String,
    root: String,
}

impl Install {
    /// `None` unless the layout is Velopack's, so a hook argument on a dev
    /// build's command line changes nothing.
    fn current() -> Option<Self> {
        let exe = std::env::current_exe().ok()?;
        let current = exe.parent()?;
        let root = current.parent()?;
        let is_current = current
            .file_name()
            .is_some_and(|n| n.eq_ignore_ascii_case("current"));
        if !is_current || !root.join("Update.exe").is_file() {
            eprintln!("[installer] not a Velopack install: {}", exe.display());
            return None;
        }
        Some(Self {
            exe: exe.to_str()?.to_owned(),
            root: root.to_str()?.to_owned(),
        })
    }

    fn after_install(&self) {
        // The scheme is the default profile's; the app rewrites it on every
        // default-profile start. Here it is registered before the first run
        // unless another live copy owns it.
        let current = reg::read(&scheme_command_key(), "");
        if may_take_scheme(current.as_deref(), &self.root, |p| Path::new(p).exists()) {
            let written = scheme_registration(&self.exe)
                .iter()
                .filter(|v| reg::write(&v.key, &v.name, &v.data))
                .count();
            self.log(&format!("scheme: {written} value(s) written"));
        } else {
            self.log("scheme: owned by another copy, left alone");
        }
        // The toast identity, marked as ours so uninstall can tell. A key
        // that is already there (a dev build's) is left as it is; the app
        // sets the icon on its first start.
        if !reg::key_exists(AUMID_KEY) {
            let ok = reg::write(AUMID_KEY, "DisplayName", AUMID_DISPLAY_NAME)
                && reg::write(AUMID_KEY, AUMID_OWNER_VALUE, &self.root);
            self.log(&format!("aumid: created ({ok})"));
        } else {
            self.log("aumid: already registered, left alone");
        }
    }

    fn before_uninstall(&self) {
        match reg::read(&scheme_command_key(), "") {
            Some(command) if command_is_ours(&command, &self.root) => {
                let ok = reg::delete_tree(SCHEME_KEY);
                self.log(&format!("scheme: removed ({ok})"));
            }
            Some(_) => self.log("scheme: owned by another copy, kept"),
            None => self.log("scheme: not registered"),
        }

        let owner = reg::read(AUMID_KEY, AUMID_OWNER_VALUE);
        if owner.is_some_and(|o| same_path(&o, &self.root)) {
            let ok = reg::delete_tree(AUMID_KEY);
            self.log(&format!("aumid: removed ({ok})"));
            if let Some(icon) = data_root().map(|d| d.join("toast-icon.png")) {
                match std::fs::remove_file(&icon) {
                    Ok(()) => self.log("toast icon: removed"),
                    Err(e) => self.log(&format!("toast icon: {e}")),
                }
            }
        } else {
            self.log("aumid: not created by this install, kept");
        }

        for (name, command) in reg::values(RUN_KEY) {
            if is_run_value(&name) && command_is_ours(&command, &self.root) {
                let ok = reg::delete_value(RUN_KEY, &name);
                self.log(&format!("run value {name}: removed ({ok})"));
            }
        }
    }

    /// One line in `%LOCALAPPDATA%\YapNative\logs\installer.log`, next to the
    /// app's own logs (Velopack keeps its own in `%LOCALAPPDATA%\velopack`).
    fn log(&self, line: &str) {
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
            let _ = writeln!(file, "{secs} [{}] {line}", self.root);
        }
    }
}

fn scheme_command_key() -> String {
    format!(r"{SCHEME_KEY}\shell\open\command")
}

/// The default profile's data root; the hooks never run as a profile.
fn data_root() -> Option<PathBuf> {
    let base = std::env::var_os("LOCALAPPDATA")?;
    Some(yap_cache::profile::data_root_in(Path::new(&base), None))
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
