//! The Listen Together stream: a thread that fetches (`source`), demuxes
//! and decodes (`crate::decode`), resamples, and fills the mixer's queue
//! (`queue`), recovering from failures the way the web does (`recovery`).

pub(crate) mod live;
pub(crate) mod queue;
pub(crate) mod recovery;
pub(crate) mod source;

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{mpsc, Arc};
use std::time::{Duration, Instant};

use symphonia::core::errors::{Error as SymphoniaError, SeekErrorKind};
use symphonia::core::formats::{FormatReader, SeekMode, SeekTo};
use symphonia::core::units::{Time, TimeBase};

use crate::decode::{self, PacketDecoder, PacketError};
use crate::resample::StreamResampler;
use crate::{Event, EventSink, StreamFailure, SAMPLE_RATE};
use live::LiveSource;
use queue::{ms_to_frames, StreamQueue};
use recovery::{
    alternate_format_url, format_of, plan_recovery, Action, MediaError, Probe, RecoveryState,
};
use source::{fetch_error, Cancel, FetchError, Net, RangeFetcher, RangeSource};

/// Decoded audio kept ahead of the mixer.
const BUFFER_FRAMES: usize = 3 * SAMPLE_RATE as usize;
/// Consecutive undecodable packets before the stream counts as undecodable.
const MAX_BAD_PACKETS: usize = 50;

/// Host, path and `format` of a stream URL: safe to log (the query carries
/// the voice session id).
pub(crate) fn describe_url(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(parsed) => format!(
            "{}{} format={}",
            parsed.host_str().unwrap_or("?"),
            parsed.path(),
            format_of(url).as_deref().unwrap_or("default")
        ),
        Err(_) => "<invalid url>".into(),
    }
}

/// Commands from the engine thread.
pub(crate) enum StreamCommand {
    /// The queue was already cleared to `generation` at `position_ms`.
    Seek {
        position_ms: u64,
        generation: u64,
    },
    Stop,
}

/// What the engine keeps of a running stream.
pub(crate) struct StreamHandle {
    commands: Sender<StreamCommand>,
    cancel: Cancel,
}

impl StreamHandle {
    pub(crate) fn seek(&self, position_ms: u64, generation: u64) {
        let _ = self.commands.send(StreamCommand::Seek {
            position_ms,
            generation,
        });
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.cancel.cancel();
        let _ = self.commands.send(StreamCommand::Stop);
    }
}

/// Everything a stream thread needs.
pub(crate) struct StreamJob<M> {
    pub key: String,
    pub url: String,
    pub position_ms: u64,
    pub queue: Arc<StreamQueue>,
    pub events: EventSink,
    /// Makes a fetcher for a URL (HTTP, or memory in tests).
    pub make_fetcher: M,
    /// Multiplies retry delays (1.0; tests shrink them).
    pub delay_scale: f64,
    /// A live radio stream on this network: one plain GET, read to its
    /// end, never sought (`make_fetcher` is not used).
    pub live: Option<Net>,
}

/// Spawns the stream thread.
pub(crate) fn spawn<M, F>(job: StreamJob<M>, cancel: Cancel) -> std::io::Result<StreamHandle>
where
    M: Fn(&str, Cancel) -> Result<F, String> + Send + 'static,
    F: RangeFetcher,
{
    let (commands, receiver) = mpsc::channel();
    let thread_cancel = cancel.clone();
    std::thread::Builder::new()
        .name("yap-audio-stream".into())
        .spawn(move || run(job, &receiver, &thread_cancel))?;
    Ok(StreamHandle { commands, cancel })
}

/// How one attempt ended.
enum Outcome {
    /// Stopped or replaced.
    Stopped,
    /// Played to the end and drained; nothing more to do.
    Done,
    Failed {
        media_error: Option<MediaError>,
        probe: Option<Probe>,
        detail: String,
        /// Where to resume.
        resume_ms: u64,
        /// The attempt pushed audio (the web resets its retry budget then).
        played: bool,
    },
}

struct Attempt<'a> {
    queue: &'a StreamQueue,
    commands: &'a Receiver<StreamCommand>,
    cancel: &'a Cancel,
    key: &'a str,
}

