//! WebView2 players for the embeds that only exist as web pages (YouTube,
//! Spotify, TikTok's own player, rich `video.url` players), driven by the
//! `WebEmbed` item in `cpp/webembed.cpp`.
//!
//! Each player is a WebView2 controller inside its own child window of the
//! app's window. The C++ item places it: the child window takes the part of
//! the item that the chat viewport shows (so the list clips it), and the
//! controller is offset inside it by what is cut off. Everything runs on the
//! GUI thread, which pumps the messages WebView2 posts its callbacks with.
//!
//! As on the web, the player is an iframe with the web's sandbox: a small
//! host page (written to the profile folder and served under the virtual
//! host [`HOST`]) frames the player URL, which must be one of the hosts the
//! desktop app's CSP allows as frames (`frame-src` in
//! desktop/src-tauri/tauri.conf.json). A real frame matters: YouTube's
//! embed refuses to play without an embedding page (error 153). Top-level
//! navigation stays on the host page; any other navigation and every new
//! window go to the system browser. Dev tools, host objects, web messages, script dialogs
//! and the context menu are off. The profile lives in
//! `%LOCALAPPDATA%\YapNative\WebView2`, apart from the installed app's.

#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use webview2_com::Microsoft::Web::WebView2::Win32::{
    ICoreWebView2, ICoreWebView2Controller, ICoreWebView2Controller2, ICoreWebView2Controller3,
    ICoreWebView2Environment, ICoreWebView2EnvironmentOptions,
    ICoreWebView2NavigationStartingEventArgs, ICoreWebView2Settings3, ICoreWebView2Settings4,
    ICoreWebView2_3, ICoreWebView2_8, COREWEBVIEW2_COLOR,
    COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS, COREWEBVIEW2_PROCESS_FAILED_KIND,
    COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED,
    COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
};
use webview2_com::{
    take_pwstr, CoreWebView2EnvironmentOptions, CreateCoreWebView2ControllerCompletedHandler,
    CreateCoreWebView2EnvironmentCompletedHandler, NavigationStartingEventHandler,
    NewWindowRequestedEventHandler, ProcessFailedEventHandler,
};
use windows::core::{w, Interface, HRESULT, HSTRING, PCWSTR, PWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SetWindowPos, HWND_TOP,
    SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_SHOWWINDOW, WINDOW_EX_STYLE, WNDCLASSW, WS_CHILD,
    WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
};

// The loader, linked statically from webview2-com-sys's copy (its own
// binding imports it from WebView2Loader.dll, which would have to ship).
#[link(name = "WebView2LoaderStatic", kind = "static")]
extern "system" {
    #[link_name = "CreateCoreWebView2EnvironmentWithOptions"]
    fn create_environment(
        browser_folder: PCWSTR,
        user_data_folder: PCWSTR,
        options: *mut core::ffi::c_void,
        handler: *mut core::ffi::c_void,
    ) -> HRESULT;
}

/// What a player tells its item: `READY` (the page is loading), `FAILED`
/// (no WebView2: the item goes back to its poster and remembers it, so a
/// click opens the link), `EXTERNAL` (open `text` in the system browser; sent
/// for a user's click only), `CRASHED` (the browser or render process died:
/// back to the poster, nothing opens).
pub type Callback = extern "C" fn(id: u64, event: i32, text: *const u16, len: usize);
const READY: i32 = 1;
const FAILED: i32 = 2;
const EXTERNAL: i32 = 3;
const CRASHED: i32 = 4;

/// Where the item is and what of it shows, in the parent's client pixels,
/// and the window's device pixel ratio (Qt's, which the page's CSS pixels
/// follow so they stay the QML item's logical pixels).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Place {
    item: RECT,
    clip: RECT,
    visible: bool,
    scale: f64,
}

struct Host {
    child: HWND,
    url: String,
    muted: bool,
    controller: Option<ICoreWebView2Controller>,
    place: Place,
}

enum Env {
    Missing,
    /// Creating; these hosts wait for it.
    Creating(Vec<u64>),
    Ready(ICoreWebView2Environment),
    Failed,
}

