//! A live Windows Graphics Capture session: frames from a monitor or window
//! paced to the share's frame rate, read back to the CPU and handed to
//! `yap-voice`.
//!
//! On an HDR display the pool delivers `R16G16B16A16Float` (scRGB) instead
//! of BGRA8, and a pixel shader on the capture's device tone-maps each frame
//! to BGRA8 SDR before the same readback (`hdr.rs`). The display is watched
//! while the share runs: HDR switched on or off, or a shared window moved
//! to another monitor, recreates the pool in the other format.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use windows::core::IInspectable;
use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use yap_voice::{push_screen_frame, ScreenFrame, ShareQuality};

use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;

use super::gpu::{frame_texture, texture_format, texture_size, Gpu, Staging};
use super::hdr::{timing_enabled, ColorTarget, ColorWatch, FrameTiming, HdrConverter, FP16_FORMAT};
use crate::pace::{interval_hns, min_update_floor_hns, FramePacer};
use crate::stop::Ender;
use crate::tonemap::ToneMap;

/// The frame pool's buffers.
const POOL_BUFFERS: i32 = 2;
/// An SDR display's frames, pushed as they are.
const SDR_FORMAT: DirectXPixelFormat = DirectXPixelFormat::B8G8R8A8UIntNormalized;
/// An HDR display's frames (scRGB), tone-mapped on the GPU first.
const HDR_FORMAT: DirectXPixelFormat = DirectXPixelFormat::R16G16B16A16Float;

fn pool_format(map: Option<ToneMap>) -> DirectXPixelFormat {
    if map.is_some() {
        HDR_FORMAT
    } else {
        SDR_FORMAT
    }
}

/// A value that crosses threads although its type does not say so.
struct Agile<T>(T);

// SAFETY: only used for the D3D11 device objects below. D3D11 devices are
// free-threaded; the immediate context is used only under `Shared::state`'s
// lock, and the device runs with multithread protection on.
unsafe impl<T> Send for Agile<T> {}
// SAFETY: as for `Send`.
unsafe impl<T> Sync for Agile<T> {}

/// The share's quality, readable from the frame callback while
/// `set_quality` writes it. The fields are independent: a frame may see the
/// new box with the old rate for one frame, which does no harm.
pub(super) struct QualityCell {
    width: AtomicU32,
    height: AtomicU32,
    bitrate: AtomicU64,
    fps: AtomicU32,
}

impl QualityCell {
    pub(super) fn new(quality: ShareQuality) -> Self {
        Self {
            width: AtomicU32::new(quality.width),
            height: AtomicU32::new(quality.height),
            bitrate: AtomicU64::new(quality.bitrate),
            fps: AtomicU32::new(quality.fps),
        }
    }

    pub(super) fn store(&self, quality: ShareQuality) {
        self.width.store(quality.width, Ordering::Relaxed);
        self.height.store(quality.height, Ordering::Relaxed);
        self.bitrate.store(quality.bitrate, Ordering::Relaxed);
        self.fps.store(quality.fps, Ordering::Relaxed);
    }

    fn load(&self) -> ShareQuality {
        ShareQuality {
            width: self.width.load(Ordering::Relaxed),
            height: self.height.load(Ordering::Relaxed),
            bitrate: self.bitrate.load(Ordering::Relaxed),
            fps: self.fps.load(Ordering::Relaxed),
        }
    }
}

/// What the frame callback keeps between frames.
struct FrameState {
    pacer: FramePacer,
    /// The size the frame pool was created or recreated at.
    pool_size: SizeInt32,
    /// The format it was created or recreated in.
    pool_format: DirectXPixelFormat,
    staging: Option<Staging>,
    /// Made on the first FP16 frame.
    converter: Option<HdrConverter>,
    /// The last tone map used, for an FP16 frame that arrives after the
    /// display went back to SDR (the pool is recreated by then).
    last_map: Option<ToneMap>,
    /// `YAP_CAPTURE_TIMING`.
    timing: Option<FrameTiming>,
}