fn probe_for(error: &FetchError) -> Option<Option<Probe>> {
    match error {
        FetchError::Status {
            status,
            retry_after_secs,
        } => Some(Some(Probe {
            status: *status,
            retry_after_secs: *retry_after_secs,
        })),
        FetchError::Network(_) => Some(None),
        FetchError::Cancelled => None,
    }
}

fn run<M, F>(job: StreamJob<M>, commands: &Receiver<StreamCommand>, cancel: &Cancel)
where
    M: Fn(&str, Cancel) -> Result<F, String>,
    F: RangeFetcher,
{
    let StreamJob {
        key,
        mut url,
        mut position_ms,
        queue,
        events,
        make_fetcher,
        delay_scale,
        live,
    } = job;
    let mut state = RecoveryState::default();
    let mut generation = queue.generation();
    loop {
        if cancel.is_cancelled() {
            return;
        }
        let attempt = Attempt {
            queue: &queue,
            commands,
            cancel,
            key: &key,
        };
        let opened = match &live {
            Some(_) => Ok(None),
            None => make_fetcher(&url, cancel.clone()).map(Some),
        };
        let outcome = match (opened, &live) {
            (Ok(None), Some(net)) => play_live(&attempt, net, &url, &mut generation),
            (Ok(None), None) => Outcome::Stopped,
            (Ok(Some(fetcher)), _) => play(&attempt, fetcher, &url, position_ms, &mut generation),
            (Err(detail), _) => {
                // A URL or header that cannot be built will not build on a
                // retry either: fail now instead of "reconnecting".
                eprintln!("[audio] stream {key}: {detail}; Fail (no fetcher)");
                (events)(Event::StreamFailed {
                    key: key.clone(),
                    failure: StreamFailure {
                        status: None,
                        code: "PLAYBACK_FAILED".to_string(),
                        message:
                            "Playback stopped and couldn't recover. Skip the track or try again."
                                .to_string(),
                    },
                });
                queue.finish(generation, true);
                return;
            }
        };
        let (media_error, probe, detail, resume_ms, played) = match outcome {
            Outcome::Stopped | Outcome::Done => return,
            Outcome::Failed {
                media_error,
                probe,
                detail,
                resume_ms,
                played,
            } => (media_error, probe, detail, resume_ms, played),
        };
        if cancel.is_cancelled() {
            return;
        }
        if played {
            state = RecoveryState::default();
        }
        position_ms = resume_ms;
        let alternate = alternate_format_url(&url);
        let plan = plan_recovery(media_error, probe, state, alternate.is_some());
        eprintln!(
            "[audio] stream {key}: {detail}; {:?} in {} ms ({})",
            plan.action, plan.delay_ms, plan.code
        );
        match plan.action {
            Action::Fail => {
                (events)(Event::StreamFailed {
                    key: key.clone(),
                    failure: StreamFailure {
                        status: probe
                            .map(|probe| probe.status)
                            .filter(|status| *status >= 400),
                        code: plan.code.to_string(),
                        message: plan.message,
                    },
                });
                queue.finish(generation, true);
                return;
            }
            Action::SwitchFormat => {
                (events)(Event::StreamRetrying {
                    key: key.clone(),
                    delay_ms: 0,
                    message: plan.message,
                });
                state.format_switched = true;
                if let Some(alternate) = alternate {
                    eprintln!(
                        "[audio] stream {key}: switching to {}",
                        describe_url(&alternate)
                    );
                    url = alternate;
                }
            }
            Action::Retry => {
                (events)(Event::StreamRetrying {
                    key: key.clone(),
                    delay_ms: plan.delay_ms,
                    message: plan.message,
                });
                state.transient_retries += 1;
                let delay = Duration::from_secs_f64(plan.delay_ms as f64 / 1000.0 * delay_scale);
                let deadline = Instant::now() + delay;
                loop {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break;
                    }
                    match commands.recv_timeout(left) {
                        Ok(StreamCommand::Seek {
                            position_ms: to,
                            generation: next,
                        }) => {
                            position_ms = to;
                            generation = next;
                        }
                        Ok(StreamCommand::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                        Err(RecvTimeoutError::Timeout) => break,
                    }
                }
            }
        }
    }
}

