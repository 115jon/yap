//! Our share's stream preview: the still other people see behind "Watch
//! stream" on our share's tile, as the web makes it (useVoiceChannel.ts
//! `STREAM_PREVIEW_CAPTURE_*`, the effect around `pushStreamPreviewUpdate`).
//!
//! While a share is live the app takes a still of the last frame it sent
//! ([`share_still`]) [`STILL_FIRST_AFTER`] after it goes live and then every
//! [`STILL_EVERY`], at most [`STILL_WIDTH`] wide, encodes it as a JPEG at
//! [`STILL_QUALITY`] and sends it as a `data:` URL in op 15
//! `stream_preview_url` on the global gateway (chat-gateway.ts
//! `sendVoiceStateUpdate`), which keeps it on our voice-channel member for
//! everyone's sidebar roster (meeting-room.ts `setVoiceMemberHint`). The
//! gateway refuses one longer than [`STILL_MAX_URL_BYTES`]
//! (`MAX_STREAM_PREVIEW_URL_BYTES`), so [`fit_still_url`] steps the quality
//! and then the width down until it fits.
//!
//! Viewers never pull the share's video for this: the still arrives with
//! the roster, so an unwatched share costs no track, transceiver or
//! renegotiation.

use std::time::Duration;

use crate::engine::I420Frame;

/// The still's width at most (`STREAM_PREVIEW_CAPTURE_WIDTH`).
pub const STILL_WIDTH: u32 = 224;
/// JPEG quality (`STREAM_PREVIEW_CAPTURE_QUALITY` 0.4).
pub const STILL_QUALITY: u8 = 40;
/// A fresh still this often while the share is live
/// (`STREAM_PREVIEW_CAPTURE_INTERVAL_MS`).
pub const STILL_EVERY: Duration = Duration::from_millis(8000);
/// The first still this long after the share goes live
/// (`STREAM_PREVIEW_CAPTURE_INITIAL_DELAY_MS`).
pub const STILL_FIRST_AFTER: Duration = Duration::from_millis(750);
/// The longest `stream_preview_url` the gateway takes
/// (meeting-room.ts `MAX_STREAM_PREVIEW_URL_BYTES`).
pub const STILL_MAX_URL_BYTES: usize = 4096;

/// The qualities and widths tried, in order, until a still fits
/// [`STILL_MAX_URL_BYTES`] (busy screens make bigger JPEGs).
const QUALITIES: [u8; 4] = [STILL_QUALITY, 30, 20, 10];
const WIDTHS: [u32; 3] = [STILL_WIDTH, 160, 112];

const JPEG_URL_PREFIX: &str = "data:image/jpeg;base64,";

/// A still as packed RGB (3 bytes a pixel, no row padding).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Still {
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

/// A still of our live share: the last frame it sent, at most `max_width`
/// wide. `None` while no share is live (or before its first frame).
pub fn share_still(max_width: u32) -> Option<Still> {
    #[cfg(feature = "libwebrtc")]
    {
        crate::share::live::still(max_width)
    }
    #[cfg(not(feature = "libwebrtc"))]
    {
        let _ = max_width;
        None
    }
}

