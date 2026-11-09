//! Live check of the game capture hook against our own test target only.
//!
//! Starts `hook_target` (a D3D11 window this probe owns), runs one hook
//! attempt on its window exactly as a share would, and reports whether
//! hooked frames arrived and what colour they were. Nothing else is ever
//! injected: the target's pid is checked before the attempt starts.
//!
//! ```text
//! YAP_OBS_SIGNED_DIR=<folder with the six OBS-signed files> \
//!   cargo run -p yap-capture --example hook_probe -- <path to hook_target.exe> <data dir> \
//!   [--fp16] [--expect <b>,<g>,<r>]
//! ```
//!
//! `--fp16` has the target render scRGB (FP16) magenta at twice SDR white;
//! the hook's frames go through the HDR tone map. `--expect` is the centre
//! pixel to pass with (default magenta, 255,0,255).

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
    let rest: Vec<String> = args.collect();
    let fp16 = rest.iter().any(|a| a == "--fp16");
    let expect: [u8; 4] = rest
        .iter()
        .skip_while(|a| *a != "--expect")
        .nth(1)
        .map(|v| {
            let c: Vec<u8> = v
                .split(',')
                .map(|c| c.trim().parse().expect("a byte"))
                .collect();
            [c[0], c[1], c[2], 255]
        })
        .unwrap_or([255, 0, 255, 255]);
    yap_capture::set_data_dir(data.into());

    let mut child = Command::new(&target)
        .args(["--seconds", "40"])
        .args(fp16.then_some("--fp16"))
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
    let ok = stats.frames > 0 && stats.center == expect;
    println!(
        "{}",
        if ok {
            format!("PASS: hooked frames of {expect:?}")
        } else {
            format!("FAIL: expected {expect:?}")
        }
    );
    std::process::exit(if ok { 0 } else { 1 });
}

#[cfg(not(windows))]
fn main() {}
