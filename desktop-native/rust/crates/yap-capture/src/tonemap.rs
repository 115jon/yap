//! HDR to SDR for a share of an HDR (advanced color) display: the curve the
//! capture's pixel shader runs (`win/hdr.hlsl`), as a CPU reference the
//! tests and the gradient check compare the shader against.
//!
//! The input is scRGB: linear BT.709 primaries where 1.0 is 80 nits, as
//! Windows Graphics Capture delivers `R16G16B16A16Float`. The output is
//! sRGB-encoded BT.709, what the BGRA8 path pushes today:
//!
//! 1. scale by `80 / sdr_white_nits`, so the display's SDR white (the
//!    "SDR content brightness" setting) lands on 1.0 and SDR content keeps
//!    the brightness it had;
//! 2. clip negatives (colours outside BT.709) to 0;
//! 3. compress the brightest channel (maxRGB) above a knee with an
//!    extended Reinhard shoulder that reaches 1.0 exactly at the display's
//!    peak (`max_nits / sdr_white_nits`), and scale all three channels by
//!    the same factor, so hue and saturation survive (a per-channel curve
//!    turns bright colours white);
//! 4. the sRGB OETF.
//!
//! With no headroom (peak at or below SDR white) the knee sits at 1.0 and
//! the curve is the identity on `[0, 1]`: an SDR display captured in FP16
//! gives what the BGRA8 path gives.

// The app uses the constants; the curve itself runs on the GPU, and the
// CPU copy here is for the tests.
#![cfg_attr(not(test), allow(dead_code))]

/// scRGB's 1.0, in nits.
pub const SCRGB_WHITE_NITS: f32 = 80.0;

/// The lowest the knee goes (relative to SDR white, linear). Below it the
/// curve is the identity; with a lot of headroom SDR white itself lands at
/// about 0.9 linear (243 of 255 after the OETF) to leave room for the
/// highlights above it.
pub const KNEE_FLOOR: f32 = 0.8;

/// The knee drops by this much per unit of headroom above 1.0, from 1.0
/// (no headroom) to [`KNEE_FLOOR`] (peak at 5x SDR white and beyond).
const KNEE_PER_HEADROOM: f32 = 0.05;

/// The tone map of one display, in the units the shader takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToneMap {
    /// `80 / sdr_white_nits`: scRGB to SDR-white-relative.
    pub scale: f32,
    /// The display's peak, relative to SDR white (at least 1.0).
    pub peak: f32,
    /// Where the shoulder starts, relative to SDR white.
    pub knee: f32,
}

impl ToneMap {
    /// The map for a display whose SDR white is `sdr_white_nits` and whose
    /// peak is `max_nits`. Values that are not finite and positive fall
    /// back to scRGB's own 80 nits; a peak under SDR white counts as none.
    pub fn from_nits(sdr_white_nits: f32, max_nits: f32) -> Self {
        let white = if sdr_white_nits.is_finite() && sdr_white_nits > 0.0 {
            sdr_white_nits
        } else {
            SCRGB_WHITE_NITS
        };
        let peak = if max_nits.is_finite() {
            (max_nits / white).max(1.0)
        } else {
            1.0
        };
        let knee = (1.0 - KNEE_PER_HEADROOM * (peak - 1.0)).clamp(KNEE_FLOOR, 1.0);
        // A headroom too small to move the knee is none (the shoulder's
        // range would be 0).
        let peak = if knee >= 1.0 { 1.0 } else { peak };
        Self {
            scale: SCRGB_WHITE_NITS / white,
            peak,
            knee,
        }
    }

    /// The curve on the brightest channel `m` (SDR-white-relative, linear):
    /// the identity up to the knee, then an extended Reinhard shoulder that
    /// meets the identity with the same slope and reaches 1.0 at the peak;
    /// 1.0 above it.
    pub fn shoulder(&self, m: f32) -> f32 {
        if m <= self.knee {
            return m;
        }
        if m >= self.peak {
            return 1.0;
        }
        let range = 1.0 - self.knee;
        let t = (m - self.knee) / range;
        let w = (self.peak - self.knee) / range;
        self.knee + range * (t * (1.0 + t / (w * w)) / (1.0 + t))
    }

