//! Camera backgrounds (`src/lib/camera-background-effects.ts`): blur the
//! room behind you, or put an image there, from a person mask that
//! MediaPipe's selfie segmenter computes.
//!
//! - The app installs an effect ([`set_camera_background`]) with a factory
//!   for its [`Segmenter`] (ONNX Runtime in `yap-segment`); `None` turns it
//!   off. Without an effect a camera frame costs nothing here.
//! - Each camera frame ([`Effect::apply`], on the capture thread, in I420): at most
//!   [`SEGMENT_FPS`] times a second its 256x256 RGB goes to the segmenter
//!   thread (newest wins); every frame is composited with the latest mask.
//!   Until the first mask the camera shows as it is, as on the web.
//! - The segmenter thread starts with the first frame and stops when the
//!   camera does ([`camera_stopped`]): the model, its ONNX session and GPU
//!   memory, and the frame buffers are let go until the next frame. Neither
//!   stopping it nor dropping the effect waits for it (a model still
//!   loading finishes on its own thread and is dropped there).
//! - The mask: the model's person confidence through the web's smoothstep
//!   (0.35..0.8) and 9-tap feather, smoothed over time (each new mask
//!   moves [`MASK_EMA`] of the way, so the edge does not shimmer between
//!   12 fps masks), then scaled up bilinearly per row.
//! - Blur: the frame shrunk 4x (box), three box passes (about a Gaussian of
//!   the web's 14 or 28 px at 720p), scaled back up. Image: the picture
//!   scaled to cover the frame, as `drawCover` draws it, converted once per
//!   frame size.
//!
//! Effects run at most at 1280x720 ([`MAX_EFFECT_WIDTH`]), as the web's
//! effect canvas does; a larger capture is scaled down first.

use std::sync::{Arc, Condvar, Mutex, PoisonError, RwLock};
use std::time::{Duration, Instant};

/// The model's input and output side.
pub const MASK_SIZE: usize = 256;
/// Segmentations a second (`SEGMENT_FPS`).
pub const SEGMENT_FPS: u32 = 12;
/// The largest frame an effect runs at (`MAX_EFFECT_WIDTH` x `_HEIGHT`).
pub const MAX_EFFECT_WIDTH: u32 = 1280;
pub const MAX_EFFECT_HEIGHT: u32 = 720;
/// `MASK_CONFIDENCE_EDGE_MIN` / `_MAX`.
const EDGE_MIN: f32 = 0.35;
const EDGE_MAX: f32 = 0.8;
/// How far each new mask moves the shown one.
pub const MASK_EMA: f32 = 0.6;
/// `BLUR_BACKGROUND_PX` at 720p.
pub const BLUR_LIGHT_PX: f32 = 14.0;
pub const BLUR_STRONG_PX: f32 = 28.0;
/// The blur works on the frame shrunk this many times.
const BLUR_SHRINK: usize = 4;

/// An RGBA picture (straight alpha), as decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// What goes behind the person (`CameraBackgroundSetting`).
#[derive(Debug, Clone, PartialEq)]
pub enum Background {
    /// `blur`, `strength: strong` (or light).
    Blur { strong: bool },
    /// A picture, scaled to cover.
    Image(Arc<Rgba>),
}

/// Computes person masks: 3x256x256 RGB in 0..1 (planar) in, 256x256
/// confidences in 0..1 out.
pub trait Segmenter: Send {
    fn segment(&mut self, rgb: &[f32]) -> Result<Vec<f32>, String>;
    /// The backend, for the log and the details (`DirectML`, `CPU`).
    fn backend(&self) -> String;
}

/// Makes the segmenter on the segmenter thread (loading a model can take a
/// moment and must not hold up the camera). Called again each time the
/// camera starts after a stop.
pub type SegmenterFactory = Arc<dyn Fn() -> Result<Box<dyn Segmenter>, String> + Send + Sync>;

/// An I420 frame borrowed mutably (the encoder's own buffer).
pub struct FrameMut<'a> {
    pub width: usize,
    pub height: usize,
    pub y: &'a mut [u8],
    pub u: &'a mut [u8],
    pub v: &'a mut [u8],
    pub stride_y: usize,
    pub stride_u: usize,
    pub stride_v: usize,
}

/// Owned I420 planes, tightly packed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Planes {
    pub width: usize,
    pub height: usize,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl Planes {
    fn chroma(&self) -> (usize, usize) {
        (self.width.div_ceil(2), self.height.div_ceil(2))
    }
}

/// How the effect is doing, for the log and the settings row.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EffectStats {
    /// The segmenter's backend once it loaded; empty before.
    pub backend: String,
    /// Why it could not load, if it could not.
    pub error: String,
    pub segmentations: u64,
    /// Mean segmentation time (model input to refined mask), ms.
    pub segment_ms: f64,
    pub frames: u64,
    /// Mean per-frame cost on the camera thread (input, background,
    /// composite), ms.
    pub frame_ms: f64,
}

#[derive(Default)]
struct Slot {
    input: Option<Vec<f32>>,
    stop: bool,
}

/// What the camera thread and one segmenter thread share.
#[derive(Default)]
struct Shared {
    slot: Mutex<Slot>,
    wake: Condvar,
    mask: Mutex<Option<Arc<Vec<u8>>>>,
}

