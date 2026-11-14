//! The phone's local playback, on the desktop's player (`yap_audio`) and
//! the app's speaker (`yap_speaker`, AAudio on Android):
//!
//! - [`spawn`]: `Realtime::spawn_audio`, the soundboard clips the room
//!   plays. Listen Together's stream is held back until the phone has
//!   controls for it.
//! - [`play`] / [`stop`]: the core's UI sounds, gated by the sound settings
//!   as on the desktop. The phone has no switches for them yet, so a fresh
//!   install plays the defaults.
//!
//! In a call the speaker plays as call audio (voice.rs tells it), so clips
//! and sounds follow the call's route and volume; outside one, UI sounds
//! play as system sounds.

use tokio::sync::mpsc;
use yap_cache::settings::{self, DesktopSettings};
use yap_core::shell::Sound;
use yap_core::store::apps::AudioCommand;
use yap_core::{AudioEvents, AudioHandle};

/// The sound settings saved in the app's folder (the defaults until the
/// phone has a page for them).
fn saved() -> DesktopSettings {
    settings::app_dir()
        .map(|dir| settings::load_settings(&dir))
        .unwrap_or_default()
}

/// Starts the player (once, from the core's runtime).
pub fn spawn(http: reqwest::Client, events: AudioEvents) -> AudioHandle {
    // Strangers' clips (MyInstants) and the stream: the app's rustls roots.
    yap_audio::set_client_setup(|builder| {
        builder.use_preconfigured_tls(crate::platform::tls_config())
    });
    let volume = || saved().sound_settings.soundboard_volume;
    let player = yap_audio::player::spawn(http, events, Box::new(volume));
    let Some(player) = player.commands else {
        return AudioHandle::none();
    };
    // Clips only: the phone has no Listen Together controls (volume, stop)
    // yet, so its stream would play with no way to turn it down.
    let (tx, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        while let Some((token, command)) = rx.recv().await {
            if is_clip(&command) && player.send((token, command)).is_err() {
                return;
            }
        }
    });
    AudioHandle { commands: Some(tx) }
}

/// A soundboard command, not one of the Listen Together stream's.
fn is_clip(command: &AudioCommand) -> bool {
    !matches!(
        command,
        AudioCommand::StreamLoad { .. }
            | AudioCommand::StreamSeek(_)
            | AudioCommand::StreamPause(_)
            | AudioCommand::StreamStop
            | AudioCommand::StreamAudio { .. }
    )
}

/// A UI sound, unless the settings switch it off. The incoming ring is not
/// played here: the shim rings with the phone's ringtone
/// (push/Ringer.java).
pub fn play(sound: Sound) {
    if sound == Sound::Ring {
        return;
    }
    let settings = saved();
    if !settings.sounds || !sound.enabled_in(&settings.sound_settings) {
        return;
    }
    yap_audio::ui::play(sound, settings.sound_settings.sound_volume);
}

/// Cuts a sound's voices (`playRingStop`).
pub fn stop(sound: Sound) {
    yap_audio::ui::stop(sound);
}
