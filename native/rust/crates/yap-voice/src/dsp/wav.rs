//! 16-bit PCM WAV: what the test hooks read (`YAP_FAKE_MIC`) and write
//! (`YAP_MIC_TAP`, `YAP_RENDER_TAP`), and the fixtures the tests make.
//! Only the canonical layout is written; reading walks the chunks and takes
//! PCM (or WAVE_FORMAT_EXTENSIBLE PCM) at 16 bits.

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

/// A decoded file: interleaved samples.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wav {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

/// Why a file could not be read.
#[derive(Debug, thiserror::Error)]
pub enum WavError {
    #[error("not a RIFF/WAVE file")]
    NotWave,
    #[error("no fmt chunk before the data")]
    NoFormat,
    #[error("not 16-bit PCM (format {format}, {bits} bits)")]
    Unsupported { format: u16, bits: u16 },
    #[error("no data chunk")]
    NoData,
    #[error(transparent)]
    Io(#[from] io::Error),
}

const PCM: u16 = 1;
const EXTENSIBLE: u16 = 0xFFFE;

fn header(rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let block = channels * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&PCM.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(block)).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

/// A whole file in memory.
pub fn encode(rate: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let data = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + samples.len() * 2);
    out.extend_from_slice(&header(rate, channels, data));
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Reads a file's bytes.
pub fn decode(bytes: &[u8]) -> Result<Wav, WavError> {
    if bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return Err(WavError::NotWave);
    }
    let mut at = 12;
    let mut format = None;
    while let (Some(id), Some(size)) = (bytes.get(at..at + 4), u32_at(bytes, at + 4)) {
        let body = at + 8;
        let size = size as usize;
        match id {
            b"fmt " => {
                let tag = u16_at(bytes, body).ok_or(WavError::NoFormat)?;
                let channels = u16_at(bytes, body + 2).ok_or(WavError::NoFormat)?;
                let rate = u32_at(bytes, body + 4).ok_or(WavError::NoFormat)?;
                let bits = u16_at(bytes, body + 14).ok_or(WavError::NoFormat)?;
                // Extensible: the sub-format GUID starts with the real tag.
                let tag = if tag == EXTENSIBLE {
                    u16_at(bytes, body + 24).unwrap_or(0)
                } else {
                    tag
                };
                if tag != PCM || bits != 16 || channels == 0 {
                    return Err(WavError::Unsupported { format: tag, bits });
                }
                format = Some((rate, channels));
            }
            b"data" => {
                let (rate, channels) = format.ok_or(WavError::NoFormat)?;
                let end = body.saturating_add(size).min(bytes.len());
                let samples = bytes[body..end]
                    .chunks_exact(2)
                    .map(|p| i16::from_le_bytes([p[0], p[1]]))
                    .collect();
                return Ok(Wav {
                    rate,
                    channels,
                    samples,
                });
            }
            _ => {}
        }
        // Chunks are padded to an even size.
        at = body.saturating_add(size + (size & 1));
    }
    Err(WavError::NoData)
}

/// Reads a file.
pub fn read(path: &Path) -> Result<Wav, WavError> {
    decode(&std::fs::read(path)?)
}

/// A file written as it goes; the sizes are patched when it finishes
/// (on drop too), so a run that stops early still leaves a playable file.
pub struct WavWriter {
    file: BufWriter<File>,
    rate: u32,
    channels: u16,
    bytes: u32,
    /// Stops taking samples past this many bytes.
    limit: u32,
}

impl WavWriter {
    /// Creates `path`; at most `max_seconds` are kept.
    pub fn create(path: &Path, rate: u32, channels: u16, max_seconds: u32) -> io::Result<Self> {
        let mut file = BufWriter::new(File::create(path)?);
        file.write_all(&header(rate, channels, 0))?;
        Ok(Self {
            file,
            rate,
            channels,
            bytes: 0,
            limit: max_seconds
                .saturating_mul(rate)
                .saturating_mul(u32::from(channels) * 2),
        })
    }

    /// Appends interleaved samples (dropped once the limit is reached).
    pub fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let room = (self.limit.saturating_sub(self.bytes) / 2) as usize;
        for s in &samples[..samples.len().min(room)] {
            self.file.write_all(&s.to_le_bytes())?;
        }
        self.bytes += u32::try_from(samples.len().min(room) * 2).unwrap_or(0);
        Ok(())
    }

    fn patch(&mut self) -> io::Result<()> {
        self.file.flush()?;
        let file = self.file.get_mut();
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header(self.rate, self.channels, self.bytes))?;
        file.seek(SeekFrom::End(0))?;
        file.flush()
    }
}

impl Drop for WavWriter {
    fn drop(&mut self) {
        let _ = self.patch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_should_round_trip() {
        let samples = vec![0, 1, -1, i16::MAX, i16::MIN, 1234];
        let wav = decode(&encode(48_000, 2, &samples)).expect("decode");
        assert_eq!(
            wav,
            Wav {
                rate: 48_000,
                channels: 2,
                samples
            }
        );
    }

    #[test]
    fn eight_bit_files_should_be_refused() {
        let mut bytes = encode(8_000, 1, &[0, 0]);
        bytes[34] = 8;
        assert!(matches!(
            decode(&bytes),
            Err(WavError::Unsupported { bits: 8, .. })
        ));
    }

    #[test]
    fn a_writer_should_patch_the_sizes_and_stop_at_its_limit() {
        let path = std::env::temp_dir().join(format!("yap-wav-{}.wav", std::process::id()));
        {
            let mut writer = WavWriter::create(&path, 10, 1, 1).expect("create");
            writer.write(&[1; 8]).expect("write");
            writer.write(&[2; 8]).expect("write");
        }
        let wav = read(&path).expect("read");
        let _ = std::fs::remove_file(&path);
        assert_eq!(wav.samples, [vec![1; 8], vec![2; 2]].concat());
    }
}
