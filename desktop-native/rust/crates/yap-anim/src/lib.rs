//! Animated pictures Qt cannot animate on its own: APNG (avatar decorations
//! and profile effects are APNG; Qt's PNG reader shows only the default
//! image) and animated WebP (X GIFs from `gif.fxtwitter.com`).
//!
//! [`decode`] returns every frame composited onto the full canvas, scaled
//! down to fit the box the caller draws (never up), premultiplied RGBA (the
//! layout of `QImage::Format_RGBA8888_Premultiplied`), with its delay. A
//! still PNG or WebP comes back as one frame, so callers need no second path.

use std::io::Cursor;

use image::codecs::png::PngDecoder;
use image::codecs::webp::WebPDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, Frame, ImageDecoder, Limits, RgbaImage};

/// Decoded frames above this many bytes are dropped (the tail of a very long
/// animation), so one odd file cannot take the process's memory.
pub const MAX_BYTES: usize = 48 * 1024 * 1024;
/// A picture declaring a canvas wider or taller than this is refused before
/// anything is allocated for it.
pub const MAX_SIDE: u32 = 4096;
/// ... or with more pixels than this (16 Mpx: 64 MB of RGBA for one frame).
pub const MAX_PIXELS: u64 = 16 * 1024 * 1024;
/// What the PNG decoder may hold at once (one canvas plus its work buffers).
const MAX_ALLOC: u64 = 256 * 1024 * 1024;
/// At most this many frames are kept.
pub const MAX_FRAMES: usize = 600;

/// Why a picture could not be decoded.
#[derive(Debug, thiserror::Error)]
pub enum AnimError {
    #[error("not a PNG or WebP picture")]
    Unsupported,
    #[error("decode: {0}")]
    Decode(#[from] image::ImageError),
    #[error("the picture has no frames")]
    Empty,
    #[error("the picture declares {0}x{1}, which is too large")]
    TooLarge(u32, u32),
}

/// One frame: `width * height * 4` premultiplied RGBA bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnimFrame {
    pub rgba: Vec<u8>,
    /// How long it shows, in milliseconds.
    pub delay_ms: u32,
}

/// A decoded picture, every frame the same size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    pub width: u32,
    pub height: u32,
    pub frames: Vec<AnimFrame>,
    /// How many times it plays; 0 is forever.
    pub loops: u32,
}

impl Animation {
    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// The decoded size in bytes.
    pub fn byte_size(&self) -> usize {
        self.frames.iter().map(|f| f.rgba.len()).sum()
    }
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}

fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
}

/// The size that fits `width x height` inside `max_w x max_h` without
/// growing it (a zero bound is no bound). Never below 1 px.
pub fn fit_size(width: u32, height: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if width == 0 || height == 0 {
        return (width, height);
    }
    let sx = if max_w == 0 {
        1.0
    } else {
        f64::from(max_w) / f64::from(width)
    };
    let sy = if max_h == 0 {
        1.0
    } else {
        f64::from(max_h) / f64::from(height)
    };
    let scale = sx.min(sy).min(1.0);
    // Sizes are at most u32 and scale <= 1, so the products fit.
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    (w, h)
}

/// The play count from APNG's `acTL` chunk (0 is forever).
fn apng_plays(bytes: &[u8]) -> u32 {
    chunk_data(bytes, b"acTL")
        .and_then(|data| data.get(4..8))
        .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

/// The loop count from WebP's `ANIM` chunk (0 is forever).
fn webp_loops(bytes: &[u8]) -> u32 {
    let Some(at) = find(bytes, b"ANIM") else {
        return 0;
    };
    bytes
        .get(at + 8 + 4..at + 8 + 6)
        .map_or(0, |b| u32::from(u16::from_le_bytes([b[0], b[1]])))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// A PNG chunk's data (walking the chunk list, not searching the bytes).
fn chunk_data<'a>(bytes: &'a [u8], kind: &[u8; 4]) -> Option<&'a [u8]> {
    let mut at = 8;
    while at + 8 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
        let name = &bytes[at + 4..at + 8];
        let data = bytes.get(at + 8..at + 8 + len)?;
        if name == kind {
            return Some(data);
        }
        if name == b"IDAT" {
            return None;
        }
        at += 12 + len;
    }
    None
}

/// Browsers show frames that ask for 10 ms or less for 100 ms (GIF and
/// WebP); a zero delay would spin.
fn delay_ms(frame: &Frame) -> u32 {
    let (num, den) = frame.delay().numer_denom_ms();
    let ms = num.checked_div(den).unwrap_or(0);
    if ms <= 10 {
        100
    } else {
        ms
    }
}