fn secs(time: Time) -> f64 {
    time.seconds as f64 + time.frac
}

/// Drains pending commands. Returns false on stop.
fn poll_commands(attempt: &Attempt<'_>, seek: &mut Option<(u64, u64)>) -> bool {
    loop {
        match attempt.commands.try_recv() {
            Ok(StreamCommand::Seek {
                position_ms,
                generation,
            }) => *seek = Some((position_ms, generation)),
            Ok(StreamCommand::Stop) | Err(TryRecvError::Disconnected) => return false,
            Err(TryRecvError::Empty) => return !attempt.cancel.is_cancelled(),
        }
    }
}

/// Decoder state for one attempt.
struct Pipeline {
    format: Box<dyn FormatReader>,
    track_id: u32,
    time_base: Option<TimeBase>,
    decoder: PacketDecoder,
    resampler: Option<(u32, StreamResampler)>,
    /// Media position (48 kHz frames) of the next frame to queue.
    out_frame: u64,
    /// Drop decoded audio before this time (seconds) after a seek.
    discard_until: Option<f64>,
    decoded: Vec<f32>,
    resampled: Vec<f32>,
}

impl Pipeline {
    fn position_ms(&self) -> u64 {
        queue::frames_to_ms(self.out_frame)
    }

    /// Seeks to `position_ms`. Ok(false): past the end.
    fn seek(&mut self, position_ms: u64) -> Result<bool, SymphoniaError> {
        let target = position_ms as f64 / 1000.0;
        let time = Time::new(position_ms / 1000, (position_ms % 1000) as f64 / 1000.0);
        let seeked = self.format.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time,
                track_id: Some(self.track_id),
            },
        );
        match seeked {
            Ok(_) => {}
            Err(SymphoniaError::SeekError(SeekErrorKind::OutOfRange)) => return Ok(false),
            Err(error) => return Err(error),
        }
        // Errors here only mean the decoder keeps stale state for a packet.
        let _ = self.decoder.reset();
        if let Some((_, resampler)) = &mut self.resampler {
            resampler.reset();
        }
        self.out_frame = ms_to_frames(position_ms);
        self.discard_until = Some(target);
        Ok(true)
    }

    /// Decodes one packet into `self.resampled` (48 kHz stereo).
    fn decode(&mut self, packet: &symphonia::core::formats::Packet) -> Result<(), PacketError> {
        self.decoded.clear();
        self.resampled.clear();
        let rate = self.decoder.decode(packet, &mut self.decoded)?;
        if let (Some(until), Some(time_base)) = (self.discard_until, self.time_base) {
            let start = secs(time_base.calc_time(packet.ts));
            let drop_secs = until - start;
            if drop_secs > 0.0 {
                let frames = (drop_secs * f64::from(rate)).round() as usize;
                let samples = (frames * 2).min(self.decoded.len());
                self.decoded.drain(..samples);
                if self.decoded.is_empty() {
                    return Ok(());
                }
            }
            self.discard_until = None;
        }
        let rebuild = self
            .resampler
            .as_ref()
            .is_none_or(|(current, _)| *current != rate);
        if rebuild {
            let resampler = StreamResampler::new(rate).map_err(PacketError::Fatal)?;
            self.resampler = Some((rate, resampler));
        }
        if let Some((_, resampler)) = &mut self.resampler {
            resampler
                .push(&self.decoded, &mut self.resampled)
                .map_err(PacketError::Fatal)?;
        }
        Ok(())
    }

    fn flush(&mut self) {
        self.resampled.clear();
        if let Some((_, resampler)) = &mut self.resampler {
            let _ = resampler.flush(&mut self.resampled);
        }
    }
}

fn failed(
    media_error: Option<MediaError>,
    probe: Option<Probe>,
    detail: String,
    resume_ms: u64,
    played: bool,
) -> Outcome {
    Outcome::Failed {
        media_error,
        probe,
        detail,
        resume_ms,
        played,
    }
}

