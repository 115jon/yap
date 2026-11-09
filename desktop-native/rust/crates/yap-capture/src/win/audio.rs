//! Share audio: WASAPI loopback of the system mix (monitor shares) or of one
//! process tree (window shares), or the synthetic tone of
//! `YAP_FAKE_SHARE_AUDIO=1`. Everything goes to `push_screen_audio` as
//! 48 kHz stereo 16-bit PCM.

use std::mem::size_of;
use std::sync::mpsc;
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{implement, Interface, Ref};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    eConsole, eRender, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, AUDIOCLIENT_ACTIVATION_PARAMS,
    AUDIOCLIENT_ACTIVATION_PARAMS_0, AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
    AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS, PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::{CoCreateInstance, BLOB, CLSCTX_ALL};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
use windows::Win32::System::Variant::VT_BLOB;
use yap_voice::{push_screen_audio, SHARE_AUDIO_CHANNELS, SHARE_AUDIO_RATE};

use super::ensure_mta;
use crate::pcm::{push_sine, silence_frames_to_pad};
use crate::stop::Stop;

/// Where the share's audio comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AudioSource {
    /// `YAP_FAKE_SHARE_AUDIO=1`.
    Fake,
    /// Everything the default output plays (monitor shares).
    System,
    /// One process and its children (window shares).
    Process(u32),
}

impl AudioSource {
    pub(super) fn describe(self) -> String {
        match self {
            Self::Fake => "fake 440 Hz sine".to_owned(),
            Self::System => "system loopback".to_owned(),
            Self::Process(pid) => format!("process loopback pid={pid}"),
        }
    }
}

/// 10 ms of sample frames: the fake tone's chunk.
const FAKE_CHUNK_FRAMES: usize = (SHARE_AUDIO_RATE / 100) as usize;
/// How long the read loop waits for the device before checking the clock
/// (loopback delivers nothing while nothing renders).
const POLL: Duration = Duration::from_millis(20);
/// Silence pushed per call when padding: 10 ms.
const SILENCE_CHUNK_FRAMES: usize = FAKE_CHUNK_FRAMES;

/// A running audio thread. Dropping it stops and joins the thread.
pub(super) struct Audio {
    stop: Arc<Stop>,
    thread: Option<JoinHandle<()>>,
    started: mpsc::Receiver<Result<(), String>>,
}

impl Audio {
    pub(super) fn spawn(source: AudioSource) -> Result<Self, String> {
        let stop = Arc::new(Stop::default());
        let (report, started) = mpsc::sync_channel(1);
        let thread = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("yap-share-audio".to_owned())
                .spawn(move || run(source, &stop, &report))
                .map_err(|e| format!("could not start the audio thread: {e}"))?
        };
        Ok(Self {
            stop,
            thread: Some(thread),
            started,
        })
    }

    /// Waits for the thread to report that audio is flowing (or why not).
    pub(super) fn wait_started(&self, timeout: Duration) -> Result<(), String> {
        self.started
            .recv_timeout(timeout)
            .map_err(|_| "timed out starting".to_owned())?
    }

    /// Stops a thread that did not start without waiting for it: it may be
    /// stuck in the OS for seconds, and exits as soon as it gets out.
    pub(super) fn abandon(mut self) {
        self.stop.stop();
        self.thread = None;
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.stop.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(source: AudioSource, stop: &Stop, report: &mpsc::SyncSender<Result<(), String>>) {
    ensure_mta();
    let result = match source {
        AudioSource::Fake => {
            let _ = report.send(Ok(()));
            run_fake(stop);
            Ok(())
        }
        AudioSource::System => open_system_loopback().and_then(|c| run_loopback(&c, stop, report)),
        AudioSource::Process(pid) => {
            open_process_loopback(pid).and_then(|c| run_loopback(&c, stop, report))
        }
    };
    if let Err(error) = result {
        // After a start report this is the thread ending on a device error;
        // before one it is the start failing.
        let _ = report.try_send(Err(error.clone()));
        eprintln!("[capture] audio stopped: {error}");
    }
}

/// The 440 Hz tone, paced by the wall clock in 10 ms chunks.
fn run_fake(stop: &Stop) {
    let started = Instant::now();
    let mut chunk = Vec::with_capacity(FAKE_CHUNK_FRAMES * SHARE_AUDIO_CHANNELS as usize);
    let mut sent = 0u64;
    loop {
        let due = started + Duration::from_millis(10) * u32::try_from(sent + 1).unwrap_or(u32::MAX);
        if stop.wait(due.saturating_duration_since(Instant::now())) {
            return;
        }
        chunk.clear();
        push_sine(
            &mut chunk,
            sent * FAKE_CHUNK_FRAMES as u64,
            FAKE_CHUNK_FRAMES,
        );
        push_screen_audio(&chunk);
        sent += 1;
    }
}

// ── Loopback clients ───────────────────────────────────────────────────────

/// 48 kHz stereo 16-bit PCM: what `push_screen_audio` takes. With
/// `AUTOCONVERTPCM` the audio engine converts the device's mix to it.
fn pcm_format() -> WAVEFORMATEX {
    let channels = SHARE_AUDIO_CHANNELS as u16;
    let block_align = channels * 2;
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: SHARE_AUDIO_RATE,
        nAvgBytesPerSec: SHARE_AUDIO_RATE * u32::from(block_align),
        nBlockAlign: block_align,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

/// The default output device's mix, initialised for loopback.
fn open_system_loopback() -> Result<IAudioClient, String> {
    // SAFETY: plain COM calls on interfaces we own; the format outlives
    // `Initialize`.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| format!("no audio device enumerator: {e}"))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|e| format!("no default output device: {e}"))?;
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|e| format!("could not open the output device: {e}"))?;
        let format = pcm_format();
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK
                    | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                0,
                0,
                &format,
                None,
            )
            .map_err(|e| format!("could not start system loopback: {e}"))?;
        Ok(client)
    }
}

