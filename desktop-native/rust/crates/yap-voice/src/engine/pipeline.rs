//! The call's own audio path (on unless `YAP_ADM_AUDIO=1`): libwebrtc's
//! device module mixes every remote track into one playout and gives no
//! hook after its processing, so neither per-peer panning nor RNNoise fits
//! inside it. Here the call does its audio itself:
//!
//! - **Mic**: a capture thread reads the mic through WASAPI (48 kHz mono),
//!   runs WebRTC's APM on each 10 ms frame (echo cancellation, noise
//!   suppression, gain control, high-pass), then RNNoise when it is on, and
//!   hands the frame to the mic's `NativeAudioSource` (unbuffered: straight
//!   to the encoder).
//! - **Speaker**: every remote audio track gets a `NativeAudioStream` (its
//!   decoded audio, 48 kHz stereo) into the [`Mixer`] with its gain and pan.
//!   A render thread mixes 10 ms at a time, writes it to the speaker through
//!   WASAPI and hands the same frame to the APM as the echo canceller's far
//!   end (`ProcessReverseStream`): what AEC cancels is exactly what plays.
//!   The echo canceller's delay hint is the speaker's queued audio plus
//!   both clients' stream latency and device period.
//! - libwebrtc's ADM stays acquired only to list the devices (the same
//!   endpoint ids as before); its recording and playout are off, so it
//!   opens nothing and its synthetic playout clock pumps the receive
//!   streams into our sinks.
//!
//! Both device threads run at MMCSS "Pro Audio" priority. A device that
//! errors, or goes a second without delivering or taking audio (its event
//! stops firing), is reopened. After [`NOTICE_AFTER`] tries in a row that
//! got no audio through, the stats tell the call ([`Pipeline::failing`]),
//! which shows its "We can't hear you" or "You can't hear anyone" banner.
//! When those tries were the device refusing our stream (it is there, but
//! will not open as 48 kHz 16-bit), the next call, and every call after it
//! in this process, goes back to libwebrtc's ADM, which opens devices in
//! their own mix format ([`settle_path`]). Never mid-call: a call keeps the
//! path it opened on.
//!
//! One call at a time: the path is process-wide, like the ADM was.
//!
//! Test hooks: `YAP_FAKE_MIC=<wav>` (48 kHz 16-bit, looped) replaces the
//! mic, `YAP_FAKE_MIC_ECHO=<gain>` adds our own speaker mix to it 60 ms
//! late (an acoustic echo), `YAP_NULL_SPEAKER=1` plays to no device (paced
//! by the clock, the echo canceller still fed), `YAP_TEST_PAN=<-1..1>`
//! puts every remote track at that pan, and `YAP_MIC_RAW_TAP`,
//! `YAP_MIC_TAP` and `YAP_RENDER_TAP` (`=<wav>`) record the mic as it came
//! in, the mic as it is sent, and the speaker mix.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use futures_util::FutureExt;
use libwebrtc::audio_frame::AudioFrame;
use libwebrtc::audio_source::native::NativeAudioSource;
use libwebrtc::native::apm::AudioProcessingModule;

use crate::audio::Processing;
use crate::dsp::denoise::Denoiser;
use crate::dsp::mix::{Mixer, Stream, SAMPLES};
use crate::dsp::wav::{self, WavWriter};
use crate::dsp::{FRAME, RATE};

#[cfg(windows)]
use super::wasapi::{Capture, Com, ProAudio, Render};

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether calls use the own path, decided per process (`YAP_ADM_AUDIO=1`
/// turns it off) and turned off by [`settle_path`] after a device kept
/// refusing it.
fn own_path() -> &'static AtomicBool {
    static ON: OnceLock<AtomicBool> = OnceLock::new();
    ON.get_or_init(|| {
        let adm = std::env::var("YAP_ADM_AUDIO").is_ok_and(|v| v == "1");
        if adm {
            eprintln!("[voice] YAP_ADM_AUDIO=1: libwebrtc's device module plays and records");
        }
        AtomicBool::new(cfg!(windows) && !adm)
    })
}

/// The own path is on. Constant for the length of a call.
pub(crate) fn enabled() -> bool {
    own_path().load(Ordering::Relaxed)
}

/// A device refused our stream [`NOTICE_AFTER`] times in a row: the next
/// call goes to the ADM.
static FALL_BACK: AtomicBool = AtomicBool::new(false);