/// Shrinks an I420 frame to at most `max_width` wide (never larger, the
/// aspect kept, as the web's canvas draws it) and converts it to RGB
/// (BT.601 limited range, what libyuv's ARGB to I420 wrote). Each output
/// pixel averages the luma under it; chroma is taken at its centre. `None`
/// for an empty size or planes too short for it.
pub fn i420_to_rgb(frame: &I420Frame<'_>, max_width: u32) -> Option<Still> {
    let (sw, sh) = (frame.width as usize, frame.height as usize);
    if sw == 0 || sh == 0 || max_width == 0 {
        return None;
    }
    let (cw, ch) = (sw.div_ceil(2), sh.div_ceil(2));
    let fits = |plane: &[u8], stride: u32, width: usize, rows: usize| {
        stride as usize >= width && plane.len() >= (rows - 1) * stride as usize + width
    };
    if !fits(frame.y, frame.stride_y, sw, sh)
        || !fits(frame.u, frame.stride_u, cw, ch)
        || !fits(frame.v, frame.stride_v, cw, ch)
    {
        return None;
    }
    let dw = sw.min(max_width as usize);
    let dh = ((dw * sh + sw / 2) / sw).max(1);
    let (sy, su, sv) = (
        frame.stride_y as usize,
        frame.stride_u as usize,
        frame.stride_v as usize,
    );
    let mut rgb = Vec::with_capacity(dw * dh * 3);
    for row in 0..dh {
        let (y0, y1) = (row * sh / dh, ((row + 1) * sh / dh).max(row * sh / dh + 1));
        for col in 0..dw {
            let (x0, x1) = (col * sw / dw, ((col + 1) * sw / dw).max(col * sw / dw + 1));
            let mut sum = 0u32;
            for y in y0..y1 {
                let line = &frame.y[y * sy..y * sy + sw];
                sum += line[x0..x1].iter().map(|&p| u32::from(p)).sum::<u32>();
            }
            let count = ((y1 - y0) * (x1 - x0)) as u32;
            let luma = (sum + count / 2) / count;
            let (cx, cy) = (((x0 + x1) / 2) / 2, ((y0 + y1) / 2) / 2);
            let u = frame.u[cy.min(ch - 1) * su + cx.min(cw - 1)];
            let v = frame.v[cy.min(ch - 1) * sv + cx.min(cw - 1)];
            rgb.extend_from_slice(&yuv_to_rgb(luma, u, v));
        }
    }
    Some(Still {
        width: dw as u32,
        height: dh as u32,
        rgb,
    })
}

fn yuv_to_rgb(y: u32, u: u8, v: u8) -> [u8; 3] {
    let c = y as i32 - 16;
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    let clamp = |x: i32| ((x + 128) >> 8).clamp(0, 255) as u8;
    [
        clamp(298 * c + 409 * e),
        clamp(298 * c - 100 * d - 208 * e),
        clamp(298 * c + 516 * d),
    ]
}

/// A JPEG as the `data:` URL the web's `canvas.toDataURL("image/jpeg")`
/// makes.
pub fn jpeg_data_url(jpeg: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut url = String::with_capacity(JPEG_URL_PREFIX.len() + jpeg.len().div_ceil(3) * 4);
    url.push_str(JPEG_URL_PREFIX);
    for chunk in jpeg.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                url.push(ALPHABET[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                url.push('=');
            }
        }
    }
    url
}

