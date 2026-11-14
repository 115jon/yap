//! Pixel arithmetic on tightly packed BGRA buffers.

/// The size a `width` x `height` image is shown at within `max_width`: the
/// same aspect, at least 1 px high, and never larger than it is.
pub(crate) fn fit_width(width: u32, height: u32, max_width: u32) -> (u32, u32) {
    if width == 0 || height == 0 || max_width == 0 {
        return (0, 0);
    }
    if width <= max_width {
        return (width, height);
    }
    let scaled =
        (u64::from(height) * u64::from(max_width) + u64::from(width) / 2) / u64::from(width);
    (max_width, u32::try_from(scaled).unwrap_or(u32::MAX).max(1))
}

/// Resamples `src` (`sw` x `sh`, tightly packed BGRA) to `dw` x `dh` by
/// averaging the source pixels each destination pixel covers (a box filter;
/// enlarging repeats pixels). Colour is averaged weighted by alpha, so
/// transparent pixels do not darken their neighbours; the result stays
/// straight (not premultiplied). `None` for a zero size or a buffer of the
/// wrong length.
pub(crate) fn box_resize(src: &[u8], sw: u32, sh: u32, dw: u32, dh: u32) -> Option<Vec<u8>> {
    let (sw, sh, dw, dh) = (sw as usize, sh as usize, dw as usize, dh as usize);
    if sw == 0 || sh == 0 || dw == 0 || dh == 0 || src.len() != sw * sh * 4 {
        return None;
    }
    let mut out = Vec::with_capacity(dw * dh * 4);
    for dy in 0..dh {
        let y0 = dy * sh / dh;
        let y1 = ((dy + 1) * sh / dh).max(y0 + 1);
        for dx in 0..dw {
            let x0 = dx * sw / dw;
            let x1 = ((dx + 1) * sw / dw).max(x0 + 1);
            let mut color = [0u64; 3];
            let (mut alpha, mut count) = (0u64, 0u64);
            for y in y0..y1 {
                for px in src[(y * sw + x0) * 4..(y * sw + x1) * 4].chunks_exact(4) {
                    let a = u64::from(px[3]);
                    color[0] += u64::from(px[0]) * a;
                    color[1] += u64::from(px[1]) * a;
                    color[2] += u64::from(px[2]) * a;
                    alpha += a;
                    count += 1;
                }
            }
            if alpha == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let channel = |sum: u64| ((sum + alpha / 2) / alpha) as u8;
                out.extend_from_slice(&[
                    channel(color[0]),
                    channel(color[1]),
                    channel(color[2]),
                    ((alpha + count / 2) / count) as u8,
                ]);
            }
        }
    }
    Some(out)
}

/// Sets every alpha byte to 255 (a capture of a window can carry zero alpha
/// that a viewer would show as nothing).
pub(crate) fn force_opaque(bgra: &mut [u8]) {
    for px in bgra.chunks_exact_mut(4) {
        px[3] = 255;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_keeps_the_aspect() {
        assert_eq!(fit_width(1920, 1080, 480), (480, 270));
    }

    #[test]
    fn fit_never_enlarges() {
        assert_eq!(fit_width(300, 200, 480), (300, 200));
    }

    #[test]
    fn fit_keeps_at_least_one_row() {
        assert_eq!(fit_width(10_000, 1, 480), (480, 1));
    }

    #[test]
    fn fit_of_nothing_is_nothing() {
        assert_eq!(fit_width(0, 10, 480), (0, 0));
        assert_eq!(fit_width(10, 10, 0), (0, 0));
    }

    #[test]
    fn halving_averages_each_2x2_block() {
        // 2x2, one channel pattern per pixel: B = 0, 100, 200, 40.
        let src = [
            0, 0, 0, 255, 100, 0, 0, 255, //
            200, 0, 0, 255, 40, 0, 0, 255,
        ];
        assert_eq!(box_resize(&src, 2, 2, 1, 1), Some(vec![85, 0, 0, 255]));
    }

    #[test]
    fn halving_a_4x2_keeps_the_left_and_right_halves_apart() {
        let mut src = Vec::new();
        for _row in 0..2 {
            src.extend_from_slice(&[10, 10, 10, 255, 10, 10, 10, 255]);
            src.extend_from_slice(&[200, 200, 200, 255, 200, 200, 200, 255]);
        }
        assert_eq!(
            box_resize(&src, 4, 2, 2, 1),
            Some(vec![10, 10, 10, 255, 200, 200, 200, 255])
        );
    }

    #[test]
    fn same_size_is_a_copy() {
        let src = [1, 2, 3, 255, 4, 5, 6, 255];
        assert_eq!(box_resize(&src, 2, 1, 2, 1), Some(src.to_vec()));
    }

    #[test]
    fn enlarging_repeats_pixels() {
        let src = [1, 2, 3, 255];
        assert_eq!(
            box_resize(&src, 1, 1, 2, 1),
            Some(vec![1, 2, 3, 255, 1, 2, 3, 255])
        );
    }

    #[test]
    fn a_transparent_neighbour_does_not_darken_the_colour() {
        // One opaque white pixel, one fully transparent black pixel.
        let src = [255, 255, 255, 255, 0, 0, 0, 0];
        assert_eq!(box_resize(&src, 2, 1, 1, 1), Some(vec![255, 255, 255, 128]));
    }

    #[test]
    fn a_wrong_length_buffer_is_refused() {
        assert_eq!(box_resize(&[0; 7], 2, 1, 1, 1), None);
    }

    #[test]
    fn a_zero_size_is_refused() {
        assert_eq!(box_resize(&[], 0, 0, 1, 1), None);
    }

    #[test]
    fn force_opaque_sets_every_alpha() {
        let mut px = [1, 2, 3, 0, 4, 5, 6, 7];
        force_opaque(&mut px);
        assert_eq!(px, [1, 2, 3, 255, 4, 5, 6, 255]);
    }
}
