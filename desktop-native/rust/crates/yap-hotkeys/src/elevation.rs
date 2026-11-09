//! Elevated windows. A hook in a process that is not elevated sees no
//! input sent to an elevated window (UIPI), so keybinds go quiet while a
//! game run as administrator has the focus. We never elevate; we notice
//! and say so.

use std::path::Path;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ACCESS_DENIED, HANDLE, HWND};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// A foreground window's process, as far as we may look.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Foreground {
    /// Its exe's file name (`game.exe`), or empty when it cannot be read.
    pub exe: String,
    /// Runs elevated: its token says so, or we may not read the token (an
    /// administrator's process denies a standard one that).
    pub elevated: bool,
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: a handle we opened and own.
        unsafe {
            CloseHandle(self.0);
        }
    }
}

/// The token's elevation, or the error that stopped us asking.
fn token_elevated(process: HANDLE) -> Result<bool, u32> {
    let mut token: HANDLE = std::ptr::null_mut();
    // SAFETY: an out pointer to a local.
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        // SAFETY: no arguments.
        return Err(unsafe { GetLastError() });
    }
    let token = Handle(token);
    let mut elevation = TOKEN_ELEVATION::default();
    let mut size = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION of the size given.
    let ok = unsafe {
        GetTokenInformation(
            token.0,
            TokenElevation,
            std::ptr::from_mut(&mut elevation).cast(),
            u32::try_from(size_of::<TOKEN_ELEVATION>()).unwrap_or(4),
            &mut size,
        )
    };
    if ok == 0 {
        // SAFETY: no arguments.
        return Err(unsafe { GetLastError() });
    }
    Ok(elevation.TokenIsElevated != 0)
}

/// This process runs elevated: then nothing is hidden from its hook.
pub fn self_elevated() -> bool {
    // SAFETY: the pseudo handle needs no closing.
    token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false)
}

/// The process behind `hwnd`; `None` for our own windows or one whose
/// process cannot be opened at all (gone, or protected).
pub fn foreground(hwnd: isize) -> Option<Foreground> {
    let mut pid = 0u32;
    // SAFETY: an out pointer to a local; a stale HWND just gives 0.
    unsafe { GetWindowThreadProcessId(hwnd as HWND, &mut pid) };
    // SAFETY: no arguments.
    if pid == 0 || pid == unsafe { GetCurrentProcessId() } {
        return None;
    }
    // SAFETY: plain call; the handle is closed by `Handle`.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let process = Handle(process);
    let mut buffer = [0u16; 1024];
    let mut len = u32::try_from(buffer.len()).unwrap_or(0);
    // SAFETY: the buffer and its length in characters.
    let exe = if unsafe {
        QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
    } != 0
    {
        let path = String::from_utf16_lossy(&buffer[..len as usize]);
        Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let elevated = match token_elevated(process.0) {
        Ok(elevated) => elevated,
        Err(code) => code == ERROR_ACCESS_DENIED,
    };
    Some(Foreground { exe, elevated })
}
