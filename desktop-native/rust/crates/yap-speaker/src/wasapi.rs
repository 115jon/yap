//! The speaker through WASAPI: shared mode, event driven, 48 kHz stereo
//! 16-bit with `AUTOCONVERTPCM` (the audio engine converts to the device's
//! mix format), by endpoint id or as the default communications endpoint.
//! Also the endpoint notification client that tells the render thread
//! when to reopen.

use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use windows::core::{implement, w, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, PROPERTYKEY, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    eCommunications, eRender, EDataFlow, ERole, IAudioClient, IAudioRenderClient, IMMDevice,
    IMMDeviceEnumerator, IMMNotificationClient, IMMNotificationClient_Impl, MMDeviceEnumerator,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE,
    DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVE_FORMAT_PCM,
};
use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PropVariantToStringAlloc};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{
    AvRevertMmThreadCharacteristics, AvSetMmThreadCharacteristicsW, CreateEventW,
    WaitForSingleObject,
};

use crate::{RATE, SAMPLES};

/// COM for the calling thread, for as long as this lives.
pub(crate) struct Com {
    initialised: bool,
    _thread: PhantomData<*const ()>,
}

impl Com {
    pub(crate) fn init() -> Self {
        // SAFETY: initialises COM on this thread; balanced in Drop when it
        // succeeded (S_FALSE included).
        let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        Self {
            initialised: hr.is_ok(),
            _thread: PhantomData,
        }
    }
}

impl Drop for Com {
    fn drop(&mut self) {
        if self.initialised {
            // SAFETY: balances the CoInitializeEx above, on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// The calling thread at MMCSS "Pro Audio" priority while this lives.
pub(crate) struct ProAudio {
    task: Option<HANDLE>,
    _thread: PhantomData<*const ()>,
}

impl ProAudio {
    pub(crate) fn register() -> Self {
        let mut index = 0u32;
        // SAFETY: a static task name and a live out-parameter.
        let task = match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) } {
            Ok(task) => Some(task),
            Err(error) => {
                eprintln!("[speaker] no Pro Audio priority: {error}");
                None
            }
        };
        Self {
            task,
            _thread: PhantomData,
        }
    }
}

impl Drop for ProAudio {
    fn drop(&mut self) {
        if let Some(task) = self.task {
            // SAFETY: reverted once, on the registering thread.
            let _ = unsafe { AvRevertMmThreadCharacteristics(task) };
        }
    }
}

struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: a handle we created, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

/// Why the device did not open.
#[derive(Debug)]
pub(crate) struct OpenError {
    pub(crate) message: String,
    /// The endpoint is there but refused our stream (48 kHz 16-bit with
    /// `AUTOCONVERTPCM`); `false`: no device at all.
    pub(crate) refused: bool,
}

impl OpenError {
    fn missing(message: String) -> Self {
        Self {
            message,
            refused: false,
        }
    }
}

fn refused(what: &'static str) -> impl Fn(windows::core::Error) -> OpenError {
    move |e| OpenError {
        message: format!("{what}: {e}"),
        refused: true,
    }
}

pub(crate) fn enumerator() -> Result<IMMDeviceEnumerator, OpenError> {
    // SAFETY: a plain COM call on a thread with COM initialised.
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
        .map_err(|e| OpenError::missing(format!("no audio device enumerator: {e}")))
}

fn id_of(device: &IMMDevice) -> Option<String> {
    // SAFETY: the id is the callee's CoTaskMem string, freed once copied.
    unsafe {
        let raw = device.GetId().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0.cast_const().cast()));
        id
    }
}

fn name_of(device: &IMMDevice) -> String {
    // SAFETY: plain COM calls; the value is cleared once read and the
    // string it gave is freed once copied.
    unsafe {
        let Ok(store) = device.OpenPropertyStore(STGM_READ) else {
            return String::new();
        };
        let Ok(mut value) = store.GetValue(&PKEY_Device_FriendlyName) else {
            return String::new();
        };
        let name = PropVariantToStringAlloc(&value).ok().map(|text| {
            let name = text.to_string().unwrap_or_default();
            CoTaskMemFree(Some(text.0.cast_const().cast()));
            name
        });
        let _ = PropVariantClear(&mut value);
        name.unwrap_or_default()
    }
}

