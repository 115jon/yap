//! libvpx's VP8/VP9 decoders and the libyuv calls the player needs, from
//! the copies inside LiveKit's `webrtc.lib` (`win-x64-release-webrtc-a545de4`
//! exports `vpx_codec_*`, `vpx_codec_vp8_dx`, `vpx_codec_vp9_dx`,
//! `I420Scale`, `ScalePlane` and `I420AlphaToARGB` with C linkage). The
//! structs mirror `vpx/vpx_codec.h`, `vpx_decoder.h` and `vpx_image.h` of
//! that libvpx (decoder ABI 12).

use std::ffi::{c_char, c_int, c_long, c_uint, c_void, CStr};
use std::ptr;

use crate::demux::Codec;
use crate::player::within_pixel_limit;
use crate::WebmError;

/// `VPX_DECODER_ABI_VERSION`: 3 + `VPX_CODEC_ABI_VERSION` (4 + image ABI 5).
const DECODER_ABI_VERSION: c_int = 12;
/// `VPX_IMG_FMT_PLANAR | 2`, `| 5`, `| 6`, `| 7`: the 8-bit layouts
/// (VP9 profile 0 is 4:2:0; profile 1 adds 4:2:2, 4:4:4 and 4:4:0).
const IMG_FMT_I420: c_int = 0x102;
const IMG_FMT_I422: c_int = 0x105;
const IMG_FMT_I444: c_int = 0x106;
const IMG_FMT_I440: c_int = 0x107;
/// `VPX_CS_SRGB`: the planes are G, B, R.
const CS_SRGB: c_int = 7;
/// libyuv `kFilterBox`.
const FILTER_BOX: c_int = 3;

#[repr(C)]
struct CodecCtx {
    name: *const c_char,
    iface: *const c_void,
    err: c_int,
    err_detail: *const c_char,
    init_flags: c_long,
    config: *const c_void,
    private: *mut c_void,
}

#[repr(C)]
struct DecCfg {
    threads: c_uint,
    w: c_uint,
    h: c_uint,
}

/// `vpx_codec_stream_info_t`.
#[repr(C)]
struct StreamInfo {
    sz: c_uint,
    w: c_uint,
    h: c_uint,
    is_kf: c_uint,
}

#[repr(C)]
pub(crate) struct Image {
    fmt: c_int,
    cs: c_int,
    range: c_int,
    w: c_uint,
    h: c_uint,
    bit_depth: c_uint,
    d_w: c_uint,
    d_h: c_uint,
    r_w: c_uint,
    r_h: c_uint,
    x_chroma_shift: c_uint,
    y_chroma_shift: c_uint,
    planes: [*mut u8; 4],
    stride: [c_int; 4],
    bps: c_int,
    user_priv: *mut c_void,
    img_data: *mut u8,
    img_data_owner: c_int,
    self_allocd: c_int,
    fb_priv: *mut c_void,
}

