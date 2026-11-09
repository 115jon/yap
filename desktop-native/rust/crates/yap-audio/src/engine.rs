//! The public handle, the engine thread that owns the voices, and the
//! mixer's place on the app's speaker (`yap-speaker`): the speaker's render
//! thread pulls 10 ms at a time from [`LocalInput`] and mixes it with the
//! call and the UI sounds, so the call's echo canceller hears it too.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::decode::{decode_clip, DecodedClip};
use crate::mixer::{Mixer, StreamVoice};
use crate::stream::queue::StreamQueue;
use crate::stream::source::{Cancel, HttpFetcher, Net};
use crate::stream::{self, describe_url, StreamHandle, StreamJob};
use crate::{tone, Clip, ClipOptions, Event, EventSink, Loudness, StreamRequest};

const POSITION_EVERY: Duration = Duration::from_millis(250);
const LEVEL_EVERY: Duration = Duration::from_millis(100);
/// How often the engine thread hands on the mixer's events while it plays.
const EVENTS_EVERY: Duration = Duration::from_millis(20);
/// One speaker frame (10 ms), in samples (48 kHz stereo).
const SAMPLES: usize = yap_speaker::SAMPLES;
const FRAME_TIME: Duration = Duration::from_millis(10);
/// Clips decoding at once; a play past this fails instead of piling up
/// threads and decoded PCM.
const MAX_DECODES: usize = 3;

enum Command {
    Play {
        key: String,
        clip: Clip,
        opts: ClipOptions,
    },
    Stop(String),
    SetPaused(String, bool),
    SetVolume(String, f32),
    StopAll,
    StreamLoad(StreamRequest),
    StreamSeek(u64),
    StreamPaused(bool),
    StreamVolume(f32),
    StreamLoudness(Loudness),
    StreamStop,
    Decoded {
        key: String,
        generation: u64,
        result: Result<DecodedClip, String>,
    },
    Shutdown,
}

/// Where the mix goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputKind {
    /// The app's speaker (`yap-speaker`) pulls it.
    Speaker,
    /// The engine thread renders it on the clock, to nothing (tests).
    Null,
}

/// The mixer as an input of the app's speaker. The engine thread changes
/// the voices under the same lock, between frames.
pub(crate) struct LocalInput {
    mixer: Mutex<Mixer>,
    /// The loudest sample since the level was last read, `f32` bits.
    peak: AtomicU32,
    /// When each voice's first frame was mixed (for the latency line).
    mixed: Mutex<Vec<(String, Instant)>>,
}

impl LocalInput {
    fn new() -> Self {
        Self {
            mixer: Mutex::new(Mixer::default()),
            peak: AtomicU32::new(0),
            mixed: Mutex::new(Vec::new()),
        }
    }

    fn mixer(&self) -> MutexGuard<'_, Mixer> {
        self.mixer.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The peak since the last call.
    fn take_peak(&self) -> f32 {
        f32::from_bits(self.peak.swap(0, Ordering::Relaxed))
    }

    fn render(&self, out: &mut [f32; SAMPLES]) -> bool {
        let mut block = [0f32; SAMPLES];
        let (active, peak) = {
            let mut mixer = self.mixer();
            let active = mixer.is_active();
            let before = mixer.events().len();
            let peak = mixer.render(&mut block);
            let now = Instant::now();
            for event in &mixer.events()[before..] {
                if let Event::Started { key } = event {
                    let mut mixed = self.mixed.lock().unwrap_or_else(PoisonError::into_inner);
                    mixed.truncate(63);
                    mixed.push((key.clone(), now));
                }
            }
            (active, peak)
        };
        for (o, s) in out.iter_mut().zip(block) {
            *o += s;
        }
        if peak > 0.0 {
            // Peaks are non-negative, so their bits order as they do.
            self.peak.fetch_max(peak.to_bits(), Ordering::Relaxed);
        }
        active
    }
}

impl yap_speaker::Input for LocalInput {
    fn mix_into(&self, out: &mut [f32; SAMPLES]) -> bool {
        self.render(out)
    }
}