/// One process tree's audio, initialised for loopback.
fn open_process_loopback(pid: u32) -> Result<IAudioClient, String> {
    let client = activate_process_loopback(pid)?;
    let format = pcm_format();
    // SAFETY: the format outlives the call.
    unsafe {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK
                | AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
            0,
            0,
            &format,
            None,
        )
    }
    .map_err(|e| format!("could not start process loopback: {e}"))?;
    Ok(client)
}

type Activated = Result<IAudioClient, String>;

/// Receives the result of `ActivateAudioInterfaceAsync`.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct ActivationHandler {
    result: Mutex<Option<mpsc::SyncSender<Activated>>>,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for ActivationHandler_Impl {
    fn ActivateCompleted(
        &self,
        operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> windows::core::Result<()> {
        let mut activation = windows::core::HRESULT(0);
        let mut activated = None;
        let client = (|| {
            let operation = operation.ok().map_err(|e| e.to_string())?;
            // SAFETY: the out values are live locals.
            unsafe { operation.GetActivateResult(&mut activation, &mut activated) }
                .map_err(|e| format!("GetActivateResult failed: {e}"))?;
            activation
                .ok()
                .map_err(|e| format!("audio interface activation failed: {e}"))?;
            activated
                .ok_or_else(|| "activation returned no interface".to_owned())?
                .cast::<IAudioClient>()
                .map_err(|e| format!("the activated interface is not an IAudioClient: {e}"))
        })();
        if let Some(sender) = self
            .result
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
        {
            let _ = sender.send(client);
        }
        Ok(())
    }
}

/// An `IAudioClient` for the audio of `pid` and its children.
fn activate_process_loopback(pid: u32) -> Activated {
    let mut params = AUDIOCLIENT_ACTIVATION_PARAMS {
        ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
            ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                TargetProcessId: pid,
                ProcessLoopbackMode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        },
    };
    let mut variant = PROPVARIANT::default();
    // SAFETY: the blob points at `params`, which lives until the end of this
    // function, past the activation's completion.
    unsafe {
        let inner = &mut *variant.Anonymous.Anonymous;
        inner.vt = VT_BLOB;
        inner.Anonymous.blob = BLOB {
            cbSize: size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
            pBlobData: std::ptr::from_mut(&mut params).cast::<u8>(),
        };
    }

    let (sender, receiver) = mpsc::sync_channel(1);
    let handler: IActivateAudioInterfaceCompletionHandler = ActivationHandler {
        result: Mutex::new(Some(sender)),
    }
    .into();
    // SAFETY: the variant and the handler are live for the call.
    let operation = unsafe {
        ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID,
            Some(&variant),
            &handler,
        )
    }
    .map_err(|e| format!("ActivateAudioInterfaceAsync failed: {e}"));
    let client = operation.and_then(|_operation| {
        receiver
            .recv_timeout(Duration::from_secs(5))
            .map_err(|_| "timed out activating process loopback".to_owned())?
    });

    // The variant's blob borrows `params`; dropping the variant would have
    // `PropVariantClear` free that stack memory.
    std::mem::forget(variant);
    client
}

