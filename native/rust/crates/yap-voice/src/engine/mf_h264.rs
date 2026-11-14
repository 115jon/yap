//! Hardware H.264 for our screen share: a Media Foundation encoder MFT
//! (NVENC, AMF or Quick Sync, as the desktop app's `wmf_encoder.rs` picks
//! them) on a thread of its own, its access units handed to libwebrtc's
//! pre-encoded pass-through (`NativeVideoSource::capture_encoded_frame`,
//! the sender on `VideoEncoderBackend::PreEncoded`).
//!
//! - Frames: the share's I420 frames (already scaled to the preset's box)
//!   are queued newest-wins and converted to NV12 straight into the MFT's
//!   input buffer, in system memory (every vendor's MFT takes that).
//! - Events: hardware MFTs are asynchronous; `BeginGetEvent` delivers
//!   `METransformNeedInput` / `METransformHaveOutput` to the same channel
//!   the frames wake, so the thread blocks in one place.
//! - Keyframes: the first frame of a stream is an IDR, and each request the
//!   pass-through raises (a viewer's PLI or FIR, a reconfiguration) forces
//!   one (`CODECAPI_AVEncVideoForceKeyFrame`), at most one per
//!   [`KEYFRAME_GAP`] (a request inside it waits for the next frame after
//!   it); a one-second GOP bounds a late joiner's wait, as the desktop
//!   app's encoder does.
//! - Rate: the pass-through reports WebRTC's bandwidth estimate; the
//!   encoder's mean bitrate follows it, capped at the preset
//!   ([`crate::h264::next_bitrate`]).
//! - A new frame size (another preset, a resized window, the parked share's
//!   black frames) starts a fresh MFT at that size. Frames smaller than
//!   320x180 are padded to it with black (some MFTs will not open below).
//! - Failure: the encoder stops taking frames and says so
//!   ([`HwEncoder::failed`]); the call pushes the share again on VP8. A
//!   stream that had produced output failing keeps later shares on VP8
//!   too; one that never got going costs only this share.
//!
//! The output is Constrained Baseline (`profile-level-id=42e01f`, mode 1),
//! which every browser, the mobile app and this client decode.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::{Duration, Instant};

use libwebrtc::video_frame::{
    EncodedFrameType, EncodedVideoCodec, EncodedVideoFrame, I420Buffer, VideoBuffer,
};
use libwebrtc::video_source::native::NativeVideoSource;
use libwebrtc::video_source::VideoResolution;
use windows::core::{implement, Interface, Ref, GUID, PWSTR};
use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::Media::MediaFoundation::{
    eAVEncCommonRateControlMode_CBR, eAVEncCommonRateControlMode_LowDelayVBR,
    eAVEncH264VProfile_Base, CODECAPI_AVEncCommonMeanBitRate, CODECAPI_AVEncCommonRateControlMode,
    CODECAPI_AVEncMPVDefaultBPictureCount, CODECAPI_AVEncMPVGOPSize,
    CODECAPI_AVEncVideoForceKeyFrame, CODECAPI_AVLowLatencyMode, ICodecAPI, IMFActivate,
    IMFAsyncCallback, IMFAsyncCallback_Impl, IMFAsyncResult, IMFMediaEventGenerator, IMFMediaType,
    IMFSample, IMFShutdown, IMFTransform, METransformHaveOutput, METransformNeedInput,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFSampleExtension_CleanPoint, MFShutdown, MFStartup, MFTEnumEx,
    MFT_ENUM_HARDWARE_VENDOR_ID_Attribute, MFT_FRIENDLY_NAME_Attribute, MFVideoFormat_H264,
    MFVideoFormat_NV12, MFSTARTUP_NOSOCKET, MFT_CATEGORY_VIDEO_ENCODER, MFT_ENUM_FLAG,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_MESSAGE_COMMAND_FLUSH,
    MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, MFT_MESSAGE_NOTIFY_END_OF_STREAM,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_OUTPUT_DATA_BUFFER, MFT_OUTPUT_DATA_BUFFER_INCOMPLETE,
    MFT_OUTPUT_STREAM_PROVIDES_SAMPLES, MFT_REGISTER_TYPE_INFO, MF_E_NOTACCEPTING,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE, MF_MT_AVG_BITRATE,
    MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MPEG2_PROFILE, MF_MT_MPEG_SEQUENCE_HEADER, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE,
    MF_TRANSFORM_ASYNC_UNLOCK, MF_VERSION,
};
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_MULTITHREADED,
};
use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_UI4,
};

use crate::h264::{self, Vendor};

extern "C" {
    fn yap_i420_to_nv12(
        src_y: *const u8,
        stride_y: i32,
        src_u: *const u8,
        stride_u: i32,
        src_v: *const u8,
        stride_v: i32,
        dst_y: *mut u8,
        dst_stride_y: i32,
        dst_uv: *mut u8,
        dst_stride_uv: i32,
        width: i32,
        height: i32,
    ) -> i32;
}

/// What the share asks of its encoder.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HwConfig {
    pub max_bitrate: u64,
    pub fps: u32,
    /// `detail` (monitors): keep resolution, give up frame rate.
    pub detail: bool,
}