extern "C" {
    fn vpx_codec_vp8_dx() -> *const c_void;
    fn vpx_codec_vp9_dx() -> *const c_void;
    fn vpx_codec_dec_init_ver(
        ctx: *mut CodecCtx,
        iface: *const c_void,
        cfg: *const DecCfg,
        flags: c_long,
        ver: c_int,
    ) -> c_int;
    fn vpx_codec_decode(
        ctx: *mut CodecCtx,
        data: *const u8,
        data_sz: c_uint,
        user_priv: *mut c_void,
        deadline: c_long,
    ) -> c_int;
    fn vpx_codec_peek_stream_info(
        iface: *const c_void,
        data: *const u8,
        data_sz: c_uint,
        info: *mut StreamInfo,
    ) -> c_int;
    fn vpx_codec_get_frame(ctx: *mut CodecCtx, iter: *mut *const c_void) -> *mut Image;
    fn vpx_codec_destroy(ctx: *mut CodecCtx) -> c_int;
    fn vpx_codec_error(ctx: *const CodecCtx) -> *const c_char;

    #[allow(clippy::too_many_arguments)]
    fn I420Scale(
        src_y: *const u8,
        src_stride_y: c_int,
        src_u: *const u8,
        src_stride_u: c_int,
        src_v: *const u8,
        src_stride_v: c_int,
        src_width: c_int,
        src_height: c_int,
        dst_y: *mut u8,
        dst_stride_y: c_int,
        dst_u: *mut u8,
        dst_stride_u: c_int,
        dst_v: *mut u8,
        dst_stride_v: c_int,
        dst_width: c_int,
        dst_height: c_int,
        filtering: c_int,
    ) -> c_int;
    fn ARGBScale(
        src_argb: *const u8,
        src_stride_argb: c_int,
        src_width: c_int,
        src_height: c_int,
        dst_argb: *mut u8,
        dst_stride_argb: c_int,
        dst_width: c_int,
        dst_height: c_int,
        filtering: c_int,
    ) -> c_int;
    fn ScalePlane(
        src: *const u8,
        src_stride: c_int,
        src_width: c_int,
        src_height: c_int,
        dst: *mut u8,
        dst_stride: c_int,
        dst_width: c_int,
        dst_height: c_int,
        filtering: c_int,
    ) -> c_int;
    #[allow(clippy::too_many_arguments)]
    fn I420AlphaToARGB(
        src_y: *const u8,
        src_stride_y: c_int,
        src_u: *const u8,
        src_stride_u: c_int,
        src_v: *const u8,
        src_stride_v: c_int,
        src_a: *const u8,
        src_stride_a: c_int,
        dst_argb: *mut u8,
        dst_stride_argb: c_int,
        width: c_int,
        height: c_int,
        attenuate: c_int,
    ) -> c_int;
}

/// One decoder instance.
pub(crate) struct Decoder {
    ctx: Box<CodecCtx>,
    /// The codec's interface table (static), for `peek_stream_info`.
    iface: *const c_void,
    /// A picture came out in a layout this player cannot show (high bit
    /// depth).
    pub(crate) unsupported: bool,
    /// A stream declared (or produced) a picture over the pixel limit.
    pub(crate) oversized: bool,
}

// SAFETY: a libvpx context has no thread affinity; the player uses it from
// one thread at a time.
unsafe impl Send for Decoder {}

impl Decoder {
    pub(crate) fn new(codec: Codec, threads: u32) -> Result<Self, WebmError> {
        let mut ctx = Box::new(CodecCtx {
            name: ptr::null(),
            iface: ptr::null(),
            err: 0,
            err_detail: ptr::null(),
            init_flags: 0,
            config: ptr::null(),
            private: ptr::null_mut(),
        });
        let cfg = DecCfg {
            threads: threads.max(1),
            w: 0,
            h: 0,
        };
        // SAFETY: the interface getters return static tables; `ctx` and
        // `cfg` are valid for the call (libvpx copies the config).
        let (iface, status) = unsafe {
            let iface = match codec {
                Codec::Vp8 => vpx_codec_vp8_dx(),
                Codec::Vp9 => vpx_codec_vp9_dx(),
            };
            (
                iface,
                vpx_codec_dec_init_ver(&mut *ctx, iface, &cfg, 0, DECODER_ABI_VERSION),
            )
        };
        if status != 0 {
            return Err(WebmError::Decode(format!("init failed ({status})")));
        }
        Ok(Self {
            ctx,
            iface,
            unsupported: false,
            oversized: false,
        })
    }

    fn error(&self) -> String {
        // SAFETY: a live context; libvpx returns a static string.
        unsafe {
            let text = vpx_codec_error(&*self.ctx);
            if text.is_null() {
                "unknown error".into()
            } else {
                CStr::from_ptr(text).to_string_lossy().into_owned()
            }
        }
    }

