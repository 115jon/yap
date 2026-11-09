//! What detection costs on this PC: the catalog's load time, its peak and
//! kept heap, then the detector running over the real process list for a
//! while (`probe [seconds]`, 120 by default), with its thread's CPU time.
//! Prints counts and timings only, never a process or game name.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Counting;

static NOW: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator and only counts.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let now = NOW.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK.fetch_max(now, Ordering::Relaxed);
        // SAFETY: same layout as asked.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        NOW.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: allocated above with this layout.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOC: Counting = Counting;

fn kib(bytes: usize) -> String {
    format!("{:.0} KiB", bytes as f64 / 1024.0)
}

/// The cycle counter's rate, from a busy 200 ms of this thread.
#[cfg(windows)]
fn tsc_ghz() -> f64 {
    let read = || {
        let mut cycles = 0u64;
        // SAFETY: the pseudo handle needs no closing.
        unsafe {
            windows_sys::Win32::System::WindowsProgramming::QueryThreadCycleTime(
                windows_sys::Win32::System::Threading::GetCurrentThread(),
                &mut cycles,
            )
        };
        cycles
    };
    let start = (Instant::now(), read());
    let mut x = 0u64;
    while start.0.elapsed() < Duration::from_millis(200) {
        x = x.wrapping_mul(31).wrapping_add(1);
    }
    std::hint::black_box(x);
    (read() - start.1) as f64 / start.0.elapsed().as_secs_f64() / 1e9
}

fn main() {
    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(120);

    let before = NOW.load(Ordering::Relaxed);
    PEAK.store(before, Ordering::Relaxed);
    let started = Instant::now();
    let catalog = yap_games::Catalog::from_gzip(yap_games::BUNDLED_CATALOG).expect("catalog");
    let took = started.elapsed();
    let kept = NOW.load(Ordering::Relaxed) - before;
    let peak = PEAK.load(Ordering::Relaxed) - before;
    println!(
        "catalog: {} apps, gzip {} in the exe, load {} ms, heap kept {} (index's own count {}), peak {}",
        catalog.len(),
        kib(yap_games::BUNDLED_CATALOG.len()),
        took.as_millis(),
        kib(kept),
        kib(catalog.heap_bytes()),
        kib(peak),
    );
    drop(catalog);

    #[cfg(windows)]
    {
        use std::sync::{Arc, Mutex};
        let changes = Arc::new(Mutex::new(0u32));
        let counted = Arc::clone(&changes);
        let detector = yap_games::detector::Detector::spawn(
            yap_games::detector::Config {
                trace: std::env::var("YAP_TRACE_GAMES").is_ok_and(|v| v == "1"),
                ..Default::default()
            },
            yap_games::Rules {
                enabled: true,
                ..Default::default()
            },
            Box::new(move |_| *counted.lock().unwrap() += 1),
        )
        .expect("detector");
        let heap_start = NOW.load(Ordering::Relaxed);
        let wall = Instant::now();
        std::thread::sleep(Duration::from_secs(seconds));
        let stats = detector.stats();
        let wall = wall.elapsed();
        let cores = std::thread::available_parallelism().map_or(1, |n| n.get()) as f64;
        // Cycles to time at the nominal clock: QueryPerformanceFrequency
        // is not the TSC, so time a known sleep in cycles of this thread's
        // busy loop instead.
        let ghz = tsc_ghz();
        let cycle_secs = stats.cycles as f64 / (ghz * 1e9);
        println!(
            "detector: {:.0} s, {} scans, {} processes opened, {} changes; in scans {:.1} ms wall ({:.2} ms a scan); thread {:.0} Mcycles = {:.1} ms at {:.2} GHz = {:.4} % of one core, {:.5} % of {} cores; sampled CPU {:.1} ms; catalog {}, heap now {}",
            wall.as_secs_f64(),
            stats.scans,
            stats.opened,
            changes.lock().unwrap(),
            stats.busy.as_secs_f64() * 1000.0,
            stats.busy.as_secs_f64() * 1000.0 / stats.scans.max(1) as f64,
            stats.cycles as f64 / 1e6,
            cycle_secs * 1000.0,
            ghz,
            cycle_secs / wall.as_secs_f64() * 100.0,
            cycle_secs / wall.as_secs_f64() * 100.0 / cores,
            cores,
            stats.cpu.as_secs_f64() * 1000.0,
            kib(stats.catalog_bytes),
            kib(NOW.load(Ordering::Relaxed).saturating_sub(heap_start.min(NOW.load(Ordering::Relaxed)))),
        );
        let start = Instant::now();
        let programs = yap_games::detector::running_programs(None);
        println!(
            "running programs: {} listed in {} ms",
            programs.len(),
            start.elapsed().as_millis()
        );
        drop(detector);
    }
}