/// Called as a call opens its media, with no other call open: a fallback a
/// failing device asked for takes effect now, for the rest of the process.
pub(crate) fn settle_path() {
    if FALL_BACK.load(Ordering::Relaxed) && own_path().swap(false, Ordering::Relaxed) {
        eprintln!(
            "[voice] WASAPI kept refusing the audio device: libwebrtc's device module \
             plays and records from now on (no RNNoise, no panning)"
        );
    }
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// A tap's file: the path the variable names for its first recording in
/// this process, then `<name>-2.wav`, `-3`, ... (a standby after a call
/// must not overwrite the call's recording).
fn tap_path(var: &str) -> Option<PathBuf> {
    static OPENED: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let path = env_path(var)?;
    let mut opened = lock(&OPENED);
    let n = opened.iter().filter(|v| *v == var).count() + 1;
    opened.push(var.to_owned());
    if n == 1 {
        return Some(path);
    }
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    Some(path.with_file_name(format!("{stem}-{n}.wav")))
}

/// The longest a tap records.
const TAP_SECONDS: u32 = 120;

/// The simulated echo's delay (`YAP_FAKE_MIC_ECHO`): 60 ms.
const ECHO_DELAY_FRAMES: usize = 6;

/// How far ahead of the speaker the render thread keeps the device buffer:
/// 30 ms (three frames).
const RENDER_AHEAD: u32 = 3 * FRAME as u32;

/// A device that delivers or takes nothing for this long, without an error
/// (its event stopped firing), counts as stopped and is reopened.
const STALL_LIMIT: Duration = Duration::from_secs(1);

/// Tries in a row that got no audio through before the call is told (and,
/// when the device refused each time, before the next call falls back).
pub(crate) const NOTICE_AFTER: u32 = 3;

/// Why a device did not open.
#[derive(Debug)]
pub(super) struct OpenError {
    pub(super) message: String,
    /// The endpoint is there but refused our stream (48 kHz 16-bit with
    /// `AUTOCONVERTPCM`); the ADM, which opens it in its own mix format,
    /// may still get it. `false`: no such device (the ADM would not find
    /// one either).
    pub(super) refused: bool,
}

impl OpenError {
    pub(super) fn missing(message: String) -> Self {
        Self {
            message,
            refused: false,
        }
    }

    pub(super) fn refused(message: String) -> Self {
        Self {
            message,
            refused: true,
        }
    }
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

/// One side's tries in a row that got no audio through.
#[derive(Default)]
struct DeviceHealth {
    failures: AtomicU32,
    /// Of those, the trailing ones where the device refused our stream.
    refusals: AtomicU32,
}

impl DeviceHealth {
    /// Audio went through.
    fn working(&self) {
        self.failures.store(0, Ordering::Relaxed);
        self.refusals.store(0, Ordering::Relaxed);
    }

    /// A try failed; `true` the moment the ADM should take over (the
    /// device refused [`NOTICE_AFTER`] times in a row).
    fn failed(&self, side: &str, refused: bool) -> bool {
        let failures = self.failures.fetch_add(1, Ordering::Relaxed) + 1;
        if failures == NOTICE_AFTER {
            eprintln!("[voice] the {side} failed {failures} times in a row: telling the call");
        }
        if !refused {
            self.refusals.store(0, Ordering::Relaxed);
            return false;
        }
        self.refusals.fetch_add(1, Ordering::Relaxed) + 1 == NOTICE_AFTER
    }

    fn failing(&self) -> bool {
        self.failures.load(Ordering::Relaxed) >= NOTICE_AFTER
    }
}

/// Asks for the ADM from the next call on (`side` refused our stream).
fn fall_back(side: &str) {
    if enabled() && !FALL_BACK.swap(true, Ordering::Relaxed) {
        eprintln!("[voice] the {side} keeps refusing WASAPI: the next call uses the ADM");
    }
}

/// Watches a device for progress: past [`STALL_LIMIT`] without any, it
/// counts as stopped.
struct Stall {
    since: Instant,
}

impl Stall {
    fn new(now: Instant) -> Self {
        Self { since: now }
    }

    fn check(&mut self, progressed: bool, now: Instant) -> Result<(), String> {
        if progressed {
            self.since = now;
            return Ok(());
        }
        let idle = now.saturating_duration_since(self.since);
        if idle >= STALL_LIMIT {
            return Err(format!("no audio for {} ms", idle.as_millis()));
        }
        Ok(())
    }
}

/// WebRTC's APM with the switches it was built for.
struct Apm {
    module: AudioProcessingModule,
    /// echo cancellation, noise suppression, gain control.
    config: (bool, bool, bool),
}

impl Apm {
    fn new((echo, noise, gain): (bool, bool, bool)) -> Self {
        // The high-pass filter goes with the rest: high fidelity turns all
        // of it off, as the browser does.
        let high_pass = echo || noise || gain;
        Self {
            module: AudioProcessingModule::new(echo, gain, high_pass, noise),
            config: (echo, noise, gain),
        }
    }

    fn active(&self) -> bool {
        self.config.0 || self.config.1 || self.config.2
    }
}

/// Per-frame CPU of the processing, summed (nanoseconds) for the stats.
#[derive(Default)]
struct Costs {
    capture_frames: AtomicU64,
    apm_ns: AtomicU64,
    rnnoise_ns: AtomicU64,
    rnnoise_frames: AtomicU64,
    render_frames: AtomicU64,
    mix_ns: AtomicU64,
    reverse_ns: AtomicU64,
}

fn add_elapsed(counter: &AtomicU64, since: Instant) {
    let ns = u64::try_from(since.elapsed().as_nanos()).unwrap_or(u64::MAX);
    counter.fetch_add(ns, Ordering::Relaxed);
}

/// What both threads share.
struct Shared {
    mixer: Mixer,
    apm: Mutex<Apm>,
    noise_reduction: AtomicBool,
    /// Where the processed mic goes (`None`: nowhere yet, or stopped).
    mic: Mutex<Option<NativeAudioSource>>,
    /// From handing a frame to the APM's far end to hearing it, ms: the
    /// speaker's queued audio when it was written, its stream latency and
    /// period.
    render_delay_ms: AtomicU32,
    /// From the mic to the APM, ms: the mic's stream latency and period.
    capture_delay_ms: AtomicU32,
    costs: Costs,
    /// The speaker mix, mono, for `YAP_FAKE_MIC_ECHO`; fed only while
    /// `echo_hook` is on.
    echo: Mutex<VecDeque<i16>>,
    echo_hook: AtomicBool,
    mic_health: DeviceHealth,
    speaker_health: DeviceHealth,
}

impl Shared {
    fn new() -> Self {
        Self {
            mixer: Mixer::new(),
            apm: Mutex::new(Apm::new((true, true, true))),
            noise_reduction: AtomicBool::new(false),
            mic: Mutex::new(None),
            render_delay_ms: AtomicU32::new(30),
            capture_delay_ms: AtomicU32::new(10),
            costs: Costs::default(),
            echo: Mutex::new(VecDeque::new()),
            echo_hook: AtomicBool::new(false),
            mic_health: DeviceHealth::default(),
            speaker_health: DeviceHealth::default(),
        }
    }
}

/// A running device thread.
struct Worker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// The endpoint it runs on (`None`: the default).
    device: Option<String>,
}

impl Worker {
    fn spawn(
        name: &str,
        device: Option<String>,
        run: impl FnOnce(Option<String>, Arc<AtomicBool>) + Send + 'static,
    ) -> Option<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (stop, device) = (Arc::clone(&stop), device.clone());
            std::thread::Builder::new()
                .name(name.to_owned())
                .spawn(move || run(device, stop))
        };
        match thread {
            Ok(thread) => Some(Self {
                stop,
                thread: Some(thread),
                device,
            }),
            Err(error) => {
                eprintln!("[voice] could not start {name}: {error}");
                None
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Default)]
struct Threads {
    capture: Option<Worker>,
    render: Option<Worker>,
    /// The endpoints the device choice resolves to.
    input: Option<String>,
    output: Option<String>,
}

/// The process-wide audio path.
pub(crate) struct Pipeline {
    shared: Arc<Shared>,
    threads: Mutex<Threads>,
}

pub(crate) fn pipeline() -> &'static Pipeline {
    static PIPELINE: OnceLock<Pipeline> = OnceLock::new();
    PIPELINE.get_or_init(|| {
        let shared = Shared::new();
        let pan = std::env::var("YAP_TEST_PAN")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .filter(|p| p.is_finite());
        if let Some(pan) = pan {
            eprintln!("[voice] test hook: every remote track at pan {pan}");
            shared.mixer.force_pan(Some(pan));
        }
        Pipeline {
            shared: Arc::new(shared),
            threads: Mutex::new(Threads::default()),
        }
    })
}