/// How the encoder is doing, for the stats line and the details panel.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct HwStats {
    /// The MFT's friendly name.
    pub name: String,
    pub frames_in: u64,
    pub frames_out: u64,
    pub keyframes: u64,
    /// Frames replaced in the queue before the encoder took them.
    pub replaced: u64,
    /// The bitrate the encoder runs at now.
    pub bitrate: u64,
    /// Mean time from `ProcessInput` to the access unit, in microseconds.
    pub latency_us: u64,
    pub width: u32,
    pub height: u32,
}

/// One message to the encoder thread.
enum Msg {
    /// A frame is waiting, or the config changed.
    Wake,
    /// An MFT event of stream `generation`.
    Event {
        generation: u64,
        kind: u32,
    },
    /// `EndGetEvent` failed for stream `generation` (the MFT shut down).
    Failed {
        generation: u64,
    },
    Stop,
}

#[derive(Default)]
struct Counters {
    frames_in: AtomicU64,
    frames_out: AtomicU64,
    keyframes: AtomicU64,
    replaced: AtomicU64,
    bitrate: AtomicU64,
    latency_total_us: AtomicU64,
    size: AtomicU64,
}

struct Shared {
    name: String,
    index: usize,
    pending: Mutex<Option<Arc<I420Buffer>>>,
    config: Mutex<HwConfig>,
    counters: Counters,
    failed: AtomicBool,
}

/// A running hardware encoder. Dropping it stops the thread without
/// waiting for it.
pub(crate) struct HwEncoder {
    shared: Arc<Shared>,
    tx: mpsc::Sender<Msg>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One enumerated hardware H.264 encoder.
struct Candidate {
    name: String,
    vendor: Vendor,
}

/// Starts Media Foundation for the scope (it is reference counted).
struct MfScope;

impl MfScope {
    fn start() -> Result<Self, String> {
        // SAFETY: plain start-up call; paired with `MFShutdown` in `drop`.
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }
            .map(|()| Self)
            .map_err(|e| format!("MFStartup: {e}"))
    }
}

impl Drop for MfScope {
    fn drop(&mut self) {
        // SAFETY: balances the `MFStartup` above.
        let _ = unsafe { MFShutdown() };
    }
}

/// COM on this thread (multithreaded), for the scope.
struct ComScope(bool);

impl ComScope {
    fn start() -> Self {
        // SAFETY: initialises COM for the calling thread only.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for ComScope {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances a successful `CoInitializeEx` on this thread.
            unsafe { CoUninitialize() };
        }
    }
}

/// An `IMFActivate` string attribute.
///
/// # Safety
///
/// `activate` must be live.
unsafe fn activate_string(activate: &IMFActivate, key: &GUID) -> Option<String> {
    let mut raw = PWSTR::null();
    let mut len = 0;
    // SAFETY: the out pointers are live locals; the string is freed below.
    unsafe { activate.GetAllocatedString(key, &mut raw, &mut len) }.ok()?;
    if raw.is_null() {
        return None;
    }
    // SAFETY: `raw` is the NUL-terminated string Media Foundation allocated.
    let value = unsafe { raw.to_string() }.ok();
    // SAFETY: allocated with CoTaskMemAlloc by `GetAllocatedString`.
    unsafe { CoTaskMemFree(Some(raw.0 as _)) };
    value
}

/// The hardware encoder MFTs that take NV12 and give H.264, in Media
/// Foundation's merit order.
///
/// # Safety
///
/// Media Foundation must be started on this process.
unsafe fn enumerate() -> Result<Vec<IMFActivate>, String> {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_NV12,
    };
    let output = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: MFVideoFormat_H264,
    };
    let mut activates = std::ptr::null_mut();
    let mut count = 0u32;
    // SAFETY: the type infos and out pointers are live locals.
    unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0),
            Some(&input),
            Some(&output),
            &mut activates,
            &mut count,
        )
    }
    .map_err(|e| format!("MFTEnumEx: {e}"))?;
    let mut list = Vec::with_capacity(count as usize);
    if count > 0 && !activates.is_null() {
        // SAFETY: MFTEnumEx returned `count` activates at `activates`; each
        // is taken (owned) once, and the array is freed below.
        for item in unsafe { std::slice::from_raw_parts_mut(activates, count as usize) } {
            if let Some(activate) = item.take() {
                list.push(activate);
            }
        }
    }
    // SAFETY: the array was allocated with CoTaskMemAlloc by MFTEnumEx.
    unsafe { CoTaskMemFree(Some(activates as _)) };
    Ok(list)
}

/// The listed encoders, named and classified.
///
/// # Safety
///
/// As [`enumerate`]; each activate must be live.
unsafe fn candidates(activates: &[IMFActivate]) -> Vec<Candidate> {
    activates
        .iter()
        .map(|activate| {
            // SAFETY: the activate is live (the caller's contract).
            let (name, vendor_id) = unsafe {
                (
                    activate_string(activate, &MFT_FRIENDLY_NAME_Attribute).unwrap_or_default(),
                    activate_string(activate, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute),
                )
            };
            Candidate {
                vendor: Vendor::classify(vendor_id.as_deref(), &name),
                name,
            }
        })
        .collect()
}

/// A stream that had produced output failed: the hardware is not to be
/// trusted, so later shares use VP8 for the rest of the process.
static BROKEN: AtomicBool = AtomicBool::new(false);

