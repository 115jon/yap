//! The game capture hook (the desktop app's `game_capture`, ported): OBS
//! Studio's own signed `graphics-hook` injected into a game window's process
//! through OBS's `inject-helper`, read over OBS's shared-texture IPC, for
//! smoother captures than Windows Graphics Capture where a game allows it.
//!
//! Safety first, in this order, before anything is injected (PR #160): the
//! built-in executable blocklist (anti-cheat services, kernel anti-cheat and
//! VAC titles), the add-only remote blocklist from the web app, the learned
//! skips (games the hook already failed on), and OBS's own `OpenProcess`
//! preflight. Only the files pinned in the desktop app's
//! `obs-signed-hook.lock.json` are used, verified (SHA-256, Authenticode,
//! signer) and held open while the helper runs.
//!
//! WGC keeps running under the hook: its frames are dropped while the hook
//! delivers, and flow again the moment it stops (see [`attempt`]).
//!
//! Not ported: the Vulkan implicit layer (its registration would touch
//! `HKCU\SOFTWARE\Khronos`), so a Vulkan game is captured by the hook only
//! when an OBS-signed hook is already resident in it, and the unsigned fork
//! build.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::OnceLock;

/// Logs only with `YAP_HOOK_DEBUG=1` (the desktop app's `log::debug!`).
#[allow(unused_macros)]
macro_rules! hook_debug {
    ($($arg:tt)*) => {
        if $crate::hook::debug_enabled() {
            eprintln!($($arg)*);
        }
    };
}

pub mod attempt;
pub mod blocklist;
pub mod hook_policy;
pub mod inject;
pub mod obs_ipc;
pub mod reader;
pub mod remote_policy;
pub mod signed_hook;

pub(crate) fn debug_enabled() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| crate::env_flag("YAP_HOOK_DEBUG"))
}

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Where the hook keeps its files: the learned skips, the cached remote
/// blocklist and the staged signed set. Set once by the app (the profile's
/// data folder); nothing is kept before.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

pub(crate) fn data_dir() -> Option<PathBuf> {
    DATA_DIR.get().cloned()
}

/// Graphics APIs the `Game_Capture_Hook` can target.
///
/// DX11 and DX12 both present through the DXGI swapchain, so the injected
/// graphics-hook intercepts the **same** `IDXGISwapChain::Present` path for
/// either — the DLL selects the right device internally (`setup_dxgi` →
/// `d3d11`/`d3d12` capture). Vulkan is intercepted differently: the DLL is an
/// implicit Vulkan layer (`vkQueuePresentKHR`), activated by the loader once its
/// manifest is registered (see `vulkan_layer`) and the game launched afterward,
/// then coordinated with the injected capture thread + IPC objects like the
/// DXGI path. All three are active-capable. OpenGL would need its own validated
/// path and stays gated off until implemented (Req 8.1, 8.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphicsApiBackend {
    Dx11,
    /// Captured via the shared DXGI present hook (same path as DX11).
    Dx12,
    /// Captured via the implicit Vulkan layer (`vkQueuePresentKHR`) coordinated
    /// with the injected capture thread/IPC; the layer must be registered with
    /// the Vulkan loader (`vulkan_layer`) and the game launched afterward.
    Vulkan,
    /// Gated off until an OpenGL interception is validated (Req 8.2).
    OpenGl,
}

impl GraphicsApiBackend {
    /// Whether this backend may be used as the active `hook` Capture_Mode.
    ///
    /// DX11/DX12 (shared DXGI present hook) and Vulkan (implicit layer + IPC)
    /// are permitted; OpenGL returns `false` so it can exist in the codebase
    /// without ever being selected until its own path is validated (Req 8.1,
    /// 8.2).
    pub fn is_active_capable(self) -> bool {
        matches!(
            self,
            GraphicsApiBackend::Dx11 | GraphicsApiBackend::Dx12 | GraphicsApiBackend::Vulkan
        )
    }