/// A running segmenter thread. Dropping it asks the thread to stop and
/// does not wait: the thread ends after the segmentation (or the model
/// load) it is in, and drops the model there.
struct Worker {
    shared: Arc<Shared>,
}

impl Worker {
    fn start(factory: SegmenterFactory, stats: Arc<Mutex<EffectStats>>) -> Self {
        let shared = Arc::new(Shared::default());
        let thread_shared = Arc::clone(&shared);
        let spawned = std::thread::Builder::new()
            .name("yap-segment".into())
            .spawn(move || segment_loop(&thread_shared, &*factory, &stats));
        // Detached (see the type's docs); a failed spawn leaves no masks,
        // so the camera shows as it is.
        if let Err(e) = spawned {
            eprintln!("[camera] segmenter thread: {e}");
        }
        Self { shared }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        lock(&self.shared.slot).stop = true;
        self.shared.wake.notify_one();
    }
}

#[derive(Default)]
struct Scratch {
    last_sent: Option<Instant>,
    background: Planes,
    /// The image background converted for one frame size.
    cover: Option<((usize, usize), Planes)>,
    small: Planes,
    tmp: Vec<u8>,
    /// The latest mask scaled to the frame (rebuilt when either changes,
    /// [`SEGMENT_FPS`] times a second at most, not every frame).
    alphas: Option<(Arc<Vec<u8>>, Alphas)>,
    frame_ns: u128,
    /// Per-stage time since the last log line: input, background, blend.
    stage_ns: [u128; 3],
    stage_frames: u32,
    logged: Option<Instant>,
}

/// A chosen background: what goes behind the person, the segmenter
/// thread while the camera runs, and the camera thread's scratch.
pub struct Effect {
    background: Background,
    factory: SegmenterFactory,
    stats: Arc<Mutex<EffectStats>>,
    /// Started by the first frame, stopped by [`Self::release`].
    worker: Mutex<Option<Worker>>,
    scratch: Mutex<Scratch>,
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Effect {
    /// A background; nothing runs (and no model loads) until a frame comes.
    pub fn start(background: Background, factory: SegmenterFactory) -> Self {
        Self {
            background,
            factory,
            stats: Arc::new(Mutex::new(EffectStats::default())),
            worker: Mutex::new(None),
            scratch: Mutex::new(Scratch::default()),
        }
    }

    pub fn stats(&self) -> EffectStats {
        lock(&self.stats).clone()
    }

    /// The camera stopped: the segmenter thread stops (without waiting for
    /// it) and the frame buffers go. The next frame starts afresh.
    pub fn release(&self) {
        let worker = lock(&self.worker).take();
        drop(worker);
        *lock(&self.scratch) = Scratch::default();
    }

    /// The running segmenter's shared state, started if it is not.
    fn shared(&self) -> Arc<Shared> {
        let mut worker = lock(&self.worker);
        let worker = worker.get_or_insert_with(|| {
            Worker::start(Arc::clone(&self.factory), Arc::clone(&self.stats))
        });
        Arc::clone(&worker.shared)
    }

    /// Composites one frame in place.
    pub fn apply(&self, frame: &mut FrameMut<'_>) {
        let started = Instant::now();
        let shared = self.shared();
        let mut scratch = lock(&self.scratch);
        let due = scratch
            .last_sent
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(1) / SEGMENT_FPS);
        if due {
            scratch.last_sent = Some(Instant::now());
            let input = model_input(frame);
            let mut slot = lock(&shared.slot);
            slot.input = Some(input);
            shared.wake.notify_one();
        }
        let mask = lock(&shared.mask).clone();
        let Some(mask) = mask else {
            return;
        };
        let scratch = &mut *scratch;
        let input_done = Instant::now();
        let background = match &self.background {
            Background::Blur { strong } => {
                let px = if *strong {
                    BLUR_STRONG_PX
                } else {
                    BLUR_LIGHT_PX
                };
                blurred(
                    frame,
                    px,
                    &mut scratch.small,
                    &mut scratch.tmp,
                    &mut scratch.background,
                );
                &scratch.background
            }
            Background::Image(image) => {
                let size = (frame.width, frame.height);
                if scratch.cover.as_ref().is_none_or(|(s, _)| *s != size) {
                    scratch.cover = Some((size, cover(image, size.0, size.1)));
                }
                match scratch.cover.as_ref() {
                    Some((_, planes)) => planes,
                    None => return,
                }
            }
        };
        let background_done = Instant::now();
        let fresh = scratch.alphas.as_ref().is_none_or(|(m, a)| {
            !Arc::ptr_eq(m, &mask) || a.width != frame.width || a.height != frame.height
        });
        if fresh {
            scratch.alphas = Some((
                Arc::clone(&mask),
                Alphas::of(&mask, frame.width, frame.height),
            ));
        }
        if let Some((_, alphas)) = scratch.alphas.as_ref() {
            composite(frame, background, alphas);
        }
        let done = Instant::now();
        scratch.stage_ns[0] += (input_done - started).as_nanos();
        scratch.stage_ns[1] += (background_done - input_done).as_nanos();
        scratch.stage_ns[2] += (done - background_done).as_nanos();
        scratch.stage_frames += 1;
        scratch.frame_ns += (done - started).as_nanos();
        let mut stats = lock(&self.stats);
        stats.frames += 1;
        stats.frame_ms = scratch.frame_ns as f64 / 1e6 / stats.frames as f64;
        if scratch
            .logged
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(10))
        {
            let n = f64::from(scratch.stage_frames.max(1));
            let ms = |ns: u128| ns as f64 / 1e6 / n;
            eprintln!(
                "[camera] background {} {}x{}: segment {:.2} ms x{}, frame input {:.2} + background {:.2} + blend {:.2} ms",
                stats.backend,
                frame.width,
                frame.height,
                stats.segment_ms,
                stats.segmentations,
                ms(scratch.stage_ns[0]),
                ms(scratch.stage_ns[1]),
                ms(scratch.stage_ns[2]),
            );
            scratch.stage_ns = [0; 3];
            scratch.stage_frames = 0;
            scratch.logged = Some(Instant::now());
        }
    }
}