fn premultiply(image: &mut RgbaImage) {
    for pixel in image.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        if a == 255 {
            continue;
        }
        let mul = |c: u8| ((u16::from(c) * u16::from(a) + 127) / 255) as u8;
        pixel.0 = [mul(r), mul(g), mul(b), a];
    }
}

struct Collector {
    max_w: u32,
    max_h: u32,
    size: Option<(u32, u32)>,
    bytes: usize,
    frames: Vec<AnimFrame>,
}

impl Collector {
    fn new(max_w: u32, max_h: u32) -> Self {
        Self {
            max_w,
            max_h,
            size: None,
            bytes: 0,
            frames: Vec::new(),
        }
    }

    /// Adds a canvas; false once the budget is spent.
    fn push(&mut self, canvas: RgbaImage, delay_ms: u32) -> bool {
        let (w, h) = *self.size.get_or_insert_with(|| {
            fit_size(canvas.width(), canvas.height(), self.max_w, self.max_h)
        });
        let mut canvas = if (canvas.width(), canvas.height()) == (w, h) {
            canvas
        } else {
            image::imageops::resize(&canvas, w, h, FilterType::Triangle)
        };
        premultiply(&mut canvas);
        let rgba = canvas.into_raw();
        if self.bytes + rgba.len() > MAX_BYTES && !self.frames.is_empty() {
            return false;
        }
        self.bytes += rgba.len();
        self.frames.push(AnimFrame { rgba, delay_ms });
        self.frames.len() < MAX_FRAMES
    }

    fn finish(self, loops: u32) -> Result<Animation, AnimError> {
        let (width, height) = self.size.ok_or(AnimError::Empty)?;
        Ok(Animation {
            width,
            height,
            frames: self.frames,
            loops,
        })
    }
}

fn collect<'a>(
    frames: impl AnimationDecoder<'a>,
    mut out: Collector,
    loops: u32,
) -> Result<Animation, AnimError> {
    for frame in frames.into_frames() {
        let frame = frame?;
        let delay = delay_ms(&frame);
        if !out.push(frame.into_buffer(), delay) {
            break;
        }
    }
    out.finish(loops)
}

fn still(image: DynamicImage, mut out: Collector) -> Result<Animation, AnimError> {
    out.push(image.into_rgba8(), 0);
    out.finish(1)
}

/// Refuses a declared canvas that is too big, from the header alone.
fn check_size(width: u32, height: u32) -> Result<(), AnimError> {
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(AnimError::TooLarge(width, height));
    }
    Ok(())
}