    /// Stable string form of the active `Graphics_API_Backend` reported through
    /// `NativeShareStats` / `Capture_Status` while the hook is the active
    /// Capture_Mode (Req 14.2). These values are part of the status contract
    /// surfaced to the renderer and MUST remain stable.
    pub fn as_str(self) -> &'static str {
        match self {
            GraphicsApiBackend::Dx11 => "dx11",
            GraphicsApiBackend::Dx12 => "dx12",
            GraphicsApiBackend::Vulkan => "vulkan",
            GraphicsApiBackend::OpenGl => "opengl",
        }
    }
}

/// Result of attempting to inject into and intercept the target process.
///
/// Only `Success` permits `hook`; a failure, an anti-cheat block, or a source
/// for which injection was never attempted all fall back to `wgc`
/// (Req 6.3, 7.4, 8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InjectionOutcome {
    /// The hook attached and intercepted the presented backbuffer.
    Success,
    /// Injection was attempted but the process could not be attached/hooked.
    Failed,
    /// Injection was refused, e.g. by anti-cheat.
    Blocked,
    /// Injection was not attempted (e.g. monitor source or hook disabled).
    NotAttempted,
}

impl InjectionOutcome {
    /// True only when the hook actually attached and intercepted a frame.
    pub fn is_success(self) -> bool {
        matches!(self, InjectionOutcome::Success)
    }
}

/// The pure anti-cheat safety decision (Req 10.2, 10.3).
///
/// Computed by `game_capture::blocklist::safety_decision` from the target's
/// executable name and the configured blocklist/allowlist; carried into the
/// selection as a plain value. A `Deny` short-circuits selection to `wgc` and
/// names the reason, so a blocklisted target is never injected (Req 10.2) and
/// the blocklist overrides a reported injection success (Req 10.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SafetyDecision {
    /// Injection may be attempted for this target.
    Allow,
    /// Injection must not be attempted; the reason is either
    /// [`FallbackReason::Blocklisted`] or [`FallbackReason::NotAllowlisted`].
    Deny(FallbackReason),
}

/// Why a session fell back from an intended `hook` to `wgc` (Req 8.4, 14.4).
///
/// `None` means the hook is active and there was no fallback. Every other
/// variant names a concrete gate that blocked the hook. Variants
/// [`FallbackReason::InteropFailed`], [`FallbackReason::TargetExited`], and
/// [`FallbackReason::HookStoppedMidSession`] describe *runtime* fallbacks raised
/// by the session orchestration mid-session rather than by the pure pre-flight
/// selection, but they live here so `NativeShareStats` can report any reason
/// from one enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallbackReason {
    /// The hook is the active Capture_Mode; no fallback occurred.
    None,
    /// The platform is not Windows (Req 13.2).
    NotWindows,
    /// The selected source is a monitor; monitors are always WGC (Req 8.2).
    MonitorSource,
    /// The selected backend's enablement gate is off or the backend is not yet
    /// active-capable (Req 3.3, 3.8).
    BackendDisabled,
    /// The Game_Capture_Hook feature/flag is disabled.
    HookDisabled,
    /// The matching-bitness Hook_Payload / Inject_Helper artifact is missing
    /// (Req 2.5, 12.6).
    MissingArtifact,
    /// The target matches the Process_Blocklist (Req 10.2, 10.6).
    Blocklisted,
    /// A Process_Allowlist is configured and the target does not match it
    /// (Req 10.3).
    NotAllowlisted,
    /// `OpenProcess`/injection was denied, treated as an anti-cheat block
    /// (Req 10.4).
    InjectionDenied,
    /// Injection was attempted but failed (Req 7.4).
    InjectionFailed,
    /// The target renders on a different GPU adapter than the Shared_D3D_Device
    /// (Req 5.4, 9.4).
    CrossAdapter,
    /// Cross_API_Interop could not copy a backbuffer into a Shared_Surface
    /// (Req 4.5).
    InteropFailed,
    /// The target process exited mid-session (Req 9.3).
    TargetExited,
    /// The hook stopped producing frames mid-session (Req 8.3).
    HookStoppedMidSession,
    /// Injection was reported, but the hook never delivered a first frame
    /// before the initial watchdog (typically an anti-cheat refusing the DLL).
    HookNoFirstFrame,
    /// The helper reported success, but the hook's objects never appeared in
    /// the target: the DLL did not load (or did not initialize) there.
    HookNotLoaded,
    /// A signed hook file failed its hash, signature, or signer check.
    HookArtifactUnverified,
    /// The hook reported a `hook_ver_major` this host does not speak.
    HookVersionUnsupported,
    /// Another host (OBS Studio, for instance) is already capturing the
    /// target with the stock hook.
    HookInUse,
    /// Earlier hook attempts on this exact build of the game ended without a
    /// frame (one for an anti-cheat-protected game, two otherwise), so it is
    /// not tried again until the game updates or the record expires (30 days
    /// for an anti-cheat game, 7 for any other).
    HookAttemptSpent,
}

