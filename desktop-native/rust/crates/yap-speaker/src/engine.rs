//! The process-wide speaker and its render thread.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::bus::{Bus, Input};
use crate::wav::WavWriter;
use crate::{FRAME, RATE, SAMPLES};

#[cfg(windows)]
use crate::wasapi::{self, Render, Watch};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Hears every frame as it goes to the device: the call's echo canceller
/// (its far end) and the fake room of `YAP_FAKE_MIC_ECHO`. Called on the
/// render thread; it must not block for long.
pub trait Tap: Send + Sync {
    /// `frame` is about to be written; it is heard `delay_ms` from now
    /// (what the device has queued, its stream latency and a period).
    fn played(&self, frame: &[i16; SAMPLES], delay_ms: u32);
}

/// Opens in a row that got no audio through before [`Speaker::failing`]
/// says so (the call's "You can't hear anyone" banner), and refusals in a
/// row before [`Speaker::refusing`] (the call falls back to the ADM).
pub const NOTICE_AFTER: u32 = 3;

/// Nothing to play and nothing holding the device for this long closes it.
const IDLE_CLOSE: Duration = Duration::from_secs(2);
/// A device that takes nothing for this long (its event stopped firing)
/// counts as lost.
const STALL_LIMIT: Duration = Duration::from_secs(1);
/// After a failed open, the mix runs on the clock this long, then retries.
const RETRY_AFTER: Duration = Duration::from_secs(1);
/// The device buffer asked for: two engine periods.
#[cfg(windows)]
const DEVICE_BUFFER: Duration = Duration::from_millis(20);
/// The longest a render tap records.
const TAP_SECONDS: u32 = 120;
/// How far ahead of the device the mix runs by default, ms.
const DEFAULT_AHEAD_MS: u32 = 20;

/// Opens in a row that got nothing through, and of those the trailing ones
/// where the device refused our stream.
#[derive(Default)]
struct Health {
    failures: AtomicU32,
    refusals: AtomicU32,
}

impl Health {
    fn working(&self) {
        self.failures.store(0, Ordering::Relaxed);
        self.refusals.store(0, Ordering::Relaxed);
    }

    fn failed(&self, refused: bool) {
        let failures = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures == NOTICE_AFTER {
            eprintln!("[speaker] failed {failures} times in a row");
        }
        if refused {
            self.refusals.fetch_add(1, Ordering::Relaxed);
        } else {
            self.refusals.store(0, Ordering::Relaxed);
        }
    }
}

#[derive(Default)]
struct Counters {
    frames: AtomicU64,
    underruns: AtomicU64,
    reopens: AtomicU64,
}

#[derive(Default)]
struct ThreadState {
    running: bool,
    handle: Option<JoinHandle<()>>,
}

/// Why the device loop returned.
enum Step {
    /// Idle long enough: the thread ends.
    Exit,
    /// Open the device (again).
    Reopen,
}

/// Holds the device open (the call, for its length). Dropping it lets the
/// speaker close once nothing plays.
#[must_use = "the device closes again when this is dropped"]
pub struct Hold(&'static Speaker);

impl Drop for Hold {
    fn drop(&mut self) {
        self.0.holds.fetch_sub(1, Ordering::AcqRel);
    }
}

impl std::fmt::Debug for Hold {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Hold")
    }
}

/// The app's one speaker ([`speaker`]).
pub struct Speaker {
    bus: Bus,
    /// Held weakly, as the inputs are.
    taps: Mutex<Vec<Weak<dyn Tap>>>,
    thread: Mutex<ThreadState>,
    /// Set by [`Self::wake`]; an idle thread checks it before it ends.
    woken: AtomicBool,
    holds: AtomicUsize,
    choice: Mutex<Option<String>>,
    #[cfg(windows)]
    watch: Arc<Watch>,
    /// No device at all (`YAP_NULL_SPEAKER=1`, other platforms, tests).
    null: bool,
    ahead_ms: u32,
    health: Health,
    counters: Counters,
    /// The device open now, for the stats.
    device: Mutex<String>,
    delay_ms: AtomicU32,
}

impl std::fmt::Debug for Speaker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Speaker")
            .field("null", &self.null)
            .field("bus", &self.bus)
            .finish_non_exhaustive()
    }
}