impl Pipeline {
    /// A new stream in the speaker mix (it leaves when dropped).
    pub(crate) fn add_stream(&self) -> Arc<Stream> {
        self.shared.mixer.add()
    }

    /// The others play (`false` while deafened).
    pub(crate) fn set_playout(&self, enabled: bool) {
        self.shared.mixer.set_enabled(enabled);
    }

    /// Where the processed mic goes.
    pub(crate) fn set_mic(&self, source: Option<NativeAudioSource>) {
        *lock(&self.shared.mic) = source;
    }

    /// The APM's switches and RNNoise. A change of the APM's rebuilds it
    /// (the echo canceller converges again within a second or so).
    pub(crate) fn set_processing(&self, p: Processing) {
        let config = (
            p.echo_cancellation,
            p.noise_suppression,
            p.auto_gain_control,
        );
        if lock(&self.shared.apm).config != config {
            // Built, and the old one freed, outside the lock: an APM's
            // state is large, and both device threads take the lock every
            // 10 ms.
            let fresh = Apm::new(config);
            let spare = {
                let mut apm = lock(&self.shared.apm);
                if apm.config == config {
                    fresh
                } else {
                    std::mem::replace(&mut *apm, fresh)
                }
            };
            drop(spare);
        }
        self.shared
            .noise_reduction
            .store(p.noise_reduction, Ordering::Relaxed);
    }