fn is_active(device: &IMMDevice) -> bool {
    // SAFETY: a plain COM call on a live device.
    unsafe { device.GetState() }.is_ok_and(|state| state == DEVICE_STATE_ACTIVE)
}

/// The endpoint for `choice` (when it is plugged in), else the default
/// communications endpoint; `true` with the default.
fn pick(
    enumerator: &IMMDeviceEnumerator,
    choice: Option<&str>,
) -> Result<(IMMDevice, bool), OpenError> {
    if let Some(id) = choice {
        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `wide` outlives the call.
        match unsafe { enumerator.GetDevice(PCWSTR::from_raw(wide.as_ptr())) } {
            Ok(device) if is_active(&device) => return Ok((device, false)),
            Ok(_) => eprintln!("[speaker] {id} is not plugged in; using the default"),
            Err(e) => eprintln!("[speaker] no device {id} ({e}); using the default"),
        }
    }
    // SAFETY: a plain COM call on a live enumerator.
    unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eCommunications) }
        .map(|device| (device, true))
        .map_err(|e| OpenError::missing(format!("no default output device: {e}")))
}

const fn hns(d: Duration) -> i64 {
    (d.as_nanos() / 100) as i64
}

fn pcm() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 2,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * 4,
        nBlockAlign: 4,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

/// An initialised (not started) shared, event-driven client.
fn client(device: &IMMDevice, buffer: Duration) -> Result<(IAudioClient, Event), OpenError> {
    // SAFETY: COM calls on interfaces we own; the format outlives
    // `Initialize`; the event outlives the client (the owner drops the
    // client first).
    unsafe {
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(refused("could not open the device"))?;
        let format = pcm();
        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_EVENTCALLBACK
                    | AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM
                    | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                hns(buffer),
                0,
                &format,
                None,
            )
            .map_err(refused("could not initialise the device"))?;
        let event = Event(
            CreateEventW(None, false, false, None)
                .map_err(refused("could not create the audio event"))?,
        );
        client
            .SetEventHandle(event.0)
            .map_err(refused("could not set the audio event"))?;
        Ok((client, event))
    }
}

/// The client's stream latency plus one device period, in 100 ns units,
/// and the period alone.
fn latency(client: &IAudioClient) -> (i64, i64) {
    let mut period = 0i64;
    // SAFETY: plain calls on an initialised client.
    let latency = unsafe {
        let latency = client.GetStreamLatency().unwrap_or(0);
        let _ = client.GetDevicePeriod(Some(&mut period), None);
        latency
    };
    (latency.max(0) + period.max(0), period.max(0))
}

/// The open speaker.
pub(crate) struct Render {
    // Field order is drop order: the client stops before the event closes.
    client: IAudioClient,
    render: IAudioRenderClient,
    event: Event,
    buffer_frames: u32,
    latency_ms: u32,
    pub(crate) id: String,
    pub(crate) name: String,
    /// Opened as the default (followed on a change of it).
    pub(crate) on_default: bool,
}

impl Render {
    /// Opens `choice` (or the default) with a `buffer` long device buffer
    /// and starts it with one frame of silence.
    pub(crate) fn open(
        enumerator: &IMMDeviceEnumerator,
        choice: Option<&str>,
        buffer: Duration,
    ) -> Result<Self, OpenError> {
        let (device, on_default) = pick(enumerator, choice)?;
        let (client, event) = client(&device, buffer)?;
        // SAFETY: COM calls on the initialised client.
        unsafe {
            let buffer_frames = client
                .GetBufferSize()
                .map_err(refused("no speaker buffer size"))?;
            let render: IAudioRenderClient =
                client.GetService().map_err(refused("no render client"))?;
            let (latency, _) = latency(&client);
            let mut this = Self {
                client,
                render,
                event,
                buffer_frames,
                latency_ms: u32::try_from(latency / 10_000).unwrap_or(0),
                id: id_of(&device).unwrap_or_default(),
                name: name_of(&device),
                on_default,
            };
            this.write(&[0; SAMPLES]).map_err(|message| OpenError {
                message,
                refused: true,
            })?;
            this.client
                .Start()
                .map_err(refused("could not start the speaker"))?;
            Ok(this)
        }
    }

