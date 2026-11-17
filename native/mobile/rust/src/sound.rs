//! The phone's local playback, on the desktop's player (`yap_audio`) and
//! the app's speaker (`yap_speaker`, AAudio on Android):
//!
//! - [`spawn`]: `Realtime::spawn_audio`, the soundboard clips the room
//!   plays and its Listen Together stream (the call page's
//!   ListenTogetherSheet stops it here and sets its volume).
//! - [`listen_volume`]: the Listen Together volume this phone saved, which
//!   the stream starts at when a call joins a room that is already playing.
//! - [`play`] / [`stop`]: the core's UI sounds, gated by the sound settings
//!   as on the desktop, with "Use custom sounds" playing a file imported
//!   for a sound instead (`yap_ui::custom_sounds`). Settings >
//!   Notifications sets them ([`settings_json`], [`set_setting`],
//!   [`play_test`]).
//!
//! In a call the speaker plays as call audio (voice.rs tells it), so clips
//! and sounds follow the call's route and volume; outside one, UI sounds
//! play as system sounds.

use yap_cache::settings::{self, DesktopSettings};
use yap_core::shell::Sound;
use yap_core::store::apps::AppsCommand;
use yap_core::{AudioEvents, AudioHandle};
use yap_ui::custom_sounds::start;

/// The sound settings saved in the app's folder (the defaults until
/// Settings changes one).
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

/// Settings > Notifications: the UI sound settings as JSON, under the
/// desktop's `Desktop` names.
pub fn settings_json() -> String {
    let s = saved();
    let p = s.sound_settings;
    serde_json::json!({
        "sounds": s.sounds,
        "soundVolume": p.sound_volume.min(100),
        "soundNotifications": p.notifications,
        "soundVoiceJoinLeave": p.voice_join_leave,
        "soundConnect": p.self_connect_disconnect,
        "soundMute": p.mute_deafen,
        "soundScreenShare": p.screen_share,
        "soundCalls": p.calls,
        "customSounds": p.custom_sounds,
    })
    .to_string()
}

/// Saves one UI sound setting (a switch as 0 or 1, `soundVolume` 0 to
/// 100), as the desktop's `Desktop.setSetting` and `setVolume` do, and
/// tells the core (join and leave sounds follow the switch and volume).
pub fn set_setting(key: &str, value: i32) {
    let Some(dir) = settings::app_dir() else {
        return;
    };
    let mut next = settings::load_settings(&dir);
    let on = value != 0;
    let sound = &mut next.sound_settings;
    match key {
        "sounds" => next.sounds = on,
        "soundVolume" => sound.sound_volume = u8::try_from(value.clamp(0, 100)).unwrap_or(100),
        "soundNotifications" => sound.notifications = on,
        "soundVoiceJoinLeave" => sound.voice_join_leave = on,
        "soundConnect" => sound.self_connect_disconnect = on,
        "soundMute" => sound.mute_deafen = on,
        "soundScreenShare" => sound.screen_share = on,
        "soundCalls" => sound.calls = on,
        "customSounds" => sound.custom_sounds = on,
        _ => {
            eprintln!("[sound] unknown setting {key}");
            return;
        }
    }
    if let Err(error) = settings::save_settings(&dir, &next) {
        eprintln!("[sound] {key} not saved: {error}");
    }
    send_local_settings();
}

/// Test: the notification sound at the sound volume, whatever the
/// switches say (the desktop's `play_test`).
pub fn play_test() {
    let sound = saved().sound_settings;
    start(Sound::Notification, sound.sound_volume, sound.custom_sounds);
}

/// The UI sound volume, 0..=100 (a custom sound's preview plays at it).
pub fn sound_volume() -> u8 {
    saved().sound_settings.sound_volume.min(100)
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
    let sound_settings = &settings.sound_settings;
    start(sound, sound_settings.sound_volume, sound_settings.custom_sounds);
}

/// Cuts a sound's voices (`playRingStop`).
pub fn stop(sound: Sound) {
    yap_audio::ui::stop(sound);
}
