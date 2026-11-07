//! `YAP_FAKE_SHARE_SOURCE=<width>x<height>` (test runs only): the picked
//! source is replaced by a synthetic picture of that size, pushed at the
//! share's frame rate. It measures the share's encode path (NV12, the
//! hardware MFT, the call) without Windows Graphics Capture, which delivers
//! a few frames a second while the displays sleep. A bar sweeps across a
//! fixed pattern, so the encoder sees motion every frame; only the bar's
//! columns are rewritten, so the generator itself costs little.

use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use yap_voice::{push_screen_frame, ScreenFrame, ShareQuality};

use crate::stop::Stop;

const BAR: u32 = 96;

/// The size asked for, if any.
pub(super) fn size() -> Option<(u32, u32)> {
    let value = std::env::var("YAP_FAKE_SHARE_SOURCE").ok()?;
    let (w, h) = value.split_once('x')?;
    let (w, h) = (w.trim().parse::<u32>().ok()?, h.trim().parse::<u32>().ok()?);
    ((64..=7680).contains(&w) && (64..=4320).contains(&h)).then_some((w & !1, h & !1))
}

/// Starts the generator; stop it with the returned [`Stop`] and join.
pub(super) fn spawn(
    width: u32,
    height: u32,
    quality: ShareQuality,
) -> Result<(Arc<Stop>, JoinHandle<()>), String> {
    let stop = Arc::new(Stop::default());
    let thread = {
        let stop = Arc::clone(&stop);
        std::thread::Builder::new()
            .name("yap-fake-share".to_owned())
            .spawn(move || run(width, height, quality, &stop))
            .map_err(|e| format!("could not start the fake share source: {e}"))?
    };
    Ok((stop, thread))
}

fn run(width: u32, height: u32, quality: ShareQuality, stop: &Stop) {
    let stride = width * 4;
    let mut bgra = vec![0u8; (stride * height) as usize];
    for y in 0..height {
        for x in 0..width {
            let i = (y * stride + x * 4) as usize;
            bgra[i] = (x * 255 / width) as u8;
            bgra[i + 1] = (y * 255 / height) as u8;
            bgra[i + 2] = (((x / 64) + (y / 64)) % 2 * 160) as u8;
            bgra[i + 3] = 255;
        }
    }
    let base = bgra.clone();
    let interval = Duration::from_nanos(1_000_000_000 / u64::from(quality.fps.max(1)));
    let started = Instant::now();
    let mut frame: u64 = 0;
    let mut bar_at: Option<u32> = None;
    while !stop.is_stopped() {
        // Put back the old bar's columns, then draw the new one.
        if let Some(at) = bar_at {
            paint(&mut bgra, &base, width, height, at, None);
        }
        let at = ((frame * 8) % u64::from(width.saturating_sub(BAR).max(1))) as u32;
        paint(&mut bgra, &base, width, height, at, Some(frame));
        bar_at = Some(at);
        push_screen_frame(&ScreenFrame {
            bgra: &bgra,
            stride,
            width,
            height,
            quality,
        });
        frame += 1;
        // std's sleep is a high-resolution timer on Windows.
        let due = started + interval * u32::try_from(frame).unwrap_or(u32::MAX);
        if let Some(wait) = due.checked_duration_since(Instant::now()) {
            std::thread::sleep(wait);
        }
    }
}

/// The bar's columns from `base` (`shade` None) or in the frame's colour.
fn paint(bgra: &mut [u8], base: &[u8], width: u32, height: u32, at: u32, shade: Option<u64>) {
    let stride = (width * 4) as usize;
    let (from, to) = ((at * 4) as usize, ((at + BAR).min(width) * 4) as usize);
    for y in 0..height as usize {
        let row = y * stride;
        match shade {
            None => bgra[row + from..row + to].copy_from_slice(&base[row + from..row + to]),
            Some(frame) => {
                let v = ((frame * 5 + y as u64) % 256) as u8;
                for px in bgra[row + from..row + to].chunks_exact_mut(4) {
                    px.copy_from_slice(&[v, 255 - v, 128, 255]);
                }
            }
        }
    }
}
