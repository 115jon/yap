//! UI sounds: each is rendered once (`yap_core::synth`, the web's recipes)
//! and played as a voice of one input on the app's speaker
//! (`yap-speaker`), so sounds overlap like Web Audio voices, follow the
//! output device, and reach the call's echo canceller like everything else
//! that plays. The web engine's gates apply: the sounds switch,
//! the per-group rate limit, and the per-group voice cap (the oldest voice
//! is cut, as `SoundEngine` fades it), each sound's category switch, and
//! the volume from the settings. With "Use custom sounds" on, a sound the
//! user imported a file for plays that file instead (decoded once, at 48
//! kHz stereo, [`crate::custom_sounds`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};
use std::time::Instant;

use yap_core::shell::{Sound, SoundGate};
use yap_core::synth;

#[derive(Default)]
struct Mixer {
    gate: SoundGate,
    /// Rendered once per sound and volume.
    rendered: HashMap<(Sound, u8), Arc<Vec<i16>>>,
    /// Imported files, decoded once per sound id (the volume is applied
    /// when one plays, so this does not grow with the slider).
    custom: HashMap<String, Arc<Vec<f32>>>,
}

fn mixer() -> &'static Mutex<Mixer> {
    static MIXER: OnceLock<Mutex<Mixer>> = OnceLock::new();
    MIXER.get_or_init(Mutex::default)
}

fn lock() -> std::sync::MutexGuard<'static, Mixer> {
    mixer().lock().unwrap_or_else(PoisonError::into_inner)
}

/// How many cycles a ring plays before it stops by itself: past the
/// server's 30 s ring timeout (`CALL_RING_TIMEOUT_MS`).
const RING_CYCLES: usize = 13;

fn pcm(sound: Sound, volume: u8) -> Vec<i16> {
    let gain = synth::volume_to_gain(volume);
    if sound.loop_period().is_some() {
        return synth::pcm16(&synth::render_loop(sound, gain, RING_CYCLES));
    }
    synth::pcm16(&synth::render(sound, gain))
}

/// Stops a sound's group at once (`playRingStop`): every voice of it is
/// reset. Never gated.
pub fn stop(sound: Sound) {
    output::stop_group(sound.spec().group);
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
    lock().custom.remove(id);
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
            let cached = lock().custom.get(&id).cloned();
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
                            lock().custom.insert(id.clone(), Arc::clone(&samples));
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
            output::play(group, max_voices, Arc::new(custom_pcm(&samples, volume)), 2);
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
        if !lock().gate.allow(sound, Instant::now()) {
            return;
        }
        play_custom(id.to_owned(), spec.group, spec.max_voices, volume);
        return;
    }
    let samples = {
        let mut mixer = lock();
        if !mixer.gate.allow(sound, Instant::now()) {
            return;
        }
        eprintln!("[sound] {sound:?}");
        Arc::clone(
            mixer
                .rendered
                .entry((sound, volume))
                .or_insert_with(|| Arc::new(pcm(sound, volume))),
        )
    };
    // With YAP_NULL_SPEAKER=1 the speaker plays to no device (its tap and
    // the fake room still hear the sound).
    let spec = sound.spec();
    output::play(spec.group, spec.max_voices, samples, 1);
}

/// The UI sounds' voices, as one input of the app's speaker.
mod output {
    use std::sync::{Arc, OnceLock};

    use yap_speaker::{Input, Voices};

    fn voices() -> &'static Arc<Voices> {
        static VOICES: OnceLock<Arc<Voices>> = OnceLock::new();
        VOICES.get_or_init(|| {
            let voices = Arc::new(Voices::default());
            let input: Arc<dyn Input> = voices.clone();
            yap_speaker::speaker().add(&input);
            voices
        })
    }

    /// Plays `samples` (48 kHz, `channels` interleaved) on the speaker.
    pub(super) fn play(
        group: &'static str,
        max_voices: usize,
        samples: Arc<Vec<i16>>,
        channels: usize,
    ) {
        voices().start(group, max_voices, samples, channels);
        yap_speaker::speaker().wake();
    }

    /// Cuts every playing voice of `group`.
    pub(super) fn stop_group(group: &'static str) {
        voices().stop(group);
    }
}