/// Decodes a PNG/APNG or WebP (animated or not) to fit `max_w x max_h`
/// (0 for no bound). A picture whose header declares more than
/// [`MAX_SIDE`] a side or [`MAX_PIXELS`] is refused before decoding.
pub fn decode(bytes: &[u8], max_w: u32, max_h: u32) -> Result<Animation, AnimError> {
    let out = Collector::new(max_w, max_h);
    if is_png(bytes) {
        let mut limits = Limits::no_limits();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_ALLOC);
        let decoder = PngDecoder::with_limits(Cursor::new(bytes), limits)?;
        let (width, height) = decoder.dimensions();
        check_size(width, height)?;
        if decoder.is_apng()? {
            return collect(decoder.apng()?, out, apng_plays(bytes));
        }
        return still(DynamicImage::from_decoder(decoder)?, out);
    }
    if is_webp(bytes) {
        let decoder = WebPDecoder::new(Cursor::new(bytes))?;
        let (width, height) = decoder.dimensions();
        check_size(width, height)?;
        if decoder.has_animation() {
            return collect(decoder, out, webp_loops(bytes));
        }
        return still(DynamicImage::from_decoder(decoder)?, out);
    }
    Err(AnimError::Unsupported)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `frames`-frame APNG, `size` square, each frame one flat colour.
    fn apng(size: u32, frames: u32, plays: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, size, size);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.set_animated(frames, plays).expect("animated");
            encoder.set_frame_delay(1, 20).expect("delay");
            let mut writer = encoder.write_header().expect("header");
            for i in 0..frames {
                let shade = u8::try_from(i * 40).unwrap_or(255);
                let pixels: Vec<u8> = (0..size * size).flat_map(|_| [shade, 0, 0, 128]).collect();
                writer.write_image_data(&pixels).expect("frame");
            }
        }
        out
    }

    #[test]
    fn apng_should_decode_every_frame() {
        let anim = decode(&apng(8, 3, 0), 0, 0).expect("decode");
        assert_eq!(anim.frames.len(), 3);
    }

    #[test]
    fn apng_play_count_should_come_from_actl() {
        let anim = decode(&apng(8, 2, 1), 0, 0).expect("decode");
        assert_eq!(anim.loops, 1);
    }

    #[test]
    fn frames_should_scale_down_to_the_box() {
        let anim = decode(&apng(8, 2, 0), 4, 4).expect("decode");
        assert_eq!(
            (anim.width, anim.height, anim.frames[0].rgba.len()),
            (4, 4, 64)
        );
    }

    #[test]
    fn frames_should_never_scale_up() {
        assert_eq!(fit_size(10, 20, 100, 100), (10, 20));
    }

    #[test]
    fn fit_should_keep_the_aspect() {
        assert_eq!(fit_size(450, 880, 225, 0), (225, 440));
    }

    #[test]
    fn delays_should_come_through_in_ms() {
        let anim = decode(&apng(4, 2, 0), 0, 0).expect("decode");
        assert_eq!(anim.frames[1].delay_ms, 50);
    }

    #[test]
    fn pixels_should_be_premultiplied() {
        let anim = decode(&apng(2, 2, 0), 0, 0).expect("decode");
        // Frame 1 is (40, 0, 0, 128): 40 * 128 / 255 rounds to 20.
        assert_eq!(&anim.frames[1].rgba[0..4], &[20, 0, 0, 128]);
    }

    #[test]
    fn other_bytes_should_be_refused() {
        assert!(matches!(
            decode(b"GIF89a....", 0, 0),
            Err(AnimError::Unsupported)
        ));
    }

    /// The CRC-32 a PNG chunk carries.
    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = if crc & 1 == 1 {
                    (crc >> 1) ^ 0xEDB8_8320
                } else {
                    crc >> 1
                };
            }
        }
        !crc
    }

    fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(&u32::try_from(data.len()).expect("len").to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        out.extend_from_slice(&body);
        out.extend_from_slice(&crc32(&body).to_be_bytes());
    }

    /// An APNG whose header declares `side` square but carries no pixels.
    fn crafted_apng(side: u32) -> Vec<u8> {
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&side.to_be_bytes());
        ihdr.extend_from_slice(&side.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
        png_chunk(&mut out, b"IHDR", &ihdr);
        png_chunk(&mut out, b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        out
    }

    fn riff_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        out.extend_from_slice(kind);
        out.extend_from_slice(&u32::try_from(data.len()).expect("len").to_le_bytes());
        out.extend_from_slice(data);
    }

    /// An animated WebP whose VP8X header declares `side` square with an
    /// empty first frame.
    fn crafted_webp(side: u32) -> Vec<u8> {
        let canvas = (side - 1).to_le_bytes();
        let mut vp8x = vec![0x02, 0, 0, 0];
        vp8x.extend_from_slice(&canvas[0..3]);
        vp8x.extend_from_slice(&canvas[0..3]);
        let mut anmf = vec![0u8; 16];
        anmf.extend_from_slice(b"VP8L");
        anmf.extend_from_slice(&0u32.to_le_bytes());
        let mut body = b"WEBP".to_vec();
        riff_chunk(&mut body, b"VP8X", &vp8x);
        riff_chunk(&mut body, b"ANIM", &[0; 6]);
        riff_chunk(&mut body, b"ANMF", &anmf);
        let mut out = Vec::new();
        riff_chunk(&mut out, b"RIFF", &body);
        out
    }

    #[test]
    fn a_20000_px_apng_should_be_refused_before_decoding() {
        let started = std::time::Instant::now();
        let result = decode(&crafted_apng(20_000), 0, 0);
        assert!(result.is_err(), "{result:?}");
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn a_20000_px_webp_should_be_refused_before_decoding() {
        let started = std::time::Instant::now();
        let result = decode(&crafted_webp(20_000), 0, 0);
        assert!(matches!(result, Err(AnimError::TooLarge(20_000, 20_000))));
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn the_size_check_should_cap_side_and_pixels() {
        // 4096 x 4096 is exactly 16 Mpx: over the limit would be one more.
        assert!(check_size(4096, 4096).is_ok());
        assert!(check_size(4097, 8).is_err());
        assert!(check_size(4096, 4096 + 1).is_err());
    }

    #[test]
    fn a_wide_but_few_pixel_canvas_is_fine() {
        assert!(check_size(4096, 1000).is_ok());
    }

    #[test]
    fn a_still_png_should_be_one_frame() {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().expect("header");
            writer.write_image_data(&[255; 16]).expect("data");
        }
        let anim = decode(&out, 0, 0).expect("decode");
        assert!(!anim.is_animated());
    }
}