/// The process's speaker.
pub fn speaker() -> &'static Speaker {
    static SPEAKER: OnceLock<Speaker> = OnceLock::new();
    SPEAKER.get_or_init(|| {
        let null = !cfg!(windows) || std::env::var("YAP_NULL_SPEAKER").is_ok_and(|v| v == "1");
        if null {
            eprintln!("[speaker] no device (clock-paced mix)");
        }
        let ahead_ms = std::env::var("YAP_SPEAKER_AHEAD_MS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok())
            .map_or(DEFAULT_AHEAD_MS, |ms| ms.clamp(10, 60));
        Speaker::new(null, ahead_ms)
    })
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The render tap's file for this thread: the named path first, then
/// `<name>-2.wav`, `-3`, ...
fn tap_file() -> Option<WavWriter> {
    static OPENED: AtomicU32 = AtomicU32::new(0);
    let path = env_path("YAP_RENDER_TAP")?;
    let n = OPENED.fetch_add(1, Ordering::Relaxed) + 1;
    let path = if n == 1 {
        path
    } else {
        let stem = path.file_stem()?.to_string_lossy().into_owned();
        path.with_file_name(format!("{stem}-{n}.wav"))
    };
    match WavWriter::create(&path, RATE, 2, TAP_SECONDS) {
        Ok(writer) => {
            let epoch_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_millis());
            eprintln!("[speaker] render tap {} from {epoch_ms} ms", path.display());
            Some(writer)
        }
        Err(error) => {
            eprintln!("[speaker] render tap {}: {error}", path.display());
            None
        }
    }
}

/// The idle timer of one render thread.
#[derive(Default)]
struct Idle {
    since: Option<Instant>,
}

impl Speaker {
    fn new(null: bool, ahead_ms: u32) -> Self {
        Self {
            bus: Bus::new(),
            taps: Mutex::new(Vec::new()),
            thread: Mutex::new(ThreadState::default()),
            woken: AtomicBool::new(false),
            holds: AtomicUsize::new(0),
            choice: Mutex::new(None),
            #[cfg(windows)]
            watch: Arc::new(Watch::default()),
            null,
            ahead_ms,
            health: Health::default(),
            counters: Counters::default(),
            device: Mutex::new(String::new()),
            delay_ms: AtomicU32::new(0),
        }
    }

    /// Puts `input` in the mix (it leaves when dropped, or on
    /// [`Self::remove`]). Call [`Self::wake`] once it has something to play.
    pub fn add(&self, input: &Arc<dyn Input>) {
        self.bus.add(input);
    }

    pub fn remove(&self, input: &Arc<dyn Input>) {
        self.bus.remove(input);
    }

    /// Something has sound to play: the render thread starts if it is not
    /// running (and an idle one stays).
    pub fn wake(&'static self) {
        let mut thread = lock(&self.thread);
        self.woken.store(true, Ordering::Release);
        if thread.running {
            return;
        }
        if let Some(old) = thread.handle.take() {
            // Ending: it left the lock with nothing more to do.
            let _ = old.join();
        }
        let spawned = std::thread::Builder::new()
            .name("yap-speaker".into())
            .spawn(move || self.run());
        match spawned {
            Ok(handle) => {
                thread.running = true;
                thread.handle = Some(handle);
            }
            Err(error) => eprintln!("[speaker] no render thread: {error}"),
        }
    }

    /// Keeps the device open while the returned guard lives (the call:
    /// its far end runs whether or not anyone talks).
    pub fn hold(&'static self) -> Hold {
        self.holds.fetch_add(1, Ordering::AcqRel);
        self.wake();
        Hold(self)
    }

    /// `tap` hears every frame from now on (until it is dropped or
    /// removed): the call's echo canceller, the mic test's.
    pub fn add_tap(&self, tap: &Arc<dyn Tap>) {
        let mut taps = lock(&self.taps);
        let weak = Arc::downgrade(tap);
        if !taps.iter().any(|t| t.ptr_eq(&weak)) {
            taps.push(weak);
        }
    }

    pub fn remove_tap(&self, tap: &Arc<dyn Tap>) {
        let weak = Arc::downgrade(tap);
        lock(&self.taps).retain(|t| !t.ptr_eq(&weak) && t.strong_count() > 0);
    }