/// The next input, or `None` once the effect stops.
fn next_input(shared: &Shared) -> Option<Vec<f32>> {
    let mut slot = lock(&shared.slot);
    loop {
        if slot.stop {
            return None;
        }
        if let Some(input) = slot.input.take() {
            return Some(input);
        }
        slot = shared
            .wake
            .wait(slot)
            .unwrap_or_else(PoisonError::into_inner);
    }
}

fn segment_loop(
    shared: &Shared,
    factory: &(dyn Fn() -> Result<Box<dyn Segmenter>, String> + Send + Sync),
    stats: &Mutex<EffectStats>,
) {
    // Nothing loads until a frame comes: a background chosen in the
    // settings costs nothing while no camera runs.
    let Some(mut pending) = next_input(shared) else {
        return;
    };
    let mut segmenter = match factory() {
        Ok(s) => {
            let backend = s.backend();
            eprintln!("[camera] background segmentation on {backend}");
            lock(stats).backend = backend;
            s
        }
        Err(why) => {
            eprintln!("[camera] background segmentation unavailable: {why}");
            lock(stats).error = why;
            return;
        }
    };
    let mut shown: Option<Vec<f32>> = None;
    let mut total_ns = 0u128;
    loop {
        // Stopped while the model loaded or segmented: drop it here.
        if lock(&shared.slot).stop {
            return;
        }
        let input = std::mem::take(&mut pending);
        let started = Instant::now();
        match segmenter.segment(&input) {
            Ok(raw) if raw.len() == MASK_SIZE * MASK_SIZE => {
                let next = smooth(shown.as_deref(), &raw);
                let refined = refine(&next);
                shown = Some(next);
                *lock(&shared.mask) = Some(Arc::new(refined));
            }
            Ok(raw) => eprintln!("[camera] segmenter gave {} values", raw.len()),
            Err(why) => eprintln!("[camera] segmentation failed: {why}"),
        }
        total_ns += started.elapsed().as_nanos();
        {
            let mut stats = lock(stats);
            stats.segmentations += 1;
            stats.segment_ms = total_ns as f64 / 1e6 / stats.segmentations as f64;
        }
        let Some(next) = next_input(shared) else {
            return;
        };
        pending = next;
    }
}

/// The running effect; `None` while no background is chosen.
static EFFECT: RwLock<Option<Arc<Effect>>> = RwLock::new(None);

/// Turns a background on (or off with `None`). A new background starts a
/// new segmenter; the old one stops. Never waits on a segmenter thread, so
/// it is safe on the GUI thread.
pub fn set_camera_background(effect: Option<(Background, SegmenterFactory)>) {
    let next = effect.map(|(background, factory)| Arc::new(Effect::start(background, factory)));
    let old = std::mem::replace(
        &mut *EFFECT.write().unwrap_or_else(PoisonError::into_inner),
        next,
    );
    drop(old);
}

/// The camera stopped: the running effect lets go of its segmenter and
/// buffers until the next frame.
pub fn camera_stopped() {
    if let Some(effect) = camera_effect() {
        effect.release();
    }
}

