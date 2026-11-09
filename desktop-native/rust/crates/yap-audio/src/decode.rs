//! Demuxing and decoding with symphonia (Opus through `crate::opus`), to
//! stereo interleaved `f32`.

use std::io::Cursor;
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, DecoderOptions, CODEC_TYPE_NULL, CODEC_TYPE_OPUS};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, Packet, Track};
use symphonia::core::io::{MediaSource, MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::dsp::push_stereo;
use crate::opus::OpusHead;
use crate::resample::{resample_clip, valid_rate};

/// Longest clip decoded into memory, in seconds (soundboard scale: a clip
/// is a few seconds; 60 s is generous).
const MAX_CLIP_SECS: u64 = 60;

/// The most decoded frames kept, whatever the source rate: 60 s at 48 kHz
/// (23 MB of stereo `f32`). A higher source rate is cut shorter (a clip
/// at 192 kHz keeps 15 s).
const MAX_CLIP_FRAMES: u64 = MAX_CLIP_SECS * crate::SAMPLE_RATE as u64;

/// A symphonia hint from an extension (`mp3`, `.m4a`) or a MIME type
/// (`audio/webm; codecs="opus"`).
pub(crate) fn hint(text: Option<&str>) -> Hint {
    let mut hint = Hint::new();
    if let Some(text) = text.map(str::trim).filter(|text| !text.is_empty()) {
        if text.contains('/') {
            let mime = text
                .split(';')
                .next()
                .unwrap_or(text)
                .trim()
                .to_ascii_lowercase();
            hint.mime_type(&mime);
            if let Some(extension) = extension_for_mime(&mime) {
                hint.with_extension(extension);
            }
        } else {
            hint.with_extension(&text.trim_start_matches('.').to_ascii_lowercase());
        }
    }
    hint
}

fn extension_for_mime(mime: &str) -> Option<&'static str> {
    Some(match mime {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" | "audio/x-m4a" | "audio/aac" | "video/mp4" => "m4a",
        "audio/ogg" | "application/ogg" | "audio/opus" => "ogg",
        "audio/webm" | "video/webm" | "audio/x-matroska" => "webm",
        "audio/wav" | "audio/x-wav" | "audio/wave" | "audio/vnd.wave" => "wav",
        "audio/flac" | "audio/x-flac" => "flac",
        _ => return None,
    })
}

