//! The production engine: libwebrtc's platform audio device module (real
//! mic and speakers, with WebRTC's APM: echo cancellation, noise
//! suppression, gain control) and one peer connection per call.

use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use libwebrtc::media_stream_track::MediaStreamTrack;
use libwebrtc::peer_connection::{
    AnswerOptions, OfferOptions, PeerConnection, PeerConnectionState,
};
use libwebrtc::peer_connection_factory::native::PeerConnectionFactoryExt;
use libwebrtc::peer_connection_factory::{
    IceServer as RtcIceServer, PeerConnectionFactory, RtcConfiguration,
};
use libwebrtc::prelude::RtcAudioTrack;
use libwebrtc::rtp_transceiver::{RtpTransceiverDirection, RtpTransceiverInit};
use libwebrtc::session_description::{SdpType, SessionDescription};
use libwebrtc::MediaType;
use tokio::sync::mpsc::{self, UnboundedReceiver};

use super::{LocalOffer, MediaEngine, MediaError, MediaSession};
use crate::machine::MediaState;
use crate::wire::IceServer;

/// One process-wide factory: it owns the platform ADM.
fn factory() -> &'static PeerConnectionFactory {
    static FACTORY: OnceLock<PeerConnectionFactory> = OnceLock::new();
    FACTORY.get_or_init(PeerConnectionFactory::default)
}

fn err(what: &str) -> impl FnOnce(libwebrtc::RtcError) -> MediaError + '_ {
    move |e| MediaError(format!("{what}: {e:?}"))
}

/// The libwebrtc engine. Stateless: the factory and its ADM are global.
#[derive(Debug, Clone, Copy, Default)]
pub struct LibwebrtcEngine;

/// One peer connection, the mic track and the remote tracks it keeps alive
/// (the ADM mixes and plays them).
#[derive(Clone)]
pub struct LibwebrtcSession {
    pc: PeerConnection,
    mic: RtcAudioTrack,
    remote_tracks: Arc<Mutex<Vec<MediaStreamTrack>>>,
}

impl MediaEngine for LibwebrtcEngine {
    type Session = LibwebrtcSession;

    fn open(
        &self,
        ice_servers: &[IceServer],
    ) -> Result<(LibwebrtcSession, UnboundedReceiver<MediaState>), MediaError> {
        let f = factory();
        if !f.acquire_platform_adm() {
            return Err(MediaError("no audio device".into()));
        }
        // Both directions through the real devices. Recording is off by
        // default (synthetic mode), and while it is off WebRTC's own
        // Init/StartRecording calls do nothing. It must be on before the
        // device track is created.
        f.set_adm_recording_enabled(true);
        f.set_adm_playout_enabled(true);
        let mic: RtcAudioTrack = f.create_device_audio_track("mic");
        let pc = f
            .create_peer_connection({
                let mut config = RtcConfiguration::default();
                config.ice_servers = ice_servers
                    .iter()
                    .map(|s| RtcIceServer {
                        urls: s.urls.clone(),
                        username: s.username.clone(),
                        password: s.password.clone(),
                    })
                    .collect();
                config
            })
            .map_err(err("peer connection"))?;
        let (state_tx, states) = mpsc::unbounded_channel();
        pc.on_connection_state_change(Some(Box::new(move |state| {
            let _ = state_tx.send(match state {
                PeerConnectionState::New => MediaState::New,
                PeerConnectionState::Connecting => MediaState::Connecting,
                PeerConnectionState::Connected => MediaState::Connected,
                PeerConnectionState::Disconnected => MediaState::Disconnected,
                PeerConnectionState::Failed => MediaState::Failed,
                PeerConnectionState::Closed => MediaState::Closed,
            });
        })));
        let remote_tracks = Arc::new(Mutex::new(Vec::new()));
        let kept = Arc::clone(&remote_tracks);
        pc.on_track(Some(Box::new(move |event| {
            kept.lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(event.track);
        })));
        Ok((
            LibwebrtcSession {
                pc,
                mic,
                remote_tracks,
            },
            states,
        ))
    }