    /// The picture size a keyframe's header declares; `None` for any other
    /// packet, or one libvpx cannot read (decoding says why).
    fn keyframe_size(&self, data: &[u8], len: c_uint) -> Option<(u32, u32)> {
        let mut info = StreamInfo {
            sz: std::mem::size_of::<StreamInfo>() as c_uint,
            w: 0,
            h: 0,
            is_kf: 0,
        };
        // SAFETY: `data` is valid for `len` bytes; `info` is a live
        // `vpx_codec_stream_info_t` with its `sz` set; `iface` is static.
        let status =
            unsafe { vpx_codec_peek_stream_info(self.iface, data.as_ptr(), len, &mut info) };
        (status == 0 && info.is_kf != 0).then_some((info.w, info.h))
    }

    /// Decodes one packet; [`Decoder::frame`] then gives its picture, if it
    /// shows one. A keyframe that declares a picture over the pixel limit
    /// is refused before libvpx allocates for it.
    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<(), WebmError> {
        let len = c_uint::try_from(data.len())
            .map_err(|_| WebmError::Decode("packet too large".into()))?;
        if let Some((w, h)) = self.keyframe_size(data, len) {
            if !within_pixel_limit(w, h) {
                self.oversized = true;
                return Err(WebmError::Decode("picture too large".into()));
            }
        }
        // SAFETY: `data` is valid for `len` bytes for the call.
        let status =
            unsafe { vpx_codec_decode(&mut *self.ctx, data.as_ptr(), len, ptr::null_mut(), 0) };
        if status != 0 {
            return Err(WebmError::Decode(self.error()));
        }
        Ok(())
    }

    /// The picture of the last decoded packet, valid until the next decode.
    pub(crate) fn frame(&mut self) -> Option<Planes<'_>> {
        let mut iter: *const c_void = ptr::null();
        // SAFETY: a live context; the image belongs to it until the next
        // decode call, which needs `&mut self`.
        let image = unsafe { vpx_codec_get_frame(&mut *self.ctx, &mut iter).as_ref()? };
        if !matches!(
            image.fmt,
            IMG_FMT_I420 | IMG_FMT_I422 | IMG_FMT_I444 | IMG_FMT_I440
        ) {
            self.unsupported = true;
        }
        let layout_ok = matches!(
            image.fmt,
            IMG_FMT_I420 | IMG_FMT_I422 | IMG_FMT_I444 | IMG_FMT_I440
        );
        if !layout_ok || image.d_w == 0 || image.d_h == 0 || image.y_chroma_shift > 1 {
            return None;
        }
        // Before any buffer is built for it (an inter frame can resize).
        if !within_pixel_limit(image.d_w, image.d_h) {
            self.oversized = true;
            return None;
        }
        let width = image.d_w;
        let height = image.d_h;
        let stride = |i: usize| u32::try_from(image.stride[i]).unwrap_or(0);
        let rows_y = height as usize;
        let rows_c = ((height + image.y_chroma_shift) >> image.y_chroma_shift) as usize;
        let plane = |i: usize, rows: usize| -> Option<&[u8]> {
            let s = stride(i) as usize;
            if image.planes[i].is_null() || s == 0 {
                return None;
            }
            // SAFETY: libvpx allocates `stride * rows` bytes per plane of the
            // displayed size.
            Some(unsafe { std::slice::from_raw_parts(image.planes[i], s * rows) })
        };
        Some(Planes {
            width,
            height,
            y: plane(0, rows_y)?,
            u: plane(1, rows_c)?,
            v: plane(2, rows_c)?,
            stride_y: stride(0),
            stride_u: stride(1),
            stride_v: stride(2),
            x_shift: image.x_chroma_shift,
            y_shift: image.y_chroma_shift,
            srgb: image.cs == CS_SRGB,
        })
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        // SAFETY: initialised in `new`, destroyed once.
        unsafe {
            vpx_codec_destroy(&mut *self.ctx);
        }
    }
}