struct Shared {
    events: EventSink,
    output: OutputKind,
    tx: Mutex<Option<Sender<Command>>>,
}

impl Shared {
    fn sender(&self) -> MutexGuard<'_, Option<Sender<Command>>> {
        self.tx.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        if let Some(tx) = self.sender().take() {
            let _ = tx.send(Command::Shutdown);
        }
    }
}

/// Handle to the playback engine. Cheap to clone; every method returns at
/// once. The engine thread stops when the last clone is dropped.
#[derive(Clone)]
pub struct Engine {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine").finish_non_exhaustive()
    }
}

impl Engine {
    /// Spawns the mixer/output thread lazily on first play. `events` is called from engine threads.
    pub fn new(events: Arc<dyn Fn(Event) + Send + Sync>) -> Self {
        Self::with_output(events, OutputKind::Speaker)
    }

    fn with_output(events: EventSink, output: OutputKind) -> Self {
        Self {
            shared: Arc::new(Shared {
                events,
                output,
                tx: Mutex::new(None),
            }),
        }
    }

    /// Sends `command`; `spawn` starts the engine thread when it is not
    /// running (plays and loads), otherwise the command is dropped.
    fn send(&self, command: Command, spawn: bool) -> bool {
        let mut tx = self.shared.sender();
        let command = match tx.as_ref() {
            Some(sender) => match sender.send(command) {
                Ok(()) => return true,
                Err(mpsc::SendError(command)) => {
                    eprintln!("[audio] engine thread gone; restarting");
                    *tx = None;
                    command
                }
            },
            None => command,
        };
        if !spawn {
            return false;
        }
        let (sender, receiver) = mpsc::channel();
        let events = Arc::clone(&self.shared.events);
        let worker_sender = sender.clone();
        let output = self.shared.output;
        let spawned = std::thread::Builder::new()
            .name("yap-audio".into())
            .spawn(move || Worker::new(events, worker_sender, output).run(&receiver));
        if let Err(error) = spawned {
            eprintln!("[audio] no engine thread: {error}");
            return false;
        }
        let sent = sender.send(command).is_ok();
        *tx = Some(sender);
        sent
    }

    /// Start (or restart: same key replaces the old voice) a clip.
    pub fn play(&self, key: &str, clip: Clip, opts: ClipOptions) {
        let command = Command::Play {
            key: key.to_string(),
            clip,
            opts,
        };
        if !self.send(command, true) {
            (self.shared.events)(Event::Failed {
                key: key.to_string(),
                error: "audio engine unavailable".into(),
            });
        }
    }

    pub fn stop(&self, key: &str) {
        self.send(Command::Stop(key.to_string()), false);
    }

    pub fn set_paused(&self, key: &str, paused: bool) {
        self.send(Command::SetPaused(key.to_string(), paused), false);
    }

    /// Linear 0..=1 (clamped), applied with a short ramp (no clicks).
    pub fn set_volume(&self, key: &str, volume: f32) {
        self.send(Command::SetVolume(key.to_string(), volume), false);
    }

    pub fn stop_all(&self) {
        self.send(Command::StopAll, false);
    }

    /// The Listen Together stream: one at a time; a new load replaces it.
    pub fn stream_load(&self, request: StreamRequest) {
        let key = request.key.clone();
        if !self.send(Command::StreamLoad(request), true) {
            (self.shared.events)(Event::StreamFailed {
                key,
                failure: crate::StreamFailure {
                    status: None,
                    code: "PLAYBACK_FAILED".into(),
                    message: "Playback stopped and couldn't recover. Skip the track or try again."
                        .into(),
                },
            });
        }
    }

    pub fn stream_seek(&self, position_ms: u64) {
        self.send(Command::StreamSeek(position_ms), false);
    }

    pub fn stream_set_paused(&self, paused: bool) {
        self.send(Command::StreamPaused(paused), false);
    }

    pub fn stream_set_volume(&self, volume: f32) {
        self.send(Command::StreamVolume(volume), false);
    }

    pub fn stream_set_loudness(&self, loudness: Loudness) {
        self.send(Command::StreamLoudness(loudness), false);
    }

