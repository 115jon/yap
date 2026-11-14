//! Steam's own word for what runs: `HKCU\Software\Valve\Steam`
//! `RunningAppID` (0 when nothing), watched with `RegNotifyChangeKeyValue`.
//! An app the catalog does not know is named from its
//! `steamapps\appmanifest_<id>.acf`.

use std::path::PathBuf;

use windows_sys::Win32::Foundation::{ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER,
    KEY_NOTIFY, KEY_QUERY_VALUE, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
};
use windows_sys::Win32::System::Threading::CreateEventW;

use crate::vdf::{steam_libraries, vdf_values};
use crate::win::Owned;

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The open Steam key and the event its changes set.
pub(crate) struct SteamWatch {
    key: HKEY,
    event: Owned,
}

// SAFETY: the key and the event are used from the detector thread only,
// which also created them.
unsafe impl Send for SteamWatch {}

impl SteamWatch {
    /// `None` when Steam was never installed for this user.
    pub(crate) fn open() -> Option<Self> {
        let path = wide(r"Software\Valve\Steam");
        let mut key: HKEY = std::ptr::null_mut();
        // SAFETY: the key is closed on drop; the event by `Owned`.
        unsafe {
            if RegOpenKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_NOTIFY,
                &mut key,
            ) != ERROR_SUCCESS
            {
                return None;
            }
            let event = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
            if event.is_null() {
                RegCloseKey(key);
                return None;
            }
            let watch = Self {
                key,
                event: Owned(event),
            };
            watch.arm();
            Some(watch)
        }
    }

    /// Asks for the next change (each notification fires once).
    pub(crate) fn arm(&self) {
        // SAFETY: the key and the event are open for `self`'s life.
        unsafe {
            RegNotifyChangeKeyValue(self.key, 0, REG_NOTIFY_CHANGE_LAST_SET, self.event.0, 1);
        }
    }

    pub(crate) fn event(&self) -> HANDLE {
        self.event.0
    }

    /// The running Steam app, or 0.
    pub(crate) fn running_app(&self) -> u32 {
        let name = wide("RunningAppID");
        let mut value = 0u32;
        let mut size = std::mem::size_of::<u32>() as u32;
        // SAFETY: the out buffer is a u32 and its size is passed.
        let status = unsafe {
            RegGetValueW(
                self.key,
                std::ptr::null(),
                name.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                std::ptr::from_mut(&mut value).cast(),
                &mut size,
            )
        };
        if status == ERROR_SUCCESS {
            value
        } else {
            0
        }
    }

    /// Steam's install folder (`SteamPath`).
    fn steam_path(&self) -> Option<PathBuf> {
        read_string(self.key, "SteamPath").map(PathBuf::from)
    }

    /// An app's name from its manifest, in any Steam library.
    pub(crate) fn manifest_name(&self, app: u32) -> Option<String> {
        let steam = self.steam_path()?;
        steam_libraries(&steam).iter().find_map(|library| {
            let text =
                std::fs::read_to_string(library.join(format!("appmanifest_{app}.acf"))).ok()?;
            let name = vdf_values(&text, "name").next();
            name
        })
    }
}

impl Drop for SteamWatch {
    fn drop(&mut self) {
        // SAFETY: opened in `open`, closed once.
        unsafe { RegCloseKey(self.key) };
    }
}

fn read_string(key: HKEY, value: &str) -> Option<String> {
    let name = wide(value);
    let mut buffer = vec![0u16; 1024];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer and its byte size are passed.
    let status = unsafe {
        RegGetValueW(
            key,
            std::ptr::null(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let len = (size as usize / 2).min(buffer.len());
    let end = buffer[..len].iter().position(|&c| c == 0).unwrap_or(len);
    Some(String::from_utf16_lossy(&buffer[..end]))
}

/// Steam's install folder (`HKCU\Software\Valve\Steam` `SteamPath`), read
/// once without watching.
pub(crate) fn steam_path() -> Option<PathBuf> {
    let path = wide(r"Software\Valve\Steam");
    let mut key: HKEY = std::ptr::null_mut();
    // SAFETY: the key is opened read-only and closed below.
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_QUERY_VALUE, &mut key)
            != ERROR_SUCCESS
        {
            return None;
        }
        let value = read_string(key, "SteamPath");
        RegCloseKey(key);
        value.map(PathBuf::from)
    }
}
