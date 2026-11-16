//! UI sounds, played by [`yap_audio::ui`] (rendered once, a voice of one
//! input on the app's speaker, the per-group rate limit and voice cap).
//! Here are the desktop's gates: the sounds switch, each sound's category
//! switch, and the volume from the settings. With "Use custom sounds" on, a sound the
//! user imported a file for plays that file instead (decoded once, at 48
//! kHz stereo, [`yap_ui::custom_sounds`]).

use yap_audio::ui;
use yap_core::shell::Sound;
use yap_ui::custom_sounds::start;

/// Stops a sound's group at once (`playRingStop`): every voice of it is
/// reset. Never gated.
pub fn stop(sound: Sound) {
    ui::stop(sound);
}

/// Plays `sound` unless the sounds switch or its category is off, or its
/// group is inside its rate limit. Never blocks the caller.
pub fn play(sound: Sound) {
    let settings = crate::desktop::current();
    if !settings.sounds || !sound.enabled_in(&settings.sound_settings) {
        return;
    }
    let custom = settings.sound_settings.custom_sounds;
    start(sound, settings.sound_settings.sound_volume, custom);
}

/// Push to talk's on (`Unmute`) and off (`Mute`) tones. Its own switch
/// (Keybinds) stands for the mute category's; the sounds switch and the
/// volume still apply.
pub fn play_ptt(on: bool) {
    let settings = crate::desktop::current();
    if !settings.sounds {
        return;
    }
    let sound = if on { Sound::Unmute } else { Sound::Mute };
    start(
        sound,
        settings.sound_settings.sound_volume,
        settings.sound_settings.custom_sounds,
    );
}

/// The Test key: plays whatever the switches say (`bypassToggles`).
pub fn play_test(sound: Sound) {
    let settings = crate::desktop::current();
    start(
        sound,
        settings.sound_settings.sound_volume,
        settings.sound_settings.custom_sounds,
    );
}
