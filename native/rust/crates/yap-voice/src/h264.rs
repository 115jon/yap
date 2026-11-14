//! The hardware share encoder's pure parts (no Media Foundation, no WebRTC),
//! so `cargo test --no-default-features` covers them: H.264 bitstream
//! handling, which hardware encoder to use, and how the encoder follows
//! WebRTC's bandwidth estimate.
//!
//! The encoder itself is `engine/mf_h264.rs` (Windows, `libwebrtc`).

use std::borrow::Cow;

/// NAL unit types the share cares about (ITU-T H.264 table 7-1).
pub const NAL_IDR: u8 = 5;
pub const NAL_SPS: u8 = 7;
pub const NAL_PPS: u8 = 8;

/// Each NAL unit's payload in an Annex-B byte stream (start codes of three
/// or four bytes), in order. Bytes before the first start code are skipped.
pub fn nal_units(stream: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= stream.len() {
        if stream[i] == 0 && stream[i + 1] == 0 && stream[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    starts
        .iter()
        .enumerate()
        .map(|(n, &start)| {
            let end = starts.get(n + 1).map_or(stream.len(), |&next| {
                // The next start code, less the zero of a four-byte one.
                let code = next - 3;
                if code > start && stream[code - 1] == 0 {
                    code - 1
                } else {
                    code
                }
            });
            &stream[start..end.max(start)]
        })
        .collect()
}

/// The NAL unit types in an Annex-B stream, in order.
pub fn nal_types(stream: &[u8]) -> Vec<u8> {
    nal_units(stream)
        .into_iter()
        .filter_map(|nal| nal.first().map(|b| b & 0x1f))
        .collect()
}

/// The stream holds an IDR slice.
pub fn is_idr(stream: &[u8]) -> bool {
    nal_types(stream).contains(&NAL_IDR)
}

/// An encoder's output as Annex-B. Media Foundation's H.264 encoders emit
/// Annex-B already; one that hands out length-prefixed (AVCC, four-byte
/// lengths) units is rewritten. `None` when it is neither.
pub fn to_annex_b(data: &[u8]) -> Option<Cow<'_, [u8]>> {
    if data.starts_with(&[0, 0, 0, 1]) || data.starts_with(&[0, 0, 1]) {
        return Some(Cow::Borrowed(data));
    }
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut i = 0;
    while i + 4 <= data.len() {
        let len = u32::from_be_bytes([data[i], data[i + 1], data[i + 2], data[i + 3]]) as usize;
        i += 4;
        if len == 0 || i + len > data.len() {
            return None;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[i..i + len]);
        i += len;
    }
    (i == data.len() && !out.is_empty()).then_some(Cow::Owned(out))
}

/// A keyframe as sent: with the SPS and PPS in front when the encoder left
/// them out of the frame (some keep them only in the media type's sequence
/// header). A viewer joining at that keyframe needs both to decode it.
pub fn with_parameter_sets<'a>(frame: &'a [u8], sequence_header: &[u8]) -> Cow<'a, [u8]> {
    let types = nal_types(frame);
    if types.contains(&NAL_SPS) && types.contains(&NAL_PPS) || sequence_header.is_empty() {
        return Cow::Borrowed(frame);
    }
    let header = to_annex_b(sequence_header).unwrap_or(Cow::Borrowed(sequence_header));
    let mut out = Vec::with_capacity(header.len() + frame.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(frame);
    Cow::Owned(out)
}

/// A hardware encoder's vendor, from its PCI vendor id or its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Vendor {
    Nvidia,
    Amd,
    Intel,
    Other,
}