/// The route answered (2xx) but the media did not work.
fn healthy() -> Option<Probe> {
    Some(Probe {
        status: 200,
        retry_after_secs: None,
    })
}

/// A symphonia error during reading: a fetch failure inside, or a media one.
fn classify(error: &SymphoniaError) -> Option<(Option<MediaError>, Option<Probe>)> {
    if let SymphoniaError::IoError(io) = error {
        return match fetch_error(io) {
            Some(fetch) => probe_for(fetch).map(|probe| (Some(MediaError::Network), probe)),
            None => Some((Some(MediaError::Network), None)),
        };
    }
    Some((Some(MediaError::Decode), healthy()))
}

/// One attempt: open, decode until the end, a failure or a stop.
fn play<F: RangeFetcher>(
    attempt: &Attempt<'_>,
    fetcher: F,
    url: &str,
    start_ms: u64,
    generation: &mut u64,
) -> Outcome {
    let source = match RangeSource::open(fetcher) {
        Ok(source) => source,
        Err(error) => {
            return match probe_for(&error) {
                Some(probe) => failed(
                    Some(MediaError::Network),
                    probe,
                    error.to_string(),
                    start_ms,
                    false,
                ),
                None => Outcome::Stopped,
            }
        }
    };
    let bytes = source.len();
    let hint = decode::hint(match format_of(url).as_deref() {
        Some("webm") => Some("webm"),
        _ => Some("m4a"),
    });
    play_opened(
        attempt,
        Box::new(source),
        bytes,
        &hint,
        url,
        start_ms,
        generation,
        false,
    )
}

/// One attempt at a live radio stream: open, decode until it ends, fails
/// or is stopped. It always starts at 0 (the web never seeks radio).
fn play_live(attempt: &Attempt<'_>, net: &Net, url: &str, generation: &mut u64) -> Outcome {
    let source = match LiveSource::open(net, url, attempt.cancel.clone()) {
        Ok(source) => source,
        Err(error) => {
            return match probe_for(&error) {
                Some(probe) => failed(
                    Some(MediaError::Network),
                    probe,
                    error.to_string(),
                    0,
                    false,
                ),
                None => Outcome::Stopped,
            }
        }
    };
    let hint = decode::hint(source.hint().as_deref());
    play_opened(
        attempt,
        Box::new(source),
        None,
        &hint,
        url,
        0,
        generation,
        true,
    )
}