/// Probes `source` and picks its audio track.
pub(crate) fn open(
    source: Box<dyn MediaSource>,
    hint: &Hint,
) -> Result<(Box<dyn FormatReader>, Track), SymphoniaError> {
    let stream = MediaSourceStream::new(source, MediaSourceStreamOptions::default());
    let format_options = FormatOptions {
        enable_gapless: true,
        ..FormatOptions::default()
    };
    let probed = symphonia::default::get_probe().format(
        hint,
        stream,
        &format_options,
        &MetadataOptions::default(),
    )?;
    let format = probed.format;
    let track = format
        .default_track()
        .filter(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        .or_else(|| {
            format
                .tracks()
                .iter()
                .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
        })
        .cloned()
        .ok_or(SymphoniaError::Unsupported("no audio track"))?;
    Ok((format, track))
}

/// Why a packet did not decode.
#[derive(Debug)]
pub(crate) enum PacketError {
    /// A corrupt packet; skip it.
    Skip(String),
    /// The decoder cannot go on.
    Fatal(String),
}

enum Kind {
    Symphonia {
        decoder: Box<dyn symphonia::core::codecs::Decoder>,
        buffer: Option<SampleBuffer<f32>>,
    },
    #[cfg(feature = "libopus")]
    Opus {
        decoder: crate::opus::Decoder,
        scratch: Vec<f32>,
    },
}

/// One track's decoder, producing stereo interleaved samples at the
/// track's own rate.
pub(crate) struct PacketDecoder {
    kind: Kind,
    name: &'static str,
}

impl PacketDecoder {
    /// Err: the codec is not supported (in this build).
    pub(crate) fn new(params: &CodecParameters) -> Result<Self, String> {
        if params.codec == CODEC_TYPE_OPUS {
            return Self::new_opus(params);
        }
        let name = symphonia::default::get_codecs()
            .get_codec(params.codec)
            .map_or("unknown", |descriptor| descriptor.short_name);
        let decoder = symphonia::default::get_codecs()
            .make(params, &DecoderOptions::default())
            .map_err(|error| format!("{name}: {error}"))?;
        Ok(Self {
            kind: Kind::Symphonia {
                decoder,
                buffer: None,
            },
            name,
        })
    }

    #[cfg(feature = "libopus")]
    fn new_opus(params: &CodecParameters) -> Result<Self, String> {
        let head = params
            .extra_data
            .as_deref()
            .and_then(OpusHead::parse)
            .unwrap_or(OpusHead {
                channels: params.channels.map_or(2, |channels| channels.count()),
                pre_skip: params.delay.map_or(0, |delay| delay as usize),
            });
        Ok(Self {
            kind: Kind::Opus {
                decoder: crate::opus::Decoder::new(head)?,
                scratch: Vec::new(),
            },
            name: "opus",
        })
    }

    #[cfg(not(feature = "libopus"))]
    fn new_opus(params: &CodecParameters) -> Result<Self, String> {
        let _ = params.extra_data.as_deref().and_then(OpusHead::parse);
        Err(crate::opus::UNSUPPORTED.to_string())
    }

    pub(crate) fn name(&self) -> &'static str {
        self.name
    }

    /// Decodes `packet`, appending stereo frames to `out`. Returns the rate.
    pub(crate) fn decode(
        &mut self,
        packet: &Packet,
        out: &mut Vec<f32>,
    ) -> Result<u32, PacketError> {
        match &mut self.kind {
            Kind::Symphonia { decoder, buffer } => {
                let decoded = match decoder.decode(packet) {
                    Ok(decoded) => decoded,
                    Err(SymphoniaError::DecodeError(error)) => {
                        return Err(PacketError::Skip(error.to_string()))
                    }
                    Err(error) => return Err(PacketError::Fatal(error.to_string())),
                };
                let spec = *decoded.spec();
                if decoded.frames() == 0 {
                    return Ok(spec.rate);
                }
                let fits = buffer.as_ref().is_some_and(|buffer| {
                    buffer.capacity() >= decoded.capacity() * spec.channels.count()
                });
                if !fits {
                    *buffer = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
                }
                if let Some(buffer) = buffer {
                    buffer.copy_interleaved_ref(decoded);
                    push_stereo(buffer.samples(), spec.channels.count(), out);
                }
                Ok(spec.rate)
            }
            #[cfg(feature = "libopus")]
            Kind::Opus { decoder, scratch } => {
                scratch.clear();
                let channels = decoder
                    .decode(&packet.data, scratch)
                    .map_err(PacketError::Skip)?;
                push_stereo(scratch, channels, out);
                Ok(crate::SAMPLE_RATE)
            }
        }
    }

    /// Clears the decoder's state (after a seek).
    pub(crate) fn reset(&mut self) -> Result<(), String> {
        match &mut self.kind {
            Kind::Symphonia { decoder, .. } => {
                decoder.reset();
                Ok(())
            }
            #[cfg(feature = "libopus")]
            Kind::Opus { decoder, .. } => decoder.reset(),
        }
    }
}

/// An in-memory clip at 48 kHz stereo.
#[derive(Debug)]
pub(crate) struct DecodedClip {
    pub samples: Vec<f32>,
    pub codec: &'static str,
}

