//! The low-level keyboard and mouse hooks (`WH_KEYBOARD_LL`,
//! `WH_MOUSE_LL`) on a thread of their own with its message loop, which
//! is what lets push to talk see a key's down and up while another app (a
//! fullscreen game too) has the focus. `RegisterHotKey` can do neither
//! holds nor mouse buttons.
//!
//! The callbacks do one thing: hand the event to the sink (a channel send)
//! and return, so Windows never drops the hook for being slow
//! (`LowLevelHooksTimeout`). They always pass the input on
//! (`CallNextHookEx`): no key or click of another app is ever swallowed.
//! The mouse hook goes in only while a binding needs a mouse button (every
//! mouse move passes through a low-level mouse hook). The same thread
//! watches the foreground window (`EVENT_SYSTEM_FOREGROUND`, out of
//! context), for the elevated-window note.

use std::cell::RefCell;
use std::io;
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::GetCurrentThreadId;
use windows_sys::Win32::UI::Accessibility::{
    SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
    UnhookWindowsHookEx, EVENT_SYSTEM_FOREGROUND, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
    LLKHF_INJECTED, LLKHF_LOWER_IL_INJECTED, LLMHF_INJECTED, LLMHF_LOWER_IL_INJECTED, MSG,
    MSLLHOOKSTRUCT, PM_NOREMOVE, WH_KEYBOARD_LL, WH_MOUSE_LL, WINEVENT_OUTOFCONTEXT,
    WINEVENT_SKIPOWNPROCESS, WM_APP, WM_KEYDOWN, WM_KEYUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_QUIT, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_USER, WM_XBUTTONDOWN, WM_XBUTTONUP, XBUTTON1,
    XBUTTON2,
};

use crate::key::{Key, MouseButton};

/// What the hook thread reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookEvent {
    Key {
        key: Key,
        down: bool,
        /// `LLKHF_INJECTED` / `LLMHF_INJECTED`: synthesized (SendInput),
        /// not typed.
        injected: bool,
    },
    /// Another process's window took the foreground (its `HWND`).
    Foreground(isize),
}

/// Where events go; called on the hook thread, so it must be quick.
pub type Sink = Box<dyn Fn(HookEvent) + Send + 'static>;

/// The thread message that puts the mouse hook in (`wParam` 1) or out (0).
const WM_SET_MOUSE: u32 = WM_APP + 1;

thread_local! {
    static SINK: RefCell<Option<Sink>> = const { RefCell::new(None) };
}

fn emit(event: HookEvent) {
    SINK.with(|sink| {
        if let Ok(sink) = sink.try_borrow() {
            if let Some(sink) = sink.as_ref() {
                sink(event);
            }
        }
    });
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        // SAFETY: for HC_ACTION, lParam points at a KBDLLHOOKSTRUCT for
        // the length of the call.
        let info = unsafe { &*(lparam as *const KBDLLHOOKSTRUCT) };
        let message = u32::try_from(wparam).unwrap_or(0);
        let down = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let up = matches!(message, WM_KEYUP | WM_SYSKEYUP);
        if down || up {
            if let Some(key) = Key::vk(info.vkCode) {
                emit(HookEvent::Key {
                    key,
                    down,
                    injected: info.flags & (LLKHF_INJECTED | LLKHF_LOWER_IL_INJECTED) != 0,
                });
            }
        }
    }
    // SAFETY: passes the call on unchanged (the hook handle is ignored).
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam != 0 {
        let message = u32::try_from(wparam).unwrap_or(0);
        // Moves and the left and right buttons leave at once.
        if matches!(
            message,
            WM_XBUTTONDOWN | WM_XBUTTONUP | WM_MBUTTONDOWN | WM_MBUTTONUP
        ) {
            // SAFETY: for HC_ACTION, lParam points at an MSLLHOOKSTRUCT.
            let info = unsafe { &*(lparam as *const MSLLHOOKSTRUCT) };
            let button = match message {
                WM_MBUTTONDOWN | WM_MBUTTONUP => Some(MouseButton::Middle),
                _ => match (info.mouseData >> 16) as u16 {
                    XBUTTON1 => Some(MouseButton::X1),
                    XBUTTON2 => Some(MouseButton::X2),
                    _ => None,
                },
            };
            if let Some(button) = button {
                emit(HookEvent::Key {
                    key: Key::Mouse(button),
                    down: matches!(message, WM_XBUTTONDOWN | WM_MBUTTONDOWN),
                    injected: info.flags & (LLMHF_INJECTED | LLMHF_LOWER_IL_INJECTED) != 0,
                });
            }
        }
    }
    // SAFETY: as in `keyboard_proc`.
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wparam, lparam) }
}

