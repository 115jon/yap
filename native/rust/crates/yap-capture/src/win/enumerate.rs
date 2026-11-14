//! What the picker lists: monitors and top-level windows, from the OS.

use std::collections::HashMap;
use std::ffi::c_void;
use std::mem::size_of;

use windows::core::{BOOL, PWSTR};
use windows::Win32::Devices::Display::{
    DisplayConfigGetDeviceInfo, GetDisplayConfigBufferSizes, QueryDisplayConfig,
    DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME, DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
    DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO, DISPLAYCONFIG_PATH_INFO,
    DISPLAYCONFIG_SOURCE_DEVICE_NAME, DISPLAYCONFIG_TARGET_DEVICE_NAME, QDC_ONLY_ACTIVE_PATHS,
};
use windows::Win32::Foundation::{CloseHandle, ERROR_SUCCESS, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DwmGetWindowAttribute, DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS,
};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId, InternalGetWindowText,
    IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

use crate::filter::{is_listed_window, is_ours, monitor_name, sort_sources};
use crate::ids::SourceId;
use crate::{Source, SourceKind};

/// `MONITORINFO.dwFlags` bit of the primary monitor.
const MONITORINFOF_PRIMARY: u32 = 1;

/// A monitor in `EnumDisplayMonitors` order.
pub(super) struct Monitor {
    /// The `HMONITOR` value.
    pub(super) handle: isize,
    /// The GDI device name (`\\.\DISPLAY1`).
    device: String,
    primary: bool,
}

impl Monitor {
    #[cfg(test)]
    pub(super) fn device_name(&self) -> String {
        self.device.clone()
    }
}

pub(super) fn monitors() -> Vec<Monitor> {
    unsafe extern "system" fn collect(
        handle: HMONITOR,
        _: HDC,
        _: *mut RECT,
        lparam: LPARAM,
    ) -> BOOL {
        // SAFETY: `lparam` is the `&mut Vec<Monitor>` passed below, alive
        // for the whole `EnumDisplayMonitors` call.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<Monitor>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        // SAFETY: `MONITORINFOEXW` starts with a `MONITORINFO`; `cbSize`
        // tells the call to fill the extended struct.
        let got =
            unsafe { GetMonitorInfoW(handle, std::ptr::from_mut(&mut info).cast::<MONITORINFO>()) };
        let (device, primary) = if got.as_bool() {
            (
                wide_to_string(&info.szDevice),
                info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
            )
        } else {
            (String::new(), false)
        };
        list.push(Monitor {
            handle: handle.0 as isize,
            device,
            primary,
        });
        true.into()
    }

    let mut list: Vec<Monitor> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    let _ = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(std::ptr::from_mut(&mut list) as isize),
        )
    };
    list
}

/// Monitor friendly names by GDI device name, from the display config.
fn friendly_names() -> HashMap<String, String> {
    let mut names = HashMap::new();
    let (mut path_count, mut mode_count) = (0u32, 0u32);
    // SAFETY: plain size query into live out values.
    let sized = unsafe {
        GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut path_count, &mut mode_count)
    };
    if sized != ERROR_SUCCESS {
        return names;
    }
    let mut paths = vec![DISPLAYCONFIG_PATH_INFO::default(); path_count as usize];
    let mut modes = vec![DISPLAYCONFIG_MODE_INFO::default(); mode_count as usize];
    // SAFETY: the buffers hold the element counts passed with them.
    let queried = unsafe {
        QueryDisplayConfig(
            QDC_ONLY_ACTIVE_PATHS,
            &mut path_count,
            paths.as_mut_ptr(),
            &mut mode_count,
            modes.as_mut_ptr(),
            None,
        )
    };
    if queried != ERROR_SUCCESS {
        return names;
    }
    for path in &paths[..path_count as usize] {
        let mut source = DISPLAYCONFIG_SOURCE_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_SOURCE_NAME,
                size: size_of::<DISPLAYCONFIG_SOURCE_DEVICE_NAME>() as u32,
                adapterId: path.sourceInfo.adapterId,
                id: path.sourceInfo.id,
            },
            ..Default::default()
        };
        let mut target = DISPLAYCONFIG_TARGET_DEVICE_NAME {
            header: DISPLAYCONFIG_DEVICE_INFO_HEADER {
                r#type: DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME,
                size: size_of::<DISPLAYCONFIG_TARGET_DEVICE_NAME>() as u32,
                adapterId: path.targetInfo.adapterId,
                id: path.targetInfo.id,
            },
            ..Default::default()
        };
        // SAFETY: each header is the first field of a struct whose size it
        // declares; 0 is success.
        let (source_ok, target_ok) = unsafe {
            (
                DisplayConfigGetDeviceInfo(&mut source.header),
                DisplayConfigGetDeviceInfo(&mut target.header),
            )
        };
        if source_ok != 0 || target_ok != 0 {
            continue;
        }
        let friendly = wide_to_string(&target.monitorFriendlyDeviceName);
        if !friendly.is_empty() {
            names.insert(wide_to_string(&source.viewGdiDeviceName), friendly);
        }
    }
    names
}

/// A window handle as the window functions take it.
pub(super) fn hwnd(value: isize) -> HWND {
    HWND(value as *mut c_void)
}