/// Decodes an opened source into the queue. A `live` source has no end: if
/// it finishes cleanly the station dropped us, which is a network failure
/// (the caller reconnects with backoff).
#[allow(clippy::too_many_arguments)]
fn play_opened(
    attempt: &Attempt<'_>,
    source: Box<dyn symphonia::core::io::MediaSource>,
    bytes: Option<u64>,
    hint: &symphonia::core::probe::Hint,
    url: &str,
    start_ms: u64,
    generation: &mut u64,
    live: bool,
) -> Outcome {
    let key = attempt.key;
    let (format, track) = match decode::open(source, hint) {
        Ok(opened) => opened,
        Err(error) => {
            if attempt.cancel.is_cancelled() {
                return Outcome::Stopped;
            }
            return match classify(&error) {
                Some((media, probe)) => {
                    let media = if media == Some(MediaError::Decode) {
                        Some(MediaError::SrcNotSupported)
                    } else {
                        media
                    };
                    failed(media, probe, format!("open: {error}"), start_ms, false)
                }
                None => Outcome::Stopped,
            };
        }
    };
    let decoder = match PacketDecoder::new(&track.codec_params) {
        Ok(decoder) => decoder,
        Err(error) => {
            return failed(
                Some(MediaError::SrcNotSupported),
                healthy(),
                error,
                start_ms,
                false,
            )
        }
    };
    eprintln!(
        "[audio] stream {key}: open {} ({} bytes, {})",
        describe_url(url),
        bytes.map_or_else(|| "?".to_string(), |bytes| bytes.to_string()),
        decoder.name()
    );
    let mut pipeline = Pipeline {
        format,
        track_id: track.id,
        time_base: track.codec_params.time_base,
        decoder,
        resampler: None,
        out_frame: ms_to_frames(start_ms),
        discard_until: None,
        decoded: Vec::new(),
        resampled: Vec::new(),
    };
    let mut played = false;
    let mut pending_seek = if start_ms > 0 {
        Some((start_ms, *generation))
    } else {
        None
    };
    let mut bad_packets = 0usize;
    let mut at_end = false;
    loop {
        if !poll_commands(attempt, &mut pending_seek) {
            return Outcome::Stopped;
        }
        if let Some((position_ms, next_generation)) = pending_seek.take() {
            *generation = next_generation;
            eprintln!("[audio] stream {key}: seek {position_ms} ms");
            match pipeline.seek(position_ms) {
                Ok(true) => at_end = false,
                Ok(false) => {
                    pipeline.out_frame = ms_to_frames(position_ms);
                    at_end = true;
                    attempt.queue.finish(*generation, false);
                }
                Err(error) => {
                    if attempt.cancel.is_cancelled() {
                        return Outcome::Stopped;
                    }
                    return match classify(&error) {
                        Some((media, probe)) => {
                            failed(media, probe, format!("seek: {error}"), position_ms, played)
                        }
                        None => Outcome::Stopped,
                    };
                }
            }
        }
        if at_end {
            // Wait for a seek back, a stop, or the mixer to drain and drop us.
            match attempt.commands.recv_timeout(Duration::from_millis(250)) {
                Ok(StreamCommand::Seek {
                    position_ms,
                    generation,
                }) => pending_seek = Some((position_ms, generation)),
                Ok(StreamCommand::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    return Outcome::Done
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
            continue;
        }
        if !attempt
            .queue
            .wait_for_room(BUFFER_FRAMES, Duration::from_millis(50))
        {
            continue;
        }
        let packet = match pipeline.format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                pipeline.flush();
                let start = pipeline.out_frame;
                let samples = std::mem::take(&mut pipeline.resampled);
                if attempt.queue.push(*generation, start, samples) {
                    played = true;
                }
                eprintln!(
                    "[audio] stream {key}: end of stream at {} ms",
                    pipeline.position_ms()
                );
                if live {
                    return failed(
                        Some(MediaError::Network),
                        None,
                        "the station closed the stream".into(),
                        0,
                        played,
                    );
                }
                attempt.queue.finish(*generation, false);
                at_end = true;
                continue;
            }
            Err(error) => {
                if attempt.cancel.is_cancelled() {
                    return Outcome::Stopped;
                }
                let resume = pipeline.position_ms();
                return match classify(&error) {
                    Some((media, probe)) => {
                        failed(media, probe, format!("read: {error}"), resume, played)
                    }
                    None => Outcome::Stopped,
                };
            }
        };
        if packet.track_id() != pipeline.track_id {
            continue;
        }
        match pipeline.decode(&packet) {
            Ok(()) => bad_packets = 0,
            Err(PacketError::Skip(error)) => {
                bad_packets += 1;
                if bad_packets >= MAX_BAD_PACKETS {
                    return failed(
                        Some(MediaError::Decode),
                        healthy(),
                        format!("{MAX_BAD_PACKETS} undecodable packets in a row: {error}"),
                        pipeline.position_ms(),
                        played,
                    );
                }
                continue;
            }
            Err(PacketError::Fatal(error)) => {
                return failed(
                    Some(MediaError::Decode),
                    healthy(),
                    error,
                    pipeline.position_ms(),
                    played,
                )
            }
        }
        if pipeline.resampled.is_empty() {
            continue;
        }
        let frames = (pipeline.resampled.len() / 2) as u64;
        let start = pipeline.out_frame;
        let samples = std::mem::take(&mut pipeline.resampled);
        if attempt.queue.push(*generation, start, samples) {
            played = true;
        }
        pipeline.out_frame += frames;
    }
}

#[cfg(test)]
mod tests {
    use super::source::tests::MemFetcher;
    use super::*;
    use crate::decode::tests::wav;
    use std::sync::Mutex;

    fn collect() -> (EventSink, Arc<Mutex<Vec<Event>>>) {
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink_events = Arc::clone(&events);
        let sink: EventSink = Arc::new(move |event| {
            if let Ok(mut events) = sink_events.lock() {
                events.push(event);
            }
        });
        (sink, events)
    }