    /// The endpoints to use (`None`: the default communications device);
    /// a running thread on another one restarts on the new one.
    pub(crate) fn set_devices(&self, input: Option<String>, output: Option<String>) {
        let mut threads = lock(&self.threads);
        threads.input = input;
        threads.output = output;
        if threads
            .capture
            .as_ref()
            .is_some_and(|w| w.device != threads.input)
        {
            threads.capture = None;
            self.shared.mic_health.working();
            threads.capture = self.spawn_capture(threads.input.clone());
        }
        if threads
            .render
            .as_ref()
            .is_some_and(|w| w.device != threads.output)
        {
            threads.render = None;
            self.shared.speaker_health.working();
            threads.render = self.spawn_render(threads.output.clone());
        }
    }

    /// Starts the speaker side (a no-op while it runs).
    pub(crate) fn start_render(&self) {
        let mut threads = lock(&self.threads);
        if threads.render.is_none() {
            threads.render = self.spawn_render(threads.output.clone());
        }
    }

    /// Starts the mic side (a no-op while it runs).
    pub(crate) fn start_capture(&self) {
        let mut threads = lock(&self.threads);
        if threads.capture.is_none() {
            threads.capture = self.spawn_capture(threads.input.clone());
        }
    }

    pub(crate) fn stop_capture(&self) {
        let worker = lock(&self.threads).capture.take();
        drop(worker);
        self.set_mic(None);
        self.shared.mic_health.working();
    }

    /// Stops both sides (the call is over).
    pub(crate) fn stop(&self) {
        let (capture, render) = {
            let mut threads = lock(&self.threads);
            (threads.capture.take(), threads.render.take())
        };
        drop(capture);
        drop(render);
        self.set_mic(None);
        self.shared.mixer.set_enabled(true);
        self.shared.echo_hook.store(false, Ordering::Relaxed);
        lock(&self.shared.echo).clear();
        self.shared.mic_health.working();
        self.shared.speaker_health.working();
    }

    /// The mic and the speaker each failed [`NOTICE_AFTER`] tries in a row
    /// (and have not worked since).
    pub(crate) fn failing(&self) -> (bool, bool) {
        (
            self.shared.mic_health.failing(),
            self.shared.speaker_health.failing(),
        )
    }

    fn spawn_capture(&self, device: Option<String>) -> Option<Worker> {
        let shared = Arc::clone(&self.shared);
        Worker::spawn("yap-mic", device, move |device, stop| {
            capture_thread(&shared, device.as_deref(), &stop);
        })
    }

    fn spawn_render(&self, device: Option<String>) -> Option<Worker> {
        let shared = Arc::clone(&self.shared);
        Worker::spawn("yap-speaker", device, move |device, stop| {
            render_thread(&shared, device.as_deref(), &stop);
        })
    }

    /// The stats line's audio part: processing cost per 10 ms frame, and
    /// the mix's queues.
    pub(crate) fn stats(&self) -> String {
        let c = &self.shared.costs;
        // Microseconds per frame since the last line.
        let per = |ns: &AtomicU64, frames: u64| {
            ns.swap(0, Ordering::Relaxed)
                .checked_div(frames)
                .unwrap_or(0)
                / 1000
        };
        let capture_frames = c.capture_frames.swap(0, Ordering::Relaxed);
        let render_frames = c.render_frames.swap(0, Ordering::Relaxed);
        let apm = per(&c.apm_ns, capture_frames);
        let rnnoise = per(&c.rnnoise_ns, c.rnnoise_frames.swap(0, Ordering::Relaxed));
        let reverse = per(&c.reverse_ns, render_frames);
        let mix = per(&c.mix_ns, render_frames);
        let m = self.shared.mixer.stats();
        let apm_config = lock(&self.shared.apm).config;
        let (mic_failing, speaker_failing) = self.failing();
        format!(
            "audio[5s frames in={capture_frames} out={render_frames} apm={apm}us rnnoise={rnnoise}us{} \
             reverse={reverse}us mix={mix}us streams={} queued={}f underruns={} trimmed={}f \
             skewed={}f delay={}+{}ms aec={} ns={} agc={}{}{}]",
            if self.shared.noise_reduction.load(Ordering::Relaxed) {
                ""
            } else {
                "(off)"
            },
            m.streams,
            m.most_queued,
            m.underruns,
            m.trimmed_frames,
            m.skewed_frames,
            self.shared.render_delay_ms.load(Ordering::Relaxed),
            self.shared.capture_delay_ms.load(Ordering::Relaxed),
            apm_config.0,
            apm_config.1,
            apm_config.2,
            if mic_failing { " mic=failing" } else { "" },
            if speaker_failing {
                " speaker=failing"
            } else {
                ""
            },
        )
    }
}

// ── The mic ────────────────────────────────────────────────────────────────