/// A decoded I420 picture.
#[derive(Clone, Copy)]
pub(crate) struct Planes<'a> {
    pub width: u32,
    pub height: u32,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub stride_y: u32,
    pub stride_u: u32,
    pub stride_v: u32,
    /// Chroma subsampling (1, 1 is 4:2:0).
    pub x_shift: u32,
    pub y_shift: u32,
    /// G, B, R planes (VP9 RGB), not Y, U, V.
    pub srgb: bool,
}

impl Planes<'_> {
    pub(crate) fn is_420(&self) -> bool {
        self.x_shift == 1 && self.y_shift == 1 && !self.srgb
    }

    fn chroma_size(&self) -> (u32, u32) {
        (
            (self.width + self.x_shift) >> self.x_shift,
            (self.height + self.y_shift) >> self.y_shift,
        )
    }
}

/// An owned I420 picture (scaled copies).
#[derive(Default)]
pub(crate) struct OwnedI420 {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

impl OwnedI420 {
    pub(crate) fn planes(&self) -> Planes<'_> {
        Planes {
            width: self.width,
            height: self.height,
            y: &self.y,
            u: &self.u,
            v: &self.v,
            stride_y: self.width,
            stride_u: self.width.div_ceil(2),
            stride_v: self.width.div_ceil(2),
            x_shift: 1,
            y_shift: 1,
            srgb: false,
        }
    }
}

/// 4:2:2, 4:4:4 or 4:4:0 YUV to I420 at the same size: the chroma planes
/// are scaled to half width and height.
pub(crate) fn to_i420(src: Planes<'_>, dst: &mut OwnedI420) {
    let (w, h) = (src.width, src.height);
    let (cw, ch) = (w.div_ceil(2), h.div_ceil(2));
    let (sw, sh) = src.chroma_size();
    dst.width = w;
    dst.height = h;
    dst.y.clear();
    for row in 0..h as usize {
        let at = row * src.stride_y as usize;
        dst.y.extend_from_slice(&src.y[at..at + w as usize]);
    }
    scale_plane(src.u, src.stride_u, sw, sh, cw, ch, &mut dst.u);
    scale_plane(src.v, src.stride_v, sw, sh, cw, ch, &mut dst.v);
}

/// VP9 RGB (planes G, B, R, any subsampling taken as 4:4:4) to BGRA at
/// the picture's size, opaque.
pub(crate) fn gbr_to_bgra(src: Planes<'_>, dst: &mut Vec<u8>) {
    let (w, h) = (src.width as usize, src.height as usize);
    dst.clear();
    dst.reserve(w * h * 4);
    let (xs, ys) = (src.x_shift as usize, src.y_shift as usize);
    for row in 0..h {
        let g = &src.y[row * src.stride_y as usize..];
        let b = &src.u[(row >> ys) * src.stride_u as usize..];
        let r = &src.v[(row >> ys) * src.stride_v as usize..];
        for col in 0..w {
            dst.extend_from_slice(&[b[col >> xs], g[col], r[col >> xs], 255]);
        }
    }
}

/// Scales a BGRA picture (box filter).
pub(crate) fn scale_bgra(
    src: &[u8],
    src_w: u32,
    src_h: u32,
    width: u32,
    height: u32,
    dst: &mut Vec<u8>,
) {
    dst.resize((width * height * 4) as usize, 0);
    // SAFETY: `src` holds `src_w * src_h * 4` bytes; `dst` was just sized.
    unsafe {
        ARGBScale(
            src.as_ptr(),
            int(src_w * 4),
            int(src_w),
            int(src_h),
            dst.as_mut_ptr(),
            int(width * 4),
            int(width),
            int(height),
            FILTER_BOX,
        );
    }
}

fn int(v: u32) -> c_int {
    c_int::try_from(v).unwrap_or(c_int::MAX)
}