    pub fn stream_stop(&self) {
        self.send(Command::StreamStop, false);
    }
}

/// One of the [`MAX_DECODES`] slots, held by a decode thread (released on
/// drop, also if the thread never starts).
struct DecodeSlot(Arc<AtomicUsize>);

impl DecodeSlot {
    fn take(counter: &Arc<AtomicUsize>) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(Arc::clone(counter))
    }
}

impl Drop for DecodeSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

/// A clip being decoded.
struct PendingClip {
    generation: u64,
    opts: ClipOptions,
}

struct Worker {
    events: EventSink,
    /// For decode threads to hand back clips.
    commands: Sender<Command>,
    output: OutputKind,
    /// The voices, shared with the speaker's render thread.
    input: Arc<LocalInput>,
    /// The same, as the speaker holds it (registered while the engine runs).
    speaker_input: Option<Arc<dyn yap_speaker::Input>>,
    pending: HashMap<String, PendingClip>,
    /// When each play arrived, for the latency line at its first frame.
    queued_at: HashMap<String, Instant>,
    next_generation: u64,
    /// Decode threads still running (stopped clips' threads count until
    /// they finish).
    decoding: Arc<AtomicUsize>,
    stream: Option<StreamHandle>,
    net: Option<(tokio::runtime::Runtime, Net)>,
    last_position: Instant,
    last_level: Instant,
    level_live: bool,
    /// `OutputKind::Null`: when the next frame is due.
    next_frame: Instant,
}

impl Worker {
    fn new(events: EventSink, commands: Sender<Command>, output: OutputKind) -> Self {
        let input = Arc::new(LocalInput::new());
        let speaker_input = (output == OutputKind::Speaker).then(|| {
            let as_input: Arc<dyn yap_speaker::Input> = input.clone();
            yap_speaker::speaker().add(&as_input);
            as_input
        });
        Self {
            events,
            commands,
            output,
            input,
            speaker_input,
            pending: HashMap::new(),
            queued_at: HashMap::new(),
            next_generation: 0,
            decoding: Arc::new(AtomicUsize::new(0)),
            stream: None,
            net: None,
            last_position: Instant::now(),
            last_level: Instant::now(),
            level_live: false,
            next_frame: Instant::now(),
        }
    }

    fn emit(&self, event: Event) {
        (self.events)(event);
    }