unsafe extern "system" fn foreground_proc(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    _object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if event == EVENT_SYSTEM_FOREGROUND && !hwnd.is_null() {
        emit(HookEvent::Foreground(hwnd as isize));
    }
}

/// The running hook thread. Dropping it (or [`Hook::stop`]) removes the
/// hooks and ends the thread.
pub struct Hook {
    thread_id: u32,
    join: Option<JoinHandle<bool>>,
}

fn install_mouse() -> HHOOK {
    // SAFETY: a valid callback and this module's handle.
    unsafe {
        SetWindowsHookExW(
            WH_MOUSE_LL,
            Some(mouse_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        )
    }
}

impl Hook {
    /// Starts the thread and installs the keyboard hook (and the mouse
    /// hook with `mouse`). Fails if Windows refuses the keyboard hook.
    pub fn start(sink: Sink, mouse: bool) -> io::Result<Hook> {
        let (ready_tx, ready_rx) = mpsc::channel::<io::Result<u32>>();
        let join = std::thread::Builder::new()
            .name("yap-hotkeys-hook".into())
            .spawn(move || run(sink, mouse, &ready_tx))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Hook {
                thread_id,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => Err(io::Error::other("hook thread ended")),
        }
    }

    /// Puts the mouse hook in or takes it out.
    pub fn set_mouse(&self, on: bool) {
        // SAFETY: a thread message to our own thread, no pointers.
        unsafe {
            PostThreadMessageW(self.thread_id, WM_SET_MOUSE, usize::from(on), 0);
        }
    }

    /// Removes the hooks and waits for the thread; `true` when every hook
    /// came out cleanly.
    pub fn stop(mut self) -> bool {
        self.shutdown()
    }

    fn shutdown(&mut self) -> bool {
        let Some(join) = self.join.take() else {
            return true;
        };
        // SAFETY: as in `set_mouse`.
        unsafe {
            PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
        }
        join.join().unwrap_or(false)
    }
}

impl Drop for Hook {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn run(sink: Sink, mouse: bool, ready: &mpsc::Sender<io::Result<u32>>) -> bool {
    SINK.with(|s| *s.borrow_mut() = Some(sink));
    let mut msg = MSG::default();
    // SAFETY: plain Win32 calls on this thread; the hooks are removed
    // before it ends.
    unsafe {
        // Makes the thread's message queue, so a stop posted right after
        // the start is not lost.
        PeekMessageW(&mut msg, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
        let keyboard = SetWindowsHookExW(
            WH_KEYBOARD_LL,
            Some(keyboard_proc),
            GetModuleHandleW(std::ptr::null()),
            0,
        );
        if keyboard.is_null() {
            let _ = ready.send(Err(io::Error::last_os_error()));
            return false;
        }
        let mut mouse_hook: HHOOK = if mouse {
            install_mouse()
        } else {
            std::ptr::null_mut()
        };
        let foreground = SetWinEventHook(
            EVENT_SYSTEM_FOREGROUND,
            EVENT_SYSTEM_FOREGROUND,
            std::ptr::null_mut(),
            Some(foreground_proc),
            0,
            0,
            WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
        );
        let _ = ready.send(Ok(GetCurrentThreadId()));
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            if msg.hwnd.is_null() && msg.message == WM_SET_MOUSE {
                let on = msg.wParam != 0;
                if on && mouse_hook.is_null() {
                    mouse_hook = install_mouse();
                } else if !on && !mouse_hook.is_null() {
                    UnhookWindowsHookEx(mouse_hook);
                    mouse_hook = std::ptr::null_mut();
                }
            }
        }
        let mut clean = UnhookWindowsHookEx(keyboard) != 0;
        if !mouse_hook.is_null() {
            clean &= UnhookWindowsHookEx(mouse_hook) != 0;
        }
        if !foreground.is_null() {
            clean &= UnhookWinEvent(foreground) != 0;
        }
        SINK.with(|s| *s.borrow_mut() = None);
        clean
    }
}

/// The key is physically down now (`GetAsyncKeyState`): the stuck-key
/// check after a key-up the hook never saw (the secure desktop, an
/// elevated window).
pub fn is_down(key: Key) -> bool {
    // SAFETY: no pointers.
    let state = unsafe { windows_sys::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState(key.async_code()) };
    // The high bit: down now.
    state < 0
}