thread_local! {
    static HOSTS: RefCell<HashMap<u64, Host>> = RefCell::new(HashMap::new());
    static ENV: RefCell<Env> = const { RefCell::new(Env::Missing) };
    static NEXT: Cell<u64> = const { Cell::new(1) };
    static CALLBACK: Cell<Option<Callback>> = const { Cell::new(None) };
    static CLASS: Cell<bool> = const { Cell::new(false) };
}

/// The virtual host the player's frame page is served under.
const HOST: &str = "player.yapnative.local";

/// The frame page: the player in an iframe with the web's sandbox
/// (LinkEmbed.tsx `EMBED_PLAYER_SANDBOX`, `EMBED_WIDGET_SANDBOX` for
/// Spotify), only for an allowed player URL.
const PAGE: &str = r#"<!doctype html>
<html><head><meta charset="utf-8"><title>Player</title>
<style>html,body{margin:0;height:100%;background:#000;overflow:hidden}iframe{border:0;width:100%;height:100%;display:block}</style>
</head><body><script>
const src = decodeURIComponent(location.hash.slice(1));
const ok = /^https:\/\/((www\.)?youtube(-nocookie)?\.com|([a-z0-9-]+\.)?tiktok\.com|open\.spotify\.com|(www\.)?instagram\.com|platform\.twitter\.com|syndication\.twitter\.com)\//.test(src);
if (ok) {
  const f = document.createElement("iframe");
  const widget = src.startsWith("https://open.spotify.com/");
  f.setAttribute("sandbox", widget
    ? "allow-forms allow-modals allow-popups allow-popups-to-escape-sandbox allow-presentation allow-scripts"
    : "allow-presentation allow-same-origin allow-scripts");
  f.setAttribute("allow", "accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share; fullscreen");
  f.allowFullscreen = true;
  f.src = src;
  document.body.appendChild(f);
}
</script></body></html>
"#;

/// `encodeURIComponent`.
fn component(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            out.push(char::from(byte));
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn page_folder() -> String {
    std::path::Path::new(&user_data_folder())
        .join("player")
        .to_string_lossy()
        .into_owned()
}

/// Writes the frame page (every start: a new build may change it).
fn write_page() -> std::io::Result<()> {
    let folder = page_folder();
    std::fs::create_dir_all(&folder)?;
    std::fs::write(std::path::Path::new(&folder).join("index.html"), PAGE)
}

/// Hosts a player page may navigate to in place (the desktop CSP's
/// `frame-src`, plus YouTube's consent and TikTok's player subdomains).
pub fn allowed(url: &str) -> bool {
    if url == "about:blank" || url.starts_with(&format!("https://{HOST}/")) {
        return true;
    }
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .rsplit('@')
        .next()
        .unwrap_or_default()
        .split(':')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    under("youtube.com")
        || under("youtube-nocookie.com")
        || under("tiktok.com")
        || host == "open.spotify.com"
        || host == "www.instagram.com"
        || host == "instagram.com"
        || host == "platform.twitter.com"
        || host == "syndication.twitter.com"
}

fn emit(id: u64, event: i32, text: &str) {
    if let Some(callback) = CALLBACK.get() {
        let wide: Vec<u16> = text.encode_utf16().collect();
        callback(id, event, wide.as_ptr(), wide.len());
    }
}

fn controller(id: u64) -> Option<ICoreWebView2Controller> {
    HOSTS.with(|hosts| hosts.borrow().get(&id).and_then(|h| h.controller.clone()))
}

unsafe extern "system" fn host_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: forwarding the window procedure's own arguments.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn register_class() -> windows::core::Result<()> {
    if CLASS.get() {
        return Ok(());
    }
    // SAFETY: plain Win32 calls with valid arguments; the class name is a
    // static string.
    unsafe {
        let instance = GetModuleHandleW(None)?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(host_proc),
            hInstance: instance.into(),
            lpszClassName: w!("YapNativeWebEmbed"),
            ..Default::default()
        };
        if RegisterClassW(&class) == 0 {
            return Err(windows::core::Error::from_thread());
        }
    }
    CLASS.set(true);
    Ok(())
}

fn user_data_folder() -> String {
    yap_cache::profile::data_root()
        .unwrap_or_default()
        .join("WebView2")
        .to_string_lossy()
        .into_owned()
}

fn fail_waiting(waiting: Vec<u64>) {
    for id in waiting {
        emit(id, FAILED, "");
    }
}

