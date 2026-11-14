//! The call's mic through WASAPI, shared mode and event driven, as 48 kHz
//! mono 16-bit (`AUTOCONVERTPCM`: the audio engine converts from the
//! device's own mix format, as yap-capture's loopback does), and the device
//! lists. The speaker is the app's (`yap-speaker`). A device is opened by
//! the endpoint id the ADM lists (`IMMDevice::GetId`), or as the default
//! communications endpoint, which is the one libwebrtc opens by default.
//!
//! The device threads also run at the "Pro Audio" MMCSS priority
//! ([`ProAudio`]), as the ADM's own audio threads do.

use std::marker::PhantomData;
use std::time::Duration;

use windows::core::{w, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Media::Audio::{
    eCapture, eRender, EDataFlow, IAudioCaptureClient, IAudioClient, IMMDevice,
    IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM, AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVE_FORMAT_PCM,
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

use super::pipeline::OpenError;
use crate::audio::{AudioDevice, AudioDevices};
use crate::dsp::RATE;

/// COM for the calling thread, for as long as this lives.
pub(super) struct Com {
    initialised: bool,
    /// Neither `Send` nor `Sync`: it must drop on the thread it was made on.
    _thread: PhantomData<*const ()>,
}

impl Com {
    pub(super) fn init() -> Self {
        // SAFETY: initialises COM on this thread; undone in Drop when it
        // succeeded (S_FALSE included: it was already initialised the same
        // way and must still be balanced).
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
            // SAFETY: balances the successful CoInitializeEx above, on the
            // same thread (Com is neither Send nor Sync).
            unsafe { CoUninitialize() };
        }
    }
}

/// The calling thread registered with MMCSS as "Pro Audio" (the audio
/// engine's own class: scheduled ahead of normal work, so a busy machine
/// does not starve a 10 ms deadline), for as long as this lives.
pub(super) struct ProAudio {
    /// `None`: the registration failed (the thread keeps its priority).
    task: Option<HANDLE>,
    /// Reverted on the thread that registered.
    _thread: PhantomData<*const ()>,
}

impl ProAudio {
    pub(super) fn register(thread: &str) -> Self {
        let mut index = 0u32;
        // SAFETY: a static task name and a live out-parameter.
        let task = match unsafe { AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut index) } {
            Ok(task) => Some(task),
            Err(error) => {
                eprintln!("[voice] {thread}: no Pro Audio priority: {error}");
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
            // SAFETY: the handle the registration returned, reverted once
            // on the same thread (ProAudio is neither Send nor Sync).
            let _ = unsafe { AvRevertMmThreadCharacteristics(task) };
        }
    }
}

/// An event handle, closed on drop.
struct Event(HANDLE);

impl Drop for Event {
    fn drop(&mut self) {
        // SAFETY: a handle we created, closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn pcm(channels: u16) -> WAVEFORMATEX {
    let block_align = channels * 2;
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: channels,
        nSamplesPerSec: RATE,
        nAvgBytesPerSec: RATE * u32::from(block_align),
        nBlockAlign: block_align,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

/// The endpoint `id`, or the default communications endpoint for `flow`.
fn device(flow: EDataFlow, id: Option<&str>) -> Result<IMMDevice, OpenError> {
    // SAFETY: plain COM calls; `wide` outlives `GetDevice`.
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| OpenError::missing(format!("no audio device enumerator: {e}")))?;
        match id {
            Some(id) => {
                let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                enumerator
                    .GetDevice(PCWSTR::from_raw(wide.as_ptr()))
                    .map_err(|e| OpenError::missing(format!("no device {id}: {e}")))
            }
            None => enumerator
                .GetDefaultAudioEndpoint(flow, yap_speaker::default_role())
                .map_err(|e| OpenError::missing(format!("no default device: {e}"))),
        }
    }
}

/// 100 ns units.
const fn hns(d: Duration) -> i64 {
    (d.as_nanos() / 100) as i64
}

/// The client's latency past its buffer, ms: the stream latency the
/// engine reports plus one device period (how late a packet can be picked
/// up or handed over).
fn latency_ms(client: &IAudioClient) -> u32 {
    let mut period = 0i64;
    // SAFETY: plain calls on an initialised client; `period` outlives the
    // call.
    let latency = unsafe {
        let latency = client.GetStreamLatency().unwrap_or(0);
        let _ = client.GetDevicePeriod(Some(&mut period), None);
        latency
    };
    u32::try_from((latency.max(0) + period.max(0)) / 10_000).unwrap_or(0)
}

fn refused(what: &'static str) -> impl Fn(windows::core::Error) -> OpenError {
    move |e| OpenError::refused(format!("{what}: {e}"))
}

/// Opens a shared, event-driven client on the device in `channels` at 48 kHz.
fn open(
    flow: EDataFlow,
    id: Option<&str>,
    channels: u16,
    buffer: Duration,
) -> Result<(IAudioClient, Event), OpenError> {
    let device = device(flow, id)?;
    // SAFETY: COM calls on interfaces we own; the format outlives
    // `Initialize`, the event outlives the client's use of it (the owner
    // drops the client first).
    unsafe {
        let client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(refused("could not open the device"))?;
        let format = pcm(channels);
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

/// Waits for the device's event; `Ok(false)` on a timeout.
fn wait(event: &Event, timeout: Duration) -> Result<bool, String> {
    let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: waiting on a live event.
    match unsafe { WaitForSingleObject(event.0, ms) } {
        WAIT_OBJECT_0 => Ok(true),
        WAIT_TIMEOUT => Ok(false),
        _ => Err("the audio event failed".into()),
    }
}

/// The mic: 48 kHz mono.
pub(super) struct Capture {
    // Field order is drop order: the client stops before the event closes.
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: Event,
}

impl Capture {
    pub(super) fn open(id: Option<&str>) -> Result<Self, OpenError> {
        let (client, event) = open(eCapture, id, 1, Duration::from_millis(100))?;
        // SAFETY: COM calls on the initialised client.
        let capture: IAudioCaptureClient =
            unsafe { client.GetService() }.map_err(refused("no capture client"))?;
        unsafe { client.Start() }.map_err(refused("could not start the mic"))?;
        Ok(Self {
            client,
            capture,
            event,
        })
    }

    /// How long a sample takes from the mic to us, past the packet it
    /// comes in, ms.
    pub(super) fn latency_ms(&self) -> u32 {
        latency_ms(&self.client)
    }

    /// Waits up to `timeout` for the device, then appends every sample it
    /// has to `out`.
    pub(super) fn read(&mut self, out: &mut Vec<i16>, timeout: Duration) -> Result<(), String> {
        wait(&self.event, timeout)?;
        loop {
            // SAFETY: COM calls on the live capture client; the buffer from
            // `GetBuffer` is read only before the matching `ReleaseBuffer`.
            unsafe {
                let available = self
                    .capture
                    .GetNextPacketSize()
                    .map_err(|e| format!("mic read failed: {e}"))?;
                if available == 0 {
                    return Ok(());
                }
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags) = (0u32, 0u32);
                self.capture
                    .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                    .map_err(|e| format!("mic read failed: {e}"))?;
                let count = frames as usize;
                if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null() {
                    out.resize(out.len() + count, 0);
                } else {
                    // Read bytewise: the buffer is not promised to be
                    // 2-byte aligned.
                    let bytes = std::slice::from_raw_parts(data, count * 2);
                    out.extend(
                        bytes
                            .chunks_exact(2)
                            .map(|p| i16::from_le_bytes([p[0], p[1]])),
                    );
                }
                self.capture
                    .ReleaseBuffer(frames)
                    .map_err(|e| format!("mic read failed: {e}"))?;
            }
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: a plain call on the live client.
        let _ = unsafe { self.client.Stop() };
    }
}

/// An endpoint's id and friendly name, as the ADM reports them
/// (`IMMDevice::GetId`, `PKEY_Device_FriendlyName`).
fn describe(device: &IMMDevice) -> Option<AudioDevice> {
    // SAFETY: plain COM calls on a live device. The id string is the
    // callee's CoTaskMem allocation, freed here once copied; the property
    // value is cleared once read, whether or not it held a string.
    unsafe {
        let raw = device.GetId().ok()?;
        let id = raw.to_string().ok();
        CoTaskMemFree(Some(raw.0.cast_const().cast()));
        let store = device.OpenPropertyStore(STGM_READ).ok()?;
        let mut value = store.GetValue(&PKEY_Device_FriendlyName).ok()?;
        let name = PropVariantToStringAlloc(&value).ok().map(|text| {
            let name = text.to_string().unwrap_or_default();
            CoTaskMemFree(Some(text.0.cast_const().cast()));
            name
        });
        let _ = PropVariantClear(&mut value);
        Some(AudioDevice {
            id: id?,
            name: name.filter(|n| !n.is_empty())?,
        })
    }
}

/// The active endpoints of `flow`, in the ADM's order.
fn endpoints(enumerator: &IMMDeviceEnumerator, flow: EDataFlow) -> Option<Vec<AudioDevice>> {
    // SAFETY: plain COM calls; every index is below the collection's count.
    unsafe {
        let all = enumerator
            .EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE)
            .ok()?;
        let count = all.GetCount().ok()?;
        Some(
            (0..count)
                .filter_map(|i| all.Item(i).ok())
                .filter_map(|device| describe(&device))
                .collect(),
        )
    }
}

/// Both device lists and the default communications endpoints, the way
/// the ADM lists them, without creating the WebRTC factory (whose synthetic
/// audio device then wakes every 10 ms for the rest of the process). `None`
/// if the enumerator is missing; the caller then asks the ADM.
pub(super) fn list_devices() -> Option<AudioDevices> {
    let _com = Com::init();
    // SAFETY: COM is initialised on this thread for the enumerator's life.
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()? };
    let default = |flow| {
        // SAFETY: a plain COM call on a live enumerator.
        unsafe { enumerator.GetDefaultAudioEndpoint(flow, yap_speaker::default_role()) }
            .ok()
            .and_then(|device| describe(&device))
    };
    Some(AudioDevices {
        inputs: endpoints(&enumerator, eCapture)?,
        outputs: endpoints(&enumerator, eRender)?,
        default_input: default(eCapture),
        default_output: default(eRender),
    })
}