/// The running effect, if any.
pub fn camera_effect() -> Option<Arc<Effect>> {
    EFFECT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// Whether a background is on (frames then go through Rust even while
/// only the preview shows them).
pub fn camera_effect_active() -> bool {
    EFFECT
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .is_some()
}

/// BT.601 limited-range YUV to RGB in 0..1.
fn rgb(y: u8, u: u8, v: u8) -> [f32; 3] {
    let c = (f32::from(y) - 16.0) * 1.164;
    let d = f32::from(u) - 128.0;
    let e = f32::from(v) - 128.0;
    let clamp = |x: f32| (x / 255.0).clamp(0.0, 1.0);
    [
        clamp(c + 1.596 * e),
        clamp(c - 0.392 * d - 0.813 * e),
        clamp(c + 2.017 * d),
    ]
}

/// The model's input: the frame stretched to 256x256 (as the segmenter
/// resizes it), planar RGB in 0..1.
pub fn model_input(frame: &FrameMut<'_>) -> Vec<f32> {
    let n = MASK_SIZE * MASK_SIZE;
    let mut out = vec![0f32; 3 * n];
    for oy in 0..MASK_SIZE {
        let sy = ((oy * 2 + 1) * frame.height / (MASK_SIZE * 2)).min(frame.height - 1);
        let cy = (sy / 2).min(frame.height.div_ceil(2) - 1);
        for ox in 0..MASK_SIZE {
            let sx = ((ox * 2 + 1) * frame.width / (MASK_SIZE * 2)).min(frame.width - 1);
            let cx = (sx / 2).min(frame.width.div_ceil(2) - 1);
            let [r, g, b] = rgb(
                frame.y[sy * frame.stride_y + sx],
                frame.u[cy * frame.stride_u + cx],
                frame.v[cy * frame.stride_v + cx],
            );
            let at = oy * MASK_SIZE + ox;
            out[at] = r;
            out[n + at] = g;
            out[2 * n + at] = b;
        }
    }
    out
}

/// Temporal smoothing: the shown confidences move [`MASK_EMA`] of the way
/// to the new ones.
pub fn smooth(shown: Option<&[f32]>, raw: &[f32]) -> Vec<f32> {
    match shown {
        Some(shown) if shown.len() == raw.len() => shown
            .iter()
            .zip(raw)
            .map(|(s, r)| s + (r - s) * MASK_EMA)
            .collect(),
        _ => raw.to_vec(),
    }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(f32::EPSILON)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The web's mask: smoothstep on the confidence, then its 9-tap feather
/// (0.4 centre, 0.1 sides, 0.05 corners), as 0..=255 alphas.
pub fn refine(confidence: &[f32]) -> Vec<u8> {
    let n = MASK_SIZE;
    // The alphas with a one-pixel border repeating the edge.
    let p = n + 2;
    let mut padded = vec![0f32; p * p];
    for y in 0..p {
        let sy = y.saturating_sub(1).min(n - 1);
        for x in 0..p {
            let sx = x.saturating_sub(1).min(n - 1);
            padded[y * p + x] = smoothstep(EDGE_MIN, EDGE_MAX, confidence[sy * n + sx]);
        }
    }
    let mut out = vec![0u8; n * n];
    for y in 0..n {
        let (up, mid, down) = (
            &padded[y * p..][..p],
            &padded[(y + 1) * p..][..p],
            &padded[(y + 2) * p..][..p],
        );
        for (x, o) in out[y * n..][..n].iter_mut().enumerate() {
            let sides = mid[x] + mid[x + 2] + up[x + 1] + down[x + 1];
            let corners = up[x] + up[x + 2] + down[x] + down[x + 2];
            let v = 0.4 * mid[x + 1] + 0.1 * sides + 0.05 * corners;
            *o = (v * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Where each output column samples the mask: the left column and the
/// right one's weight in 0..=256.
fn mask_columns(width: usize) -> Vec<(usize, u32)> {
    let n = MASK_SIZE;
    (0..width)
        .map(|x| {
            let fx = ((x as f32 + 0.5) * n as f32 / width as f32 - 0.5).clamp(0.0, (n - 1) as f32);
            let x0 = (fx as usize).min(n - 2);
            (x0, ((fx - x0 as f32) * 256.0).round() as u32)
        })
        .collect()
}

/// One output row of the mask for output row `y` of `height` (bilinear,
/// fixed point), as 0..=255. `between` is scratch for the mask row
/// interpolated vertically.
fn mask_row(
    mask: &[u8],
    columns: &[(usize, u32)],
    height: usize,
    y: usize,
    between: &mut [u32; MASK_SIZE],
    row: &mut Vec<u8>,
) {
    let n = MASK_SIZE;
    let fy = ((y as f32 + 0.5) * n as f32 / height as f32 - 0.5).clamp(0.0, (n - 1) as f32);
    let y0 = (fy as usize).min(n - 2);
    let wy = ((fy - y0 as f32) * 256.0).round() as u32;
    let (r0, r1) = (&mask[y0 * n..][..n], &mask[(y0 + 1) * n..][..n]);
    for ((b, &top), &bottom) in between.iter_mut().zip(r0).zip(r1) {
        // 0..=255 * 256.
        *b = u32::from(top) * (256 - wy) + u32::from(bottom) * wy;
    }
    row.clear();
    row.extend(
        columns
            .iter()
            .map(|&(x0, wx)| ((between[x0] * (256 - wx) + between[x0 + 1] * wx) >> 16) as u8),
    );
}

/// `out = background + (frame - background) * alpha` on one row: the
/// person's runs are left alone, the background's copied, only the edge
/// blended.
fn blend_row(out: &mut [u8], background: &[u8], alpha: impl Iterator<Item = u8>) {
    for ((o, &b), a) in out.iter_mut().zip(background).zip(alpha) {
        match a {
            255 => {}
            0 => *o = b,
            a => {
                // 0..=255 to 0..=256, so 255 keeps the frame exactly.
                let a = i32::from(a) + i32::from(a >> 7);
                let (f, b) = (i32::from(*o), i32::from(b));
                *o = (b + (((f - b) * a) >> 8)) as u8;
            }
        }
    }
}

/// A mask scaled to one frame size: an alpha per luma pixel, and per
/// chroma sample (every other value of the even luma rows).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Alphas {
    pub width: usize,
    pub height: usize,
    pub luma: Vec<u8>,
    pub chroma: Vec<u8>,
}

impl Alphas {
    /// `mask` (256x256) scaled bilinearly to `w` x `h`.
    pub fn of(mask: &[u8], w: usize, h: usize) -> Self {
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let mut out = Self {
            width: w,
            height: h,
            luma: Vec::with_capacity(w * h),
            chroma: Vec::with_capacity(cw * ch),
        };
        let columns = mask_columns(w);
        let mut between = [0u32; MASK_SIZE];
        let mut row = Vec::with_capacity(w);
        for y in 0..h {
            mask_row(mask, &columns, h, y, &mut between, &mut row);
            out.luma.extend_from_slice(&row);
            if y % 2 == 0 {
                // The last column of an odd width repeats its neighbour.
                out.chroma.extend((0..cw).map(|x| row[(2 * x).min(w - 1)]));
            }
        }
        out
    }
}

/// The person over `background` (same size as the frame), by `alphas`
/// (scaled to it).
pub fn composite(frame: &mut FrameMut<'_>, background: &Planes, alphas: &Alphas) {
    let (w, h) = (frame.width, frame.height);
    if background.width != w || background.height != h || alphas.width != w || alphas.height != h {
        return;
    }
    let (cw, ch) = background.chroma();
    for y in 0..h {
        blend_row(
            &mut frame.y[y * frame.stride_y..][..w],
            &background.y[y * w..][..w],
            alphas.luma[y * w..][..w].iter().copied(),
        );
    }
    for y in 0..ch {
        let alpha = &alphas.chroma[y * cw..][..cw];
        blend_row(
            &mut frame.u[y * frame.stride_u..][..cw],
            &background.u[y * cw..][..cw],
            alpha.iter().copied(),
        );
        blend_row(
            &mut frame.v[y * frame.stride_v..][..cw],
            &background.v[y * cw..][..cw],
            alpha.iter().copied(),
        );
    }
}

#[cfg(feature = "libwebrtc")]
extern "C" {
    /// libyuv's `ScalePlane` (box filter down, bilinear up).
    fn yap_scale_plane(
        src: *const u8,
        src_stride: i32,
        src_width: i32,
        src_height: i32,
        dst: *mut u8,
        dst_stride: i32,
        dst_width: i32,
        dst_height: i32,
    ) -> i32;
}

/// Scales one plane with libyuv (SIMD). `false` if it refused.
#[cfg(feature = "libwebrtc")]
fn scale_plane(
    src: &[u8],
    stride: usize,
    w: usize,
    h: usize,
    dst: &mut [u8],
    dw: usize,
    dh: usize,
) -> bool {
    let int = |n: usize| i32::try_from(n).unwrap_or(i32::MAX);
    if src.len() < (h - 1) * stride + w || dst.len() < dw * dh {
        return false;
    }
    // SAFETY: both planes were checked to hold every row the sizes and
    // strides read or write; libyuv keeps no pointer.
    unsafe {
        yap_scale_plane(
            src.as_ptr(),
            int(stride),
            int(w),
            int(h),
            dst.as_mut_ptr(),
            int(dw),
            int(dw),
            int(dh),
        ) == 0
    }
}

/// `src` (w x h, `stride`) shrunk `k` times by box average into `dst`.
fn shrink(
    src: &[u8],
    stride: usize,
    w: usize,
    h: usize,
    k: usize,
    dst: &mut Vec<u8>,
) -> (usize, usize) {
    let (sw, sh) = ((w / k).max(1), (h / k).max(1));
    dst.clear();
    dst.resize(sw * sh, 0);
    #[cfg(feature = "libwebrtc")]
    if scale_plane(src, stride, w, h, dst, sw, sh) {
        return (sw, sh);
    }
    let area = (k * k) as u32;
    for y in 0..sh {
        for x in 0..sw {
            let mut sum = 0u32;
            for dy in 0..k {
                let row = &src[((y * k + dy).min(h - 1)) * stride..];
                for dx in 0..k {
                    sum += u32::from(row[(x * k + dx).min(w - 1)]);
                }
            }
            dst[y * sw + x] = (sum / area) as u8;
        }
    }
    (sw, sh)
}

/// One box pass of `radius` along rows, then along columns, edges clamped
/// (running sums; the column pass keeps a sum per column and walks rows,
/// so both passes read memory in order).
fn box_blur(plane: &mut [u8], w: usize, h: usize, radius: usize, tmp: &mut Vec<u8>) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    tmp.clear();
    tmp.resize(w * h, 0);
    let window = (2 * radius + 1) as u32;
    // Rows: plane -> tmp.
    for (src, dst) in plane.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
        let at = |i: isize| u32::from(src[i.clamp(0, w as isize - 1) as usize]);
        let r = radius as isize;
        let mut sum: u32 = (-r..=r).map(at).sum();
        for (i, out) in dst.iter_mut().enumerate() {
            *out = (sum / window) as u8;
            let i = i as isize;
            sum = sum + at(i + r + 1) - at(i - r);
        }
    }
    // Columns: tmp -> plane, one running sum per column.
    let row = |y: isize| &tmp[(y.clamp(0, h as isize - 1) as usize) * w..][..w];
    let r = radius as isize;
    let mut sums = vec![0u32; w];
    for y in -r..=r {
        for (s, &v) in sums.iter_mut().zip(row(y)) {
            *s += u32::from(v);
        }
    }
    for y in 0..h as isize {
        let out = &mut plane[y as usize * w..][..w];
        for (o, &s) in out.iter_mut().zip(&sums) {
            *o = (s / window) as u8;
        }
        let (add, sub) = (row(y + r + 1), row(y - r));
        for ((s, &a), &b) in sums.iter_mut().zip(add).zip(sub) {
            *s = *s + u32::from(a) - u32::from(b);
        }
    }
}

/// `src` (sw x sh) scaled up bilinearly to `w` x `h` into `dst`.
fn grow(src: &[u8], sw: usize, sh: usize, w: usize, h: usize, dst: &mut Vec<u8>) {
    dst.clear();
    dst.resize(w * h, 0);
    #[cfg(feature = "libwebrtc")]
    if scale_plane(src, sw, sw, sh, dst, w, h) {
        return;
    }
    for y in 0..h {
        let fy = ((y as f32 + 0.5) * sh as f32 / h as f32 - 0.5).clamp(0.0, (sh - 1) as f32);
        let y0 = fy as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let wy = fy - y0 as f32;
        for x in 0..w {
            let fx = ((x as f32 + 0.5) * sw as f32 / w as f32 - 0.5).clamp(0.0, (sw - 1) as f32);
            let x0 = fx as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let wx = fx - x0 as f32;
            let p = |yy: usize, xx: usize| f32::from(src[yy * sw + xx]);
            let top = p(y0, x0) + (p(y0, x1) - p(y0, x0)) * wx;
            let bottom = p(y1, x0) + (p(y1, x1) - p(y1, x0)) * wx;
            dst[y * w + x] = (top + (bottom - top) * wy).round() as u8;
        }
    }
}

/// The frame blurred by about `px` (a Gaussian's size at 720p) into `out`.
fn blurred(frame: &FrameMut<'_>, px: f32, small: &mut Planes, tmp: &mut Vec<u8>, out: &mut Planes) {
    let (w, h) = (frame.width, frame.height);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    // Three box passes of width b approximate a Gaussian of sigma
    // sqrt((b^2 - 1) / 4); the blur's size scales with the frame height.
    let sigma = px * h as f32 / MAX_EFFECT_HEIGHT as f32 / BLUR_SHRINK as f32;
    let width = (12.0 * sigma * sigma / 3.0 + 1.0).sqrt();
    let radius = ((width - 1.0) / 2.0).round().max(1.0) as usize;
    out.width = w;
    out.height = h;
    // Chroma is half size: half the radius.
    let chroma_radius = radius.div_ceil(2);
    let planes = [
        BlurPlane {
            src: frame.y,
            stride: frame.stride_y,
            size: (w, h),
            radius,
            shrunk: &mut small.y,
            grown: &mut out.y,
        },
        BlurPlane {
            src: frame.u,
            stride: frame.stride_u,
            size: (cw, ch),
            radius: chroma_radius,
            shrunk: &mut small.u,
            grown: &mut out.u,
        },
        BlurPlane {
            src: frame.v,
            stride: frame.stride_v,
            size: (cw, ch),
            radius: chroma_radius,
            shrunk: &mut small.v,
            grown: &mut out.v,
        },
    ];
    for plane in planes {
        let (pw, ph) = plane.size;
        let (sw, sh) = shrink(plane.src, plane.stride, pw, ph, BLUR_SHRINK, plane.shrunk);
        for _ in 0..3 {
            box_blur(plane.shrunk, sw, sh, plane.radius, tmp);
        }
        grow(plane.shrunk, sw, sh, pw, ph, plane.grown);
    }
}

/// One plane of a frame on its way through the blur.
struct BlurPlane<'a> {
    src: &'a [u8],
    stride: usize,
    size: (usize, usize),
    radius: usize,
    shrunk: &'a mut Vec<u8>,
    grown: &'a mut Vec<u8>,
}

/// `image` scaled to cover `w` x `h` (centred, cropped), as I420.
pub fn cover(image: &Rgba, w: usize, h: usize) -> Planes {
    let (iw, ih) = (image.width as usize, image.height as usize);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let mut out = Planes {
        width: w,
        height: h,
        y: vec![16; w * h],
        u: vec![128; cw * ch],
        v: vec![128; cw * ch],
    };
    if iw == 0 || ih == 0 || image.pixels.len() < iw * ih * 4 {
        return out;
    }
    let scale = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let ox = (iw as f32 - w as f32 / scale) / 2.0;
    let oy = (ih as f32 - h as f32 / scale) / 2.0;
    let sample = |x: usize, y: usize| {
        let sx = ((x as f32 + 0.5) / scale + ox).clamp(0.0, (iw - 1) as f32) as usize;
        let sy = ((y as f32 + 0.5) / scale + oy).clamp(0.0, (ih - 1) as f32) as usize;
        let p = &image.pixels[(sy * iw + sx) * 4..];
        (f32::from(p[0]), f32::from(p[1]), f32::from(p[2]))
    };
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = sample(x, y);
            out.y[y * w + x] = (16.0 + 0.257 * r + 0.504 * g + 0.098 * b).round() as u8;
        }
    }
    for y in 0..ch {
        for x in 0..cw {
            let (r, g, b) = sample((x * 2).min(w - 1), (y * 2).min(h - 1));
            out.u[y * cw + x] = (128.0 - 0.148 * r - 0.291 * g + 0.439 * b).round() as u8;
            out.v[y * cw + x] = (128.0 + 0.439 * r - 0.368 * g - 0.071 * b).round() as u8;
        }
    }
    out
}