    fn run(mut self, commands: &Receiver<Command>) {
        loop {
            // Read under one lock: a frame that ends the last voice pushes
            // its `Ended` in the same render that makes the mixer idle, so
            // an idle mixer with no events left has nothing more to say.
            let (active, unsent, streaming) = {
                let mixer = self.input.mixer();
                (
                    mixer.is_active(),
                    !mixer.events().is_empty(),
                    mixer.stream().is_some(),
                )
            };
            let timeout = if active {
                if self.output == OutputKind::Null {
                    self.next_frame.saturating_duration_since(Instant::now())
                } else {
                    EVENTS_EVERY
                }
            } else if unsent || self.level_live {
                // The last events and the meter's final 0.
                EVENTS_EVERY
            } else if streaming || !self.pending.is_empty() {
                POSITION_EVERY
            } else {
                Duration::MAX
            };
            let received = if timeout == Duration::MAX {
                commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
            } else {
                commands.recv_timeout(timeout)
            };
            match received {
                Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => break,
                Ok(command) => {
                    self.handle(command);
                    // Anything else queued, before the next frame.
                    loop {
                        match commands.try_recv() {
                            Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => {
                                self.leave();
                                return;
                            }
                            Ok(command) => self.handle(command),
                            Err(TryRecvError::Empty) => break,
                        }
                    }
                    self.wake();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            if self.output == OutputKind::Null {
                self.render_due();
            }
            self.report_level();
            self.report_position();
            self.flush_events();
        }
        self.leave();
    }

    /// Takes the voices off the speaker (the engine is gone).
    fn leave(&mut self) {
        if let Some(input) = self.speaker_input.take() {
            yap_speaker::speaker().remove(&input);
        }
    }

    /// Something may have become audible: the speaker starts if it slept.
    fn wake(&mut self) {
        let active = self.input.mixer().is_active();
        if !active {
            return;
        }
        match self.output {
            OutputKind::Speaker => yap_speaker::speaker().wake(),
            OutputKind::Null => {
                if self.next_frame < Instant::now() - FRAME_TIME * 5 {
                    self.next_frame = Instant::now();
                }
            }
        }
    }

    /// `OutputKind::Null`: the frames the clock has made due, to nothing.
    fn render_due(&mut self) {
        let now = Instant::now();
        if self.next_frame + FRAME_TIME * 5 < now {
            self.next_frame = now;
        }
        while self.next_frame <= now {
            let mut out = [0f32; SAMPLES];
            if !self.input.render(&mut out) {
                self.next_frame = now + FRAME_TIME;
                return;
            }
            self.next_frame += FRAME_TIME;
        }
    }

    /// The output meter: the peak every [`LEVEL_EVERY`] while something
    /// plays, then one 0.
    fn report_level(&mut self) {
        if self.last_level.elapsed() < LEVEL_EVERY {
            return;
        }
        self.last_level = Instant::now();
        let active = self.input.mixer().is_active();
        let peak = self.input.take_peak();
        if active || peak > 0.0 {
            self.level_live = true;
            self.emit(Event::Level(peak));
        } else if self.level_live {
            self.level_live = false;
            self.emit(Event::Level(0.0));
        }
    }

    fn report_position(&mut self) {
        if self.last_position.elapsed() < POSITION_EVERY {
            return;
        }
        self.last_position = Instant::now();
        let position = {
            let mut mixer = self.input.mixer();
            if mixer
                .stream()
                .is_some_and(|stream| stream.queue().failed())
            {
                // Failed while paused: the stream thread already reported it.
                mixer.set_stream(None);
                drop(mixer);
                self.stream = None;
                return;
            }
            mixer.stream().map(|stream| Event::StreamPosition {
                key: stream.key().to_string(),
                position_ms: stream.queue().position_ms(),
                paused: stream.paused(),
            })
        };
        // Sent outside the lock: the speaker's render thread takes it.
        if let Some(event) = position {
            self.emit(event);
        } else if self.stream.is_some() {
            // The mixer dropped the stream (failed): stop its thread.
            self.stream = None;
        }
    }

    fn flush_events(&mut self) {
        let events = self.input.mixer().take_events();
        for event in events {
            if let Event::Started { key } = &event {
                if let Some(at) = self.queued_at.remove(key) {
                    let mixed = {
                        let mut mixed = self
                            .input
                            .mixed
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner);
                        let found = mixed.iter().rposition(|(k, _)| k == key);
                        found.map(|i| mixed.swap_remove(i).1)
                    };
                    let mixed_ms = mixed.map_or(f64::NAN, |m| {
                        m.saturating_duration_since(at).as_secs_f64() * 1000.0
                    });
                    let device_ms = match self.output {
                        OutputKind::Speaker => yap_speaker::speaker().delay_ms(),
                        OutputKind::Null => 0,
                    };
                    eprintln!(
                        "[audio] {key}: first frame mixed {mixed_ms:.1} ms after the play, heard {device_ms} ms after that"
                    );
                }
            }
            if let Event::Ended { key } = &event {
                eprintln!("[audio] ended {key}");
            }
            if let Event::StreamEnded { key } = &event {
                // The voice and its thread stay: a seek back (or a play after
                // the end) restarts the stream from there.
                eprintln!("[audio] stream {key}: ended");
            }
            self.emit(event);
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::Play { key, clip, opts } => self.play(key, clip, opts),
            Command::Decoded {
                key,
                generation,
                result,
            } => self.decoded(key, generation, result),
            Command::Stop(key) => {
                eprintln!("[audio] stop {key}");
                if self.pending.remove(&key).is_some() {
                    self.input.mixer().push_event(Event::Ended { key });
                } else {
                    self.input.mixer().stop(&key);
                }
            }
            Command::SetPaused(key, paused) => match self.pending.get_mut(&key) {
                Some(pending) => pending.opts.paused = paused,
                None => self.input.mixer().set_paused(&key, paused),
            },
            Command::SetVolume(key, volume) => match self.pending.get_mut(&key) {
                Some(pending) => pending.opts.volume = volume,
                // Tones scale by the ratio to the volume baked into them.
                None => self.input.mixer().set_volume(&key, volume),
            },
            Command::StopAll => {
                eprintln!("[audio] stop all");
                for (key, _) in self.pending.drain() {
                    self.input.mixer().push_event(Event::Ended { key });
                }
                self.input.mixer().stop_all();
            }
            Command::StreamLoad(request) => self.stream_load(request),
            Command::StreamSeek(position_ms) => {
                let mixer = self.input.mixer();
                if let (Some(stream), Some(handle)) = (mixer.stream(), &self.stream) {
                    eprintln!("[audio] stream {}: seek {position_ms} ms", stream.key());
                    let generation = stream.queue().clear(position_ms);
                    handle.seek(position_ms, generation);
                }
            }
            Command::StreamPaused(paused) => self.input.mixer().set_stream_paused(paused),
            Command::StreamVolume(volume) => self.input.mixer().set_stream_volume(volume),
            Command::StreamLoudness(loudness) => self.input.mixer().set_stream_loudness(loudness),
            Command::StreamStop => {
                if let Some(stream) = self.input.mixer().stream() {
                    eprintln!("[audio] stream {}: stop", stream.key());
                }
                self.stream = None;
                self.input.mixer().set_stream(None);
            }
            Command::Shutdown => {}
        }
    }

    fn play(&mut self, key: String, clip: Clip, opts: ClipOptions) {
        if self.queued_at.len() > 64 {
            self.queued_at.clear();
        }
        self.queued_at.insert(key.clone(), Instant::now());
        self.input.mixer().remove_clip(&key);
        self.pending.remove(&key);
        match clip {
            Clip::Tone(id) => {
                let Some(spec) = tone::find(&id) else {
                    eprintln!("[audio] failed {key}: unknown tone {id:?}");
                    self.input.mixer().push_event(Event::Failed {
                        key,
                        error: format!("unknown tone {id:?}"),
                    });
                    return;
                };
                eprintln!("[audio] play {key} (tone {id})");
                // The web starts the ramp at the volume; render with it baked in
                // and let the voice gain carry later volume changes as a ratio.
                let volume = if opts.volume.is_finite() {
                    opts.volume.clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let baked = if volume > 0.0 { volume } else { 1.0 };
                let pcm: Arc<[f32]> = tone::render(spec, baked).into();
                self.input.mixer().add_clip(key, pcm, &opts, 1.0 / baked);
            }
            Clip::Encoded { bytes, hint } => {
                if self.decoding.load(Ordering::Acquire) >= MAX_DECODES {
                    eprintln!("[audio] failed {key}: too many clips decoding");
                    self.input.mixer().push_event(Event::Failed {
                        key,
                        error: "too many clips decoding at once".into(),
                    });
                    return;
                }
                self.next_generation += 1;
                let generation = self.next_generation;
                self.pending
                    .insert(key.clone(), PendingClip { generation, opts });
                let commands = self.commands.clone();
                let thread_key = key.clone();
                let guard = DecodeSlot::take(&self.decoding);
                let spawned = std::thread::Builder::new()
                    .name("yap-audio-decode".into())
                    .spawn(move || {
                        let _slot = guard;
                        let result = decode_clip(bytes, hint.as_deref());
                        let _ = commands.send(Command::Decoded {
                            key: thread_key,
                            generation,
                            result,
                        });
                    });
                if let Err(error) = spawned {
                    self.pending.remove(&key);
                    self.input.mixer().push_event(Event::Failed {
                        key,
                        error: format!("no decode thread: {error}"),
                    });
                }
            }
        }
    }

    fn decoded(&mut self, key: String, generation: u64, result: Result<DecodedClip, String>) {
        let current = self
            .pending
            .get(&key)
            .is_some_and(|pending| pending.generation == generation);
        if !current {
            return;
        }
        let Some(pending) = self.pending.remove(&key) else {
            return;
        };
        match result {
            Ok(clip) => {
                eprintln!(
                    "[audio] play {key} ({}, {:.2} s)",
                    clip.codec,
                    clip.samples.len() as f64 / 2.0 / f64::from(crate::SAMPLE_RATE)
                );
                self.input.mixer()
                    .add_clip(key, clip.samples.into(), &pending.opts, 1.0);
            }
            Err(error) => {
                eprintln!("[audio] failed {key}: {error}");
                self.input.mixer().push_event(Event::Failed { key, error });
            }
        }
    }

    fn net(&mut self) -> Option<Net> {
        if self.net.is_none() {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(1)
                .thread_name("yap-audio-net")
                .enable_all()
                .build();
            let runtime = match runtime {
                Ok(runtime) => runtime,
                Err(error) => {
                    eprintln!("[audio] no network runtime: {error}");
                    return None;
                }
            };
            let client = {
                let _entered = runtime.enter();
                // The stream's host is the API or a CDN, never this machine.
                reqwest::Client::builder()
                    .connect_timeout(Duration::from_secs(10))
                    .dns_resolver(crate::guard::PublicResolver)
                    .build()
            };
            let client = match client {
                Ok(client) => client,
                Err(error) => {
                    eprintln!("[audio] no HTTP client: {error}");
                    return None;
                }
            };
            let net = Net {
                handle: runtime.handle().clone(),
                client,
            };
            self.net = Some((runtime, net));
        }
        self.net.as_ref().map(|(_, net)| net.clone())
    }

    fn stream_load(&mut self, request: StreamRequest) {
        // Replace: stop the old thread first, without a StreamEnded.
        self.stream = None;
        self.input.mixer().set_stream(None);
        eprintln!(
            "[audio] stream {}: load {} at {} ms{}",
            request.key,
            describe_url(&request.url),
            request.position_ms,
            if request.paused { " (paused)" } else { "" }
        );
        let fail = |worker: &Self, message: String| {
            eprintln!("[audio] stream {}: {message}", request.key);
            worker.emit(Event::StreamFailed {
                key: request.key.clone(),
                failure: crate::StreamFailure {
                    status: None,
                    code: "PLAYBACK_FAILED".into(),
                    message: "Playback stopped and couldn't recover. Skip the track or try again."
                        .into(),
                },
            });
        };
        let Some(net) = self.net() else {
            fail(self, "no network".into());
            return;
        };
        let queue = Arc::new(StreamQueue::new(request.position_ms));
        let headers = request.headers.clone();
        let job = StreamJob {
            key: request.key.clone(),
            url: request.url.clone(),
            position_ms: request.position_ms,
            queue: Arc::clone(&queue),
            events: Arc::clone(&self.events),
            live: request.live.then(|| net.clone()),
            make_fetcher: move |url: &str, cancel: Cancel| {
                HttpFetcher::new(net.clone(), url, &headers, cancel)
            },
            delay_scale: 1.0,
        };
        match stream::spawn(job, Cancel::default()) {
            Ok(handle) => {
                self.stream = Some(handle);
                self.input.mixer().set_stream(Some(StreamVoice::new(
                    request.key,
                    queue,
                    request.paused,
                    request.volume,
                    request.loudness,
                )));
                self.last_position = Instant::now() - POSITION_EVERY;
            }
            Err(error) => fail(self, format!("no stream thread: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine() -> (Engine, Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let events: EventSink = Arc::new(move |event| {
            if let Ok(tx) = tx.lock() {
                let _ = tx.send(event);
            }
        });
        (Engine::with_output(events, OutputKind::Null), rx)
    }

    /// Next event that is not a level reading.
    fn next(rx: &Receiver<Event>) -> Option<Event> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(Event::Level(_)) => continue,
                Ok(event) => return Some(event),
                Err(_) => return None,
            }
        }
    }

