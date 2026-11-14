//! The phone's local playback, on the desktop's player (`yap_audio`) and
//! the app's speaker (`yap_speaker`, AAudio on Android):
//!
//! - [`spawn`]: `Realtime::spawn_audio`, the soundboard clips the room
//!   plays and its Listen Together stream (the call page's
//!   ListenTogetherSheet stops it here and sets its volume).
//! - [`listen_volume`]: the Listen Together volume this phone saved, which
//!   the stream starts at when a call joins a room that is already playing.
//! - [`play`] / [`stop`]: the core's UI sounds, gated by the sound settings
//!   as on the desktop. The phone has no switches for them yet, so a fresh
//!   install plays the defaults.
//!
//! In a call the speaker plays as call audio (voice.rs tells it), so clips
//! and sounds follow the call's route and volume; outside one, UI sounds
//! play as system sounds.

use yap_cache::settings::{self, DesktopSettings};
use yap_core::shell::Sound;
use yap_core::store::apps::AppsCommand;
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

/// The Listen Together volume a phone starts at before its first change:
/// half, so a room already playing does not come in at full volume.
const DEFAULT_LISTEN_VOLUME: u8 = 50;
/// The file in the app's folder that keeps it (one number, 0..=100).
const LISTEN_VOLUME_FILE: &str = "listen-volume";

/// The saved Listen Together volume, 0..=100.
pub fn listen_volume() -> u8 {
    settings::app_dir()
        .and_then(|dir| std::fs::read_to_string(dir.join(LISTEN_VOLUME_FILE)).ok())
        .and_then(|text| text.trim().parse::<u8>().ok())
        .map_or(DEFAULT_LISTEN_VOLUME, |volume| volume.min(100))
}

/// Saves the Listen Together volume (the core already has it).
pub fn set_listen_volume(volume: u8) {
    let volume = volume.min(100);
    if listen_volume() == volume {
        return;
    }
    if let Some(dir) = settings::app_dir() {
        if let Err(error) = std::fs::write(dir.join(LISTEN_VOLUME_FILE), volume.to_string()) {
            eprintln!("[sound] listen volume not saved: {error}");
        }
    }
}

/// The soundboard volume setting, 0..=100 (the sheet's slider).
pub fn soundboard_volume() -> u8 {
    saved().sound_settings.soundboard_volume.min(100)
}

/// Saves the soundboard volume setting and applies it to the clips playing
/// now and later, as the desktop's `Desktop.setVolume` does.
pub fn set_soundboard_volume(volume: u8) {
    let volume = volume.min(100);
    if let Some(dir) = settings::app_dir() {
        let mut next = settings::load_settings(&dir);
        if next.sound_settings.soundboard_volume != volume {
            next.sound_settings.soundboard_volume = volume;
            if let Err(error) = settings::save_settings(&dir, &next) {
                eprintln!("[sound] soundboard volume not saved: {error}");
            }
        }
    }
    yap_audio::player::set_soundboard_volume(volume);
    send_local_settings();
}

/// The core's copy of the sound settings (join and leave sounds play at
/// the sound volume, the soundboard at its own).
pub fn send_local_settings() {
    let s = saved();
    yap_ui::apps::send(AppsCommand::LocalSettings {
        sounds_enabled: s.sounds,
        sound_volume: s.sound_settings.sound_volume,
        soundboard_volume: s.sound_settings.soundboard_volume,
    });
    yap_ui::apps::send(AppsCommand::LtVolume(f64::from(listen_volume()) / 100.0));
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
