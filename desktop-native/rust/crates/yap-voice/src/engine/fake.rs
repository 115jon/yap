//! A media engine that touches no devices or network: it records every
//! call and answers with canned SDP. State changes are pushed by the test.

use std::sync::{Arc, Mutex, PoisonError};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use super::{LocalOffer, MediaEngine, MediaError, MediaSession};
use crate::machine::MediaState;
use crate::wire::IceServer;

/// One call made on a [`FakeEngine`] or its session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeOp {
    Open { ice_servers: usize },
    PublishMic { stream_id: String },
    ApplyAnswer { sdp: String, what: &'static str },
    StartCapture,
    AnswerOffer { sdp: String },
    CloseMids(Vec<String>),
    SetMicEnabled(bool),
    Stats,
    Close,
    Release,
}

#[derive(Debug, Default)]
struct Shared {
    ops: Vec<FakeOp>,
    states: Option<UnboundedSender<MediaState>>,
    fail_open: bool,
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

    fn set_mic_enabled(&self, enabled: bool) {
        record(&self.shared, FakeOp::SetMicEnabled(enabled));
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