pub(super) fn window_pid(window: HWND) -> Option<u32> {
    let mut pid = 0u32;
    // SAFETY: a plain query with a live out value.
    let thread = unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    (thread != 0 && pid != 0).then_some(pid)
}

/// A window's title without sending it a message (`GetWindowText` would
/// send `WM_GETTEXT` to a window of our own process and wait for its
/// thread).
pub(super) fn window_title(window: HWND) -> String {
    let mut buffer = [0u16; 512];
    // SAFETY: the call fills at most `buffer.len()` units.
    let len = unsafe { InternalGetWindowText(window, &mut buffer) };
    wide_to_string(&buffer[..usize::try_from(len).unwrap_or(0).min(buffer.len())])
}

/// Whether `window` is one of this process's, or titled as the test window.
pub(super) fn window_is_ours(window: HWND) -> bool {
    // SAFETY: no arguments.
    let ours = window_pid(window).is_some_and(|pid| pid == unsafe { GetCurrentProcessId() });
    is_ours(&window_title(window), ours)
}

/// The full path of a process's executable.
pub(super) fn process_image_path(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed below; the buffer length is passed.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = vec![0u16; 32_768];
        let mut len = buffer.len() as u32;
        let queried = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut len,
        );
        let _ = CloseHandle(process);
        queried.ok()?;
        Some(String::from_utf16_lossy(&buffer[..len as usize]))
    }
}

/// The executable's name without its extension (`chrome`), or empty.
fn process_name(pid: u32) -> String {
    process_image_path(pid)
        .and_then(|path| {
            std::path::Path::new(&path)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .unwrap_or_default()
}

fn wide_to_string(wide: &[u16]) -> String {
    let end = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..end])
}

/// Top-level windows, front to back.
fn top_level_windows() -> Vec<isize> {
    unsafe extern "system" fn collect(window: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `&mut Vec<isize>` passed below, alive for
        // the whole `EnumWindows` call.
        let list = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
        list.push(window.0 as isize);
        true.into()
    }

    let mut list: Vec<isize> = Vec::new();
    // SAFETY: the callback only touches `list`, which outlives the call.
    let _ = unsafe {
        EnumWindows(
            Some(collect),
            LPARAM(std::ptr::from_mut(&mut list) as isize),
        )
    };
    list
}

/// Cloaked windows (other virtual desktops, suspended UWP apps) are in
/// `EnumWindows` and "visible" but not on screen.
fn is_cloaked(window: HWND) -> bool {
    let mut cloaked = 0u32;
    // SAFETY: `cloaked` is a `u32`, the size the attribute reports.
    let got = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED,
            std::ptr::from_mut(&mut cloaked).cast(),
            size_of::<u32>() as u32,
        )
    };
    got.is_ok() && cloaked != 0
}

/// The window's size on screen (without the invisible resize borders).
fn window_size(window: HWND) -> Option<(i32, i32)> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is a `RECT`, the size the attribute reports.
    let framed = unsafe {
        DwmGetWindowAttribute(
            window,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            std::ptr::from_mut(&mut rect).cast(),
            size_of::<RECT>() as u32,
        )
    };
    // SAFETY: a plain query into a live out value.
    if framed.is_err() && unsafe { GetWindowRect(window, &mut rect) }.is_err() {
        return None;
    }
    Some((rect.right - rect.left, rect.bottom - rect.top))
}

pub(super) fn sources() -> Vec<Source> {
    let mut sources = Vec::new();

    let friendly = friendly_names();
    for (index, monitor) in monitors().iter().enumerate() {
        sources.push(Source {
            id: SourceId::Monitor(index).to_string(),
            name: monitor_name(
                friendly.get(&monitor.device).map(String::as_str),
                index,
                monitor.primary,
            ),
            kind: SourceKind::Monitor,
            app_name: String::new(),
            ours: false,
        });
    }

    // SAFETY: no arguments.
    let our_pid = unsafe { GetCurrentProcessId() };
    let mut process_names: HashMap<u32, String> = HashMap::new();
    for value in top_level_windows() {
        let window = hwnd(value);
        // SAFETY: plain queries on a handle that may have died since the
        // enumeration; they then just report false.
        let hidden = unsafe { !IsWindowVisible(window).as_bool() || IsIconic(window).as_bool() };
        // SAFETY: as above.
        let ex_style = unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32;
        if hidden || ex_style & WS_EX_TOOLWINDOW.0 != 0 || is_cloaked(window) {
            continue;
        }
        let title = window_title(window);
        let Some((width, height)) = window_size(window) else {
            continue;
        };
        let pid = window_pid(window);
        let app_name = pid.map_or_else(String::new, |pid| {
            process_names
                .entry(pid)
                .or_insert_with(|| process_name(pid))
                .clone()
        });
        if !is_listed_window(&title, &app_name, width, height) {
            continue;
        }
        sources.push(Source {
            id: SourceId::Window(value).to_string(),
            kind: SourceKind::Window,
            ours: is_ours(&title, pid == Some(our_pid)),
            name: title,
            app_name,
        });
    }

    sort_sources(&mut sources);
    sources
}
