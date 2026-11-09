//! The render tap's file: 16-bit PCM WAV written as it goes, its sizes
//! patched when it finishes (on drop too).

use std::fs::File;
use std::io::{self, BufWriter, Seek, SeekFrom, Write};
use std::path::Path;

fn header(rate: u32, channels: u16, data_bytes: u32) -> [u8; 44] {
    let block = channels * 2;
    let mut h = [0u8; 44];
    h[0..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&data_bytes.saturating_add(36).to_le_bytes());
    h[8..12].copy_from_slice(b"WAVE");
    h[12..16].copy_from_slice(b"fmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&channels.to_le_bytes());
    h[24..28].copy_from_slice(&rate.to_le_bytes());
    h[28..32].copy_from_slice(&(rate * u32::from(block)).to_le_bytes());
    h[32..34].copy_from_slice(&block.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data_bytes.to_le_bytes());
    h
}

pub(crate) struct WavWriter {
    file: BufWriter<File>,
    rate: u32,
    channels: u16,
    bytes: u32,
    limit: u32,
}

impl WavWriter {
    /// Creates `path`; at most `max_seconds` are kept.
    pub(crate) fn create(
        path: &Path,
        rate: u32,
        channels: u16,
        max_seconds: u32,
    ) -> io::Result<Self> {
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
    pub(crate) fn write(&mut self, samples: &[i16]) -> io::Result<()> {
        let room = (self.limit.saturating_sub(self.bytes) / 2) as usize;
        let take = samples.len().min(room);
        for s in &samples[..take] {
            self.file.write_all(&s.to_le_bytes())?;
        }
        self.bytes += u32::try_from(take * 2).unwrap_or(0);
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
    fn a_writer_should_patch_the_sizes_and_stop_at_its_limit() {
        let path = std::env::temp_dir().join(format!("yap-speaker-wav-{}.wav", std::process::id()));
        {
            let mut writer = WavWriter::create(&path, 10, 1, 1).expect("create");
            writer.write(&[1; 8]).expect("write");
            writer.write(&[2; 8]).expect("write");
        }
        let bytes = std::fs::read(&path).expect("read");
        let _ = std::fs::remove_file(&path);
        assert_eq!(&bytes[40..44], &20u32.to_le_bytes());
        assert_eq!(bytes.len(), 44 + 20);
    }
}
