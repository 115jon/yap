//! Camera simulcast: which layer a viewer pulls for the size it renders a
//! camera at. A port of the web's `src/lib/voice/video-layers.ts`.
//!
//! Cameras publish rids `f`, `h`, `q` (full, half, quarter), highest first.
//! A full-stage tile takes `f`, a grid tile `h` or `q`, a camera nothing
//! renders `q`.

use std::time::Duration;

/// One encoding our camera is pushed with (`CAMERA_SIMULCAST_ENCODINGS`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraEncoding {
    pub rid: &'static str,
    pub scale_down_by: f64,
    /// Bits per second; `None` keeps libwebrtc's default for the size.
    pub max_bitrate: Option<u64>,
    pub max_framerate: Option<f64>,
}

/// The camera's simulcast encodings, highest first, as the web pushes them:
/// the top layer keeps the encoder's default bitrate for the capture size,
/// the lower two are capped so a grid of tiles stays cheap.
pub const CAMERA_SIMULCAST: [CameraEncoding; 3] = [
    CameraEncoding {
        rid: "f",
        scale_down_by: 1.0,
        max_bitrate: None,
        max_framerate: None,
    },
    CameraEncoding {
        rid: "h",
        scale_down_by: 2.0,
        max_bitrate: Some(500_000),
        max_framerate: None,
    },
    CameraEncoding {
        rid: "q",
        scale_down_by: 4.0,
        max_bitrate: Some(150_000),
        max_framerate: Some(15.0),
    },
];

/// A simulcast layer, by quality.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    Low,
    Medium,
    High,
}

/// Up to this many source pixels wide the quarter layer is enough.
pub const LOW_LAYER_MAX_WIDTH: u32 = 360;
/// Up to this, the half layer.
pub const MEDIUM_LAYER_MAX_WIDTH: u32 = 800;

/// A move to a higher layer waits this long for the size to settle.
pub const LAYER_UPGRADE_DELAY: Duration = Duration::from_millis(150);
/// A move down waits longer: a tile that shrinks for a moment keeps its
/// sharp layer.
pub const LAYER_DOWNGRADE_DELAY: Duration = Duration::from_millis(1000);
/// After a switch with `priorityOrdering: "none"`, the bandwidth-aware
/// policy is restored this much later (partytracks `LAYER_SWITCH_SETTLE_MS`).
pub const LAYER_SWITCH_SETTLE: Duration = Duration::from_secs(2);

/// Device pixels a tile renders a camera at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RenderSize {
    pub width: u32,
    pub height: u32,
}

impl RenderSize {
    /// Source pixels wide this size needs (cameras are 16:9, so a tall box
    /// needs what its height implies).
    fn need(self) -> u32 {
        let by_height = u64::from(self.height) * 16 / 9;
        self.width.max(u32::try_from(by_height).unwrap_or(u32::MAX))
    }

    /// The larger of two sizes, by the source width each needs.
    pub fn larger(self, other: Self) -> Self {
        if other.need() > self.need() {
            other
        } else {
            self
        }
    }

    /// This size, scaled down (aspect kept) until it needs no more than
    /// `quality`'s layer; a size within it already is returned as it is.
    pub fn within(self, quality: Quality) -> Self {
        let most = match quality {
            Quality::Low => LOW_LAYER_MAX_WIDTH,
            Quality::Medium => MEDIUM_LAYER_MAX_WIDTH,
            Quality::High => return self,
        };
        let need = self.need();
        if need <= most {
            return self;
        }
        let scale =
            |n: u32| u32::try_from(u64::from(n) * u64::from(most) / u64::from(need)).unwrap_or(0);
        Self {
            width: scale(self.width),
            height: scale(self.height),
        }
    }
}

/// The layer a camera needs for the largest size it is rendered at; one
/// nothing renders (or a 0x0 box) gets the low layer.
pub fn resolve_quality(size: Option<RenderSize>) -> Quality {
    match size {
        Some(s) if s.width > 0 && s.height > 0 => match s.need() {
            0..=LOW_LAYER_MAX_WIDTH => Quality::Low,
            w if w <= MEDIUM_LAYER_MAX_WIDTH => Quality::Medium,
            _ => Quality::High,
        },
        _ => Quality::Low,
    }
}

/// The rid to pull for `quality` from the announced rids (highest first);
/// `None` for a publication without simulcast.
pub fn select_rid(quality: Quality, rids: &[String]) -> Option<String> {
    if rids.len() < 2 {
        return None;
    }
    let at = match quality {
        Quality::High => 0,
        Quality::Low => rids.len() - 1,
        Quality::Medium => (rids.len() - 1) / 2,
    };
    rids.get(at).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: u32, height: u32) -> Option<RenderSize> {
        Some(RenderSize { width, height })
    }

    fn rids() -> Vec<String> {
        ["f", "h", "q"].map(str::to_owned).to_vec()
    }

    #[test]
    fn nothing_rendered_should_take_the_low_layer() {
        assert_eq!(resolve_quality(None), Quality::Low);
        assert_eq!(resolve_quality(size(0, 0)), Quality::Low);
    }

    #[test]
    fn tile_sizes_should_map_to_layers_like_the_web() {
        assert_eq!(resolve_quality(size(320, 180)), Quality::Low);
        assert_eq!(resolve_quality(size(640, 360)), Quality::Medium);
        assert_eq!(resolve_quality(size(1280, 720)), Quality::High);
        // A tall box needs its height's worth of 16:9 width.
        assert_eq!(resolve_quality(size(300, 500)), Quality::High);
    }

    #[test]
    fn select_rid_should_pick_top_middle_and_bottom() {
        assert_eq!(select_rid(Quality::High, &rids()).as_deref(), Some("f"));
        assert_eq!(select_rid(Quality::Medium, &rids()).as_deref(), Some("h"));
        assert_eq!(select_rid(Quality::Low, &rids()).as_deref(), Some("q"));
    }

    #[test]
    fn single_layer_should_have_no_rid() {
        assert_eq!(select_rid(Quality::High, &["f".to_owned()]), None);
        assert_eq!(select_rid(Quality::Low, &[]), None);
    }

    #[test]
    fn within_should_cap_a_size_at_the_layer_and_keep_smaller_ones() {
        let big = RenderSize {
            width: 1280,
            height: 720,
        };
        assert_eq!(big.within(Quality::High), big);
        assert_eq!(
            resolve_quality(Some(big.within(Quality::Medium))),
            Quality::Medium
        );
        assert_eq!(
            resolve_quality(Some(big.within(Quality::Low))),
            Quality::Low
        );
        let small = RenderSize {
            width: 320,
            height: 180,
        };
        assert_eq!(small.within(Quality::Low), small);
        // A tall box is capped by what its height needs.
        let tall = RenderSize {
            width: 300,
            height: 600,
        };
        assert_eq!(
            resolve_quality(Some(tall.within(Quality::Medium))),
            Quality::Medium
        );
    }

    #[test]
    fn larger_should_compare_by_needed_width() {
        let wide = RenderSize {
            width: 800,
            height: 100,
        };
        let tall = RenderSize {
            width: 300,
            height: 600,
        };
        assert_eq!(wide.larger(tall), tall);
    }
}