/// Where the mic's samples come from.
enum MicSource {
    #[cfg(windows)]
    Device(Capture),
    /// `YAP_FAKE_MIC`: a file, looped, paced by the clock.
    Fake {
        samples: Vec<i16>,
        at: usize,
        started: Instant,
        sent: u64,
        echo_gain: f32,
    },
}

impl MicSource {
    fn open(shared: &Shared, device: Option<&str>) -> Result<Self, OpenError> {
        if let Some(path) = env_path("YAP_FAKE_MIC") {
            let file = wav::read(&path)
                .map_err(|e| OpenError::missing(format!("{}: {e}", path.display())))?;
            if file.rate != RATE {
                return Err(OpenError::missing(format!(
                    "{}: {} Hz, not 48000",
                    path.display(),
                    file.rate
                )));
            }
            let samples = crate::dsp::channel(&file.samples, usize::from(file.channels), 0);
            if samples.len() < FRAME {
                return Err(OpenError::missing(format!(
                    "{}: shorter than 10 ms",
                    path.display()
                )));
            }
            let echo_gain = std::env::var("YAP_FAKE_MIC_ECHO")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|g| g.is_finite() && *g > 0.0)
                .unwrap_or(0.0);
            eprintln!(
                "[voice] test hook: mic from {} ({:.1} s, echo {echo_gain})",
                path.display(),
                samples.len() as f64 / f64::from(RATE)
            );
            shared.echo_hook.store(echo_gain > 0.0, Ordering::Relaxed);
            shared.capture_delay_ms.store(10, Ordering::Relaxed);
            return Ok(Self::Fake {
                samples,
                at: 0,
                started: Instant::now(),
                sent: 0,
                echo_gain,
            });
        }
        #[cfg(windows)]
        {
            let capture = Capture::open(device)?;
            shared
                .capture_delay_ms
                .store(capture.latency_ms(), Ordering::Relaxed);
            Ok(Self::Device(capture))
        }
        #[cfg(not(windows))]
        {
            let _ = device;
            Err(OpenError::missing(
                "no audio devices on this platform".into(),
            ))
        }
    }

    fn read(&mut self, shared: &Shared, out: &mut Vec<i16>) -> Result<(), String> {
        match self {
            #[cfg(windows)]
            Self::Device(capture) => capture.read(out, Duration::from_millis(50)),
            Self::Fake {
                samples,
                at,
                started,
                sent,
                echo_gain,
            } => {
                let due = *started + Duration::from_millis(10 * (*sent + 1));
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
                let start = out.len();
                for _ in 0..FRAME {
                    out.push(samples[*at]);
                    *at = (*at + 1) % samples.len();
                }
                if *echo_gain > 0.0 {
                    let mut ring = lock(&shared.echo);
                    if ring.len() >= (ECHO_DELAY_FRAMES + 1) * FRAME {
                        for s in &mut out[start..] {
                            let echo = f32::from(ring.pop_front().unwrap_or(0)) * *echo_gain;
                            *s = (f32::from(*s) + echo).round().clamp(-32768.0, 32767.0) as i16;
                        }
                    }
                }
                *sent += 1;
                Ok(())
            }
        }
    }
}

/// The mic side of one 10 ms frame: the APM, then RNNoise when it is on.
/// Pure enough to test: the `Shared` it reads and the denoiser it keeps.
fn process_capture(shared: &Shared, denoiser: &mut Option<Denoiser>, frame: &mut [i16]) {
    let started = Instant::now();
    {
        let mut apm = lock(&shared.apm);
        if apm.active() {
            let delay = shared.render_delay_ms.load(Ordering::Relaxed)
                + shared.capture_delay_ms.load(Ordering::Relaxed);
            let _ = apm
                .module
                .set_stream_delay_ms(i32::try_from(delay).unwrap_or(40));
            if let Err(error) = apm.module.process_stream(frame, RATE as i32, 1) {
                eprintln!("[voice] mic processing failed: {}", error.message);
            }
        }
    }
    add_elapsed(&shared.costs.apm_ns, started);
    shared.costs.capture_frames.fetch_add(1, Ordering::Relaxed);
    if shared.noise_reduction.load(Ordering::Relaxed) {
        let started = Instant::now();
        denoiser.get_or_insert_with(Denoiser::new).process(frame);
        add_elapsed(&shared.costs.rnnoise_ns, started);
        shared.costs.rnnoise_frames.fetch_add(1, Ordering::Relaxed);
    } else {
        // A fresh state next time: the old one's memory is stale.
        *denoiser = None;
    }
}

/// The speaker side of one 10 ms frame: the mix, and the same frame to the
/// echo canceller as its far end.
fn process_render(shared: &Shared, frame: &mut [i16; SAMPLES]) {
    let started = Instant::now();
    shared.mixer.render(frame);
    add_elapsed(&shared.costs.mix_ns, started);
    let started = Instant::now();
    {
        let mut apm = lock(&shared.apm);
        if apm.active() {
            // A copy on the stack: the APM may write into what it is given,
            // and the frame still goes to the speaker.
            let mut far = *frame;
            if let Err(error) = apm.module.process_reverse_stream(&mut far, RATE as i32, 2) {
                eprintln!("[voice] far-end processing failed: {}", error.message);
            }
        }
    }
    add_elapsed(&shared.costs.reverse_ns, started);
    shared.costs.render_frames.fetch_add(1, Ordering::Relaxed);
}

