//! The Windows calls the detector makes. Every process call is
//! `PROCESS_QUERY_LIMITED_INFORMATION | SYNCHRONIZE` at most; nothing reads
//! another process's memory or command line.

use std::ffi::c_void;

use windows_sys::Win32::Foundation::WAIT_OBJECT_0;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, WaitForSingleObject,
    PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetForegroundWindow, GetWindow, GetWindowLongPtrW, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GWL_EXSTYLE, GW_OWNER, WS_EX_TOOLWINDOW,
};

/// A handle closed on drop.
pub(crate) struct Owned(pub HANDLE);

// SAFETY: a process or event handle may be used and closed from any thread.
unsafe impl Send for Owned {}

impl Drop for Owned {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            // SAFETY: the handle is ours and closed once.
            unsafe { CloseHandle(self.0) };
        }
    }
}

fn wide(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..end])
}

/// Every process: (pid, executable file name). Opens no process.
pub(crate) fn snapshot() -> Vec<(u32, String)> {
    let mut out = Vec::with_capacity(400);
    // SAFETY: the snapshot handle is closed by `Owned`; the entry carries
    // its size as the calls require.
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE || snap.is_null() {
            return out;
        }
        let snap = Owned(snap);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap.0, &mut entry) != 0;
        while ok {
            out.push((entry.th32ProcessID, wide(&entry.szExeFile)));
            ok = Process32NextW(snap.0, &mut entry) != 0;
        }
    }
    out
}

impl Owned {
    /// The process has exited (its handle is signaled). An exited process
    /// stays listed while anyone, us included, holds a handle to it.
    pub(crate) fn exited(&self) -> bool {
        // SAFETY: a zero timeout only polls the handle.
        unsafe { WaitForSingleObject(self.0, 0) == WAIT_OBJECT_0 }
    }
}

/// A process opened to read its image path and wait for its exit; either
/// may be missing (an anti-cheat or an elevated process refuses).
pub(crate) struct Opened {
    pub path: Option<String>,
    pub wait: Option<Owned>,
}

/// Opens `pid` with `QUERY_LIMITED_INFORMATION | SYNCHRONIZE`, or
/// `SYNCHRONIZE` alone when that is refused.
pub(crate) fn open(pid: u32) -> Opened {
    // SAFETY: handles are owned by `Owned`; the buffer length is passed.
    unsafe {
        let handle = OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
            0,
            pid,
        );
        if handle.is_null() {
            let wait = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            return Opened {
                path: None,
                wait: (!wait.is_null()).then_some(Owned(wait)),
            };
        }
        let handle = Owned(handle);
        let mut buffer = vec![0u16; 1024];
        let mut len = buffer.len() as u32;
        let path = (QueryFullProcessImageNameW(
            handle.0,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut len,
        ) != 0)
            .then(|| String::from_utf16_lossy(&buffer[..len as usize]));
        Opened {
            path,
            wait: Some(handle),
        }
    }
}

/// `%WINDIR%`, lower case with `/`.
pub(crate) fn windows_dir() -> String {
    let mut buffer = [0u16; 260];
    // SAFETY: the buffer length is passed.
    let len = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    crate::catalog::normalize(&wide(&buffer[..(len as usize).min(buffer.len())]))
}

pub(crate) fn own_pid() -> u32 {
    // SAFETY: no arguments.
    unsafe { GetCurrentProcessId() }
}

pub(crate) fn window_pid(window: HWND) -> u32 {
    let mut pid = 0u32;
    if !window.is_null() {
        // SAFETY: `pid` outlives the call.
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
    }
    pid
}

/// The process of the window in the foreground, if any.
pub(crate) fn foreground_pid() -> u32 {
    // SAFETY: no arguments.
    window_pid(unsafe { GetForegroundWindow() })
}

/// Visible, unowned, titled top-level windows that are not tool windows:
/// (pid, title), front to back.
pub(crate) fn app_windows() -> Vec<(u32, String)> {
    unsafe extern "system" fn collect(window: HWND, lparam: LPARAM) -> windows_sys::core::BOOL {
        // SAFETY: `lparam` is the `Vec` below, alive for the whole call.
        let list = unsafe { &mut *(lparam as *mut Vec<(u32, String)>) };
        // SAFETY: plain queries on a window handle EnumWindows passed.
        unsafe {
            let tool = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0;
            if IsWindowVisible(window) == 0 || tool || !GetWindow(window, GW_OWNER).is_null() {
                return 1;
            }
            let mut title = [0u16; 256];
            let len = GetWindowTextW(window, title.as_mut_ptr(), title.len() as i32);
            if len > 0 {
                list.push((window_pid(window), wide(&title[..len as usize])));
            }
        }
        1
    }
    let mut list: Vec<(u32, String)> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    unsafe {
        EnumWindows(
            Some(collect),
            std::ptr::from_mut(&mut list) as *mut c_void as LPARAM,
        );
    }
    list
}