    fn release(&self) {
        factory().release_platform_adm();
    }
}

impl MediaSession for LibwebrtcSession {
    async fn publish_mic(&self, stream_id: &str) -> Result<LocalOffer, MediaError> {
        let transceiver = self
            .pc
            .add_transceiver(
                MediaStreamTrack::Audio(self.mic.clone()),
                RtpTransceiverInit {
                    direction: RtpTransceiverDirection::SendOnly,
                    stream_ids: vec![stream_id.to_owned()],
                    send_encodings: vec![],
                },
            )
            .map_err(err("add transceiver"))?;
        let opus: Vec<_> = factory()
            .get_rtp_sender_capabilities(MediaType::Audio)
            .codecs
            .into_iter()
            .filter(|c| c.mime_type.eq_ignore_ascii_case("audio/opus"))
            .collect();
        let _ = transceiver.set_codec_preferences(opus);
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local offer"))?;
        Ok(LocalOffer {
            sdp: offer.to_string(),
            mid: transceiver.mid().unwrap_or_else(|| "0".into()),
        })
    }

    async fn apply_answer(&self, sdp: &str, what: &'static str) -> Result<(), MediaError> {
        let answer = SessionDescription::parse(sdp, SdpType::Answer)
            .map_err(|e| MediaError(format!("{what} sdp: {e:?}")))?;
        self.pc
            .set_remote_description(answer)
            .await
            .map_err(|e| MediaError(format!("remote {what}: {e:?}")))
    }

    fn start_capture(&self) {
        let f = factory();
        if !f.recording_is_initialized() {
            let ok = f.init_recording() && f.start_recording();
            eprintln!("[voice] started recording explicitly: {ok}");
        }
    }

    async fn answer_offer(&self, sdp: &str) -> Result<String, MediaError> {
        let offer = SessionDescription::parse(sdp, SdpType::Offer)
            .map_err(|e| MediaError(format!("offer sdp: {e:?}")))?;
        self.pc
            .set_remote_description(offer)
            .await
            .map_err(err("remote offer"))?;
        let answer = self
            .pc
            .create_answer(AnswerOptions::default())
            .await
            .map_err(err("answer"))?;
        self.pc
            .set_local_description(answer.clone())
            .await
            .map_err(err("local answer"))?;
        Ok(answer.to_string())
    }

    async fn close_mids(&self, mids: &[String]) -> Result<String, MediaError> {
        for transceiver in self.pc.transceivers() {
            if transceiver.mid().is_some_and(|mid| mids.contains(&mid)) {
                let _ = transceiver.stop();
            }
        }
        let offer = self
            .pc
            .create_offer(OfferOptions::default())
            .await
            .map_err(err("close offer"))?;
        self.pc
            .set_local_description(offer.clone())
            .await
            .map_err(err("local close offer"))?;
        Ok(offer.to_string())
    }

    fn set_mic_enabled(&self, enabled: bool) {
        self.mic.set_enabled(enabled);
    }

    async fn stats(&self) -> Result<String, MediaError> {
        use libwebrtc::stats::RtcStats;
        let stats = self
            .pc
            .get_stats()
            .await
            .map_err(|e| MediaError(format!("{e:?}")))?;
        let mut line = String::from("[voice] stats");
        for stat in stats {
            match stat {
                RtcStats::OutboundRtp(out) => {
                    line += &format!(" out[{}]={}B", out.outbound.mid, out.sent.bytes_sent);
                }
                RtcStats::InboundRtp(inbound) => {
                    line += &format!(
                        " in[{}]={}B level={:.3}",
                        inbound.inbound.mid,
                        inbound.inbound.bytes_received,
                        inbound.inbound.audio_level
                    );
                }
                _ => {}
            }
        }
        Ok(line)
    }

    fn close(&self) {
        self.remote_tracks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        self.pc.close();
    }
}
