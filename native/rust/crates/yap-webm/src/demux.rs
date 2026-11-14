//! A WebM file as one video track's packets.
//!
//! The file is read from memory in one pass. Every packet of the first VP8
//! or VP9 track is kept (compressed), with its presentation time and, when
//! the track carries alpha, the alpha frame from the block's
//! `BlockAdditional` (id 1). That is how browsers read transparent WebM:
//! the side stream is a second VP8/VP9 stream whose luma is the alpha.
//! Other tracks (audio, subtitles) are skipped.

use std::io::Cursor;

use webm_iterable::matroska_spec::{
    Block, EbmlSpecification, Master, MatroskaSpec, SimpleBlock, TagDataType,
};
use webm_iterable::WebmIterator;

use crate::WebmError;

/// Most packets a file may carry (about 55 minutes at 30 fps).
pub const MAX_PACKETS: usize = 100_000;
/// The longest clip played, in milliseconds (one hour).
pub const MAX_DURATION_MS: u64 = 60 * 60 * 1000;

/// The video codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Vp8,
    Vp9,
}

/// One compressed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub pts_ms: u64,
    pub keyframe: bool,
    pub data: Vec<u8>,
    /// The alpha frame, when the track has alpha.
    pub alpha: Option<Vec<u8>>,
}

/// The video track of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Video {
    pub codec: Codec,
    pub width: u32,
    pub height: u32,
    /// `AlphaMode` 1: the packets carry alpha frames.
    pub alpha: bool,
    pub duration_ms: u64,
    /// In file order (presentation order for VP8/VP9).
    pub packets: Vec<Packet>,
}

impl Video {
    /// The packet to start decoding from to show `pts_ms`: the last
    /// keyframe at or before it.
    pub fn keyframe_before(&self, pts_ms: u64) -> usize {
        self.packets
            .iter()
            .enumerate()
            .take_while(|(_, p)| p.pts_ms <= pts_ms)
            .filter(|(_, p)| p.keyframe)
            .map(|(i, _)| i)
            .last()
            .unwrap_or(0)
    }
}

#[derive(Debug, Default)]
struct Track {
    number: u64,
    codec: Option<Codec>,
    width: u32,
    height: u32,
    alpha: bool,
    frame_ns: u64,
}

fn children(tag: &MatroskaSpec) -> &[MatroskaSpec] {
    match tag {
        MatroskaSpec::TrackEntry(Master::Full(c))
        | MatroskaSpec::Video(Master::Full(c))
        | MatroskaSpec::BlockGroup(Master::Full(c))
        | MatroskaSpec::BlockAdditions(Master::Full(c))
        | MatroskaSpec::BlockMore(Master::Full(c)) => c,
        _ => &[],
    }
}