/// Creates the shared environment once; `id` waits for it.
fn with_environment(id: u64) {
    let ready = ENV.with(|env| {
        let mut env = env.borrow_mut();
        match &mut *env {
            Env::Ready(environment) => Some(Ok(environment.clone())),
            Env::Failed => Some(Err(())),
            Env::Creating(waiting) => {
                waiting.push(id);
                None
            }
            Env::Missing => {
                *env = Env::Creating(vec![id]);
                None
            }
        }
    });
    match ready {
        Some(Ok(environment)) => create_controller(id, &environment),
        Some(Err(())) => emit(id, FAILED, ""),
        None => start_environment(),
    }
}

/// `YAP_FAKE_NO_WEBVIEW2=1` (tests): environment creation fails as it
/// does on a PC without the WebView2 runtime, for this process only.
fn fake_missing_runtime() -> bool {
    std::env::var("YAP_FAKE_NO_WEBVIEW2").is_ok_and(|v| v == "1")
}

/// Fails every player waiting on the environment and remembers it.
fn fail_environment(reason: &str) {
    eprintln!("[webembed] no WebView2 environment: {reason}");
    let waiting = ENV.with(|env| {
        let mut env = env.borrow_mut();
        let waiting = match &mut *env {
            Env::Creating(waiting) => std::mem::take(waiting),
            _ => Vec::new(),
        };
        *env = Env::Failed;
        waiting
    });
    fail_waiting(waiting);
}

fn start_environment() {
    let creating = ENV.with(|env| matches!(&*env.borrow(), Env::Creating(w) if w.len() == 1));
    if !creating {
        return;
    }
    if fake_missing_runtime() {
        fail_environment("YAP_FAKE_NO_WEBVIEW2=1");
        return;
    }
    let handler = CreateCoreWebView2EnvironmentCompletedHandler::create(Box::new(
        |result: windows::core::Result<()>, environment: Option<ICoreWebView2Environment>| {
            let waiting = ENV.with(|env| {
                let mut env = env.borrow_mut();
                let waiting = match &mut *env {
                    Env::Creating(waiting) => std::mem::take(waiting),
                    _ => Vec::new(),
                };
                *env = match (&result, &environment) {
                    (Ok(()), Some(environment)) => Env::Ready(environment.clone()),
                    _ => Env::Failed,
                };
                waiting
            });
            match (result, environment) {
                (Ok(()), Some(environment)) => {
                    for id in waiting {
                        create_controller(id, &environment);
                    }
                }
                (result, _) => {
                    eprintln!("[webembed] no WebView2 environment: {result:?}");
                    fail_waiting(waiting);
                }
            }
            Ok(())
        },
    ));
    if let Err(error) = write_page() {
        eprintln!("[webembed] player page: {error}");
    }
    let options = CoreWebView2EnvironmentOptions::default();
    // SAFETY: the setter writes the options object's own cell.
    unsafe {
        // Players start on the click that opened them, with sound.
        options.set_additional_browser_arguments(
            "--autoplay-policy=no-user-gesture-required".to_owned(),
        );
    }
    let options: ICoreWebView2EnvironmentOptions = options.into();
    let folder = HSTRING::from(user_data_folder());
    // SAFETY: valid strings and COM pointers for the call; the loader
    // AddRefs what it keeps.
    let hr = unsafe {
        create_environment(
            PCWSTR::null(),
            PCWSTR(folder.as_ptr()),
            options.as_raw(),
            handler.as_raw(),
        )
    };
    if hr.is_err() {
        fail_environment(&format!("CreateCoreWebView2EnvironmentWithOptions: {hr:?}"));
    }
}