/// Scales `src` into `dst` at `width x height` (box filter).
pub(crate) fn scale_i420(src: Planes<'_>, width: u32, height: u32, dst: &mut OwnedI420) {
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    dst.width = width;
    dst.height = height;
    dst.y.resize((width * height) as usize, 0);
    dst.u.resize((cw * ch) as usize, 128);
    dst.v.resize((cw * ch) as usize, 128);
    // SAFETY: every plane is valid for its stride and rows (checked when
    // `src` was made; `dst` was just sized).
    unsafe {
        I420Scale(
            src.y.as_ptr(),
            int(src.stride_y),
            src.u.as_ptr(),
            int(src.stride_u),
            src.v.as_ptr(),
            int(src.stride_v),
            int(src.width),
            int(src.height),
            dst.y.as_mut_ptr(),
            int(width),
            dst.u.as_mut_ptr(),
            int(cw),
            dst.v.as_mut_ptr(),
            int(cw),
            int(width),
            int(height),
            FILTER_BOX,
        );
    }
}

/// Scales one plane (an alpha frame's luma) into `dst`.
pub(crate) fn scale_plane(
    src: &[u8],
    stride: u32,
    src_w: u32,
    src_h: u32,
    width: u32,
    height: u32,
    dst: &mut Vec<u8>,
) {
    dst.resize((width * height) as usize, 255);
    // SAFETY: `src` holds `stride * src_h` bytes; `dst` was just sized.
    unsafe {
        ScalePlane(
            src.as_ptr(),
            int(stride),
            int(src_w),
            int(src_h),
            dst.as_mut_ptr(),
            int(width),
            int(width),
            int(height),
            FILTER_BOX,
        );
    }
}

/// I420 plus an alpha plane to premultiplied BGRA (libyuv's "ARGB" is
/// B, G, R, A in memory), `width * 4` bytes a row.
pub(crate) fn alpha_to_bgra(src: Planes<'_>, alpha: &[u8], alpha_stride: u32, dst: &mut Vec<u8>) {
    dst.resize((src.width * src.height * 4) as usize, 0);
    // SAFETY: the planes and `alpha` cover `width x height`; `dst` was
    // just sized.
    unsafe {
        I420AlphaToARGB(
            src.y.as_ptr(),
            int(src.stride_y),
            src.u.as_ptr(),
            int(src.stride_u),
            src.v.as_ptr(),
            int(src.stride_v),
            alpha.as_ptr(),
            int(alpha_stride),
            dst.as_mut_ptr(),
            int(src.width * 4),
            int(src.width),
            int(src.height),
            1,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A VP8 keyframe's first ten bytes: frame tag (key, shown), start
    /// code, then width and height.
    fn vp8_keyframe(width: u16, height: u16) -> Vec<u8> {
        let mut data = vec![0x10, 0x00, 0x00, 0x9d, 0x01, 0x2a];
        data.extend(width.to_le_bytes());
        data.extend(height.to_le_bytes());
        data
    }

    #[test]
    fn a_keyframe_declaring_a_huge_picture_should_be_refused() {
        let mut decoder = Decoder::new(Codec::Vp8, 1).expect("decoder");
        let result = decoder.decode(&vp8_keyframe(16383, 16383));
        assert_eq!(
            (result, decoder.oversized),
            (Err(WebmError::Decode("picture too large".into())), true)
        );
    }

    #[test]
    fn a_keyframe_declaring_a_small_picture_should_reach_the_decoder() {
        let mut decoder = Decoder::new(Codec::Vp8, 1).expect("decoder");
        // The rest of the frame is missing, so libvpx fails it, but not
        // as too large.
        let result = decoder.decode(&vp8_keyframe(64, 48));
        assert!(
            !decoder.oversized
                && !matches!(result, Err(WebmError::Decode(ref t)) if t == "picture too large")
        );
    }

    #[test]
    fn a_packet_that_is_not_a_keyframe_should_not_be_measured() {
        let decoder = Decoder::new(Codec::Vp8, 1).expect("decoder");
        // Frame tag bit 0 set: an inter frame.
        let mut data = vp8_keyframe(16383, 16383);
        data[0] |= 1;
        assert_eq!(decoder.keyframe_size(&data, 10), None);
    }
}