    fn drain(queue: &StreamQueue) -> Vec<f32> {
        let mut all = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let mut block = vec![0.0; 1920];
            let pulled = queue.pull(&mut block);
            all.extend_from_slice(&block[..pulled.frames * 2]);
            if pulled.over || Instant::now() > deadline {
                return all;
            }
            if pulled.frames == 0 {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    }

    fn job<M>(
        url: &str,
        position_ms: u64,
        queue: &Arc<StreamQueue>,
        events: EventSink,
        make_fetcher: M,
    ) -> StreamJob<M> {
        StreamJob {
            key: "entry".into(),
            url: url.into(),
            position_ms,
            queue: Arc::clone(queue),
            events,
            make_fetcher,
            delay_scale: 0.001,
            live: None,
        }
    }

    const URL: &str = "https://yap.test/api/listen-together/stream?videoId=v&format=mp4";

    #[test]
    fn streams_a_wav_through_the_queue() {
        let data = wav(44_100, 2, 440.0, 2.0);
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let make = move |_: &str, _: Cancel| Ok(MemFetcher::new(data.clone()));
        let handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        assert!(handle.is_ok());
        let pcm = drain(&queue);
        let frames = pcm.len() / 2;
        assert!((95_900..=96_100).contains(&frames), "{frames}");
        assert!(queue.ended());
        assert!(events
            .lock()
            .map(|events| events.is_empty())
            .unwrap_or(false));
    }

    #[test]
    fn a_live_source_decodes_without_seeking() {
        let data = wav(44_100, 2, 440.0, 1.0);
        let queue = Arc::new(StreamQueue::new(0));
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        for chunk in data.chunks(4096) {
            tx.try_send(Ok(chunk.to_vec())).expect("send");
        }
        drop(tx);
        let (stop, commands) = mpsc::channel();
        let decoder = std::thread::spawn({
            let queue = Arc::clone(&queue);
            move || {
                let source = LiveSource::from_channel(rx, Some("audio/wav".into()));
                let cancel = Cancel::default();
                let attempt = Attempt {
                    queue: &queue,
                    commands: &commands,
                    cancel: &cancel,
                    key: "radio",
                };
                let hint = decode::hint(source.hint().as_deref());
                let mut generation = queue.generation();
                play_opened(
                    &attempt,
                    Box::new(source),
                    None,
                    &hint,
                    URL,
                    0,
                    &mut generation,
                    true,
                )
            }
        });
        let outcome = decoder.join().expect("decoder");
        drop(stop);
        // A live stream never ends on its own: a clean end is a dropped
        // connection, retried like any network failure, and the queue is
        // left open for the audio that follows the reconnect.
        match outcome {
            Outcome::Failed {
                media_error,
                probe,
                resume_ms,
                played,
                ..
            } => {
                assert_eq!(media_error, Some(MediaError::Network));
                assert!(probe.is_none());
                assert_eq!(resume_ms, 0);
                assert!(played);
            }
            _ => panic!("expected a network failure"),
        }
        assert!(!queue.ended());
        let mut frames = 0;
        loop {
            let mut block = vec![0.0; 1920];
            let pulled = queue.pull(&mut block);
            frames += pulled.frames;
            if pulled.frames == 0 {
                break;
            }
        }
        assert!((44_000..=48_200).contains(&frames), "{frames}");
    }

    #[test]
    fn starts_at_the_requested_position() {
        let data = wav(48_000, 1, 440.0, 2.0);
        let queue = Arc::new(StreamQueue::new(1500));
        let (sink, _) = collect();
        let make = move |_: &str, _: Cancel| Ok(MemFetcher::new(data.clone()));
        let handle = spawn(job(URL, 1500, &queue, sink, make), Cancel::default());
        assert!(handle.is_ok());
        let pcm = drain(&queue);
        assert_eq!(pcm.len() / 2, 24_000);
        assert_eq!(queue.position_ms(), 2000);
    }

    #[test]
    fn session_refusal_fails_at_once_with_the_web_copy() {
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let make = |_: &str, _: Cancel| {
            let mut fetcher = MemFetcher::new(Vec::new());
            fetcher.fail_from = Some((
                0,
                FetchError::Status {
                    status: 403,
                    retry_after_secs: None,
                },
            ));
            Ok(fetcher)
        };
        let _handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        drain(&queue);
        let events = events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default();
        assert_eq!(
            events,
            [Event::StreamFailed {
                key: "entry".into(),
                failure: StreamFailure {
                    status: Some(403),
                    code: "PLAYBACK_SESSION".into(),
                    message: "Listening needs an active voice connection. Rejoin the channel to keep listening.".into(),
                },
            }]
        );
    }

    #[test]
    fn blocked_route_retries_three_times_then_fails() {
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let make = |_: &str, _: Cancel| {
            let mut fetcher = MemFetcher::new(Vec::new());
            fetcher.fail_from = Some((
                0,
                FetchError::Status {
                    status: 503,
                    retry_after_secs: Some(20.0),
                },
            ));
            Ok(fetcher)
        };
        let _handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        drain(&queue);
        let events = events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default();
        let delays: Vec<u64> = events
            .iter()
            .filter_map(|event| match event {
                Event::StreamRetrying { delay_ms, .. } => Some(*delay_ms),
                _ => None,
            })
            .collect();
        assert_eq!(delays, [20_000, 40_000, 60_000]);
        assert!(matches!(
            events.last(),
            Some(Event::StreamFailed { failure, .. }) if failure.code == "PLAYBACK_UNAVAILABLE"
        ));
    }

    #[test]
    fn undecodable_mp4_switches_to_webm_once() {
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let urls = Arc::new(Mutex::new(Vec::new()));
        let seen = Arc::clone(&urls);
        let make = move |url: &str, _: Cancel| {
            if let Ok(mut seen) = seen.lock() {
                seen.push(url.to_string());
            }
            Ok(MemFetcher::new(b"not a media file at all".to_vec()))
        };
        let _handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        drain(&queue);
        let urls = urls.lock().map(|urls| urls.clone()).unwrap_or_default();
        assert_eq!(urls.len(), 2);
        assert_eq!(format_of(&urls[1]).as_deref(), Some("webm"));
        let events = events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default();
        assert!(matches!(
            &events[0],
            Event::StreamRetrying { delay_ms: 0, .. }
        ));
        assert!(matches!(
            &events[1],
            Event::StreamFailed { failure, .. } if failure.code == "PLAYBACK_SRC_NOT_SUPPORTED"
        ));
    }

    #[test]
    fn an_unbuildable_fetcher_fails_at_once_without_retrying() {
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let tries = Arc::new(Mutex::new(0usize));
        let counted = Arc::clone(&tries);
        let make = move |_: &str, _: Cancel| {
            if let Ok(mut tries) = counted.lock() {
                *tries += 1;
            }
            Err::<MemFetcher, String>("bad stream URL: relative URL without a base".into())
        };
        let _handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        drain(&queue);
        assert_eq!(tries.lock().map(|tries| *tries).unwrap_or(0), 1);
        let events = events
            .lock()
            .map(|events| events.clone())
            .unwrap_or_default();
        assert_eq!(events.len(), 1, "{events:?}");
        assert!(matches!(
            &events[0],
            Event::StreamFailed { failure, .. } if failure.code == "PLAYBACK_FAILED"
        ));
        assert!(queue.failed());
    }

    #[test]
    fn stop_ends_the_thread_without_events() {
        let data = wav(48_000, 2, 440.0, 30.0);
        let queue = Arc::new(StreamQueue::new(0));
        let (sink, events) = collect();
        let make = move |_: &str, _: Cancel| Ok(MemFetcher::new(data.clone()));
        let handle = spawn(job(URL, 0, &queue, sink, make), Cancel::default());
        std::thread::sleep(Duration::from_millis(50));
        drop(handle);
        std::thread::sleep(Duration::from_millis(100));
        assert!(!queue.is_finished());
        assert!(events
            .lock()
            .map(|events| events.is_empty())
            .unwrap_or(false));
    }
}
