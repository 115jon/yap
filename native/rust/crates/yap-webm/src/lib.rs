//! WebM playback for the native client, which Qt's Media Foundation backend
//! cannot open (its source resolver has no WebM byte-stream handler, so the
//! player waits in `LoadingMedia` forever).
//!
//! - [`demux`]: the file, read whole from memory with `webm-iterable`, as
//!   one video track's packets. VP8 and VP9, with the alpha side stream
//!   (`BlockAdditional` id 1, `AlphaMode` 1) that animated nameplates use.
//! - `vpx` (feature `libvpx`): libvpx's decoder, from LiveKit's
//!   `webrtc.lib`, plus libyuv for scaling and the alpha conversion.
//! - [`Player`]: a thread that fetches, demuxes, decodes and paces frames
//!   to a [`Sink`], looping, pausing and seeking on command.
//!
//! Audio is not played: nameplates and profile media are silent loops.

mod demux;
mod player;
#[cfg(feature = "libvpx")]
mod vpx;

pub use demux::{demux, Codec, Packet, Video};
pub use player::{fit_within, Event, Frame, Options, Player, Sink};

/// Why a WebM could not be played.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WebmError {
    #[error("not a WebM file: {0}")]
    Parse(String),
    #[error("no VP8 or VP9 video track")]
    NoVideo,
    #[error("decoder: {0}")]
    Decode(String),
    #[error("WebM decoding is not built in")]
    Unsupported,
    #[error("{0}")]
    Fetch(String),
    #[error("the player stopped unexpectedly")]
    Crashed,
}

/// Whether this build can decode (the `libvpx` feature).
pub const fn can_decode() -> bool {
    cfg!(feature = "libvpx")
}
