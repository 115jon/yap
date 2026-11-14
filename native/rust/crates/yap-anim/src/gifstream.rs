//! Streamed GIFs (chat GIFs, the GIF picker, pins): decoded one frame at a
//! time, as a browser plays them, so only the file and one canvas are held.
//!
//! [`scan`] reads a file's size, frame count, play count and delays without
//! decoding any pixels. A [`GifStream`] then composites frame after frame
//! onto its canvas, the way browsers do:
//!
//! - the canvas starts transparent;
//! - each frame's transparent index leaves the pixel under it;
//! - a frame's local palette replaces the global one for that frame;
//! - interlaced frames come out in display order (the `gif` crate's job);
//! - a frame's disposal applies before the next frame: none/keep leaves it,
//!   "background" clears its rectangle to transparent (browsers ignore the
//!   background colour), "previous" puts back what was under it;
//! - parts of a frame outside the logical screen are clipped. A screen that
//!   declares 0 wide or high takes the first frame's extent (as Qt does).
//!
//! Frames come out premultiplied RGBA (`QImage::Format_RGBA8888_Premultiplied`),
//! scaled down to the size the caller asks for.

use std::io::Cursor;
use std::sync::Arc;

use gif::{ColorOutput, DecodeOptions, Decoder, DisposalMethod, MemoryLimit, Repeat};
use image::imageops::FilterType;
use image::{ImageBuffer, Rgba};

use crate::{check_size, AnimError};

/// How much one frame's indices may take while it decodes.
const FRAME_LIMIT: u64 = 64 * 1024 * 1024;

/// Browsers show frames that ask for 10 ms or less for 100 ms; a zero delay
/// would spin. GIF delays are in hundredths of a second.
pub fn delay_ms(centiseconds: u16) -> u32 {
    let ms = u32::from(centiseconds) * 10;
    if ms <= 10 {
        100
    } else {
        ms
    }
}

/// What [`scan`] reads from a GIF's blocks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GifInfo {
    /// The canvas: the logical screen, or the first frame's extent when the
    /// screen declares no size.
    pub width: u32,
    pub height: u32,
    /// Each frame's delay in ms, clamped as browsers clamp it.
    pub delays: Vec<u32>,
    /// How many times it plays; 0 is forever. No NETSCAPE block plays once;
    /// a count of n repeats n times after the first pass.
    pub loops: u32,
}

impl GifInfo {
    pub fn frames(&self) -> usize {
        self.delays.len()
    }

    /// One pass in ms.
    pub fn duration_ms(&self) -> u64 {
        self.delays.iter().map(|&d| u64::from(d)).sum()
    }
}

pub fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}

fn options(skip_pixels: bool) -> DecodeOptions {
    let mut options = DecodeOptions::new();
    options.set_color_output(ColorOutput::Indexed);
    options.set_memory_limit(MemoryLimit::Bytes(
        std::num::NonZeroU64::new(FRAME_LIMIT).expect("non-zero"),
    ));
    options.skip_frame_decoding(skip_pixels);
    options.allow_unknown_blocks(true);
    options
}

fn decode_err(error: gif::DecodingError) -> AnimError {
    AnimError::Gif(error.to_string())
}

/// The canvas a decoder's screen gives, with its first frame's extent for a
/// side the screen leaves at 0.
fn canvas_size(screen: (u16, u16), first: Option<(u16, u16, u16, u16)>) -> (u32, u32) {
    let (mut w, mut h) = (u32::from(screen.0), u32::from(screen.1));
    if let Some((left, top, fw, fh)) = first {
        if w == 0 {
            w = u32::from(left) + u32::from(fw);
        }
        if h == 0 {
            h = u32::from(top) + u32::from(fh);
        }
    }
    (w, h)
}

