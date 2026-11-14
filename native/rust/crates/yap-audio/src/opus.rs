//! Opus decoding. Symphonia demuxes Ogg and WebM Opus but has no Opus
//! decoder; with the `libopus` feature this module calls the libopus that
//! LiveKit's `webrtc.lib` carries (the app links it). Without the feature,
//! Opus is reported as unsupported.

/// The Opus header fields the decoder needs (RFC 7845 §5.1 `OpusHead`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct OpusHead {
    pub channels: usize,
    pub pre_skip: usize,
}

impl OpusHead {
    /// Parses `OpusHead` (Ogg's identification header, WebM's CodecPrivate).
    pub(crate) fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < 19 || &data[..8] != b"OpusHead" {
            return None;
        }
        Some(Self {
            channels: usize::from(data[9]),
            pre_skip: usize::from(u16::from_le_bytes([data[10], data[11]])),
        })
    }
}

#[cfg(not(feature = "libopus"))]
pub(crate) const UNSUPPORTED: &str =
    "Opus needs the libopus feature (this build cannot decode Opus)";

#[cfg(feature = "libopus")]
pub(crate) use imp::Decoder;

#[cfg(feature = "libopus")]
mod imp {
    use std::ffi::c_int;
    use std::ptr::NonNull;

    use super::OpusHead;

    #[repr(C)]
    struct OpusDecoder {
        _private: [u8; 0],
    }

    // Resolved at the final link from LiveKit's `webrtc.lib` (libopus 1.x).
    extern "C" {
        fn opus_decoder_create(fs: i32, channels: c_int, error: *mut c_int) -> *mut OpusDecoder;
        fn opus_decode_float(
            st: *mut OpusDecoder,
            data: *const u8,
            len: i32,
            pcm: *mut f32,
            frame_size: c_int,
            decode_fec: c_int,
        ) -> c_int;
        fn opus_decoder_destroy(st: *mut OpusDecoder);
    }

    /// 120 ms at 48 kHz, the longest Opus packet.
    const MAX_FRAMES: usize = 5760;

    /// A mono or stereo libopus decoder at 48 kHz.
    pub(crate) struct Decoder {
        state: NonNull<OpusDecoder>,
        channels: usize,
        skip_left: usize,
        pcm: Vec<f32>,
    }

    impl Decoder {
        pub(crate) fn new(head: OpusHead) -> Result<Self, String> {
            if !(1..=2).contains(&head.channels) {
                return Err(format!(
                    "Opus with {} channels is not supported",
                    head.channels
                ));
            }
            let mut error: c_int = 0;
            let channels = c_int::try_from(head.channels).unwrap_or(2);
            // SAFETY: plain C constructor; `error` outlives the call. The
            // result is checked for null before use.
            let state = unsafe { opus_decoder_create(48_000, channels, &mut error) };
            let state = NonNull::new(state)
                .filter(|_| error == 0)
                .ok_or_else(|| format!("opus_decoder_create failed ({error})"))?;
            Ok(Self {
                state,
                channels: head.channels,
                skip_left: head.pre_skip,
                pcm: vec![0.0; MAX_FRAMES * head.channels],
            })
        }

        /// Decodes one packet, appending `channels`-interleaved samples to
        /// `out` minus what is left of the pre-skip. Returns the channel count.
        pub(crate) fn decode(
            &mut self,
            packet: &[u8],
            out: &mut Vec<f32>,
        ) -> Result<usize, String> {
            let len =
                i32::try_from(packet.len()).map_err(|_| "Opus packet too large".to_string())?;
            // SAFETY: `state` is a live decoder owned by `self`; `packet` is
            // valid for `len` bytes; `pcm` holds MAX_FRAMES frames of
            // `channels` samples, the size passed as `frame_size`.
            let frames = unsafe {
                opus_decode_float(
                    self.state.as_ptr(),
                    packet.as_ptr(),
                    len,
                    self.pcm.as_mut_ptr(),
                    MAX_FRAMES as c_int,
                    0,
                )
            };
            let frames = usize::try_from(frames)
                .map_err(|_| format!("opus_decode_float failed ({frames})"))?;
            let skip = self.skip_left.min(frames);
            self.skip_left -= skip;
            out.extend_from_slice(&self.pcm[skip * self.channels..frames * self.channels]);
            Ok(self.channels)
        }

        /// A fresh decoder (after a seek); the pre-skip only applies at the start.
        pub(crate) fn reset(&mut self) -> Result<(), String> {
            let fresh = Self::new(OpusHead {
                channels: self.channels,
                pre_skip: 0,
            })?;
            *self = fresh;
            Ok(())
        }
    }

    impl Drop for Decoder {
        fn drop(&mut self) {
            // SAFETY: `state` came from `opus_decoder_create` and is
            // destroyed exactly once, here.
            unsafe { opus_decoder_destroy(self.state.as_ptr()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_head_parses_channels_and_pre_skip() {
        let mut head = b"OpusHead".to_vec();
        head.extend_from_slice(&[1, 2, 0x38, 0x01, 0x80, 0xBB, 0, 0, 0, 0, 0]);
        assert_eq!(
            OpusHead::parse(&head),
            Some(OpusHead {
                channels: 2,
                pre_skip: 312
            })
        );
        assert_eq!(OpusHead::parse(b"OpusTags"), None);
    }
}