fn create_controller(id: u64, environment: &ICoreWebView2Environment) {
    let Some(child) = HOSTS.with(|hosts| hosts.borrow().get(&id).map(|h| h.child)) else {
        return;
    };
    let handler = CreateCoreWebView2ControllerCompletedHandler::create(Box::new(
        move |result: windows::core::Result<()>, controller: Option<ICoreWebView2Controller>| {
            let controller = match (result, controller) {
                (Ok(()), Some(controller)) => controller,
                (result, _) => {
                    eprintln!("[webembed] controller: {result:?}");
                    emit(id, FAILED, "");
                    return Ok(());
                }
            };
            let alive = HOSTS.with(|hosts| {
                let mut hosts = hosts.borrow_mut();
                match hosts.get_mut(&id) {
                    Some(host) => {
                        host.controller = Some(controller.clone());
                        Some((host.url.clone(), host.muted, host.place))
                    }
                    None => None,
                }
            });
            let Some((url, muted, place)) = alive else {
                // Closed while it was being created.
                // SAFETY: a live controller we own.
                let _ = unsafe { controller.Close() };
                return Ok(());
            };
            if let Err(error) = start_page(id, &controller, &url, muted) {
                eprintln!("[webembed] page: {error:?}");
                emit(id, FAILED, "");
                return Ok(());
            }
            apply_place(id, place);
            emit(id, READY, "");
            Ok(())
        },
    ));
    // SAFETY: `child` is our live window; the handler is a COM object.
    if let Err(error) = unsafe { environment.CreateCoreWebView2Controller(child, &handler) } {
        eprintln!("[webembed] CreateCoreWebView2Controller: {error:?}");
        emit(id, FAILED, "");
    }
}

fn uri_of(get: impl FnOnce(*mut PWSTR) -> windows::core::Result<()>) -> String {
    let mut uri = PWSTR::null();
    if get(&mut uri).is_err() {
        return String::new();
    }
    take_pwstr(uri)
}

/// Whether a failed process ends the player: the browser, the host page's
/// renderer, or the player frame's own (the page has no other frame).
fn fatal(kind: COREWEBVIEW2_PROCESS_FAILED_KIND) -> bool {
    kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
        || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED
}

/// The browser process died: the shared environment is gone with it, so
/// the next player makes a new one instead of failing on the dead one.
fn forget_environment() {
    ENV.with(|env| {
        let mut env = env.borrow_mut();
        if matches!(&*env, Env::Ready(_)) {
            *env = Env::Missing;
        }
    });
}

/// A navigation of the page or one of its frames: cancelled unless the host
/// is allowed; a cancelled http(s) one opens externally only when the user
/// started it.
fn guard_navigation(
    id: u64,
    args: Option<ICoreWebView2NavigationStartingEventArgs>,
) -> windows::core::Result<()> {
    let Some(args) = args else { return Ok(()) };
    // SAFETY: COM calls on a live event args object with valid, writable
    // out-pointers, inside the handler.
    unsafe {
        let uri = uri_of(|out| args.Uri(out));
        if allowed(&uri) {
            return Ok(());
        }
        args.SetCancel(true)?;
        if uri.starts_with("https://") || uri.starts_with("http://") {
            let mut user = windows::core::BOOL(0);
            let _ = args.IsUserInitiated(&mut user);
            if user.as_bool() {
                emit(id, EXTERNAL, &uri);
            }
        }
    }
    Ok(())
}