    /// Waits up to `timeout` for the device to want more; `Ok(false)` on a
    /// timeout.
    pub(crate) fn wait(&self, timeout: Duration) -> Result<bool, String> {
        let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
        // SAFETY: waiting on a live event.
        match unsafe { WaitForSingleObject(self.event.0, ms) } {
            WAIT_OBJECT_0 => Ok(true),
            WAIT_TIMEOUT => Ok(false),
            _ => Err("the audio event failed".into()),
        }
    }

    /// From the device buffer to the ear, ms: stream latency and a period.
    pub(crate) fn latency_ms(&self) -> u32 {
        self.latency_ms
    }

    pub(crate) fn capacity(&self) -> u32 {
        self.buffer_frames
    }

    /// Frames queued on the device, not played yet. An error here is
    /// usually `AUDCLNT_E_DEVICE_INVALIDATED` (unplugged, format change).
    pub(crate) fn queued(&self) -> Result<u32, String> {
        // SAFETY: a plain call on the live client.
        unsafe { self.client.GetCurrentPadding() }.map_err(|e| format!("speaker failed: {e}"))
    }

    /// Queues interleaved stereo (as many frames as fit).
    pub(crate) fn write(&mut self, samples: &[i16]) -> Result<(), String> {
        let room = self.buffer_frames.saturating_sub(self.queued()?);
        let frames = u32::try_from(samples.len() / 2)
            .unwrap_or(u32::MAX)
            .min(room);
        if frames == 0 {
            return Ok(());
        }
        // SAFETY: `GetBuffer` hands out room for `frames` stereo 16-bit
        // frames, written bytewise (no alignment promise) before the
        // matching `ReleaseBuffer`.
        unsafe {
            let data = self
                .render
                .GetBuffer(frames)
                .map_err(|e| format!("speaker write failed: {e}"))?;
            let bytes = std::slice::from_raw_parts_mut(data, frames as usize * 4);
            for (pair, s) in bytes.chunks_exact_mut(2).zip(samples) {
                pair.copy_from_slice(&s.to_le_bytes());
            }
            self.render
                .ReleaseBuffer(frames, 0)
                .map_err(|e| format!("speaker write failed: {e}"))
        }
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        // SAFETY: a plain call on the live client.
        let _ = unsafe { self.client.Stop() };
    }
}

/// What the notification client watches, shared with the render thread.
#[derive(Default)]
pub(crate) struct Watch {
    /// The render thread should reopen.
    pub(crate) reopen: AtomicBool,
    /// The endpoint open now, and whether it is the default.
    pub(crate) open: Mutex<(String, bool)>,
    /// The chosen endpoint (`None`: the default).
    pub(crate) choice: Mutex<Option<String>>,
}

impl Watch {
    fn reopen(&self, why: &str) {
        if !self.reopen.swap(true, Ordering::Relaxed) {
            eprintln!("[speaker] {why}: reopening");
        }
    }