// ── The read loop ──────────────────────────────────────────────────────────

/// Closes the event handle on every exit path.
struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: a handle we created, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Stops the client on every exit path.
struct Running<'a>(&'a IAudioClient);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        // SAFETY: a plain call on a live client.
        let _ = unsafe { self.0.Stop() };
    }
}

/// Reads the loopback until `stop`, pushing what it plays and, when it plays
/// nothing, silence at the pace of the clock.
fn run_loopback(
    client: &IAudioClient,
    stop: &Stop,
    report: &mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    // SAFETY: COM calls on the live client; the event outlives the stream
    // (`running` stops it first, then `event` closes).
    let (event, capture) = unsafe {
        let event = CreateEventW(None, false, false, None)
            .map_err(|e| format!("could not create the audio event: {e}"))?;
        let event = Event(event);
        client
            .SetEventHandle(event.0)
            .map_err(|e| format!("could not set the audio event: {e}"))?;
        let capture: IAudioCaptureClient = client
            .GetService()
            .map_err(|e| format!("no capture client: {e}"))?;
        (event, capture)
    };
    // SAFETY: a plain call on the live client.
    unsafe { client.Start() }.map_err(|e| format!("could not start the audio stream: {e}"))?;
    let _running = Running(client);
    let _ = report.send(Ok(()));

    let started = Instant::now();
    let mut emitted = 0u64;
    let mut samples: Vec<i16> = Vec::new();
    let silence = vec![0i16; SILENCE_CHUNK_FRAMES * SHARE_AUDIO_CHANNELS as usize];
    while !stop.is_stopped() {
        // SAFETY: waiting on a live event.
        let waited = unsafe { WaitForSingleObject(event.0, POLL.as_millis() as u32) };
        if waited != WAIT_OBJECT_0 && waited != WAIT_TIMEOUT {
            return Err("the audio event failed".into());
        }
        if waited == WAIT_OBJECT_0 {
            emitted += drain(&capture, &mut samples)?;
        }
        let mut missing = silence_frames_to_pad(started.elapsed(), emitted);
        while missing > 0 && !stop.is_stopped() {
            let frames = missing.min(SILENCE_CHUNK_FRAMES as u64);
            push_screen_audio(&silence[..frames as usize * SHARE_AUDIO_CHANNELS as usize]);
            emitted += frames;
            missing -= frames;
        }
    }
    Ok(())
}

/// Pushes every packet the device has; returns the sample frames pushed.
fn drain(capture: &IAudioCaptureClient, samples: &mut Vec<i16>) -> Result<u64, String> {
    let channels = SHARE_AUDIO_CHANNELS as usize;
    let mut pushed = 0u64;
    loop {
        // SAFETY: COM calls on the live capture client; the buffer from
        // `GetBuffer` is read only before the matching `ReleaseBuffer`.
        unsafe {
            let available = capture
                .GetNextPacketSize()
                .map_err(|e| format!("GetNextPacketSize failed: {e}"))?;
            if available == 0 {
                return Ok(pushed);
            }
            let mut data = std::ptr::null_mut();
            let (mut frames, mut flags) = (0u32, 0u32);
            capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .map_err(|e| format!("GetBuffer failed: {e}"))?;

            samples.clear();
            let count = frames as usize * channels;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                samples.resize(count, 0);
            } else {
                // The engine's 16-bit PCM; read it bytewise, as the buffer
                // is not promised to be 2-byte aligned.
                let bytes = std::slice::from_raw_parts(data, count * 2);
                samples.extend(
                    bytes
                        .chunks_exact(2)
                        .map(|pair| i16::from_le_bytes([pair[0], pair[1]])),
                );
            }
            capture
                .ReleaseBuffer(frames)
                .map_err(|e| format!("ReleaseBuffer failed: {e}"))?;
            push_screen_audio(samples);
            pushed += u64::from(frames);
        }
    }
}
