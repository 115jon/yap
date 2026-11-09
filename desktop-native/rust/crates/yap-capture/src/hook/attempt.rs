//! One hook attempt for a shared window, on its own thread (the desktop
//! app's `attempt_hook_injection` and `run_hook_capture_loop`, ported).
//!
//! The share starts on WGC as always. This thread runs the safety checks,
//! verifies and stages the signed files, starts the IPC objects, runs OBS's
//! `inject-helper` and waits for the first hooked frame. From then on it
//! pushes the hook's frames and holds the WGC session's gate shut, so WGC
//! frames are dropped. Whenever the hook goes quiet for a moment the gate
//! opens again and WGC carries the share; when the hook gives up, WGC simply
//! goes on. A hook that never delivers a frame is remembered
//! (`hook_policy`), so the same build of a game is not tried again for a
//! while.

use std::path::Path;
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{CloseHandle, HWND};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;
use yap_voice::ShareQuality;

use super::blocklist::{anti_cheat_marker_near, default_blocklist, safety_decision};
use super::hook_policy::{self, choose_flavor, FlavorChoice, FlavorInputs};
use super::inject::{
    detect_bitness, detect_graphics_api, load_all_graphics_offsets, plan_injection,
    poll_async_helper, run_inject_helper, spawn_inject_helper_async, Bitness, InjectionMode,
    ObsArtifacts,
};
use super::obs_ipc::{
    foreign_obs_hook_present, stock_host_active, HookFlavor, ObsIpcChannel, DEFAULT_FRAME_WAIT_MS,
};
use super::reader::HookReader;
use super::signed_hook::{self, VerifiedSet};
use super::{
    process_has_hook_module, process_hook_module_path, remote_policy, FallbackReason,
    GraphicsApiBackend, SafetyDecision,
};
use crate::stop::Stop;

/// How long the hook has to deliver its first frame. The stock DLL commits
/// to the first API whose hook installs and retries only every 4 s, so a
/// DX11/12 game that also loads `d3d9.dll` can take 4-5 s (the desktop
/// app's `hook_first_frame_timeout_ms` for the signed hook).
const FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(6);
/// The hook counts as delivering while its last frame is this recent; past
/// it, WGC frames flow again until the hook catches up.
const HOOK_FRESH: Duration = Duration::from_millis(500);
const STATS_EVERY: Duration = Duration::from_secs(5);

/// What the hook has delivered, for logs and the probe.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HookStats {
    pub frames: u64,
    /// The last frame's centre pixel, BGRA.
    pub center: [u8; 4],
    /// The hook is the live source right now.
    pub live: bool,
}

/// A running attempt. Dropping it stops the thread, which stops the hook
/// (the game keeps the DLL loaded, dormant, as with OBS) and reopens the
/// WGC gate.
pub struct HookCapture {
    stop: Arc<Stop>,
    quality: Arc<Mutex<ShareQuality>>,
    stats: Arc<Mutex<HookStats>>,
    thread: Option<JoinHandle<()>>,
}

impl HookCapture {
    pub fn set_quality(&self, quality: ShareQuality) {
        *self.quality.lock().unwrap_or_else(PoisonError::into_inner) = quality;
    }

