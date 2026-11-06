//! Live check of the game capture hook against our own test target only.
//!
//! Starts `hook_target` (a D3D11 window this probe owns), runs one hook
//! attempt on its window exactly as a share would, and reports whether
//! hooked frames arrived and what colour they were. Nothing else is ever
//! injected: the target's pid is checked before the attempt starts.
//!
//! ```text
//! YAP_OBS_SIGNED_DIR=<folder with the six OBS-signed files> \
//!   cargo run -p yap-capture --example hook_probe -- <path to hook_target.exe> <data dir>
//! ```

#[cfg(windows)]
fn main() {
    use std::io::BufRead;
    use std::process::{Command, Stdio};
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let mut args = std::env::args().skip(1);
    let target = args.next().expect("the hook_target executable");
    let data = args.next().expect("a data folder for the probe");
    yap_capture::set_data_dir(data.into());

    let mut child = Command::new(&target)
        .args(["--seconds", "40"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("start the target");
    let mut line = String::new();
    std::io::BufReader::new(child.stdout.take().expect("stdout"))
        .read_line(&mut line)
        .expect("the target's hwnd");
    let hwnd: isize = line
        .trim()
        .strip_prefix("hwnd ")
        .and_then(|v| v.parse().ok())
        .expect("hwnd line");
    let mut pid = 0u32;
    // SAFETY: a plain query.
    unsafe { GetWindowThreadProcessId(HWND(hwnd as *mut _), Some(&mut pid)) };
    assert_eq!(pid, child.id(), "the window must be our own target's");
    println!("target pid {pid} hwnd {hwnd:#x}");

    let gate = Arc::new(AtomicBool::new(false));
    let quality = yap_voice::ShareQuality {
        width: 1280,
        height: 720,
        bitrate: 2_500_000,
        fps: 30,
    };
    let started = Instant::now();
    let hook = yap_capture::hook::attempt::spawn(hwnd, quality, Arc::clone(&gate))
        .expect("the hook thread");
    let mut first = None;
    while started.elapsed() < Duration::from_secs(15) && !hook.finished() {
        let stats = hook.stats();
        if stats.frames > 0 && first.is_none() {
            first = Some(started.elapsed());
        }
        if stats.frames >= 60 {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let stats = hook.stats();
    println!(
        "first frame after {:?}; {} frames; centre BGRA {:?}; gate {}",
        first,
        stats.frames,
        stats.center,
        gate.load(std::sync::atomic::Ordering::Acquire)
    );
    drop(hook);
    let _ = child.kill();
    let _ = child.wait();
    let ok = stats.frames > 0 && stats.center == [255, 0, 255, 255];
    println!(
        "{}",
        if ok {
            "PASS: hooked magenta frames"
        } else {
            "FAIL"
        }
    );
    std::process::exit(if ok { 0 } else { 1 });
}

#[cfg(not(windows))]
fn main() {}