    /// One scRGB pixel to linear SDR (each channel in `[0, 1]`).
    pub fn map_linear(&self, rgb: [f32; 3]) -> [f32; 3] {
        // `f32::max` drops a NaN for the 0 (as HLSL's `max` does).
        let rgb = rgb.map(|c| (c * self.scale).max(0.0));
        let m = rgb[0].max(rgb[1]).max(rgb[2]);
        let rgb = if m > self.knee {
            let k = self.shoulder(m) / m;
            rgb.map(|c| c * k)
        } else {
            rgb
        };
        rgb.map(|c| c.clamp(0.0, 1.0))
    }

    /// One scRGB pixel to sRGB-encoded SDR, each channel in `[0, 1]`.
    pub fn map(&self, rgb: [f32; 3]) -> [f32; 3] {
        self.map_linear(rgb).map(srgb_oetf)
    }

    /// One scRGB pixel to 8-bit BGRA (opaque), rounded as a UNORM render
    /// target rounds.
    pub fn map_bgra8(&self, rgb: [f32; 3]) -> [u8; 4] {
        let [r, g, b] = self.map(rgb).map(unorm8);
        [b, g, r, 255]
    }
}

/// The sRGB OETF (IEC 61966-2-1) of a linear value in `[0, 1]`.
pub fn srgb_oetf(c: f32) -> f32 {
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// The sRGB EOTF (decode) of an encoded value in `[0, 1]`.
pub fn srgb_eotf(v: f32) -> f32 {
    if v <= 0.040_45 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// `[0, 1]` to 8 bits, round to nearest (D3D's float to UNORM rule).
pub fn unorm8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}

/// `YAP_TEST_HDR_CAPTURE=<sdrWhiteNits>,<maxNits>` (tests): the capture
/// uses FP16 and this tone map even on an SDR display. `80,80` is the
/// identity, which on an SDR display must look as the BGRA8 path does.
pub fn test_override() -> Option<ToneMap> {
    parse_override(&std::env::var("YAP_TEST_HDR_CAPTURE").ok()?)
}

fn parse_override(value: &str) -> Option<ToneMap> {
    let (white, max) = value.split_once(',')?;
    let white: f32 = white.trim().parse().ok()?;
    let max: f32 = max.trim().parse().ok()?;
    ((1.0..=10_000.0).contains(&white) && (1.0..=10_000.0).contains(&max))
        .then(|| ToneMap::from_nits(white, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn no_headroom_should_be_the_identity_on_sdr_values() {
        let map = ToneMap::from_nits(80.0, 80.0);
        assert_eq!((map.scale, map.peak, map.knee), (1.0, 1.0, 1.0));
        for i in 0..=255u8 {
            let linear = srgb_eotf(f32::from(i) / 255.0);
            assert_eq!(map.map_bgra8([linear; 3]), [i, i, i, 255], "code {i}");
        }
    }

    #[test]
    fn sdr_white_should_scale_to_one() {
        // 200-nit SDR white is 2.5 in scRGB.
        let map = ToneMap::from_nits(200.0, 200.0);
        assert!(close(map.scale, 0.4));
        assert_eq!(map.map_bgra8([2.5; 3]), [255; 4]);
        // Mid grey at that white: 0.214 linear of white (sRGB 128).
        let grey = srgb_eotf(128.0 / 255.0) * 2.5;
        assert_eq!(map.map_bgra8([grey; 3]), [128, 128, 128, 255]);
    }

    #[test]
    fn the_knee_should_follow_the_headroom() {
        assert!(close(ToneMap::from_nits(200.0, 400.0).knee, 0.95));
        assert!(close(ToneMap::from_nits(200.0, 600.0).knee, 0.9));
        assert!(close(ToneMap::from_nits(200.0, 1000.0).knee, 0.8));
        assert!(close(ToneMap::from_nits(80.0, 1000.0).knee, KNEE_FLOOR));
        let map = ToneMap::from_nits(200.0, 1000.0);
        assert!(close(map.peak, 5.0));
    }

    #[test]
    fn the_shoulder_should_be_continuous_monotonic_and_reach_one_at_the_peak() {
        let map = ToneMap::from_nits(200.0, 1000.0);
        assert!(close(map.shoulder(map.knee), map.knee));
        // Slope 1 at the knee.
        let slope = (map.shoulder(map.knee + 1e-3) - map.knee) / 1e-3;
        assert!((slope - 1.0).abs() < 1e-2, "slope {slope}");
        assert!(close(map.shoulder(map.peak), 1.0));
        assert_eq!(map.shoulder(10.0), 1.0);
        let mut last = 0.0;
        for i in 0..=1000 {
            let y = map.shoulder(i as f32 * 0.006);
            assert!(y >= last, "falls at {i}");
            assert!(y <= 1.0);
            last = y;
        }
    }

    #[test]
    fn known_values_at_200_and_1000_nits() {
        let map = ToneMap::from_nits(200.0, 1000.0);
        // knee 0.8, w = (5 - 0.8) / 0.2 = 21. SDR white (t = 1):
        // 0.8 + 0.2 * (1 + 1/441) / 2 = 0.900227.
        assert!(close(map.shoulder(1.0), 0.900_227));
        // 2x SDR white (t = 6): 0.8 + 0.2 * 6 * (1 + 6/441) / 7.
        assert!(close(
            map.shoulder(2.0),
            0.8 + 0.2 * 6.0 * (1.0 + 6.0 / 441.0) / 7.0
        ));
        // SDR white in sRGB 8 bits, and the peak.
        assert_eq!(map.map_bgra8([2.5; 3]), [243, 243, 243, 255]);
        assert_eq!(map.map_bgra8([12.5; 3]), [255; 4]);
        // Below the knee nothing changes: 0.5 of white.
        let half = map.map_linear([1.25; 3]);
        assert!(close(half[0], 0.5));
    }

    #[test]
    fn maxrgb_should_keep_the_ratios_of_a_bright_colour() {
        let map = ToneMap::from_nits(200.0, 1000.0);
        // A saturated orange at 4x SDR white: 10 : 5 : 1 in scRGB units.
        let out = map.map_linear([10.0, 5.0, 1.0]);
        assert!(close(out[1] / out[0], 0.5));
        assert!(close(out[2] / out[0], 0.1));
        assert!(out[0] < 1.0);
        // A per-channel clip would have made it (1, 1, 0.4): near white.
    }

    #[test]
    fn colours_outside_bt709_and_nan_should_clip_to_zero() {
        let map = ToneMap::from_nits(80.0, 1000.0);
        assert_eq!(map.map_linear([-0.2, 0.5, f32::NAN]), [0.0, 0.5, 0.0]);
    }

    #[test]
    fn the_override_should_parse_white_and_peak() {
        assert_eq!(
            parse_override("200,1000"),
            Some(ToneMap::from_nits(200.0, 1000.0))
        );
        assert_eq!(
            parse_override(" 80 , 80 "),
            Some(ToneMap::from_nits(80.0, 80.0))
        );
        assert_eq!(parse_override("200"), None);
        assert_eq!(parse_override("0,1000"), None);
        assert_eq!(parse_override("x,1"), None);
    }

    #[test]
    fn the_oetf_should_match_srgb() {
        assert_eq!(srgb_oetf(0.0), 0.0);
        assert!(close(srgb_oetf(1.0), 1.0));
        assert!(close(srgb_oetf(0.18), 0.461_356));
        for i in 0..=255u8 {
            let v = f32::from(i) / 255.0;
            assert_eq!(unorm8(srgb_oetf(srgb_eotf(v))), i);
        }
    }
}