/// Reads a GIF's canvas, frames, delays and play count; no pixels are
/// decoded. A canvas over [`crate::MAX_SIDE`] a side or [`crate::MAX_PIXELS`]
/// is refused.
pub fn scan(bytes: &[u8]) -> Result<GifInfo, AnimError> {
    if !is_gif(bytes) {
        return Err(AnimError::Unsupported);
    }
    let mut decoder = options(true)
        .read_info(Cursor::new(bytes))
        .map_err(decode_err)?;
    let screen = (decoder.width(), decoder.height());
    if screen.0 > 0 && screen.1 > 0 {
        check_size(u32::from(screen.0), u32::from(screen.1))?;
    }
    let mut delays = Vec::new();
    let mut first = None;
    loop {
        match decoder.read_next_frame() {
            Ok(Some(frame)) => {
                // Refused here, the file goes to Qt's reader, which has its
                // own allocation limit.
                let (w, h) = (usize::from(frame.width), usize::from(frame.height));
                if !frame_fits(w, h) {
                    return Err(AnimError::TooLarge(
                        u32::from(frame.width),
                        u32::from(frame.height),
                    ));
                }
                if first.is_none() {
                    first = Some((frame.left, frame.top, frame.width, frame.height));
                }
                delays.push(delay_ms(frame.delay));
            }
            Ok(None) => break,
            // A file cut short plays the frames before the cut.
            Err(_) if !delays.is_empty() => break,
            Err(error) => return Err(decode_err(error)),
        }
    }
    if delays.is_empty() {
        return Err(AnimError::Empty);
    }
    let (width, height) = canvas_size(screen, first);
    if width == 0 || height == 0 {
        return Err(AnimError::Empty);
    }
    check_size(width, height)?;
    let loops = match decoder.repeat() {
        Repeat::Infinite => 0,
        Repeat::Finite(n) => u32::from(n) + 1,
    };
    Ok(GifInfo {
        width,
        height,
        delays,
        loops,
    })
}

/// A rectangle of the canvas, clipped to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

/// What the frame on the canvas leaves for the next one.
#[derive(Debug)]
enum Dispose {
    Keep,
    Clear(Rect),
    /// The rectangle's pixels from before the frame, row by row.
    Restore(Rect, Vec<u8>),
}

/// Plays one GIF frame by frame. Holds the file (shared), the decoder's
/// state, the canvas, and what the current frame's disposal needs.
pub struct GifStream {
    bytes: Arc<[u8]>,
    decoder: Option<Decoder<Cursor<Arc<[u8]>>>>,
    width: usize,
    height: usize,
    canvas: Vec<u8>,
    dispose: Dispose,
    /// The frame on the canvas (`None` before the first).
    current: Option<usize>,
    /// The frames the file turned out to hold, once it ended or broke.
    end: Option<usize>,
    /// The frame being drawn's indices (kept to be reused).
    indices: Vec<u8>,
    /// The file's global palette (empty: none).
    global: Vec<u8>,
}

/// One frame's descriptor and control block.
struct Meta {
    left: usize,
    top: usize,
    width: usize,
    height: usize,
    dispose: DisposalMethod,
    transparent: Option<u8>,
    interlaced: bool,
    palette: Option<Vec<u8>>,
}

/// Whether a frame of `width x height` is within the limit a canvas has
/// ([`crate::MAX_PIXELS`]).
fn frame_fits(width: usize, height: usize) -> bool {
    (width as u64) * (height as u64) <= crate::MAX_PIXELS
}

/// Interlaced rows (0, 8, 16, ...; 4, 12, ...; 2, 6, ...; 1, 3, ...) in
/// display order.
fn deinterlace(stored: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut out = vec![0; stored.len()];
    let rows = [(0, 8), (4, 8), (2, 4), (1, 2)]
        .into_iter()
        .flat_map(|(first, step)| (first..height).step_by(step));
    for (line, row) in stored.chunks_exact(width.max(1)).zip(rows) {
        out[row * width..(row + 1) * width].copy_from_slice(line);
    }
    out
}

