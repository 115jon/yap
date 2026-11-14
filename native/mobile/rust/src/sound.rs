//! The phone's local playback, on the desktop's player (`yap_audio`) and
//! the app's speaker (`yap_speaker`, AAudio on Android):
//!
//! - [`spawn`]: `Realtime::spawn_audio`, the soundboard clips the room
//!   plays (and Listen Together, once the phone has it).
//! - [`play`] / [`stop`]: the core's UI sounds, gated by the sound settings
//!   as on the desktop. The phone has no switches for them yet, so a fresh
//!   install plays the defaults.
//!
//! In a call the speaker plays as call audio (voice.rs tells it), so clips
//! and sounds follow the call's route and volume; outside one, UI sounds
//! play as system sounds.

use yap_cache::settings::{self, DesktopSettings};
use yap_core::shell::Sound;
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
    yap_audio::player::spawn(http, events, Box::new(volume))
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
