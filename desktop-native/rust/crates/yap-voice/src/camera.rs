//! Our camera's frames on their way into the call.
//!
//! The app captures (Qt Multimedia, or the synthetic test pattern) and hands
//! every frame to [`push_camera_frame`] on whatever thread it arrives. While
//! a call's camera is live (on), the frame is converted once, by libyuv,
//! straight into the I420 buffer the encoder takes. While nothing is live,
//! a frame costs one atomic load and nothing else.
//!
//! The call's camera publication owns the [`Keepalive`]: while the camera is
//! off, its SFU track gets a 64x36 black frame each second
//! (`video-keepalive.ts`), so the SFU keeps the track and turning the camera
//! back on resumes it.

/// How a captured frame's bytes are laid out (memory order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Planar Y, U, V (YV12 comes as this with U and V swapped).
    I420,
    /// Y, then interleaved U/V.
    Nv12,
    /// Y, then interleaved V/U.
    Nv21,
    /// Packed Y0 U Y1 V.
    Yuy2,
    /// Packed U Y0 V Y1.
    Uyvy,
    /// Packed B, G, R, A (Qt's ARGB32 / BGRA8888 on little-endian).
    Bgra,
    /// Packed R, G, B, A.
    Rgba,
}

/// One captured frame, borrowed for the length of [`push_camera_frame`].
/// Unused planes are empty with stride 0.
#[derive(Debug, Clone, Copy)]
pub struct CameraFrame<'a> {
    pub format: PixelFormat,
    pub width: u32,
    pub height: u32,
    pub planes: [&'a [u8]; 3],
    pub strides: [u32; 3],
}

impl CameraFrame<'_> {
    /// Each plane holds the rows its format reads: `(rows - 1) * stride`
    /// plus one row, with strides wide enough for a row.
    pub fn is_valid(&self) -> bool {
        let (w, h) = (self.width as usize, self.height as usize);
        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return false;
        }
        let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
        let fits = |plane: usize, row: usize, rows: usize| {
            let stride = self.strides[plane] as usize;
            stride >= row && self.planes[plane].len() >= (rows - 1) * stride + row
        };
        match self.format {
            PixelFormat::I420 => fits(0, w, h) && fits(1, cw, ch) && fits(2, cw, ch),
            PixelFormat::Nv12 | PixelFormat::Nv21 => fits(0, w, h) && fits(1, cw * 2, ch),
            PixelFormat::Yuy2 | PixelFormat::Uyvy => fits(0, cw * 4, h),
            PixelFormat::Bgra | PixelFormat::Rgba => fits(0, w * 4, h),
        }
    }
}

/// Hands a captured frame to the call's camera while it is live. `true` if
/// it went to the encoder. Any thread; never blocks on the call.
pub fn push_camera_frame(frame: &CameraFrame<'_>) -> bool {
    #[cfg(feature = "libwebrtc")]
    {
        live::push(frame)
    }
    #[cfg(not(feature = "libwebrtc"))]
    {
        let _ = frame;
        false
    }
}

// The keepalive's numbers; only the libwebrtc engine sends it.

/// One keepalive frame: alternating near-blacks, so each one is a change
/// the encoder sends (the web paints `#000000` and `#010101`, which are
/// luma 16 and 17).
#[cfg_attr(not(feature = "libwebrtc"), allow(dead_code))]
pub(crate) const KEEPALIVE_SIZE: (u32, u32) = (64, 36);
#[cfg_attr(not(feature = "libwebrtc"), allow(dead_code))]
pub(crate) const KEEPALIVE_EVERY: std::time::Duration = std::time::Duration::from_secs(1);
/// How long a parked camera may go without a keepalive frame before its
/// SFU track counts as gone (`PUBLICATION_STALE_AFTER_MS`, under the SFU's
/// 30 s inactivity window).
#[cfg_attr(not(feature = "libwebrtc"), allow(dead_code))]
pub(crate) const STALE_AFTER: std::time::Duration = std::time::Duration::from_secs(25);