impl GifStream {
    /// A stream over `bytes`, whose canvas `info` (from [`scan`]) describes.
    pub fn new(bytes: Arc<[u8]>, info: &GifInfo) -> Self {
        let width = info.width as usize;
        let height = info.height as usize;
        Self {
            bytes,
            decoder: None,
            width,
            height,
            canvas: Vec::new(),
            dispose: Dispose::Keep,
            current: None,
            end: None,
            indices: Vec::new(),
            global: Vec::new(),
        }
    }

    pub fn width(&self) -> u32 {
        self.width as u32
    }

    pub fn height(&self) -> u32 {
        self.height as u32
    }

    /// The frame on the canvas.
    pub fn current(&self) -> Option<usize> {
        self.current
    }

    /// The canvas: `width * height * 4` premultiplied RGBA bytes.
    pub fn canvas(&self) -> &[u8] {
        &self.canvas
    }

    /// Back before the first frame.
    fn rewind(&mut self) -> Result<(), AnimError> {
        self.decoder = None;
        self.current = None;
        self.dispose = Dispose::Keep;
        self.canvas.clear();
        self.canvas.resize(self.width * self.height * 4, 0);
        let decoder = options(false)
            .read_info(Cursor::new(Arc::clone(&self.bytes)))
            .map_err(decode_err)?;
        self.global = decoder
            .global_palette()
            .map(<[u8]>::to_vec)
            .unwrap_or_default();
        self.decoder = Some(decoder);
        Ok(())
    }