fn send_mic(shared: &Shared, frame: &[i16]) {
    let mic = lock(&shared.mic);
    let Some(source) = mic.as_ref() else {
        return;
    };
    let frame = AudioFrame {
        data: frame.into(),
        sample_rate: RATE,
        num_channels: 1,
        samples_per_channel: FRAME as u32,
    };
    // Unbuffered: straight to the track's sinks, ready at once.
    if let Some(Err(error)) = source.capture_frame(&frame).now_or_never() {
        eprintln!("[voice] mic frame refused: {}", error.message);
    }
}

fn capture_thread(shared: &Shared, device: Option<&str>, stop: &AtomicBool) {
    #[cfg(windows)]
    let _com = Com::init();
    #[cfg(windows)]
    let _priority = ProAudio::register("yap-mic");
    let tap_of = |name: &str| {
        tap_path(name).and_then(|path| {
            WavWriter::create(&path, RATE, 1, TAP_SECONDS)
                .map_err(|e| eprintln!("[voice] {name} {}: {e}", path.display()))
                .ok()
        })
    };
    let (mut raw_tap, mut tap) = (tap_of("YAP_MIC_RAW_TAP"), tap_of("YAP_MIC_TAP"));
    let mut denoiser = None;
    let mut pending: Vec<i16> = Vec::with_capacity(FRAME * 4);
    while !stop.load(Ordering::Relaxed) {
        let mut source = match MicSource::open(shared, device) {
            Ok(source) => {
                if !matches!(source, MicSource::Fake { .. }) {
                    eprintln!("[voice] mic open ({})", device.unwrap_or("default"));
                }
                source
            }
            Err(error) => {
                eprintln!("[voice] mic unavailable: {error}; retrying");
                if shared.mic_health.failed("mic", error.refused) {
                    fall_back("mic");
                }
                sleep_unless(stop, Duration::from_secs(2));
                continue;
            }
        };
        pending.clear();
        let mut stall = Stall::new(Instant::now());
        let mut worked = false;
        while !stop.load(Ordering::Relaxed) {
            let before = pending.len();
            let read = source
                .read(shared, &mut pending)
                .and_then(|()| stall.check(pending.len() > before, Instant::now()));
            if let Err(error) = read {
                eprintln!("[voice] mic stopped: {error}; reopening");
                if !worked {
                    shared.mic_health.failed("mic", false);
                }
                break;
            }
            if !worked && pending.len() > before {
                worked = true;
                shared.mic_health.working();
            }
            let whole = pending.len() / FRAME * FRAME;
            for frame in pending[..whole].chunks_exact_mut(FRAME) {
                if let Some(writer) = raw_tap.as_mut() {
                    let _ = writer.write(frame);
                }
                process_capture(shared, &mut denoiser, frame);
                if let Some(writer) = tap.as_mut() {
                    let _ = writer.write(frame);
                }
                send_mic(shared, frame);
            }
            pending.drain(..whole);
        }
    }
}

// ── The speaker ────────────────────────────────────────────────────────────

