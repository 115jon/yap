//! The media side of a call behind one trait: [`MediaEngine`] owns the
//! audio devices and opens one peer connection per call as a
//! [`MediaSession`]. [`LibwebrtcEngine`] is production; [`FakeEngine`]
//! records what it was asked, for tests.

use std::future::Future;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::audio::Processing;
use crate::machine::MediaState;
use crate::wire::IceServer;

mod fake;
#[cfg(feature = "libwebrtc")]
mod webrtc;

pub use fake::{FakeEngine, FakeOp, FakeSession};
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

/// Owns the audio devices for the length of a call.
pub trait MediaEngine: Send + Sync + 'static {
    type Session: MediaSession;

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

    fn set_mic_enabled(&self, enabled: bool);

    /// Plays or silences every remote track, including ones that arrive
    /// later.
    fn set_playout_enabled(&self, enabled: bool);

    /// The mic's capture processing; `Err` says why it was refused.
    fn set_processing(&self, processing: Processing) -> Result<(), MediaError>;

    /// The gain of the remote track on `mid` (1.0 is as sent, 0.0
    /// silent), kept for the track if it has not arrived yet.
    fn set_track_gain(&self, mid: &str, gain: f64);

    /// The mic's cumulative `(totalAudioEnergy, totalSamplesDuration)`
    /// from the send stream, once it has any.
    fn mic_energy(&self) -> impl Future<Output = Option<(f64, f64)>> + Send;

    /// One line of media stats for the log.
    fn stats(&self) -> impl Future<Output = Result<String, MediaError>> + Send;

    /// Drops remote tracks and closes the peer connection.
    fn close(&self);
}
