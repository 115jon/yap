//! Plugs `yap_voice` calls into the store, with the desktop's side of a
//! call: the call itself (state machine, sockets, SFU, libwebrtc media)
//! and its store driver live in `yap_voice` ([`yap_voice::call`], which the
//! mobile app runs too). Here: screen capture, the video on screen, the mic
//! meter and a share's stills. The voice settings (volumes, sensitivity,
//! processing) reach the call from [`crate::audio`] through
//! [`set_call_audio`]; the mic level goes back the same way.

use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::watch;
use yap_core::{Api, VoiceCommand, VoiceEvents};
use yap_voice::call::Host;
use yap_voice::VideoIntent;

pub use yap_voice::call::{
    now_ms, send_disconnect_beacon, send_stale_beacon, set_call_audio, set_transmit, CallAudio,
    Target,
};

struct Desktop;

impl Host for Desktop {
    type Capturer = yap_capture::Capturer;

    fn capturer(&self) -> Self::Capturer {
        yap_capture::Capturer
    }

    fn prepare(&self) {
        crate::video::install_output();
    }

    fn video(&self) -> watch::Receiver<VideoIntent> {
        crate::video::subscribe()
    }

    fn mic_level(&self, level: Option<f64>) {
        crate::audio::set_live_level(level);
    }

    fn still_jpeg(&self, still: &yap_voice::Still, quality: u8) -> Option<Vec<u8>> {
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, quality)
            .encode(
                &still.rgb,
                still.width,
                still.height,
                image::ExtendedColorType::Rgb8,
            )
            .ok()?;
        Some(jpeg)
    }
}

/// Runs one call until it is left or ends; reports through `events`.
pub async fn run(
    api: Api,
    http: reqwest::Client,
    target: Target,
    commands: UnboundedReceiver<VoiceCommand>,
    events: VoiceEvents,
) {
    yap_voice::call::run(Arc::new(Desktop), api, http, target, commands, events).await;
}
