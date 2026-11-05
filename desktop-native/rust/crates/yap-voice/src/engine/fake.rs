//! A media engine that touches no devices or network: it records every
//! call and answers with canned SDP. State changes are pushed by the test.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::{LocalOffer, MediaEngine, MediaError, MediaSession};
use crate::audio::Processing;
use crate::machine::MediaState;
use crate::share::ShareEncoding;
use crate::wire::IceServer;

/// One call made on a [`FakeEngine`] or its session.
#[derive(Debug, Clone, PartialEq)]
pub enum FakeOp {
    Open {
        ice_servers: usize,
    },
    PublishMic {
        stream_id: String,
    },
    ApplyAnswer {
        sdp: String,
        what: &'static str,
    },
    StartCapture,
    AnswerOffer {
        sdp: String,
    },
    CloseMids(Vec<String>),
    PublishCamera {
        stream_id: String,
    },
    SetCameraLive(bool),
    PublishScreen {
        stream_id: String,
        encoding: ShareEncoding,
    },
    SetScreenEncoding(ShareEncoding),
    SetScreenLive(bool),
    PublishScreenAudio {
        stream_id: String,
    },
    RetireScreenAudio,
    SetScreenAudioEnabled(bool),
    SetMicEnabled(bool),
    SetPlayoutEnabled(bool),
    SetTrackGain {
        mid: String,
        gain: f64,
    },
    SetProcessing(Processing),
    SetVideoSink {
        mid: String,
        key: Option<String>,
    },
    StopLocal,
    Stats,
    Close,
    Release,
}

#[derive(Debug, Default)]
struct Shared {
    ops: Vec<FakeOp>,
    states: Option<UnboundedSender<MediaState>>,
    fail_open: bool,
    /// What `mic_energy` answers.
    energy: Option<(f64, f64)>,
    /// What `camera_stale` answers.
    camera_stale: bool,
    /// What `screen_stale` answers.
    screen_stale: bool,
}

/// Records calls; clones share the record.
#[derive(Debug, Clone, Default)]
pub struct FakeEngine {
    shared: Arc<Mutex<Shared>>,
}

/// The session of a [`FakeEngine`]; shares its record.
#[derive(Debug, Clone)]
pub struct FakeSession {
    shared: Arc<Mutex<Shared>>,
}

fn record(shared: &Mutex<Shared>, op: FakeOp) {
    shared
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .ops
        .push(op);
}

impl FakeEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Makes the next [`MediaEngine::open`] fail ("no audio device").
    pub fn fail_open(&self) {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .fail_open = true;
    }

    /// Sets the mic's cumulative energy counters `mic_energy` reports.
    pub fn set_mic_energy(&self, total_energy: f64, total_duration: f64) {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .energy = Some((total_energy, total_duration));
    }

    /// Makes `camera_stale` say the parked camera's track may be gone.
    pub fn set_camera_stale(&self, stale: bool) {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .camera_stale = stale;
    }

    /// Makes `screen_stale` say the parked share's track may be gone.
    pub fn set_screen_stale(&self, stale: bool) {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .screen_stale = stale;
    }

    /// Everything called so far, in order.
    pub fn ops(&self) -> Vec<FakeOp> {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .ops
            .clone()
    }

    /// Reports a connection state, as the peer connection would; `false`
    /// if no session is open.
    pub fn push_state(&self, state: MediaState) -> bool {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .states
            .as_ref()
            .is_some_and(|tx| tx.send(state).is_ok())
    }
}

impl MediaEngine for FakeEngine {
    type Session = FakeSession;

    fn open(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(FakeSession, UnboundedReceiver<MediaState>), MediaError> {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        shared.ops.push(FakeOp::Open {
            ice_servers: ice_servers.len(),
        });
        if shared.fail_open {
            return Err(MediaError("no audio device".into()));
        }
        let (tx, rx) = mpsc::unbounded_channel();
        shared.states = Some(tx);
        Ok((
            FakeSession {
                shared: Arc::clone(&self.shared),
            },
            rx,
        ))
    }

    fn release(&self) {
        record(&self.shared, FakeOp::Release);
    }
}

impl MediaSession for FakeSession {
    async fn publish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        record(
            &self.shared,
            FakeOp::PublishMic {
                stream_id: stream_id.to_owned(),
            },
        );
        Ok(LocalOffer {
            sdp: "fake-offer".into(),
            mid: "0".into(),
        })
    }