/// What the callbacks share with the session.
struct Shared {
    gpu: Agile<Gpu>,
    quality: QualityCell,
    /// Also the lock the immediate context is used under.
    state: Mutex<FrameState>,
    closed: AtomicBool,
    /// Set while the game capture hook carries the share: frames are
    /// dropped as they arrive, so WGC resumes the moment it is cleared.
    gated: Arc<AtomicBool>,
    /// Error sites already reported.
    reported: Mutex<HashSet<&'static str>>,
    /// The tone map the display wants now (`None`: SDR, BGRA8), kept by
    /// the session's [`ColorWatch`].
    wanted: Arc<Mutex<Option<ToneMap>>>,
    /// Cleared when FP16 failed on this device: the share stays BGRA8.
    fp16_ok: AtomicBool,
    /// For the timing log.
    label: String,
}

/// A frame error: where, and what.
struct FrameError {
    site: &'static str,
    message: String,
}

fn at<E: std::fmt::Display>(site: &'static str) -> impl FnOnce(E) -> FrameError {
    move |error| FrameError {
        site,
        message: error.to_string(),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shared {
    /// Never panics, never returns an error: a callback has nowhere to put
    /// either. The first error of each kind is logged.
    fn on_frame(&self, pool: &Direct3D11CaptureFramePool) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let outcome = catch_unwind(AssertUnwindSafe(|| self.handle_frame(pool)));
        let error = match outcome {
            Ok(Ok(())) => return,
            Ok(Err(error)) => error,
            Err(_) => FrameError {
                site: "panic",
                message: "the frame callback panicked".into(),
            },
        };
        if lock(&self.reported).insert(error.site) {
            eprintln!("[capture] {}: {}", error.site, error.message);
        }
    }

    fn handle_frame(&self, pool: &Direct3D11CaptureFramePool) -> Result<(), FrameError> {
        // No frame (the pool is closing, or another event took it) is not
        // an error.
        let Ok(frame) = pool.TryGetNextFrame() else {
            return Ok(());
        };
        if self.gated.load(Ordering::Acquire) {
            return Ok(());
        }
        let pts_hns = frame
            .SystemRelativeTime()
            .map(|time| time.Duration)
            .unwrap_or(0);
        let quality = self.quality.load();

        let mut state = lock(&self.state);
        if self.closed.load(Ordering::Acquire) {
            return Ok(());
        }
        if !state.pacer.admit(pts_hns, interval_hns(quality.fps)) {
            return Ok(());
        }

        let content = frame.ContentSize().map_err(at("content size"))?;
        let (Ok(width), Ok(height)) = (u32::try_from(content.Width), u32::try_from(content.Height))
        else {
            return Ok(());
        };
        if width == 0 || height == 0 {
            return Ok(());
        }
        let wanted = if self.fp16_ok.load(Ordering::Relaxed) {
            *lock(&self.wanted)
        } else {
            None
        };
        // The source changed shape, or its display changed between SDR and
        // HDR: rebuild the pool so the next frames carry all of it, in the
        // format the display needs. This frame belongs to the old pool and
        // stays valid.
        let format = pool_format(wanted);
        if state.pool_size != content || state.pool_format != format {
            pool.Recreate(&self.gpu.0.winrt, format, POOL_BUFFERS, content)
                .map_err(at("recreate pool"))?;
            state.pool_size = content;
            state.pool_format = format;
        }

        let texture = frame_texture(&frame).map_err(at("frame texture"))?;
        let (texture_width, texture_height) = texture_size(&texture);
        // Mid-resize the texture can still be the old, smaller size.
        let (width, height) = (width.min(texture_width), height.min(texture_height));
        let staging = match state.staging.take() {
            Some(staging) if staging.size() == (width, height) => staging,
            _ => Staging::new(&self.gpu.0.device, width, height).map_err(at("staging"))?,
        };
        // By the frame's own format: one from before a switch is still in
        // the old one.
        let fp16 = texture_format(&texture) == FP16_FORMAT;
        let context = &self.gpu.0.context;
        let state = &mut *state;
        if let Some(timing) = state.timing.as_mut() {
            timing.begin(context);
        }
        if fp16 {
            let map = wanted
                .or(state.last_map)
                .unwrap_or_else(|| ToneMap::from_nits(80.0, 80.0));
            state.last_map = Some(map);
            let converted = match self.convert(state, &texture, (width, height), map) {
                Ok(converted) => converted,
                Err(error) => {
                    state.staging = Some(staging);
                    return Err(error);
                }
            };
            if let Some(timing) = state.timing.as_ref() {
                timing.converted(context);
            }
            staging.copy(context, &converted);
        } else {
            if let Some(timing) = state.timing.as_ref() {
                timing.converted(context);
            }
            staging.copy(context, &texture);
        }
        if let Some(timing) = state.timing.as_ref() {
            timing.copied(context);
        }
        let mut read_done = None;
        let pushed = staging.map(context, |bgra, stride| {
            read_done = Some(Instant::now());
            push_screen_frame(&ScreenFrame {
                bgra,
                stride,
                width,
                height,
                quality,
            })
        });
        state.staging = Some(staging);
        if let (Some(timing), Some(read_done)) = (state.timing.as_mut(), read_done) {
            timing.finish(context, read_done, &self.label, fp16);
        }
        pushed.map(drop).map_err(at("read frame"))
    }

    /// Tone-maps an FP16 frame into the converter's BGRA8 texture. A
    /// converter that cannot be made turns FP16 off for the session (the
    /// next frame recreates the pool in BGRA8).
    fn convert(
        &self,
        state: &mut FrameState,
        texture: &ID3D11Texture2D,
        size: (u32, u32),
        map: ToneMap,
    ) -> Result<ID3D11Texture2D, FrameError> {
        if state.converter.is_none() {
            match HdrConverter::new(&self.gpu.0.device) {
                Ok(converter) => state.converter = Some(converter),
                Err(error) => {
                    self.fp16_ok.store(false, Ordering::Relaxed);
                    return Err(at("hdr shader")(error));
                }
            }
        }
        let Some(converter) = state.converter.as_mut() else {
            return Err(at("hdr shader")("no converter"));
        };
        converter
            .convert(&self.gpu.0.device, &self.gpu.0.context, texture, size, map)
            .cloned()
            .map_err(at("hdr convert"))
    }
}

/// Asks WGC not to deliver much faster than the share's frame rate
/// (`MinUpdateInterval`, Windows 11 24H2 and later). Elsewhere it fails and
/// the callback's pacer does all of the pacing.
fn apply_min_update_interval(session: &GraphicsCaptureSession, fps: u32) {
    let _ = session.SetMinUpdateInterval(TimeSpan {
        Duration: min_update_floor_hns(fps),
    });
}

/// A running capture. Dropping it unregisters the handlers and closes the
/// session and the pool, and no frame is pushed after the drop returns.
pub(super) struct WgcSession {
    item: GraphicsCaptureItem,
    session: GraphicsCaptureSession,
    pool: Direct3D11CaptureFramePool,
    arrived: i64,
    closed: Option<i64>,
    shared: Arc<Shared>,
    size: (u32, u32),
    /// Follows the display's HDR state; stopped after the pool closes.
    _color: ColorWatch,
}

// SAFETY: the WinRT capture classes held here are agile (free-threaded):
// they may be used and released from any thread. `Shared` is `Send + Sync`
// by its own fields.
unsafe impl Send for WgcSession {}

impl WgcSession {
    /// Starts capturing `item`, which shows `target`. `ended` fires if the
    /// item closes.
    pub(super) fn start(
        item: GraphicsCaptureItem,
        target: ColorTarget,
        label: String,
        gpu: Gpu,
        quality: ShareQuality,
        cursor: bool,
        ended: Arc<Ender>,
    ) -> Result<Self, String> {
        let size = item.Size().map_err(|e| format!("no capture size: {e}"))?;
        let color = ColorWatch::start(target, label.clone());
        let mut format = pool_format(color.now());
        let mut fp16_ok = true;
        let created = |format| {
            Direct3D11CaptureFramePool::CreateFreeThreaded(&gpu.winrt, format, POOL_BUFFERS, size)
        };
        let pool = match created(format) {
            Err(error) if format == HDR_FORMAT => {
                eprintln!("[capture] {label} no FP16 frame pool ({error}): BGRA8 instead");
                (format, fp16_ok) = (SDR_FORMAT, false);
                created(format)
            }
            pool => pool,
        }
        .map_err(|e| format!("could not create the frame pool: {e}"))?;
        let timing = if timing_enabled() {
            FrameTiming::new(&gpu.device)
                .map_err(|e| eprintln!("[capture] no timing queries: {e}"))
                .ok()
        } else {
            None
        };
        let session = pool
            .CreateCaptureSession(&item)
            .map_err(|e| format!("could not create the capture session: {e}"))?;

        // Failures are fine: the cursor option and the border switch are
        // newer than the session itself.
        let _ = session.SetIsCursorCaptureEnabled(cursor);
        let _ = session.SetIsBorderRequired(false);
        apply_min_update_interval(&session, quality.fps);

        let shared = Arc::new(Shared {
            gpu: Agile(gpu),
            quality: QualityCell::new(quality),
            state: Mutex::new(FrameState {
                pacer: FramePacer::default(),
                pool_size: size,
                pool_format: format,
                staging: None,
                converter: None,
                last_map: None,
                timing,
            }),
            closed: AtomicBool::new(false),
            gated: Arc::new(AtomicBool::new(false)),
            reported: Mutex::new(HashSet::new()),
            wanted: color.cell(),
            fp16_ok: AtomicBool::new(fp16_ok),
            label,
        });

        let on_frame = Arc::clone(&shared);
        let arrived = pool
            .FrameArrived(
                &TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(
                    move |pool, _| {
                        if let Some(pool) = pool.as_ref() {
                            on_frame.on_frame(pool);
                        }
                        Ok(())
                    },
                ),
            )
            .map_err(|e| format!("could not watch for frames: {e}"))?;

        let on_closed = Arc::clone(&ended);
        let closed = item
            .Closed(
                &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new(move |_, _| {
                    on_closed.fire();
                    Ok(())
                }),
            )
            .map_err(|e| eprintln!("[capture] could not watch for the source closing: {e}"))
            .ok();

        // From here a failure drops `capture`, which unregisters the
        // handlers and closes the session and the pool.
        let capture = Self {
            item,
            session,
            pool,
            arrived,
            closed,
            shared,
            size: (size.Width.max(0) as u32, size.Height.max(0) as u32),
            _color: color,
        };
        capture
            .session
            .StartCapture()
            .map_err(|e| format!("could not start the capture: {e}"))?;
        Ok(capture)
    }

    /// The gate the game capture hook holds while it carries the share.
    pub(super) fn gate(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shared.gated)
    }

    /// The source's size when the capture started.
    pub(super) fn size(&self) -> (u32, u32) {
        self.size
    }

    pub(super) fn set_quality(&self, quality: ShareQuality) {
        self.shared.quality.store(quality);
        apply_min_update_interval(&self.session, quality.fps);
    }
}

impl Drop for WgcSession {
    fn drop(&mut self) {
        // First stop new work and wait out a callback that is running (it
        // holds `state` while it reads the frame), so nothing reads a frame
        // of the pool while it closes; none that follows pushes.
        self.shared.closed.store(true, Ordering::Release);
        drop(lock(&self.shared.state));
        let _ = self.pool.RemoveFrameArrived(self.arrived);
        if let Some(token) = self.closed {
            let _ = self.item.RemoveClosed(token);
        }
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