    pub fn stats(&self) -> HookStats {
        *self.stats.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether the attempt has ended (refused, failed, or stopped).
    pub fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
}

/// How long dropping waits for the thread to wind down. The thread checks
/// the stop signal between its steps and on every frame, but one step (the
/// first `get-graphics-offsets` run) can take up to 5 s and is not
/// interruptible. A drop on the voice driver's async path must not wait for
/// it, so past this the thread is left to finish on its own: it sees the
/// stop signal at its next step and cleans up (helper killed, reader
/// stopped, gate closed) without a caller.
const JOIN_GRACE: Duration = Duration::from_millis(250);

impl Drop for HookCapture {
    fn drop(&mut self) {
        self.stop.stop();
        let Some(thread) = self.thread.take() else {
            return;
        };
        let deadline = Instant::now() + JOIN_GRACE;
        while !thread.is_finished() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(5));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

/// Starts an attempt on the window `hwnd`. `gate` is the WGC session's: set
/// while the hook is the live source.
pub fn spawn(
    hwnd: isize,
    quality: ShareQuality,
    gate: Arc<AtomicBool>,
) -> Result<HookCapture, String> {
    let stop = Arc::new(Stop::default());
    let quality = Arc::new(Mutex::new(quality));
    let stats = Arc::new(Mutex::new(HookStats::default()));
    let thread = {
        let (stop, quality, stats) = (Arc::clone(&stop), Arc::clone(&quality), Arc::clone(&stats));
        std::thread::Builder::new()
            .name("yap-capture-hook".to_owned())
            .spawn(move || {
                run(hwnd, &stop, &quality, &stats, &gate);
                gate.store(false, Ordering::Release);
            })
            .map_err(|e| format!("could not start the hook thread: {e}"))?
    };
    Ok(HookCapture {
        stop,
        quality,
        stats,
        thread: Some(thread),
    })
}

/// Everything a started hook needs on the frame loop.
struct Prepared {
    reader: HookReader,
    helper: Option<Child>,
    /// Held open until the helper has exited.
    verified: Option<VerifiedSet>,
    exe: String,
    pid: u32,
}

/// Why no hook: logged, and WGC carries the share.
struct Refusal(FallbackReason, String);

fn refuse(reason: FallbackReason, why: impl Into<String>) -> Refusal {
    Refusal(reason, why.into())
}

/// Ends `prepare` as soon as the share has stopped, before the next step
/// touches the game or spawns anything. [`FallbackReason::None`] marks it.
fn check_stop(stop: &Stop) -> Result<(), Refusal> {
    if stop.is_stopped() {
        Err(refuse(FallbackReason::None, "the share stopped"))
    } else {
        Ok(())
    }
}

fn host_bitness() -> Bitness {
    if cfg!(target_pointer_width = "64") {
        Bitness::X64
    } else {
        Bitness::X86
    }
}

fn frame_interval_ns(fps: u32) -> u64 {
    if fps == 0 {
        0
    } else {
        1_000_000_000 / u64::from(fps)
    }
}

fn run(
    hwnd: isize,
    stop: &Stop,
    quality: &Mutex<ShareQuality>,
    stats: &Mutex<HookStats>,
    gate: &AtomicBool,
) {
    let fps = quality.lock().unwrap_or_else(PoisonError::into_inner).fps;
    let mut prepared = match prepare(hwnd, fps, stop) {
        Ok(prepared) => prepared,
        Err(Refusal(FallbackReason::None, _)) => {
            eprintln!("[hook] stopped before the hook started");
            return;
        }
        Err(Refusal(reason, why)) => {
            eprintln!(
                "[hook] not hooking ({}): {why}; WGC carries the share",
                reason.as_str()
            );
            return;
        }
    };
    let started = Instant::now();
    let mut frames: u64 = 0;
    let mut last_frame: Option<Instant> = None;
    let mut last_stats = Instant::now();
    let mut interval = frame_interval_ns(fps);
    let end = loop {
        if stop.is_stopped() {
            break FallbackReason::None;
        }
        if let Some(child) = prepared.helper.as_mut() {
            if let Some(outcome) = poll_async_helper(child) {
                prepared.helper = None;
                prepared.verified = None;
                if !outcome.is_success() && frames == 0 {
                    break FallbackReason::InjectionFailed;
                }
            }
        }
        if frames == 0 && started.elapsed() > FIRST_FRAME_TIMEOUT {
            hook_policy::record_hook_failure(&prepared.exe);
            break FallbackReason::HookNoFirstFrame;
        }
        if frames > 0 && prepared.reader.channel().target_exited() {
            break FallbackReason::TargetExited;
        }
        let now_quality = *quality.lock().unwrap_or_else(PoisonError::into_inner);
        let wanted = frame_interval_ns(now_quality.fps);
        if wanted != interval {
            prepared.reader.channel().set_frame_interval(wanted);
            interval = wanted;
        }
        match prepared.reader.next_frame(now_quality) {
            Ok(true) => {
                frames += 1;
                last_frame = Some(Instant::now());
                if frames == 1 {
                    hook_policy::record_hook_success(&prepared.exe);
                    eprintln!(
                        "[hook] first frame from pid {} after {} ms; the hook carries the share",
                        prepared.pid,
                        started.elapsed().as_millis()
                    );
                }
                gate.store(true, Ordering::Release);
            }
            Ok(false) => {}
            Err(error) => {
                eprintln!("[hook] reading stopped: {error}");
                if frames == 0 {
                    hook_policy::record_hook_failure(&prepared.exe);
                }
                break FallbackReason::HookStoppedMidSession;
            }
        }
        let live = last_frame.is_some_and(|at| at.elapsed() < HOOK_FRESH);
        if !live {
            gate.store(false, Ordering::Release);
        }
        {
            let mut s = stats.lock().unwrap_or_else(PoisonError::into_inner);
            s.frames = frames;
            s.live = live;
            s.center = prepared.reader.center();
        }
        if last_stats.elapsed() >= STATS_EVERY && frames > 0 {
            eprintln!("[hook] pid {}: {frames} frames, live={live}", prepared.pid);
            last_stats = Instant::now();
        }
    };
    gate.store(false, Ordering::Release);
    prepared.reader.stop();
    if let Some(mut child) = prepared.helper.take() {
        // The helper exits on its own once the DLL is in; never leave it.
        let _ = child.kill();
        let _ = child.wait();
    }
    stats.lock().unwrap_or_else(PoisonError::into_inner).live = false;
    eprintln!(
        "[hook] ended after {frames} frames ({}); WGC carries the share",
        if end == FallbackReason::None {
            "stopped"
        } else {
            end.as_str()
        }
    );
}

fn prepare(hwnd: isize, fps: u32, stop: &Stop) -> Result<Prepared, Refusal> {
    // 1. The window's process and thread.
    check_stop(stop)?;
    let mut pid = 0u32;
    // SAFETY: a plain query; a dead handle gives 0.
    let thread_id = unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
    if thread_id == 0 || pid == 0 {
        return Err(refuse(
            FallbackReason::InjectionFailed,
            "the window has no process",
        ));
    }
    // SAFETY: a plain query.
    if pid == unsafe { GetCurrentProcessId() } {
        return Err(refuse(
            FallbackReason::BackendDisabled,
            "the window is ours",
        ));
    }
    let exe = process_image_name(pid).ok_or_else(|| {
        refuse(
            FallbackReason::InjectionFailed,
            "the process cannot be identified",
        )
    })?;
    let bitness = detect_bitness(pid)
        .map_err(|e| refuse(FallbackReason::InjectionFailed, format!("bitness: {e}")))?;

    // 2. The anti-cheat gate, before any file is touched: the built-in
    //    blocklist, the remote one, the learned skips, OBS's preflight.
    check_stop(stop)?;
    if let SafetyDecision::Deny(reason) = safety_decision(&exe, &default_blocklist(), &[]) {
        return Err(refuse(reason, format!("{exe} is on the blocklist")));
    }
    if remote_policy::blocks(&exe) {
        return Err(refuse(
            FallbackReason::Blocklisted,
            format!("{exe} is on the remote blocklist"),
        ));
    }
    let protected = anti_cheat_marker_near(Path::new(&exe)).is_some();
    if hook_policy::learned_skip(&exe, protected) {
        return Err(refuse(
            FallbackReason::HookAttemptSpent,
            format!("the hook delivered nothing on earlier shares of {exe}"),
        ));
    }
    if !openable_like_obs(pid) {
        return Err(refuse(
            FallbackReason::InjectionDenied,
            format!(
                "{exe} (pid {pid}) refuses query and synchronize access (elevated or protected)"
            ),
        ));
    }
    let inputs = FlavorInputs {
        protected,
        other_host_active: stock_host_active(pid),
        own_requested: hook_policy::own_flavor_requested(),
    };
    match choose_flavor(&inputs) {
        FlavorChoice::Use(HookFlavor::ObsSigned) => {}
        FlavorChoice::Use(HookFlavor::Own) => {
            return Err(refuse(
                FallbackReason::MissingArtifact,
                "no fork build in this client",
            ));
        }
        FlavorChoice::Skip(reason) => {
            return Err(refuse(reason, "another host is capturing it"));
        }
    }

    // 3. The API it draws with.
    check_stop(stop)?;
    let backend = detect_graphics_api(pid).unwrap_or(GraphicsApiBackend::Dx11);
    if !backend.is_active_capable() {
        return Err(refuse(
            FallbackReason::BackendDisabled,
            format!("{} is not hooked", backend.as_str()),
        ));
    }

    // 4. The signed files: verified against the lock, staged, held open.
    check_stop(stop)?;
    if signed_hook::bundled_dir().is_none() {
        return Err(refuse(
            FallbackReason::MissingArtifact,
            "the signed hook is not bundled",
        ));
    }
    let verified = signed_hook::ensure_staged()
        .and_then(|dir| signed_hook::open_verified(&dir, &signed_hook::file_names(bitness)))
        .map_err(|e| refuse(FallbackReason::HookArtifactUnverified, e))?;
    let lock = signed_hook::lock();
    eprintln!(
        "[hook] OBS-signed hook verified (OBS {}, hook {}, {bitness:?}, signer {})",
        lock.obs_version, lock.hook_ver, lock.signer_subject
    );
    let artifacts = ObsArtifacts::discover(&verified.dir);
    let strategy = plan_injection(host_bitness(), bitness, &artifacts)
        .map_err(|reason| refuse(reason, "the files for this bitness are missing"))?;

    // 5. A stock hook already in the game (an earlier share, or OBS's own
    //    copy) is attached to, after checking it is OBS-signed.
    check_stop(stop)?;
    let mut attach = false;
    if foreign_obs_hook_present(pid) {
        match process_hook_module_path(pid)
            .as_deref()
            .map(signed_hook::is_obs_signed_module)
        {
            Some(Ok(_)) => attach = true,
            _ => {
                return Err(refuse(
                    FallbackReason::HookInUse,
                    "a hook that is not OBS-signed is already in it",
                ));
            }
        }
    }
    if backend == GraphicsApiBackend::Vulkan && !process_has_hook_module(pid) {
        return Err(refuse(
            FallbackReason::BackendDisabled,
            "a Vulkan game is captured only through a capture layer, which this client does not register",
        ));
    }

    // 6. The present offsets, from OBS's own get-graphics-offsets (the one
    //    step that can run for seconds, so the last check before it matters).
    check_stop(stop)?;
    let offsets = load_all_graphics_offsets(&artifacts, bitness, pid)
        .ok_or_else(|| refuse(FallbackReason::InjectionFailed, "no graphics offsets"))?;
    if matches!(backend, GraphicsApiBackend::Dx11 | GraphicsApiBackend::Dx12)
        && !offsets.dxgi.hookable()
    {
        return Err(refuse(
            FallbackReason::BackendDisabled,
            "the DXGI offsets are not hookable",
        ));
    }

    // 7. The IPC objects first (the stock DLL looks for them as it loads),
    //    then the helper, as OBS does.
    check_stop(stop)?;
    let channel = ObsIpcChannel::start_flavor(
        HookFlavor::ObsSigned,
        pid,
        offsets,
        frame_interval_ns(fps),
        DEFAULT_FRAME_WAIT_MS,
    )
    .map_err(|e| refuse(FallbackReason::InjectionFailed, format!("IPC: {e:?}")))?;
    // Never inject into a game for a share that has already ended.
    check_stop(stop)?;
    let helper = if attach {
        None
    } else {
        match spawn_inject_helper_async(strategy, &artifacts, InjectionMode::Safe { thread_id }) {
            Ok(child) => Some(child),
            Err(_) => {
                let outcome =
                    run_inject_helper(strategy, &artifacts, InjectionMode::Direct { pid });
                if !outcome.is_success() {
                    return Err(refuse(
                        FallbackReason::InjectionFailed,
                        format!("{outcome:?}"),
                    ));
                }
                None
            }
        }
    };
    eprintln!(
        "[hook] {} pid {pid} ({}, {})",
        if attach {
            "attached to the resident hook in"
        } else {
            "injecting into"
        },
        backend.as_str(),
        Path::new(&exe)
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    );
    let reader =
        HookReader::new(channel, hwnd).map_err(|e| refuse(FallbackReason::InteropFailed, e))?;
    Ok(Prepared {
        reader,
        helper,
        verified: Some(verified),
        exe,
        pid,
    })
}

/// The process's full image path (`None`: it cannot be identified, and is
/// not injected).
fn process_image_name(pid: u32) -> Option<String> {
    // SAFETY: the handle is closed below; the buffer outlives the call.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let result = QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(process);
        result.ok()?;
        Some(String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// OBS's preflight (`OpenProcess(PROCESS_QUERY_INFORMATION | SYNCHRONIZE)`):
/// a process that refuses it is elevated or anti-cheat protected, and the
/// hook never loads there.
fn openable_like_obs(pid: u32) -> bool {
    // SAFETY: the handle is closed at once.
    unsafe {
        match OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SYNCHRONIZE, false, pid) {
            Ok(process) => {
                let _ = CloseHandle(process);
                true
            }
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_returns_before_any_step_once_stopped() {
        let stop = Stop::default();
        stop.stop();
        // Window 0 would otherwise be refused as "no process".
        match prepare(0, 60, &stop) {
            Err(Refusal(reason, _)) => assert_eq!(reason, FallbackReason::None),
            Ok(_) => panic!("a stopped share must not prepare a hook"),
        }
        match prepare(0, 60, &Stop::default()) {
            Err(Refusal(reason, _)) => assert_eq!(reason, FallbackReason::InjectionFailed),
            Ok(_) => panic!("window 0 has no process"),
        }
    }

    #[test]
    fn dropping_a_busy_attempt_does_not_wait_for_it() {
        let stop = Arc::new(Stop::default());
        // A thread stuck in an uninterruptible step (the 5 s helper).
        let thread = std::thread::spawn(|| std::thread::sleep(Duration::from_secs(3)));
        let capture = HookCapture {
            stop: Arc::clone(&stop),
            quality: Arc::new(Mutex::new(ShareQuality::of("720p"))),
            stats: Arc::new(Mutex::new(HookStats::default())),
            thread: Some(thread),
        };
        let started = Instant::now();
        drop(capture);
        assert!(stop.is_stopped());
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
