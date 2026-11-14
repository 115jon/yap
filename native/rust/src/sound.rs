//! UI sounds, played by [`yap_audio::ui`] (rendered once, a voice of one
//! input on the app's speaker, the per-group rate limit and voice cap).
//! Here are the desktop's gates: the sounds switch, each sound's category
//! switch, and the volume from the settings. With "Use custom sounds" on, a sound the
//! user imported a file for plays that file instead (decoded once, at 48
//! kHz stereo, [`crate::custom_sounds`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use yap_audio::ui;
use yap_core::shell::Sound;
use yap_core::synth;

/// Imported files, decoded once per sound id (the volume is applied when
/// one plays, so this does not grow with the slider).
fn custom() -> std::sync::MutexGuard<'static, HashMap<String, Arc<Vec<f32>>>> {
    static CUSTOM: OnceLock<Mutex<HashMap<String, Arc<Vec<f32>>>>> = OnceLock::new();
    CUSTOM
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
}

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

/// The native sound a catalog id plays as, when this build has one.
fn native_sound(id: &str) -> Option<Sound> {
    Sound::ALL
        .into_iter()
        .find(|s| yap_core::custom_sounds::id_of(*s) == id)
}

/// The custom sounds card's preview: the imported file when there is one,
/// else the built-in sound.
pub fn preview(id: &str) {
    let volume = crate::desktop::current().sound_settings.sound_volume;
    if volume == 0 {
        return;
    }
    if crate::custom_sounds::file_for(id).is_some() {
        let group = native_sound(id).map_or("custom", |s| s.spec().group);
        play_custom(id.to_owned(), group, 1, volume);
    } else if let Some(sound) = native_sound(id) {
        start(sound, volume, false);
    }
}

/// A new file (or none) for a sound: its decoded copies go.
pub fn forget_custom(id: &str) {
    custom().remove(id);
}

/// 48 kHz stereo `f32` to 16-bit at the sound volume.
fn custom_pcm(samples: &[f32], volume: u8) -> Vec<i16> {
    // A gain in 0..=1 loses nothing as f32.
    #[allow(clippy::cast_possible_truncation)]
    let gain = synth::volume_to_gain(volume) as f32;
    samples
        .iter()
        // Clamped to ±1 first, so the cast cannot overflow.
        .map(|s| ((s * gain).clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16)
        .collect()
}

/// Plays an imported file on a short-lived thread (decoded there the
/// first time).
fn play_custom(id: String, group: &'static str, max_voices: usize, volume: u8) {
    let spawned = std::thread::Builder::new()
        .name("yap-sound".into())
        .spawn(move || {
            let cached = custom().get(&id).cloned();
            let samples = match cached {
                Some(samples) => samples,
                None => {
                    let Some(path) = crate::custom_sounds::file_for(&id) else {
                        return;
                    };
                    let hint = path.extension().and_then(|e| e.to_str()).map(str::to_owned);
                    let decoded = std::fs::read(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|bytes| yap_audio::decode_sound(&bytes, hint.as_deref()));
                    match decoded {
                        Ok((samples, _)) => {
                            let samples = Arc::new(samples);
                            custom().insert(id.clone(), Arc::clone(&samples));
                            samples
                        }
                        Err(error) => {
                            eprintln!("[sound] custom {id}: {error}");
                            return;
                        }
                    }
                }
            };
            eprintln!("[sound] custom {id}");
            ui::play_pcm(group, max_voices, Arc::new(custom_pcm(&samples, volume)), 2);
        });
    if let Err(error) = spawned {
        eprintln!("[sound] no thread: {error}");
    }
}

fn start(sound: Sound, volume: u8, custom: bool) {
    if volume == 0 {
        return;
    }
    let id = yap_core::custom_sounds::id_of(sound);
    if custom && crate::custom_sounds::file_for(id).is_some() {
        let spec = sound.spec();
        if !ui::allow(sound) {
            return;
        }
        play_custom(id.to_owned(), spec.group, spec.max_voices, volume);
        return;
    }
    ui::play(sound, volume);
}