impl FallbackReason {
    /// Stable string form reported through `NativeShareStats` (Req 8.4, 14.4).
    pub fn as_str(self) -> &'static str {
        match self {
            FallbackReason::None => "none",
            FallbackReason::NotWindows => "not_windows",
            FallbackReason::MonitorSource => "monitor_source",
            FallbackReason::BackendDisabled => "backend_disabled",
            FallbackReason::HookDisabled => "hook_disabled",
            FallbackReason::MissingArtifact => "missing_artifact",
            FallbackReason::Blocklisted => "blocklisted",
            FallbackReason::NotAllowlisted => "not_allowlisted",
            FallbackReason::InjectionDenied => "injection_denied",
            FallbackReason::InjectionFailed => "injection_failed",
            FallbackReason::CrossAdapter => "cross_adapter",
            FallbackReason::InteropFailed => "interop_failed",
            FallbackReason::TargetExited => "target_exited",
            FallbackReason::HookStoppedMidSession => "hook_stopped_mid_session",
            FallbackReason::HookNoFirstFrame => "hook_no_first_frame",
            FallbackReason::HookNotLoaded => "hook_not_loaded",
            FallbackReason::HookArtifactUnverified => "hook_artifact_unverified",
            FallbackReason::HookVersionUnsupported => "hook_version_unsupported",
            FallbackReason::HookInUse => "hook_in_use",
            FallbackReason::HookAttemptSpent => "hook_attempt_spent",
        }
    }
}

/// Whether the target process has a `graphics-hook{64,32}.dll` loaded, which
/// for a Vulkan game means a capture layer attached at `vkCreateInstance`. A
/// Vulkan target without it cannot be hooked, so injecting would only put a
/// DLL in the game for nothing.
pub fn process_has_hook_module(pid: u32) -> bool {
    process_hook_module_path(pid).is_some()
}

/// The full path of the `graphics-hook{64,32}.dll` loaded in `pid`, if any.
pub fn process_hook_module_path(pid: u32) -> Option<PathBuf> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    let Ok(snapshot) =
        (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid) })
    else {
        return None;
    };
    let mut found = None;
    let mut entry = MODULEENTRY32W {
        dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
        ..Default::default()
    };
    let mut walk = unsafe { Module32FirstW(snapshot, &mut entry) };
    while walk.is_ok() {
        let len = entry
            .szModule
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szModule.len());
        let module = String::from_utf16_lossy(&entry.szModule[..len]);
        if module.eq_ignore_ascii_case("graphics-hook64.dll")
            || module.eq_ignore_ascii_case("graphics-hook32.dll")
        {
            let len = entry
                .szExePath
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(entry.szExePath.len());
            found = Some(PathBuf::from(String::from_utf16_lossy(
                &entry.szExePath[..len],
            )));
            break;
        }
        entry = MODULEENTRY32W {
            dwSize: std::mem::size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        walk = unsafe { Module32NextW(snapshot, &mut entry) };
    }
    unsafe {
        let _ = CloseHandle(snapshot);
    }
    found
}
