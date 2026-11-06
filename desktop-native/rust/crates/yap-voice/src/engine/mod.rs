//! The media side of a call behind one trait: [`MediaEngine`] owns the
//! audio devices and opens one peer connection per call as a
//! [`MediaSession`]. [`LibwebrtcEngine`] is production; [`FakeEngine`]
//! records what it was asked, for tests. Decoded remote video goes to a
//! [`VideoOutput`] the app installs, frame by frame, off the UI thread.

use std::future::Future;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::audio::Processing;
use crate::machine::MediaState;
use crate::share::ShareEncoding;
use crate::wire::IceServer;

mod fake;
#[cfg(feature = "libwebrtc")]
mod webrtc;

pub use fake::{FakeEngine, FakeOp, FakeSession};
#[cfg(feature = "libwebrtc")]
pub(crate) use webrtc::installed_video_output as video_output;
#[cfg(feature = "libwebrtc")]
pub use webrtc::{LibwebrtcEngine, LibwebrtcSession};

/// A media operation failed; the text is for the log and the call's error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct MediaError(pub String);

/// Our local offer for the mic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalOffer {
    pub sdp: String,
    /// The send transceiver's mid.
    pub mid: String,
}

/// One decoded video frame in I420 (planar Y, then U and V at half size),
/// borrowed for the length of [`VideoOutput::frame`].
#[derive(Debug, Clone, Copy)]
pub struct I420Frame<'a> {
    pub width: u32,
    pub height: u32,
    /// Clockwise degrees to turn it for display (0, 90, 180, 270).
    pub rotation: u16,
    pub y: &'a [u8],
    pub u: &'a [u8],
    pub v: &'a [u8],
    pub stride_y: u32,
    pub stride_u: u32,
    pub stride_v: u32,
}

/// Where remote video frames go. Called on a media thread (a tokio worker
/// fed by WebRTC's decoder), once per frame of each routed track; it must
/// not block.
pub trait VideoOutput: Send + Sync + 'static {
    /// A frame of the track routed under `key` (its alias, e.g.
    /// `cam-video-<participant>`).
    fn frame(&self, key: &str, frame: &I420Frame<'_>);
}

/// One outbound video encoding, as the stats report it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutboundVideo {
    pub mid: String,
    /// The simulcast layer (`f`, `h`, `q`), empty for a single layer.
    pub rid: String,
    pub bytes_sent: u64,
    pub frames_encoded: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// libwebrtc's `encoderImplementation` (`libvpx`, ...).
    pub encoder: String,
    /// `none`, `cpu`, `bandwidth` or `other`.
    pub limitation: String,
    pub active: bool,
}

/// One stats snapshot of a peer connection (the driver takes one every 5 s):
/// the log line, and the numbers the voice details, the streaming stats and
/// the audio sentinels read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaStats {
    /// The `[voice] stats ...` line for the log.
    pub line: String,
    /// Our mic's outbound RTP, bytes and packets so far.
    pub mic_bytes_sent: u64,
    pub mic_packets_sent: u64,
    /// Every inbound audio stream, summed.
    pub audio_bytes_received: u64,
    pub audio_packets_received: u64,
    pub audio_packets_lost: i64,
    /// The highest inbound audio jitter, seconds.
    pub audio_jitter: f64,
    /// Inbound RTP streams (remote tracks that receive).
    pub inbound_streams: u32,
    /// The selected candidate pair's current round trip, seconds.
    pub round_trip: Option<f64>,
    /// The remote candidate of that pair (`address:port protocol`).
    pub server: String,
    /// Our camera's and share's encodings.
    pub outbound_video: Vec<OutboundVideo>,
}