/// Decodes and resamples a whole clip.
pub(crate) fn decode_clip(
    bytes: Arc<[u8]>,
    hint_text: Option<&str>,
) -> Result<DecodedClip, String> {
    let (mut format, track) = open(Box::new(Cursor::new(bytes)), &hint(hint_text))
        .map_err(|error| format!("unsupported audio: {error}"))?;
    // A header declaring a silly rate would make the length cap meaningless
    // (and the resampler build huge FFTs): refuse it before decoding.
    if let Some(declared) = track.codec_params.sample_rate {
        if !valid_rate(declared) {
            return Err(format!("unsupported sample rate {declared} Hz"));
        }
    }
    let mut decoder = PacketDecoder::new(&track.codec_params)?;
    let mut samples = Vec::new();
    let mut rate = track.codec_params.sample_rate.unwrap_or(crate::SAMPLE_RATE);
    let mut skipped = 0usize;
    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error))
                if error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(error) => {
                if samples.is_empty() {
                    return Err(format!("{}: {error}", decoder.name()));
                }
                // A truncated tail: play what decoded.
                break;
            }
        };
        if packet.track_id() != track.id {
            continue;
        }
        match decoder.decode(&packet, &mut samples) {
            Ok(packet_rate) => {
                if !valid_rate(packet_rate) {
                    return Err(format!("unsupported sample rate {packet_rate} Hz"));
                }
                rate = packet_rate;
            }
            Err(PacketError::Skip(_)) => skipped += 1,
            Err(PacketError::Fatal(error)) => return Err(format!("{}: {error}", decoder.name())),
        }
        // Both bounds: the length in seconds, and absolute frames (a high
        // source rate must not buy more memory).
        let limit = MAX_CLIP_FRAMES.min(MAX_CLIP_SECS * u64::from(rate));
        if samples.len() as u64 / 2 > limit {
            eprintln!("[audio] clip longer than {MAX_CLIP_SECS} s; truncated");
            samples.truncate(limit as usize * 2);
            break;
        }
    }
    if samples.is_empty() {
        return Err(format!(
            "{}: no audio decoded ({skipped} bad packets)",
            decoder.name()
        ));
    }
    let samples = resample_clip(samples, rate)?;
    Ok(DecodedClip {
        samples,
        codec: decoder.name(),
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_short_wav_should_decode_with_its_length() {
        let bytes = wav(44_100, 1, 440.0, 0.5);
        let (samples, seconds) = crate::decode_sound(&bytes, Some("wav")).unwrap_or_default();
        assert!(!samples.is_empty() && (seconds - 0.5).abs() < 0.02);
    }

    /// A 16-bit PCM WAV of a sine.
    pub(crate) fn wav(rate: u32, channels: u16, hz: f32, seconds: f32) -> Vec<u8> {
        let frames = (rate as f32 * seconds) as u32;
        let data_len = frames * u32::from(channels) * 2;
        let mut out = Vec::with_capacity(44 + data_len as usize);
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_len).to_le_bytes());
        out.extend_from_slice(b"WAVEfmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&rate.to_le_bytes());
        out.extend_from_slice(&(rate * u32::from(channels) * 2).to_le_bytes());
        out.extend_from_slice(&(channels * 2).to_le_bytes());
        out.extend_from_slice(&16u16.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_len.to_le_bytes());
        for frame in 0..frames {
            let x = (2.0 * std::f32::consts::PI * hz * frame as f32 / rate as f32).sin() * 0.5;
            let sample = (x * 32767.0) as i16;
            for _ in 0..channels {
                out.extend_from_slice(&sample.to_le_bytes());
            }
        }
        out
    }

    fn decode(bytes: &[u8], hint: Option<&str>) -> DecodedClip {
        match decode_clip(Arc::from(bytes), hint) {
            Ok(clip) => clip,
            Err(error) => panic!("{error}"),
        }
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0f32, |m, s| m.max(s.abs()))
    }

    #[test]
    fn wav_decodes_and_resamples_to_48k_stereo() {
        let clip = decode(&wav(44_100, 1, 440.0, 0.5), Some("audio/wav"));
        let frames = clip.samples.len() / 2;
        assert!((23_990..=24_010).contains(&frames), "{frames}");
        assert_eq!(clip.codec, "pcm_s16le");
        assert!((peak(&clip.samples) - 0.5).abs() < 0.02);
        // Mono is copied to both channels.
        assert_eq!(clip.samples[2000], clip.samples[2001]);
    }

    #[test]
    fn wav_at_48k_keeps_its_exact_length() {
        let clip = decode(&wav(48_000, 2, 440.0, 0.25), Some("wav"));
        assert_eq!(clip.samples.len(), 12_000 * 2);
    }

    #[test]
    fn silly_declared_rates_are_rejected_quickly() {
        // 1 Hz would turn a per-second length cap into "never"; 655 kHz is
        // past anything real. Both have enough data to decode if accepted.
        for bytes in [wav(1, 1, 0.0, 4000.0), wav(655_350, 1, 440.0, 0.05)] {
            let started = std::time::Instant::now();
            let error = decode_clip(Arc::from(&bytes[..]), Some("wav")).err();
            assert!(error.is_some(), "accepted a silly sample rate");
            assert!(started.elapsed() < std::time::Duration::from_secs(2));
        }
        let error = decode_clip(Arc::from(&wav(1, 1, 0.0, 400.0)[..]), Some("wav")).err();
        assert!(error.is_some_and(|error| error.contains("sample rate")));
    }

    #[test]
    fn long_clips_are_cut_by_absolute_frames() {
        // 70 s at 16 kHz is cut at 60 s (2.88 M frames once at 48 kHz)...
        let clip = decode(&wav(16_000, 1, 220.0, 70.0), Some("wav"));
        let frames = clip.samples.len() / 2;
        assert!((2_800_000..=2_900_000).contains(&frames), "{frames}");
        // ...and 20 s at 192 kHz by the absolute frame cap (15 s).
        let clip = decode(&wav(192_000, 1, 220.0, 20.0), Some("wav"));
        let frames = clip.samples.len() / 2;
        assert!((710_000..=730_000).contains(&frames), "{frames}");
    }

    #[test]
    fn garbage_is_reported_as_unsupported() {
        let error = decode_clip(Arc::from(&b"definitely not audio"[..]), None).err();
        assert!(error.is_some_and(|error| error.starts_with("unsupported audio")));
    }

    #[test]
    fn hint_accepts_mime_types_and_extensions() {
        // Probing a WAV with a misleading hint still works: the hint only orders probes.
        let clip = decode(&wav(16_000, 1, 300.0, 0.1), Some(".mp3"));
        assert_eq!(clip.samples.len(), 4800 * 2);
        assert_eq!(extension_for_mime("audio/webm"), Some("webm"));
    }

    /// The fixtures: 0.2 s of 440 Hz at 1/16 full scale (ffmpeg's `sine`
    /// source is 1/8, halved), mono, 44.1 kHz (Opus: 48 kHz). Made with:
    /// `ffmpeg -f lavfi -i sine=frequency=440:sample_rate=44100:duration=0.2
    /// -af volume=0.5 -ac 1 -map_metadata -1 -c:a <codec> tone.<ext>`.
    fn assert_fixture(clip: &DecodedClip) {
        let frames = clip.samples.len() / 2;
        let level = peak(&clip.samples);
        assert!((8000..=12_000).contains(&frames), "{frames} frames");
        assert!((0.04..0.09).contains(&level), "peak {level}");
    }

    #[test]
    fn mp3_fixture_decodes() {
        let clip = decode(include_bytes!("../tests/fixtures/tone.mp3"), Some("mp3"));
        assert_eq!(clip.codec, "mp3");
        assert_fixture(&clip);
    }

    #[test]
    fn ogg_vorbis_fixture_decodes() {
        let clip = decode(
            include_bytes!("../tests/fixtures/tone.ogg"),
            Some("audio/ogg"),
        );
        assert_eq!(clip.codec, "vorbis");
        assert_fixture(&clip);
    }

    #[test]
    fn m4a_aac_fixture_decodes() {
        let clip = decode(
            include_bytes!("../tests/fixtures/tone.m4a"),
            Some("audio/mp4"),
        );
        assert_eq!(clip.codec, "aac");
        assert_fixture(&clip);
    }

    #[cfg(not(feature = "libopus"))]
    #[test]
    fn opus_without_the_feature_fails_clearly() {
        let error = decode_clip(
            Arc::from(&include_bytes!("../tests/fixtures/tone.opus")[..]),
            Some("ogg"),
        )
        .err();
        assert_eq!(error.as_deref(), Some(crate::opus::UNSUPPORTED));
    }
}