    /// The output endpoint by id; `None` follows the Windows default
    /// communications device. A change reopens at once.
    pub fn set_device(&self, choice: Option<String>) {
        let choice = choice.filter(|id| !id.is_empty());
        let mut current = lock(&self.choice);
        if *current == choice {
            return;
        }
        eprintln!(
            "[speaker] device: {}",
            choice.as_deref().unwrap_or("default (followed)")
        );
        current.clone_from(&choice);
        #[cfg(windows)]
        {
            *lock(&self.watch.choice) = choice;
            self.watch.reopen.store(true, Ordering::Relaxed);
        }
    }

    /// The chosen endpoint (`None`: the default).
    pub fn device_choice(&self) -> Option<String> {
        lock(&self.choice).clone()
    }

    /// [`NOTICE_AFTER`] opens in a row got nothing through.
    pub fn failing(&self) -> bool {
        self.health.failures.load(Ordering::Relaxed) >= NOTICE_AFTER
    }

    /// [`NOTICE_AFTER`] opens in a row were refused by a device that is
    /// there (it will not take 48 kHz 16-bit).
    pub fn refusing(&self) -> bool {
        self.health.refusals.load(Ordering::Relaxed) >= NOTICE_AFTER
    }

    /// From a frame's tap to the ear, ms, as last measured.
    pub fn delay_ms(&self) -> u32 {
        self.delay_ms.load(Ordering::Relaxed)
    }

    /// The stats line's speaker part; the counts are since the last call.
    pub fn stats(&self) -> String {
        let c = &self.counters;
        format!(
            "speaker[{} frames={} underruns={} reopens={} delay={}ms inputs={}]",
            if self.null {
                "null".to_owned()
            } else {
                lock(&self.device).clone()
            },
            c.frames.swap(0, Ordering::Relaxed),
            c.underruns.swap(0, Ordering::Relaxed),
            c.reopens.swap(0, Ordering::Relaxed),
            self.delay_ms(),
            self.bus.len(),
        )
    }

    /// One frame: mixed, tapped, recorded. `true` if an input plays.
    fn next_frame(
        &self,
        frame: &mut [i16; SAMPLES],
        file: &mut Option<WavWriter>,
        delay_ms: u32,
    ) -> bool {
        let active = self.bus.render(frame);
        if let Some(writer) = file.as_mut() {
            let _ = writer.write(frame);
        }
        {
            let mut taps = lock(&self.taps);
            taps.retain(|t| t.strong_count() > 0);
            for tap in taps.iter().filter_map(Weak::upgrade) {
                tap.played(frame, delay_ms);
            }
        }
        self.delay_ms.store(delay_ms, Ordering::Relaxed);
        self.counters.frames.fetch_add(1, Ordering::Relaxed);
        active
    }

    /// `true` once the thread should end: nothing played or held for
    /// [`IDLE_CLOSE`] and no wake since. Clears `running` when it says so.
    fn idle_over(&self, active: bool, idle: &mut Idle) -> bool {
        if active || self.holds.load(Ordering::Acquire) > 0 {
            idle.since = None;
            return false;
        }
        let since = *idle.since.get_or_insert_with(Instant::now);
        if since.elapsed() < IDLE_CLOSE {
            return false;
        }
        let mut thread = lock(&self.thread);
        if self.woken.swap(false, Ordering::AcqRel) || self.holds.load(Ordering::Acquire) > 0 {
            idle.since = None;
            return false;
        }
        thread.running = false;
        true
    }

    /// The mix on the clock, to no device: forever with no device at all,
    /// else for `limit` (then the caller retries the device).
    fn pace(
        &self,
        frame: &mut [i16; SAMPLES],
        file: &mut Option<WavWriter>,
        idle: &mut Idle,
        limit: Option<Duration>,
    ) -> Step {
        let start = Instant::now();
        let mut sent = 0u64;
        loop {
            if limit.is_some_and(|l| start.elapsed() >= l) {
                return Step::Reopen;
            }
            let due = start + Duration::from_millis(10 * (sent + 1));
            std::thread::sleep(due.saturating_duration_since(Instant::now()));
            let active = self.next_frame(frame, file, 10);
            sent += 1;
            if self.idle_over(active, idle) {
                return Step::Exit;
            }
        }
    }