    #[test]
    fn the_local_input_should_add_its_mix_and_report_its_peak() {
        let input = LocalInput::new();
        input.mixer().add_clip(
            "a".into(),
            vec![0.25f32; 9600].into(),
            &ClipOptions::default(),
            1.0,
        );
        // What the other inputs put there stays: the speaker sums.
        let mut out = [0.5f32; SAMPLES];
        assert!(input.render(&mut out));
        assert!((out[0] - 0.75).abs() < 1e-6 && (out[SAMPLES - 1] - 0.75).abs() < 1e-6);
        assert!((input.take_peak() - 0.25).abs() < 1e-6);
        assert_eq!(input.take_peak(), 0.0, "read once");
    }

    #[test]
    fn an_idle_local_input_should_add_silence_and_let_the_speaker_close() {
        let input = LocalInput::new();
        let mut out = [0.1f32; SAMPLES];
        assert!(!input.render(&mut out));
        assert!(out.iter().all(|s| (*s - 0.1).abs() < 1e-9));
    }

    #[test]
    fn tone_plays_through_to_ended() {
        let (engine, rx) = engine();
        engine.play("t", Clip::Tone("ping".into()), ClipOptions::default());
        assert_eq!(next(&rx), Some(Event::Started { key: "t".into() }));
        assert_eq!(next(&rx), Some(Event::Ended { key: "t".into() }));
    }