fn start_page(
    id: u64,
    controller: &ICoreWebView2Controller,
    url: &str,
    muted: bool,
) -> windows::core::Result<()> {
    // SAFETY: COM calls on live objects with valid arguments, all on the
    // thread that created them.
    unsafe {
        // Black under the page while it loads (the stage it replaces).
        if let Ok(controller2) = controller.cast::<ICoreWebView2Controller2>() {
            let _ = controller2.SetDefaultBackgroundColor(COREWEBVIEW2_COLOR {
                A: 255,
                R: 0,
                G: 0,
                B: 0,
            });
        }
        let webview: ICoreWebView2 = controller.CoreWebView2()?;
        let settings = webview.Settings()?;
        settings.SetAreDevToolsEnabled(false)?;
        settings.SetAreDefaultContextMenusEnabled(false)?;
        settings.SetIsStatusBarEnabled(false)?;
        settings.SetAreHostObjectsAllowed(false)?;
        settings.SetIsWebMessageEnabled(false)?;
        settings.SetAreDefaultScriptDialogsEnabled(false)?;
        settings.SetIsZoomControlEnabled(false)?;
        // No browser shortcuts (F5, Ctrl+P, Ctrl+F ...) and nothing saved:
        // this is a player, not a browser.
        if let Ok(settings3) = settings.cast::<ICoreWebView2Settings3>() {
            settings3.SetAreBrowserAcceleratorKeysEnabled(false)?;
        }
        if let Ok(settings4) = settings.cast::<ICoreWebView2Settings4>() {
            settings4.SetIsPasswordAutosaveEnabled(false)?;
            settings4.SetIsGeneralAutofillEnabled(false)?;
        }
        if muted {
            if let Ok(webview8) = webview.cast::<ICoreWebView2_8>() {
                webview8.SetIsMuted(true)?;
            }
        }

        let mut token = 0i64;
        // The page and every frame in it stay on the allowed hosts. Anything
        // else is cancelled, and reaches the system browser only when the
        // user's own click started it.
        webview.add_NavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                guard_navigation(id, args)
            })),
            &mut token,
        )?;
        webview.add_FrameNavigationStarting(
            &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                guard_navigation(id, args)
            })),
            &mut token,
        )?;
        webview.add_NewWindowRequested(
            &NewWindowRequestedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else { return Ok(()) };
                args.SetHandled(true)?;
                let uri = uri_of(|out| args.Uri(out));
                let mut user = windows::core::BOOL(0);
                let _ = args.IsUserInitiated(&mut user);
                if user.as_bool() && (uri.starts_with("https://") || uri.starts_with("http://")) {
                    emit(id, EXTERNAL, &uri);
                }
                Ok(())
            })),
            &mut token,
        )?;
        webview.add_ProcessFailed(
            &ProcessFailedEventHandler::create(Box::new(move |_, args| {
                let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
                if let Some(args) = args {
                    let _ = args.ProcessFailedKind(&mut kind);
                }
                eprintln!("[webembed] process failed: {kind:?}");
                // Only the browser or the page's renderer dying ends the
                // player; a GPU, utility or unresponsive process recovers.
                if kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED {
                    forget_environment();
                }
                if fatal(kind) {
                    emit(id, CRASHED, "");
                }
                Ok(())
            })),
            &mut token,
        )?;

        let webview3: ICoreWebView2_3 = webview.cast()?;
        webview3.SetVirtualHostNameToFolderMapping(
            &HSTRING::from(HOST),
            &HSTRING::from(page_folder()),
            COREWEBVIEW2_HOST_RESOURCE_ACCESS_KIND_DENY_CORS,
        )?;
        let page = format!("https://{HOST}/index.html#{}", component(url));
        webview.Navigate(&HSTRING::from(page))?;
    }
    Ok(())
}

fn apply_place(id: u64, place: Place) {
    let Some(child) = HOSTS.with(|hosts| hosts.borrow().get(&id).map(|h| h.child)) else {
        return;
    };
    let clip = place.clip;
    let shown = place.visible && clip.right > clip.left && clip.bottom > clip.top;
    let flags = SWP_NOACTIVATE
        | if shown {
            SWP_SHOWWINDOW
        } else {
            SWP_HIDEWINDOW
        };
    // SAFETY: our live child window.
    let _ = unsafe {
        SetWindowPos(
            child,
            Some(HWND_TOP),
            clip.left,
            clip.top,
            (clip.right - clip.left).max(1),
            (clip.bottom - clip.top).max(1),
            flags,
        )
    };
    if let Some(controller) = controller(id) {
        let item = place.item;
        let bounds = RECT {
            left: item.left - clip.left,
            top: item.top - clip.top,
            right: item.right - clip.left,
            bottom: item.bottom - clip.top,
        };
        // SAFETY: a live controller on its own thread.
        unsafe {
            // Qt's ratio, not the monitor's: they differ under
            // QT_SCALE_FACTOR or a rounding policy, and the bounds above are
            // Qt's device pixels. Set before the bounds, so a move to a
            // monitor with another scale lays the page out once.
            if place.scale > 0.0 {
                if let Ok(controller3) = controller.cast::<ICoreWebView2Controller3>() {
                    let _ = controller3.SetShouldDetectMonitorScaleChanges(false);
                    let mut current = 0.0;
                    let _ = controller3.RasterizationScale(&mut current);
                    if (current - place.scale).abs() > 1e-3 {
                        let _ = controller3.SetRasterizationScale(place.scale);
                    }
                }
            }
            let _ = controller.SetBounds(bounds);
            let _ = controller.SetIsVisible(shown);
        }
    }
}

/// Registers the item's callback (once, before any player opens).
#[no_mangle]
pub extern "C" fn yap_web_set_callback(callback: Callback) {
    CALLBACK.set(Some(callback));
}