impl Vendor {
    /// From `MFT_ENUM_HARDWARE_VENDOR_ID_Attribute` (`VEN_10DE`, or bare
    /// hex) and the friendly name, the id winning.
    pub fn classify(vendor_id: Option<&str>, name: &str) -> Self {
        let id = vendor_id.and_then(|raw| {
            let raw = raw.trim();
            let hex = raw
                .strip_prefix("VEN_")
                .or_else(|| raw.strip_prefix("ven_"))
                .unwrap_or(raw);
            u32::from_str_radix(hex.trim(), 16).ok()
        });
        match id {
            Some(0x10de) => return Self::Nvidia,
            Some(0x1002 | 0x1022) => return Self::Amd,
            Some(0x8086) => return Self::Intel,
            _ => {}
        }
        let name = name.to_ascii_lowercase();
        if name.contains("nvidia") || name.contains("nvenc") {
            Self::Nvidia
        } else if name.contains("amd") || name.contains("amf") || name.contains("radeon") {
            Self::Amd
        } else if name.contains("intel") || name.contains("quick sync") || name.contains("qsv") {
            Self::Intel
        } else {
            Self::Other
        }
    }
}

/// The encoder to use among the enumerated hardware ones: the desktop app's
/// fixed order (NVENC, AMF, Quick Sync, then any other hardware MFT), the
/// enumeration order breaking ties.
pub fn pick_encoder(vendors: &[Vendor]) -> Option<usize> {
    vendors
        .iter()
        .enumerate()
        .min_by_key(|(index, vendor)| (**vendor, *index))
        .map(|(index, _)| index)
}

/// The lowest bitrate the encoder is ever set to (WebRTC's own floor for a
/// screencast is about this).
pub const MIN_BITRATE: u64 = 150_000;

/// A new target moves the encoder only when it differs this much (a
/// fraction of the current rate), so the estimate's small steps cost no
/// encoder calls.
const RATE_STEP: f64 = 0.08;

/// The bitrate the encoder should move to, if any: WebRTC's target capped
/// at the preset's maximum and floored at [`MIN_BITRATE`].
pub fn next_bitrate(current: u64, target: u64, max: u64) -> Option<u64> {
    let want = target.min(max).max(MIN_BITRATE.min(max));
    let moved = (want as f64 - current as f64).abs() / (current.max(1) as f64);
    (want != current && (moved >= RATE_STEP || want == max)).then_some(want)
}

/// Whether to encode frame `index` of a `detail` share. The encoder cannot
/// scale, so under a tight estimate a detail share keeps its resolution and
/// gives up frame rate (what `maintain-resolution` does in a browser):
/// below half the preset's bitrate, every other frame; below a quarter,
/// one in three. A `motion` share sends every frame and lets the quality go.
pub fn keep_frame(detail: bool, target: u64, max: u64, index: u64) -> bool {
    if !detail || max == 0 {
        return true;
    }
    let every = if target * 4 < max {
        3
    } else if target * 2 < max {
        2
    } else {
        1
    };
    index.is_multiple_of(every)
}

/// The smallest frame the hardware encoder is given: some vendors' MFTs
/// refuse to open below about this, so smaller frames (a tiny window) are
/// padded up to it ([`hardware_size`]).
pub const MIN_HARDWARE_SIZE: (u32, u32) = (320, 180);

/// The size a `w` x `h` frame is encoded at in hardware: at least
/// [`MIN_HARDWARE_SIZE`] on each side, and even (NV12).
pub fn hardware_size(w: u32, h: u32) -> (u32, u32) {
    let side = |n: u32, min: u32| n.max(min).next_multiple_of(2);
    (side(w, MIN_HARDWARE_SIZE.0), side(h, MIN_HARDWARE_SIZE.1))
}

/// Where an `inner` side sits centred in an `outer` one when padded, on
/// an even offset so the chroma lines up.
pub fn pad_offset(inner: u32, outer: u32) -> u32 {
    (outer.saturating_sub(inner) / 2) & !1
}

/// At most this many frames are tracked between `ProcessInput` and their
/// access unit (an MFT that drops frames never answers for them).
pub const IN_FLIGHT_MAX: usize = 32;

/// Frames handed to the encoder and not yet out, by sample time (100 ns),
/// oldest first, with what was noted at input (when, for the latency).
#[derive(Debug)]
pub struct InFlight<T> {
    queue: std::collections::VecDeque<(i64, T)>,
}

impl<T> Default for InFlight<T> {
    fn default() -> Self {
        Self {
            queue: std::collections::VecDeque::with_capacity(IN_FLIGHT_MAX),
        }
    }
}