    #[test]
    fn unknown_tone_fails() {
        let (engine, rx) = engine();
        engine.play("t", Clip::Tone("airhorn".into()), ClipOptions::default());
        assert!(matches!(next(&rx), Some(Event::Failed { key, .. }) if key == "t"));
    }

    #[test]
    fn encoded_clip_decodes_and_plays() {
        let (engine, rx) = engine();
        let bytes: Arc<[u8]> = crate::decode::tests::wav(44_100, 1, 440.0, 0.1).into();
        engine.play(
            "c",
            Clip::Encoded {
                bytes,
                hint: Some("wav".into()),
            },
            ClipOptions::default(),
        );
        assert_eq!(next(&rx), Some(Event::Started { key: "c".into() }));
        assert_eq!(next(&rx), Some(Event::Ended { key: "c".into() }));
    }

    #[test]
    fn stopping_a_clip_still_decoding_ends_it() {
        let (engine, rx) = engine();
        let bytes: Arc<[u8]> = crate::decode::tests::wav(44_100, 2, 440.0, 5.0).into();
        engine.play(
            "c",
            Clip::Encoded { bytes, hint: None },
            ClipOptions::default(),
        );
        engine.stop("c");
        assert_eq!(next(&rx), Some(Event::Ended { key: "c".into() }));
        // The late decode result is dropped: nothing starts.
        let late = rx.recv_timeout(Duration::from_millis(500));
        assert!(late.is_err() || matches!(late, Ok(Event::Level(_))));
    }