/// Opens a player for `url` (UTF-16, `len` units) inside `parent`, hidden
/// until placed. Returns its id, or 0 when no child window could be made.
///
/// # Safety
/// `url` must point to `len` readable UTF-16 units; `parent` must be a live
/// window of this thread.
#[no_mangle]
pub unsafe extern "C" fn yap_web_open(
    parent: isize,
    url: *const u16,
    len: usize,
    muted: bool,
) -> u64 {
    if url.is_null() || len == 0 || parent == 0 {
        return 0;
    }
    // SAFETY: the caller guarantees `len` units at `url`.
    let url = String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(url, len) });
    if !allowed(&url) {
        return 0;
    }
    if let Err(error) = register_class() {
        eprintln!("[webembed] window class: {error:?}");
        return 0;
    }
    // SAFETY: a child of the caller's live window, our registered class.
    let child = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            w!("YapNativeWebEmbed"),
            w!(""),
            WS_CHILD | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
            0,
            0,
            1,
            1,
            Some(HWND(parent as *mut core::ffi::c_void)),
            None,
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
    };
    let Ok(child) = child else {
        return 0;
    };
    let id = NEXT.get();
    NEXT.set(id + 1);
    HOSTS.with(|hosts| {
        hosts.borrow_mut().insert(
            id,
            Host {
                child,
                url,
                muted,
                controller: None,
                place: Place::default(),
            },
        )
    });
    with_environment(id);
    id
}

/// Moves a player: `item_*` is the whole item and `clip_*` the part shown,
/// both in the parent's client pixels; `scale` is the window's device pixel
/// ratio.
#[no_mangle]
#[allow(clippy::too_many_arguments)]
pub extern "C" fn yap_web_place(
    id: u64,
    item_x: i32,
    item_y: i32,
    item_w: i32,
    item_h: i32,
    clip_x: i32,
    clip_y: i32,
    clip_w: i32,
    clip_h: i32,
    visible: bool,
    scale: f64,
) {
    let place = Place {
        item: RECT {
            left: item_x,
            top: item_y,
            right: item_x + item_w,
            bottom: item_y + item_h,
        },
        clip: RECT {
            left: clip_x,
            top: clip_y,
            right: clip_x + clip_w,
            bottom: clip_y + clip_h,
        },
        visible,
        scale,
    };
    let changed = HOSTS.with(|hosts| {
        let mut hosts = hosts.borrow_mut();
        match hosts.get_mut(&id) {
            Some(host) if host.place != place => {
                host.place = place;
                true
            }
            _ => false,
        }
    });
    if changed {
        apply_place(id, place);
    }
}

/// The app's window moved on screen (the child windows did not move in it):
/// WebView2 needs to hear it for its own popups and accessibility.
#[no_mangle]
pub extern "C" fn yap_web_parent_moved(id: u64) {
    if let Some(controller) = controller(id) {
        // SAFETY: a live controller on its own thread.
        let _ = unsafe { controller.NotifyParentWindowPositionChanged() };
    }
}

/// Closes a player and its window.
#[no_mangle]
pub extern "C" fn yap_web_close(id: u64) {
    let Some(host) = HOSTS.with(|hosts| hosts.borrow_mut().remove(&id)) else {
        return;
    };
    // SAFETY: our own controller and window, on their thread.
    unsafe {
        if let Some(controller) = host.controller {
            let _ = controller.Close();
        }
        let _ = DestroyWindow(host.child);
    }
}

#[cfg(test)]
mod tests {
    use super::allowed;

    #[test]
    fn player_hosts_should_load_in_place() {
        assert!(allowed("https://www.youtube.com/embed/x?autoplay=1"));
    }

    #[test]
    fn other_hosts_should_leave() {
        assert!(!allowed("https://evil.example/youtube.com"));
    }

    #[test]
    fn a_lookalike_suffix_should_leave() {
        assert!(!allowed("https://notyoutube.com/embed"));
    }

    #[test]
    fn a_crashed_player_frame_should_end_the_player() {
        assert!(super::fatal(
            super::COREWEBVIEW2_PROCESS_FAILED_KIND_FRAME_RENDER_PROCESS_EXITED
        ));
    }

    #[test]
    fn plain_http_should_leave() {
        assert!(!allowed("http://www.youtube.com/embed/x"));
    }
}