/// The encoder failed: the share falls back to VP8. `produced`: the stream
/// that failed had produced output, so later shares do too. A stream that
/// never got going (it would not open at this size, or refused its first
/// input) costs this share only.
fn fail(shared: &Shared, produced: bool) {
    shared.failed.store(true, Ordering::Relaxed);
    if produced {
        BROKEN.store(true, Ordering::Relaxed);
    }
}

/// `YAP_TEST_MFT_FAIL_AFTER_SECS=<n>`: a test hook. `n` s after a share's
/// encoder first produces output, it fails as a dead MFT would (once per
/// process). Unset: off.
fn test_fail_after() -> Option<Duration> {
    std::env::var("YAP_TEST_MFT_FAIL_AFTER_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|secs| secs.is_finite() && *secs >= 0.0)
        .map(Duration::from_secs_f64)
}

/// The test hook has failed an encoder already.
static TEST_FAILED: AtomicBool = AtomicBool::new(false);

/// `YAP_SHARE_ENCODER=software`: shares use libwebrtc's VP8, as on a machine
/// without a hardware encoder (for comparisons and fallback tests).
fn forced_software() -> bool {
    std::env::var("YAP_SHARE_ENCODER").is_ok_and(|v| v.eq_ignore_ascii_case("software"))
}

/// The hardware H.264 encoder shares use, checked once per process: listed,
/// activated and configured at 1280x720 (`None` when there is none, it
/// refuses, or `YAP_SHARE_ENCODER=software`). Blocks for the probe on the
/// first call (about 100 ms).
pub(crate) fn probe() -> Option<(&'static str, usize)> {
    if BROKEN.load(Ordering::Relaxed) {
        return None;
    }
    static PROBE: OnceLock<Option<(String, usize)>> = OnceLock::new();
    PROBE
        .get_or_init(|| {
            if forced_software() {
                eprintln!("[share] hardware encoder off (YAP_SHARE_ENCODER=software)");
                return None;
            }
            let found = std::thread::Builder::new()
                .name("yap-h264-probe".into())
                .spawn(probe_now)
                .map_err(|e| e.to_string())
                .and_then(|t| t.join().map_err(|_| "probe panicked".to_owned()))
                .and_then(|r| r);
            match &found {
                Ok((name, _)) => eprintln!("[share] hardware H.264 encoder: {name}"),
                Err(why) => eprintln!("[share] no hardware H.264 encoder ({why}); shares use VP8"),
            }
            found.ok()
        })
        .as_ref()
        .map(|(name, index)| (name.as_str(), *index))
}

fn probe_now() -> Result<(String, usize), String> {
    let _com = ComScope::start();
    let _mf = MfScope::start()?;
    // SAFETY: Media Foundation is started for this scope.
    let activates = unsafe { enumerate() }?;
    // SAFETY: as above; the activates are live.
    let list = unsafe { candidates(&activates) };
    for c in &list {
        eprintln!(
            "[share] hardware encoder listed: {} ({:?})",
            c.name, c.vendor
        );
    }
    let vendors: Vec<Vendor> = list.iter().map(|c| c.vendor).collect();
    let index = h264::pick_encoder(&vendors).ok_or("none listed")?;
    let config = HwConfig {
        max_bitrate: 5_000_000,
        fps: 30,
        detail: true,
    };
    // SAFETY: as above.
    let stream = unsafe { Stream::open(&activates[index], (1280, 720), &config, 0, None) }?;
    stream.shutdown();
    Ok((list[index].name.clone(), index))
}

impl HwEncoder {
    /// Starts an encoder feeding `source`, on the probed MFT. Fails when
    /// there is none.
    pub(crate) fn start(source: NativeVideoSource, config: HwConfig) -> Result<Self, String> {
        let (name, index) = probe().ok_or("no hardware H.264 encoder")?;
        let shared = Arc::new(Shared {
            name: name.to_owned(),
            index,
            pending: Mutex::new(None),
            config: Mutex::new(config),
            counters: Counters::default(),
            failed: AtomicBool::new(false),
        });
        let (tx, rx) = mpsc::channel();
        let thread_shared = Arc::clone(&shared);
        let events = tx.clone();
        // Detached: see the `Drop` below.
        std::thread::Builder::new()
            .name("yap-h264".into())
            .spawn(move || run(&thread_shared, &source, &rx, &events))
            .map_err(|e| format!("encoder thread: {e}"))?;
        Ok(Self { shared, tx })
    }

    /// Queues a frame (newest wins). `false` once the encoder failed.
    pub(crate) fn submit(&self, frame: Arc<I420Buffer>) -> bool {
        if self.shared.failed.load(Ordering::Relaxed) {
            return false;
        }
        if lock(&self.shared.pending).replace(frame).is_some() {
            self.shared
                .counters
                .replaced
                .fetch_add(1, Ordering::Relaxed);
        }
        self.shared
            .counters
            .frames_in
            .fetch_add(1, Ordering::Relaxed);
        self.tx.send(Msg::Wake).is_ok()
    }

    /// A new preset or content hint.
    pub(crate) fn set_config(&self, config: HwConfig) {
        *lock(&self.shared.config) = config;
        let _ = self.tx.send(Msg::Wake);
    }

    pub(crate) fn failed(&self) -> bool {
        self.shared.failed.load(Ordering::Relaxed)
    }

    pub(crate) fn stats(&self) -> HwStats {
        let c = &self.shared.counters;
        let out = c.frames_out.load(Ordering::Relaxed);
        let size = c.size.load(Ordering::Relaxed);
        HwStats {
            name: self.shared.name.clone(),
            frames_in: c.frames_in.load(Ordering::Relaxed),
            frames_out: out,
            keyframes: c.keyframes.load(Ordering::Relaxed),
            replaced: c.replaced.load(Ordering::Relaxed),
            bitrate: c.bitrate.load(Ordering::Relaxed),
            latency_us: c.latency_total_us.load(Ordering::Relaxed) / out.max(1),
            width: (size >> 32) as u32,
            height: size as u32,
        }
    }
}

impl Drop for HwEncoder {
    fn drop(&mut self) {
        // Not joined: the share's send side drops on an async worker, and
        // an MFT's shutdown can take a while. The thread shuts its MFT down
        // and ends on its own once it reads this.
        let _ = self.tx.send(Msg::Stop);
    }
}

/// Forwards each MFT event to the encoder thread. The generator travels as
/// the call's state, so the callback holds no reference back to the MFT.
#[implement(IMFAsyncCallback)]
struct EventCallback {
    tx: mpsc::Sender<Msg>,
    generation: u64,
}

impl IMFAsyncCallback_Impl for EventCallback_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> windows::core::Result<()> {
        // E_NOTIMPL: the default work queue.
        Err(windows::Win32::Foundation::E_NOTIMPL.into())
    }

    fn Invoke(&self, result: Ref<'_, IMFAsyncResult>) -> windows::core::Result<()> {
        let result = result.ok()?;
        // SAFETY: the state is the generator `BeginGetEvent` was called on.
        let kind = unsafe {
            result
                .GetState()
                .and_then(|state| state.cast::<IMFMediaEventGenerator>())
                .and_then(|generator| generator.EndGetEvent(result))
                .and_then(|event| event.GetType())
        };
        let msg = match kind {
            Ok(kind) => Msg::Event {
                generation: self.generation,
                kind,
            },
            Err(_) => Msg::Failed {
                generation: self.generation,
            },
        };
        let _ = self.tx.send(msg);
        Ok(())
    }
}