    #[test]
    fn decodes_in_flight_are_bounded() {
        let (engine, rx) = engine();
        // Long enough that the first few are still decoding when the rest arrive.
        let bytes: Arc<[u8]> = crate::decode::tests::wav(44_100, 2, 440.0, 30.0).into();
        let plays = 16;
        for index in 0..plays {
            engine.play(
                &format!("c{index}"),
                Clip::Encoded {
                    bytes: Arc::clone(&bytes),
                    hint: Some("wav".into()),
                },
                ClipOptions::default(),
            );
        }
        let mut busy = 0;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Some(Event::Failed { error, .. }) = next(&rx) {
                assert!(error.contains("too many"), "{error}");
                busy += 1;
                break;
            }
        }
        assert!(busy > 0, "no play was refused");
    }

    #[test]
    fn undecodable_clip_fails() {
        let (engine, rx) = engine();
        engine.play(
            "c",
            Clip::Encoded {
                bytes: Arc::from(&b"nope"[..]),
                hint: Some("mp3".into()),
            },
            ClipOptions::default(),
        );
        assert!(matches!(next(&rx), Some(Event::Failed { key, .. }) if key == "c"));
    }

    #[test]
    fn commands_before_the_first_play_are_ignored() {
        let (engine, rx) = engine();
        engine.stop("x");
        engine.stream_seek(10);
        assert!(engine.shared.sender().is_none());
        assert_eq!(rx.recv_timeout(Duration::from_millis(50)).ok(), None);
    }
}