    fn run(&'static self) {
        let mut frame = [0i16; SAMPLES];
        let mut file = tap_file();
        let mut idle = Idle::default();
        self.woken.store(false, Ordering::Release);
        if self.null {
            let _ = self.pace(&mut frame, &mut file, &mut idle, None);
            eprintln!("[speaker] idle: stopped");
            return;
        }
        #[cfg(windows)]
        self.run_device(&mut frame, &mut file, &mut idle);
        #[cfg(not(windows))]
        let _ = self.pace(&mut frame, &mut file, &mut idle, None);
    }

    #[cfg(windows)]
    fn run_device(
        &'static self,
        frame: &mut [i16; SAMPLES],
        file: &mut Option<WavWriter>,
        idle: &mut Idle,
    ) {
        let _com = wasapi::Com::init();
        let _priority = wasapi::ProAudio::register();
        let enumerator = match wasapi::enumerator() {
            Ok(enumerator) => enumerator,
            Err(error) => {
                eprintln!("[speaker] {}; mixing to nothing", error.message);
                self.health.failed(false);
                let _ = self.pace(frame, file, idle, None);
                return;
            }
        };
        let _notifications = wasapi::Registration::new(&enumerator, Arc::clone(&self.watch));
        loop {
            self.watch.reopen.store(false, Ordering::Relaxed);
            let choice = lock(&self.choice).clone();
            let step = match Render::open(&enumerator, choice.as_deref(), DEVICE_BUFFER) {
                Ok(mut render) => {
                    eprintln!(
                        "[speaker] open {} ({}, {} frame buffer, {} ms ahead, {} ms latency)",
                        render.name,
                        if render.on_default {
                            "default"
                        } else {
                            "chosen"
                        },
                        render.capacity(),
                        self.ahead_ms,
                        render.latency_ms()
                    );
                    *lock(&self.watch.open) = (render.id.clone(), render.on_default);
                    lock(&self.device).clone_from(&render.name);
                    self.play(&mut render, frame, file, idle)
                }
                Err(error) => {
                    eprintln!(
                        "[speaker] unavailable: {}; mixing to nothing for {} ms",
                        error.message,
                        RETRY_AFTER.as_millis()
                    );
                    self.health.failed(error.refused);
                    lock(&self.device).clear();
                    self.pace(frame, file, idle, Some(RETRY_AFTER))
                }
            };
            match step {
                Step::Exit => {
                    eprintln!("[speaker] idle: closed");
                    lock(&self.device).clear();
                    return;
                }
                Step::Reopen => {
                    self.counters.reopens.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Feeds an open device until it is lost, a reopen is asked for, or
    /// the speaker has been idle long enough.
    #[cfg(windows)]
    fn play(
        &self,
        render: &mut Render,
        frame: &mut [i16; SAMPLES],
        file: &mut Option<WavWriter>,
        idle: &mut Idle,
    ) -> Step {
        let ahead = (self.ahead_ms * RATE / 1000).min(render.capacity());
        let latency = render.latency_ms();
        let mut progressed = Instant::now();
        let mut worked = false;
        let lost = |error: String, worked: bool| {
            eprintln!("[speaker] device lost: {error}; reopening");
            if !worked {
                self.health.failed(false);
            }
            Step::Reopen
        };
        loop {
            if self.watch.reopen.swap(false, Ordering::Relaxed) {
                return Step::Reopen;
            }
            if let Err(error) = render.wait(Duration::from_millis(50)) {
                return lost(error, worked);
            }
            let mut wrote = false;
            loop {
                let queued = match render.queued() {
                    Ok(queued) => queued,
                    Err(error) => return lost(error, worked),
                };
                if !wrote && worked && queued == 0 {
                    self.counters.underruns.fetch_add(1, Ordering::Relaxed);
                }
                if queued + FRAME as u32 > ahead {
                    break;
                }
                let delay = (queued + FRAME as u32) * 1000 / RATE + latency;
                let active = self.next_frame(frame, file, delay);
                if let Err(error) = render.write(frame) {
                    return lost(error, worked);
                }
                wrote = true;
                if self.idle_over(active, idle) {
                    return Step::Exit;
                }
            }
            if wrote {
                progressed = Instant::now();
                if !worked {
                    worked = true;
                    self.health.working();
                }
            } else if progressed.elapsed() >= STALL_LIMIT {
                return lost(
                    format!("no audio taken for {} ms", progressed.elapsed().as_millis()),
                    worked,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A constant on both channels, active while `on`.
    struct Dc {
        on: AtomicBool,
    }

    impl Input for Dc {
        fn mix_into(&self, out: &mut [f32; SAMPLES]) -> bool {
            let on = self.on.load(Ordering::Relaxed);
            if on {
                for s in out.iter_mut() {
                    *s += 0.25;
                }
            }
            on
        }
    }

    #[derive(Default)]
    struct Count {
        frames: AtomicU64,
        loud: AtomicU64,
    }

    impl Tap for Count {
        fn played(&self, frame: &[i16; SAMPLES], delay_ms: u32) {
            assert_eq!(delay_ms, 10);
            self.frames.fetch_add(1, Ordering::Relaxed);
            if frame[0] == 8192 {
                self.loud.fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn null_speaker() -> &'static Speaker {
        Box::leak(Box::new(Speaker::new(true, DEFAULT_AHEAD_MS)))
    }

    fn running(s: &Speaker) -> bool {
        lock(&s.thread).running
    }

    #[test]
    fn the_tap_should_hear_every_frame_on_the_clock() {
        let s = null_speaker();
        let tap = Arc::new(Count::default());
        let tap_dyn: Arc<dyn Tap> = tap.clone();
        s.add_tap(&tap_dyn);
        let dc = Arc::new(Dc {
            on: AtomicBool::new(true),
        });
        let input: Arc<dyn Input> = dc.clone();
        s.add(&input);
        let hold = s.hold();
        std::thread::sleep(Duration::from_millis(300));
        let frames = tap.frames.load(Ordering::Relaxed);
        assert!((20..=40).contains(&frames), "{frames} frames in 300 ms");
        assert_eq!(tap.loud.load(Ordering::Relaxed), frames);
        s.remove_tap(&tap_dyn);
        std::thread::sleep(Duration::from_millis(50));
        let after = tap.frames.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(
            tap.frames.load(Ordering::Relaxed),
            after,
            "removed: hears no more"
        );
        drop(hold);
    }

    #[test]
    fn the_thread_should_end_once_idle_and_start_again_on_a_wake() {
        let s = null_speaker();
        let dc = Arc::new(Dc {
            on: AtomicBool::new(true),
        });
        let input: Arc<dyn Input> = dc.clone();
        s.add(&input);
        s.wake();
        std::thread::sleep(Duration::from_millis(100));
        assert!(running(s));
        dc.on.store(false, Ordering::Relaxed);
        std::thread::sleep(IDLE_CLOSE + Duration::from_millis(400));
        assert!(!running(s), "idle for {IDLE_CLOSE:?}: closed");
        dc.on.store(true, Ordering::Relaxed);
        s.wake();
        assert!(running(s));
    }

    #[test]
    fn a_hold_should_keep_an_idle_speaker_running() {
        let s = null_speaker();
        let hold = s.hold();
        std::thread::sleep(IDLE_CLOSE + Duration::from_millis(300));
        assert!(running(s));
        drop(hold);
    }

    #[test]
    fn three_refused_opens_should_say_refusing_and_audio_should_clear_it() {
        let h = Health::default();
        h.failed(true);
        h.failed(true);
        assert!(h.refusals.load(Ordering::Relaxed) < NOTICE_AFTER);
        h.failed(true);
        assert!(h.failures.load(Ordering::Relaxed) >= NOTICE_AFTER);
        assert!(h.refusals.load(Ordering::Relaxed) >= NOTICE_AFTER);
        h.failed(false);
        assert_eq!(
            h.refusals.load(Ordering::Relaxed),
            0,
            "a missing device breaks the run"
        );
        h.working();
        assert_eq!(h.failures.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_new_device_choice_should_ask_for_a_reopen() {
        let s = Speaker::new(true, DEFAULT_AHEAD_MS);
        s.set_device(Some("{id}".into()));
        assert_eq!(s.device_choice().as_deref(), Some("{id}"));
        #[cfg(windows)]
        assert!(s.watch.reopen.swap(false, Ordering::Relaxed));
        s.set_device(Some("{id}".into()));
        #[cfg(windows)]
        assert!(
            !s.watch.reopen.load(Ordering::Relaxed),
            "unchanged: no reopen"
        );
        s.set_device(Some(String::new()));
        assert_eq!(s.device_choice(), None, "empty means the default");
    }
}