fn variant_u32(value: u32) -> VARIANT {
    let mut v = VARIANT::default();
    v.Anonymous = VARIANT_0 {
        Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
            vt: VT_UI4,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: VARIANT_0_0_0 { ulVal: value },
        }),
    };
    v
}

fn variant_bool(value: bool) -> VARIANT {
    let mut v = VARIANT::default();
    v.Anonymous = VARIANT_0 {
        Anonymous: std::mem::ManuallyDrop::new(VARIANT_0_0 {
            vt: VT_BOOL,
            wReserved1: 0,
            wReserved2: 0,
            wReserved3: 0,
            Anonymous: VARIANT_0_0_0 {
                boolVal: if value { VARIANT_TRUE } else { VARIANT_FALSE },
            },
        }),
    };
    v
}

fn bps(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// One MFT at one frame size.
struct Stream {
    mft: IMFTransform,
    events: IMFMediaEventGenerator,
    callback: IMFAsyncCallback,
    codec: Option<ICodecAPI>,
    generation: u64,
    size: (u32, u32),
    provides_samples: bool,
    /// The SPS and PPS from the output type, for keyframes without them.
    sequence_header: Vec<u8>,
    need_input: u32,
    bitrate: u64,
    fps: u32,
    epoch: Instant,
    /// When each frame in flight went in, by its sample time.
    in_flight: h264::InFlight<Instant>,
    frame_index: u64,
    /// It handed at least one access unit on.
    produced: bool,
}

impl Stream {
    /// Activates `activate` and configures it for `size`.
    ///
    /// # Safety
    ///
    /// Media Foundation and COM must be up on this thread; `activate` live.
    unsafe fn open(
        activate: &IMFActivate,
        size: (u32, u32),
        config: &HwConfig,
        generation: u64,
        events_tx: Option<&mpsc::Sender<Msg>>,
    ) -> Result<Self, String> {
        // SAFETY (whole body): COM calls on live interfaces made here, with
        // live arguments; the caller's contract covers the runtime.
        unsafe {
            let mft: IMFTransform = activate
                .ActivateObject()
                .map_err(|e| format!("ActivateObject: {e}"))?;
            if let Ok(attributes) = mft.GetAttributes() {
                attributes
                    .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
                    .map_err(|e| format!("async unlock: {e}"))?;
            }
            let bitrate = config.max_bitrate;
            let fps = config.fps.max(1);
            let rate = (u64::from(fps) << 32) | 1;
            let frame = (u64::from(size.0) << 32) | u64::from(size.1);
            let set_types = || -> windows::core::Result<()> {
                let out: IMFMediaType = MFCreateMediaType()?;
                out.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                out.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
                out.SetUINT32(&MF_MT_AVG_BITRATE, bps(bitrate))?;
                out.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
                out.SetUINT64(&MF_MT_FRAME_SIZE, frame)?;
                out.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
                out.SetUINT32(&MF_MT_MPEG2_PROFILE, eAVEncH264VProfile_Base.0 as u32)?;
                mft.SetOutputType(0, &out, 0)?;
                let input: IMFMediaType = MFCreateMediaType()?;
                input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
                input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
                input.SetUINT64(&MF_MT_FRAME_RATE, rate)?;
                input.SetUINT64(&MF_MT_FRAME_SIZE, frame)?;
                input.SetUINT32(&MF_MT_INTERLACE_MODE, 2)?;
                input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, (1u64 << 32) | 1)?;
                mft.SetInputType(0, &input, 0)
            };
            if let Err(e) = set_types() {
                shutdown_mft(&mft);
                return Err(format!("media types at {}x{}: {e}", size.0, size.1));
            }
            let codec = mft.cast::<ICodecAPI>().ok();
            if let Some(codec) = &codec {
                let set = |api: &GUID, value: VARIANT| codec.SetValue(api, &value).is_ok();
                let low_latency = set(&CODECAPI_AVLowLatencyMode, variant_bool(true));
                let rc = set(
                    &CODECAPI_AVEncCommonRateControlMode,
                    variant_u32(eAVEncCommonRateControlMode_LowDelayVBR.0 as u32),
                ) || set(
                    &CODECAPI_AVEncCommonRateControlMode,
                    variant_u32(eAVEncCommonRateControlMode_CBR.0 as u32),
                );
                let gop = set(&CODECAPI_AVEncMPVGOPSize, variant_u32(fps));
                let no_b = set(&CODECAPI_AVEncMPVDefaultBPictureCount, variant_u32(0));
                let mean = set(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(bps(bitrate)));
                if generation > 0 {
                    eprintln!(
                        "[share] h264 {}x{}@{fps} low-latency {low_latency} rate-control {rc} gop {gop} no-b {no_b} mean {mean}",
                        size.0, size.1
                    );
                }
            }
            let start = || -> windows::core::Result<()> {
                mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0)?;
                mft.ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)?;
                mft.ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            };
            if let Err(e) = start() {
                shutdown_mft(&mft);
                return Err(format!("start streaming: {e}"));
            }
            let provides_samples = mft
                .GetOutputStreamInfo(0)
                .is_ok_and(|info| info.dwFlags & MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32 != 0);
            let events: IMFMediaEventGenerator = match mft.cast() {
                Ok(events) => events,
                Err(e) => {
                    shutdown_mft(&mft);
                    return Err(format!("not an asynchronous MFT: {e}"));
                }
            };
            let (tx, _keep) = match events_tx {
                Some(tx) => (tx.clone(), None),
                // The probe listens to nothing.
                None => {
                    let (tx, rx) = mpsc::channel();
                    (tx, Some(rx))
                }
            };
            let callback: IMFAsyncCallback = EventCallback { tx, generation }.into();
            let stream = Self {
                sequence_header: sequence_header(&mft),
                mft,
                events,
                callback,
                codec,
                generation,
                size,
                provides_samples,
                need_input: 0,
                bitrate,
                fps,
                epoch: Instant::now(),
                in_flight: h264::InFlight::default(),
                frame_index: 0,
                produced: false,
            };
            if events_tx.is_some() {
                stream.listen()?;
            }
            Ok(stream)
        }
    }

    /// Asks for the next event.
    fn listen(&self) -> Result<(), String> {
        // SAFETY: live generator and callback; the generator is the state.
        unsafe { self.events.BeginGetEvent(&self.callback, &self.events) }
            .map_err(|e| format!("BeginGetEvent: {e}"))
    }

    fn set_codec(&self, api: &GUID, value: VARIANT) -> bool {
        self.codec
            .as_ref()
            // SAFETY: a live ICodecAPI and a live VARIANT.
            .is_some_and(|codec| unsafe { codec.SetValue(api, &value) }.is_ok())
    }

    fn force_keyframe(&self) {
        if !self.set_codec(&CODECAPI_AVEncVideoForceKeyFrame, variant_u32(1)) {
            eprintln!("[share] h264 keyframe request refused");
        }
    }

    fn set_bitrate(&mut self, bitrate: u64) {
        if self.set_codec(&CODECAPI_AVEncCommonMeanBitRate, variant_u32(bps(bitrate))) {
            self.bitrate = bitrate;
        }
    }

    /// Hands `frame` to the MFT.
    fn encode(&mut self, frame: &I420Buffer, keyframe: bool) -> Result<bool, String> {
        let (w, h) = self.size;
        let luma = w as usize * h as usize;
        let len = luma + luma / 2;
        let (y, u, v) = frame.data();
        let (sy, su, sv) = frame.strides();
        let int = |n: u32| i32::try_from(n).unwrap_or(i32::MAX);
        // SAFETY (whole block): Media Foundation calls on live objects; the
        // NV12 write stays inside the locked buffer of `len` bytes (`w` x
        // `h` luma, then `w` x `h / 2` of interleaved chroma), and the
        // source planes are the frame's own at its size, which is `size`.
        unsafe {
            let buffer = MFCreateMemoryBuffer(len as u32).map_err(|e| format!("buffer: {e}"))?;
            let mut data = std::ptr::null_mut();
            buffer
                .Lock(&mut data, None, None)
                .map_err(|e| format!("lock: {e}"))?;
            let converted = yap_i420_to_nv12(
                y.as_ptr(),
                int(sy),
                u.as_ptr(),
                int(su),
                v.as_ptr(),
                int(sv),
                data,
                int(w),
                data.add(luma),
                int(w),
                int(w),
                int(h),
            );
            let _ = buffer.Unlock();
            if converted != 0 {
                return Err("NV12 conversion failed".into());
            }
            buffer
                .SetCurrentLength(len as u32)
                .map_err(|e| format!("length: {e}"))?;
            let sample = MFCreateSample().map_err(|e| format!("sample: {e}"))?;
            sample
                .AddBuffer(&buffer)
                .map_err(|e| format!("add buffer: {e}"))?;
            let hns = i64::try_from(self.epoch.elapsed().as_nanos() / 100).unwrap_or(i64::MAX);
            let _ = sample.SetSampleTime(hns);
            let _ = sample.SetSampleDuration(10_000_000 / i64::from(self.fps.max(1)));
            if keyframe {
                self.force_keyframe();
            }
            match self.mft.ProcessInput(0, &sample, 0) {
                Ok(()) => {
                    self.in_flight.push(hns, Instant::now());
                    self.frame_index += 1;
                    Ok(true)
                }
                Err(e) if e.code() == MF_E_NOTACCEPTING => Ok(false),
                Err(e) => Err(format!("ProcessInput: {e}")),
            }
        }
    }

    /// Takes the MFT's output after `METransformHaveOutput` and hands each
    /// access unit to `source`.
    fn drain(&mut self, source: &NativeVideoSource, counters: &Counters) -> Result<(), String> {
        loop {
            // SAFETY: Media Foundation calls on live objects; each COM
            // reference put into the output struct is taken back out of its
            // `ManuallyDrop`, so it is released on every path.
            let (result, sample, incomplete) = unsafe {
                let own = if self.provides_samples {
                    None
                } else {
                    let sample = MFCreateSample().map_err(|e| format!("out sample: {e}"))?;
                    let (w, h) = self.size;
                    let buffer = MFCreateMemoryBuffer((w * h * 3 / 2).max(1 << 16))
                        .map_err(|e| format!("out buffer: {e}"))?;
                    sample
                        .AddBuffer(&buffer)
                        .map_err(|e| format!("out add: {e}"))?;
                    Some(sample)
                };
                let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                    dwStreamID: 0,
                    pSample: std::mem::ManuallyDrop::new(own),
                    dwStatus: 0,
                    pEvents: std::mem::ManuallyDrop::new(None),
                }];
                let mut status = 0u32;
                let result = self.mft.ProcessOutput(0, &mut buffers, &mut status);
                let sample = std::mem::ManuallyDrop::take(&mut buffers[0].pSample);
                drop(std::mem::ManuallyDrop::take(&mut buffers[0].pEvents));
                let incomplete =
                    buffers[0].dwStatus & MFT_OUTPUT_DATA_BUFFER_INCOMPLETE.0 as u32 != 0;
                (result, sample, incomplete)
            };
            match result {
                Ok(()) => {}
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(()),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    // An asynchronous MFT answers one ProcessOutput per
                    // METransformHaveOutput: once it has the new type, it
                    // raises another for the output it holds.
                    self.renegotiate_output()?;
                    return Ok(());
                }
                Err(e) => return Err(format!("ProcessOutput: {e}")),
            }
            if let Some(sample) = sample {
                self.send(&sample, source, counters)?;
            }
            if !incomplete {
                return Ok(());
            }
        }
    }

    fn renegotiate_output(&mut self) -> Result<(), String> {
        // SAFETY: live MFT; the type it offers goes straight back to it.
        unsafe {
            let offered = self
                .mft
                .GetOutputAvailableType(0, 0)
                .map_err(|e| format!("output type: {e}"))?;
            self.mft
                .SetOutputType(0, &offered, 0)
                .map_err(|e| format!("set output type: {e}"))?;
        }
        let header = sequence_header(&self.mft);
        if !header.is_empty() {
            self.sequence_header = header;
        }
        Ok(())
    }

    fn send(
        &mut self,
        sample: &IMFSample,
        source: &NativeVideoSource,
        counters: &Counters,
    ) -> Result<(), String> {
        // SAFETY: live sample; the locked bytes are copied before unlocking.
        let (bytes, clean, time) = unsafe {
            let clean = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) == 1;
            let time = sample.GetSampleTime().ok();
            let buffer = sample
                .ConvertToContiguousBuffer()
                .map_err(|e| format!("contiguous: {e}"))?;
            let mut data = std::ptr::null_mut();
            let mut len = 0u32;
            buffer
                .Lock(&mut data, None, Some(&mut len))
                .map_err(|e| format!("lock out: {e}"))?;
            let bytes = std::slice::from_raw_parts(data, len as usize).to_vec();
            let _ = buffer.Unlock();
            (bytes, clean, time)
        };
        if let Some(started) = self.in_flight.take(time) {
            let us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
            counters.latency_total_us.fetch_add(us, Ordering::Relaxed);
        }
        if bytes.is_empty() {
            return Ok(());
        }
        let Some(annex_b) = h264::to_annex_b(&bytes) else {
            eprintln!("[share] h264 output is neither Annex-B nor length-prefixed");
            return Ok(());
        };
        let key = clean || h264::is_idr(&annex_b);
        let payload = if key {
            h264::with_parameter_sets(&annex_b, &self.sequence_header)
        } else {
            std::borrow::Cow::Borrowed(&annex_b[..])
        };
        source.capture_encoded_frame(&EncodedVideoFrame {
            codec: EncodedVideoCodec::H264,
            payload: &payload,
            timestamp_us: 0,
            frame_type: if key {
                EncodedFrameType::Key
            } else {
                EncodedFrameType::Delta
            },
            resolution: VideoResolution {
                width: self.size.0,
                height: self.size.1,
            },
            frame_metadata: None,
        });
        counters.frames_out.fetch_add(1, Ordering::Relaxed);
        self.produced = true;
        if key {
            counters.keyframes.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    fn shutdown(&self) {
        // SAFETY: live MFT.
        unsafe {
            let _ = self.mft.ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
        }
        shutdown_mft(&self.mft);
    }
}