fn sleep_unless(stop: &AtomicBool, total: Duration) {
    let until = Instant::now() + total;
    while !stop.load(Ordering::Relaxed) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// One frame for the speaker: mixed, fed to the echo canceller, recorded
/// by the taps.
fn next_render_frame(shared: &Shared, frame: &mut [i16; SAMPLES], tap: &mut Option<WavWriter>) {
    process_render(shared, frame);
    if let Some(writer) = tap.as_mut() {
        let _ = writer.write(frame);
    }
    if !shared.echo_hook.load(Ordering::Relaxed) {
        return;
    }
    let mut ring = lock(&shared.echo);
    if ring.is_empty() {
        ring.extend(std::iter::repeat_n(0, ECHO_DELAY_FRAMES * FRAME));
    }
    ring.extend(
        frame
            .chunks_exact(2)
            .map(|p| ((i32::from(p[0]) + i32::from(p[1])) / 2) as i16),
    );
    // The mic side reads a frame per frame; this bounds a slower one.
    let keep = (ECHO_DELAY_FRAMES + 4) * FRAME;
    if ring.len() > keep {
        let extra = ring.len() - keep;
        ring.drain(..extra);
    }
}

/// No device (`YAP_NULL_SPEAKER=1`, or while the device cannot open): a
/// frame every 10 ms of the clock, for `total` (or until stopped).
fn render_null(
    shared: &Shared,
    stop: &AtomicBool,
    tap: &mut Option<WavWriter>,
    total: Option<Duration>,
) {
    let started = Instant::now();
    let mut frame = [0i16; SAMPLES];
    let mut sent = 0u64;
    shared.render_delay_ms.store(10, Ordering::Relaxed);
    while !stop.load(Ordering::Relaxed) && total.is_none_or(|t| started.elapsed() < t) {
        let due = started + Duration::from_millis(10 * (sent + 1));
        std::thread::sleep(due.saturating_duration_since(Instant::now()));
        next_render_frame(shared, &mut frame, tap);
        sent += 1;
    }
}

fn render_thread(shared: &Shared, device: Option<&str>, stop: &AtomicBool) {
    #[cfg(windows)]
    let _com = Com::init();
    #[cfg(windows)]
    let _priority = ProAudio::register("yap-speaker");
    let mut tap = tap_path("YAP_RENDER_TAP").and_then(|path| {
        WavWriter::create(&path, RATE, 2, TAP_SECONDS)
            .map_err(|e| eprintln!("[voice] render tap {}: {e}", path.display()))
            .ok()
    });
    if std::env::var("YAP_NULL_SPEAKER").is_ok_and(|v| v == "1") {
        eprintln!("[voice] test hook: no speaker (clock-paced mix)");
        render_null(shared, stop, &mut tap, None);
        return;
    }
    #[cfg(windows)]
    while !stop.load(Ordering::Relaxed) {
        let mut render = match Render::open(device) {
            Ok(render) => {
                eprintln!(
                    "[voice] speaker open ({}, {} frame buffer, {} ms latency)",
                    device.unwrap_or("default"),
                    render.capacity(),
                    render.latency_ms()
                );
                render
            }
            Err(error) => {
                eprintln!("[voice] speaker unavailable: {error}; mixing to nothing for 2 s");
                if shared.speaker_health.failed("speaker", error.refused) {
                    fall_back("speaker");
                }
                render_null(shared, stop, &mut tap, Some(Duration::from_secs(2)));
                continue;
            }
        };
        let mut frame = [0i16; SAMPLES];
        let ahead = RENDER_AHEAD.min(render.capacity());
        let latency = render.latency_ms();
        let mut stall = Stall::new(Instant::now());
        let mut worked = false;
        let stopped = 'device: loop {
            if stop.load(Ordering::Relaxed) {
                break None;
            }
            if let Err(error) = render.wait(Duration::from_millis(50)) {
                break Some(error);
            }
            let mut wrote = false;
            loop {
                let queued = match render.queued() {
                    Ok(queued) => queued,
                    Err(error) => break 'device Some(error),
                };
                if queued + FRAME as u32 > ahead {
                    break;
                }
                next_render_frame(shared, &mut frame, &mut tap);
                if let Err(error) = render.write(&frame) {
                    break 'device Some(error);
                }
                wrote = true;
                let ms = (queued + FRAME as u32) * 1000 / RATE + latency;
                shared.render_delay_ms.store(ms, Ordering::Relaxed);
            }
            if wrote && !worked {
                worked = true;
                shared.speaker_health.working();
            }
            if let Err(error) = stall.check(wrote, Instant::now()) {
                break Some(error);
            }
        };
        if let Some(error) = stopped {
            eprintln!("[voice] speaker stopped: {error}; reopening");
            if !worked {
                shared.speaker_health.failed("speaker", false);
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = device;
        render_null(shared, stop, &mut tap, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::denoise::tests::Noise;
    use crate::dsp::rms;

    fn shared(processing: Processing) -> Shared {
        let shared = Shared::new();
        shared.render_delay_ms.store(10, Ordering::Relaxed);
        let config = (
            processing.echo_cancellation,
            processing.noise_suppression,
            processing.auto_gain_control,
        );
        *lock(&shared.apm) = Apm::new(config);
        shared
            .noise_reduction
            .store(processing.noise_reduction, Ordering::Relaxed);
        shared
    }

    /// Runs `seconds` of a far-end talker (noise bursts, as speech comes)
    /// through the mix to the "speaker", and the same audio back into the
    /// "mic" 40 ms later at half level, as a room would; returns the mic's
    /// RMS before and after processing over the last two seconds.
    fn echo_run(processing: Processing, seconds: usize) -> (f64, f64) {
        let shared = shared(processing);
        let stream = shared.mixer.add();
        let mut noise = Noise::new(7);
        let mut denoiser = None;
        let delay = 4;
        let mut played: VecDeque<Vec<i16>> = VecDeque::new();
        let (mut before, mut after) = (Vec::new(), Vec::new());
        let frames = seconds * 100;
        for n in 0..frames {
            // The far end: 300 ms bursts every 400 ms, mono.
            let talking = (n % 40) < 30;
            let far: Vec<i16> = (0..FRAME)
                .map(|_| {
                    if talking {
                        (noise.next() * 6000.0) as i16
                    } else {
                        0
                    }
                })
                .collect();
            stream.push(&far, 1);
            let mut out = [0i16; SAMPLES];
            process_render(&shared, &mut out);
            played.push_back(crate::dsp::channel(&out, 2, 0));
            let mut mic: Vec<i16> = if played.len() > delay {
                played
                    .pop_front()
                    .unwrap_or_default()
                    .iter()
                    .map(|s| s / 2)
                    .collect()
            } else {
                vec![0; FRAME]
            };
            let input = mic.clone();
            process_capture(&shared, &mut denoiser, &mut mic);
            if n >= frames - 200 {
                before.extend(input);
                after.extend(mic);
            }
        }
        (rms(&before), rms(&after))
    }

    fn aec_only(on: bool) -> Processing {
        Processing {
            echo_cancellation: on,
            noise_suppression: false,
            auto_gain_control: false,
            noise_reduction: false,
        }
    }

    /// The far end the render side hands the APM is what the capture side
    /// cancels: the echo drops by 50 dB once converged.
    #[test]
    fn the_echo_canceller_should_remove_what_the_speaker_played() {
        let (before, after) = echo_run(aec_only(true), 6);
        let erle = 20.0 * (before / after.max(1e-9)).log10();
        eprintln!("echo: {before:.4} -> {after:.4} RMS ({erle:.1} dB)");
        assert!(erle >= 50.0, "only {erle:.1} dB of echo removed");
    }

    #[test]
    fn without_echo_cancellation_the_echo_should_pass() {
        let (before, after) = echo_run(aec_only(false), 3);
        let change = 20.0 * (before / after.max(1e-9)).log10();
        assert!(change.abs() < 1.0, "{change:.1} dB");
    }

    #[test]
    fn rnnoise_should_run_only_while_it_is_on() {
        let shared = shared(Processing {
            noise_reduction: true,
            ..aec_only(false)
        });
        let mut denoiser = None;
        process_capture(&shared, &mut denoiser, &mut [0; FRAME]);
        assert!(denoiser.is_some());
        shared.noise_reduction.store(false, Ordering::Relaxed);
        process_capture(&shared, &mut denoiser, &mut [0; FRAME]);
        assert!(denoiser.is_none());
    }

    #[test]
    fn a_new_apm_config_should_replace_the_module() {
        let pipeline = Pipeline {
            shared: Arc::new(shared(Processing::default())),
            threads: Mutex::new(Threads::default()),
        };
        pipeline.set_processing(aec_only(true));
        assert_eq!(lock(&pipeline.shared.apm).config, (true, false, false));
    }

    #[test]
    fn the_echo_ring_should_stay_empty_without_the_hook() {
        let shared = shared(aec_only(false));
        let mut frame = [0i16; SAMPLES];
        next_render_frame(&shared, &mut frame, &mut None);
        assert!(lock(&shared.echo).is_empty());
    }

    /// A mock device whose event stops firing: every wait times out after
    /// 50 ms and delivers nothing. The loop gives up after about a second.
    #[test]
    fn a_device_that_stops_firing_should_count_as_stopped_after_a_second() {
        let start = Instant::now();
        let mut stall = Stall::new(start);
        let mut now = start;
        let waits = (1..=100)
            .find(|_| {
                now += Duration::from_millis(50);
                stall.check(false, now).is_err()
            })
            .unwrap_or(0);
        assert_eq!(waits, 20);
    }

    #[test]
    fn audio_should_keep_a_device_alive() {
        let start = Instant::now();
        let mut stall = Stall::new(start);
        let ok = (1..=100).all(|n| {
            let now = start + Duration::from_millis(50 * n);
            // Every tenth wait (500 ms) brings audio.
            stall.check(n % 10 == 0, now).is_ok()
        });
        assert!(ok);
    }

    #[test]
    fn three_failed_tries_should_tell_the_call() {
        let health = DeviceHealth::default();
        let flags: Vec<bool> = (0..NOTICE_AFTER)
            .map(|_| {
                health.failed("mic", false);
                health.failing()
            })
            .collect();
        assert_eq!(flags, vec![false, false, true]);
    }

    #[test]
    fn audio_through_should_clear_the_failures() {
        let health = DeviceHealth::default();
        for _ in 0..NOTICE_AFTER {
            health.failed("mic", false);
        }
        health.working();
        assert!(!health.failing());
    }

    #[test]
    fn three_refusals_in_a_row_should_ask_for_the_adm_once() {
        let health = DeviceHealth::default();
        let asks: Vec<bool> = (0..NOTICE_AFTER + 1)
            .map(|_| health.failed("speaker", true))
            .collect();
        assert_eq!(asks, vec![false, false, true, false]);
    }

    #[test]
    fn a_missing_device_should_not_ask_for_the_adm() {
        let health = DeviceHealth::default();
        health.failed("mic", true);
        health.failed("mic", true);
        let asked = health.failed("mic", false);
        assert!(!asked && health.failing());
    }
}
