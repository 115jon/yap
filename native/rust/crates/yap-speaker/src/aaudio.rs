//! The Android output: one AAudio stream (through the `ndk` crate), 48 kHz
//! interleaved stereo 16-bit, written from the render thread with blocking
//! writes, so the device paces the mix.
//!
//! What the stream plays as follows the call ([`crate::Speaker::set_call`]).
//! In a call it is call audio (`USAGE_VOICE_COMMUNICATION`), so Android
//! routes it with the call (the earpiece, the speaker, a wired or a
//! Bluetooth headset, as the app's call routing picks with
//! `setCommunicationDevice`) at the call's volume. Outside one it plays as a
//! UI sound (`USAGE_ASSISTANCE_SONIFICATION`), at the system volume and
//! silent when the ringer is. Neither asks for audio focus: in a call the
//! call holds it, and UI sounds are too short to.
//!
//! A route change Android cannot follow in place ends the stream
//! (`AAUDIO_ERROR_DISCONNECTED`); the render thread opens it again.

use std::time::Duration;

use ndk::audio::{
    AudioContentType, AudioDirection, AudioError, AudioFormat, AudioPerformanceMode,
    AudioSharingMode, AudioStream, AudioStreamBuilder, AudioStreamState, AudioUsage,
};

use crate::{CHANNELS, FRAME, RATE, SAMPLES};

/// Why a stream did not open.
pub(crate) struct OpenError {
    pub message: String,
    /// The device is there but will not take our format.
    pub refused: bool,
}

fn failed(what: &str, error: AudioError) -> OpenError {
    OpenError {
        message: format!("{what}: {error:?}"),
        refused: false,
    }
}

/// An open, started output stream.
pub(crate) struct Render {
    stream: AudioStream,
    /// For the log and the stats.
    pub name: String,
}

impl Render {
    /// Opens and starts the output, as call audio when `call` is set, with
    /// about `ahead_ms` queued for the device.
    pub(crate) fn open(call: bool, ahead_ms: u32) -> Result<Self, OpenError> {
        let (usage, content, label) = if call {
            (
                AudioUsage::VoiceCommunication,
                AudioContentType::Speech,
                "call",
            )
        } else {
            (
                AudioUsage::AssistanceSonification,
                AudioContentType::Sonification,
                "ui",
            )
        };
        let stream = AudioStreamBuilder::new()
            .map_err(|e| failed("no stream builder", e))?
            .direction(AudioDirection::Output)
            .sharing_mode(AudioSharingMode::Shared)
            // Not low latency: that path (MMAP) may not route with the
            // call, and nothing here needs it.
            .performance_mode(AudioPerformanceMode::None)
            .sample_rate(RATE as i32)
            .channel_count(CHANNELS as i32)
            .format(AudioFormat::PCM_I16)
            .usage(usage)
            .content_type(content)
            .open_stream()
            .map_err(|e| failed("open", e))?;
        // AAudio converts a requested format it cannot play natively; a
        // stream that came back otherwise is not one this mix can feed.
        if stream.sample_rate() != RATE as i32
            || stream.channel_count() != CHANNELS as i32
            || stream.format() != AudioFormat::PCM_I16
        {
            return Err(OpenError {
                message: format!(
                    "opened as {} Hz x{} {:?}",
                    stream.sample_rate(),
                    stream.channel_count(),
                    stream.format()
                ),
                refused: true,
            });
        }
        // `ahead_ms` queued, and at least two bursts: enough not to
        // underrun, short enough that a clip starts with the room.
        let burst = stream.frames_per_burst().max(1);
        let ahead = i32::try_from(ahead_ms * RATE / 1000).unwrap_or(i32::MAX);
        let wanted = (burst * 2).max(ahead).max(FRAME as i32);
        let _ = stream.set_buffer_size_in_frames(wanted.min(stream.buffer_capacity_in_frames()));
        stream.request_start().map_err(|e| failed("start", e))?;
        let name = format!("aaudio {label} (device {})", stream.device_id());
        Ok(Self { stream, name })
    }

    /// Frames queued for the device but not yet played.
    fn queued(&self) -> i64 {
        (self.stream.frames_written() - self.stream.frames_read()).max(0)
    }

    /// From a frame written now to the ear, ms (the queue only; AAudio does
    /// not tell the device's own latency without a timestamp).
    pub(crate) fn delay_ms(&self) -> u32 {
        let frames = self.queued() + FRAME as i64;
        u32::try_from(frames * 1000 / i64::from(RATE)).unwrap_or(u32::MAX)
    }

    /// Writes `frame`, blocking while the device's buffer is full, for at
    /// most `timeout`. The frames taken (fewer than [`FRAME`] on a timeout).
    pub(crate) fn write(&self, frame: &[i16; SAMPLES], timeout: Duration) -> Result<usize, String> {
        if self.stream.state() == AudioStreamState::Disconnected {
            return Err("disconnected".into());
        }
        let nanos = i64::try_from(timeout.as_nanos()).unwrap_or(i64::MAX);
        // SAFETY: `frame` holds FRAME interleaved stereo i16 frames, the
        // stream's format, and outlives the call.
        let written = unsafe {
            self.stream
                .write(frame.as_ptr().cast(), FRAME as i32, nanos)
        };
        match written {
            Ok(n) => Ok(n as usize),
            // ndk 0.9 reads AAudioStream_write's result as a status, so a
            // write of n > 0 frames comes back as an "error" holding n.
            // AAudio's errors are negative.
            Err(error) => match usize::try_from(i32::from(error)) {
                Ok(n) => Ok(n),
                Err(_) => Err(format!("{error:?}")),
            },
        }
    }

    /// Underruns since the stream opened.
    pub(crate) fn xruns(&self) -> u64 {
        u64::try_from(self.stream.x_run_count()).unwrap_or(0)
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        let _ = self.stream.request_stop();
    }
}