    fn open(&self) -> (String, bool) {
        self.open
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn choice(&self) -> Option<String> {
        self.choice
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The decisions, apart from COM so they can be tested.
    pub(crate) fn default_changed(&self, flow: EDataFlow, role: ERole, id: &str) {
        let (open, on_default) = self.open();
        if flow == eRender && role == eCommunications && on_default && id != open {
            self.reopen("the default output changed");
        }
    }

    pub(crate) fn state_changed(&self, id: &str, active: bool) {
        let (open, on_default) = self.open();
        if id == open && !active {
            self.reopen("the output device went away");
        } else if active && on_default && self.choice().as_deref() == Some(id) {
            self.reopen("the chosen output device is back");
        }
    }
}

#[implement(IMMNotificationClient)]
struct Notifier(Arc<Watch>);

fn text(id: &PCWSTR) -> String {
    if id.is_null() {
        return String::new();
    }
    // SAFETY: the system passes a valid, terminated string for the call.
    unsafe { id.to_string() }.unwrap_or_default()
}

// Called on a system thread: only flags are set here.
impl IMMNotificationClient_Impl for Notifier_Impl {
    fn OnDeviceStateChanged(&self, id: &PCWSTR, state: DEVICE_STATE) -> windows::core::Result<()> {
        self.0.state_changed(&text(id), state == DEVICE_STATE_ACTIVE);
        Ok(())
    }

    fn OnDeviceAdded(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.0.state_changed(&text(id), true);
        Ok(())
    }

    fn OnDeviceRemoved(&self, id: &PCWSTR) -> windows::core::Result<()> {
        self.0.state_changed(&text(id), false);
        Ok(())
    }

    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        id: &PCWSTR,
    ) -> windows::core::Result<()> {
        self.0.default_changed(flow, role, &text(id));
        Ok(())
    }

    fn OnPropertyValueChanged(&self, _id: &PCWSTR, _key: &PROPERTYKEY) -> windows::core::Result<()> {
        Ok(())
    }
}

/// The notification client, registered while this lives.
pub(crate) struct Registration {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

impl Registration {
    pub(crate) fn new(enumerator: &IMMDeviceEnumerator, watch: Arc<Watch>) -> Option<Self> {
        let client: IMMNotificationClient = Notifier(watch).into();
        // SAFETY: a plain COM call; the client stays alive until it is
        // unregistered in Drop.
        match unsafe { enumerator.RegisterEndpointNotificationCallback(&client) } {
            Ok(()) => Some(Self {
                enumerator: enumerator.clone(),
                client,
            }),
            Err(error) => {
                eprintln!("[speaker] no device notifications: {error}");
                None
            }
        }
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        // SAFETY: the client registered in `new`, unregistered once.
        let _ = unsafe {
            self.enumerator
                .UnregisterEndpointNotificationCallback(&self.client)
        };
    }
}

/// What a client on the default output would get, measured without
/// starting it (nothing plays).
#[derive(Debug, Clone)]
pub struct Probe {
    pub name: String,
    /// The device buffer WASAPI gave for the request, ms.
    pub buffer_ms: f64,
    /// `GetDevicePeriod` (default), ms.
    pub period_ms: f64,
    /// `GetStreamLatency` plus a period, ms.
    pub latency_ms: f64,
}

/// Opens the default communications output as the speaker would, with a
/// `buffer` request, reads its sizes and latency, and closes it unstarted.
pub fn probe_default(buffer: Duration) -> Result<Probe, String> {
    let _com = Com::init();
    let enumerator = enumerator().map_err(|e| e.message)?;
    let (device, _) = pick(&enumerator, None).map_err(|e| e.message)?;
    let (client, _event) = client(&device, buffer).map_err(|e| e.message)?;
    // SAFETY: a plain call on the initialised client.
    let frames = unsafe { client.GetBufferSize() }.map_err(|e| e.to_string())?;
    let (latency, period) = latency(&client);
    Ok(Probe {
        name: name_of(&device),
        buffer_ms: f64::from(frames) * 1000.0 / f64::from(RATE),
        period_ms: period as f64 / 10_000.0,
        latency_ms: latency as f64 / 10_000.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Media::Audio::{eCapture, eConsole};

    fn watch(open: &str, on_default: bool, choice: Option<&str>) -> Watch {
        let w = Watch::default();
        *w.open.lock().unwrap() = (open.into(), on_default);
        *w.choice.lock().unwrap() = choice.map(str::to_owned);
        w
    }

    #[test]
    fn a_new_default_should_reopen_only_when_following_it() {
        let w = watch("a", true, None);
        w.default_changed(eRender, eConsole, "b");
        w.default_changed(eCapture, eCommunications, "b");
        assert!(!w.reopen.load(Ordering::Relaxed), "other role or flow");
        w.default_changed(eRender, eCommunications, "a");
        assert!(!w.reopen.load(Ordering::Relaxed), "already on it");
        w.default_changed(eRender, eCommunications, "b");
        assert!(w.reopen.load(Ordering::Relaxed));
        let chosen = watch("c", false, Some("c"));
        chosen.default_changed(eRender, eCommunications, "b");
        assert!(!chosen.reopen.load(Ordering::Relaxed), "a chosen device stays");
    }

    #[test]
    fn losing_the_open_device_should_reopen() {
        let w = watch("a", false, Some("a"));
        w.state_changed("z", false);
        assert!(!w.reopen.load(Ordering::Relaxed));
        w.state_changed("a", false);
        assert!(w.reopen.load(Ordering::Relaxed));
    }

    #[test]
    fn the_chosen_device_coming_back_should_reopen_on_it() {
        let w = watch("default", true, Some("c"));
        w.state_changed("other", true);
        assert!(!w.reopen.load(Ordering::Relaxed));
        w.state_changed("c", true);
        assert!(w.reopen.load(Ordering::Relaxed));
    }
}
