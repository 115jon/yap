//! The colour picker's maths (`qml/components/ColorPopover.qml`, what
//! Chromium's `<input type="color">` popup does for the web): HSV, RGB and
//! `#RRGGBB` both ways, and the recent colours list.
//!
//! Hue is in degrees `[0, 360)`, saturation and value in `[0, 1]`. Channels
//! round to the nearest byte, so a colour that came from a hex goes back to
//! the same hex. A grey or black has no hue of its own: [`hex_to_hsv`] gives
//! hue 0 there, and the picker keeps the hue it had instead.

use crate::settings::normalize_hex_color;

/// How many recent colours the picker keeps.
pub const RECENT_COLORS: usize = 8;

/// A colour as hue, saturation and value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

fn unit(x: f64) -> f64 {
    if x.is_finite() {
        x.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Hue folded into `[0, 360)`; not a number reads as 0.
fn degrees(h: f64) -> f64 {
    if !h.is_finite() {
        return 0.0;
    }
    let h = h.rem_euclid(360.0);
    // rem_euclid of a tiny negative can round up to 360 itself.
    if h >= 360.0 {
        0.0
    } else {
        h
    }
}

fn byte(x: f64) -> u8 {
    // In [0, 255] after the clamp, so the cast cannot truncate.
    (unit(x) * 255.0).round() as u8
}

/// RGB bytes for a hue (any angle, folded), saturation and value (clamped).
pub fn hsv_to_rgb(h: f64, s: f64, v: f64) -> [u8; 3] {
    let (h, s, v) = (degrees(h), unit(s), unit(v));
    let c = v * s;
    let sector = h / 60.0;
    let x = c * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match sector as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [byte(r + m), byte(g + m), byte(b + m)]
}

/// The HSV of RGB bytes; a grey has hue 0 and saturation 0, black value 0.
pub fn rgb_to_hsv([r, g, b]: [u8; 3]) -> Hsv {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / delta).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    Hsv {
        h: degrees(h),
        s,
        v: max,
    }
}

/// `#RRGGBB` (upper case) for RGB bytes.
pub fn rgb_to_hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

/// The bytes of `#RRGGBB`, `RRGGBB`, `#RGB` or `RGB` (any case, spaces
/// around ignored); `None` for anything else.
pub fn hex_to_rgb(text: &str) -> Option<[u8; 3]> {
    let hex = normalize_hex_color(text)?;
    let value = u32::from_str_radix(&hex[1..], 16).ok()?;
    let [_, r, g, b] = value.to_be_bytes();
    Some([r, g, b])
}

/// `#RRGGBB` for a hue, saturation and value.
pub fn hsv_to_hex(h: f64, s: f64, v: f64) -> String {
    rgb_to_hex(hsv_to_rgb(h, s, v))
}

/// The HSV of a hex colour, as [`hex_to_rgb`] reads it.
pub fn hex_to_hsv(text: &str) -> Option<Hsv> {
    hex_to_rgb(text).map(rgb_to_hsv)
}

/// Puts `hex` first in `recents` (moving it if it was there), keeps
/// [`RECENT_COLORS`] at most, and says whether the list changed. Not a
/// colour: nothing changes.
pub fn remember(recents: &mut Vec<String>, hex: &str) -> bool {
    let Some(hex) = normalize_hex_color(hex) else {
        return false;
    };
    if recents.first() == Some(&hex) {
        return false;
    }
    recents.retain(|c| normalize_hex_color(c).as_deref() != Some(hex.as_str()));
    recents.insert(0, hex);
    recents.truncate(RECENT_COLORS);
    true
}

/// The recents as saved, cleaned: colours only, upper case, each once, at
/// most [`RECENT_COLORS`].
pub fn clean_recents(saved: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for hex in saved.iter().filter_map(|c| normalize_hex_color(c)) {
        if !out.contains(&hex) {
            out.push(hex);
        }
        if out.len() == RECENT_COLORS {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn hsv_to_hex_should_give_the_primaries_and_greys() {
        assert_eq!(hsv_to_hex(0.0, 1.0, 1.0), "#FF0000");
        assert_eq!(hsv_to_hex(60.0, 1.0, 1.0), "#FFFF00");
        assert_eq!(hsv_to_hex(120.0, 1.0, 1.0), "#00FF00");
        assert_eq!(hsv_to_hex(180.0, 1.0, 1.0), "#00FFFF");
        assert_eq!(hsv_to_hex(240.0, 1.0, 1.0), "#0000FF");
        assert_eq!(hsv_to_hex(300.0, 1.0, 1.0), "#FF00FF");
        assert_eq!(hsv_to_hex(200.0, 0.0, 1.0), "#FFFFFF");
        assert_eq!(hsv_to_hex(200.0, 0.0, 0.5), "#808080");
        assert_eq!(hsv_to_hex(200.0, 1.0, 0.0), "#000000");
        // Chromium's picker for #33CCFF: 195°, 80 %, 100 %.
        assert_eq!(hsv_to_hex(195.0, 0.8, 1.0), "#33CCFF");
    }

    #[test]
    fn hsv_to_rgb_should_fold_the_hue_and_clamp_the_rest() {
        assert_eq!(hsv_to_rgb(360.0, 1.0, 1.0), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(-120.0, 1.0, 1.0), [0, 0, 255]);
        assert_eq!(hsv_to_rgb(720.0 + 120.0, 1.0, 1.0), [0, 255, 0]);
        assert_eq!(hsv_to_rgb(0.0, 2.0, 1.5), [255, 0, 0]);
        assert_eq!(hsv_to_rgb(0.0, -1.0, -1.0), [0, 0, 0]);
        assert_eq!(hsv_to_rgb(f64::NAN, f64::NAN, 1.0), [255, 255, 255]);
        assert_eq!(hsv_to_rgb(-1e-18, 1.0, 1.0), [255, 0, 0]);
    }

    #[test]
    fn rgb_to_hsv_should_read_hue_saturation_and_value() {
        let red = rgb_to_hsv([255, 0, 0]);
        assert!(close(red.h, 0.0) && close(red.s, 1.0) && close(red.v, 1.0));
        let magenta_ish = rgb_to_hsv([255, 0, 128]);
        assert!(
            magenta_ish.h > 329.0 && magenta_ish.h < 331.0,
            "{magenta_ish:?}"
        );
        let grey = rgb_to_hsv([128, 128, 128]);
        assert!(close(grey.h, 0.0) && close(grey.s, 0.0) && close(grey.v, 128.0 / 255.0));
        let black = rgb_to_hsv([0, 0, 0]);
        assert_eq!(
            black,
            Hsv {
                h: 0.0,
                s: 0.0,
                v: 0.0
            }
        );
        let sky = rgb_to_hsv([0x33, 0xCC, 0xFF]);
        assert!(
            close(sky.h, 195.0) && close(sky.s, 0.8) && close(sky.v, 1.0),
            "{sky:?}"
        );
    }

    #[test]
    fn hex_should_parse_short_long_and_refuse_the_rest() {
        assert_eq!(hex_to_rgb("#33ccff"), Some([0x33, 0xCC, 0xFF]));
        assert_eq!(hex_to_rgb(" 33CCFF "), Some([0x33, 0xCC, 0xFF]));
        assert_eq!(hex_to_rgb("#3cf"), Some([0x33, 0xCC, 0xFF]));
        assert_eq!(hex_to_rgb("fff"), Some([255, 255, 255]));
        for bad in [
            "",
            "#",
            "#12",
            "#1234",
            "#12345",
            "#1234567",
            "#GGGGGG",
            "rgb(1,2,3)",
            "##123456",
        ] {
            assert_eq!(hex_to_rgb(bad), None, "{bad}");
        }
        assert_eq!(rgb_to_hex([0, 10, 255]), "#000AFF");
    }

    #[test]
    fn every_byte_triple_should_round_trip_through_hsv() {
        // Every channel value on each axis, and a lattice of mixes.
        let mut seen = 0;
        for r in (0..=255u8).step_by(5) {
            for g in (0..=255u8).step_by(15) {
                for b in (0..=255u8).step_by(17) {
                    let hsv = rgb_to_hsv([r, g, b]);
                    assert_eq!(
                        hsv_to_rgb(hsv.h, hsv.s, hsv.v),
                        [r, g, b],
                        "{r} {g} {b} {hsv:?}"
                    );
                    seen += 1;
                }
            }
        }
        for x in 0..=255u8 {
            for rgb in [[x, 0, 0], [0, x, 0], [0, 0, x], [x, x, x], [255, x, 0]] {
                let hsv = rgb_to_hsv(rgb);
                assert_eq!(hsv_to_rgb(hsv.h, hsv.s, hsv.v), rgb);
            }
        }
        assert!(seen > 1000);
    }

    #[test]
    fn hex_to_hsv_and_back_should_keep_the_hex() {
        for hex in [
            "#000000", "#FFFFFF", "#99AAB5", "#3E9B4F", "#F472B6", "#C084FC", "#010203",
        ] {
            let hsv = hex_to_hsv(hex).expect(hex);
            assert_eq!(hsv_to_hex(hsv.h, hsv.s, hsv.v), hex);
        }
        assert_eq!(hex_to_hsv("nope"), None);
    }

    #[test]
    fn remember_should_put_the_pick_first_once_and_keep_eight() {
        let mut recents = Vec::new();
        assert!(remember(&mut recents, "#33ccff"));
        assert!(!remember(&mut recents, "33CCFF"), "already first");
        assert!(!remember(&mut recents, "not a colour"));
        assert_eq!(recents, ["#33CCFF"]);
        for hex in [
            "#000001", "#000002", "#000003", "#000004", "#000005", "#000006", "#000007", "#000008",
        ] {
            assert!(remember(&mut recents, hex));
        }
        assert_eq!(recents.len(), RECENT_COLORS);
        assert_eq!(recents[0], "#000008");
        assert!(
            !recents.contains(&"#33CCFF".to_owned()),
            "the oldest fell off"
        );
        assert!(remember(&mut recents, "#000003"));
        assert_eq!(recents[0], "#000003");
        assert_eq!(recents.iter().filter(|c| *c == "#000003").count(), 1);
        assert_eq!(recents.len(), RECENT_COLORS);
    }

    #[test]
    fn clean_recents_should_drop_junk_and_repeats() {
        let saved: Vec<String> = [
            "#abc", "nope", "#AABBCC", "#123456", "", "#000001", "#000002", "#000003", "#000004",
            "#000005", "#000006", "#000007",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let clean = clean_recents(&saved);
        assert_eq!(clean.len(), RECENT_COLORS);
        assert_eq!(&clean[..3], ["#AABBCC", "#123456", "#000001"]);
    }
}