#[cfg_attr(not(feature = "libwebrtc"), allow(dead_code))]
pub(crate) fn keepalive_luma(odd: bool) -> u8 {
    if odd {
        16
    } else {
        17
    }
}

#[cfg(feature = "libwebrtc")]
pub(crate) mod live {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Arc, PoisonError, RwLock};
    use std::time::Instant;

    use libwebrtc::video_frame::{I420Buffer, VideoFrame, VideoRotation};
    use libwebrtc::video_source::native::NativeVideoSource;
    use tokio::task::JoinHandle;

    use super::{
        keepalive_luma, CameraFrame, PixelFormat, KEEPALIVE_EVERY, KEEPALIVE_SIZE, STALE_AFTER,
    };

    extern "C" {
        fn yap_camera_to_i420(
            format: i32,
            src0: *const u8,
            stride0: i32,
            src1: *const u8,
            stride1: i32,
            src2: *const u8,
            stride2: i32,
            width: i32,
            height: i32,
            dst_y: *mut u8,
            stride_y: i32,
            dst_u: *mut u8,
            stride_u: i32,
            dst_v: *mut u8,
            stride_v: i32,
        ) -> i32;
    }

    /// The live camera's source; `None` while no call camera is on.
    static SINK: RwLock<Option<NativeVideoSource>> = RwLock::new(None);
    /// A cheap check before the lock, for the common nothing-live case.
    static LIVE: AtomicBool = AtomicBool::new(false);

    pub(crate) fn set_sink(source: Option<NativeVideoSource>) {
        let mut sink = SINK.write().unwrap_or_else(PoisonError::into_inner);
        LIVE.store(source.is_some(), Ordering::Release);
        *sink = source;
    }

    fn code(format: PixelFormat) -> i32 {
        match format {
            PixelFormat::I420 => 0,
            PixelFormat::Nv12 => 1,
            PixelFormat::Nv21 => 2,
            PixelFormat::Yuy2 => 3,
            PixelFormat::Uyvy => 4,
            PixelFormat::Bgra => 5,
            PixelFormat::Rgba => 6,
        }
    }

    fn stride(n: u32) -> i32 {
        i32::try_from(n).unwrap_or(i32::MAX)
    }

    pub(crate) fn push(frame: &CameraFrame<'_>) -> bool {
        if !LIVE.load(Ordering::Acquire) || !frame.is_valid() {
            return false;
        }
        let Some(source) = SINK.read().unwrap_or_else(PoisonError::into_inner).clone() else {
            return false;
        };
        let mut buffer = I420Buffer::new(frame.width, frame.height);
        let (sy, su, sv) = buffer.strides();
        let (y, u, v) = buffer.data_mut();
        let [p0, p1, p2] = frame.planes;
        let [s0, s1, s2] = frame.strides;
        // SAFETY: `is_valid` checked every source plane against the rows
        // and strides this format reads; the destination planes are the
        // buffer's own, sized for `width` x `height`. libyuv reads and
        // writes nothing else and keeps no pointer.
        let converted = unsafe {
            yap_camera_to_i420(
                code(frame.format),
                p0.as_ptr(),
                stride(s0),
                p1.as_ptr(),
                stride(s1),
                p2.as_ptr(),
                stride(s2),
                stride(frame.width),
                stride(frame.height),
                y.as_mut_ptr(),
                stride(sy),
                u.as_mut_ptr(),
                stride(su),
                v.as_mut_ptr(),
                stride(sv),
            )
        };
        if converted != 0 {
            return false;
        }
        source.capture_frame(&VideoFrame {
            rotation: VideoRotation::VideoRotation0,
            timestamp_us: 0,
            frame_metadata: None,
            buffer,
        })
    }

    /// The black frames a parked camera sends, and how well they kept time.
    pub(crate) struct Keepalive {
        parked: Arc<AtomicBool>,
        /// The longest gap between two keepalive frames since it parked, in
        /// milliseconds.
        longest_gap_ms: Arc<AtomicU64>,
        /// When the last keepalive frame went out, in milliseconds since
        /// `epoch`: a stall that has not ended yet counts too.
        last_ms: Arc<AtomicU64>,
        epoch: Instant,
        task: JoinHandle<()>,
    }

    fn millis(since: Instant) -> u64 {
        u64::try_from(since.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    impl Keepalive {
        /// Starts parked: a fresh push sends black until the camera is live.
        pub(crate) fn start(source: NativeVideoSource) -> Self {
            let parked = Arc::new(AtomicBool::new(true));
            let longest_gap_ms = Arc::new(AtomicU64::new(0));
            let last_ms = Arc::new(AtomicU64::new(0));
            let epoch = Instant::now();
            let task = tokio::spawn(run(
                source,
                Arc::clone(&parked),
                Arc::clone(&longest_gap_ms),
                Arc::clone(&last_ms),
                epoch,
            ));
            Self {
                parked,
                longest_gap_ms,
                last_ms,
                epoch,
                task,
            }
        }

        pub(crate) fn park(&self, parked: bool) {
            if parked {
                self.longest_gap_ms.store(0, Ordering::Relaxed);
                self.last_ms.store(millis(self.epoch), Ordering::Relaxed);
            }
            self.parked.store(parked, Ordering::Relaxed);
        }

        /// Parked, and a gap long enough that the SFU may have dropped the
        /// track.
        pub(crate) fn stale(&self) -> bool {
            // As the web's `longestGapMs()`: the longest gap, or the one
            // still open since the last frame.
            let open = millis(self.epoch).saturating_sub(self.last_ms.load(Ordering::Relaxed));
            let gap = self.longest_gap_ms.load(Ordering::Relaxed).max(open);
            self.parked.load(Ordering::Relaxed) && u128::from(gap) >= STALE_AFTER.as_millis()
        }
    }

    impl Drop for Keepalive {
        fn drop(&mut self) {
            self.task.abort();
        }
    }

    async fn run(
        source: NativeVideoSource,
        parked: Arc<AtomicBool>,
        longest_gap_ms: Arc<AtomicU64>,
        last_ms: Arc<AtomicU64>,
        epoch: Instant,
    ) {
        let (w, h) = KEEPALIVE_SIZE;
        let mut ticks = tokio::time::interval(KEEPALIVE_EVERY);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut odd = false;
        loop {
            ticks.tick().await;
            let now = millis(epoch);
            let last = last_ms.swap(now, Ordering::Relaxed);
            if !parked.load(Ordering::Relaxed) {
                continue;
            }
            longest_gap_ms.fetch_max(now.saturating_sub(last), Ordering::Relaxed);
            odd = !odd;
            let mut buffer = I420Buffer::new(w, h);
            let (y, u, v) = buffer.data_mut();
            y.fill(keepalive_luma(odd));
            u.fill(128);
            v.fill(128);
            source.capture_frame(&VideoFrame {
                rotation: VideoRotation::VideoRotation0,
                timestamp_us: 0,
                frame_metadata: None,
                buffer,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(format: PixelFormat, len: usize, stride: u32) -> bool {
        let data = vec![0u8; len];
        CameraFrame {
            format,
            width: 4,
            height: 2,
            planes: [&data, &data, &data],
            strides: [stride, stride, stride],
        }
        .is_valid()
    }

    #[test]
    fn packed_frames_should_need_a_full_last_row() {
        assert!(frame(PixelFormat::Bgra, 16 + 16, 16));
        assert!(!frame(PixelFormat::Bgra, 16 + 15, 16));
    }

    #[test]
    fn strides_narrower_than_a_row_should_be_refused() {
        assert!(!frame(PixelFormat::Yuy2, 64, 7));
        assert!(frame(PixelFormat::Yuy2, 64, 8));
    }

    #[test]
    fn keepalive_frames_should_alternate_lumas() {
        assert_ne!(keepalive_luma(true), keepalive_luma(false));
    }
}