    async fn apply_answer(&self, sdp: &str, what: &'static str) -> Result<(), MediaError> {
        record(
            &self.shared,
            FakeOp::ApplyAnswer {
                sdp: sdp.to_owned(),
                what,
            },
        );
        Ok(())
    }

    fn start_capture(&self) {
        record(&self.shared, FakeOp::StartCapture);
    }

    async fn answer_offer(&self, sdp: &str) -> Result<String, MediaError> {
        record(
            &self.shared,
            FakeOp::AnswerOffer {
                sdp: sdp.to_owned(),
            },
        );
        Ok("fake-answer".into())
    }

    async fn close_mids(&self, mids: &[String]) -> Result<String, MediaError> {
        record(&self.shared, FakeOp::CloseMids(mids.to_vec()));
        Ok("fake-close-offer".into())
    }

    async fn publish_camera(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        record(
            &self.shared,
            FakeOp::PublishCamera {
                stream_id: stream_id.to_owned(),
            },
        );
        Ok(LocalOffer {
            sdp: "fake-camera-offer".into(),
            mid: "1".into(),
        })
    }

    fn camera_rids(&self) -> Vec<String> {
        ["f", "h", "q"].map(str::to_owned).to_vec()
    }

    fn set_camera_live(&self, live: bool) {
        record(&self.shared, FakeOp::SetCameraLive(live));
    }

    fn camera_stale(&self) -> bool {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .camera_stale
    }

    async fn publish_screen(
        &self,
        stream_id: &str,
        encoding: &ShareEncoding,
    ) -> Result<LocalOffer, MediaError> {
        record(
            &self.shared,
            FakeOp::PublishScreen {
                stream_id: stream_id.to_owned(),
                encoding: *encoding,
            },
        );
        Ok(LocalOffer {
            sdp: "fake-screen-offer".into(),
            mid: "2".into(),
        })
    }

    fn set_screen_encoding(&self, encoding: &ShareEncoding) {
        record(&self.shared, FakeOp::SetScreenEncoding(*encoding));
    }

    fn set_screen_live(&self, live: bool) {
        record(&self.shared, FakeOp::SetScreenLive(live));
    }

    fn screen_stale(&self) -> bool {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .screen_stale
    }

    async fn publish_screen_audio(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        record(
            &self.shared,
            FakeOp::PublishScreenAudio {
                stream_id: stream_id.to_owned(),
            },
        );
        Ok(LocalOffer {
            sdp: "fake-screen-audio-offer".into(),
            mid: "3".into(),
        })
    }

    fn retire_screen_audio(&self) {
        record(&self.shared, FakeOp::RetireScreenAudio);
    }

    fn set_screen_audio_enabled(&self, enabled: bool) {
        record(&self.shared, FakeOp::SetScreenAudioEnabled(enabled));
    }

    fn set_mic_enabled(&self, enabled: bool) {
        record(&self.shared, FakeOp::SetMicEnabled(enabled));
    }

    fn set_playout_enabled(&self, enabled: bool) {
        record(&self.shared, FakeOp::SetPlayoutEnabled(enabled));
    }

    fn set_processing(&self, processing: Processing) -> Result<(), MediaError> {
        record(&self.shared, FakeOp::SetProcessing(processing));
        Ok(())
    }

    fn set_track_gain(&self, mid: &str, gain: f64) {
        record(
            &self.shared,
            FakeOp::SetTrackGain {
                mid: mid.to_owned(),
                gain,
            },
        );
    }

    fn set_video_sink(&self, mid: &str, key: Option<&str>) {
        record(
            &self.shared,
            FakeOp::SetVideoSink {
                mid: mid.to_owned(),
                key: key.map(str::to_owned),
            },
        );
    }

    fn stop_local(&self) {
        record(&self.shared, FakeOp::StopLocal);
    }

    async fn mic_energy(&self) -> Option<(f64, f64)> {
        self.shared
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .energy
    }

    async fn stats(&self) -> Result<String, MediaError> {
        record(&self.shared, FakeOp::Stats);
        Ok("[voice] stats".into())
    }

    fn close(&self) {
        let mut shared = self.shared.lock().unwrap_or_else(PoisonError::into_inner);
        shared.ops.push(FakeOp::Close);
        shared.states = None;
    }
}