/// The size an effect runs at for a `w` x `h` capture: as it is, or
/// scaled down (even sides) to fit 1280x720.
pub fn effect_size(w: u32, h: u32) -> (u32, u32) {
    if w <= MAX_EFFECT_WIDTH && h <= MAX_EFFECT_HEIGHT {
        return (w, h);
    }
    let scale = (f64::from(MAX_EFFECT_WIDTH) / f64::from(w))
        .min(f64::from(MAX_EFFECT_HEIGHT) / f64::from(h));
    let even = |n: f64| ((n.round() as u32) & !1).max(2);
    (even(f64::from(w) * scale), even(f64::from(h) * scale))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planes(w: usize, h: usize, luma: u8) -> Planes {
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        Planes {
            width: w,
            height: h,
            y: vec![luma; w * h],
            u: vec![128; cw * ch],
            v: vec![128; cw * ch],
        }
    }

    fn frame(p: &mut Planes) -> FrameMut<'_> {
        let (w, h) = (p.width, p.height);
        let cw = w.div_ceil(2);
        FrameMut {
            width: w,
            height: h,
            y: &mut p.y,
            u: &mut p.u,
            v: &mut p.v,
            stride_y: w,
            stride_u: cw,
            stride_v: cw,
        }
    }

    #[test]
    fn a_full_mask_should_keep_the_person() {
        let mut person = planes(64, 36, 200);
        composite(
            &mut frame(&mut person),
            &planes(64, 36, 20),
            &Alphas::of(&vec![255; MASK_SIZE * MASK_SIZE], 64, 36),
        );
        assert!(person.y.iter().all(|&v| v == 200));
    }

    #[test]
    fn an_empty_mask_should_show_the_background() {
        let mut person = planes(64, 36, 200);
        composite(
            &mut frame(&mut person),
            &planes(64, 36, 20),
            &Alphas::of(&vec![0; MASK_SIZE * MASK_SIZE], 64, 36),
        );
        assert!(person.y.iter().all(|&v| v == 20));
    }

    #[test]
    fn the_left_half_mask_should_keep_the_left_half() {
        let mut person = planes(64, 36, 200);
        let mask: Vec<u8> = (0..MASK_SIZE * MASK_SIZE)
            .map(|i| {
                if i % MASK_SIZE < MASK_SIZE / 2 {
                    255
                } else {
                    0
                }
            })
            .collect();
        composite(
            &mut frame(&mut person),
            &planes(64, 36, 20),
            &Alphas::of(&mask, 64, 36),
        );
        assert_eq!((person.y[10 * 64 + 2], person.y[10 * 64 + 61]), (200, 20));
    }

    #[test]
    fn confidence_below_the_edge_should_be_transparent() {
        let refined = refine(&vec![0.2; MASK_SIZE * MASK_SIZE]);
        assert!(refined.iter().all(|&a| a == 0));
    }

    #[test]
    fn confidence_above_the_edge_should_be_opaque() {
        let refined = refine(&vec![0.9; MASK_SIZE * MASK_SIZE]);
        assert!(refined.iter().all(|&a| a >= 254));
    }

    #[test]
    fn a_new_mask_should_move_part_of_the_way() {
        let next = smooth(Some(&[0.0]), &[1.0]);
        assert!((next[0] - MASK_EMA).abs() < 1e-6);
    }

    #[test]
    fn the_first_mask_should_be_taken_as_is() {
        assert_eq!(smooth(None, &[0.7]), vec![0.7]);
    }

    #[test]
    fn a_flat_frame_should_blur_to_itself() {
        let mut flat = planes(128, 72, 90);
        let (mut small, mut tmp, mut out) = (Planes::default(), Vec::new(), Planes::default());
        blurred(
            &frame(&mut flat),
            BLUR_STRONG_PX,
            &mut small,
            &mut tmp,
            &mut out,
        );
        assert!(out.y.iter().all(|&v| v == 90));
    }

    #[test]
    fn blur_should_soften_an_edge() {
        let mut edge = planes(128, 72, 0);
        for y in 0..72 {
            for x in 64..128 {
                edge.y[y * 128 + x] = 255;
            }
        }
        let (mut small, mut tmp, mut out) = (Planes::default(), Vec::new(), Planes::default());
        blurred(
            &frame(&mut edge),
            BLUR_STRONG_PX,
            &mut small,
            &mut tmp,
            &mut out,
        );
        let mid = out.y[36 * 128 + 64];
        assert!((40..215).contains(&mid), "edge value {mid}");
    }

    #[test]
    fn cover_should_crop_a_wide_image_to_the_frame() {
        // Left half red, right half blue, 4:1; covering 16:9 crops the sides
        // and keeps both colours either side of the centre.
        let (iw, ih) = (400u32, 100u32);
        let pixels = (0..iw * ih)
            .flat_map(|i| {
                if i % iw < iw / 2 {
                    [255, 0, 0, 255]
                } else {
                    [0, 0, 255, 255]
                }
            })
            .collect();
        let out = cover(
            &Rgba {
                width: iw,
                height: ih,
                pixels,
            },
            32,
            18,
        );
        let (cw, _) = out.chroma();
        assert!(out.v[4 * cw + 2] > 200, "red has high V");
        assert!(out.u[4 * cw + cw - 3] > 200, "blue has high U");
    }

    /// A segmenter that sees the same confidence everywhere.
    struct Flat(f32);

    impl Segmenter for Flat {
        fn segment(&mut self, _: &[f32]) -> Result<Vec<f32>, String> {
            Ok(vec![self.0; MASK_SIZE * MASK_SIZE])
        }

        fn backend(&self) -> String {
            "test".into()
        }
    }

    #[test]
    fn the_segmenter_should_load_only_when_a_frame_comes() {
        use std::sync::atomic::{AtomicBool, Ordering};
        static LOADED: AtomicBool = AtomicBool::new(false);
        let effect = Effect::start(
            Background::Blur { strong: true },
            Arc::new(|| {
                LOADED.store(true, Ordering::SeqCst);
                Ok(Box::new(Flat(1.0)) as Box<dyn Segmenter>)
            }),
        );
        std::thread::sleep(Duration::from_millis(50));
        assert!(!LOADED.load(Ordering::SeqCst));
        drop(effect);
    }

    /// A factory that takes `load` to load and counts its loads.
    fn slow_factory(
        load: Duration,
        loads: Arc<std::sync::atomic::AtomicUsize>,
    ) -> SegmenterFactory {
        Arc::new(move || {
            std::thread::sleep(load);
            loads.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(Box::new(Flat(1.0)) as Box<dyn Segmenter>)
        })
    }

    /// Feeds frames until `loads` reaches `n` (or 2 s pass).
    fn feed_until_loaded(effect: &Effect, loads: &std::sync::atomic::AtomicUsize, n: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while loads.load(std::sync::atomic::Ordering::SeqCst) < n && Instant::now() < deadline {
            let mut room = planes(64, 36, 200);
            effect.apply(&mut frame(&mut room));
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn dropping_an_effect_should_not_wait_for_a_loading_model() {
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let effect = Effect::start(
            Background::Blur { strong: true },
            slow_factory(Duration::from_secs(1), Arc::clone(&loads)),
        );
        let mut room = planes(64, 36, 200);
        effect.apply(&mut frame(&mut room));
        // The model is loading on the segmenter thread now.
        std::thread::sleep(Duration::from_millis(50));
        let started = Instant::now();
        drop(effect);
        assert!(
            started.elapsed() < Duration::from_millis(200),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn a_camera_stop_should_let_go_of_the_frame_buffers() {
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let effect = Effect::start(
            Background::Blur { strong: true },
            slow_factory(Duration::ZERO, Arc::clone(&loads)),
        );
        feed_until_loaded(&effect, &loads, 1);
        // A mask has come by now, so the scratch holds a frame's worth.
        let deadline = Instant::now() + Duration::from_secs(2);
        while lock(&effect.scratch).alphas.is_none() && Instant::now() < deadline {
            let mut room = planes(64, 36, 200);
            effect.apply(&mut frame(&mut room));
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(lock(&effect.scratch).alphas.is_some(), "no mask came");
        effect.release();
        let scratch = lock(&effect.scratch);
        assert!(scratch.alphas.is_none() && scratch.background.y.is_empty());
    }

    #[test]
    fn a_camera_stop_should_stop_the_segmenter_until_the_next_frame() {
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let effect = Effect::start(
            Background::Blur { strong: true },
            slow_factory(Duration::ZERO, Arc::clone(&loads)),
        );
        feed_until_loaded(&effect, &loads, 1);
        effect.release();
        assert!(lock(&effect.worker).is_none());
    }

    #[test]
    fn the_next_frame_after_a_stop_should_load_the_model_again() {
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let effect = Effect::start(
            Background::Blur { strong: true },
            slow_factory(Duration::ZERO, Arc::clone(&loads)),
        );
        feed_until_loaded(&effect, &loads, 1);
        effect.release();
        feed_until_loaded(&effect, &loads, 2);
        assert_eq!(loads.load(std::sync::atomic::Ordering::SeqCst), 2);
    }

    #[test]
    fn an_image_background_should_replace_the_room_once_masked() {
        let red = Rgba {
            width: 4,
            height: 4,
            pixels: [255, 0, 0, 255].repeat(16),
        };
        let effect = Effect::start(
            Background::Image(Arc::new(red)),
            Arc::new(|| Ok(Box::new(Flat(0.0)) as Box<dyn Segmenter>)),
        );
        let mut room = planes(64, 36, 200);
        let deadline = Instant::now() + Duration::from_secs(2);
        while room.y[0] == 200 && Instant::now() < deadline {
            room = planes(64, 36, 200);
            effect.apply(&mut frame(&mut room));
            std::thread::sleep(Duration::from_millis(20));
        }
        // Red is luma 82 in BT.601 limited range.
        assert!(room.y.iter().all(|&v| v == 82), "luma {}", room.y[0]);
    }

    #[test]
    fn large_captures_should_run_the_effect_at_720p() {
        assert_eq!(effect_size(1920, 1080), (1280, 720));
        assert_eq!(effect_size(640, 480), (640, 480));
    }

    #[test]
    fn model_input_should_be_planar_rgb_in_unit_range() {
        let mut white = planes(64, 36, 235);
        let input = model_input(&frame(&mut white));
        assert_eq!(input.len(), 3 * MASK_SIZE * MASK_SIZE);
        assert!(input.iter().all(|&v| (0.99..=1.0).contains(&v)));
    }
}
