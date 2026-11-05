//! The public handle and the engine thread that owns the mixer and output.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use crate::decode::{decode_clip, DecodedClip};
use crate::dsp::to_i16;
use crate::mixer::{Mixer, StreamVoice};
use crate::output::{Device, BLOCK_FRAMES};
use crate::stream::queue::StreamQueue;
use crate::stream::source::{Cancel, HttpFetcher, Net};
use crate::stream::{self, describe_url, StreamHandle, StreamJob};
use crate::{tone, Clip, ClipOptions, Event, EventSink, Loudness, StreamRequest};

/// Close the device after this long with nothing audible.
const IDLE_CLOSE: Duration = Duration::from_secs(2);
const POSITION_EVERY: Duration = Duration::from_millis(250);
const LEVEL_EVERY: Duration = Duration::from_millis(100);
const BLOCK: Duration = Duration::from_millis(20);
/// Clips decoding at once; a play past this fails instead of piling up
/// threads and decoded PCM.
const MAX_DECODES: usize = 3;

enum Command {
    SetDevice(Option<String>),
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

/// Where blocks go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputKind {
    /// `waveOut` (falls back to pacing when no device opens).
    Device,
    /// Real-time pacing, no device (tests).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "only tests run without a device")
    )]
    Null,
}

struct Shared {
    events: EventSink,
    output: OutputKind,
    tx: Mutex<Option<Sender<Command>>>,
    device: Mutex<Option<String>>,
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
        Self::with_output(events, OutputKind::Device)
    }

    fn with_output(events: EventSink, output: OutputKind) -> Self {
        Self {
            shared: Arc::new(Shared {
                events,
                output,
                tx: Mutex::new(None),
                device: Mutex::new(None),
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
        let device = self
            .shared
            .device
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let events = Arc::clone(&self.shared.events);
        let worker_sender = sender.clone();
        let output = self.shared.output;
        // The worker owns the (thread-bound) device, so it is built on its thread.
        let spawned = std::thread::Builder::new()
            .name("yap-audio".into())
            .spawn(move || Worker::new(events, worker_sender, device, output).run(&receiver));
        if let Err(error) = spawned {
            eprintln!("[audio] no engine thread: {error}");
            return false;
        }
        let sent = sender.send(command).is_ok();
        *tx = Some(sender);
        sent
    }

    /// Output device by name (as the voice settings store it, i.e. the full Windows endpoint
    /// name); None or no match = system default (WAVE_MAPPER). waveOut names are truncated to
    /// 31 chars (WAVEOUTCAPSW.szPname), so match by prefix / case-insensitively. Switching
    /// mid-play reopens the device without dropping voices.
    pub fn set_output_device(&self, name: Option<String>) {
        *self
            .shared
            .device
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = name.clone();
        self.send(Command::SetDevice(name), false);
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

enum Sink {
    Device(Device),
    /// No device: sleep through each block so voices still end on time.
    Paced(Instant),
}

struct Worker {
    events: EventSink,
    /// For decode threads to hand back clips.
    commands: Sender<Command>,
    output: OutputKind,
    device_name: Option<String>,
    mixer: Mixer,
    sink: Option<Sink>,
    pending: HashMap<String, PendingClip>,
    next_generation: u64,
    /// Decode threads still running (stopped clips' threads count until
    /// they finish).
    decoding: Arc<AtomicUsize>,
    stream: Option<StreamHandle>,
    net: Option<(tokio::runtime::Runtime, Net)>,
    quiet_since: Option<Instant>,
    last_position: Instant,
    last_level: Instant,
    level: f32,
    level_live: bool,
    block: Vec<f32>,
    pcm: Vec<i16>,
}

impl Worker {
    fn new(
        events: EventSink,
        commands: Sender<Command>,
        device_name: Option<String>,
        output: OutputKind,
    ) -> Self {
        Self {
            events,
            commands,
            output,
            device_name,
            mixer: Mixer::default(),
            sink: None,
            pending: HashMap::new(),
            next_generation: 0,
            decoding: Arc::new(AtomicUsize::new(0)),
            stream: None,
            net: None,
            quiet_since: None,
            last_position: Instant::now(),
            last_level: Instant::now(),
            level: 0.0,
            level_live: false,
            block: vec![0.0; BLOCK_FRAMES * 2],
            pcm: vec![0; BLOCK_FRAMES * 2],
        }
    }

    fn emit(&self, event: Event) {
        (self.events)(event);
    }

    fn run(mut self, commands: &Receiver<Command>) {
        loop {
            if self.mixer.is_active() && self.sink.is_none() {
                self.open_sink();
            }
            if self.sink.is_some() {
                loop {
                    match commands.try_recv() {
                        Ok(Command::Shutdown) | Err(TryRecvError::Disconnected) => return,
                        Ok(command) => self.handle(command),
                        Err(TryRecvError::Empty) => break,
                    }
                }
                self.render();
            } else {
                let received = if self.mixer.stream().is_some() || !self.pending.is_empty() {
                    commands.recv_timeout(POSITION_EVERY)
                } else {
                    commands.recv().map_err(|_| RecvTimeoutError::Disconnected)
                };
                match received {
                    Ok(Command::Shutdown) | Err(RecvTimeoutError::Disconnected) => return,
                    Ok(command) => self.handle(command),
                    Err(RecvTimeoutError::Timeout) => {}
                }
            }
            self.report_position();
            self.flush_events();
        }
    }

    fn open_sink(&mut self) {
        let sink = match self.output {
            OutputKind::Device => match Device::open(self.device_name.as_deref()) {
                Ok(device) => {
                    eprintln!(
                        "[audio] output open ({})",
                        self.device_name.as_deref().unwrap_or("default")
                    );
                    Sink::Device(device)
                }
                Err(error) => {
                    eprintln!("[audio] no output device: {error}");
                    Sink::Paced(Instant::now())
                }
            },
            OutputKind::Null => Sink::Paced(Instant::now()),
        };
        self.sink = Some(sink);
        self.quiet_since = None;
    }

    fn close_sink(&mut self) {
        if let Some(Sink::Device(_)) = self.sink.take() {
            eprintln!("[audio] output closed (idle)");
        }
        self.sink = None;
        if self.level_live {
            self.level_live = false;
            self.emit(Event::Level(0.0));
        }
    }

    /// Renders and outputs one block, and closes the device once idle.
    fn render(&mut self) {
        let active = self.mixer.is_active();
        let peak = self.mixer.render(&mut self.block);
        for (out, sample) in self.pcm.iter_mut().zip(&self.block) {
            *out = to_i16(*sample);
        }
        let failed = match &mut self.sink {
            Some(Sink::Device(device)) => device.write(&self.pcm).err(),
            Some(Sink::Paced(next)) => {
                *next += BLOCK;
                let now = Instant::now();
                if *next > now {
                    std::thread::sleep(*next - now);
                } else if now - *next > BLOCK * 5 {
                    *next = now;
                }
                None
            }
            None => None,
        };
        if let Some(error) = failed {
            eprintln!("[audio] output error: {error}; pacing without a device");
            self.sink = Some(Sink::Paced(Instant::now()));
        }
        if active {
            self.quiet_since = None;
            self.level = self.level.max(peak);
            if self.last_level.elapsed() >= LEVEL_EVERY {
                let level = std::mem::take(&mut self.level);
                self.level_live = true;
                self.last_level = Instant::now();
                self.emit(Event::Level(level));
            }
        } else {
            let since = *self.quiet_since.get_or_insert_with(Instant::now);
            if self.level_live {
                self.level_live = false;
                self.level = 0.0;
                self.emit(Event::Level(0.0));
            }
            if since.elapsed() >= IDLE_CLOSE {
                self.close_sink();
            }
        }
    }

    fn report_position(&mut self) {
        if self.last_position.elapsed() < POSITION_EVERY {
            return;
        }
        self.last_position = Instant::now();
        if self
            .mixer
            .stream()
            .is_some_and(|stream| stream.queue().failed())
        {
            // Failed while paused: the stream thread already reported it.
            self.mixer.set_stream(None);
            self.stream = None;
            return;
        }
        if let Some(stream) = self.mixer.stream() {
            let event = Event::StreamPosition {
                key: stream.key().to_string(),
                position_ms: stream.queue().position_ms(),
                paused: stream.paused(),
            };
            self.emit(event);
        } else if self.stream.is_some() {
            // The mixer dropped the stream (failed): stop its thread.
            self.stream = None;
        }
    }

    fn flush_events(&mut self) {
        for event in self.mixer.take_events() {
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
            Command::SetDevice(name) => {
                if name != self.device_name {
                    self.device_name = name;
                    if matches!(self.sink, Some(Sink::Device(_))) {
                        // Reopen on the next block; voices keep their place.
                        self.sink = None;
                        self.open_sink();
                    }
                }
            }
            Command::Play { key, clip, opts } => self.play(key, clip, opts),
            Command::Decoded {
                key,
                generation,
                result,
            } => self.decoded(key, generation, result),
            Command::Stop(key) => {
                eprintln!("[audio] stop {key}");
                if self.pending.remove(&key).is_some() {
                    self.mixer.push_event(Event::Ended { key });
                } else {
                    self.mixer.stop(&key);
                }
            }
            Command::SetPaused(key, paused) => match self.pending.get_mut(&key) {
                Some(pending) => pending.opts.paused = paused,
                None => self.mixer.set_paused(&key, paused),
            },
            Command::SetVolume(key, volume) => match self.pending.get_mut(&key) {
                Some(pending) => pending.opts.volume = volume,
                // Tones scale by the ratio to the volume baked into them.
                None => self.mixer.set_volume(&key, volume),
            },
            Command::StopAll => {
                eprintln!("[audio] stop all");
                for (key, _) in self.pending.drain() {
                    self.mixer.push_event(Event::Ended { key });
                }
                self.mixer.stop_all();
            }
            Command::StreamLoad(request) => self.stream_load(request),
            Command::StreamSeek(position_ms) => {
                if let (Some(stream), Some(handle)) = (self.mixer.stream(), &self.stream) {
                    eprintln!("[audio] stream {}: seek {position_ms} ms", stream.key());
                    let generation = stream.queue().clear(position_ms);
                    handle.seek(position_ms, generation);
                }
            }
            Command::StreamPaused(paused) => self.mixer.set_stream_paused(paused),
            Command::StreamVolume(volume) => self.mixer.set_stream_volume(volume),
            Command::StreamLoudness(loudness) => self.mixer.set_stream_loudness(loudness),
            Command::StreamStop => {
                if let Some(stream) = self.mixer.stream() {
                    eprintln!("[audio] stream {}: stop", stream.key());
                }
                self.stream = None;
                self.mixer.set_stream(None);
            }
            Command::Shutdown => {}
        }
    }

    fn play(&mut self, key: String, clip: Clip, opts: ClipOptions) {
        self.mixer.remove_clip(&key);
        self.pending.remove(&key);
        match clip {
            Clip::Tone(id) => {
                let Some(spec) = tone::find(&id) else {
                    eprintln!("[audio] failed {key}: unknown tone {id:?}");
                    self.mixer.push_event(Event::Failed {
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
                self.mixer.add_clip(key, pcm, &opts, 1.0 / baked);
            }
            Clip::Encoded { bytes, hint } => {
                if self.decoding.load(Ordering::Acquire) >= MAX_DECODES {
                    eprintln!("[audio] failed {key}: too many clips decoding");
                    self.mixer.push_event(Event::Failed {
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
                    self.mixer.push_event(Event::Failed {
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
                self.mixer
                    .add_clip(key, clip.samples.into(), &pending.opts, 1.0);
            }
            Err(error) => {
                eprintln!("[audio] failed {key}: {error}");
                self.mixer.push_event(Event::Failed { key, error });
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
                reqwest::Client::builder()
                    .connect_timeout(Duration::from_secs(10))
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
        self.mixer.set_stream(None);
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
            make_fetcher: move |url: &str, cancel: Cancel| {
                HttpFetcher::new(net.clone(), url, &headers, cancel)
            },
            delay_scale: 1.0,
        };
        match stream::spawn(job, Cancel::default()) {
            Ok(handle) => {
                self.stream = Some(handle);
                self.mixer.set_stream(Some(StreamVoice::new(
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
        engine.set_output_device(Some("Speakers".into()));
        assert!(engine.shared.sender().is_none());
        assert_eq!(rx.recv_timeout(Duration::from_millis(50)).ok(), None);
    }
}