    /// Composites frame `index` onto the canvas, decoding forward from the
    /// frame shown (or from the start: frames build on the ones before).
    /// `Ok(false)` when the file holds fewer frames.
    pub fn seek(&mut self, index: usize) -> Result<bool, AnimError> {
        if self.current == Some(index) {
            return Ok(true);
        }
        if self.end.is_some_and(|end| index >= end) {
            return Ok(false);
        }
        if self.decoder.is_none() || self.current.is_none_or(|at| at > index) {
            self.rewind()?;
        }
        while self.current != Some(index) {
            if !self.step()? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// The next frame onto the canvas; false at the end of the file.
    fn step(&mut self) -> Result<bool, AnimError> {
        let next = self.current.map_or(0, |at| at + 1);
        match std::mem::replace(&mut self.dispose, Dispose::Keep) {
            Dispose::Keep => {}
            Dispose::Clear(rect) => self.fill(rect, None),
            Dispose::Restore(rect, saved) => self.fill(rect, Some(&saved)),
        }
        // Out of `self` while its frame is borrowed, so the canvas can be
        // written; put back below.
        let Some(mut decoder) = self.decoder.take() else {
            return Ok(false);
        };
        let drawn = self.draw(&mut decoder, next);
        self.decoder = Some(decoder);
        drawn
    }

    fn draw(
        &mut self,
        decoder: &mut Decoder<Cursor<Arc<[u8]>>>,
        next: usize,
    ) -> Result<bool, AnimError> {
        let frame = match decoder.next_frame_info() {
            Ok(Some(frame)) => Meta {
                left: usize::from(frame.left),
                top: usize::from(frame.top),
                width: usize::from(frame.width),
                height: usize::from(frame.height),
                dispose: frame.dispose,
                transparent: frame.transparent,
                interlaced: frame.interlaced,
                palette: frame.palette.clone(),
            },
            Ok(None) => {
                self.end = Some(next);
                return Ok(false);
            }
            Err(error) => return self.broken(next, decode_err(error)),
        };
        // The frame's indices in the order the file stores them. The crate
        // deinterlaces row by row, which stops short ("image truncated") on
        // some interlaced frames Qt, Pillow and browsers read whole (a
        // 33x37 one); reading the frame in one piece and putting the rows
        // in place here does not.
        let pixels = frame.width * frame.height;
        // A frame descriptor is up to 65535x65535 whatever the screen says,
        // and the crate checks no limit on this path: one larger than the
        // limit (16 Mpx, a canvas's) ends the animation before anything is
        // allocated for it.
        if !frame_fits(frame.width, frame.height) {
            return self.broken(
                next,
                AnimError::TooLarge(frame.width as u32, frame.height as u32),
            );
        }
        let mut indices = std::mem::take(&mut self.indices);
        indices.clear();
        indices.resize(pixels, 0);
        if pixels > 0 {
            match decoder.fill_buffer(&mut indices) {
                Ok(true) => {}
                // Cut short: the file ends before this frame.
                Ok(false) => {
                    self.indices = indices;
                    return self.broken(next, AnimError::Gif("image truncated".into()));
                }
                Err(error) => {
                    self.indices = indices;
                    return self.broken(next, decode_err(error));
                }
            }
        }
        if frame.interlaced {
            indices = deinterlace(&indices, frame.width, frame.height);
        }
        let palette: &[u8] = match frame.palette.as_deref() {
            Some(local) => local,
            None => &self.global,
        };
        let rect = Rect {
            x: frame.left.min(self.width),
            y: frame.top.min(self.height),
            w: (frame.left + frame.width)
                .min(self.width)
                .saturating_sub(frame.left),
            h: (frame.top + frame.height)
                .min(self.height)
                .saturating_sub(frame.top),
        };
        let dispose = match frame.dispose {
            DisposalMethod::Background => Dispose::Clear(rect),
            DisposalMethod::Previous => Dispose::Restore(rect, self.copy_rect(rect)),
            DisposalMethod::Any | DisposalMethod::Keep => Dispose::Keep,
        };
        // Each index's pixel, or none: the transparent index and an index
        // past the palette leave the pixel under them.
        let mut colours = [None::<[u8; 4]>; 256];
        for (index, rgb) in palette.chunks_exact(3).take(256).enumerate() {
            colours[index] = Some([rgb[0], rgb[1], rgb[2], 255]);
        }
        if let Some(index) = frame.transparent {
            colours[usize::from(index)] = None;
        }
        let stride = self.width * 4;
        if frame.width > 0 {
            for (row, line) in indices.chunks_exact(frame.width).take(rect.h).enumerate() {
                let start = (rect.y + row) * stride + rect.x * 4;
                let out = &mut self.canvas[start..start + rect.w * 4];
                for (pixel, &index) in out.chunks_exact_mut(4).zip(&line[..rect.w]) {
                    if let Some(colour) = colours[usize::from(index)] {
                        pixel.copy_from_slice(&colour);
                    }
                }
            }
        }
        self.indices = indices;
        self.dispose = dispose;
        self.current = Some(next);
        Ok(true)
    }

    /// Frame `next` cannot be read: the file ends there. The first frame is
    /// an error.
    fn broken(&mut self, next: usize, error: AnimError) -> Result<bool, AnimError> {
        self.end = Some(next);
        if next == 0 {
            return Err(error);
        }
        Ok(false)
    }

    fn copy_rect(&self, rect: Rect) -> Vec<u8> {
        let stride = self.width * 4;
        let mut out = Vec::with_capacity(rect.w * rect.h * 4);
        for row in rect.y..rect.y + rect.h {
            let start = row * stride + rect.x * 4;
            out.extend_from_slice(&self.canvas[start..start + rect.w * 4]);
        }
        out
    }

    /// Clears `rect` (no source) or puts `source` (from [`Self::copy_rect`])
    /// back into it.
    fn fill(&mut self, rect: Rect, source: Option<&[u8]>) {
        let stride = self.width * 4;
        let row_bytes = rect.w * 4;
        for row in 0..rect.h {
            let start = (rect.y + row) * stride + rect.x * 4;
            let out = &mut self.canvas[start..start + row_bytes];
            match source {
                Some(saved) => out.copy_from_slice(&saved[row * row_bytes..(row + 1) * row_bytes]),
                None => out.fill(0),
            }
        }
    }

    /// The canvas into `out` (`out_w x out_h`, `stride` bytes a row),
    /// scaled when the sizes differ.
    pub fn write(&self, out: &mut [u8], out_w: u32, out_h: u32, stride: usize) -> bool {
        let (ow, oh) = (out_w as usize, out_h as usize);
        if ow == 0 || oh == 0 || stride < ow * 4 || out.len() < stride * (oh - 1) + ow * 4 {
            return false;
        }
        if self.canvas.len() != self.width * self.height * 4 {
            return false;
        }
        if (ow, oh) == (self.width, self.height) {
            for (row, src) in self.canvas.chunks_exact(self.width * 4).enumerate() {
                out[row * stride..row * stride + ow * 4].copy_from_slice(src);
            }
            return true;
        }
        let Some(view) =
            ImageBuffer::<Rgba<u8>, &[u8]>::from_raw(self.width(), self.height(), &self.canvas)
        else {
            return false;
        };
        // Premultiplied in, premultiplied out: a linear filter is right on
        // premultiplied pixels.
        let scaled = image::imageops::resize(&view, out_w, out_h, FilterType::Triangle);
        for (row, src) in scaled.as_raw().chunks_exact(ow * 4).enumerate() {
            out[row * stride..row * stride + ow * 4].copy_from_slice(src);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::borrow::Cow;

    const RED: [u8; 4] = [255, 0, 0, 255];
    const GREEN: [u8; 4] = [0, 255, 0, 255];
    const BLUE: [u8; 4] = [0, 0, 255, 255];
    const CLEAR: [u8; 4] = [0, 0, 0, 0];
    /// Global palette: 0 red, 1 green, 2 blue, 3 the transparent slot.
    const PALETTE: [u8; 12] = [255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0];

    struct Spec {
        rect: (u16, u16, u16, u16),
        index: u8,
        dispose: DisposalMethod,
        delay: u16,
        transparent: Option<u8>,
        palette: Option<Vec<u8>>,
        interlaced: bool,
    }

    fn spec(rect: (u16, u16, u16, u16), index: u8, dispose: DisposalMethod) -> Spec {
        Spec {
            rect,
            index,
            dispose,
            delay: 5,
            transparent: Some(3),
            palette: None,
            interlaced: false,
        }
    }

    /// Rows in GIF interlace order (rows 0, 8, ...; 4, 12, ...; 2, 6, ...;
    /// 1, 3, ...).
    fn interlace(pixels: &[u8], w: usize, h: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(pixels.len());
        for (start, step) in [(0, 8), (4, 8), (2, 4), (1, 2)] {
            for row in (start..h).step_by(step) {
                out.extend_from_slice(&pixels[row * w..(row + 1) * w]);
            }
        }
        out
    }

    fn encode(size: (u16, u16), frames: &[Spec], repeat: Option<Repeat>) -> Vec<u8> {
        encode_pixels(size, frames, repeat, |s| {
            vec![s.index; usize::from(s.rect.2) * usize::from(s.rect.3)]
        })
    }

    fn encode_pixels(
        size: (u16, u16),
        frames: &[Spec],
        repeat: Option<Repeat>,
        pixels: impl Fn(&Spec) -> Vec<u8>,
    ) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut out, size.0, size.1, &PALETTE).expect("enc");
            if let Some(repeat) = repeat {
                encoder.set_repeat(repeat).expect("repeat");
            }
            for s in frames {
                let (x, y, w, h) = s.rect;
                let mut data = pixels(s);
                if s.interlaced {
                    data = interlace(&data, usize::from(w), usize::from(h));
                }
                let frame = gif::Frame {
                    left: x,
                    top: y,
                    width: w,
                    height: h,
                    delay: s.delay,
                    dispose: s.dispose,
                    transparent: s.transparent,
                    palette: s.palette.clone(),
                    interlaced: s.interlaced,
                    buffer: Cow::Owned(data),
                    ..gif::Frame::default()
                };
                encoder.write_frame(&frame).expect("frame");
            }
        }
        out
    }

    fn open(bytes: Vec<u8>) -> (GifInfo, GifStream) {
        let info = scan(&bytes).expect("scan");
        let stream = GifStream::new(Arc::from(bytes), &info);
        (info, stream)
    }

    fn pixel(stream: &GifStream, x: usize, y: usize) -> [u8; 4] {
        let at = (y * stream.width + x) * 4;
        stream.canvas()[at..at + 4].try_into().expect("pixel")
    }

    #[test]
    fn scan_should_count_frames_and_clamp_delays() {
        let mut frames = vec![
            spec((0, 0, 4, 4), 0, DisposalMethod::Keep),
            spec((0, 0, 4, 4), 1, DisposalMethod::Keep),
            spec((0, 0, 4, 4), 2, DisposalMethod::Keep),
        ];
        frames[0].delay = 0;
        frames[1].delay = 1;
        frames[2].delay = 2;
        let info = scan(&encode((4, 4), &frames, None)).expect("scan");
        assert_eq!(info.frames(), 3);
        // 0 and 10 ms are shown for 100 ms; 20 ms stays.
        assert_eq!(info.delays, vec![100, 100, 20]);
        assert_eq!(info.duration_ms(), 220);
    }

    #[test]
    fn loops_should_follow_the_netscape_block() {
        let one = [spec((0, 0, 2, 2), 0, DisposalMethod::Keep)];
        assert_eq!(scan(&encode((2, 2), &one, None)).expect("scan").loops, 1);
        let forever = encode((2, 2), &one, Some(Repeat::Infinite));
        assert_eq!(scan(&forever).expect("scan").loops, 0);
        let twice_more = encode((2, 2), &one, Some(Repeat::Finite(2)));
        assert_eq!(scan(&twice_more).expect("scan").loops, 3);
    }

    #[test]
    fn the_canvas_should_start_transparent_and_keep_what_is_under_transparency() {
        let frames = [
            spec((0, 0, 2, 2), 0, DisposalMethod::Keep),
            // Transparent index everywhere: frame 0 shows through.
            spec((0, 0, 2, 2), 3, DisposalMethod::Keep),
            spec((1, 1, 1, 1), 2, DisposalMethod::Keep),
        ];
        let (_, mut stream) = open(encode((4, 4), &frames, None));
        assert!(stream.seek(0).expect("seek"));
        assert_eq!(pixel(&stream, 0, 0), RED);
        assert_eq!(pixel(&stream, 3, 3), CLEAR);
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), RED);
        assert!(stream.seek(2).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), BLUE);
        assert_eq!(pixel(&stream, 0, 0), RED);
    }

    #[test]
    fn disposal_background_should_clear_the_frame_to_transparent() {
        let frames = [
            spec((0, 0, 4, 4), 0, DisposalMethod::Keep),
            spec((1, 1, 2, 2), 1, DisposalMethod::Background),
            spec((0, 0, 1, 1), 2, DisposalMethod::Keep),
        ];
        let (_, mut stream) = open(encode((4, 4), &frames, None));
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), GREEN);
        assert!(stream.seek(2).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), CLEAR);
        assert_eq!(pixel(&stream, 2, 2), CLEAR);
        assert_eq!(pixel(&stream, 3, 3), RED);
        assert_eq!(pixel(&stream, 0, 0), BLUE);
    }

    #[test]
    fn disposal_previous_should_restore_what_was_under() {
        let frames = [
            spec((0, 0, 4, 4), 0, DisposalMethod::Keep),
            spec((1, 1, 2, 2), 1, DisposalMethod::Previous),
            spec((0, 0, 1, 1), 2, DisposalMethod::Previous),
            spec((3, 3, 1, 1), 1, DisposalMethod::Keep),
        ];
        let (_, mut stream) = open(encode((4, 4), &frames, None));
        assert!(stream.seek(2).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), RED);
        assert_eq!(pixel(&stream, 0, 0), BLUE);
        assert!(stream.seek(3).expect("seek"));
        assert_eq!(pixel(&stream, 0, 0), RED);
        assert_eq!(pixel(&stream, 3, 3), GREEN);
    }

    #[test]
    fn disposal_previous_on_the_first_frame_should_leave_it_transparent() {
        let frames = [
            spec((0, 0, 2, 2), 0, DisposalMethod::Previous),
            spec((0, 0, 1, 1), 1, DisposalMethod::Keep),
        ];
        let (_, mut stream) = open(encode((2, 2), &frames, None));
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(pixel(&stream, 0, 0), GREEN);
        assert_eq!(pixel(&stream, 1, 1), CLEAR);
    }

    #[test]
    fn a_local_palette_should_apply_to_its_frame_only() {
        let mut frames = [
            spec((0, 0, 2, 1), 0, DisposalMethod::Keep),
            spec((1, 0, 1, 1), 0, DisposalMethod::Keep),
            spec((0, 0, 1, 1), 1, DisposalMethod::Keep),
        ];
        // Index 0 is white in frame 1's own palette.
        frames[1].palette = Some(vec![255, 255, 255, 0, 0, 0]);
        frames[1].transparent = None;
        let (_, mut stream) = open(encode((2, 1), &frames, None));
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(pixel(&stream, 1, 0), [255, 255, 255, 255]);
        assert!(stream.seek(2).expect("seek"));
        assert_eq!(pixel(&stream, 0, 0), GREEN);
        assert_eq!(pixel(&stream, 1, 0), [255, 255, 255, 255]);
    }

    #[test]
    fn an_interlaced_frame_should_come_out_in_row_order() {
        let mut frames = [spec((0, 0, 1, 10), 0, DisposalMethod::Keep)];
        frames[0].interlaced = true;
        frames[0].transparent = None;
        // Row r has index r % 3.
        let bytes = encode_pixels((1, 10), &frames, None, |_| {
            (0..10u8).map(|r| r % 3).collect()
        });
        let (_, mut stream) = open(bytes);
        assert!(stream.seek(0).expect("seek"));
        for row in 0..10 {
            let want = [RED, GREEN, BLUE][row % 3];
            assert_eq!(pixel(&stream, 0, row), want, "row {row}");
        }
    }

    #[test]
    fn interlaced_frames_of_any_size_should_decode_whole() {
        // gif 0.14.2's own row-by-row deinterlace stops short on some sizes
        // (33x37 among them); every size here must come out whole.
        for (w, h) in [
            (33u16, 37u16),
            (1, 1),
            (1, 9),
            (7, 3),
            (64, 65),
            (79, 41),
            (255, 2),
        ] {
            let mut frames = [spec((0, 0, w, h), 0, DisposalMethod::Keep)];
            frames[0].interlaced = true;
            frames[0].transparent = None;
            let (w_, h_) = (usize::from(w), usize::from(h));
            let bytes = encode_pixels((w, h), &frames, None, |_| {
                (0..h_)
                    .flat_map(|r| (0..w_).map(move |c| u8::try_from((r * 7 + c) % 3).unwrap_or(0)))
                    .collect()
            });
            let (_, mut stream) = open(bytes);
            assert!(stream.seek(0).expect("seek"), "{w}x{h}");
            for row in 0..h_ {
                for col in 0..w_ {
                    let want = [RED, GREEN, BLUE][(row * 7 + col) % 3];
                    assert_eq!(pixel(&stream, col, row), want, "{w}x{h} at {col},{row}");
                }
            }
        }
    }

    #[test]
    fn a_frame_past_the_screen_should_be_clipped() {
        let frames = [
            spec((0, 0, 2, 2), 0, DisposalMethod::Keep),
            spec((1, 1, 4, 4), 1, DisposalMethod::Background),
            spec((0, 0, 1, 1), 2, DisposalMethod::Keep),
        ];
        let (info, mut stream) = open(encode((2, 2), &frames, None));
        assert_eq!((info.width, info.height), (2, 2));
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), GREEN);
        assert_eq!(pixel(&stream, 0, 0), RED);
        assert!(stream.seek(2).expect("seek"));
        assert_eq!(pixel(&stream, 1, 1), CLEAR);
    }

    #[test]
    fn a_screen_with_no_size_should_take_the_first_frame() {
        let frames = [spec((1, 2, 3, 4), 0, DisposalMethod::Keep)];
        let info = scan(&encode((0, 0), &frames, None)).expect("scan");
        assert_eq!((info.width, info.height), (4, 6));
    }

    #[test]
    fn a_huge_screen_should_be_refused() {
        let frames = [spec((0, 0, 1, 1), 0, DisposalMethod::Keep)];
        assert!(matches!(
            scan(&encode((20_000, 20_000), &frames, None)),
            Err(AnimError::TooLarge(20_000, 20_000))
        ));
    }

    #[test]
    fn a_huge_frame_on_a_small_screen_should_be_refused() {
        // A 2x2 screen whose one frame declares 5000x4000 (20 Mpx, over the
        // limit), with an empty LZW stream (clear, end).
        let mut out = b"GIF89a".to_vec();
        out.extend_from_slice(&[2, 0, 2, 0, 0x80, 0, 0, 255, 0, 0, 0, 255, 0]);
        out.extend_from_slice(&[0x2C, 0, 0, 0, 0]);
        out.extend_from_slice(&5000u16.to_le_bytes());
        out.extend_from_slice(&4000u16.to_le_bytes());
        out.extend_from_slice(&[0, 2, 1, 0x2C, 0, 0x3B]);
        assert!(matches!(scan(&out), Err(AnimError::TooLarge(5000, 4000))));
    }

    #[test]
    fn seeking_back_should_replay_from_the_start() {
        let frames = [
            spec((0, 0, 2, 2), 0, DisposalMethod::Keep),
            spec((0, 0, 1, 1), 1, DisposalMethod::Keep),
        ];
        let (_, mut stream) = open(encode((2, 2), &frames, None));
        assert!(stream.seek(1).expect("seek"));
        let second = stream.canvas().to_vec();
        assert!(stream.seek(0).expect("seek"));
        assert_eq!(pixel(&stream, 0, 0), RED);
        assert!(stream.seek(1).expect("seek"));
        assert_eq!(stream.canvas(), &second[..]);
        // Past the end.
        assert!(!stream.seek(2).expect("seek"));
        assert!(stream.seek(0).expect("seek"));
    }

    #[test]
    fn a_truncated_file_should_play_the_frames_before_the_cut() {
        let frames = [
            spec((0, 0, 8, 8), 0, DisposalMethod::Keep),
            spec((0, 0, 8, 8), 1, DisposalMethod::Keep),
        ];
        let mut bytes = encode((8, 8), &frames, None);
        bytes.truncate(bytes.len() - 6);
        let (info, mut stream) = open(bytes);
        assert!(info.frames() >= 1);
        assert!(stream.seek(0).expect("seek"));
    }

    #[test]
    fn write_should_scale_down_into_the_stride() {
        let frames = [spec((0, 0, 4, 4), 1, DisposalMethod::Keep)];
        let (_, mut stream) = open(encode((4, 4), &frames, None));
        assert!(stream.seek(0).expect("seek"));
        let mut out = vec![9u8; 12 * 2];
        assert!(stream.write(&mut out, 2, 2, 12));
        assert_eq!(&out[0..4], &GREEN);
        // Past the row's pixels the stride's padding is untouched.
        assert_eq!(&out[8..12], &[9, 9, 9, 9]);
        let mut same = vec![0u8; 64];
        assert!(stream.write(&mut same, 4, 4, 16));
        assert_eq!(&same[..], stream.canvas());
    }

    #[test]
    fn other_bytes_should_be_refused() {
        assert!(matches!(scan(b"\x89PNG...."), Err(AnimError::Unsupported)));
    }
}