/// Owns the audio devices for the length of a call.
pub trait MediaEngine: Send + Sync + 'static {
    type Session: MediaSession;

    /// Like [`MediaEngine::open`], for a hidden standby: the devices are
    /// held but the mic is not opened until [`MediaSession::promote_mic`].
    fn open_standby(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(Self::Session, UnboundedReceiver<MediaState>), MediaError> {
        self.open(ice_servers)
    }

    /// A receive-only peer connection for video pulls of its own (a fresh
    /// path after a closed pull: PartyTracks `SplitVideoTransport`). It has
    /// no mic and touches no device.
    fn open_receiver(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(Self::Session, UnboundedReceiver<MediaState>), MediaError>;

    /// Opens the devices and one peer connection with these ICE servers.
    /// The receiver carries its connection state changes.
    fn open(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(Self::Session, UnboundedReceiver<MediaState>), MediaError>;

    /// Gives the devices back. Called once when a call ends, even if
    /// [`MediaEngine::open`] failed.
    fn release(&self);

    /// Follows the device choice while a call runs: a chosen device that
    /// was unplugged falls back to the default, and "Default" follows a
    /// new system default. Returns a log line when it switched.
    fn check_devices(&self) -> Option<String> {
        None
    }
}

/// One peer connection to the SFU. Clones share it.
pub trait MediaSession: Clone + Send + Sync + 'static {
    /// Adds the mic as a send-only Opus transceiver in `stream_id`, and
    /// sets and returns the local offer (§3.3b: we offer, the SFU answers).
    fn publish_mic(
        &self,
        stream_id: &str,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send;

    /// A standby's push: a silent track stands in for the mic (no device
    /// is opened). Sets and returns the local offer.
    fn publish_standby(
        &self,
        stream_id: &str,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send {
        self.publish_mic(stream_id)
    }

    /// The standby is promoted: the device mic replaces the silence on the
    /// same sender (no renegotiation).
    fn promote_mic(&self) -> Result<(), MediaError> {
        Ok(())
    }

    /// Pushes the mic again as a new send transceiver (a republish: the SFU
    /// lost the track, op 110 or a stale standby); the old one is stopped.
    /// Sets and returns the local offer.
    fn republish_mic(
        &self,
        stream_id: &str,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send;

    /// Applies an SFU answer to our last offer. `what` names it in errors
    /// (`answer`, `close answer`).
    fn apply_answer(
        &self,
        sdp: &str,
        what: &'static str,
    ) -> impl Future<Output = Result<(), MediaError>> + Send;

    /// Starts capture if WebRTC did not (after the mic's answer).
    fn start_capture(&self);

    /// Applies an SFU offer for a pull and returns our answer (§3.3c).
    fn answer_offer(&self, sdp: &str) -> impl Future<Output = Result<String, MediaError>> + Send;

    /// Stops the receive transceivers with these mids and returns the
    /// local offer that drops them (§3.3d: we offer).
    fn close_mids(
        &self,
        mids: &[String],
    ) -> impl Future<Output = Result<String, MediaError>> + Send;

    /// Adds our camera as a send-only VP8 transceiver in `stream_id` with
    /// the camera simulcast encodings ([`crate::CAMERA_SIMULCAST`]), parked
    /// on the keepalive, and sets and returns the local offer. A camera
    /// pushed before is retired first (its transceiver stays, fed nothing,
    /// so it sends nothing, as PartyTracks' `republishTrack` leaves it).
    fn publish_camera(
        &self,
        stream_id: &str,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send;

    /// The layers our camera's sender negotiated, highest first, when it
    /// negotiated more than one (`negotiatedSimulcastRids`).
    fn camera_rids(&self) -> Vec<String>;

    /// Camera frames (`true`) or the keepalive's black frames.
    fn set_camera_live(&self, live: bool);

    /// The parked camera's keepalive stalled long enough that the SFU may
    /// have dropped its track, or there is no camera send at all: it must
    /// be pushed afresh.
    fn camera_stale(&self) -> bool;

    /// Adds our share as a send-only VP8 transceiver in `stream_id` with
    /// one encoding capped as `encoding` says, parked on the keepalive, and
    /// sets and returns the local offer. A share pushed before is retired
    /// first, as the camera's is.
    fn publish_screen(
        &self,
        stream_id: &str,
        encoding: &ShareEncoding,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send;

    /// Drops a camera send whose push failed (its keepalive stops, its
    /// transceiver is retired).
    fn abandon_camera(&self);

    /// Drops a share send whose push failed, as [`Self::abandon_camera`].
    fn abandon_screen(&self);

    /// The share's caps and content hint, in place (`setParameters`).
    fn set_screen_encoding(&self, encoding: &ShareEncoding);

    /// Captured share frames (`true`) or the keepalive's black frames.
    fn set_screen_live(&self, live: bool);

    /// The parked share's keepalive stalled long enough that the SFU may
    /// have dropped its track, or there is no share send: push afresh.
    fn screen_stale(&self) -> bool;

    /// Adds the share's audio (48 kHz stereo, fed by
    /// [`crate::push_screen_audio`]) as a send-only Opus transceiver in
    /// `stream_id`, and sets and returns the local offer. A send there was
    /// is abandoned first.
    fn publish_screen_audio(
        &self,
        stream_id: &str,
    ) -> impl Future<Output = Result<LocalOffer, MediaError>> + Send;

    /// Parks the share's audio between shares: disabled and fed silence,
    /// so the SFU keeps its track for the next share with audio.
    fn park_screen_audio(&self);

    /// Resumes the parked share audio (`true`), unless there is none or
    /// its keepalive stalled long enough that it must be pushed afresh.
    fn resume_screen_audio(&self) -> bool;

    /// Retires the share's audio send (a failed push, or before a fresh
    /// one): nothing feeds it and its transceiver stays, sending nothing.
    fn abandon_screen_audio(&self);

    /// The share audio track on or off (in place: off sends silence).
    fn set_screen_audio_enabled(&self, enabled: bool);

    fn set_mic_enabled(&self, enabled: bool);

    /// Plays or silences every remote track, including ones that arrive
    /// later.
    fn set_playout_enabled(&self, enabled: bool);

    /// The mic's capture processing; `Err` says why it was refused.
    fn set_processing(&self, processing: Processing) -> Result<(), MediaError>;

    /// The gain of the remote track on `mid` (1.0 is as sent, 0.0
    /// silent), kept for the track if it has not arrived yet.
    fn set_track_gain(&self, mid: &str, gain: f64);

    /// Routes the frames of the remote video received on `mid` to the
    /// [`VideoOutput`] under `key`, or stops (`None`: frames are dropped
    /// before any conversion). Kept for a track that has not arrived yet.
    fn set_video_sink(&self, mid: &str, key: Option<&str>);

    /// Stops the mic, the camera and the share (teardown, before the
    /// sockets close).
    fn stop_local(&self);

    /// The mic's cumulative `(totalAudioEnergy, totalSamplesDuration)`
    /// from the send stream, once it has any.
    fn mic_energy(&self) -> impl Future<Output = Option<(f64, f64)>> + Send;

    /// A stats snapshot (and its log line).
    fn stats(&self) -> impl Future<Output = Result<MediaStats, MediaError>> + Send;

    /// Drops remote tracks and closes the peer connection.
    fn close(&self);
}