impl<T> InFlight<T> {
    /// A frame went in at sample time `time`. The oldest is forgotten
    /// past [`IN_FLIGHT_MAX`].
    pub fn push(&mut self, time: i64, value: T) {
        if self.queue.len() >= IN_FLIGHT_MAX {
            self.queue.pop_front();
        }
        self.queue.push_back((time, value));
    }

    /// The input of an access unit with sample time `time` (`None`: the
    /// encoder gave none, so the oldest). Frames before it that never came
    /// out are dropped.
    pub fn take(&mut self, time: Option<i64>) -> Option<T> {
        let Some(time) = time else {
            return self.queue.pop_front().map(|(_, v)| v);
        };
        while self.queue.front().is_some_and(|(at, _)| *at < time) {
            self.queue.pop_front();
        }
        match self.queue.front() {
            Some((at, _)) if *at == time => self.queue.pop_front().map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

/// The order H.264 payloads are offered in: constrained baseline level 3.1
/// with packetization mode 1 first (what the desktop app and web peers
/// negotiate; every browser decodes it), then other mode-1 entries, then the
/// rest. `fmtp` is the codec's `a=fmtp` line.
pub fn h264_rank(fmtp: &str) -> u8 {
    let fmtp = fmtp.to_ascii_lowercase();
    let mode1 = fmtp.contains("packetization-mode=1");
    match (fmtp.contains("profile-level-id=42e01f"), mode1) {
        (true, true) => 0,
        (false, true) => 1,
        (true, false) => 2,
        (false, false) => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPS: &[u8] = &[0x67, 0x42, 0xe0, 0x1f];
    const PPS: &[u8] = &[0x68, 0xce, 0x3c, 0x80];
    const IDR: &[u8] = &[0x65, 0x88, 0x84];
    const P: &[u8] = &[0x41, 0x9a, 0x02];

    fn annex_b(units: &[&[u8]]) -> Vec<u8> {
        units
            .iter()
            .flat_map(|u| [&[0u8, 0, 0, 1][..], u].concat())
            .collect()
    }

    #[test]
    fn nal_units_should_split_on_three_and_four_byte_start_codes() {
        let mut stream = annex_b(&[SPS, PPS]);
        stream.extend_from_slice(&[0, 0, 1]);
        stream.extend_from_slice(IDR);
        assert_eq!(nal_units(&stream), vec![SPS, PPS, IDR]);
    }

    #[test]
    fn nal_types_should_read_the_low_five_bits() {
        assert_eq!(nal_types(&annex_b(&[SPS, PPS, IDR])), vec![7, 8, 5]);
    }

    #[test]
    fn a_delta_frame_should_not_be_idr() {
        assert!(!is_idr(&annex_b(&[P])));
        assert!(is_idr(&annex_b(&[IDR])));
    }

    #[test]
    fn length_prefixed_output_should_become_annex_b() {
        let avcc = [&[0, 0, 0, 4][..], SPS, &[0, 0, 0, 3], IDR].concat();
        assert_eq!(
            to_annex_b(&avcc).as_deref(),
            Some(annex_b(&[SPS, IDR]).as_slice())
        );
    }

    #[test]
    fn garbage_should_not_pass_as_annex_b() {
        assert_eq!(to_annex_b(&[0, 0, 0, 9, 1]), None);
    }

    #[test]
    fn a_keyframe_without_parameter_sets_should_get_the_header() {
        let header = annex_b(&[SPS, PPS]);
        let frame = annex_b(&[IDR]);
        assert_eq!(
            nal_types(&with_parameter_sets(&frame, &header)),
            vec![7, 8, 5]
        );
    }

    #[test]
    fn a_keyframe_with_parameter_sets_should_be_left_alone() {
        let frame = annex_b(&[SPS, PPS, IDR]);
        assert!(matches!(
            with_parameter_sets(&frame, &annex_b(&[SPS, PPS])),
            Cow::Borrowed(_)
        ));
    }

    #[test]
    fn vendor_id_should_win_over_the_name() {
        assert_eq!(
            Vendor::classify(Some("VEN_10DE"), "Generic"),
            Vendor::Nvidia
        );
        assert_eq!(Vendor::classify(Some("8086"), "NVIDIA"), Vendor::Intel);
        assert_eq!(Vendor::classify(None, "AMDh264Encoder"), Vendor::Amd);
        assert_eq!(Vendor::classify(None, "H264 Encoder MFT"), Vendor::Other);
    }

    #[test]
    fn pick_should_prefer_nvenc_then_amf_then_quick_sync() {
        assert_eq!(
            pick_encoder(&[Vendor::Intel, Vendor::Amd, Vendor::Nvidia]),
            Some(2)
        );
        assert_eq!(pick_encoder(&[Vendor::Other, Vendor::Intel]), Some(1));
        assert_eq!(pick_encoder(&[]), None);
    }

    #[test]
    fn small_estimate_steps_should_not_move_the_encoder() {
        assert_eq!(next_bitrate(4_000_000, 4_100_000, 8_000_000), None);
        assert_eq!(
            next_bitrate(4_000_000, 3_000_000, 8_000_000),
            Some(3_000_000)
        );
    }

    #[test]
    fn the_target_should_be_capped_by_the_preset_and_floored() {
        assert_eq!(
            next_bitrate(7_900_000, 20_000_000, 8_000_000),
            Some(8_000_000)
        );
        assert_eq!(
            next_bitrate(1_000_000, 10_000, 8_000_000),
            Some(MIN_BITRATE)
        );
    }

    #[test]
    fn detail_shares_should_drop_frames_under_a_tight_estimate() {
        let kept = |target| {
            (0..12)
                .filter(|&i| keep_frame(true, target, 8_000_000, i))
                .count()
        };
        assert_eq!(kept(8_000_000), 12);
        assert_eq!(kept(3_000_000), 6);
        assert_eq!(kept(1_000_000), 4);
    }

    #[test]
    fn motion_shares_should_keep_every_frame() {
        assert!((0..6).all(|i| keep_frame(false, 100_000, 8_000_000, i)));
    }

    #[test]
    fn constrained_baseline_mode_one_should_rank_first() {
        let cb = "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=42e01f";
        let high = "level-asymmetry-allowed=1;packetization-mode=1;profile-level-id=640c1f";
        let mode0 = "level-asymmetry-allowed=1;packetization-mode=0;profile-level-id=42e01f";
        assert!(h264_rank(cb) < h264_rank(high));
        assert!(h264_rank(high) < h264_rank(mode0));
    }

    #[test]
    fn a_tiny_window_should_be_padded_to_the_hardware_minimum() {
        assert_eq!(hardware_size(200, 100), (320, 180));
    }

    #[test]
    fn a_narrow_window_should_be_padded_on_its_short_side_only() {
        assert_eq!(hardware_size(1000, 120), (1000, 180));
    }

    #[test]
    fn a_large_frame_should_keep_its_size_in_hardware() {
        assert_eq!(hardware_size(1920, 1080), (1920, 1080));
    }

    #[test]
    fn hardware_sizes_should_be_even() {
        assert_eq!(hardware_size(641, 361), (642, 362));
    }

    #[test]
    fn padding_should_centre_on_an_even_offset() {
        assert_eq!((pad_offset(200, 320), pad_offset(98, 180)), (60, 40));
    }

    #[test]
    fn an_access_unit_should_take_its_own_input() {
        let mut queue = InFlight::default();
        queue.push(10, "a");
        queue.push(20, "b");
        assert_eq!(queue.take(Some(20)), Some("b"));
    }

    #[test]
    fn frames_the_encoder_dropped_should_be_forgotten() {
        let mut queue = InFlight::default();
        queue.push(10, "a");
        queue.push(20, "b");
        queue.take(Some(20));
        assert!(queue.is_empty());
    }

    #[test]
    fn in_flight_frames_should_be_capped() {
        let mut queue = InFlight::default();
        for t in 0..100 {
            queue.push(t, ());
        }
        assert_eq!(queue.len(), IN_FLIGHT_MAX);
    }

    #[test]
    fn an_access_unit_without_a_time_should_take_the_oldest() {
        let mut queue = InFlight::default();
        queue.push(10, "a");
        queue.push(20, "b");
        assert_eq!(queue.take(None), Some("a"));
    }
}