/// The still's URL, the first that fits [`STILL_MAX_URL_BYTES`]: `still`
/// takes one at a width, `encode` makes its JPEG at a quality. Every
/// quality is tried at [`STILL_WIDTH`] first, then at smaller widths.
/// `None` when there is no still or nothing fits.
pub fn fit_still_url(
    mut still: impl FnMut(u32) -> Option<Still>,
    mut encode: impl FnMut(&Still, u8) -> Option<Vec<u8>>,
) -> Option<String> {
    for width in WIDTHS {
        let picture = still(width)?;
        for quality in QUALITIES {
            let Some(jpeg) = encode(&picture, quality) else {
                continue;
            };
            let url = jpeg_data_url(&jpeg);
            if url.len() <= STILL_MAX_URL_BYTES {
                return Some(url);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(width: u32, height: u32, y: u8, u: u8, v: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
        let (cw, ch) = (width.div_ceil(2) as usize, height.div_ceil(2) as usize);
        (
            vec![y; (width * height) as usize],
            vec![u; cw * ch],
            vec![v; cw * ch],
        )
    }

    fn frame<'a>(
        width: u32,
        height: u32,
        planes: &'a (Vec<u8>, Vec<u8>, Vec<u8>),
    ) -> I420Frame<'a> {
        I420Frame {
            width,
            height,
            rotation: 0,
            y: &planes.0,
            u: &planes.1,
            v: &planes.2,
            stride_y: width,
            stride_u: width.div_ceil(2),
            stride_v: width.div_ceil(2),
        }
    }

    #[test]
    fn a_1080p_frame_should_shrink_to_the_web_width_keeping_its_aspect() {
        let planes = plain(1920, 1080, 128, 128, 128);
        let still = i420_to_rgb(&frame(1920, 1080, &planes), STILL_WIDTH).expect("still");
        assert_eq!((still.width, still.height), (224, 126));
        assert_eq!(still.rgb.len(), 224 * 126 * 3);
    }

    #[test]
    fn a_small_frame_should_not_be_enlarged() {
        let planes = plain(160, 90, 128, 128, 128);
        let still = i420_to_rgb(&frame(160, 90, &planes), STILL_WIDTH).expect("still");
        assert_eq!((still.width, still.height), (160, 90));
    }

    #[test]
    fn colours_should_convert_as_bt601_limited_range() {
        let black = plain(64, 36, 16, 128, 128);
        let white = plain(64, 36, 235, 128, 128);
        let b = i420_to_rgb(&frame(64, 36, &black), 32).expect("black");
        let w = i420_to_rgb(&frame(64, 36, &white), 32).expect("white");
        assert_eq!(&b.rgb[..3], &[0, 0, 0]);
        assert_eq!(&w.rgb[..3], &[255, 255, 255]);
    }

    #[test]
    fn shrinking_should_average_the_luma_under_each_pixel() {
        // Alternating black and white columns average to mid grey.
        let mut planes = plain(4, 2, 16, 128, 128);
        for (i, p) in planes.0.iter_mut().enumerate() {
            if i % 2 == 1 {
                *p = 236;
            }
        }
        let still = i420_to_rgb(&frame(4, 2, &planes), 2).expect("still");
        assert_eq!((still.width, still.height), (2, 1));
        // Y 126: (298 * 110 + 128) >> 8 = 128.
        assert_eq!(&still.rgb[..3], &[128, 128, 128]);
    }

    #[test]
    fn short_planes_should_give_no_still() {
        let planes = plain(64, 36, 16, 128, 128);
        let mut bad = frame(64, 36, &planes);
        bad.height = 72;
        assert_eq!(i420_to_rgb(&bad, 32), None);
        assert_eq!(i420_to_rgb(&frame(0, 36, &planes), 32), None);
    }

    #[test]
    fn a_jpeg_should_become_a_base64_data_url() {
        assert_eq!(jpeg_data_url(b""), "data:image/jpeg;base64,");
        assert_eq!(jpeg_data_url(b"f"), "data:image/jpeg;base64,Zg==");
        assert_eq!(jpeg_data_url(b"fo"), "data:image/jpeg;base64,Zm8=");
        assert_eq!(jpeg_data_url(b"foobar"), "data:image/jpeg;base64,Zm9vYmFy");
        assert_eq!(
            jpeg_data_url(&[0xff, 0xd8, 0xff]),
            "data:image/jpeg;base64,/9j/"
        );
    }

    fn still_at(width: u32) -> Option<Still> {
        Some(Still {
            width,
            height: width * 9 / 16,
            rgb: Vec::new(),
        })
    }

    #[test]
    fn the_first_still_should_be_the_web_size_and_quality_when_it_fits() {
        let mut tried = Vec::new();
        let url = fit_still_url(still_at, |s, q| {
            tried.push((s.width, q));
            Some(vec![0; 100])
        })
        .expect("fits");
        assert_eq!(tried, [(224, 40)]);
        assert!(url.starts_with("data:image/jpeg;base64,"));
    }

    #[test]
    fn a_busy_still_should_step_quality_then_width_down_until_it_fits() {
        let mut tried = Vec::new();
        // Fits only at 160 wide, quality 20 (base64 of 3000 bytes is 4000).
        let url = fit_still_url(still_at, |s, q| {
            tried.push((s.width, q));
            Some(vec![
                0;
                if s.width <= 160 && q <= 20 {
                    3000
                } else {
                    3100
                }
            ])
        })
        .expect("fits");
        assert!(url.len() <= STILL_MAX_URL_BYTES);
        assert_eq!(
            tried,
            [
                (224, 40),
                (224, 30),
                (224, 20),
                (224, 10),
                (160, 40),
                (160, 30),
                (160, 20)
            ]
        );
    }

    #[test]
    fn nothing_should_be_sent_without_a_still_or_when_nothing_fits() {
        assert_eq!(fit_still_url(|_| None, |_, _| Some(vec![0; 10])), None);
        assert_eq!(fit_still_url(still_at, |_, _| Some(vec![0; 4000])), None);
    }
}