fn shutdown_mft(mft: &IMFTransform) {
    // SAFETY: live MFT; shutting down also cancels a pending BeginGetEvent
    // (its callback gets MF_E_SHUTDOWN and reports `Failed`).
    unsafe {
        let _ = mft.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
        if let Ok(shutdown) = mft.cast::<IMFShutdown>() {
            let _ = shutdown.Shutdown();
        }
    }
}

/// The output type's sequence header (SPS and PPS), if it carries one.
fn sequence_header(mft: &IMFTransform) -> Vec<u8> {
    // SAFETY: live MFT; the blob is read into a buffer of its size.
    unsafe {
        let Ok(kind) = mft.GetOutputCurrentType(0) else {
            return Vec::new();
        };
        let Ok(size) = kind.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else {
            return Vec::new();
        };
        let mut blob = vec![0u8; size as usize];
        if kind
            .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
            .is_err()
        {
            return Vec::new();
        }
        blob
    }
}

/// How long the thread waits with nothing to do before checking again.
const IDLE: Duration = Duration::from_millis(500);
/// At most one forced keyframe per this long (a burst of PLIs is one IDR).
const KEYFRAME_GAP: Duration = Duration::from_millis(250);

fn run(
    shared: &Shared,
    source: &NativeVideoSource,
    rx: &mpsc::Receiver<Msg>,
    events: &mpsc::Sender<Msg>,
) {
    let _com = ComScope::start();
    let mf = match MfScope::start() {
        Ok(mf) => mf,
        Err(why) => {
            eprintln!("[share] h264 encoder: {why}");
            fail(shared, false);
            return;
        }
    };
    // SAFETY: Media Foundation is up for this thread's scope.
    let activates = match unsafe { enumerate() } {
        Ok(list) if list.len() > shared.index => list,
        Ok(_) => {
            eprintln!("[share] h264 encoder: the probed MFT is gone");
            fail(shared, false);
            return;
        }
        Err(why) => {
            eprintln!("[share] h264 encoder: {why}");
            fail(shared, false);
            return;
        }
    };
    let activate = &activates[shared.index];
    let mut stream: Option<Stream> = None;
    let mut generation = 0u64;
    let mut last_key = Instant::now() - KEYFRAME_GAP;
    let mut applied = *lock(&shared.config);
    let fail_after = test_fail_after().filter(|_| !TEST_FAILED.load(Ordering::Relaxed));
    let mut fail_at: Option<Instant> = None;
    loop {
        let msg = match rx.recv_timeout(IDLE) {
            Ok(msg) => Some(msg),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        let outcome = (|| -> Result<bool, String> {
            if let Some(after) = fail_after {
                let produced = shared.counters.frames_out.load(Ordering::Relaxed) > 0;
                if fail_at.is_none() && produced {
                    eprintln!("[share] test hook: failing the h264 encoder in {after:?}");
                    fail_at = Some(Instant::now() + after);
                }
                if fail_at.is_some_and(|at| Instant::now() >= at)
                    && !TEST_FAILED.swap(true, Ordering::Relaxed)
                {
                    return Err("test hook: the MFT stopped (YAP_TEST_MFT_FAIL_AFTER_SECS)".into());
                }
            }
            match msg {
                Some(Msg::Stop) => return Ok(false),
                Some(Msg::Event {
                    generation: g,
                    kind,
                }) => {
                    if let Some(s) = stream.as_mut().filter(|s| s.generation == g) {
                        if kind == METransformNeedInput.0 as u32 {
                            s.need_input += 1;
                        } else if kind == METransformHaveOutput.0 as u32 {
                            s.drain(source, &shared.counters)?;
                        }
                        s.listen()?;
                    }
                }
                Some(Msg::Failed { generation: g }) => {
                    if stream.as_ref().is_some_and(|s| s.generation == g) {
                        return Err("the encoder stopped delivering events".into());
                    }
                }
                Some(Msg::Wake) | None => {}
            }
            let config = *lock(&shared.config);
            if config != applied {
                // A new preset: the next stream opens with it; the running
                // one moves its bitrate cap now.
                if let Some(s) = stream.as_mut() {
                    if s.bitrate > config.max_bitrate || s.fps != config.fps {
                        let size = s.size;
                        s.shutdown();
                        stream = None;
                        eprintln!("[share] h264 preset changed at {}x{}", size.0, size.1);
                    }
                }
                applied = config;
            }
            feed(
                shared,
                source,
                activate,
                events,
                &mut stream,
                &mut generation,
                &mut last_key,
                &applied,
            )?;
            Ok(true)
        })();
        match outcome {
            Ok(true) => {}
            Ok(false) => break,
            Err(why) => {
                // Only a stream that worked marks the hardware broken; one
                // that never produced (an open at this size, a first input
                // refused) falls back for this share alone.
                let produced = stream.as_ref().is_some_and(|s| s.produced);
                eprintln!(
                    "[share] h264 encoder failed{}: {why}",
                    if produced { "" } else { " before any output" }
                );
                fail(shared, produced);
                break;
            }
        }
    }
    if let Some(s) = stream.take() {
        s.shutdown();
    }
    drop(activates);
    drop(mf);
}

/// Hands waiting frames to the MFT while it asks for input, opening a
/// stream at the frame's size when there is none or the size changed.
#[allow(clippy::too_many_arguments)]
fn feed(
    shared: &Shared,
    source: &NativeVideoSource,
    activate: &IMFActivate,
    events: &mpsc::Sender<Msg>,
    stream: &mut Option<Stream>,
    generation: &mut u64,
    last_key: &mut Instant,
    config: &HwConfig,
) -> Result<(), String> {
    loop {
        let Some(frame) = lock(&shared.pending).take() else {
            return Ok(());
        };
        let frame = at_hardware_size(frame);
        let size = (frame.width(), frame.height());
        if stream.as_ref().is_some_and(|s| s.size != size) {
            if let Some(old) = stream.take() {
                old.shutdown();
            }
        }
        if stream.is_none() {
            *generation += 1;
            // SAFETY: Media Foundation and COM are up on this thread; the
            // activate is live for the thread's run.
            let opened =
                unsafe { Stream::open(activate, size, config, *generation, Some(events)) }?;
            shared
                .counters
                .bitrate
                .store(opened.bitrate, Ordering::Relaxed);
            shared.counters.size.store(
                (u64::from(size.0) << 32) | u64::from(size.1),
                Ordering::Relaxed,
            );
            *stream = Some(opened);
        }
        let Some(s) = stream.as_mut() else {
            return Ok(());
        };
        if s.need_input == 0 {
            // Not yet: keep the frame unless a newer one came meanwhile.
            let mut pending = lock(&shared.pending);
            if pending.is_none() {
                *pending = Some(frame);
            }
            return Ok(());
        }
        if let Some(rate) = source.take_rate_control_request() {
            if let Some(next) =
                h264::next_bitrate(s.bitrate, rate.target_bitrate_bps, config.max_bitrate)
            {
                s.set_bitrate(next);
                shared.counters.bitrate.store(s.bitrate, Ordering::Relaxed);
            }
        }
        // Inside the gap the request is left with the pass-through, so a
        // later frame honours it (taking it first would drop it).
        let keyframe = last_key.elapsed() >= KEYFRAME_GAP && source.take_keyframe_request();
        if keyframe {
            *last_key = Instant::now();
        }
        let index = s.frame_index;
        if !keyframe && !h264::keep_frame(config.detail, s.bitrate, config.max_bitrate, index) {
            s.frame_index += 1;
            continue;
        }
        if s.encode(&frame, keyframe)? {
            s.need_input -= 1;
        } else {
            s.need_input = 0;
        }
    }
}

/// `frame` at the size the hardware encodes it ([`h264::hardware_size`]):
/// as it is, or centred on black when it is smaller (a tiny window).
fn at_hardware_size(frame: Arc<I420Buffer>) -> Arc<I420Buffer> {
    let (w, h) = (frame.width(), frame.height());
    let (tw, th) = h264::hardware_size(w, h);
    if (tw, th) == (w, h) {
        return frame;
    }
    let mut out = I420Buffer::new(tw, th);
    let (dsy, dsu, dsv) = out.strides();
    let (ssy, ssu, ssv) = frame.strides();
    let (sy, su, sv) = frame.data();
    let (ox, oy) = (h264::pad_offset(w, tw), h264::pad_offset(h, th));
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let (dy, du, dv) = out.data_mut();
    dy.fill(16);
    du.fill(128);
    dv.fill(128);
    copy_plane((sy, ssy), (dy, dsy), (w, h), (ox, oy));
    copy_plane((su, ssu), (du, dsu), (cw, ch), (ox / 2, oy / 2));
    copy_plane((sv, ssv), (dv, dsv), (cw, ch), (ox / 2, oy / 2));
    Arc::new(out)
}

/// Copies a `size` plane (`src`, its stride) into `dst` (its stride) at
/// `at`. Rows that would not fit are left out.
fn copy_plane(
    (src, src_stride): (&[u8], u32),
    (dst, dst_stride): (&mut [u8], u32),
    (w, h): (u32, u32),
    (x, y): (u32, u32),
) {
    let (w, x) = (w as usize, x as usize);
    let (ss, ds) = (src_stride as usize, dst_stride as usize);
    for row in 0..h as usize {
        let from = src.get(row * ss..row * ss + w);
        let at = (y as usize + row) * ds + x;
        if let (Some(from), Some(to)) = (from, dst.get_mut(at..at + w)) {
            to.copy_from_slice(from);
        }
    }
}