fn track_of(entry: &MatroskaSpec) -> Track {
    let mut track = Track::default();
    let mut video = false;
    for tag in children(entry) {
        match tag {
            MatroskaSpec::TrackNumber(n) => track.number = *n,
            MatroskaSpec::TrackType(t) => video = *t == 1,
            MatroskaSpec::CodecID(id) => {
                track.codec = match id.as_str() {
                    "V_VP8" => Some(Codec::Vp8),
                    "V_VP9" => Some(Codec::Vp9),
                    _ => None,
                }
            }
            MatroskaSpec::DefaultDuration(ns) => track.frame_ns = *ns,
            MatroskaSpec::Video(_) => {
                for v in children(tag) {
                    match v {
                        MatroskaSpec::PixelWidth(w) => {
                            track.width = u32::try_from(*w).unwrap_or(0);
                        }
                        MatroskaSpec::PixelHeight(h) => {
                            track.height = u32::try_from(*h).unwrap_or(0);
                        }
                        MatroskaSpec::AlphaMode(mode) => track.alpha = *mode == 1,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    if !video {
        track.codec = None;
    }
    track
}

/// The alpha frame of a block group: `BlockAdditional` with id 1 (the
/// default when `BlockAddID` is absent).
fn alpha_of(group: &MatroskaSpec) -> Option<Vec<u8>> {
    let additions = children(group)
        .iter()
        .find(|t| matches!(t, MatroskaSpec::BlockAdditions(_)))?;
    children(additions).iter().find_map(|more| {
        let fields = children(more);
        let id = fields
            .iter()
            .find_map(|f| match f {
                MatroskaSpec::BlockAddID(id) => Some(*id),
                _ => None,
            })
            .unwrap_or(1);
        let data = fields.iter().find_map(|f| match f {
            MatroskaSpec::BlockAdditional(data) => Some(data.clone()),
            _ => None,
        })?;
        (id == 1 && !data.is_empty()).then_some(data)
    })
}

fn pts_ms(cluster: u64, relative: i16, scale_ns: u64) -> u64 {
    let ticks = i128::from(cluster) + i128::from(relative);
    let ns = ticks.max(0) * i128::from(scale_ns);
    u64::try_from(ns / 1_000_000).unwrap_or(u64::MAX)
}

/// The cluster id, and the elements that end an open unknown-size cluster
/// (the next cluster or any other top-level element of a segment).
const CLUSTER_ID: u64 = 0x1F43_B675;
const SEGMENT_LEVEL: [u64; 8] = [
    0x114D_9B74, // SeekHead
    0x1549_A966, // Info
    0x1654_AE6B, // Tracks
    0x1C53_BB6B, // Cues
    0x1043_A770, // Chapters
    0x1254_C367, // Tags
    0x1941_A469, // Attachments
    CLUSTER_ID,
];
/// Masters nested deeper than this are not walked.
const MAX_DEPTH: usize = 24;

/// An element id: 1 to 4 bytes, marker bits kept.
fn read_id(bytes: &[u8]) -> Option<(u64, usize)> {
    let len = bytes.first()?.leading_zeros() as usize + 1;
    if len > 4 || bytes.len() < len {
        return None;
    }
    let id = bytes[..len]
        .iter()
        .fold(0u64, |id, b| (id << 8) | u64::from(*b));
    Some((id, len))
}

/// An element size: 1 to 8 bytes; `None` for the "unknown" value.
fn read_size(bytes: &[u8]) -> Option<(Option<u64>, usize)> {
    let first = *bytes.first()?;
    if first == 0 {
        return None;
    }
    let len = first.leading_zeros() as usize + 1;
    if bytes.len() < len {
        return None;
    }
    let mut value = u64::from(first) & (0xFF >> len);
    for b in &bytes[1..len] {
        value = (value << 8) | u64::from(*b);
    }
    let unknown = value == (1u64 << (7 * len)) - 1;
    Some(((!unknown).then_some(value), len))
}

struct Open {
    id: u64,
    end: usize,
    unknown: bool,
}

/// How much of `bytes` is safe to hand to the EBML reader. `ebml-iterable`
/// allocates an element's declared size before it checks the data is
/// there, so a size of 2^40 aborts the process. This walks the element
/// headers (id, size) the way the reader will and stops at the first one
/// that is malformed: a size that overruns its parent or the buffer, an
/// unknown size on a plain element, or a header cut short. What lies
/// before it is whole. A master (segment, cluster) cut by the end of the
/// file is kept: the reader streams masters instead of buffering their
/// size.
fn safe_prefix(bytes: &[u8]) -> usize {
    let total = bytes.len();
    let mut stack: Vec<Open> = Vec::new();
    let mut pos = 0usize;
    while pos < total {
        while stack.last().is_some_and(|m| pos >= m.end) {
            stack.pop();
        }
        let limit = stack.last().map_or(total, |m| m.end);
        let window = &bytes[pos..limit];
        let Some((id, id_len)) = read_id(window) else {
            return pos;
        };
        let Some((size, size_len)) = read_size(&window[id_len..]) else {
            return pos;
        };
        // An unknown-size cluster ends where the next top-level element
        // starts.
        while stack
            .last()
            .is_some_and(|m| m.unknown && m.id == CLUSTER_ID && SEGMENT_LEVEL.contains(&id))
        {
            stack.pop();
        }
        let limit = stack.last().map_or(total, |m| m.end);
        let body = pos + id_len + size_len;
        if body > limit {
            return pos;
        }
        let master = MatroskaSpec::get_tag_data_type(id) == Some(TagDataType::Master);
        match size {
            None if master && stack.len() < MAX_DEPTH => {
                stack.push(Open {
                    id,
                    end: limit,
                    unknown: true,
                });
                pos = body;
            }
            None => return pos,
            Some(size) => {
                let Some(end) = usize::try_from(size)
                    .ok()
                    .and_then(|size| body.checked_add(size))
                else {
                    return pos;
                };
                if master {
                    // Cut by the end of the file, not by a parent: keep it.
                    let end = if end > limit && limit == total {
                        total
                    } else {
                        end
                    };
                    if end > limit || stack.len() >= MAX_DEPTH {
                        return pos;
                    }
                    stack.push(Open {
                        id,
                        end,
                        unknown: false,
                    });
                    pos = body;
                } else if end > limit {
                    return pos;
                } else {
                    pos = end;
                }
            }
        }
    }
    total
}

/// Adds a packet, refusing a file with too many or one too far in time.
fn keep(packets: &mut Vec<Packet>, packet: Packet) -> Result<(), WebmError> {
    if packets.len() >= MAX_PACKETS {
        return Err(WebmError::Parse("too many frames".into()));
    }
    if packet.pts_ms > MAX_DURATION_MS {
        return Err(WebmError::Parse("clip too long".into()));
    }
    packets.push(packet);
    Ok(())
}

/// Reads the first VP8/VP9 track of a WebM (or Matroska) file.
pub fn demux(bytes: &[u8]) -> Result<Video, WebmError> {
    let bytes = &bytes[..safe_prefix(bytes)];
    let tags = WebmIterator::new(
        Cursor::new(bytes),
        &[
            MatroskaSpec::TrackEntry(Master::Start),
            MatroskaSpec::BlockGroup(Master::Start),
        ],
    );
    let mut scale_ns: u64 = 1_000_000;
    let mut duration_ticks: Option<f64> = None;
    let mut cluster: u64 = 0;
    let mut track: Option<Track> = None;
    let mut packets: Vec<Packet> = Vec::new();
    let mut saw_ebml = false;
    for tag in tags {
        let tag = match tag {
            Ok(tag) => tag,
            // A truncated tail still plays what came before it.
            Err(error) if !packets.is_empty() => {
                let _ = error;
                break;
            }
            Err(error) => return Err(WebmError::Parse(error.to_string())),
        };
        match &tag {
            MatroskaSpec::Ebml(_) => saw_ebml = true,
            MatroskaSpec::TimestampScale(s) if *s > 0 => scale_ns = *s,
            MatroskaSpec::Duration(d) => duration_ticks = Some(*d),
            MatroskaSpec::TrackEntry(Master::Full(_)) => {
                let entry = track_of(&tag);
                if track.is_none() && entry.codec.is_some() {
                    track = Some(entry);
                }
            }
            MatroskaSpec::Timestamp(t) => cluster = *t,
            MatroskaSpec::SimpleBlock(_) => {
                let Some(t) = track.as_ref() else { continue };
                let Ok(block) = SimpleBlock::try_from(&tag) else {
                    continue;
                };
                if block.track != t.number {
                    continue;
                }
                let pts = pts_ms(cluster, block.timestamp, scale_ns);
                let Ok(frames) = block.read_frame_data() else {
                    continue;
                };
                for frame in frames {
                    keep(
                        &mut packets,
                        Packet {
                            pts_ms: pts,
                            keyframe: block.keyframe,
                            data: frame.data.to_vec(),
                            alpha: None,
                        },
                    )?;
                }
            }
            MatroskaSpec::BlockGroup(Master::Full(group)) => {
                let Some(t) = track.as_ref() else { continue };
                let Some(raw) = group.iter().find(|c| matches!(c, MatroskaSpec::Block(_))) else {
                    continue;
                };
                let Ok(block) = Block::try_from(raw) else {
                    continue;
                };
                if block.track != t.number {
                    continue;
                }
                let keyframe = !group
                    .iter()
                    .any(|c| matches!(c, MatroskaSpec::ReferenceBlock(_)));
                let pts = pts_ms(cluster, block.timestamp, scale_ns);
                let alpha = if t.alpha { alpha_of(&tag) } else { None };
                let Ok(frames) = block.read_frame_data() else {
                    continue;
                };
                for frame in frames {
                    keep(
                        &mut packets,
                        Packet {
                            pts_ms: pts,
                            keyframe,
                            data: frame.data.to_vec(),
                            alpha: alpha.clone(),
                        },
                    )?;
                }
            }
            _ => {}
        }
    }
    if !saw_ebml {
        return Err(WebmError::Parse("no EBML header".into()));
    }
    let track = track.ok_or(WebmError::NoVideo)?;
    let codec = track.codec.ok_or(WebmError::NoVideo)?;
    if packets.is_empty() {
        return Err(WebmError::NoVideo);
    }
    // The first packet is where decoding starts, whatever the flags say.
    packets[0].keyframe = true;
    let last = packets.iter().map(|p| p.pts_ms).max().unwrap_or(0);
    let frame_ms = if track.frame_ns > 0 {
        track.frame_ns / 1_000_000
    } else {
        // Two packets apart, else a 30 fps frame.
        packets
            .windows(2)
            .map(|w| w[1].pts_ms.saturating_sub(w[0].pts_ms))
            .find(|d| *d > 0)
            .unwrap_or(33)
    };
    // A default duration under a millisecond is still a frame.
    let frame_ms = frame_ms.max(1);
    let declared = duration_ticks
        .filter(|d| d.is_finite() && *d > 0.0)
        .map(|d| (d * scale_ns as f64 / 1_000_000.0) as u64);
    let duration_ms = declared.unwrap_or(0).max(last + frame_ms);
    if duration_ms > MAX_DURATION_MS {
        return Err(WebmError::Parse("clip too long".into()));
    }
    Ok(Video {
        codec,
        width: track.width,
        height: track.height,
        alpha: track.alpha,
        duration_ms,
        packets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALPHA_VP9: &[u8] = include_bytes!("../tests/fixtures/alpha-vp9.webm");
    const OPAQUE_VP8: &[u8] = include_bytes!("../tests/fixtures/opaque-vp8.webm");

    #[test]
    fn a_vp9_nameplate_should_carry_its_alpha_frames() {
        let video = demux(ALPHA_VP9).expect("demux");
        assert_eq!(
            (video.codec, video.width, video.height, video.alpha),
            (Codec::Vp9, 64, 32, true)
        );
        assert!(video.packets.iter().all(|p| p.alpha.is_some()));
    }

    #[test]
    fn every_frame_should_be_read_in_order() {
        let video = demux(OPAQUE_VP8).expect("demux");
        let pts: Vec<u64> = video.packets.iter().map(|p| p.pts_ms).collect();
        assert_eq!(pts, vec![0, 100, 200, 300, 400, 500]);
    }

    #[test]
    fn an_opaque_track_should_have_no_alpha() {
        let video = demux(OPAQUE_VP8).expect("demux");
        assert!(!video.alpha && video.packets.iter().all(|p| p.alpha.is_none()));
    }

    #[test]
    fn the_duration_should_cover_the_last_frame() {
        let video = demux(OPAQUE_VP8).expect("demux");
        assert_eq!(video.duration_ms, 600);
    }

    #[test]
    fn a_seek_should_start_at_the_keyframe_before_it() {
        let video = demux(OPAQUE_VP8).expect("demux");
        // -g 3: keyframes at 0 and 300 ms.
        assert_eq!(video.keyframe_before(450), 3);
    }

    #[test]
    fn not_a_webm_should_fail_to_parse() {
        assert!(matches!(
            demux(b"\x89PNG\r\n\x1a\nnot a webm"),
            Err(WebmError::Parse(_))
        ));
    }

    #[test]
    fn a_truncated_file_should_keep_the_frames_before_the_cut() {
        let cut = &OPAQUE_VP8[..OPAQUE_VP8.len() * 2 / 3];
        let video = demux(cut).expect("demux");
        assert!(!video.packets.is_empty() && video.packets.len() < 6);
    }

    // A small EBML writer: every size in the 8-byte form.
    fn el(id: &[u8], body: &[u8]) -> Vec<u8> {
        sized(id, body.len() as u64, body)
    }

    fn sized(id: &[u8], size: u64, body: &[u8]) -> Vec<u8> {
        let mut out = id.to_vec();
        out.push(0x01);
        out.extend_from_slice(&size.to_be_bytes()[1..]);
        out.extend_from_slice(body);
        out
    }

    fn uint(id: &[u8], value: u64) -> Vec<u8> {
        el(id, &value.to_be_bytes())
    }

    fn header() -> Vec<u8> {
        el(&[0x1A, 0x45, 0xDF, 0xA3], &el(&[0x42, 0x82], b"webm"))
    }

    fn tracks(default_duration_ns: u64) -> Vec<u8> {
        let mut entry = uint(&[0xD7], 1);
        entry.extend(uint(&[0x83], 1));
        entry.extend(el(&[0x86], b"V_VP8"));
        if default_duration_ns > 0 {
            entry.extend(uint(&[0x23, 0xE3, 0x83], default_duration_ns));
        }
        entry.extend(el(
            &[0xE0],
            &[uint(&[0xB0], 64), uint(&[0xBA], 48)].concat(),
        ));
        el(&[0x16, 0x54, 0xAE, 0x6B], &el(&[0xAE], &entry))
    }

    fn info(duration_ms: Option<f64>) -> Vec<u8> {
        let mut body = uint(&[0x2A, 0xD7, 0xB1], 1_000_000);
        if let Some(d) = duration_ms {
            body.extend(el(&[0x44, 0x89], &d.to_be_bytes()));
        }
        el(&[0x15, 0x49, 0xA9, 0x66], &body)
    }

    /// `count` one-byte frames, all at the cluster's time.
    fn cluster(count: usize) -> Vec<u8> {
        let mut body = uint(&[0xE7], 0);
        for _ in 0..count {
            // track 1, time 0, keyframe flag, one byte of data.
            body.extend(el(&[0xA3], &[0x81, 0, 0, 0x80, 0]));
        }
        el(&[0x1F, 0x43, 0xB6, 0x75], &body)
    }

    fn file(parts: &[Vec<u8>]) -> Vec<u8> {
        let mut out = header();
        out.extend(el(&[0x18, 0x53, 0x80, 0x67], &parts.concat()));
        out
    }

    #[test]
    fn a_built_file_should_demux() {
        let video = demux(&file(&[info(None), tracks(0), cluster(2)])).expect("demux");
        assert_eq!((video.packets.len(), video.width), (2, 64));
    }

    #[test]
    fn a_huge_declared_size_should_fail_without_allocating_it() {
        // Info holds a TimestampScale whose size says 2^40 bytes: the
        // reader would allocate that before noticing the file ends.
        let evil = sized(&[0x2A, 0xD7, 0xB1], 1 << 40, &[1]);
        let bytes = file(&[el(&[0x15, 0x49, 0xA9, 0x66], &evil), tracks(0), cluster(2)]);
        assert!(demux(&bytes).is_err());
        // The same size under 4 GB (the reader's own limit), as the
        // segment's first element.
        let evil = sized(&[0x4D, 0x80], 3_000_000_000, &[1]);
        assert!(demux(&file(&[evil, tracks(0), cluster(2)])).is_err());
    }

    #[test]
    fn a_huge_size_after_good_frames_should_keep_the_frames() {
        let mut parts = vec![info(None), tracks(0), cluster(2)];
        parts.push(sized(&[0xEC], 1 << 40, &[0]));
        let video = demux(&file(&parts)).expect("demux");
        assert_eq!(video.packets.len(), 2);
    }

    #[test]
    fn a_child_larger_than_its_parent_should_be_cut() {
        // The cluster says 100 bytes, its first child says 1000.
        let child = sized(&[0xE7], 1000, &[0; 1000]);
        let cluster = sized(&[0x1F, 0x43, 0xB6, 0x75], 100, &child);
        // Only the cluster's own 12-byte header is kept.
        assert_eq!(safe_prefix(&cluster), 12);
    }

    #[test]
    fn an_unknown_size_should_end_a_cluster_at_the_next_one() {
        let mut one = vec![0x1F, 0x43, 0xB6, 0x75, 0xFF];
        one.extend(uint(&[0xE7], 0));
        // Far more than MAX_DEPTH of them: each closes the one before.
        let many = one.repeat(100);
        assert_eq!(safe_prefix(&many), many.len());
    }

    #[test]
    fn a_file_cut_inside_its_tracks_should_fail() {
        let whole = file(&[info(None), tracks(0), cluster(2)]);
        let at = whole.len() - cluster(2).len() - 20;
        assert!(demux(&whole[..at]).is_err());
    }

    #[test]
    fn garbage_should_fail_fast() {
        for bytes in [
            &[0u8; 64][..],
            &[0xFF; 64][..],
            &[0x1A, 0x45, 0xDF, 0xA3, 0xFF, 0xFF][..],
            &[
                0x1A, 0x45, 0xDF, 0xA3, 0x01, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFE,
            ][..],
            &[][..],
        ] {
            assert!(demux(bytes).is_err(), "{bytes:?}");
        }
    }

    #[test]
    fn a_sub_millisecond_frame_should_still_last_a_millisecond() {
        let bytes = file(&[info(None), tracks(500_000), cluster(1)]);
        assert!(demux(&bytes).expect("demux").duration_ms >= 1);
    }

    #[test]
    fn a_declared_duration_should_not_shorten_the_last_frame() {
        let bytes = file(&[info(Some(0.4)), tracks(0), cluster(1)]);
        assert!(demux(&bytes).expect("demux").duration_ms >= 33);
    }

    #[test]
    fn too_many_frames_should_be_refused() {
        let bytes = file(&[info(None), tracks(0), cluster(MAX_PACKETS + 1)]);
        assert!(matches!(demux(&bytes), Err(WebmError::Parse(_))));
        let bytes = file(&[info(None), tracks(0), cluster(MAX_PACKETS)]);
        assert_eq!(demux(&bytes).expect("demux").packets.len(), MAX_PACKETS);
    }

    #[test]
    fn a_clip_over_an_hour_should_be_refused() {
        let long = (MAX_DURATION_MS + 1000) as f64;
        let bytes = file(&[info(Some(long)), tracks(0), cluster(1)]);
        assert!(matches!(demux(&bytes), Err(WebmError::Parse(_))));
    }
}
